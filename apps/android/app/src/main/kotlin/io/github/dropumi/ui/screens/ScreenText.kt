// SPDX-License-Identifier: MPL-2.0
//
// The words the screens show, as plain functions of the app state, so the
// rules behind them are tested on the host JVM. Peer strings are inserted
// with string templates only, never as a format string or markup.
package io.github.dropumi.ui.screens

import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.TransferPhase
import io.github.dropumi.appstate.TransferRecord
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.NearbyPeer
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale
import kotlin.math.roundToInt

object ScreenText {
    /**
     * A nearby device's second line. A hint that matches a paired device only
     * means "looks like" until the handshake checks the key, so a nearby
     * device is never called "Paired" (ADR-0004).
     */
    fun nearbySubtitle(peer: NearbyPeer): String = when {
        !peer.compatible -> "Needs a newer version of Dropumi"
        peer.hintAlias != null -> "Looks like ${peer.hintAlias}"
        else -> "Not paired"
    }

    /** "1 device", "3 devices". */
    fun deviceCount(n: Int): String = if (n == 1) "1 device" else "$n devices"

    /** "2 items", for things still being prepared. */
    fun itemCount(n: Int): String = if (n == 1) "1 item" else "$n items"

    /** Seconds left in the Everyone window, or null when it is not running. */
    fun everyoneSecondsLeft(state: AppState, nowMs: Long): Long? {
        val until = state.receiver?.everyoneUntil ?: return null
        return maxOf(0L, (until.toLong() - nowMs) / 1000)
    }

    /** Who may send right now, as the status pill says it. */
    fun visibilityLabel(state: AppState, nowMs: Long): String {
        if (state.receiver == null) return if (state.visibility == ReceiveVisibility.OFF) "Not receiving" else "Starting…"
        return when (state.visibility) {
            ReceiveVisibility.OFF -> "Not receiving"
            ReceiveVisibility.PAIRED_ONLY -> "Paired devices only"
            ReceiveVisibility.EVERYONE ->
                everyoneSecondsLeft(state, nowMs)?.let { "Everyone · ${Formatting.countdown(it)}" } ?: "Everyone"
        }
    }

    /** The caption under "Who can send to this device". Receiving runs only while the app is open (2c-1). */
    fun visibilityCaption(state: AppState, nowMs: Long): String = when (state.visibility) {
        ReceiveVisibility.OFF -> "Not receiving. Other devices can't find or send to this one."
        ReceiveVisibility.PAIRED_ONLY ->
            if (state.advertising is AdvertiseState.GateClosed) {
                "Only paired devices can send. Only while Dropumi is open."
            } else {
                "Only paired devices can send. Others on the network can still see this device's name. " +
                    "Only while Dropumi is open."
            }
        ReceiveVisibility.EVERYONE -> {
            val left = everyoneSecondsLeft(state, nowMs)
            val window = if (left != null) Formatting.countdown(left) else "10 minutes"
            "Anyone nearby can ask to send for $window. You compare a code before accepting. Only while Dropumi is open."
        }
    }

    /** A transfer's title: its first file, "and N more", or a count. */
    fun transferTitle(record: TransferRecord): String {
        val first = record.fileNames.firstOrNull() ?: return Formatting.fileCount(record.fileCount)
        val more = maxOf(record.fileCount, record.fileNames.size) - 1
        return if (more > 0) "$first and $more more" else first
    }

    /** "Sending to ‹peer›" or "Receiving from ‹peer›", with the peer shown alias first. */
    fun transferPeerLine(record: TransferRecord): String {
        val name = record.peerLabel.title
        return if (record.direction == Direction.OUTGOING) "Sending to $name" else "Receiving from $name"
    }

    /** The status line of a running transfer. */
    fun transferStatus(record: TransferRecord): String = when (record.phase) {
        TransferPhase.Connecting -> "Connecting and checking the device…"
        TransferPhase.WaitingForAcceptance -> "Waiting for the other person to accept…"
        TransferPhase.WaitingForAnswer -> "Waiting for your answer…"
        TransferPhase.Transferring -> record.progress?.let {
            "${Formatting.bytes(it.bytesDone)} of ${Formatting.bytes(it.bytesTotal)}"
        } ?: "Starting…"
        is TransferPhase.Finished -> ""
    }

    /** "42% · 28 MB/s · 20 s left", once there is progress. */
    fun transferRate(record: TransferRecord): String {
        val fraction = record.fraction ?: return ""
        val parts = mutableListOf("${(fraction * 100).roundToInt()}%")
        record.bytesPerSecond?.let { parts += Formatting.speed(it) }
        record.secondsLeft?.let { parts += Formatting.timeLeft(it) }
        return parts.joinToString(" · ")
    }

    /** "N active · M finished today". */
    fun transfersSubtitle(state: AppState, nowMs: Long, zone: ZoneId = ZoneId.systemDefault()): String {
        val today = Instant.ofEpochMilli(nowMs).atZone(zone).toLocalDate()
        val finished = state.history.count { Instant.ofEpochMilli(it.finishedAtMs).atZone(zone).toLocalDate() == today }
        return "${state.active.size} active · $finished finished today"
    }

    /** A finished transfer's title. */
    fun historyTitle(entry: HistoryEntry): String {
        val first = entry.fileNames.firstOrNull() ?: return entry.summary
        val more = entry.fileNames.size - 1
        return if (more > 0) "$first and $more more" else first
    }

    /** "Received from ‹peer› · 14:05", plus the outcome when it is not a success. */
    fun historySubtitle(entry: HistoryEntry, zone: ZoneId = ZoneId.systemDefault(), locale: Locale = Locale.getDefault()): String {
        val time = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale)
            .format(Instant.ofEpochMilli(entry.finishedAtMs).atZone(zone))
        val who = if (entry.outgoing) "Sent to ${entry.peerName}" else "Received from ${entry.peerName}"
        return if (entry.outcome == HistoryEntry.Outcome.COMPLETED) "$who · $time" else "$who · $time\n${entry.summary}"
    }

    /** What TalkBack says for a finished transfer's outcome icon. */
    fun outcomeLabel(outcome: HistoryEntry.Outcome): String = when (outcome) {
        HistoryEntry.Outcome.COMPLETED -> "Done"
        HistoryEntry.Outcome.PARTIAL -> "Some files failed"
        HistoryEntry.Outcome.REJECTED -> "Declined"
        HistoryEntry.Outcome.CANCELLED -> "Cancelled"
        HistoryEntry.Outcome.FAILED -> "Failed"
    }

    /** A fingerprint in groups of four, and the same groups for TalkBack, read one group at a time. */
    fun spokenFingerprint(full: String): String = full.chunked(4).joinToString(", ") { it.toList().joinToString(" ") }
}
