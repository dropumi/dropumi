// SPDX-License-Identifier: MPL-2.0
//
// First launch (a port of the Apple app's OnboardingView): what Dropumi is,
// this device's name, a word about the local network, and who may send.
// Receiving starts only on Start. The name field is prefilled only with the
// core's own random "Dropumi-XXXX", never a system, model, Bluetooth or
// account name (ADR-0013 D3). Choosing Everyone asks for a confirmation in
// the window first (plan §7.2 item 5).
package io.github.dropumi.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.ui.Confirmation
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSTextField
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.Wordmark
import kotlinx.coroutines.launch

private const val STEPS = 4

@Composable
fun OnboardingScreen(state: AppState) {
    val model = LocalModel.current
    val scope = LocalAppScope.current
    val confirmations = LocalConfirmations.current
    var step by rememberSaveable { mutableIntStateOf(0) }
    // Starts empty and takes only the core's own name (never a system name).
    var name by rememberSaveable { mutableStateOf("") }
    var choice by rememberSaveable { mutableStateOf(ReceiveVisibility.PAIRED_ONLY) }
    var working by rememberSaveable { mutableStateOf(false) }
    val suggested = state.suggestedDeviceName
    LaunchedEffect(suggested) {
        if (name.isEmpty() && suggested != null) name = suggested
    }
    val trimmed = name.trim()

    fun next() {
        when (step) {
            1 -> {
                working = true
                scope.launch {
                    if (trimmed != state.identity?.name) model.renameDevice(trimmed)
                    working = false
                    step = 2
                }
            }
            STEPS - 1 -> {
                // Receiving starts only now.
                working = true
                val value = choice
                scope.launch {
                    model.finishOnboarding(value)
                    working = false
                }
            }
            else -> step += 1
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(DS.colors.bg)
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .imePadding(),
    ) {
        Box(contentAlignment = Alignment.TopCenter, modifier = Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState())) {
            Column(
                verticalArrangement = Arrangement.spacedBy(20.dp),
                modifier = Modifier.widthIn(max = 560.dp).fillMaxWidth().padding(DSMetrics.screenPadding),
            ) {
                when (step) {
                    0 -> Welcome()
                    1 -> DeviceNameStep(name) { name = it }
                    2 -> LocalNetworkStep()
                    else -> WhoCanSendStep(choice) { value ->
                        if (value == ReceiveVisibility.EVERYONE && choice != value) {
                            confirmations.ask(everyoneConfirmation { choice = ReceiveVisibility.EVERYONE })
                        } else {
                            choice = value
                        }
                    }
                }
            }
        }
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.align(Alignment.CenterHorizontally).widthIn(max = 560.dp).fillMaxWidth().padding(DSMetrics.screenPadding),
        ) {
            PrimaryButton(
                if (step == STEPS - 1) "Start" else "Continue",
                ::next,
                enabled = !working && !(step == 1 && trimmed.isEmpty()),
                modifier = Modifier.testTag("onboarding.next"),
            )
            PageDots(step)
        }
    }
}

/** The confirmation for letting everyone send (also used by Settings and the banner). */
fun everyoneConfirmation(onConfirm: () -> Unit) = Confirmation(
    title = "Let everyone send for 10 minutes?",
    message = "Anyone on this network can ask to send you files. You compare a code before accepting each one. " +
        "After 10 minutes it goes back to paired devices only.",
    confirmLabel = "Let everyone send",
    onConfirm = onConfirm,
)

@Composable
private fun PageDots(step: Int) {
    val c = DS.colors
    Row(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier.semantics(mergeDescendants = true) { contentDescription = "Step ${step + 1} of $STEPS" },
    ) {
        repeat(STEPS) { index ->
            Box(
                Modifier
                    .height(6.dp)
                    .width(if (index == step) 18.dp else 6.dp)
                    .clip(CircleShape)
                    .background(if (index == step) c.accent else c.border),
            )
        }
    }
}

@Composable
private fun Point(icon: Int, text: String) {
    val c = DS.colors
    Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Icon(painterResource(icon), contentDescription = null, tint = c.accent, modifier = Modifier.size(22.dp))
        Text(text, style = DSType.body, color = c.textPrimary, modifier = Modifier.weight(1f))
    }
}

@Composable
private fun Welcome() {
    Spacer(Modifier.height(24.dp))
    Wordmark(height = 36.dp)
    Text("Just drop it nearby.", style = DSType.screenTitle, color = DS.colors.textPrimary)
    Point(DSIcons.nearby, "Files go straight to the other device over your local network.")
    Point(DSIcons.noCloud, "No account, no cloud, no internet needed.")
    Point(DSIcons.shield, "Every connection is encrypted, and you decide who can send to you.")
    Caption("Dropumi is a pre-release and has not had a security audit yet.")
}

@Composable
private fun DeviceNameStep(name: String, onName: (String) -> Unit) {
    Spacer(Modifier.height(24.dp))
    ScreenHeader("Name this device", "Nearby devices see this name when they look for devices.")
    DSTextField(name, onName, label = "Device name", modifier = Modifier.testTag("onboarding.name"))
    Caption("Anyone on the same network can see it, so avoid your full name.")
}

@Composable
private fun LocalNetworkStep() {
    Spacer(Modifier.height(24.dp))
    ScreenHeader(
        "Find devices on your network",
        "Dropumi looks for nearby devices on your local network, and they find it the same way.",
    )
    Point(DSIcons.wifi, "Use the same Wi-Fi network on both devices. Nothing goes through the internet.")
    Point(
        DSIcons.nearby,
        "On some Android versions other devices can't see this one. They can still send to it after pairing, or by its address.",
    )
}

@Composable
private fun WhoCanSendStep(choice: ReceiveVisibility, onChoice: (ReceiveVisibility) -> Unit) {
    Spacer(Modifier.height(24.dp))
    ScreenHeader("Who can send to this device?", "You can change this at any time in Settings.")
    Column(verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.selectableGroup()) {
        VisibilityOption(
            ReceiveVisibility.PAIRED_ONLY, choice, onChoice,
            "Paired devices only",
            "Recommended. Devices you paired with by comparing a code. You still answer each request.",
        )
        VisibilityOption(
            ReceiveVisibility.EVERYONE, choice, onChoice,
            "Everyone, for 10 minutes",
            "Anyone nearby can ask. You compare a code before accepting. Then it goes back to paired devices only.",
        )
        VisibilityOption(
            ReceiveVisibility.OFF, choice, onChoice,
            "No one for now",
            "This device doesn't receive. You can still send.",
        )
    }
}

@Composable
private fun VisibilityOption(
    value: ReceiveVisibility,
    choice: ReceiveVisibility,
    onChoice: (ReceiveVisibility) -> Unit,
    title: String,
    detail: String,
) {
    val c = DS.colors
    val selected = value == choice
    val shape = RoundedCornerShape(16.dp)
    Row(
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.surface)
            .border(if (selected) 2.dp else 1.dp, if (selected) c.accent else c.border, shape)
            .selectable(selected = selected, role = Role.RadioButton) { onChoice(value) }
            .padding(14.dp)
            .testTag("onboarding.visibility.${value.name}"),
    ) {
        RadioButton(
            selected = selected,
            onClick = null,
            colors = RadioButtonDefaults.colors(selectedColor = c.accent, unselectedColor = c.textSecondary),
        )
        Column(verticalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.weight(1f)) {
            Text(title, style = DSType.rowTitle, color = c.textPrimary)
            Text(detail, style = DSType.meta, color = c.textSecondary)
        }
    }
}
