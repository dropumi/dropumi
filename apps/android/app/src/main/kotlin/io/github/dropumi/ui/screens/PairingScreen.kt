// SPDX-License-Identifier: MPL-2.0
//
// Pair a device (a port of the Apple app's PairingScreen): compare a
// six-digit code once; afterwards the devices know each other's key
// (ADR-0004). Both people open this screen, which makes each device ready
// to pair (the model opens a five-minute window, with a countdown and
// Stop), and one of them taps the other device. The code itself appears as
// the prompt (PromptScreen, unchanged); trust is stored only if both users
// confirm, and "Paired" shows only on PairingFinished(Paired). Names in the
// list are claims: shown verbatim, in quotes, and a row never says
// "Paired". Where this device is not listed nearby (ADR-0013 D5), the
// ready card shows the addresses to type instead.
//
// The screen tells the model when it is shown and when it goes: Back,
// Done and Stop leave it (the window closes); another tab only hides it (a
// result stays for the return); a rotation recreates the activity, and the
// same window is kept.
package io.github.dropumi.ui.screens

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.PairingFlow
import io.github.dropumi.appstate.PairingOutcome
import io.github.dropumi.appstate.PairingSession
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ui.Router
import io.github.dropumi.ui.Screen
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSCard
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSRow
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.DeviceAvatar
import io.github.dropumi.ui.design.EmptyState
import io.github.dropumi.ui.design.LinkButton
import io.github.dropumi.ui.design.OutlineButton
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionHeading
import io.github.dropumi.ui.design.SoftPill
import io.github.dropumi.ui.design.largeText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@Composable
fun PairingScreen(state: AppState) {
    val model = LocalModel.current
    val router = LocalRouter.current
    ReadyWhileShown(model, router)
    BrowseWhileVisible(model)
    ScreenColumn {
        ScreenHeader("Pair a device", "Open this screen on both devices. Then tap the other device on one of them.")
        Content(state)
    }
}

/**
 * Tells the model that the screen is shown, and how it went: a recreation
 * of the activity (it comes straight back), hidden by another tab (its
 * route is still in that tab's stack), or left.
 */
@Composable
private fun ReadyWhileShown(model: AppModel, router: Router) {
    val activity = LocalContext.current.findActivity()
    // The tab this screen lives in: a tab switch leaves it in that tab's stack.
    val tab = remember { router.tab }
    DisposableEffect(model) {
        model.enterPairingScreen()
        onDispose {
            when {
                activity?.isChangingConfigurations == true -> model.pairingScreenRecreating()
                Screen.Pairing in router.stack(tab) -> model.leavePairingScreen(hidden = true)
                else -> model.leavePairingScreen()
            }
        }
    }
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}

@Composable
private fun Content(state: AppState) {
    val model = LocalModel.current
    val router = LocalRouter.current
    val flow = state.pairingFlow
    // Leaving the screen through Done and Stop: the model first, then Back.
    val done = {
        model.dismissPairing()
        router.pop()
        Unit
    }
    val stop = {
        model.stopPairing()
        router.pop()
        Unit
    }
    when (val phase = flow.phase) {
        PairingFlow.Phase.Idle, PairingFlow.Phase.Opening, is PairingFlow.Phase.Waiting -> Columns(
            left = {
                ReadyCard(state, (phase as? PairingFlow.Phase.Waiting)?.expiresAtMs, stop)
                PairingSteps()
            },
            right = {
                flow.note?.let { PairingNote(it.noteText ?: it.text.message) }
                PairingDeviceList(state)
            },
        )
        is PairingFlow.Phase.Switching -> StatusCard("Asking “${phase.name}”…", ASKING_LINE) { model.cancelPairing() }
        is PairingFlow.Phase.Asking -> StatusCard("Asking “${phase.name}”…", ASKING_LINE) { model.cancelPairing() }
        PairingFlow.Phase.Comparing -> StatusCard("Compare the code", "Check that both screens show the same code.", cancel = null)
        is PairingFlow.Phase.Paused ->
            if (phase.outcome is PairingOutcome.CouldNotWait) {
                // This device can still ask one in the list.
                Columns(
                    left = { ResultCard(phase.outcome, paused = true, done) { model.startWaiting() } },
                    right = { PairingDeviceList(state) },
                )
            } else {
                ResultCard(phase.outcome, paused = true, done) { model.startWaiting() }
            }
        is PairingFlow.Phase.Finished -> ResultCard(phase.outcome, paused = false, done) { model.startWaiting() }
    }
}

private const val ASKING_LINE = "A code appears on both devices. Check that they match."

/**
 * Two columns on a wide window (the ready card and the steps, then the
 * list); one column on phones and at large text (font scale 1.3 and up).
 */
@Composable
private fun Columns(left: @Composable () -> Unit, right: @Composable () -> Unit) {
    val large = largeText()
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        if (maxWidth >= DSMetrics.wideLayoutMinWidth && !large) {
            Row(horizontalArrangement = Arrangement.spacedBy(40.dp)) {
                Column(verticalArrangement = Arrangement.spacedBy(DSMetrics.sectionGap), modifier = Modifier.widthIn(max = 320.dp).weight(1f)) {
                    left()
                }
                Column(verticalArrangement = Arrangement.spacedBy(DSMetrics.sectionGap), modifier = Modifier.weight(1f)) {
                    right()
                }
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(DSMetrics.sectionGap)) {
                left()
                right()
            }
        }
    }
}

/**
 * This device is getting ready to pair, or is ready: the time left, the
 * name other devices see, and the addresses for devices that don't list it.
 * [expiresAtMs] is null while getting ready.
 */
@Composable
private fun ReadyCard(state: AppState, expiresAtMs: Long?, stop: () -> Unit) {
    val c = DS.colors
    DSCard(padding = 20.dp, modifier = Modifier.testTag("pair.ready")) {
        Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.Top) {
            DeviceAvatar(icon = DSIcons.pair)
            Column(verticalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.weight(1f)) {
                if (expiresAtMs != null) {
                    Countdown(expiresAtMs)
                    state.identity?.name?.let { name ->
                        Text(
                            "Other devices see this one as “$name”.",
                            style = DSType.meta,
                            color = c.textSecondary,
                            modifier = Modifier.testTag("pair.name"),
                        )
                    }
                } else {
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Spinner(18.dp)
                        Text("Getting ready to pair…", style = DSType.rowTitle, color = c.textPrimary)
                    }
                }
            }
        }
        if (expiresAtMs != null) {
            if (state.visibility != ReceiveVisibility.OFF) {
                Caption("Receiving files is paused until you leave this screen.")
            }
            AddressLines(state)
        }
        Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxWidth()) {
            OutlineButton("Stop", stop, modifier = Modifier.widthIn(max = 260.dp).testTag("pair.stop"))
        }
    }
}

/**
 * "Ready to pair · 4:59". TalkBack reads it as one element, "Ready to
 * pair", whose value is in whole minutes, so it changes once a minute and
 * not every second.
 */
@Composable
private fun Countdown(expiresAtMs: Long) {
    // The model's clock is the wall clock in the app.
    val now = rememberNowMs(ticking = true)
    val left = maxOf(0L, (expiresAtMs - now) / 1000)
    val minutes = minutesLeft(left)
    Text(
        "Ready to pair · ${Formatting.countdown(left)}",
        style = DSType.rowTitle.copy(fontFeatureSettings = "tnum"),
        color = DS.colors.textPrimary,
        modifier = Modifier.clearAndSetSemantics {
            contentDescription = "Ready to pair"
            stateDescription = minutes
            testTag = "pair.countdown"
        },
    )
}

/** Whole minutes left, as TalkBack says them. */
internal fun minutesLeft(seconds: Long): String {
    val minutes = seconds / 60
    return when {
        minutes <= 0 -> "Less than a minute left"
        minutes == 1L -> "1 minute left"
        else -> "$minutes minutes left"
    }
}

/** The addresses another device can type to pair with this one: the window's port on this device's local networks. */
@Composable
private fun AddressLines(state: AppState) {
    val model = LocalModel.current
    val c = DS.colors
    val port = state.pairing?.takeIf { it.role == PairingSession.Role.Window && it.result == null }?.port
    // Reading the network's addresses asks the system: off the main thread.
    val addresses by produceState(emptyList<String>(), port) {
        value = withContext(Dispatchers.IO) { model.pairingAddresses() }
    }
    if (addresses.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Caption("Not listed on the other device? There, choose Pair by address and type:")
        SelectionContainer {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                for (address in addresses) {
                    val host = address.substringBeforeLast(':')
                    val portText = address.substringAfterLast(':')
                    Text(
                        address,
                        style = DSType.fingerprint.copy(fontSize = DSType.body.fontSize),
                        color = c.textPrimary,
                        modifier = Modifier
                            .semantics { contentDescription = "Address $host, port $portText" }
                            .testTag("pair.address"),
                    )
                }
            }
        }
    }
}

/** "How to pair": three numbered steps, each read as one sentence. */
@Composable
private fun PairingSteps() {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.testTag("pair.steps")) {
        SectionHeading("How to pair")
        Step(1, "Open Pair a device on the other device too.")
        Step(2, "On one of them, tap Pair next to the other.")
        Step(3, "Check that both screens show the same code. Then tap Codes match on both.")
    }
}

@Composable
private fun Step(number: Int, text: String) {
    val c = DS.colors
    // The badge grows with the text size.
    val badge = 26.dp * LocalDensity.current.fontScale
    Row(
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.Top,
        modifier = Modifier.clearAndSetSemantics { contentDescription = "$number. $text" },
    ) {
        Box(contentAlignment = Alignment.Center, modifier = Modifier.size(badge).clip(CircleShape).background(c.accentSoft)) {
            Text("$number", style = DSType.smallButton, color = c.accentText)
        }
        Text(text, style = DSType.body, color = c.textPrimary, modifier = Modifier.weight(1f))
    }
}

/** What happened to the last request, while this device is ready to pair again. */
@Composable
private fun PairingNote(text: String) {
    val c = DS.colors
    val shape = RoundedCornerShape(18.dp)
    Row(
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.border, shape)
            .padding(16.dp)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }
            .testTag("pair.note"),
    ) {
        Icon(painterResource(DSIcons.info), contentDescription = null, tint = c.accent, modifier = Modifier.size(20.dp))
        Text(text, style = DSType.body, color = c.textPrimary, modifier = Modifier.weight(1f))
    }
}

/** Nearby devices, each with Pair, and the way to pair by address. */
@Composable
private fun PairingDeviceList(state: AppState) {
    val model = LocalModel.current
    val router = LocalRouter.current
    // Not while this device is asking or comparing a code (the list is hidden then too).
    val enabled = when (state.pairingFlow.phase) {
        is PairingFlow.Phase.Switching, is PairingFlow.Phase.Asking, PairingFlow.Phase.Comparing -> false
        else -> true
    }
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SectionHeading("Nearby devices", modifier = Modifier.testTag("pair.list"))
        if (state.nearby.isEmpty()) {
            EmptyState(
                DSIcons.nearby,
                "Looking for devices…",
                "Open Pair a device on the other device. Both need to be on the same Wi-Fi network.",
                modifier = Modifier.testTag("pair.empty"),
            )
        }
        for (peer in state.nearby) {
            DSRow(
                title = peer.name,
                subtitle = ScreenText.nearbySubtitle(peer),
                leading = { DeviceAvatar() },
                modifier = Modifier.testTag("pair.row"),
            ) {
                SoftPill(
                    "Pair",
                    { model.startPairing(SendTarget.Nearby(peer.instance), peer.name) },
                    enabled = enabled && peer.compatible,
                    modifier = Modifier.semantics { contentDescription = "Pair with ${peer.name}" }.testTag("pair.peer"),
                )
            }
        }
        Column(modifier = Modifier.padding(top = 8.dp)) {
            LinkButton("Not listed? Pair by address", { router.openPairByAddress() }, modifier = Modifier.testTag("pair.byAddress"))
            // Android: other devices may not list this one (ADR-0013 D5).
            Caption("Other devices may not list this phone. On them, choose Pair by address and type an address shown here.")
        }
    }
}

/** Asking a device, or comparing the code. */
@Composable
private fun StatusCard(title: String, line: String, cancel: (() -> Unit)?) {
    val c = DS.colors
    DSCard(padding = 20.dp, modifier = Modifier.testTag("pair.status")) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Spinner(28.dp)
            Text(
                title,
                style = DSType.sectionHeading,
                color = c.textPrimary,
                textAlign = TextAlign.Center,
                modifier = Modifier.semantics {
                    heading()
                    liveRegion = LiveRegionMode.Polite
                },
            )
            Text(line, style = DSType.body, color = c.textSecondary, textAlign = TextAlign.Center)
            if (cancel != null) {
                OutlineButton("Cancel", cancel, modifier = Modifier.widthIn(max = 260.dp).testTag("pair.cancel"))
            }
        }
    }
}

/** A spinner TalkBack does not read. */
@Composable
private fun Spinner(size: androidx.compose.ui.unit.Dp) {
    CircularProgressIndicator(
        color = DS.colors.accent,
        strokeWidth = 2.5.dp,
        modifier = Modifier.size(size).clearAndSetSemantics {},
    )
}

/**
 * How an attempt ended (Done and Try again), or why this device is not
 * ready to pair (one button). Its title is a heading that TalkBack
 * announces when it appears.
 */
@Composable
private fun ResultCard(outcome: PairingOutcome, paused: Boolean, done: () -> Unit, again: () -> Unit) {
    val c = DS.colors
    val text = outcome.text
    DSCard(padding = 20.dp, modifier = Modifier.testTag("pair.result")) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Icon(painterResource(icon(outcome, text.isSecurity)), contentDescription = null, tint = tint(outcome, text.isSecurity), modifier = Modifier.size(34.dp))
            Text(
                text.title,
                style = DSType.sectionHeading,
                color = c.textPrimary,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .semantics {
                        heading()
                        liveRegion = LiveRegionMode.Polite
                    }
                    .testTag("pair.result.title"),
            )
            Text(text.message, style = DSType.body, color = c.textSecondary, textAlign = TextAlign.Center)
            Column(verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.widthIn(max = 260.dp)) {
                val againTitle = when (outcome) {
                    is PairingOutcome.Paired -> "Pair another device"
                    PairingOutcome.TimeUp, PairingOutcome.UsedUp -> "Start again"
                    else -> "Try again"
                }
                when {
                    paused -> PrimaryButton(againTitle, again, modifier = Modifier.testTag("pair.again"))
                    outcome is PairingOutcome.Paired || outcome == PairingOutcome.YouDeclined -> {
                        PrimaryButton("Done", done, modifier = Modifier.testTag("pair.done"))
                        OutlineButton(againTitle, again, modifier = Modifier.testTag("pair.again"))
                    }
                    else -> {
                        PrimaryButton(againTitle, again, modifier = Modifier.testTag("pair.again"))
                        OutlineButton("Done", done, modifier = Modifier.testTag("pair.done"))
                    }
                }
            }
        }
    }
}

private fun icon(outcome: PairingOutcome, security: Boolean): Int = when {
    security -> DSIcons.warning
    outcome is PairingOutcome.Paired -> DSIcons.check
    outcome == PairingOutcome.YouDeclined || outcome is PairingOutcome.OtherDeclined || outcome == PairingOutcome.Cancelled -> DSIcons.failed
    outcome is PairingOutcome.ConnectionLost || outcome is PairingOutcome.Failed -> DSIcons.warning
    else -> DSIcons.info
}

/** `danger` only for a possible attack. */
@Composable
private fun tint(outcome: PairingOutcome, security: Boolean): Color = when {
    security -> DS.colors.danger
    outcome is PairingOutcome.Paired -> DS.colors.success
    else -> DS.colors.textSecondary
}
