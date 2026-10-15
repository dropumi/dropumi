// SPDX-License-Identifier: MPL-2.0
//
// Settings (a port of the Apple app's SettingsScreen). Turning on Everyone
// and Auto-accept paired devices are confirmed in the window first (plan
// §7.2 item 5). Receiving runs only while Dropumi is open (2c-1).
package io.github.dropumi.ui.screens

import android.content.ActivityNotFoundException
import android.content.Intent
import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.github.dropumi.BuildConfig
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Appearance
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.ui.Confirmation
import io.github.dropumi.ui.Screen
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSSegmented
import io.github.dropumi.ui.design.DSSwitch
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SettingsRow
import io.github.dropumi.ui.design.ThisDeviceDisc
import io.github.dropumi.ui.design.rowDivider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val SOURCE_URL = "https://github.com/dropumi/dropumi"

/** The note about Save (G45), also shown the first time Save is used. */
const val SAVE_DISCLOSURE = "Saved files can be opened by other apps that can see your Downloads."

@Composable
fun SettingsScreen(state: AppState) {
    val model = LocalModel.current
    val router = LocalRouter.current
    val scope = LocalAppScope.current
    val confirmations = LocalConfirmations.current
    val context = LocalContext.current
    LaunchedEffect(Unit) { model.refreshTrusted() }

    ScreenColumn {
        ScreenHeader("Settings", "Your device and how it's found")
        ThisDeviceRow(state) { router.push(Screen.ThisDevice) }
        WhoCanSend(state) { value ->
            if (value == ReceiveVisibility.EVERYONE && state.visibility != value) {
                confirmations.ask(everyoneConfirmation { scope.launch { model.setVisibility(ReceiveVisibility.EVERYONE) } })
            } else if (value != state.visibility) {
                scope.launch { model.setVisibility(value) }
            }
        }
        Column {
            SettingsRow(
                DSIcons.shield,
                "Paired devices",
                ScreenText.deviceCount(state.trusted.size),
                onClick = { router.push(Screen.PairedDevices) },
                modifier = Modifier.testTag("settings.paired"),
            )
            SettingsRow(
                DSIcons.pair,
                "Pair a device",
                "Compare a code once; then send without checks",
                onClick = { router.push(Screen.Pairing) },
                modifier = Modifier.testTag("settings.pair"),
            )
            SettingsRow(
                DSIcons.code,
                "Connect by address",
                "When a device doesn't show up nearby",
                onClick = { router.push(Screen.ConnectByAddress) },
                modifier = Modifier.testTag("settings.address"),
            )
            ReceivedFilesRow()
            SettingsRow(
                DSIcons.shield,
                "Auto-accept paired devices",
                "Skip the question for devices you paired",
            ) {
                DSSwitch(
                    checked = state.autoAcceptPaired,
                    onCheckedChange = { on ->
                        if (on) {
                            confirmations.ask(
                                Confirmation(
                                    title = "Accept paired devices without asking?",
                                    message = "Files from devices you paired with are saved without a question. " +
                                        "Devices that aren't paired still need your answer.",
                                    confirmLabel = "Turn on",
                                ) { scope.launch { model.setAutoAcceptPaired(true) } },
                            )
                        } else {
                            scope.launch { model.setAutoAcceptPaired(false) }
                        }
                    },
                    label = "Auto-accept paired devices",
                    modifier = Modifier.testTag("settings.autoAccept"),
                )
            }
            AppearanceRow(state)
            SettingsRow(DSIcons.licences, "Licences", "The open-source parts Dropumi is built from", onClick = { router.push(Screen.Licences) })
            SettingsRow(DSIcons.code, "Open source", "View the code and contribute", onClick = {
                try {
                    context.startActivity(Intent(Intent.ACTION_VIEW, SOURCE_URL.toUri()).addCategory(Intent.CATEGORY_BROWSABLE))
                } catch (_: ActivityNotFoundException) {
                    model.notice("No browser", "Open $SOURCE_URL on another device.")
                }
            })
        }
        Caption(
            "Free · No account · No internet needed · Version ${BuildConfig.VERSION_NAME} (${BuildConfig.COMMIT})",
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun ThisDeviceRow(state: AppState, onClick: () -> Unit) {
    val c = DS.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(vertical = 8.dp)
            .testTag("settings.thisDevice"),
    ) {
        ThisDeviceDisc(52.dp)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp), modifier = Modifier.weight(1f)) {
            Text(state.identity?.name.orEmpty(), style = DSType.rowTitle, color = c.textPrimary)
            Text("Nearby devices see this name", style = DSType.meta, color = c.textSecondary)
        }
        Icon(painterResource(DSIcons.chevron), contentDescription = null, tint = c.textSecondary)
    }
}

@Composable
private fun WhoCanSend(state: AppState, onChoose: (ReceiveVisibility) -> Unit) {
    val now = rememberNowMs(ticking = state.receiver?.everyoneUntil != null)
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Caption("Who can send to this device")
        DSSegmented(
            options = listOf(
                ReceiveVisibility.PAIRED_ONLY to "Paired only",
                ReceiveVisibility.EVERYONE to "Everyone · 10 min",
                ReceiveVisibility.OFF to "Off",
            ),
            selected = state.visibility,
            onSelect = onChoose,
            modifier = Modifier.testTag("settings.visibility"),
        )
        Caption(
            if (state.pairingWindowActive) {
                "A pairing window is open. A change here applies when it closes."
            } else {
                ScreenText.visibilityCaption(state, now)
            },
        )
    }
}

/** Received files: where they are, what they use, and who else can see saved ones. */
@Composable
private fun ReceivedFilesRow() {
    val files = LocalFileActions.current
    val used by produceState<Long?>(initialValue = null, files) {
        value = withContext(Dispatchers.IO) { files.usedBytes() }
    }
    val usage = used?.let { " Using ${Formatting.bytes(it.toULong())}." } ?: ""
    SettingsRow(
        DSIcons.receiveFolder,
        "Received files",
        "Kept inside Dropumi until you save or share them.$usage $SAVE_DISCLOSURE",
    )
}

@Composable
private fun AppearanceRow(state: AppState) {
    val model = LocalModel.current
    val choices = remember { Appearance.choices(Build.VERSION.SDK_INT) }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.rowDivider(DS.colors.border).padding(vertical = 12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            Icon(painterResource(DSIcons.appearance), contentDescription = null, tint = DS.colors.accent)
            Text("Appearance", style = DSType.rowTitle, color = DS.colors.textPrimary)
        }
        DSSegmented(
            options = choices.map {
                it to when (it) {
                    Appearance.SYSTEM -> "Match system"
                    Appearance.LIGHT -> "Light"
                    Appearance.DARK -> "Dark"
                }
            },
            selected = state.appearance,
            onSelect = model::setAppearance,
            modifier = Modifier.testTag("settings.appearance"),
        )
    }
}
