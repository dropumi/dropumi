// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.appstate

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TargetInputTest {
    @Test
    fun connectAddressRejectsBadPort() {
        for (bad in listOf(
            "192.168.1.20:0",
            "192.168.1.20:65536",
            "192.168.1.20:99999999999",
            "192.168.1.20:-1",
            "192.168.1.20:+80",
            "192.168.1.20:abc",
            "192.168.1.20:",
            "192.168.1.20",
            ":47321",
            "fe80::1:47321",
            "[fe80::1]",
            "[]:47321",
            "host name:47321",
            "",
            "   ",
        )) {
            assertNull("accepted \"$bad\"", TargetInput.address(bad))
            assertTrue("no reason for \"$bad\"", TargetInput.addressProblem(bad) != null || bad.isBlank())
        }
        assertEquals("192.168.1.20:1", TargetInput.address("192.168.1.20:1"))
        assertEquals("192.168.1.20:65535", TargetInput.address(" 192.168.1.20:65535 "))
        assertEquals("[fe80::1]:47321", TargetInput.address("[fe80::1]:47321"))
        assertEquals("laptop.local:47321", TargetInput.address("laptop.local:47321"))
        assertNull(TargetInput.addressProblem("192.168.1.20:47321"))
        assertEquals("The port must be a number from 1 to 65535.", TargetInput.addressProblem("192.168.1.20:0"))
        // Nothing typed yet is not an error to show.
        assertNull(TargetInput.addressProblem(""))
    }

    @Test
    fun connectPinNeedsTwentyFourCharacters() {
        val full = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRST"
        assertTrue("an empty pin is allowed", TargetInput.pinValid(""))
        assertNull("an empty pin is no pin", TargetInput.pin("  "))
        assertFalse(TargetInput.pinValid(full.take(23)))
        assertTrue(TargetInput.pinValid(full.take(24)))
        assertTrue(TargetInput.pinValid(full))
        // Dashes and spaces are only for reading: they do not count.
        assertFalse(TargetInput.pinValid("ABCD-EFGH-IJKL-MNOP-QRST-UVW"))
        assertTrue(TargetInput.pinValid("ABCD-EFGH-IJKL-MNOP-QRST-UVWX"))
        assertEquals("ABCDEFGHIJKLMNOPQRSTUVWX", TargetInput.pin("abcd efgh-ijkl mnop-qrst uvwx"))
        assertFalse("longer than a fingerprint", TargetInput.pinValid(full + "A"))
    }

    @Test
    fun addByFingerprintNeedsAllFiftyTwoCharacters() {
        val full = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRST"
        assertEquals(full, TargetInput.fullFingerprint(Formatting.fingerprint(full)))
        assertEquals(full, TargetInput.fullFingerprint(full.lowercase()))
        assertNull(TargetInput.fullFingerprint(full.dropLast(1)))
        assertNull(TargetInput.fullFingerprint(full + "A"))
        assertNull(TargetInput.fullFingerprint(""))
    }
}
