// SPDX-License-Identifier: MPL-2.0
//
// Confirmations for trust and policy changes (ADR-0013 D8; plan §7.2 item
// 5): Who can send → Everyone, Auto-accept paired devices → on, Add by
// fingerprint, Remove a paired device, Reset identity, and Delete of
// received files. Each is a panel drawn inside MainActivity's own window,
// never a Dialog, AlertDialog or bottom sheet (each of those is a separate
// window that the activity's touch filtering cannot see). While one shows,
// MainActivity protects it from overlays (TouchGuard): an obscured gesture
// is dropped whole, with "Close it to answer", and on API 31+ other apps'
// overlay windows are hidden.
//
// The panel has no default action: nothing is focused when it opens, and
// Enter, NumPad Enter, DPAD centre and Space act only on a button the user
// moved focus to with Tab or an arrow (MainActivity, KeyGate), never on
// focus Android gave a button by itself. Back, Escape or a tap outside the
// card cancels it.
package io.github.dropumi.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusTarget
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTag
import androidx.compose.ui.unit.dp
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.OutlineButton
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.largeText

/** One question. [onConfirm] runs once, after the panel has closed. */
data class Confirmation(
    val title: String,
    val message: String,
    val confirmLabel: String,
    val destructive: Boolean = false,
    val onConfirm: () -> Unit,
)

@Stable
class Confirmations {
    var current: Confirmation? by mutableStateOf(null)
        private set

    /** Whether a confirmation is on screen (the activity protects it from overlays). */
    val showing: Boolean get() = current != null

    fun ask(confirmation: Confirmation) {
        current = confirmation
    }

    fun cancel() {
        current = null
    }

    /** Confirms the confirmation on screen, if it is still [which]. */
    fun confirm(which: Confirmation) {
        if (current !== which) return
        current = null
        which.onConfirm()
    }
}

/**
 * The panel, drawn over the screens while a confirmation shows. [backEnabled]
 * is off while a prompt is drawn over the panel: Back is the prompt's then.
 */
@Composable
fun ConfirmationPanel(confirmations: Confirmations, backEnabled: Boolean = true, focus: FocusRequester? = null) {
    val confirmation = confirmations.current ?: return
    val c = DS.colors
    val guard = LocalTouchGuard.current
    val gate = LocalKeyGate.current
    val surface = KeySurface.Panel(confirmation)
    BackHandler(enabled = backEnabled) { confirmations.cancel() }
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .fillMaxSize()
            .background(if (c.isDark) Color.Black.copy(alpha = 0.6f) else c.navy.copy(alpha = 0.55f))
            // Taps outside the card cancel; nothing reaches the screen below.
            .pointerInput(confirmation) {
                awaitPointerEventScope {
                    while (true) {
                        val event = awaitPointerEvent()
                        // The card consumes its own taps; only a tap that ends outside it cancels.
                        val onCard = event.changes.any { it.isConsumed }
                        event.changes.forEach { it.consume() }
                        val released = event.changes.all { !it.pressed } && event.changes.any { it.previousPressed }
                        if (released && !onCard) confirmations.cancel()
                    }
                }
            }
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .padding(DSMetrics.screenPadding),
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(16.dp),
            modifier = Modifier
                .widthIn(max = 460.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(DSMetrics.cardRadius))
                .background(c.surface)
                .border(1.dp, c.border, RoundedCornerShape(DSMetrics.cardRadius))
                // The card itself takes its taps, so they do not cancel.
                .pointerInput(confirmation) {
                    awaitPointerEventScope {
                        while (true) awaitPointerEvent().changes.forEach { it.consume() }
                    }
                }
                .verticalScroll(rememberScrollState())
                // Where keyboard focus lands when it would enter the screens
                // below; not a button, so Enter does nothing there.
                .then(if (focus != null) Modifier.focusRequester(focus).focusTarget() else Modifier)
                .padding(20.dp)
                .semantics { paneTitle = confirmation.title; testTag = "confirmation" },
        ) {
            Text(confirmation.title, style = DSType.sectionHeading, color = c.textPrimary, modifier = Modifier.semantics { heading() })
            Text(confirmation.message, style = DSType.body, color = c.textSecondary)
            if (guard.notice == OverlayNotice.ANSWER) {
                OverlayNoticeText(guard, OverlayNotice.ANSWER, color = c.danger, modifier = Modifier.fillMaxWidth())
            }
            val confirm: @Composable (Modifier) -> Unit = { m ->
                if (confirmation.destructive) {
                    OutlineButton(
                        confirmation.confirmLabel,
                        { confirmations.confirm(confirmation) },
                        modifier = m.gatedButton(gate, surface, CONFIRM).semantics { testTag = CONFIRM },
                        color = c.danger,
                        outline = c.danger,
                    )
                } else {
                    PrimaryButton(
                        confirmation.confirmLabel,
                        { confirmations.confirm(confirmation) },
                        modifier = m.gatedButton(gate, surface, CONFIRM).semantics { testTag = CONFIRM },
                    )
                }
            }
            val cancel: @Composable (Modifier) -> Unit = { m ->
                OutlineButton(
                    "Cancel",
                    { confirmations.cancel() },
                    modifier = m.gatedButton(gate, surface, CANCEL).semantics { testTag = CANCEL },
                )
            }
            if (largeText()) {
                cancel(Modifier)
                confirm(Modifier)
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    cancel(Modifier.weight(1f))
                    confirm(Modifier.weight(1f))
                }
            }
        }
    }
}

private const val CONFIRM = "confirmation.confirm"
private const val CANCEL = "confirmation.cancel"
