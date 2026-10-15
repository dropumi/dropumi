// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.discovery

import io.github.dropumi.core.files.ContentUriFile
import io.github.dropumi.core.files.DocumentGuard
import io.github.dropumi.core.files.DocumentMeta
import io.github.dropumi.core.files.DocumentSource
import io.github.dropumi.core.files.FakeDoc
import io.github.dropumi.core.files.FakeDocumentSource
import io.github.dropumi.core.files.OpenDocument
import io.github.dropumi.core.files.PolicyGuard
import io.github.dropumi.core.files.UriGuardPolicy
import io.github.dropumi.ffi.CallbackException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * R24: no stack trace, URI or path reaches Rust. Every PlatformDiscovery,
 * ForeignFile and ForeignReader method, fed an exception whose text holds a
 * URI and a path: the fallible ones succeed or throw only
 * CallbackException.Failed with a fixed phrase; the infallible ones never
 * throw and return fixed values.
 */
class CallbackLeakTest {
    private val leak = RuntimeException("content://com.example.files/doc/1 /data/user/0/io.github.dropumi/no_backup/dropumi/identity.key")

    private fun assertFixed(expected: Set<String>, call: () -> Unit) {
        val thrown = runCatching(call).exceptionOrNull() ?: return
        val failed = thrown as CallbackException.Failed
        assertTrue("a fixed phrase", failed.detail in expected)
        assertFalse(failed.detail.contains("content://"))
        assertFalse(failed.detail.contains("/data/"))
        assertNull(failed.cause)
    }

    @Test
    fun callbacksNeverLeakStackTraces() {
        discoveryCallbacks()
        documentCallbacks()
    }

    private fun discoveryCallbacks() {
        val port = FakeNsdPort().also { it.failure = leak }
        val discovery = NsdDiscovery(adapter(port))
        val phrases = setOf(
            NsdDiscovery.ADVERTISE_FAILED, NsdDiscovery.WITHDRAW_FAILED, NsdDiscovery.BROWSE_FAILED,
            NsdDiscovery.STOP_FAILED, NsdAdapterCore.FAILED,
        )
        assertFixed(phrases) { discovery.advertise(ad("dropumi-11111111")) }
        assertFixed(phrases) { discovery.withdraw("dropumi-11111111") }
        assertFixed(phrases) { discovery.startBrowsing(RecordingSink()) }
        assertFixed(phrases) { discovery.stopBrowse() }

        // The refusals with their own fixed phrases pass through unchanged.
        val closed = NsdDiscovery(adapter(FakeNsdPort(), gate = HostLabelGate.OBSERVED))
        val refusal = runCatching { closed.advertise(ad("dropumi-11111111")) }.exceptionOrNull() as CallbackException.Failed
        assertEquals(NsdAdapterCore.GATE_CLOSED, refusal.detail)
        val offline = NsdDiscovery(adapter(FakeNsdPort(), addresses = emptyList()))
        val noNetwork = runCatching { offline.advertise(ad("dropumi-11111111")) }.exceptionOrNull() as CallbackException.Failed
        assertEquals(NsdAdapterCore.NO_IPV4, noNetwork.detail)
    }

    private fun documentCallbacks() {
        val uri = "content://com.example.files/document/1"
        val source = FakeDocumentSource(mapOf(uri to FakeDoc(DocumentMeta("a.jpg", 3), ByteArray(3), failure = leak)))
        val file = (ContentUriFile.create(source, uri, PolicyGuard()) as ContentUriFile.Created.Allowed).file
        // Infallible in Rust: never throw. The failing query gives no metadata,
        // so the size comes from the guarded descriptor.
        assertEquals(ContentUriFile.FALLBACK_NAME, file.displayName())
        assertEquals(3uL, file.size())
        assertNull(file.modified())
        // Fallible: a fixed phrase only.
        val reader = file.openReader()
        assertFixed(setOf(ContentUriFile.UNREADABLE)) { reader.read(16u) }
        reader.closeDocument()

        // A source that throws on open, and a guard that throws, too.
        val throwing = object : DocumentSource {
            override fun query(uri: String): DocumentMeta = throw leak
            override fun open(uri: String): OpenDocument = throw leak
        }
        val broken = (ContentUriFile.create(throwing, uri, PolicyGuard()) as ContentUriFile.Created.Allowed).file
        assertEquals(ContentUriFile.FALLBACK_NAME, broken.displayName())
        assertNull(broken.size())
        assertNull(broken.modified())
        assertFixed(setOf(ContentUriFile.UNREADABLE)) { broken.openReader() }
        val angry = object : DocumentGuard {
            override fun checkUri(uri: String): UriGuardPolicy.Refusal? = throw leak
            override fun checkOpen(document: OpenDocument): UriGuardPolicy.Refusal? = throw leak
        }
        assertTrue(ContentUriFile.create(source, uri, angry) is ContentUriFile.Created.Refused)
    }
}
