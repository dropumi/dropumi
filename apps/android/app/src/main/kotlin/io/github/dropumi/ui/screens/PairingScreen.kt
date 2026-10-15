// SPDX-License-Identifier: MPL-2.0
//
// Pairing (a port of the Apple app's PairingScreen): compare a six-digit
// code once; afterwards the devices know each other's key (ADR-0004).
// Either this device asks a nearby one, or it opens a five-minute window
// for another device to ask. The code itself appears as a prompt; trust is
// stored only if both users confirm, and "Paired" shows only on
// PairingFinished(Paired). Where this device is not listed nearby
// (ADR-0013 D5), the window shows the address to type instead.
package io.github.dropumi.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.ErrorCatalog
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.PairingSession
import io.github.dropumi.appstate.PeerLabel
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSCard
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.EmptyState
import io.github.dropumi.ui.design.OutlineButton
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionHeading
import io.github.dropumi.ui.design.SoftPill
import io.github.dropumi.ui.design.DSRow
import io.github.dropumi.ui.design.DeviceAvatar
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import androidx.compose.ui.semantics.contentDescription

@Composable
fun PairingScreen(state: AppState) {
    val model = LocalModel.current
    BrowseWhileVisible(model)
    ScreenColumn {
        ScreenHeader("Pair a device", "You compare a code once. After that, the devices recognise each other.")
        val session = state.pairing
        if (session != null) PairingStatus(state, session) else Choices(state)
    }
}

@Composable
private fun Choices(state: AppState) {
    val model = LocalModel.current
    val scope = LocalAppScope.current
    Column(verticalArrangement = Arrangement.spacedBy(DSMetrics.sectionGap)) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            SectionHeading("Let another device pair with this one")
            Caption(
                if (state.advertising is AdvertiseState.GateClosed) {
                    "For five minutes, a device can ask to pair with this one by typing its address. You both check a code."
                } else {
                    "For five minutes, a device can ask to pair with this one. You both check a code."
                },
            )
            PrimaryButton("Wait for a device", { scope.launch { model.openPairingWindow() } }, modifier = Modifier.testTag("pair.window"))
        }
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            SectionHeading("Pair with a nearby device")
            Caption("On the other device, open Dropumi and choose to wait for a device.")
            if (state.nearby.isEmpty()) {
                EmptyState(DSIcons.nearby, "Looking for devices…", "Make sure the other device is on the same Wi-Fi network.")
            }
            for (peer in state.nearby) {
                DSRow(
                    title = peer.name,
                    subtitle = ScreenText.nearbySubtitle(peer),
                    leading = { DeviceAvatar() },
                ) {
                    SoftPill(
                        "Pair",
                        { scope.launch { model.pair(SendTarget.Nearby(peer.instance), peer.name) } },
                        enabled = peer.compatible,
                        modifier = Modifier.semantics { contentDescription = "Pair with ${peer.name}" },
                    )
                }
            }
        }
    }
}

@Composable
private fun PairingStatus(state: AppState, session: PairingSession) {
    val model = LocalModel.current
    val c = DS.colors
    DSCard(padding = 20.dp, modifier = Modifier.testTag("pair.status")) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
        ) {
            when (val result = session.result) {
                null -> Waiting(state, session)
                is PairingResult.Paired -> Outcome(
                    DSIcons.check, c.success,
                    "Paired — ${PeerLabel.of(result.peer).title}",
                    "${PeerLabel.of(result.peer).title} can now send to this device, and you can send to it without a code.",
                )
                is PairingResult.Failed -> {
                    val text = ErrorCatalog.text(result.error)
                    Outcome(DSIcons.warning, c.danger, text.title, text.message)
                }
                PairingResult.Cancelled -> Outcome(DSIcons.failed, c.textSecondary, "Pairing cancelled", "Nothing was saved.")
            }
            if (session.result == null) {
                OutlineButton("Cancel", { model.cancelPairing() }, modifier = Modifier.widthIn(max = 260.dp).testTag("pair.cancel"))
            } else {
                PrimaryButton("Done", { model.dismissPairing() }, modifier = Modifier.widthIn(max = 260.dp).testTag("pair.done"))
            }
        }
    }
}

@Composable
private fun Waiting(state: AppState, session: PairingSession) {
    val model = LocalModel.current
    val c = DS.colors
    CircularProgressIndicator(color = c.accent, modifier = Modifier.size(28.dp))
    when (val role = session.role) {
        is PairingSession.Role.Initiator -> {
            Text("Asking ${role.name}…", style = DSType.rowTitle, color = c.textPrimary, textAlign = TextAlign.Center)
            Caption("A code appears on both devices. Check that they match.")
        }
        PairingSession.Role.Window -> {
            val now = rememberNowMs(ticking = true)
            val left = session.expiresAtMs?.let { maxOf(0L, (it - now) / 1000) } ?: 0L
            Text("Waiting for a device · ${Formatting.countdown(left)}", style = DSType.rowTitle, color = c.textPrimary, textAlign = TextAlign.Center)
            Caption("On the other device, choose Pair a device, then this one:")
            Text(state.identity?.name.orEmpty(), style = DSType.sectionHeading, color = c.textPrimary, textAlign = TextAlign.Center)
            // Reading the network's addresses asks the system: off the main thread.
            val addresses by produceState(emptyList<String>(), session.port) {
                value = withContext(Dispatchers.IO) { model.pairingAddresses() }
            }
            if (addresses.isNotEmpty()) {
                Caption(
                    if (state.advertising is AdvertiseState.GateClosed) {
                        "Other devices can't see this one nearby. On the other device, choose Connect by address and type:"
                    } else {
                        "If it doesn't show up there, choose Connect by address on the other device and type:"
                    },
                )
                SelectionContainer {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        for (address in addresses) {
                            Text(address, style = DSType.fingerprint.copy(fontSize = DSType.body.fontSize), color = c.textPrimary)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Outcome(icon: Int, tint: androidx.compose.ui.graphics.Color, title: String, message: String) {
    val c = DS.colors
    Icon(painterResource(icon), contentDescription = null, tint = tint, modifier = Modifier.size(34.dp))
    Text(title, style = DSType.sectionHeading, color = c.textPrimary, textAlign = TextAlign.Center)
    Text(message, style = DSType.body, color = c.textSecondary, textAlign = TextAlign.Center)
}
