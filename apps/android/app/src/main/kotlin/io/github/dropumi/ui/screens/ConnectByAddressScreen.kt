// SPDX-License-Identifier: MPL-2.0
//
// Connect by address (a port of the Apple app's ConnectByAddressScreen), for
// networks where discovery does not work and for devices that are not
// listed nearby. The address needs a port from 1 to 65535 (fixes Apple gap
// 6). An optional fingerprint pin (at least 24 characters) is checked
// before anything is sent; without one, both screens show a code to
// compare first (`allowUnverified`). Its pair mode (PairByAddressScreen) is
// opened from the pairing screen, for devices that are not listed.
package io.github.dropumi.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.TargetInput
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ui.LocalTouchGuard
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSTextField
import io.github.dropumi.ui.design.OutlineButton
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionLabel
import io.github.dropumi.ui.design.SoftPill
import kotlinx.coroutines.launch

@Composable
fun ConnectByAddressScreen(state: AppState, picking: DocumentPicking) {
    val model = LocalModel.current
    val router = LocalRouter.current
    val scope = LocalAppScope.current
    val guard = LocalTouchGuard.current
    val c = DS.colors
    var address by rememberSaveable { mutableStateOf("") }
    var pin by rememberSaveable { mutableStateOf("") }
    var sending by remember { mutableStateOf(false) }
    val target = TargetInput.address(address)
    val addressProblem = TargetInput.addressProblem(address)
    val pinValid = TargetInput.pinValid(pin)
    val ready = target != null && pinValid

    fun sendTarget(): SendTarget? = target?.let { SendTarget.Address(it, TargetInput.pin(pin)) }

    ScreenColumn {
        ScreenHeader("Connect by address", "Type the address shown on the other device under Settings › This device.")
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionLabel("Address")
            DSTextField(
                address,
                { address = it },
                label = "Address",
                placeholder = "192.168.1.20:47321",
                monospace = true,
                keyboardType = KeyboardType.Uri,
                isError = addressProblem != null,
                modifier = Modifier.testTag("address.field"),
            )
            if (addressProblem != null) Caption(addressProblem, color = c.danger)
            SectionLabel("Fingerprint (optional)")
            DSTextField(
                pin,
                { pin = it },
                label = "At least 24 characters",
                monospace = true,
                capitalizeAll = true,
                isError = !pinValid,
                modifier = Modifier.testTag("address.pin"),
            )
            Caption(
                when {
                    TargetInput.pin(pin) == null -> "Without a fingerprint, you compare a code with the other device first."
                    pinValid -> "Dropumi checks this fingerprint before anything is sent."
                    else -> "Type ${TargetInput.MIN_PIN_LENGTH} to ${TargetInput.FINGERPRINT_LENGTH} characters of the fingerprint."
                },
                color = if (pinValid) c.textSecondary else c.danger,
            )
        }
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Caption(
                if (state.draft.isEmpty()) "No files chosen yet." else "${Formatting.fileCount(state.draft.size)} ready to send.",
            )
            if (state.draft.isEmpty()) SoftPill("Choose files", { picking.pick(openSend = false) })
            PrimaryButton(
                "Send files",
                {
                    val t = sendTarget() ?: return@PrimaryButton
                    val name = target ?: return@PrimaryButton
                    // R23: a click that came through another app's window never sends.
                    if (!guard.allowSend()) return@PrimaryButton
                    sending = true
                    scope.launch {
                        model.sendDraft(t, name, allowUnverified = true)
                        sending = false
                        if (model.state.value.draft.isEmpty()) router.openTransfers()
                    }
                },
                enabled = ready && state.draft.isNotEmpty() && picking.preparing == 0 && !sending,
                modifier = Modifier.testTag("address.send"),
            )
            OutlineButton(
                "Pair with this address",
                {
                    val t = sendTarget() ?: return@OutlineButton
                    val name = target ?: return@OutlineButton
                    model.startPairing(t, name)
                    router.openPairing()
                },
                enabled = ready,
            )
        }
    }
}

/**
 * Connect by address in its pair mode, opened from the pairing screen: the
 * address only. Pair asks that address and goes back to the pairing
 * screen, which shows the request.
 */
@Composable
fun PairByAddressScreen() {
    val model = LocalModel.current
    val router = LocalRouter.current
    val c = DS.colors
    var address by rememberSaveable { mutableStateOf("") }
    val target = TargetInput.address(address)
    val addressProblem = TargetInput.addressProblem(address)

    ScreenColumn {
        ScreenHeader("Pair by address", "Type the address the other device shows on its Pair a device screen.")
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionLabel("Address")
            DSTextField(
                address,
                { address = it },
                label = "Address",
                placeholder = "192.168.1.20:47321",
                monospace = true,
                keyboardType = KeyboardType.Uri,
                isError = addressProblem != null,
                modifier = Modifier.testTag("address.field"),
            )
            if (addressProblem != null) Caption(addressProblem, color = c.danger)
        }
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton(
                "Pair",
                {
                    val t = target ?: return@PrimaryButton
                    model.startPairing(SendTarget.Address(t, null), t)
                    router.openPairing()
                },
                enabled = target != null,
                modifier = Modifier.testTag("address.pair"),
            )
            Caption("You'll compare a code next.")
        }
    }
}
