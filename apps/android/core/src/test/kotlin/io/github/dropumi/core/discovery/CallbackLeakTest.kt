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
 * URI and a path:
 * - the fallible ones that must report the failure (advertise,
 *   startBrowse, openReader, read) throw CallbackException.Failed with a
 *   fixed phrase. Returning normally would hand Rust a success that never
 *   happened (an empty read is the end of the file, so a send would be cut
 *   short), so it fails the test;
 * - withdraw and stopBrowse swallow a failure by design (there is nothing
 *   left to withdraw or stop): they return normally or throw only a fixed
 *   phrase;
 * - the infallible ones never throw and return fixed values.
 */
class CallbackLeakTest {
    private val leak = RuntimeException("content://com.example.files/doc/1 /data/user/0/io.github.dropumi/no_backup/dropumi/identity.key")

    private fun assertNoLeak(failed: CallbackException.Failed) {
        for (text in listOf(failed.detail, failed.message.orEmpty())) {
            assertFalse(text, text.contains("content://"))
            assertFalse(text, text.contains("/data/"))
            assertFalse(text, text.contains("identity.key"))
        }
        assertNull(failed.cause)
    }

    /** The call must fail with CallbackException.Failed carrying one of [expected]; a normal return fails the test. */
    private fun assertFailsWithFixed(expected: Set<String>, call: () -> Unit) {
        val thrown = runCatching(call).exceptionOrNull()
            ?: throw AssertionError("returned normally: a failure must reach Rust as a failure")
        val failed = thrown as? CallbackException.Failed
            ?: throw AssertionError("threw ${thrown.javaClass.name}, not CallbackException.Failed")
        assertTrue("a fixed phrase: ${failed.detail}", failed.detail in expected)
        assertNoLeak(failed)
    }

    /** Only for methods that swallow failures by design: a normal return, or a fixed phrase. */
    private fun assertQuietOrFixed(expected: Set<String>, call: () -> Unit) {
        val thrown = runCatching(call).exceptionOrNull() ?: return
        val failed = thrown as? CallbackException.Failed
            ?: throw AssertionError("threw ${thrown.javaClass.name}, not CallbackException.Failed")
        assertTrue("a fixed phrase: ${failed.detail}", failed.detail in expected)
        assertNoLeak(failed)
    }

    @Test
    fun callbacksNeverLeakStackTraces() {
        discoveryCallbacks()
        documentCallbacks()
    }

    private fun discoveryCallbacks() {
        // advertise and startBrowse against a port that throws: a failure.
        val failing = NsdDiscovery(adapter(FakeNsdPort().also { it.failure = leak }))
        assertFailsWithFixed(setOf(NsdDiscovery.ADVERTISE_FAILED, NsdAdapterCore.FAILED)) {
            failing.advertise(ad("dropumi-11111111"))
        }
        assertFailsWithFixed(setOf(NsdDiscovery.BROWSE_FAILED, NsdAdapterCore.FAILED)) {
            failing.startBrowsing(RecordingSink())
        }

        // withdraw and stopBrowse after a working advertise and browse, with
        // the port throwing from then on: NSD's unregister and stopDiscovery
        // really throw, and the failure is swallowed (or a fixed phrase).
        val port = FakeNsdPort()
        val discovery = NsdDiscovery(adapter(port))
        discovery.advertise(ad("dropumi-11111111"))
        discovery.startBrowsing(RecordingSink())
        port.failure = leak
        assertQuietOrFixed(setOf(NsdDiscovery.WITHDRAW_FAILED, NsdAdapterCore.FAILED)) { discovery.withdraw("dropumi-11111111") }
        assertQuietOrFixed(setOf(NsdDiscovery.STOP_FAILED, NsdAdapterCore.FAILED)) { discovery.stopBrowse() }

        // The refusals with their own fixed phrases pass through unchanged.
        val closed = NsdDiscovery(adapter(FakeNsdPort(), gate = HostLabelGate.OBSERVED))
        assertFailsWithFixed(setOf(NsdAdapterCore.GATE_CLOSED)) { closed.advertise(ad("dropumi-11111111")) }
        val offline = NsdDiscovery(adapter(FakeNsdPort(), addresses = emptyList()))
        assertFailsWithFixed(setOf(NsdAdapterCore.NO_IPV4)) { offline.advertise(ad("dropumi-11111111")) }
    }

    private fun documentCallbacks() {
        val uri = "content://com.example.files/document/1"
        val unreadable = setOf(ContentUriFile.UNREADABLE)

        // A document whose query and reads throw.
        val source = FakeDocumentSource(mapOf(uri to FakeDoc(DocumentMeta("a.jpg", 3), ByteArray(3), failure = leak)))
        val file = (ContentUriFile.create(source, uri, PolicyGuard()) as ContentUriFile.Created.Allowed).file
        // Infallible in Rust: never throw. The failing query gives no metadata,
        // so the size comes from the guarded descriptor.
        assertEquals(ContentUriFile.FALLBACK_NAME, file.displayName())
        assertEquals(3uL, file.size())
        assertNull(file.modified())
        // Fallible: a read that throws must fail, never look like the end of the file.
        val reader = file.openReader()
        assertFailsWithFixed(unreadable) { reader.read(16u) }
        reader.closeDocument()

        // A source that throws on query and on open.
        val throwing = object : DocumentSource {
            override fun query(uri: String): DocumentMeta = throw leak
            override fun open(uri: String): OpenDocument = throw leak
        }
        val broken = (ContentUriFile.create(throwing, uri, PolicyGuard()) as ContentUriFile.Created.Allowed).file
        assertEquals(ContentUriFile.FALLBACK_NAME, broken.displayName())
        assertNull(broken.size())
        assertNull(broken.modified())
        assertFailsWithFixed(unreadable) { broken.openReader() }

        // A guard that throws: refused at create, and at open (fail closed).
        val angry = object : DocumentGuard {
            override fun checkUri(uri: String): UriGuardPolicy.Refusal? = throw leak
            override fun checkOpen(document: OpenDocument): UriGuardPolicy.Refusal? = throw leak
        }
        assertTrue(ContentUriFile.create(source, uri, angry) is ContentUriFile.Created.Refused)
        val openThrows = object : DocumentGuard {
            override fun checkUri(uri: String): UriGuardPolicy.Refusal? = null
            override fun checkOpen(document: OpenDocument): UriGuardPolicy.Refusal? = throw leak
        }
        val readable = FakeDocumentSource(mapOf(uri to FakeDoc(DocumentMeta("a.jpg", 3), ByteArray(3))))
        val unchecked = (ContentUriFile.create(readable, uri, openThrows) as ContentUriFile.Created.Allowed).file
        assertFailsWithFixed(unreadable) { unchecked.openReader() }
        assertTrue("nothing read", readable.calls.none { it.startsWith("read") })
        assertTrue("closed after the refusal", "close $uri" in readable.calls)
    }
}
