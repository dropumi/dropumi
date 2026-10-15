// SPDX-License-Identifier: MPL-2.0
//
// DocumentSource over an injected ContentResolver (instrumented tests pass
// a MockContentResolver with in-process providers).

package io.github.dropumi.core.files

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.system.Os
import android.system.OsConstants
import java.io.FileInputStream
import java.io.IOException

class AndroidDocumentSource(private val resolver: ContentResolver) : DocumentSource {
    override fun query(uri: String): DocumentMeta {
        val cursor = resolver.query(Uri.parse(uri), null, null, null, null) ?: return DocumentMeta()
        return cursor.use { c ->
            if (!c.moveToFirst()) return DocumentMeta()
            DocumentMeta(
                displayName = c.string(OpenableColumns.DISPLAY_NAME),
                size = c.long(OpenableColumns.SIZE),
                lastModifiedMs = c.long(DocumentsContract.Document.COLUMN_LAST_MODIFIED),
            )
        }
    }

    override fun open(uri: String): OpenDocument {
        val descriptor = resolver.openFileDescriptor(Uri.parse(uri), "r") ?: throw IOException("no descriptor")
        return DescriptorDocument(descriptor)
    }

    private fun Cursor.string(column: String): String? =
        getColumnIndex(column).takeIf { it >= 0 && !isNull(it) }?.let(::getString)

    private fun Cursor.long(column: String): Long? =
        getColumnIndex(column).takeIf { it >= 0 && !isNull(it) }?.let(::getLong)
}

/** An open ParcelFileDescriptor: the descriptor checked is the one read. */
class DescriptorDocument(private val descriptor: ParcelFileDescriptor) : OpenDocument {
    private val input = FileInputStream(descriptor.fileDescriptor)

    override fun fileId(): FileId {
        val stat = Os.fstat(descriptor.fileDescriptor)
        return FileId(stat.st_dev, stat.st_ino)
    }

    override fun procPath(): String = Os.readlink("/proc/self/fd/${descriptor.fd}")

    override fun regularFileSize(): Long? {
        val stat = Os.fstat(descriptor.fileDescriptor)
        return if (OsConstants.S_ISREG(stat.st_mode)) stat.st_size else null
    }

    override fun read(buffer: ByteArray, offset: Int, length: Int): Int = input.read(buffer, offset, length)

    /** Closes the descriptor (the stream shares it and is not closed separately). */
    override fun close() = descriptor.close()
}
