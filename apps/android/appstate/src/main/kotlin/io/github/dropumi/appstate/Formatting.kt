// SPDX-License-Identifier: MPL-2.0

package io.github.dropumi.appstate

import java.util.Locale
import kotlin.math.ceil
import kotlin.math.roundToLong

object Formatting {
    private val UNITS = listOf("KB", "MB", "GB", "TB", "PB", "EB")

    /** File sizes as people read them ("18.2 MB"), in decimal units like the Apple app. */
    fun bytes(n: ULong, locale: Locale = Locale.getDefault()): String {
        if (n < 1000u) return if (n == 1uL) "1 byte" else "$n bytes"
        var value = n.toDouble() / 1000
        var unit = 0
        while (value >= 999.5 && unit < UNITS.lastIndex) {
            value /= 1000
            unit++
        }
        // KB without decimals, MB with one, larger units with two; no trailing zeros.
        val decimals = minOf(unit, 2)
        val text = String.format(locale, "%.${decimals}f", value)
        val separator = String.format(locale, "%.1f", 1.5)[1]
        val trimmed = if (decimals == 0) text else text.trimEnd('0').trimEnd(separator)
        return "$trimmed ${UNITS[unit]}"
    }

    /** "28 MB/s". */
    fun speed(bytesPerSecond: Double, locale: Locale = Locale.getDefault()): String =
        bytes(bytesPerSecond.coerceAtLeast(0.0).roundToLong().toULong(), locale) + "/s"

    /** "20 s left", "3 min left". */
    fun timeLeft(seconds: Double): String {
        val s = maxOf(1L, seconds.roundToLong())
        if (s < 60) return "$s s left"
        val minutes = ceil(s / 60.0).toLong()
        return "$minutes min left"
    }

    /** "1 file", "3 files". */
    fun fileCount(n: Int): String = if (n == 1) "1 file" else "$n files"

    /** "123 456": six digits in two groups, for reading aloud. */
    fun code(digits: String): String =
        if (digits.length == 6) "${digits.take(3)} ${digits.takeLast(3)}" else digits

    /** The code spelled digit by digit for TalkBack: "1 2 3, 4 5 6". */
    fun spokenCode(digits: String): String {
        if (digits.length != 6) return digits
        val first = digits.take(3).toList().joinToString(" ")
        val second = digits.takeLast(3).toList().joinToString(" ")
        return "$first, $second"
    }

    /**
     * A full fingerprint in groups of four ("ABCD-EFGH-…", 13 groups for the 52
     * characters), like the core's short display form. The dashes are only for
     * reading: "Add by fingerprint" and the core ignore them.
     */
    fun fingerprint(full: String): String = full.chunked(4).joinToString("-")

    /** "m:ss" for countdowns. */
    fun countdown(seconds: Long): String {
        val s = maxOf(0L, seconds)
        return "%d:%02d".format(Locale.ROOT, s / 60, s % 60)
    }
}
