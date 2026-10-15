// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.DropumiTheme
import kotlinx.coroutines.launch

/**
 * The only activity that draws Dropumi's screens. It is the launcher entry,
 * so any app can start it; it never takes share items from its intent: they
 * come only from the in-process hand-off that ShareActivity writes.
 */
class MainActivity : ComponentActivity() {
    private val graph: AppGraph get() = (application as AppGraphHost).graph

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        graph.loader.load()
        setContent {
            DropumiTheme(darkTheme = isSystemInDarkTheme()) {
                Shell()
            }
        }
    }

    override fun onResume() {
        super.onResume()
        // Shared items handed over while the activity was away (singleTask:
        // a share brings this instance back instead of starting a new one).
        val model = graph.loader.model ?: return
        lifecycleScope.launch { model.takeSharedItems() }
    }
}

/** The app shell. The screens are not built yet; this says so. */
@Composable
private fun Shell() {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(DS.colors.bg)
            .safeDrawingPadding()
            .padding(DSMetrics.screenPadding),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            painter = painterResource(DSIcons.nearby),
            contentDescription = null,
            tint = DS.colors.accent,
            modifier = Modifier.size(28.dp),
        )
        Text("Dropumi", style = DSType.screenTitle, color = DS.colors.textPrimary)
        Text(
            "Pre-release build. The app's screens are not built yet.",
            style = DSType.body,
            color = DS.colors.textSecondary,
        )
    }
}
