// SPDX-License-Identifier: MPL-2.0
//
// Pairing as people see it (a port of the Apple app's Pairing.swift): how an
// attempt ended, in plain words, and what the pairing screen does next.
// Pure (no I/O, no core calls), so every rule is tested on its own. Rules it
// keeps:
//
// - Only `PairingFinished(Paired)` means paired.
// - An attempt that reached a code never makes this device ready to pair
//   again without a tap.
// - A window never names the device that connected to it: any name it has
//   is only a claim.
// - The texts never say "window", "SAS" or "fingerprint".

package io.github.dropumi.appstate

import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.PeerInfo

/** Why this device cancelled one of its own pairing operations. */
enum class PairingCancelReason {
    /** Stop or Cancel. */
    USER,

    /** The pairing screen went away. */
    LEAVING,

    /** The app went to the background. */
    BACKGROUND,

    /** This device closed its window to ask another device. */
    DIALING,
}

/** How a pairing attempt ended, or why it did not start. */
sealed interface PairingOutcome {
    /** Both people confirmed the code; the device is now paired. */
    data class Paired(val peer: PeerInfo) : PairingOutcome

    /** The user of this device said the codes don't match. */
    data object YouDeclined : PairingOutcome

    /** Nobody confirmed the code before it expired. */
    data object CodeExpired : PairingOutcome

    /** The other device said no, stopped, or did not answer in time. Null when its name is not known or only claimed. */
    data class OtherDeclined(val name: String?) : PairingOutcome

    /** The connection dropped after the code appeared. */
    data class ConnectionLost(val name: String?) : PairingOutcome

    /** The device asked is not ready to pair. */
    data class NotWaiting(val name: String) : PairingOutcome

    /** The device asked is busy. */
    data class Busy(val name: String) : PairingOutcome

    /** The device asked could not be reached. */
    data class Unreachable(val name: String) : PairingOutcome

    /** The device tapped changed in the list (it may have just got ready). */
    data class NoLongerNearby(val name: String) : PairingOutcome

    /** The core did not start the request; its reason. */
    data class CouldNotStart(val reason: String) : PairingOutcome

    /** Nobody asked to pair before this device's time ran out. */
    data object TimeUp : PairingOutcome

    /** A device connected to this one, but not to pair. */
    data object UsedUp : PairingOutcome

    /** This device could not get ready to pair; the core's reason (the card's text does not show it). */
    data class CouldNotWait(val reason: String) : PairingOutcome

    /** Cancelled on this device. */
    data object Cancelled : PairingOutcome

    /** Any other failure. */
    data class Failed(val code: ErrorCode, val name: String?) : PairingOutcome

    /**
     * The result card: a title, a message, and whether it signals a possible
     * attack. Names are claims or aliases; show them verbatim.
     */
    val text: ErrorText
        get() = when (this) {
            is Paired -> ErrorText(
                "Paired with “${PeerLabel.of(peer).title}”",
                "You can now send files to each other without a code. Each time, Dropumi checks that it's the same device.",
                false,
            )
            YouDeclined -> ErrorText(
                "You said the codes don't match",
                "Nothing was saved. If the codes were different, another device may have answered. " +
                    "Pair only when both devices are in front of you.",
                false,
            )
            CodeExpired -> ErrorText("The code expired", "Nobody tapped Codes match in time. Nothing was saved.", false)
            is OtherDeclined -> ErrorText(
                name?.let { "“$it” didn't confirm the code" } ?: "The other device didn't confirm the code",
                "It said the codes don't match, stopped, or didn't answer in time. Nothing was saved.",
                false,
            )
            is ConnectionLost -> ErrorText(
                "Connection lost",
                "The connection dropped before pairing finished. Nothing was saved.",
                false,
            )
            // Shown as notes; as a card they keep the note's words.
            is NotWaiting, is Busy, is Unreachable, is NoLongerNearby -> ErrorText(DID_NOT_START, noteText.orEmpty(), false)
            is CouldNotStart -> ErrorText(DID_NOT_START, sentence(reason), false)
            TimeUp -> ErrorText("Time's up", "No device asked to pair in 5 minutes. Nothing was saved.", false)
            UsedUp -> ErrorText(DID_NOT_START, "A device connected, but not to pair. Nothing was saved.", false)
            is CouldNotWait -> ErrorText(
                "This device can't get ready to pair",
                "It may be busy with a transfer. Try again in a moment. You can still pair with a device in the list.",
                false,
            )
            Cancelled -> ErrorText("Pairing cancelled", NOTHING_SAVED, false)
            is Failed -> {
                val text = ErrorCatalog.text(code)
                // The catalog's pairing text says it already.
                val message = if (text.message.endsWith(NOTHING_SAVED)) text.message else text.message + " " + NOTHING_SAVED
                ErrorText(text.title, message, text.isSecurity)
            }
        }

    /** The note shown above the device list when this device is ready to pair again. Null for outcomes shown only as a card. */
    val noteText: String?
        get() = when (this) {
            is NotWaiting ->
                "“$name” isn't waiting to pair. Open Pair a device on “$name” too, then tap it here again. " +
                    "If you both tapped at once, let only one of you tap."
            is Busy -> "“$name” is busy. It may be sending or receiving files. Try again in a moment."
            is Unreachable -> "Couldn't reach “$name”. Check that both devices are on the same Wi-Fi network."
            is NoLongerNearby -> "“$name” changed in the list. It may have just got ready to pair. Tap it again."
            is CouldNotStart -> "Pairing didn't start. ${sentence(reason)}"
            Cancelled -> "Pairing cancelled."
            is Failed -> "Pairing didn't start. ${ErrorCatalog.text(code).message}"
            is Paired, YouDeclined, CodeExpired, is OtherDeclined, is ConnectionLost, TimeUp, UsedUp, is CouldNotWait -> null
        }
}

private const val DID_NOT_START = "Pairing didn't start"
private const val NOTHING_SAVED = "Nothing was saved."

/** The core's short reasons ("another operation is running") as a sentence. */
private fun sentence(reason: String): String {
    val text = reason.trim()
    val last = text.lastOrNull() ?: return text
    val capitalized = text.replaceFirstChar { it.uppercase() }
    return if (last in ".!?") capitalized else "$capitalized."
}

/** What the pairing screen shows. */
data class PairingFlow(
    val phase: Phase = Phase.Idle,
    /** A note above the device list (the device is ready again). */
    val note: PairingOutcome? = null,
) {
    sealed interface Phase {
        data object Idle : Phase

        /** This device is getting ready to pair. */
        data object Opening : Phase

        /** Ready to pair until [expiresAtMs] (the model's clock). */
        data class Waiting(val expiresAtMs: Long) : Phase

        /** Closing this device's window to ask [name]. */
        data class Switching(val name: String) : Phase

        /** Asking [name] to pair. */
        data class Asking(val name: String) : Phase

        /** A code is on screen. */
        data object Comparing : Phase

        /** Not ready to pair until the user taps. */
        data class Paused(val outcome: PairingOutcome) : Phase

        /** An attempt ended. */
        data class Finished(val outcome: PairingOutcome) : Phase
    }
}

/** What the model knows when a pairing operation ends. */
data class PairingFacts(
    val role: PairingSession.Role,
    val result: PairingResult,
    /** A code was seen for this operation (`SessionCode` or a code prompt). */
    val reachedCode: Boolean = false,
    /** What the user of this device answered on the code prompt. */
    val localAnswer: Boolean? = null,
    /** The code's time ran out here without an answer. */
    val localCodeExpired: Boolean = false,
    /** Window: its time is (about) up. */
    val windowElapsed: Boolean = false,
    /** Window: `PairingWindowOpened` was seen. */
    val listening: Boolean = false,
    /** Why this device cancelled the operation, if it did. */
    val cancelReason: PairingCancelReason? = null,
    /** Request: the name in the list. Window: null (only ever a claim). */
    val name: String? = null,
)

/** What the pairing screen does with an ended operation. */
sealed interface PairingVerdict {
    /** Nothing to show: this device ended it itself. */
    data object Silent : PairingVerdict

    /** Get ready to pair again, with a note. Only for a request that ended before a code. */
    data class WaitAgain(val note: PairingOutcome?) : PairingVerdict

    /** A card with one button; not ready to pair until it is tapped. */
    data class Paused(val outcome: PairingOutcome) : PairingVerdict

    /** A result card with Done and Try again. */
    data class Finished(val outcome: PairingOutcome) : PairingVerdict
}

/** Turns the facts of an ended pairing operation into what the user sees. */
object PairingClassifier {
    fun classify(facts: PairingFacts): PairingVerdict {
        val code = when (val result = facts.result) {
            is PairingResult.Paired -> return PairingVerdict.Finished(PairingOutcome.Paired(result.peer))
            PairingResult.Cancelled -> return cancelled(facts)
            is PairingResult.Failed -> result.error
        }
        return when (val role = facts.role) {
            // A window never names the device that connected: only a claim.
            PairingSession.Role.Window ->
                if (facts.reachedCode) {
                    PairingVerdict.Finished(afterCode(code, facts, name = null))
                } else {
                    PairingVerdict.Paused(windowBeforeCode(code, facts))
                }
            is PairingSession.Role.Initiator -> {
                val name = facts.name ?: role.name
                if (facts.reachedCode) PairingVerdict.Finished(afterCode(code, facts, name)) else requestBeforeCode(code, name)
            }
        }
    }

    /** A request that threw before it started. A vanished list entry is not an error the user caused. */
    fun classifyRequestError(error: Throwable, name: String): PairingOutcome =
        if (error is DropumiException.NotFound) {
            PairingOutcome.NoLongerNearby(name)
        } else {
            PairingOutcome.CouldNotStart(ErrorCatalog.message(error))
        }

    /** Opening this device's window threw. */
    fun classifyWindowError(error: Throwable): PairingOutcome = PairingOutcome.CouldNotWait(ErrorCatalog.message(error))

    private fun cancelled(facts: PairingFacts): PairingVerdict = when (facts.cancelReason) {
        PairingCancelReason.LEAVING, PairingCancelReason.BACKGROUND, PairingCancelReason.DIALING -> PairingVerdict.Silent
        // Stop on a window: the screen has already gone. Cancel while asking
        // makes this device ready again; once a code was shown, only a tap does.
        PairingCancelReason.USER -> when {
            facts.role !is PairingSession.Role.Initiator -> PairingVerdict.Silent
            facts.reachedCode -> PairingVerdict.Finished(PairingOutcome.Cancelled)
            else -> PairingVerdict.WaitAgain(PairingOutcome.Cancelled)
        }
        // Not cancelled by this app: say so, and wait for a tap.
        null -> PairingVerdict.Finished(PairingOutcome.Cancelled)
    }

    private fun afterCode(code: ErrorCode, facts: PairingFacts, name: String?): PairingOutcome {
        // The core reports every "no" (and a code that ran out) as a
        // rejection on both devices; only this device's answer tells who.
        if (code == ErrorCode.PAIRING_REJECTED || code == ErrorCode.SAS_REJECTED || code == ErrorCode.TIMEOUT) {
            if (facts.localAnswer == false) return PairingOutcome.YouDeclined
            if (facts.localAnswer == null && facts.localCodeExpired) return PairingOutcome.CodeExpired
            return PairingOutcome.OtherDeclined(name)
        }
        if (code == ErrorCode.CONNECTION_LOST) return PairingOutcome.ConnectionLost(name)
        return PairingOutcome.Failed(code, name)
    }

    private fun windowBeforeCode(code: ErrorCode, facts: PairingFacts): PairingOutcome {
        if (code == ErrorCode.TIMEOUT && facts.windowElapsed) return PairingOutcome.TimeUp
        if (!facts.listening) return PairingOutcome.CouldNotWait(ErrorCatalog.text(code).message)
        // Something else used the window (a transfer, a stalled handshake).
        // It is one-shot: it opens again only on "Start again".
        return PairingOutcome.UsedUp
    }

    private fun requestBeforeCode(code: ErrorCode, name: String): PairingVerdict = when (code) {
        ErrorCode.NOT_AUTHORIZED -> PairingVerdict.WaitAgain(PairingOutcome.NotWaiting(name))
        ErrorCode.BUSY -> PairingVerdict.WaitAgain(PairingOutcome.Busy(name))
        ErrorCode.CONNECTION_LOST, ErrorCode.TIMEOUT -> PairingVerdict.WaitAgain(PairingOutcome.Unreachable(name))
        // A possible attack is never a passing note.
        else -> if (ErrorCatalog.text(code).isSecurity) {
            PairingVerdict.Finished(PairingOutcome.Failed(code, name))
        } else {
            PairingVerdict.WaitAgain(PairingOutcome.Failed(code, name))
        }
    }
}
