// SPDX-License-Identifier: MPL-2.0
//
// Where documents to send come from, in plain Kotlin types, so
// ContentUriFile and the staging copy run on the host JVM with a fake.
// AndroidDocumentSource is the real one (an injected ContentResolver).

package io.github.dropumi.core.files

import java.io.Closeable

/** A document's metadata columns; any of them may be missing. */
data class DocumentMeta(
    /** OpenableColumns.DISPLAY_NAME, as the provider gives it. */
    val displayName: String? = null,
    /** OpenableColumns.SIZE in bytes. */
    val size: Long? = null,
    /** DocumentsContract.Document.COLUMN_LAST_MODIFIED, in milliseconds. */
    val lastModifiedMs: Long? = null,
)

/** The identity of an open file: `st_dev` and `st_ino` from fstat. */
data class FileId(val device: Long, val inode: Long)

/**
 * An open document. The descriptor that UriGuard checks is the one that is
 * read, so nothing can be swapped between the check and the read.
 */
interface OpenDocument : Closeable {
    /** fstat of the descriptor; throws if it fails. */
    fun fileId(): FileId

    /** Where `/proc/self/fd/<n>` points; throws if it cannot be read. */
    fun procPath(): String

    /** The size fstat reports if the descriptor is a regular file, else null; throws if fstat fails. */
    fun regularFileSize(): Long?

    /** Reads up to [length] bytes into [buffer]; -1 at the end. */
    fun read(buffer: ByteArray, offset: Int, length: Int): Int
}

interface DocumentSource {
    fun query(uri: String): DocumentMeta
    fun open(uri: String): OpenDocument
}

/** Checks a document before it is queried, opened or read (UriGuard). */
interface DocumentGuard {
    /** Steps 1-2 (scheme, own provider), on the URI text alone; null if allowed. */
    fun checkUri(uri: String): UriGuardPolicy.Refusal?

    /** Step 3 (file identity) on the descriptor that will be read; null if allowed. */
    fun checkOpen(document: OpenDocument): UriGuardPolicy.Refusal?
}
