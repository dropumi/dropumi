// SPDX-License-Identifier: MPL-2.0
//
// The Dropumi logo, drawn from the artwork in branding/final/ (the path data
// of dropumi-symbol-reversed.svg and dropumi-wordmark.svg, unchanged; the
// circles written as arcs). Colours follow branding/GUIDELINES.md: navy and
// Umi Teal on light backgrounds, white and Light Teal on dark ones.
package io.github.dropumi.ui.design

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Logo colours (branding/GUIDELINES.md). */
private object BrandColors {
    val navy = Color(0xFF0B1F3A)
    val umiTeal = Color(0xFF0E94A6)
    val lightTeal = Color(0xFF3CC4D4)
    val white = Color(0xFFFFFFFF)
}

private const val DROP = "M128 46 C128 46 98 91.1 98 124 A30 30 0 0 0 158 124 C158 91.1 128 46 128 46 Z"
private val RIPPLES = listOf("M169.6 86.5 A56 56 0 1 1 86.4 86.5", "M193.4 65.1 A88 88 0 1 1 62.6 65.1")

private val WORD_DROP = listOf(
    circle(52f, -48f, 44f),
    "M96 -132 V0",
    "M138 -92 V0",
    "M138 -44 A48 48 0 0 1 186 -92",
    circle(260f, -48f, 44f),
    circle(390f, -48f, 44f),
    "M346 -92 V48",
)
private val WORD_UMI = listOf(
    "M476 -92 V-48 A44 44 0 0 0 564 -48",
    "M564 -92 V0",
    "M606 -92 V0",
    "M606 -56 A36 36 0 0 1 678 -56 V0",
    "M678 -56 A36 36 0 0 1 750 -56 V0",
    "M792 -92 V0",
)
private val WORD_DOT = circle(792f, -130f, 12f)

private fun circle(cx: Float, cy: Float, r: Float): String =
    "M${cx - r} $cy A$r $r 0 1 0 ${cx + r} $cy A$r $r 0 1 0 ${cx - r} $cy Z"

/** The symbol for dark backgrounds (a white drop, Light Teal ripples), as on the navy disc. */
private val symbolReversed: ImageVector = ImageVector.Builder(
    name = "dropumi-symbol-reversed",
    defaultWidth = 256.dp,
    defaultHeight = 256.dp,
    viewportWidth = 256f,
    viewportHeight = 256f,
).apply {
    group(translationY = -3f) {
        addPath(addPathNodes(DROP), fill = SolidColor(BrandColors.white))
        for (d in RIPPLES) {
            addPath(
                addPathNodes(d),
                fill = null,
                stroke = SolidColor(BrandColors.lightTeal),
                strokeLineWidth = 13f,
                strokeLineCap = StrokeCap.Round,
            )
        }
    }
}.build()

private fun wordmark(drop: Color, umi: Color): ImageVector = ImageVector.Builder(
    name = "dropumi-wordmark",
    defaultWidth = 816.dp,
    defaultHeight = 200.dp,
    viewportWidth = 816f,
    viewportHeight = 200f,
).apply {
    group(translationX = 8f, translationY = 142f) {
        for (d in WORD_DROP) {
            addPath(addPathNodes(d), fill = null, stroke = SolidColor(drop), strokeLineWidth = 16f, strokeLineCap = StrokeCap.Round)
        }
        for (d in WORD_UMI) {
            addPath(addPathNodes(d), fill = null, stroke = SolidColor(umi), strokeLineWidth = 16f, strokeLineCap = StrokeCap.Round)
        }
        addPath(addPathNodes(WORD_DOT), fill = SolidColor(umi))
    }
}.build()

private val wordmarkLight = wordmark(BrandColors.navy, BrandColors.umiTeal)
private val wordmarkDark = wordmark(BrandColors.white, BrandColors.lightTeal)

/** The reversed symbol, for the navy disc. Decorative: it has no accessibility label. */
@Composable
fun BrandSymbol(modifier: Modifier = Modifier) {
    Image(imageVector = symbolReversed, contentDescription = null, modifier = modifier.aspectRatio(1f))
}

/** The wordmark at [height], in the colours for the current theme. TalkBack reads "Dropumi". */
@Composable
fun Wordmark(height: Dp, modifier: Modifier = Modifier) {
    val dark = DS.colors.isDark
    val image = remember(dark) { if (dark) wordmarkDark else wordmarkLight }
    Image(
        imageVector = image,
        contentDescription = null,
        modifier = modifier
            .height(height)
            .aspectRatio(816f / 200f)
            .semantics { contentDescription = "Dropumi" },
    )
}
