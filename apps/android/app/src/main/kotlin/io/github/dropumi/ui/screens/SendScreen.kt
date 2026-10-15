// SPDX-License-Identifier: MPL-2.0
//
// Send (a port of the Apple app's SendScreen): the chosen files and where
// they go. The draft is filled only from the system file picker and from
// the in-process share hand-off (never from MainActivity's intent,
// ADR-0013 D6); every item passes UriGuard when it is described and again
// when it is opened. A device whose hint matches exactly one paired device
// is sent to with that device's key pinned; any other device is
// unverified, and both screens show a code to compare before anything is
// sent (`allowUnverified` only then).
package io.github.dropumi.ui.screens

import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.ffi.NearbyPeer
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ui.Screen
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSRow
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.EmptyState
import io.github.dropumi.ui.design.FileTile
import io.github.dropumi.ui.design.IconAction
import io.github.dropumi.ui.design.LinkButton
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionHeading
import kotlinx.coroutines.launch

@Composable
fun SendScreen(state: AppState, preselected: String?, picking: DocumentPicking) {
    val model = LocalModel.current
    val router = LocalRouter.current
    val scope = LocalAppScope.current
    var selected by remember(preselected) { mutableStateOf(preselected) }
    var sending by remember { mutableStateOf(false) }
    BrowseWhileVisible(model)
    val peer = state.nearby.firstOrNull { it.instance == selected }

    ScreenColumn {
        Row(verticalAlignment = Alignment.Top) {
            ScreenHeader("Send", draftSubtitle(state), modifier = Modifier.weight(1f))
            if (state.draft.isNotEmpty()) {
                LinkButton("Clear all", { model.clearDraft() }, modifier = Modifier.testTag("send.clear"))
            }
        }
        Files(state, picking.preparing)
        LinkButton("Add files", { picking.pick(openSend = false) }, icon = DSIcons.add, modifier = Modifier.testTag("send.addFiles"))
        Targets(state, selected, peer) { selected = it }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            PrimaryButton(
                text = if (peer != null) "Send to ${peer.name}" else "Choose a device",
                onClick = {
                    val target = peer ?: return@PrimaryButton
                    sending = true
                    scope.launch {
                        model.sendDraft(
                            SendTarget.Nearby(target.instance),
                            target.name,
                            allowUnverified = target.hintAlias == null,
                        )
                        sending = false
                        if (model.state.value.draft.isEmpty()) router.openTransfers()
                    }
                },
                enabled = peer != null && peer.compatible && state.draft.isNotEmpty() && picking.preparing == 0 && !sending,
                icon = if (peer != null) DSIcons.send else null,
                modifier = Modifier.testTag("send.go"),
            )
            Caption("Sent directly over your local network")
        }
    }
}

private fun draftSubtitle(state: AppState): String {
    if (state.draft.isEmpty()) return "Choose files, then a device"
    val count = Formatting.fileCount(state.draft.size)
    val sizes = state.draft.map { it.size }
    if (sizes.any { it == null }) return "$count ready to go"
    val total = sizes.fold(0uL) { sum, size -> sum + (size ?: 0uL) }
    return "$count · ${Formatting.bytes(total)} ready to go"
}

@Composable
private fun Files(state: AppState, preparing: Int) {
    val model = LocalModel.current
    if (state.draft.isEmpty() && preparing == 0) {
        EmptyState(DSIcons.upload, "No files yet", "Add files to send them to a nearby device.")
        return
    }
    Column {
        for (file in state.draft) {
            DSRow(
                title = file.name,
                subtitle = file.size?.let { Formatting.bytes(it) } ?: "Size not known yet",
                leading = { FileTile(file.name) },
            ) {
                IconAction(DSIcons.close, "Remove ${file.name}", { model.removeFromDraft(file) })
            }
        }
        if (preparing > 0) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.padding(vertical = 12.dp),
            ) {
                CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp, color = DS.colors.accent)
                Caption("Preparing ${ScreenText.itemCount(preparing)}…")
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Targets(state: AppState, selected: String?, peer: NearbyPeer?, onSelect: (String) -> Unit) {
    val router = LocalRouter.current
    val c = DS.colors
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionHeading("Send to")
        if (state.nearby.isEmpty()) Caption("Looking for devices… Open Dropumi on the other device.")
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            for (candidate in state.nearby) {
                FilterChip(
                    selected = candidate.instance == selected,
                    onClick = { onSelect(candidate.instance) },
                    enabled = candidate.compatible,
                    label = {
                        Text(candidate.name, style = DSType.smallButton, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 220.dp))
                    },
                    leadingIcon = { Icon(painterResource(DSIcons.phone), contentDescription = null, modifier = Modifier.size(14.dp)) },
                    shape = CircleShape,
                    colors = FilterChipDefaults.filterChipColors(
                        containerColor = c.surface,
                        labelColor = c.textPrimary,
                        iconColor = c.textPrimary,
                        selectedContainerColor = c.accent,
                        selectedLabelColor = c.onAccent,
                        selectedLeadingIconColor = c.onAccent,
                    ),
                    modifier = Modifier.heightIn(min = 48.dp).testTag("send.target"),
                )
            }
            FilterChip(
                selected = false,
                onClick = { router.push(Screen.ConnectByAddress) },
                label = { Text("Address…", style = DSType.smallButton) },
                leadingIcon = { Icon(painterResource(DSIcons.code), contentDescription = null, modifier = Modifier.size(14.dp)) },
                shape = CircleShape,
                colors = FilterChipDefaults.filterChipColors(containerColor = c.surface, labelColor = c.textPrimary, iconColor = c.textPrimary),
                modifier = Modifier.heightIn(min = 48.dp).semantics { contentDescription = "Send to an address" },
            )
        }
        if (peer != null) {
            Caption(
                if (peer.hintAlias != null) {
                    "Dropumi checks that this is your paired device before sending."
                } else {
                    "This device isn't paired. You'll both see a code to compare before anything is sent."
                },
            )
        }
        Spacer(Modifier.fillMaxWidth())
    }
}
