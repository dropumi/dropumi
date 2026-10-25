// SPDX-License-Identifier: MPL-2.0
//
// Paired devices (a port of the Apple app's TrustedDevicesScreen): rename,
// remove, or add one by its full 52-character fingerprint. There is no Send
// button here on purpose: sending goes through a nearby device or an
// address, where the key is checked. Removing and adding are confirmed in
// the window first (plan §7.2 item 5); failures the core reports are shown
// (fixes Apple gap 2).
package io.github.dropumi.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.Alignment
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.TargetInput
import io.github.dropumi.ffi.TrustMethod
import io.github.dropumi.ffi.TrustedDeviceInfo
import io.github.dropumi.ui.Confirmation
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSCard
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSRow
import io.github.dropumi.ui.design.DSTextField
import io.github.dropumi.ui.design.DeviceAvatar
import io.github.dropumi.ui.design.EmptyState
import io.github.dropumi.ui.design.IconAction
import io.github.dropumi.ui.design.LinkButton
import io.github.dropumi.ui.design.OutlineButton
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionHeading
import io.github.dropumi.ui.design.largeText
import kotlinx.coroutines.launch

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun PairedDevicesScreen(state: AppState) {
    val router = LocalRouter.current
    var adding by rememberSaveable { mutableStateOf(false) }
    var renaming by rememberSaveable { mutableStateOf<String?>(null) }
    ScreenColumn {
        ScreenHeader("Paired devices", "These devices can send to this one when you choose Paired only.")
        if (adding) {
            AddByFingerprint(onDone = { adding = false })
        } else {
            // The screen's actions; they wrap at large text.
            FlowRow {
                LinkButton("Pair a device", { router.openPairing() }, icon = DSIcons.pair, modifier = Modifier.testTag("paired.pair"))
                LinkButton("Add by fingerprint", { adding = true }, icon = DSIcons.add, modifier = Modifier.testTag("paired.add"))
            }
        }
        if (state.trusted.isEmpty() && !adding) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.fillMaxWidth().testTag("paired.empty"),
            ) {
                EmptyState(
                    DSIcons.shield,
                    "No paired devices",
                    "Pair a device to send to each other without a code. You can also add one by its fingerprint.",
                )
                PrimaryButton("Pair a device", { router.openPairing() }, modifier = Modifier.widthIn(max = 260.dp).testTag("paired.emptyPair"))
                LinkButton("Add by fingerprint", { adding = true }, modifier = Modifier.testTag("paired.emptyAdd"))
            }
        }
        Column {
            for (device in state.trusted) {
                if (renaming == device.fingerprint) {
                    RenameDevice(device, onDone = { renaming = null })
                } else {
                    PairedRow(device, onRename = { renaming = device.fingerprint })
                }
            }
        }
    }
}

private fun subtitle(device: TrustedDeviceInfo): String {
    val method = if (device.method == TrustMethod.CODE) "Paired with a code" else "Added by fingerprint"
    return "$method · ${device.fingerprintDisplay}"
}

@Composable
private fun PairedRow(device: TrustedDeviceInfo, onRename: () -> Unit) {
    val model = LocalModel.current
    val scope = LocalAppScope.current
    val confirmations = LocalConfirmations.current
    var menu by remember { mutableStateOf(false) }
    DSRow(
        title = device.alias ?: "Unnamed device",
        subtitle = subtitle(device),
        leading = { DeviceAvatar() },
        modifier = Modifier.testTag("paired.row"),
    ) {
        Box {
            IconAction(DSIcons.more, "Actions for ${device.alias ?: "this device"}", { menu = true })
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(text = { Text("Rename…") }, onClick = {
                    menu = false
                    onRename()
                })
                DropdownMenuItem(text = { Text("Remove…", color = DS.colors.danger) }, onClick = {
                    menu = false
                    confirmations.ask(
                        Confirmation(
                            title = "Remove this device?",
                            message = "It can no longer send to this device while Paired only is on. You can pair again later.",
                            confirmLabel = "Remove",
                            destructive = true,
                        ) { scope.launch { model.forget(device) } },
                    )
                })
            }
        }
    }
}

@Composable
private fun RenameDevice(device: TrustedDeviceInfo, onDone: () -> Unit) {
    val model = LocalModel.current
    val scope = LocalAppScope.current
    var name by rememberSaveable(device.fingerprint) { mutableStateOf(device.alias.orEmpty()) }
    val trimmed = name.trim()
    DSCard(modifier = Modifier.padding(vertical = 8.dp)) {
        SectionHeading("Rename")
        DSTextField(name, { name = it }, label = "Name")
        Caption(device.fingerprintDisplay)
        Buttons(
            cancel = { OutlineButton("Cancel", onDone, modifier = it) },
            confirm = {
                PrimaryButton("Save", {
                    scope.launch {
                        if (model.rename(device, trimmed)) onDone()
                    }
                }, enabled = trimmed.isNotEmpty(), modifier = it)
            },
        )
    }
}

@Composable
private fun AddByFingerprint(onDone: () -> Unit) {
    val model = LocalModel.current
    val scope = LocalAppScope.current
    val confirmations = LocalConfirmations.current
    var fingerprint by rememberSaveable { mutableStateOf("") }
    var alias by rememberSaveable { mutableStateOf("") }
    val full = TargetInput.fullFingerprint(fingerprint)
    val typed = TargetInput.compact(fingerprint).length
    DSCard {
        SectionHeading("Add by fingerprint")
        DSTextField(
            fingerprint,
            { fingerprint = it },
            label = "Fingerprint (52 characters)",
            monospace = true,
            capitalizeAll = true,
            singleLine = false,
            modifier = Modifier.testTag("paired.fingerprint"),
        )
        if (typed > 0 && full == null) Caption("$typed of ${TargetInput.FINGERPRINT_LENGTH} characters")
        DSTextField(alias, { alias = it }, label = "Name (optional)")
        Caption(
            "Read the full fingerprint on the other device: Settings › This device. " +
                "Only add a fingerprint you checked yourself.",
        )
        Buttons(
            cancel = { OutlineButton("Cancel", onDone, modifier = it) },
            confirm = {
                PrimaryButton("Add", {
                    val value = full ?: return@PrimaryButton
                    val name = alias.trim().ifEmpty { null }
                    confirmations.ask(
                        Confirmation(
                            title = "Add this device?",
                            message = "It can send to this device while Paired only is on, and you can send to it " +
                                "without comparing a code. Fingerprint: ${Formatting.fingerprint(value)}",
                            confirmLabel = "Add device",
                        ) {
                            scope.launch { if (model.trust(value, name)) onDone() }
                        },
                    )
                }, enabled = full != null, modifier = it.testTag("paired.addConfirm"))
            },
        )
    }
}

/** Two buttons side by side, or stacked at large text. */
@Composable
internal fun Buttons(cancel: @Composable (Modifier) -> Unit, confirm: @Composable (Modifier) -> Unit) {
    if (largeText()) {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            cancel(Modifier)
            confirm(Modifier)
        }
    } else {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            cancel(Modifier.weight(1f))
            confirm(Modifier.weight(1f))
        }
    }
}
