// SPDX-License-Identifier: MPL-2.0
//
// A content:// document the core reads through the app (ForeignFile). The
// only way in is create(), which runs UriGuard's scheme and own-provider
// checks first, so a refused URI is never queried or opened, not even for
// its metadata. openReader() runs the whole guard again, on the descriptor
// it then reads, for every source (share sheet, document picker, interop).
//
// No stack trace or URI reaches Rust (R24). openReader and read are
// fallible: any failure becomes CallbackException.Failed with a fixed
// phrase. displayName, size, modified and closeDocument are infallible in
// Rust (a Kotlin exception there would end the send as an internal error
// carrying the trace), so they never throw: they return "shared-file",
// null, null, or swallow the error.

package io.github.dropumi.core.files

import io.github.dropumi.ffi.CallbackException
import io.github.dropumi.ffi.ForeignFile
import io.github.dropumi.ffi.ForeignReader

class ContentUriFile private constructor(
    private val source: DocumentSource,
    private val uri: String,
    private val guard: DocumentGuard,
) : ForeignFile {
    sealed interface Created {
        data class Allowed(val file: ContentUriFile) : Created
        data class Refused(val reason: UriGuardPolicy.Refusal) : Created
    }

    /** Read once, on the first metadata call (the core's calling thread, never the main thread). */
    private val meta: DocumentMeta? by lazy {
        try {
            source.query(uri)
        } catch (_: Throwable) {
            null
        }
    }

    override fun displayName(): String = try {
        nameFor(meta?.displayName)
    } catch (_: Throwable) {
        FALLBACK_NAME
    }

    /** The provider's size; otherwise fstat of a guarded descriptor if it is a regular file; otherwise null. */
    override fun size(): ULong? = try {
        meta?.size?.takeIf { it >= 0 }?.toULong() ?: statSize()?.toULong()
    } catch (_: Throwable) {
        null
    }

    override fun modified(): Long? = try {
        meta?.lastModifiedMs?.takeIf { it > 0 }?.let { it / MILLIS_PER_SECOND }
    } catch (_: Throwable) {
        null
    }

    override fun openReader(): ForeignReader {
        try {
            if (guard.checkUri(uri) != null) throw CallbackException.Failed(UNREADABLE)
            val document = source.open(uri)
            val refusal = try {
                guard.checkOpen(document)
            } catch (_: Throwable) {
                UriGuardPolicy.Refusal.UNCHECKABLE
            }
            if (refusal != null) {
                closeQuietly(document)
                throw CallbackException.Failed(UNREADABLE)
            }
            return Reader(document)
        } catch (_: Throwable) {
            throw CallbackException.Failed(UNREADABLE)
        }
    }

    /** fstat size through a descriptor opened for this only, guarded and always closed. */
    private fun statSize(): Long? {
        if (guard.checkUri(uri) != null) return null
        return source.open(uri).use { document ->
            if (guard.checkOpen(document) != null) null else document.regularFileSize()
        }
    }

    private class Reader(private val document: OpenDocument) : ForeignReader {
        @Synchronized
        override fun read(maxLen: UInt): ByteArray {
            try {
                val length = minOf(maxLen.toLong(), MAX_READ.toLong()).toInt()
                if (length == 0) return ByteArray(0)
                val buffer = ByteArray(length)
                val n = document.read(buffer, 0, length)
                return if (n <= 0) ByteArray(0) else buffer.copyOf(n)
            } catch (_: Throwable) {
                throw CallbackException.Failed(UNREADABLE)
            }
        }

        override fun closeDocument() = closeQuietly(document)
    }

    companion object {
        /** The name used when the provider gives none: never a part of the URI. */
        const val FALLBACK_NAME = "shared-file"
        const val UNREADABLE = "could not read the document"
        private const val MAX_READ = 1024 * 1024
        private const val MILLIS_PER_SECOND = 1000L

        /** Runs UriGuard's steps 1-2 and only then makes the file. */
        fun create(source: DocumentSource, uri: String, guard: DocumentGuard): Created {
            val refusal = try {
                guard.checkUri(uri)
            } catch (_: Throwable) {
                UriGuardPolicy.Refusal.UNCHECKABLE
            }
            return if (refusal != null) Created.Refused(refusal) else Created.Allowed(ContentUriFile(source, uri, guard))
        }

        /**
         * The name to offer: the last component of DISPLAY_NAME without control
         * characters; `shared-file` if nothing is left. The URI (authority,
         * path segments, document id) is never used: it would send
         * provider-internal identifiers to the peer.
         */
        fun nameFor(displayName: String?): String {
            val last = displayName?.substringAfterLast('/')?.substringAfterLast('\\') ?: return FALLBACK_NAME
            val clean = last.filterNot { it.isISOControl() }.trim()
            return clean.ifEmpty { FALLBACK_NAME }
        }

        internal fun closeQuietly(document: OpenDocument) {
            try {
                document.close()
            } catch (_: Throwable) {
                // Nothing more to do.
            }
        }
    }
}
