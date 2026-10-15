// SPDX-License-Identifier: MPL-2.0
//
// Transfers (a port of the Apple app's TransfersScreen): running transfers
// with progress, then the history. "Done" is shown only for
// TransferFinished(COMPLETED); a partial transfer says so. Received files
// stay inside Dropumi (ADR-0013 D4) and leave only through an explicit
// Open, Share or Save on their row; Delete removes Dropumi's copies after a
// confirmation.
package io.github.dropumi.ui.screens

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.dropumi.ReceivedFileActions
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.TransferRecord
import io.github.dropumi.ui.Confirmation
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSProgressBar
import io.github.dropumi.ui.design.DSRow
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.EmptyState
import io.github.dropumi.ui.design.FileTile
import io.github.dropumi.ui.design.IconAction
import io.github.dropumi.ui.design.LinkButton
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SectionHeading
import io.github.dropumi.ui.design.SoftPill
import io.github.dropumi.ui.design.largeText
import kotlinx.coroutines.launch

@Composable
fun TransfersScreen(state: AppState) {
    val model = LocalModel.current
    val scope = LocalAppScope.current
    val confirmations = LocalConfirmations.current
    val files = LocalFileActions.current
    // API 26-28: Save asks for a folder first; the entry waits here meanwhile.
    var savingEntry by rememberSaveable { mutableStateOf<String?>(null) }
    val folderPicker = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val id = savingEntry
        savingEntry = null
        val tree = result.data?.data
        val entry = model.state.value.history.firstOrNull { it.id == id }
        if (result.resultCode == Activity.RESULT_OK && tree != null && entry != null) {
            scope.launch { model.saveToFolder(entry, tree.toString()) }
        }
    }
    val now = rememberNowMs(ticking = false)

    ScreenColumn {
        ScreenHeader("Transfers", ScreenText.transfersSubtitle(state, now))
        if (state.active.isEmpty() && state.history.isEmpty()) {
            EmptyState(
                DSIcons.transfers,
                "No transfers yet",
                "Files you send or receive show up here.",
                modifier = Modifier.testTag("transfers.empty"),
            )
        }
        for (record in state.active) TransferCard(record) { model.cancel(record) }
        if (state.history.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    SectionHeading("Earlier", modifier = Modifier.weight(1f))
                    LinkButton(
                        "Clear",
                        {
                            confirmations.ask(
                                Confirmation(
                                    title = "Clear the list?",
                                    message = "Files you received are deleted from Dropumi too. " +
                                        "Files you saved to Downloads or another folder stay there.",
                                    confirmLabel = "Clear and delete",
                                    destructive = true,
                                ) { scope.launch { model.clearHistory() } },
                            )
                        },
                        modifier = Modifier.testTag("transfers.clear"),
                    )
                }
                for (entry in state.history) {
                    HistoryRow(
                        entry,
                        onOpen = { name, context -> open(files, name, model, context) },
                        onShare = { context -> share(files, entry, model, context) },
                        onSave = {
                            if (model.savesToDownloads) {
                                scope.launch { model.saveToDownloads(entry) }
                            } else {
                                savingEntry = entry.id
                                try {
                                    folderPicker.launch(files.pickSaveFolder())
                                } catch (_: ActivityNotFoundException) {
                                    savingEntry = null
                                    model.notice("Not saved", "No app on this device can choose a folder.")
                                }
                            }
                        },
                        onDelete = {
                            confirmations.ask(
                                Confirmation(
                                    title = "Delete these files?",
                                    message = "Dropumi's copies of the files from this transfer are deleted. " +
                                        "Copies you saved or shared elsewhere stay there.",
                                    confirmLabel = "Delete",
                                    destructive = true,
                                ) { scope.launch { model.deleteReceived(entry) } },
                            )
                        },
                    )
                }
            }
        }
    }
}

private fun open(files: ReceivedFileActions, savedAs: String, model: AppModel, context: Context) {
    val intent = files.open(savedAs)
    if (intent == null) {
        model.notice("Can't open", "This file is no longer in Dropumi.")
        return
    }
    start(context, intent, model, "No app on this device can open this file. Share it or save it instead.")
}

private fun share(files: ReceivedFileActions, entry: HistoryEntry, model: AppModel, context: Context) {
    val intent = files.share(entry.savedAs)
    if (intent == null) {
        model.notice("Can't share", "These files are no longer in Dropumi.")
        return
    }
    start(context, intent, model, "No app on this device can take these files.")
}

private fun start(context: Context, intent: Intent, model: AppModel, failure: String) {
    try {
        context.startActivity(intent)
    } catch (_: ActivityNotFoundException) {
        model.notice("Nothing to open it with", failure)
    }
}

/** One running transfer. */
@Composable
fun TransferCard(record: TransferRecord, onCancel: () -> Unit) {
    val c = DS.colors
    val shape = RoundedCornerShape(DSMetrics.cardRadius)
    val large = largeText()
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.border, shape)
            .padding(16.dp)
            .testTag("transfer.card"),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            FileTile(record.fileNames.firstOrNull().orEmpty())
            Column(verticalArrangement = Arrangement.spacedBy(2.dp), modifier = Modifier.weight(1f).semantics(mergeDescendants = true) {}) {
                Text(
                    ScreenText.transferTitle(record),
                    style = DSType.rowTitle,
                    color = c.textPrimary,
                    maxLines = if (large) Int.MAX_VALUE else 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(ScreenText.transferPeerLine(record), style = DSType.meta, color = c.textSecondary)
            }
            IconAction(DSIcons.close, "Cancel", onCancel, modifier = Modifier.testTag("transfer.cancel"))
        }
        DSProgressBar(record.fraction)
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.semantics(mergeDescendants = true) {}) {
            Text(ScreenText.transferStatus(record), style = DSType.meta, color = c.textSecondary, modifier = Modifier.weight(1f))
            Text(ScreenText.transferRate(record), style = DSType.smallButton, color = c.accentText)
        }
    }
}

/** One finished transfer; received files get Open, Share, Save and Delete. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun HistoryRow(
    entry: HistoryEntry,
    onOpen: (String, Context) -> Unit,
    onShare: (Context) -> Unit,
    onSave: () -> Unit,
    onDelete: () -> Unit,
) {
    val c = DS.colors
    val context = LocalContext.current
    val received = !entry.outgoing && entry.savedAs.isNotEmpty()
    var expanded by rememberSaveable(entry.id) { mutableStateOf(false) }
    Column {
        DSRow(
            title = ScreenText.historyTitle(entry),
            subtitle = ScreenText.historySubtitle(entry),
            onClick = if (received) ({ expanded = !expanded }) else null,
            leading = { FileTile(entry.fileNames.firstOrNull().orEmpty(), muted = true) },
            modifier = Modifier
                .semantics { if (received) stateDescription = if (expanded) "Actions shown" else "Tap for actions" }
                .testTag("history.row"),
        ) {
            val (icon, tint) = when (entry.outcome) {
                HistoryEntry.Outcome.COMPLETED -> DSIcons.check to c.success
                HistoryEntry.Outcome.PARTIAL -> DSIcons.partial to c.danger
                else -> DSIcons.failed to c.textSecondary
            }
            Icon(
                painterResource(icon),
                contentDescription = ScreenText.outcomeLabel(entry.outcome),
                tint = tint,
                modifier = Modifier.size(22.dp),
            )
        }
        if (received && expanded) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.padding(start = 58.dp, top = 8.dp, bottom = 12.dp)) {
                if (entry.savedAs.size > 1) {
                    for (name in entry.savedAs) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .fillMaxWidth()
                                .heightIn(min = 48.dp)
                                .clickable(role = Role.Button, onClickLabel = "Open") { onOpen(name, context) },
                        ) {
                            Text(name, style = DSType.body, color = c.textPrimary, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                            Text("Open", style = DSType.smallButton, color = c.accentText, modifier = Modifier.padding(horizontal = 12.dp))
                        }
                    }
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    if (entry.savedAs.size == 1) SoftPill("Open", { onOpen(entry.savedAs.first(), context) }, modifier = Modifier.testTag("history.open"))
                    SoftPill("Share", { onShare(context) }, modifier = Modifier.testTag("history.share"))
                    SoftPill(
                        if (LocalModel.current.savesToDownloads) "Save to Downloads" else "Save to…",
                        onSave,
                        modifier = Modifier.testTag("history.save"),
                    )
                    SoftPill("Delete", onDelete, modifier = Modifier.testTag("history.delete"))
                }
            }
        }
    }
}
