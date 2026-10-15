// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi

import android.graphics.Color
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.lifecycle.lifecycleScope
import io.github.dropumi.ui.Confirmations
import io.github.dropumi.ui.DropumiApp
import io.github.dropumi.ui.Router
import kotlinx.coroutines.launch

/**
 * The only activity that draws Dropumi's screens. It is the launcher entry,
 * so any app can start it; it never takes share items from its intent: they
 * come only from the in-process hand-off that ShareActivity writes.
 */
class MainActivity : ComponentActivity() {
    private val graph: AppGraph get() = (application as AppGraphHost).graph

    /** The in-window confirmations (trust and policy changes) on screen, if any. */
    val confirmations = Confirmations()

    private var darkBars: Boolean? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        // The launch screen is navy: light system bar icons until the theme is known.
        applySystemBars(dark = true)
        super.onCreate(savedInstanceState)
        graph.loader.load()
        setContent {
            val router = rememberSaveable(saver = Router.Saver) { Router() }
            DropumiApp(graph, router, confirmations, onDarkTheme = ::applySystemBars)
        }
    }

    override fun onResume() {
        super.onResume()
        // Shared items handed over while the activity was away (singleTask:
        // a share brings this instance back instead of starting a new one).
        val model = graph.loader.model ?: return
        lifecycleScope.launch { model.takeSharedItems() }
    }

    /** Edge to edge, with system bar icons that match the theme the user picked. */
    private fun applySystemBars(dark: Boolean) {
        if (darkBars == dark) return
        darkBars = dark
        val bars = if (dark) {
            SystemBarStyle.dark(Color.TRANSPARENT)
        } else {
            SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
        }
        // Before API 27 the navigation bar's icons cannot be dark: a scrim keeps them readable.
        val navigation = if (dark) SystemBarStyle.dark(DARK_SCRIM) else SystemBarStyle.light(LIGHT_SCRIM, DARK_SCRIM)
        enableEdgeToEdge(statusBarStyle = bars, navigationBarStyle = navigation)
    }

    private companion object {
        // The scrims androidx.activity uses by default.
        val LIGHT_SCRIM = Color.argb(0xe6, 0xFF, 0xFF, 0xFF)
        val DARK_SCRIM = Color.argb(0x80, 0x1b, 0x1b, 0x1b)
    }
}
