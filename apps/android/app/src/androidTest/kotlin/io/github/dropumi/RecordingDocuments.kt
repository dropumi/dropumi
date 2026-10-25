// SPDX-License-Identifier: MPL-2.0
//
// The fake graph's Documents: it accepts every URI and records every call,
// so a UI state test can tell whether the app tried to read a URI at all
// (UriGuard is not what such a test checks; the real one runs in the
// instrumented guard tests).

package io.github.dropumi

import io.github.dropumi.core.files.Documents
import io.github.dropumi.ffi.CallbackException
import io.github.dropumi.ffi.ForeignFile
import io.github.dropumi.ffi.ForeignReader
import io.github.dropumi.ffi.SendItem
import java.util.Collections

class RecordingDocuments : Documents {
    private val log: MutableList<String> = Collections.synchronizedList(mutableListOf())

    /** Every call so far, as "describe <uri>" or "prepare <uri>". */
    val calls: List<String> get() = synchronized(log) { log.toList() }

    fun clear() = log.clear()

    override fun describe(uri: String): Documents.Described {
        log += "describe $uri"
        return Documents.Described(NAME, 1u)
    }

    override fun prepare(uri: String): Documents.Prepared {
        log += "prepare $uri"
        return Documents.Prepared.Stream(SendItem.Stream(Unreadable))
    }

    /** Never read in these tests; reading it fails like a document that cannot be read. */
    private object Unreadable : ForeignFile {
        override fun displayName() = NAME
        override fun size(): ULong = 1u
        override fun modified(): Long? = null
        override fun openReader(): ForeignReader = throw CallbackException.Failed("test document")
    }

    companion object {
        const val NAME = "recorded"
    }
}
