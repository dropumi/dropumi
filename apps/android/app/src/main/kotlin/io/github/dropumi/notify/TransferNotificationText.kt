// SPDX-License-Identifier: MPL-2.0
//
// The words of the transfer service's notification, as plain functions of
// the running transfers, so the rules are tested on the host JVM. It holds
// counts and sizes only: never a peer name, alias, claimed name or file
// name, on the lock screen or off it (ADR-0013 D7; plan §6.6). The details
// are in Dropumi's Transfers screen.
package io.github.dropumi.notify

import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.TransferRecord
import io.github.dropumi.ffi.Direction

object TransferNotificationText {
    /** The lock screen's version: generic, the same for every transfer. */
    const val PUBLIC_TITLE = "Dropumi is transferring files"

    /** The notification channel's name in the system's settings. */
    const val CHANNEL_NAME = "Transfers"

    /** What the notification says, and its progress (0..[PROGRESS_MAX]; null while unknown). */
    data class Content(val title: String, val text: String, val progress: Int?)

    const val PROGRESS_MAX = 1000

    fun of(active: List<TransferRecord>): Content {
        if (active.isEmpty()) return Content(PUBLIC_TITLE, "", null)
        val outgoing = active.filter { it.direction == Direction.OUTGOING }
        val incoming = active.filter { it.direction == Direction.INCOMING }
        val title = when {
            incoming.isEmpty() -> "Sending " + Formatting.fileCount(outgoing.sumOf { it.fileCount })
            outgoing.isEmpty() -> "Receiving " + Formatting.fileCount(incoming.sumOf { it.fileCount })
            else -> "${active.size} transfers"
        }
        val moving = active.mapNotNull { it.progress }.filter { it.bytesTotal > 0uL }
        if (moving.isEmpty()) return Content(title, "Waiting…", null)
        val done = moving.fold(0uL) { sum, p -> sum + minOf(p.bytesDone, p.bytesTotal) }
        val total = moving.fold(0uL) { sum, p -> sum + p.bytesTotal }
        val progress = (done.toDouble() / total.toDouble() * PROGRESS_MAX).toInt().coerceIn(0, PROGRESS_MAX)
        return Content(title, "${Formatting.bytes(done)} of ${Formatting.bytes(total)}", progress)
    }
}
