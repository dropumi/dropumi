// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui.design

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.math.pow

/**
 * The colour tables in design.md, read from the file itself (its path comes
 * from the `dropumi.designMd` system property, set in app/build.gradle.kts).
 */
private object DesignMd {
    private const val HEX = "#([0-9A-Fa-f]{6}(?:[0-9A-Fa-f]{2})?)"

    // | `bg` | `#F4F8FA` | `#07121F` | screen / window background |
    private val themedRow = Regex("^\\| `([a-z0-9-]+)` \\| `$HEX` \\| `$HEX` \\|")

    // | Receive card | `#12294A` | file list card on navy |
    private val fixedRow = Regex("^\\| ([A-Za-z ]+?) \\| `$HEX` \\|")

    private val lines: List<String> = run {
        val path = System.getProperty("dropumi.designMd")
            ?: throw AssertionError("the dropumi.designMd system property is not set")
        File(path).readLines()
    }

    /** Token name to (light, dark), from the theme table. */
    private val themedTokens: Map<String, Pair<Color, Color>> = lines.mapNotNull { line ->
        themedRow.find(line)?.destructured?.let { (name, light, dark) -> name to (color(light) to color(dark)) }
    }.toMap()

    /** Name to colour, from the fixed-colour table (and the brand palette). */
    private val fixedColours: Map<String, Color> = lines.mapNotNull { line ->
        fixedRow.find(line)?.destructured?.let { (name, hex) -> name to color(hex) }
    }.toMap()

    fun themed(token: String, dark: Boolean): Color {
        val (light, darkValue) = themedTokens[token] ?: throw AssertionError("design.md has no `$token` token")
        return if (dark) darkValue else light
    }

    fun fixed(name: String): Color = fixedColours[name] ?: throw AssertionError("design.md has no \"$name\" colour")

    /** `RRGGBB` or `RRGGBBAA` (design.md's order) to a Compose colour. */
    private fun color(hex: String): Color {
        val rgb = hex.substring(0, 6).toLong(16)
        val alpha = if (hex.length == 8) hex.substring(6, 8).toLong(16) else 0xFF
        return Color((alpha shl 24) or rgb)
    }
}

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

    /** Both themes, every field: the data class equality covers each token, and a new field fails to compile here. */
    @Test
    fun paletteMatchesDesignMd() {
        for ((dark, palette) in listOf(false to DSPalette.Light, true to DSPalette.Dark)) {
            fun md(token: String) = DesignMd.themed(token, dark)
            val expected = DSColors(
                isDark = dark,
                bg = md("bg"),
                surface = md("surface"),
                surface2 = md("surface-2"),
                textPrimary = md("text-primary"),
                textSecondary = md("text-secondary"),
                border = md("border"),
                accent = md("accent"),
                accentSoft = md("accent-soft"),
                accentText = md("accent-text"),
                ring = md("ring"),
                onAccent = md("on-accent"),
                success = md("success"),
                danger = md("danger"),
                navy = md("navy"),
            )
            assertEquals(if (dark) "dark" else "light", expected, palette)
        }
    }

    @Test
    fun receivePaletteMatchesDesignMd() {
        assertEquals(DesignMd.fixed("Deep Navy"), DSReceive.background)
        assertEquals(DesignMd.fixed("Receive card"), DSReceive.card)
        assertEquals(DesignMd.fixed("Receive divider"), DSReceive.divider)
        assertEquals(DesignMd.fixed("Receive muted text"), DSReceive.muted)
        // On navy the prompt uses the dark accent pair (design.md contrast notes).
        assertEquals(DesignMd.themed("accent", dark = true), DSReceive.accent)
        assertEquals(DesignMd.themed("on-accent", dark = true), DSReceive.onAccent)
        // The one deliberate exception: design.md's outline is about 2.1:1 on
        // navy and is raised to 3:1 (declineOutlineOnNavyIsAtLeastThreeToOne).
        assertTrue(DSReceive.outline != DesignMd.fixed("Receive outline"))
        assertEquals(Color(0xFF5B7A9C), DSReceive.outline)
        // Every DSReceive colour is checked above (Compose colours are stored as long).
        val colours = DSReceive::class.java.declaredFields.filter { it.type == java.lang.Long.TYPE }.map { it.name }
        assertEquals(
            setOf("background", "card", "divider", "muted", "outline", "accent", "onAccent"),
            colours.toSet(),
        )
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
