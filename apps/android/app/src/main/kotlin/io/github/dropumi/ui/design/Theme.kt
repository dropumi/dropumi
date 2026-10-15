// SPDX-License-Identifier: MPL-2.0
//
// The app theme: the design.md tokens mapped onto Material 3, whose
// components the screens use. Dynamic colour is off on every API level.
package io.github.dropumi.ui.design

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

private val LocalDSColors = staticCompositionLocalOf { DSPalette.Light }

/** Access to the current theme's tokens inside composition. */
object DS {
    val colors: DSColors
        @Composable
        @ReadOnlyComposable
        get() = LocalDSColors.current
}

/** Wraps the app's content in the Dropumi theme. */
@Composable
fun DropumiTheme(darkTheme: Boolean, content: @Composable () -> Unit) {
    val colors = if (darkTheme) DSPalette.Dark else DSPalette.Light
    CompositionLocalProvider(LocalDSColors provides colors) {
        MaterialTheme(
            colorScheme = colorScheme(colors),
            typography = typography,
            content = content,
        )
    }
}

private fun colorScheme(c: DSColors): ColorScheme {
    val base = if (c.isDark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = c.accent,
        onPrimary = c.onAccent,
        primaryContainer = c.accentSoft,
        onPrimaryContainer = c.accentText,
        secondary = c.accent,
        onSecondary = c.onAccent,
        secondaryContainer = c.accentSoft,
        onSecondaryContainer = c.accentText,
        tertiary = c.success,
        onTertiary = c.onAccent,
        background = c.bg,
        onBackground = c.textPrimary,
        surface = c.surface,
        onSurface = c.textPrimary,
        surfaceVariant = c.surface2,
        onSurfaceVariant = c.textSecondary,
        surfaceTint = Color.Transparent,
        surfaceBright = c.surface,
        surfaceDim = c.surface2,
        surfaceContainerLowest = c.surface,
        surfaceContainerLow = c.surface,
        surfaceContainer = c.surface,
        surfaceContainerHigh = c.surface2,
        surfaceContainerHighest = c.surface2,
        outline = c.border,
        outlineVariant = c.border,
        error = c.danger,
        onError = c.onAccent,
    )
}

private val typography = Typography(
    displaySmall = DSType.screenTitle,
    headlineLarge = DSType.screenTitle,
    headlineMedium = DSType.dialogName,
    headlineSmall = DSType.sectionHeading,
    titleLarge = DSType.sectionHeading,
    titleMedium = DSType.rowTitle,
    titleSmall = DSType.smallButton,
    bodyLarge = DSType.body,
    bodyMedium = DSType.body,
    bodySmall = DSType.meta,
    labelLarge = DSType.button,
    labelMedium = DSType.smallButton,
    labelSmall = DSType.tabLabel,
)
