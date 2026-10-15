// SPDX-License-Identifier: MPL-2.0
//
// Received files stay in app-private filesDir/received (0700, files 0600)
// until the user explicitly saves, shares or opens them (ADR-0013 D4).
// Save, on a tap only:
//
// 1. The name must be a single component inside the received folder. The
//    file is opened with O_NOFOLLOW, and fstat must report a regular file
//    of exactly the size the core reported when it saved it.
// 2. API 29+: MediaStore Downloads, RELATIVE_PATH Download/Dropumi/, with
//    IS_PENDING set while it is written. API 26-28: the Storage Access
//    Framework, into a folder the user picked; a new document is created
//    for every file (the provider picks a new name on a collision), so an
//    existing document is never opened for writing.
// 3. The copy counts bytes and succeeds only if the count equals both the
//    reported size and the fstat size; then IS_PENDING is cleared. On any
//    mismatch or error the pending row or the created document is deleted.
// 4. Save never deletes the internal copy; deleting it is a separate action.
//
// There is no hash check here: the core already verified each file with
// BLAKE3, and hand-written crypto is not allowed. Save checks identity and
// size only.

package io.github.dropumi.core.files

import android.content.ContentResolver
import android.content.ContentValues
import android.net.Uri
import android.os.Build
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import java.io.File
import java.io.FileDescriptor
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.util.Locale

/** A received file as the core reported it: its name in the received folder and its size. */
data class ReceivedFile(val savedAs: String, val size: Long)

/** The pure rules of exporting. */
object ExportRules {
    const val OCTET_STREAM = "application/octet-stream"

    /** A single name inside the received folder: not empty, no `/`, no NUL, not `.` or `..`. */
    fun isSingleName(savedAs: String): Boolean =
        savedAs.isNotEmpty() && '/' !in savedAs && '\u0000' !in savedAs && savedAs != "." && savedAs != ".."

    /**
     * The MIME type from the extension of the (already sanitized) saved name
     * only, through [lookup] (production: MimeTypeMap), else
     * `application/octet-stream`. A leading dot alone is not an extension.
     */
    fun mimeFor(savedAs: String, lookup: (String) -> String?): String {
        val dot = savedAs.lastIndexOf('.')
        if (dot <= 0 || dot == savedAs.length - 1) return OCTET_STREAM
        val extension = savedAs.substring(dot + 1).lowercase(Locale.ROOT)
        return lookup(extension)?.takeIf { it.isNotBlank() } ?: OCTET_STREAM
    }
}

/** What saving received files gives. */
sealed interface ExportResult {
    /** Every file was saved; [uris] are the new items in shared storage. */
    data class Saved(val uris: List<String>) : ExportResult

    /** Stopped at a file: [saved] files were saved before it (they are complete). */
    data class Failed(val reason: Reason, val saved: Int) : ExportResult

    enum class Reason { NOT_A_RECEIVED_FILE, CHANGED, NO_SPACE, WRITE_FAILED }
}

/** What AppModel needs to save received files. */
interface ReceivedExporter {
    /** API 29+: Save goes straight to Downloads. Otherwise the user picks a folder first. */
    val savesToDownloads: Boolean

    fun saveToDownloads(files: List<ReceivedFile>): ExportResult

    /** Saves into the folder [treeUri] that the user picked (ACTION_OPEN_DOCUMENT_TREE). */
    fun saveToFolder(treeUri: String, files: List<ReceivedFile>): ExportResult
}

class ReceivedExport(
    private val resolver: ContentResolver,
    private val receiveDir: File,
    /** Free bytes in shared storage (production: StatFs). */
    private val sharedFreeSpace: () -> Long,
    private val mimeLookup: (String) -> String?,
) : ReceivedExporter {
    override val savesToDownloads: Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q

    private class Failure(val reason: ExportResult.Reason) : Exception()

    override fun saveToDownloads(files: List<ReceivedFile>): ExportResult {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return ExportResult.Failed(ExportResult.Reason.WRITE_FAILED, 0)
        if (files.sumOf { it.size } > sharedFreeSpace()) return ExportResult.Failed(ExportResult.Reason.NO_SPACE, 0)
        return each(files) { file, source -> insertDownload(file, source) }
    }

    override fun saveToFolder(treeUri: String, files: List<ReceivedFile>): ExportResult {
        val tree = Uri.parse(treeUri)
        val parent = try {
            DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        } catch (_: RuntimeException) {
            return ExportResult.Failed(ExportResult.Reason.WRITE_FAILED, 0)
        }
        return each(files) { file, source -> createDocument(parent, file, source) }
    }

    /** Saves the files one after another; stops at the first failure. */
    private fun each(files: List<ReceivedFile>, save: (ReceivedFile, Source) -> Uri): ExportResult {
        val saved = mutableListOf<String>()
        for (file in files) {
            try {
                openSource(file).use { source -> saved += save(file, source).toString() }
            } catch (e: Failure) {
                return ExportResult.Failed(e.reason, saved.size)
            } catch (_: Exception) {
                return ExportResult.Failed(ExportResult.Reason.WRITE_FAILED, saved.size)
            }
        }
        return ExportResult.Saved(saved)
    }

    /** The received file, opened without following links and checked with fstat. */
    private class Source(val descriptor: FileDescriptor, val size: Long) : AutoCloseable {
        override fun close() {
            try {
                Os.close(descriptor)
            } catch (_: ErrnoException) {
                // Nothing more to do.
            }
        }
    }

    private fun openSource(file: ReceivedFile): Source {
        if (!ExportRules.isSingleName(file.savedAs)) throw Failure(ExportResult.Reason.NOT_A_RECEIVED_FILE)
        val path = File(receiveDir, file.savedAs)
        if (path.parentFile != receiveDir) throw Failure(ExportResult.Reason.NOT_A_RECEIVED_FILE)
        val descriptor = try {
            // OsConstants.O_CLOEXEC exists from API 27; nothing here execs.
            val cloexec = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) OsConstants.O_CLOEXEC else 0
            Os.open(path.path, OsConstants.O_RDONLY or OsConstants.O_NOFOLLOW or cloexec, 0)
        } catch (_: ErrnoException) {
            throw Failure(ExportResult.Reason.NOT_A_RECEIVED_FILE)
        }
        try {
            val stat = Os.fstat(descriptor)
            if (!OsConstants.S_ISREG(stat.st_mode)) throw Failure(ExportResult.Reason.NOT_A_RECEIVED_FILE)
            if (stat.st_size != file.size) throw Failure(ExportResult.Reason.CHANGED)
            return Source(descriptor, stat.st_size)
        } catch (e: Exception) {
            Source(descriptor, 0).close()
            throw if (e is Failure) e else Failure(ExportResult.Reason.NOT_A_RECEIVED_FILE)
        }
    }

    private fun insertDownload(file: ReceivedFile, source: Source): Uri {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) throw Failure(ExportResult.Reason.WRITE_FAILED)
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, file.savedAs)
            put(MediaStore.MediaColumns.MIME_TYPE, ExportRules.mimeFor(file.savedAs, mimeLookup))
            put(MediaStore.MediaColumns.RELATIVE_PATH, DOWNLOADS_FOLDER)
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val uri = resolver.insert(MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY), values)
            ?: throw Failure(ExportResult.Reason.WRITE_FAILED)
        try {
            copy(source, uri, file.size)
            val done = ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }
            if (resolver.update(uri, done, null, null) != 1) throw Failure(ExportResult.Reason.WRITE_FAILED)
            return uri
        } catch (e: Exception) {
            deleteQuietly { resolver.delete(uri, null, null) }
            throw e
        }
    }

    private fun createDocument(parent: Uri, file: ReceivedFile, source: Source): Uri {
        val mime = ExportRules.mimeFor(file.savedAs, mimeLookup)
        val created = DocumentsContract.createDocument(resolver, parent, mime, file.savedAs)
            ?: throw Failure(ExportResult.Reason.WRITE_FAILED)
        try {
            copy(source, created, file.size)
            return created
        } catch (e: Exception) {
            deleteQuietly { DocumentsContract.deleteDocument(resolver, created) }
            throw e
        }
    }

    /** Copies [source] into [target], counting bytes; the count must equal both sizes. */
    private fun copy(source: Source, target: Uri, reported: Long) {
        val out = resolver.openFileDescriptor(target, "w") ?: throw Failure(ExportResult.Reason.WRITE_FAILED)
        var copied = 0L
        ParcelFileDescriptor.AutoCloseOutputStream(out).use { stream ->
            val input = FileInputStream(source.descriptor)
            val buffer = ByteArray(BUFFER)
            while (true) {
                val n = input.read(buffer)
                if (n < 0) break
                copied += n
                if (copied > reported) throw Failure(ExportResult.Reason.CHANGED)
                stream.write(buffer, 0, n)
            }
            stream.flush()
            syncQuietly(stream)
        }
        if (copied != reported || copied != source.size) throw Failure(ExportResult.Reason.CHANGED)
    }

    private fun syncQuietly(stream: FileOutputStream) {
        try {
            stream.fd.sync()
        } catch (_: IOException) {
            // Some providers' descriptors are pipes or sockets: nothing to sync.
        }
    }

    private fun deleteQuietly(delete: () -> Unit) {
        try {
            delete()
        } catch (_: Exception) {
            // Best effort: nothing more can be done.
        }
    }

    companion object {
        const val DOWNLOADS_FOLDER = "Download/Dropumi/"
        private const val BUFFER = 64 * 1024
    }
}
