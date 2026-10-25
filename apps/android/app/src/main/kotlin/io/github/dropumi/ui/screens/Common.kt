// SPDX-License-Identifier: MPL-2.0
//
// Pieces several screens share: what the screens reach through
// composition locals, browsing while a device list is visible, the
// once-a-second clock for countdowns, the status pill, and the bar of a
// screen opened above a tab.
package io.github.dropumi.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import io.github.dropumi.ReceivedFileActions
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.AppState
import io.github.dropumi.ui.Confirmations
import io.github.dropumi.ui.Router
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.IconAction
import io.github.dropumi.ui.design.StatusPill
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay

/** The app model. */
val LocalModel = staticCompositionLocalOf<AppModel> { error("no AppModel") }

/** Where the user is. */
val LocalRouter = staticCompositionLocalOf<Router> { error("no Router") }

/** The in-window confirmation panel. */
val LocalConfirmations = staticCompositionLocalOf<Confirmations> { error("no Confirmations") }

/** Open, Share and the Save folder picker for received files. */
val LocalFileActions = staticCompositionLocalOf<ReceivedFileActions> { error("no ReceivedFileActions") }

/**
 * The app's long-lived scope (the main thread). Model calls started from a
 * screen run here, so leaving the screen does not cancel a send or a
 * setting change half-way.
 */
val LocalAppScope = staticCompositionLocalOf<CoroutineScope> { error("no app scope") }

/**
 * Looks for nearby devices while the calling screen is on screen and the
 * activity is started; stops when either ends (AppModel counts the lists).
 */
@Composable
fun BrowseWhileVisible(model: AppModel) {
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner, model) {
        var browsing = false
        fun begin() {
            if (!browsing) {
                browsing = true
                model.beginBrowsing()
            }
        }
        fun end() {
            if (browsing) {
                browsing = false
                model.endBrowsing()
            }
        }
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_START -> begin()
                Lifecycle.Event.ON_STOP -> end()
                else -> Unit
            }
        }
        if (owner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) begin()
        owner.lifecycle.addObserver(observer)
        onDispose {
            owner.lifecycle.removeObserver(observer)
            end()
        }
    }
}

/**
 * The wall clock in milliseconds, updated once a second while [ticking] is
 * true (for countdowns); otherwise read once.
 */
@Composable
fun rememberNowMs(ticking: Boolean): Long {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(ticking) {
        now = System.currentTimeMillis()
        while (ticking) {
            delay(1_000 - now % 1_000)
            now = System.currentTimeMillis()
        }
    }
    return now
}

/** Who may send to this device right now. */
@Composable
fun VisibilityPill(state: AppState, modifier: Modifier = Modifier) {
    val now = rememberNowMs(ticking = state.receiver?.everyoneUntil != null)
    val ready = ScreenText.readyToPair(state)
    StatusPill(
        ScreenText.visibilityLabel(state, now),
        active = state.receiver != null || ready,
        // Ready to pair: the accent dot, as on the pairing screen.
        dot = if (ready) DS.colors.accent else null,
        modifier = modifier.testTag("visibility.pill"),
    )
}

/** A screen opened above a tab: a Back button, then the content. */
@Composable
fun PushedScreen(onBack: () -> Unit, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize()) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.Start,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
        ) {
            IconAction(DSIcons.back, "Back", onBack, modifier = Modifier.testTag("screen.back"))
        }
        content()
    }
}
