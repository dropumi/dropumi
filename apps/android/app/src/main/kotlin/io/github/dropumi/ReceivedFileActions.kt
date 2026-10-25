// SPDX-License-Identifier: MPL-2.0
//
// Open, Share and the Save folder picker for received files (ADR-0013 D4).
// Files stay in app-private storage; each explicit action grants another
// app read access to the chosen files only, through the FileProvider, whose
// only path is the received folder.

package io.github.dropumi

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import io.github.dropumi.core.files.ExportRules
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.attribute.BasicFileAttributes

class ReceivedFileActions(private val context: Context, private val receiveDir: File) {
    private val authority = context.packageName + AUTHORITY_SUFFIX

    /** A chooser that opens one received file, or null if it is not one. */
    fun open(savedAs: String): Intent? {
        val uri = uriFor(savedAs) ?: return null
        val view = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, mimeFor(savedAs))
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        return Intent.createChooser(view, null)
    }

    /** A chooser that shares received files, or null if none of them is one. */
    fun share(names: List<String>): Intent? {
        val pairs = names.mapNotNull { name -> uriFor(name)?.let { name to it } }
        if (pairs.isEmpty()) return null
        val uris = pairs.map { it.second }
        val send = if (uris.size == 1) {
            Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_STREAM, uris.first()).setType(mimeFor(pairs.first().first))
        } else {
            Intent(Intent.ACTION_SEND_MULTIPLE).putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList(uris)).setType("*/*")
        }
        send.clipData = ClipData.newRawUri(null, uris.first()).apply { uris.drop(1).forEach { addItem(ClipData.Item(it)) } }
        send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        return Intent.createChooser(send, null)
    }

    /** API 26-28: the folder picker Save uses, starting in Downloads where the system allows it. */
    fun pickSaveFolder(): Intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
        .putExtra(DocumentsContract.EXTRA_INITIAL_URI, DocumentsContract.buildDocumentUri(EXTERNAL_STORAGE, "primary:Download"))

    /**
     * Bytes the received folder's regular files use (Settings → Received
     * files). Links are not followed. Call it off the main thread.
     */
    fun usedBytes(): Long {
        val dir = receiveDir.toPath()
        if (!Files.isDirectory(dir, LinkOption.NOFOLLOW_LINKS)) return 0
        return try {
            Files.newDirectoryStream(dir).use { entries ->
                entries.sumOf { path ->
                    val attributes = Files.readAttributes(path, BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
                    if (attributes.isRegularFile) attributes.size() else 0L
                }
            }
        } catch (_: IOException) {
            0
        }
    }

    /** A content URI for a single received file; null for anything else. */
    fun uriFor(savedAs: String): Uri? {
        if (!ExportRules.isSingleName(savedAs)) return null
        val file = File(receiveDir, savedAs)
        if (!file.isFile) return null
        return try {
            FileProvider.getUriForFile(context, authority, file)
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    private fun mimeFor(savedAs: String): String =
        ExportRules.mimeFor(savedAs) { MimeTypeMap.getSingleton().getMimeTypeFromExtension(it) }

    companion object {
        const val AUTHORITY_SUFFIX = ".fileprovider"
        private const val EXTERNAL_STORAGE = "com.android.externalstorage.documents"
    }
}
