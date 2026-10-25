// SPDX-License-Identifier: MPL-2.0
//
// One sentence per error, in plain words. Exception text is never logged
// (ADR-0013 D3): errors thrown by node calls are mapped here, and only the
// core's own detail sentences are ever shown.

package io.github.dropumi.appstate

import io.github.dropumi.core.files.DocumentException
import io.github.dropumi.ffi.CancelledBy
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.RejectCause
import io.github.dropumi.ffi.TransferResult
import io.github.dropumi.ffi.TransferStatus

data class ErrorText(
    val title: String,
    val message: String,
    /** Whether it signals a possible attack or impersonation. */
    val isSecurity: Boolean,
)

object ErrorCatalog {
    fun text(code: ErrorCode): ErrorText = when (code) {
        ErrorCode.INCOMPATIBLE_VERSION -> ErrorText(
            "Different Dropumi versions",
            "The other device runs a version of Dropumi this one cannot talk to. Update both.",
            false,
        )
        ErrorCode.HANDSHAKE_FAILED -> ErrorText(
            "Secure connection failed",
            "The devices could not set up an encrypted connection. Nothing was sent.",
            true,
        )
        ErrorCode.PEER_IDENTITY_MISMATCH -> ErrorText(
            "Not the device you paired with",
            "This device is not the paired device it claims to be. Nothing was sent.",
            true,
        )
        ErrorCode.UNVERIFIED_PEER -> ErrorText(
            "Not paired",
            "This device is not paired with yours. Pair first, or send once and compare a code.",
            false,
        )
        ErrorCode.NOT_AUTHORIZED -> ErrorText(
            "Only paired devices",
            "The other device only accepts files from devices it has paired with.",
            false,
        )
        // Both devices get this code, whoever said no, so it blames no one.
        ErrorCode.PAIRING_REJECTED -> ErrorText(
            "Pairing not confirmed",
            "The code was not confirmed on both devices. Nothing was saved.",
            false,
        )
        ErrorCode.SAS_REJECTED -> ErrorText(
            "Codes did not match",
            "The code was not confirmed, so nothing was sent or saved.",
            true,
        )
        ErrorCode.PROTOCOL_VIOLATION -> ErrorText(
            "Unexpected data",
            "The other device sent something Dropumi does not accept. The transfer stopped.",
            true,
        )
        ErrorCode.LIMIT_EXCEEDED -> ErrorText(
            "Too much at once",
            "The other device went past a safety limit. The transfer stopped.",
            true,
        )
        ErrorCode.INTEGRITY -> ErrorText(
            "File check failed",
            "A file did not match what was sent and was deleted.",
            true,
        )
        ErrorCode.INSUFFICIENT_SPACE -> ErrorText(
            "Not enough space",
            "There is not enough free space for these files.",
            false,
        )
        ErrorCode.STORAGE -> ErrorText(
            "Could not save",
            "A file could not be saved on this device.",
            false,
        )
        ErrorCode.SOURCE_CHANGED -> ErrorText(
            "A file changed",
            "A file changed while it was being sent, so it was not sent.",
            false,
        )
        ErrorCode.CONNECTION_LOST -> ErrorText(
            "Connection lost",
            "The connection dropped. Check that both devices are on the same network.",
            false,
        )
        ErrorCode.TIMEOUT -> ErrorText(
            "No answer",
            "The other device did not answer in time.",
            false,
        )
        ErrorCode.BUSY -> ErrorText(
            "Busy",
            "The other device is busy. Try again in a moment.",
            false,
        )
        ErrorCode.REMOTE_ERROR -> ErrorText(
            "The other device stopped",
            "The other device reported a problem and stopped the transfer.",
            false,
        )
        ErrorCode.INTERNAL -> ErrorText(
            "Something went wrong",
            "Dropumi hit an unexpected problem. Try again.",
            false,
        )
    }

    /** The headline for a finished transfer. */
    fun summary(result: TransferResult, direction: Direction): String = when (result.status) {
        TransferStatus.COMPLETED ->
            if (direction == Direction.OUTGOING) "Sent and checked" else "All files saved and checked"
        TransferStatus.COMPLETED_WITH_FAILURES ->
            "${result.filesStored} of ${result.filesStored + result.filesFailed} files saved"
        TransferStatus.REJECTED -> when (result.rejectCause) {
            RejectCause.TIMEOUT -> "Request expired"
            RejectCause.BUSY -> "The device was busy"
            else -> if (direction == Direction.OUTGOING) "Declined" else "You declined"
        }
        TransferStatus.CANCELLED ->
            if (result.cancelledBy == CancelledBy.LOCAL) "Cancelled by you" else "Cancelled by the other device"
        TransferStatus.FAILED -> text(result.error ?: ErrorCode.INTERNAL).title
    }

    /** A short message for an error thrown by a node call (or anything else). */
    fun message(error: Throwable): String {
        if (error is DocumentException) {
            return when (error.reason) {
                DocumentException.Reason.REFUSED -> "Dropumi can't send this item."
                DocumentException.Reason.TOO_LARGE -> "This item is too large to prepare for sending."
                DocumentException.Reason.UNREADABLE -> "This item could not be read."
            }
        }
        val e = error as? DropumiException ?: return GENERIC
        return when (e) {
            is DropumiException.InvalidInput -> e.detail
            is DropumiException.NotFound -> e.detail
            is DropumiException.Busy -> e.detail
            is DropumiException.InvalidState -> e.detail
            is DropumiException.DiscoveryUnavailable -> "Looking for nearby devices is not available right now."
            is DropumiException.Config ->
                "Dropumi's settings on this device could not be read. " +
                    "Reset this device's identity in Settings if this keeps happening."
            is DropumiException.PromptNotFound -> "That request is no longer waiting."
            is DropumiException.PromptKindMismatch -> "That answer does not fit this request."
            is DropumiException.ShutDown -> "Dropumi is shutting down."
            is DropumiException.Transfer -> text(e.code).message
            is DropumiException.Internal -> GENERIC
        }
    }

    private const val GENERIC = "Something went wrong. Try again."
}
