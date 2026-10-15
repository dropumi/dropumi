// SPDX-License-Identifier: MPL-2.0

package io.github.dropumi.appstate

import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.FileStatus
import io.github.dropumi.ffi.PeerInfo
import io.github.dropumi.ffi.Progress
import io.github.dropumi.ffi.TransferResult

/**
 * Where a transfer stands. Only [Finished] with `TransferStatus.COMPLETED` is
 * a success; everything before it is in progress.
 */
sealed interface TransferPhase {
    /** Sending: connecting and checking the other device. */
    data object Connecting : TransferPhase

    /** Sending: the offer was sent; the other person has not answered. */
    data object WaitingForAcceptance : TransferPhase

    /** Receiving: this device's user has not answered yet. */
    data object WaitingForAnswer : TransferPhase

    /** Data is moving. */
    data object Transferring : TransferPhase

    /** It ended. */
    data class Finished(val result: TransferResult) : TransferPhase
}

/** One stored or failed file of a transfer. */
data class FileResult(
    val index: UInt,
    val status: FileStatus,
    /** For received files: the final name in the receive folder. */
    val savedAs: String?,
    /** For received files: the BLAKE3 digest (hex). */
    val blake3: String?,
    /** For received files: the size the core verified and saved. */
    val size: ULong? = null,
)

/** One transfer, as its card shows it. */
data class TransferRecord(
    /** `out-<operation>` or `in-<transfer id>`. */
    val id: String,
    val direction: Direction,
    val operation: ULong?,
    val transferId: String? = null,
    /** The authenticated peer, once the channel is up. */
    val peer: PeerInfo? = null,
    /** What the UI calls the peer before authentication (the chosen device). */
    val targetName: String,
    val fileNames: List<String>,
    val fileCount: Int = fileNames.size,
    val totalBytes: ULong,
    val progress: Progress? = null,
    val phase: TransferPhase,
    val sessionCode: String? = null,
    val fileResults: List<FileResult> = emptyList(),
    val startedAtMs: Long,
    val finishedAtMs: Long? = null,
    /** Recent progress samples (time in ms, bytes done), for the speed. */
    val samples: List<Pair<Long, ULong>> = emptyList(),
) {
    /**
     * The name to show for the other device: the local alias first, else the
     * name it claims (labelled as a claim), else the chosen name.
     */
    val peerLabel: PeerLabel get() = peer?.let(PeerLabel::of) ?: PeerLabel(targetName, null, false)

    val isFinished: Boolean get() = phase is TransferPhase.Finished

    /** The result, once finished. */
    val result: TransferResult? get() = (phase as? TransferPhase.Finished)?.result

    /** Records progress and keeps a short window of samples for the speed. */
    fun recorded(progress: Progress, nowMs: Long): TransferRecord {
        val moving = phase == TransferPhase.WaitingForAcceptance || phase == TransferPhase.WaitingForAnswer
        val kept = (samples + (nowMs to progress.bytesDone)).filter { nowMs - it.first <= SPEED_WINDOW_MS }
        return copy(progress = progress, phase = if (moving) TransferPhase.Transferring else phase, samples = kept)
    }

    /** Bytes per second over the last few seconds, once two samples are at least a second apart. */
    val bytesPerSecond: Double?
        get() {
            val first = samples.firstOrNull() ?: return null
            val last = samples.last()
            val ms = last.first - first.first
            if (ms < 1000 || last.second < first.second) return null
            return (last.second - first.second).toDouble() / (ms / 1000.0)
        }

    /** Seconds left, once the speed is known. */
    val secondsLeft: Double?
        get() {
            val speed = bytesPerSecond?.takeIf { it > 0 } ?: return null
            val p = progress ?: return null
            val left = if (p.bytesTotal > p.bytesDone) p.bytesTotal - p.bytesDone else 0uL
            return left.toDouble() / speed
        }

    /** 0..1, or null before any progress. */
    val fraction: Double?
        get() {
            val p = progress?.takeIf { it.bytesTotal > 0uL } ?: return null
            return minOf(1.0, p.bytesDone.toDouble() / p.bytesTotal.toDouble())
        }

    private companion object {
        const val SPEED_WINDOW_MS = 3_000L
    }
}

/**
 * How to show a peer (ADR-0004): the local alias first; a name the peer
 * claims is shown as a claim; unverified peers are labelled. Strings are
 * kept verbatim and must be shown as literal text.
 */
data class PeerLabel(
    /** Alias, or "Unverified device". */
    val title: String,
    /** The name the peer claims, when different from the title. */
    val claim: String?,
    /** Whether the peer is a paired device. */
    val verified: Boolean,
) {
    companion object {
        const val UNVERIFIED = "Unverified device"

        fun of(peer: PeerInfo): PeerLabel {
            val alias = peer.alias
            return when {
                peer.trusted && alias != null ->
                    PeerLabel(alias, if (peer.claimedName == alias) null else peer.claimedName, true)
                peer.trusted -> PeerLabel(peer.claimedName, null, true)
                else -> PeerLabel(UNVERIFIED, peer.claimedName, false)
            }
        }
    }
}
