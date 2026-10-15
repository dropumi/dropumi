// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.discovery

import io.github.dropumi.ffi.CallbackException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * R24: no stack trace, URI or path reaches Rust. Every PlatformDiscovery
 * method, fed an exception whose text holds a URI and a path, either
 * succeeds or throws only CallbackException.Failed with a fixed phrase.
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
}
