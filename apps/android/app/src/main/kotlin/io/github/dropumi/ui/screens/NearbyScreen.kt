// SPDX-License-Identifier: MPL-2.0
//
// Nearby (a port of the Apple app's NearbyScreen): this device on the
// radar, nearby devices, and a Send button for each. Names come from the
// network and are shown verbatim; a device is never labelled "Paired" here,
// because nothing has checked its key yet (ADR-0004). Browsing runs only
// while this screen is visible. Where this device cannot be advertised (the
// R21 gate, ADR-0013 D5) a card says so: it is browse-only.
package io.github.dropumi.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.ffi.NearbyPeer
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ui.Screen
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSRow
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.DeviceAvatar
import io.github.dropumi.ui.design.EmptyState
import io.github.dropumi.ui.design.IconAction
import io.github.dropumi.ui.design.LinkButton
import io.github.dropumi.ui.design.ProblemCard
import io.github.dropumi.ui.design.Radar
import io.github.dropumi.ui.design.RadarItem
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionHeading
import io.github.dropumi.ui.design.SoftPill
import io.github.dropumi.ui.design.Wordmark
import kotlinx.coroutines.launch

@Composable
fun NearbyScreen(state: AppState, wide: Boolean, onChooseFiles: () -> Unit) {
    val model = LocalModel.current
    val router = LocalRouter.current
    BrowseWhileVisible(model)
    ScreenColumn(horizontalPadding = if (wide) DSMetrics.widePadding else DSMetrics.screenPadding) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            if (!wide) Wordmark(height = 26.dp)
            // The pill takes what is left (it wraps at large text); the menu keeps its 48 dp.
            Box(contentAlignment = Alignment.CenterEnd, modifier = Modifier.weight(1f)) { VisibilityPill(state) }
            MoreMenu(onChooseFiles)
        }
        ScreenHeader("Nearby", "${ScreenText.deviceCount(state.nearby.size)} found · works without internet")
        Problems(state)
        val items = state.nearby.map { RadarItem(it.instance, it.name) }
        if (wide) {
            Row(horizontalArrangement = Arrangement.spacedBy(40.dp)) {
                Radar(items, Modifier.weight(1f).widthIn(max = 420.dp))
                Column(verticalArrangement = Arrangement.spacedBy(DSMetrics.sectionGap), modifier = Modifier.weight(1f)) {
                    ChooseFilesCard(onChooseFiles)
                    DeviceList(state, "Send to") { router.openSend(it) }
                }
            }
        } else {
            Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxWidth()) {
                Radar(items, Modifier.widthIn(max = 330.dp))
            }
            DeviceList(state, "Tap a device to send") { router.openSend(it) }
        }
    }
}

@Composable
private fun MoreMenu(onChooseFiles: () -> Unit) {
    val router = LocalRouter.current
    var open by remember { mutableStateOf(false) }
    Box {
        IconAction(
            DSIcons.more,
            "More",
            { open = true },
            tint = DS.colors.textPrimary,
            modifier = Modifier.clip(CircleShape).background(DS.colors.surface2).testTag("nearby.more"),
        )
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(
                text = { Text("Choose files to send") },
                leadingIcon = { Icon(painterResource(DSIcons.upload), contentDescription = null) },
                onClick = {
                    open = false
                    onChooseFiles()
                },
            )
            DropdownMenuItem(
                text = { Text("Connect by address") },
                leadingIcon = { Icon(painterResource(DSIcons.code), contentDescription = null) },
                onClick = {
                    open = false
                    router.push(Screen.ConnectByAddress)
                },
            )
            DropdownMenuItem(
                text = { Text("Pair a device") },
                leadingIcon = { Icon(painterResource(DSIcons.pair), contentDescription = null) },
                onClick = {
                    open = false
                    router.openPairing()
                },
            )
        }
    }
}

/** What keeps this device from being found, or from finding others. All text is the app's own. */
@Composable
private fun Problems(state: AppState) {
    val model = LocalModel.current
    val router = LocalRouter.current
    val scope = LocalAppScope.current
    val kind = if (LocalConfiguration.current.smallestScreenWidthDp >= 600) "tablet" else "phone"
    when {
        state.visibility == ReceiveVisibility.OFF -> ProblemCard(
            "Not receiving",
            "Other devices can't find or send to this one. You can still send.",
            "Turn on for paired devices",
        ) { scope.launch { model.setVisibility(ReceiveVisibility.PAIRED_ONLY) } }
        state.advertising is AdvertiseState.GateClosed -> ProblemCard(
            "Not listed nearby",
            "Other devices can't see this $kind on this Android version. " +
                "They can still send to it by typing its address.",
            "Show my address",
        ) { router.openThisDevice() }
        state.advertising is AdvertiseState.Failed -> ProblemCard(
            "Not listed nearby",
            when ((state.advertising as AdvertiseState.Failed).reason) {
                AdvertiseState.Reason.NO_IPV4_NETWORK ->
                    "Connect to a Wi-Fi network so other devices can see this $kind."
                AdvertiseState.Reason.RENAMED, AdvertiseState.Reason.REGISTRATION_FAILED ->
                    "Android could not list this $kind on the network. Other devices can still send to it by address."
            },
            "Show my address",
        ) { router.openThisDevice() }
    }
    state.discoveryProblem?.let { ProblemCard("Nearby devices can't be found", it) }
}

/** Tablets: the place to start a send without a device chosen. */
@Composable
private fun ChooseFilesCard(onChooseFiles: () -> Unit) {
    val c = DS.colors
    val shape = RoundedCornerShape(DSMetrics.cardRadius)
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterVertically),
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 180.dp)
            .clip(shape)
            .background(c.accentSoft)
            .border(1.5.dp, c.ring, shape)
            .padding(16.dp),
    ) {
        Box(
            contentAlignment = Alignment.Center,
            modifier = Modifier.size(56.dp).clip(CircleShape).background(c.surface),
        ) {
            Icon(painterResource(DSIcons.upload), contentDescription = null, tint = c.accent, modifier = Modifier.size(22.dp))
        }
        Text("Send files to a nearby device", style = DSType.sectionHeading, color = c.textPrimary)
        LinkButton("Choose files", onChooseFiles)
    }
}

@Composable
private fun DeviceList(state: AppState, title: String, onSend: (String) -> Unit) {
    val model = LocalModel.current
    val router = LocalRouter.current
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        SectionHeading(title)
        if (state.nearby.isEmpty()) {
            EmptyState(
                DSIcons.nearby,
                "Looking for devices…",
                "Open Dropumi on the other device and use the same Wi-Fi network. " +
                    "Devices that only accept paired devices still show up here.",
                modifier = Modifier.testTag("nearby.empty"),
            )
        }
        for (peer in state.nearby) {
            NearbyRow(
                peer,
                onSend = { onSend(peer.instance) },
                onPair = {
                    model.startPairing(SendTarget.Nearby(peer.instance), peer.name)
                    router.openPairing()
                },
            )
        }
        LinkButton("Pair a device", { router.openPairing() }, icon = DSIcons.pair, modifier = Modifier.testTag("nearby.pair"))
    }
}

@Composable
fun NearbyRow(peer: NearbyPeer, onSend: () -> Unit, onPair: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Box {
        DSRow(
            title = peer.name,
            subtitle = ScreenText.nearbySubtitle(peer),
            onClick = if (peer.compatible) onSend else null,
            onLongClick = if (peer.compatible) ({ menu = true }) else null,
            longClickLabel = "Pair with this device",
            leading = { DeviceAvatar() },
            modifier = Modifier.testTag("nearby.row"),
        ) {
            SoftPill(
                "Send",
                onSend,
                enabled = peer.compatible,
                modifier = Modifier.semantics { contentDescription = "Send to ${peer.name}" },
            )
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(
                text = { Text("Pair with this device") },
                leadingIcon = { Icon(painterResource(DSIcons.pair), contentDescription = null) },
                onClick = {
                    menu = false
                    onPair()
                },
            )
        }
    }
}
