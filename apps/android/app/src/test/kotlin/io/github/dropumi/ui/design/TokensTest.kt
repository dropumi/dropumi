// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui.design

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.pow

/** design.md tokens and the contrast rules the app relies on (WCAG 2 ratios). */
class TokensTest {
    private fun channel(c: Float): Double =
        if (c <= 0.04045f) c / 12.92 else ((c + 0.055) / 1.055).pow(2.4)

    private fun luminance(color: Color): Double =
        0.2126 * channel(color.red) + 0.7152 * channel(color.green) + 0.0722 * channel(color.blue)

    private fun contrast(a: Color, b: Color): Double {
        val (hi, lo) = listOf(luminance(a), luminance(b)).sortedDescending()
        return (hi + 0.05) / (lo + 0.05)
    }

    private fun assertContrast(min: Double, fg: Color, bg: Color, what: String) {
        val ratio = contrast(fg, bg)
        assertTrue("$what: $ratio < $min", ratio >= min)
    }

    @Test
    fun tokensMatchDesignMd() {
        assertEquals(Color(0xFFF4F8FA), DSPalette.Light.bg)
        assertEquals(Color(0xFF07121F), DSPalette.Dark.bg)
        assertEquals(Color(0xFF0E8A9B), DSPalette.Light.accent)
        assertEquals(Color(0xFF3CC4D4), DSPalette.Dark.accent)
        assertEquals(Color(0xFF0B1F3A), DSPalette.Light.navy)
        assertEquals(Color(0xFF0B1F3A), DSPalette.Dark.navy)
        assertEquals(Color(0xFF0B1F3A), DSReceive.background)
    }

    @Test
    fun smallTextMeetsFourAndAHalfToOne() {
        for (p in listOf(DSPalette.Light, DSPalette.Dark)) {
            val mode = if (p.isDark) "dark" else "light"
            assertContrast(4.5, p.textPrimary, p.bg, "$mode textPrimary on bg")
            assertContrast(4.5, p.textSecondary, p.bg, "$mode textSecondary on bg")
            assertContrast(4.5, p.textSecondary, p.surface, "$mode textSecondary on surface")
            assertContrast(4.5, p.accentText, p.accentSoft, "$mode accentText on accentSoft")
            assertContrast(4.5, p.accentText, p.surface, "$mode accentText on surface")
        }
        assertContrast(4.5, DSReceive.muted, DSReceive.background, "muted on navy")
        assertContrast(4.5, DSReceive.muted, DSReceive.card, "muted on the navy card")
        assertContrast(4.5, DSReceive.onAccent, DSReceive.accent, "Accept label")
    }

    @Test
    fun declineOutlineOnNavyIsAtLeastThreeToOne() {
        assertContrast(3.0, DSReceive.outline, DSReceive.background, "Decline outline on navy")
        assertContrast(3.0, DSReceive.outline, DSReceive.card, "Decline outline on the navy card")
    }
}
