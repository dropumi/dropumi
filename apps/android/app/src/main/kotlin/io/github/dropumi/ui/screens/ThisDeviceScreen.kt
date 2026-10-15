// SPDX-License-Identifier: MPL-2.0
//
// This device (a port of the Apple app's ThisDeviceScreen): its name, its
// full fingerprint in 13 groups of 4, the addresses other devices can type,
// and Reset identity (confirmed in the window first). Rename and reset stop
// the receiver first and start it again (AppModel).
package io.github.dropumi.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.ui.Confirmation
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSTextField
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.OutlineButton
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionLabel
import io.github.dropumi.ui.design.SoftPill
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun ThisDeviceScreen(state: AppState) {
    val model = LocalModel.current
    val scope = LocalAppScope.current
    val confirmations = LocalConfirmations.current
    val c = DS.colors
    var name by rememberSaveable { mutableStateOf("") }
    var saving by rememberSaveable { mutableStateOf(false) }
    val current = state.identity?.name
    LaunchedEffect(current) {
        if (current != null && !saving) name = current
    }
    val trimmed = name.trim()

    fun save() {
        if (trimmed.isEmpty() || trimmed == current) return
        saving = true
        scope.launch {
            model.renameDevice(trimmed)
            name = model.state.value.identity?.name ?: trimmed
            saving = false
        }
    }

    ScreenColumn {
        ScreenHeader("This device", null)
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionLabel("Name")
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                DSTextField(name, { name = it }, label = "Device name", modifier = Modifier.weight(1f).testTag("device.name"))
                SoftPill("Save", ::save, enabled = !saving && trimmed.isNotEmpty() && trimmed != current)
            }
            Caption("Nearby devices see this name. It is only a label: devices recognise each other by their key.")
        }
        state.identity?.let { identity ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SectionLabel("Fingerprint")
                // All 52 characters: Add by fingerprint needs every one of them.
                SelectionContainer {
                    Text(
                        Formatting.fingerprint(identity.fingerprint),
                        style = DSType.fingerprint.copy(fontSize = DSType.body.fontSize),
                        color = c.textPrimary,
                        modifier = Modifier
                            .semantics { contentDescription = "Fingerprint: " + ScreenText.spokenFingerprint(identity.fingerprint) }
                            .testTag("device.fingerprint"),
                    )
                }
                Caption(
                    "This is the full fingerprint. Another device can add this one by typing all of it " +
                        "(Add by fingerprint). Compare it in person.",
                )
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionLabel("Address")
            // Reading the network's addresses asks the system: off the main thread.
            val addresses by produceState<List<String>?>(null, state.receiver?.port) {
                value = withContext(Dispatchers.IO) { model.connectionDetails() }
            }
            val list = addresses
            if (state.receiver == null) {
                Text("Not receiving right now.", style = DSType.body, color = c.textSecondary, modifier = Modifier.testTag("device.address.none"))
            } else if (list != null && list.isEmpty()) {
                Text("No local network address right now. Connect to Wi-Fi.", style = DSType.body, color = c.textSecondary)
            } else if (list != null) {
                SelectionContainer {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        for (address in list) {
                            Text(address, style = DSType.fingerprint.copy(fontSize = DSType.body.fontSize), color = c.textPrimary, modifier = Modifier.testTag("device.address"))
                        }
                    }
                }
                Caption("If this device doesn't show up nearby, type this address on the other device (Connect by address).")
            }
        }
        OutlineButton(
            "Reset identity…",
            {
                confirmations.ask(
                    Confirmation(
                        title = "Reset this device's identity?",
                        message = "This device gets a new key and fingerprint. Every device you paired with must pair again.",
                        confirmLabel = "Reset identity",
                        destructive = true,
                    ) { scope.launch { model.resetIdentity() } },
                )
            },
            color = c.danger,
            outline = c.danger,
            modifier = Modifier.widthIn(max = 320.dp).testTag("device.reset"),
        )
    }
}
