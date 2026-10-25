// SPDX-License-Identifier: MPL-2.0
//
// The design.md tokens as Compose colours and metrics. Use them by name;
// never use the raw values in screens.
package io.github.dropumi.ui.design

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** design.md colour tokens for one theme. */
@Immutable
data class DSColors(
    val isDark: Boolean,
    val bg: Color,
    val surface: Color,
    val surface2: Color,
    val textPrimary: Color,
    val textSecondary: Color,
    val border: Color,
    /** Light accent is about 4.1:1 on white: bold or large text and controls only. */
    val accent: Color,
    val accentSoft: Color,
    /**
     * Accent for small text (pills, links, captions): light `accent` is below
     * 4.5:1 on white and on `accent-soft`, so small light-mode text uses this
     * darker teal. The same token as the Apple apps.
     */
    val accentText: Color,
    val ring: Color,
    val onAccent: Color,
    val success: Color,
    val danger: Color,
    /** Fixed brand navy, the same in both themes. */
    val navy: Color,
)

/** The two themes from design.md. */
object DSPalette {
    val Light = DSColors(
        isDark = false,
        bg = Color(0xFFF4F8FA),
        surface = Color(0xFFFFFFFF),
        surface2 = Color(0xFFE8F0F4),
        textPrimary = Color(0xFF0B1F3A),
        textSecondary = Color(0xFF5B6B7D),
        border = Color(0xFFDCE5EB),
        accent = Color(0xFF0E8A9B),
        accentSoft = Color(0xFFD6EEF2),
        accentText = Color(0xFF0B6E7C),
        ring = Color(0x400E94A6),
        onAccent = Color(0xFFFFFFFF),
        success = Color(0xFF1E9E6A),
        danger = Color(0xFFC93B3B),
        navy = Color(0xFF0B1F3A),
    )

    val Dark = DSColors(
        isDark = true,
        bg = Color(0xFF07121F),
        surface = Color(0xFF0F2238),
        surface2 = Color(0xFF162C45),
        textPrimary = Color(0xFFF2F7FA),
        textSecondary = Color(0xFF9DB0C2),
        border = Color(0xFF1F3652),
        accent = Color(0xFF3CC4D4),
        accentSoft = Color(0xFF123A4C),
        accentText = Color(0xFF3CC4D4),
        ring = Color(0x403CC4D4),
        onAccent = Color(0xFF06111F),
        success = Color(0xFF4FD3A0),
        danger = Color(0xFFFF8080),
        navy = Color(0xFF0B1F3A),
    )
}

/** The full-screen prompt palette: always navy, in both themes. */
object DSReceive {
    val background = Color(0xFF0B1F3A)
    val card = Color(0xFF12294A)
    val divider = Color(0xFF25405F)
    val muted = Color(0xFF9FB4C7)

    /**
     * The Decline outline. design.md's `#3A5573` is about 2.1:1 on navy; this
     * is raised to at least 3:1 on navy and on the card (non-text contrast).
     */
    val outline = Color(0xFF5B7A9C)
    val accent = Color(0xFF3CC4D4)
    val onAccent = Color(0xFF06111F)
}

/** Spacing, radii and touch targets (design.md "Shape, spacing and elevation"). */
object DSMetrics {
    val screenPadding = 20.dp
    val widePadding = 40.dp
    val sectionGap = 24.dp
    val rowPadding = 12.dp
    val cardRadius = 22.dp
    val tileRadius = 12.dp

    /** The smallest touch target; smaller visuals get padded hit areas. */
    val minTouchTarget = 48.dp

    /** Below this width the app uses a bottom navigation bar, otherwise a rail. */
    val wideLayoutMinWidth = 600.dp
}
