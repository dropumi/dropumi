// SPDX-License-Identifier: MPL-2.0
//
// Shell commands for instrumented tests (run as the shell user through
// UiAutomation), and device-wide display and font settings that a test
// changes and must put back. scripts/android/ui-tests.sh also puts them
// back in its exit trap, in case a run dies before a test's close().

package io.github.dropumi

import android.content.ComponentCallbacks
import android.content.res.Configuration
import android.os.ParcelFileDescriptor
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Runs [command] as the shell user and returns what it printed, once it has ended. */
fun shell(command: String): String {
    val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    return ParcelFileDescriptor.AutoCloseInputStream(pfd).bufferedReader().use { it.readText() }
}

private fun appConfiguration(): Configuration =
    InstrumentationRegistry.getInstrumentation().targetContext.applicationContext.resources.configuration

/**
 * Runs [change] and waits (at most 15 s) for the configuration change it
 * causes to reach the app: event-driven, through the Application's
 * ComponentCallbacks, never a fixed sleep.
 */
private fun changeConfiguration(what: String, change: () -> Unit, reached: (Configuration) -> Boolean) {
    val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
    val latch = CountDownLatch(1)
    val callbacks = object : ComponentCallbacks {
        override fun onConfigurationChanged(newConfig: Configuration) {
            if (reached(newConfig)) latch.countDown()
        }

        @Deprecated("Deprecated in Java")
        override fun onLowMemory() = Unit
    }
    app.registerComponentCallbacks(callbacks)
    try {
        change()
        if (!reached(appConfiguration()) && !latch.await(CONFIGURATION_TIMEOUT_S, TimeUnit.SECONDS)) {
            throw AssertionError("timed out waiting for $what")
        }
    } finally {
        app.unregisterComponentCallbacks(callbacks)
    }
}

private const val CONFIGURATION_TIMEOUT_S = 15L

/**
 * A display size and density for this test only (`wm size`, `wm density`),
 * so a layout can be checked on any device. [close] puts back what was set
 * before (an earlier override, or none).
 */
class DisplayOverride private constructor(
    private val size: String?,
    private val density: String?,
    private val original: Configuration,
) : AutoCloseable {
    override fun close() {
        changeConfiguration("the display to return", {
            shell(if (size != null) "wm size $size" else "wm size reset")
            shell(if (density != null) "wm density $density" else "wm density reset")
        }) { it.densityDpi == original.densityDpi && it.screenWidthDp == original.screenWidthDp }
    }

    companion object {
        /**
         * A square display of [sidePx] pixels at [dpi], so its width in dp is
         * about the same in every rotation (a side system bar can take up to
         * [SYSTEM_BAR_DP] of it). Returns once the app's configuration has
         * the new density and width.
         */
        fun square(sidePx: Int, dpi: Int): DisplayOverride {
            val override = DisplayOverride(
                override(shell("wm size"), "Override size:"),
                override(shell("wm density"), "Override density:"),
                Configuration(appConfiguration()),
            )
            val widthDp = sidePx * DENSITY_DEFAULT / dpi
            changeConfiguration("a display ${widthDp}dp wide", {
                shell("wm size ${sidePx}x$sidePx")
                shell("wm density $dpi")
            }) { it.densityDpi == dpi && it.screenWidthDp in (widthDp - SYSTEM_BAR_DP)..widthDp }
            return override
        }

        private const val DENSITY_DEFAULT = 160
        private const val SYSTEM_BAR_DP = 80

        private fun override(output: String, label: String): String? =
            output.lines().firstOrNull { it.startsWith(label) }?.substringAfter(label)?.trim()?.takeIf { it.isNotEmpty() }
    }
}

/**
 * The system font scale for this test only (`settings put system
 * font_scale`). [close] puts back the earlier value (or none).
 */
class FontScaleOverride private constructor(private val original: String?) : AutoCloseable {
    override fun close() {
        val back = original?.toFloatOrNull() ?: 1f
        changeConfiguration("the font scale to return", {
            shell(if (original != null) "settings put system font_scale $original" else "settings delete system font_scale")
        }) { it.fontScale == back }
    }

    companion object {
        /** Sets [scale] and returns once the app's configuration has it. */
        fun set(scale: Float): FontScaleOverride {
            val original = shell("settings get system font_scale").trim().takeUnless { it.isEmpty() || it == "null" }
            changeConfiguration("font scale $scale", { shell("settings put system font_scale $scale") }) { it.fontScale == scale }
            return FontScaleOverride(original)
        }
    }
}
