// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.discovery

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HostLabelGateTest {
    /** Nothing has been observed yet: every Android device is browse-only. */
    @Test
    fun nsdGateClosedByDefault() {
        for (sdk in 26..40) {
            for (module in listOf(null, 0L, 1L, 350_000_000L, Long.MAX_VALUE)) {
                assertFalse("API $sdk module $module", HostLabelGate.OBSERVED.allows(sdk, module))
            }
        }
        val port = FakeNsdPort()
        val nsd = adapter(port, gate = HostLabelGate.OBSERVED)
        assertFalse(nsd.gateOpen)
        assertEquals(AdvertiseState.GateClosed, nsd.state.value)
        val refusal = runCatching { nsd.advertise(ad("dropumi-11111111")) }.exceptionOrNull() as NsdFailure
        assertEquals(NsdAdapterCore.GATE_CLOSED, refusal.phrase)
        assertTrue("nothing was registered", port.registered.isEmpty())
    }

    @Test
    fun nsdGateOpensOnlyForObservedPairs() {
        val gate = HostLabelGate(
            listOf(
                HostLabelGate.Observed(28, null),
                HostLabelGate.Observed(34, 340_000_000L..340_000_000L),
            ),
        )
        assertTrue(gate.allows(28, null))
        assertFalse(gate.allows(27, null))
        assertFalse(gate.allows(29, null))
        assertTrue(gate.allows(34, 340_000_000L))
        assertFalse(gate.allows(35, 340_000_000L))
        assertFalse(gate.allows(34, 340_000_001L))
        // A module-less entry never opens a level that has a module.
        assertFalse(HostLabelGate(listOf(HostLabelGate.Observed(34, null))).allows(34, 1L))
    }

    @Test
    fun nsdGateClosedWhenModuleVersionUnreadable() {
        val gate = HostLabelGate(listOf(HostLabelGate.Observed(34, 1L..Long.MAX_VALUE)))
        assertFalse(gate.allows(34, null))
        assertFalse(adapterFor(gate, sdk = 34, module = null).gateOpen)
    }

    @Test
    fun nsdGateClosedOutsideObservedRange() {
        val gate = HostLabelGate(listOf(HostLabelGate.Observed(35, 100L..200L)))
        assertFalse(gate.allows(35, 99L))
        assertTrue(gate.allows(35, 100L))
        assertTrue(gate.allows(35, 150L))
        assertTrue(gate.allows(35, 200L))
        assertFalse(gate.allows(35, 201L))
    }

    private fun adapterFor(gate: HostLabelGate, sdk: Int, module: Long?) =
        NsdAdapterCore(FakeNsdPort(), gate, sdk, module, ipv4Addresses = { listOf("192.168.1.5") })
}
