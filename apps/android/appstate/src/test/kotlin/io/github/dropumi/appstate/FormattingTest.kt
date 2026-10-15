// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.appstate

import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FormattingTest {
    @Test
    fun bytesUseDecimalUnits() {
        val l = Locale.US
        assertEquals("0 bytes", Formatting.bytes(0u, l))
        assertEquals("1 byte", Formatting.bytes(1u, l))
        assertEquals("999 bytes", Formatting.bytes(999u, l))
        assertEquals("1 KB", Formatting.bytes(1_000u, l))
        assertEquals("18.2 MB", Formatting.bytes(18_200_000u, l))
        assertEquals("18 MB", Formatting.bytes(18_000_000u, l))
        assertEquals("1.25 GB", Formatting.bytes(1_250_000_000u, l))
        assertEquals("18,2 MB", Formatting.bytes(18_200_000u, Locale.GERMANY))
        assertTrue(Formatting.bytes(ULong.MAX_VALUE, l).endsWith(" EB"))
    }

    @Test
    fun countdownsAndTimesLeft() {
        assertEquals("0:00", Formatting.countdown(-5))
        assertEquals("1:05", Formatting.countdown(65))
        assertEquals("1 s left", Formatting.timeLeft(0.2))
        assertEquals("59 s left", Formatting.timeLeft(59.0))
        assertEquals("2 min left", Formatting.timeLeft(61.0))
    }

    @Test
    fun errorCatalogCoversEveryErrorCode() {
        for (code in ErrorCode.entries) {
            val text = ErrorCatalog.text(code)
            assertTrue(code.name, text.title.isNotBlank() && text.message.isNotBlank())
            assertEquals(code.name, text.message, ErrorCatalog.message(DropumiException.Transfer(code, "local detail")))
        }
    }

    /** Text from anything but the core's own detail sentences never reaches the screen. */
    @Test
    fun foreignExceptionTextIsNeverShown() {
        val leaky = RuntimeException("content://x/y /data/user/0/io.github.dropumi/no_backup/dropumi/identity.key")
        val message = ErrorCatalog.message(leaky)
        assertFalse(message.contains("content://"))
        assertFalse(message.contains("/data/"))
        assertFalse(ErrorCatalog.message(DropumiException.Internal("panic at /src/x.rs")).contains("/src/"))
    }
}
