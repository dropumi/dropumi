// SPDX-License-Identifier: MPL-2.0
//
// Plus Jakarta Sans (SIL Open Font License 1.1, assets/licenses/OFL.txt),
// bundled as one variable font so no font is ever downloaded. Each weight
// sets the font's `wght` axis explicitly: Compose's stable resource-font API
// passes no variation settings, so every weight would render the font's
// default instance. Sizes are in sp, so they follow the system
// font size. Codes and fingerprints use the system monospace font so digits
// are easy to compare.
package io.github.dropumi.ui.design

import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import io.github.dropumi.R

/** The bundled variable font at the four weights the design uses. */
val PlusJakartaSans = FontFamily(
    jakarta(FontWeight.Normal),
    jakarta(FontWeight.Medium),
    jakarta(FontWeight.SemiBold),
    jakarta(FontWeight.Bold),
)

@OptIn(ExperimentalTextApi::class)
private fun jakarta(weight: FontWeight) = Font(
    resId = R.font.plus_jakarta_sans,
    weight = weight,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
)

/** design.md type roles. */
object DSType {
    private fun style(size: Int, weight: FontWeight) =
        TextStyle(fontFamily = PlusJakartaSans, fontSize = size.sp, fontWeight = weight)

    /** 32 / 700: the title of every screen. */
    val screenTitle = style(32, FontWeight.Bold)

    /** 28 / 700: the sender's name on the prompt. */
    val dialogName = style(28, FontWeight.Bold)

    /** 18 / 600. */
    val sectionHeading = style(18, FontWeight.SemiBold)

    /** 16 / 600: file and device names. */
    val rowTitle = style(16, FontWeight.SemiBold)

    /** 15 / 400. */
    val body = style(15, FontWeight.Normal)

    /** 13 / 400: sizes, devices, times. */
    val meta = style(13, FontWeight.Normal)

    /** 16 / 600: primary buttons. */
    val button = style(16, FontWeight.SemiBold)

    /** 13 / 600: small buttons. */
    val smallButton = style(13, FontWeight.SemiBold)

    /** 12 / 700; callers upper-case the text. */
    val sectionLabel = style(12, FontWeight.Bold)

    /** 10 / 500: navigation labels. */
    val tabLabel = style(10, FontWeight.Medium)

    /** A large monospace verification code ("123 456"). */
    val code = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 34.sp, fontWeight = FontWeight.Bold)

    /** Fingerprints. */
    val fingerprint = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 13.sp)
}
