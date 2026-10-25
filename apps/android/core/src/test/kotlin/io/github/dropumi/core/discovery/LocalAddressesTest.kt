// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.discovery

import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Test

class LocalAddressesTest {
    private fun ip(vararg bytes: Int): InetAddress = InetAddress.getByAddress(ByteArray(bytes.size) { bytes[it].toByte() })

    @Test
    fun localAddressesExcludeLoopbackAndLinkLocal() {
        val wifi = LanCandidate(
            wifi = true,
            addresses = listOf(
                ip(127, 0, 0, 1),
                ip(169, 254, 3, 4),
                ip(0, 0, 0, 0),
                ip(192, 168, 1, 5),
                ip(0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1),
                ip(192, 168, 1, 5),
            ),
        )
        assertEquals(listOf("192.168.1.5"), LocalAddressRules.ipv4(listOf(wifi)))
    }

    /** NET-5: with a cellular default network and Wi-Fi without internet, only the Wi-Fi address counts. */
    @Test
    fun localAddressesUseWifiAndEthernetNeverCellular() {
        val networks = listOf(
            LanCandidate(cellular = true, addresses = listOf(ip(10, 64, 1, 2))),
            LanCandidate(wifi = true, addresses = listOf(ip(192, 168, 1, 5))),
            LanCandidate(ethernet = true, addresses = listOf(ip(10, 0, 0, 7))),
            // A network that is both Wi-Fi and cellular is not a LAN.
            LanCandidate(wifi = true, cellular = true, addresses = listOf(ip(10, 99, 0, 1))),
        )
        assertEquals(listOf("192.168.1.5", "10.0.0.7"), LocalAddressRules.ipv4(networks))
    }
}
