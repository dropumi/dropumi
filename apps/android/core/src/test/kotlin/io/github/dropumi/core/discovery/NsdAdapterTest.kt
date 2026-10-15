// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.discovery

import kotlin.concurrent.thread
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertTrue
import org.junit.Test

class NsdAdapterTest {
    @Test
    fun nsdRenamedRegistrationIsWithdrawnAndDropped() {
        for (infoCallbacks in listOf(false, true)) {
            val port = FakeNsdPort(hasServiceInfoCallback = infoCallbacks)
            val nsd = adapter(port)
            // advertise() returns the requested name at once.
            assertEquals("dropumi-11111111", nsd.advertise(ad("dropumi-11111111")))
            assertEquals(AdvertiseState.Registering, nsd.state.value)
            val (_, registration) = port.registered.single()
            registration.registered("dropumi-11111111 (2)")
            nsd.awaitIdle()
            assertEquals(listOf(registration), port.unregistered)
            assertEquals(AdvertiseState.Failed(AdvertiseState.Reason.RENAMED), nsd.state.value)

            val sink = RecordingSink()
            nsd.startBrowse(sink)
            val browse = port.discoveries.single()
            browse.found(service("dropumi-11111111 (2)"))
            browse.found(service("dropumi-22222222"))
            nsd.awaitIdle()
            val asked = if (infoCallbacks) port.infoCallbacks.map { it.first.name } else port.resolves.map { it.first.name }
            assertEquals("the renamed own name is dropped until it is withdrawn", listOf("dropumi-22222222"), asked)

            // Once NSD confirms the withdrawal, that name is someone else's.
            registration.unregistered()
            browse.found(service("dropumi-11111111 (2)"))
            nsd.awaitIdle()
            if (infoCallbacks) {
                assertEquals(2, port.infoCallbacks.size)
            } else {
                port.resolves.first().second.resolved(service("dropumi-22222222"))
                nsd.awaitIdle()
                assertEquals(listOf("dropumi-22222222", "dropumi-11111111 (2)"), port.resolves.map { it.first.name })
            }
        }
    }

    @Test
    fun registeredUnderTheRequestedNameIsRegistered() {
        val port = FakeNsdPort()
        val nsd = adapter(port)
        nsd.advertise(ad("dropumi-11111111"))
        port.registered.single().second.registered("dropumi-11111111")
        nsd.awaitIdle()
        assertEquals(AdvertiseState.Registered, nsd.state.value)
        nsd.withdraw("dropumi-11111111")
        assertEquals(1, port.unregistered.size)
        assertEquals(AdvertiseState.Idle, nsd.state.value)
        // The TXT is handed to NSD unchanged.
        assertArrayEquals("Kitchen tablet".toByteArray(), port.registered.single().first.attributes["n"])
    }

    @Test
    fun nsdResolveQueueIsBoundedAt64() {
        val port = FakeNsdPort(hasServiceInfoCallback = false)
        val nsd = adapter(port)
        val sink = RecordingSink()
        nsd.startBrowse(sink)
        val browse = port.discoveries.single()
        repeat(70) { browse.found(service("svc-$it")) }
        browse.found(service("svc-1")) // a duplicate while it waits
        nsd.awaitIdle()
        assertEquals("one resolve at a time", 1, port.resolves.size)
        // Answer each resolve in turn until none is left.
        var answered = 0
        while (answered < port.resolves.size) {
            val (svc, listener) = port.resolves[answered++]
            listener.resolved(svc)
            nsd.awaitIdle()
        }
        assertEquals((0 until 64).map { "svc-$it" }, sink.resolvedNames())
    }

    @Test
    fun nsdServiceInfoCallbacksBoundedAt64() {
        val port = FakeNsdPort(hasServiceInfoCallback = true)
        val nsd = adapter(port)
        val sink = RecordingSink()
        nsd.startBrowse(sink)
        val browse = port.discoveries.single()
        repeat(65) { browse.found(service("svc-$it")) }
        browse.found(service("svc-3"))
        nsd.awaitIdle()
        assertEquals("the 65th is dropped", (0 until 64).map { "svc-$it" }, port.liveInfoNames())

        // Lost: its callback is unregistered and the core is told.
        browse.lost(service("svc-0"))
        nsd.awaitIdle()
        assertEquals(63, port.liveInfoNames().size)
        assertTrue("lost svc-0" in sink.events)
        browse.found(service("svc-99"))
        nsd.awaitIdle()
        assertEquals(64, port.liveInfoNames().size)

        // A new session unregisters every callback of the old one.
        nsd.startBrowse(RecordingSink())
        nsd.awaitIdle()
        assertTrue(port.liveInfoNames().isEmpty())
        port.discoveries.last().found(service("svc-1"))
        nsd.awaitIdle()
        assertEquals(listOf("svc-1"), port.liveInfoNames())

        // Stopping unregisters them too.
        nsd.stopBrowse()
        assertTrue(port.liveInfoNames().isEmpty())
        assertEquals(2, port.stopped.size)
    }

    @Test
    fun nsdAlreadyActiveRetriesAtMostThreeTimes() {
        val port = FakeNsdPort(hasServiceInfoCallback = false)
        val nsd = adapter(port)
        val sink = RecordingSink()
        nsd.startBrowse(sink)
        val browse = port.discoveries.single()
        browse.found(service("a"))
        browse.found(service("b"))
        nsd.awaitIdle()
        repeat(4) {
            val (svc, listener) = port.resolves.last()
            assertEquals("a", svc.name)
            listener.failed(svc, NsdPort.FAILURE_ALREADY_ACTIVE)
            nsd.awaitIdle()
        }
        // One try and three retries for "a", then "b".
        assertEquals(listOf("a", "a", "a", "a", "b"), port.resolves.map { it.first.name })
        val (svc, listener) = port.resolves.last()
        listener.resolved(svc)
        nsd.awaitIdle()
        assertEquals(listOf("b"), sink.resolvedNames())
    }

    @Test
    fun nsdStaleSessionCallbacksAreIgnored() {
        val port = FakeNsdPort(hasServiceInfoCallback = false)
        val nsd = adapter(port)
        val old = RecordingSink()
        nsd.startBrowse(old)
        val oldBrowse = port.discoveries.single()
        oldBrowse.found(service("a"))
        nsd.awaitIdle()
        val (svc, oldResolve) = port.resolves.single()

        val current = RecordingSink()
        nsd.startBrowse(current)
        oldBrowse.found(service("b"))
        oldBrowse.lost(service("a"))
        oldResolve.resolved(svc)
        nsd.awaitIdle()
        assertTrue(old.events.isEmpty())
        assertTrue(current.events.isEmpty())
        assertEquals("nothing new was resolved for the old session", 1, port.resolves.size)
    }

    @Test
    fun nsdNewListenerPerSession() {
        val port = FakeNsdPort()
        val nsd = adapter(port)
        nsd.startBrowse(RecordingSink())
        nsd.startBrowse(RecordingSink())
        assertEquals(2, port.discoveries.size)
        assertNotSame(port.discoveries[0], port.discoveries[1])
        assertEquals("the old session was stopped", listOf(port.discoveries[0]), port.stopped)
        nsd.advertise(ad("dropumi-11111111"))
        nsd.withdraw("dropumi-11111111")
        nsd.advertise(ad("dropumi-11111111"))
        assertEquals(2, port.registered.size)
        assertNotSame(port.registered[0].second, port.registered[1].second)
    }

    @Test
    fun nsdTxtNullValueBecomesEmpty() {
        val port = FakeNsdPort(hasServiceInfoCallback = true)
        val nsd = adapter(port)
        val sink = RecordingSink()
        nsd.startBrowse(sink)
        port.discoveries.single().found(service("a"))
        nsd.awaitIdle()
        port.infoCallbacks.single().second.updated(service("a", mapOf("v" to "1".toByteArray(), "flag" to null)))
        nsd.awaitIdle()
        val txt = sink.txt.single().associate { it.key to it.value }
        assertArrayEquals("1".toByteArray(), txt["v"])
        assertArrayEquals(ByteArray(0), txt["flag"])
    }

    @Test
    fun nsdAdvertiseWithoutIpv4Fails() {
        val port = FakeNsdPort()
        val nsd = adapter(port, addresses = emptyList())
        val e = runCatching { nsd.advertise(ad("dropumi-11111111")) }.exceptionOrNull()
        assertEquals(NsdAdapterCore.NO_IPV4, (e as NsdFailure).phrase)
        assertTrue(port.registered.isEmpty())
        assertEquals(AdvertiseState.Failed(AdvertiseState.Reason.NO_IPV4_NETWORK), nsd.state.value)
    }

    @Test
    fun nsdLateRegistrationFailureIsReported() {
        val port = FakeNsdPort()
        val nsd = adapter(port)
        nsd.advertise(ad("dropumi-11111111"))
        port.registered.single().second.registrationFailed(0)
        nsd.awaitIdle()
        assertEquals(AdvertiseState.Failed(AdvertiseState.Reason.REGISTRATION_FAILED), nsd.state.value)
    }

    @Test
    fun nsdSinkNeverCalledOnMainThread() {
        for (infoCallbacks in listOf(false, true)) {
            val port = FakeNsdPort(hasServiceInfoCallback = infoCallbacks)
            val nsd = adapter(port)
            val sink = RecordingSink()
            nsd.startBrowse(sink)
            val browse = port.discoveries.single()
            // NSD below 33 calls back on a thread of its choosing: here, "main".
            thread(name = "main") {
                browse.found(service("a"))
                browse.startFailed(0)
            }.join()
            nsd.awaitIdle()
            thread(name = "main") {
                if (infoCallbacks) {
                    port.infoCallbacks.single().second.updated(service("a"))
                } else {
                    port.resolves.single().second.resolved(service("a"))
                }
                browse.lost(service("a"))
            }.join()
            nsd.awaitIdle()
            assertEquals(listOf("failed", "resolved a", "lost a"), sink.events)
            assertEquals(setOf(NsdAdapterCore.THREAD_NAME), sink.threads)
        }
    }

    /** A sink that throws (the core refusing a result) does not stop the adapter. */
    @Test
    fun aFailingSinkDoesNotStopTheAdapter() {
        val port = FakeNsdPort(hasServiceInfoCallback = true)
        val nsd = adapter(port)
        var calls = 0
        nsd.startBrowse(
            object : io.github.dropumi.ffi.BrowseSinkInterface {
                override fun failed(reason: String) = Unit
                override fun lost(instance: String) = Unit
                override fun resolved(
                    instance: String,
                    txt: List<io.github.dropumi.ffi.TxtEntry>,
                    addresses: List<String>,
                    port: UShort,
                ) {
                    calls++
                    throw IllegalStateException("refused")
                }
            },
        )
        port.discoveries.single().found(service("a"))
        port.discoveries.single().found(service("b"))
        nsd.awaitIdle()
        port.infoCallbacks.forEach { (svc, cb) -> cb.updated(svc) }
        nsd.awaitIdle()
        assertEquals(2, calls)
        assertFalse(port.infoCallbacks.isEmpty())
    }
}
