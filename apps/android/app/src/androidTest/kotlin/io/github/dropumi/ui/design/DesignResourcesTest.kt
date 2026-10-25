// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui.design

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.core.content.res.ResourcesCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The bundled design resources load on the device: every Lucide icon and the font. */
@RunWith(AndroidJUnit4::class)
class DesignResourcesTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val sample = "Dropumi shares files nearby"

    @Test
    fun everyLucideIconInflatesAndDraws() {
        val icons = R.drawable::class.java.fields.filter { it.name.startsWith("lucide_") }
        assertEquals(33, icons.size)
        val bitmap = Bitmap.createBitmap(48, 48, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        for (field in icons) {
            val drawable = ContextCompat.getDrawable(context, field.getInt(null))
            assertNotNull(field.name, drawable)
            drawable!!.setBounds(0, 0, 48, 48)
            bitmap.eraseColor(0)
            drawable.draw(canvas)
            val drawn = IntArray(48 * 48).also { bitmap.getPixels(it, 0, 48, 0, 0, 48, 48) }.count { it != 0 }
            assertTrue("${field.name} drew nothing", drawn > 0)
        }
    }

    /** The bundled font is variable: its `wght` axis takes 400 to 700, and bolder text is wider. */
    @Test
    fun bundledFontHasAWeightAxis() {
        val typeface = ResourcesCompat.getFont(context, R.font.plus_jakarta_sans)
        assertNotNull(typeface)
        val widths = listOf(400, 500, 600, 700).map { weight ->
            val paint = Paint().apply {
                this.typeface = typeface
                textSize = 40f
            }
            assertTrue("wght $weight", paint.setFontVariationSettings("'wght' $weight"))
            paint.measureText(sample)
        }
        assertEquals(widths.sorted(), widths)
        assertTrue("700 is not wider than 400: $widths", widths.last() > widths.first())
    }

    /**
     * The Compose path the screens use: `PlusJakartaSans` (Typography.kt)
     * resolved and laid out by Compose. Each weight must come from the font's
     * `wght` axis; without the per-weight variation settings every weight
     * renders the default instance, and these widths would all be equal.
     */
    @Test
    fun composeFontFamilyDrawsEachWeightFromTheAxis() {
        val measurer = TextMeasurer(
            defaultFontFamilyResolver = createFontFamilyResolver(context),
            defaultDensity = Density(context),
            defaultLayoutDirection = LayoutDirection.Ltr,
        )
        fun width(style: TextStyle) = measurer.measure(sample, style.copy(fontSize = 40.sp)).size.width

        val weights = listOf(FontWeight.Normal, FontWeight.Medium, FontWeight.SemiBold, FontWeight.Bold)
        val widths = weights.map { width(TextStyle(fontFamily = PlusJakartaSans, fontWeight = it)) }
        assertTrue("each weight must be wider than the one before: $widths", widths.zipWithNext().all { (a, b) -> b > a })

        // The type roles carry the family and weight: at one size, the 700
        // screen title is wider than the 400 body text.
        val title = width(DSType.screenTitle)
        val body = width(DSType.body)
        assertTrue("screenTitle $title is not wider than body $body", title > body)
    }
}
