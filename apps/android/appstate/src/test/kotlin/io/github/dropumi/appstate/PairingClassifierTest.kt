// SPDX-License-Identifier: MPL-2.0
//
// The Apple app's PairingClassifierTests, ported one to one (same names):
// how an ended pairing operation reads, and what the pairing screen does
// next. Pure; no core.

package io.github.dropumi.appstate

import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.PeerInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PairingClassifierTest {
    private val listed = "Maya's phone"

    private val everyCancelReason: List<PairingCancelReason?> = listOf(null) + PairingCancelReason.entries

    private fun peer(name: String, trusted: Boolean = false, alias: String? = null) =
        PeerInfo("ZYXWVUTSRQPONMLKJIHGFEDCBA765432ZYXWVUTSRQPONMLKJIHG", "ZYXW-VUTS-RQPO-NMLK-JIHG-FEDC", trusted, alias, name)

    /** This device asked [listed]. */
    private fun request(
        result: PairingResult,
        reachedCode: Boolean = false,
        localAnswer: Boolean? = null,
        localCodeExpired: Boolean = false,
        cancelReason: PairingCancelReason? = null,
    ) = PairingFacts(
        role = PairingSession.Role.Initiator(listed),
        result = result,
        reachedCode = reachedCode,
        localAnswer = localAnswer,
        localCodeExpired = localCodeExpired,
        cancelReason = cancelReason,
        name = listed,
    )

    /** This device's window, open and listening unless told otherwise. */
    private fun window(
        result: PairingResult,
        reachedCode: Boolean = false,
        localAnswer: Boolean? = null,
        localCodeExpired: Boolean = false,
        windowElapsed: Boolean = false,
        listening: Boolean = true,
        cancelReason: PairingCancelReason? = null,
        name: String? = null,
    ) = PairingFacts(
        role = PairingSession.Role.Window,
        result = result,
        reachedCode = reachedCode,
        localAnswer = localAnswer,
        localCodeExpired = localCodeExpired,
        windowElapsed = windowElapsed,
        listening = listening,
        cancelReason = cancelReason,
        name = name,
    )

    private fun classify(facts: PairingFacts) = PairingClassifier.classify(facts)

    private fun failed(code: ErrorCode) = PairingResult.Failed(code)

    private val PairingVerdict.waitsAgain: Boolean get() = this is PairingVerdict.WaitAgain

    private val PairingVerdict.outcome: PairingOutcome?
        get() = when (this) {
            PairingVerdict.Silent -> null
            is PairingVerdict.WaitAgain -> note
            is PairingVerdict.Paused -> outcome
            is PairingVerdict.Finished -> outcome
        }

    /** Everything the user can read for it. */
    private val PairingOutcome.allText: List<String> get() = listOfNotNull(text.title, text.message, noteText)

    /** One of each outcome, with a failure for every error code. */
    private val everyOutcome: List<PairingOutcome>
        get() = listOf(
            PairingOutcome.Paired(peer(listed, trusted = true, alias = listed)),
            PairingOutcome.YouDeclined,
            PairingOutcome.CodeExpired,
            PairingOutcome.OtherDeclined(listed),
            PairingOutcome.OtherDeclined(null),
            PairingOutcome.ConnectionLost(listed),
            PairingOutcome.ConnectionLost(null),
            PairingOutcome.NotWaiting(listed),
            PairingOutcome.Busy(listed),
            PairingOutcome.Unreachable(listed),
            PairingOutcome.NoLongerNearby(listed),
            PairingOutcome.CouldNotStart("another operation is running"),
            PairingOutcome.TimeUp,
            PairingOutcome.UsedUp,
            PairingOutcome.CouldNotWait("address in use"),
            PairingOutcome.Cancelled,
        ) + ErrorCode.entries.flatMap { listOf(PairingOutcome.Failed(it, listed), PairingOutcome.Failed(it, null)) }

    // Results after a code

    @Test
    fun pairedIsFinishedAndNeverReopens() {
        val paired = peer(listed, trusted = true, alias = listed)
        for (reachedCode in listOf(false, true)) {
            for (reason in everyCancelReason) {
                val expected = PairingVerdict.Finished(PairingOutcome.Paired(paired))
                assertEquals(expected, classify(request(PairingResult.Paired(paired), reachedCode, cancelReason = reason)))
                assertEquals(expected, classify(window(PairingResult.Paired(paired), reachedCode, cancelReason = reason)))
            }
        }
    }

    @Test
    fun localNoIsYouDeclined() {
        for (code in listOf(ErrorCode.PAIRING_REJECTED, ErrorCode.SAS_REJECTED)) {
            for (expired in listOf(false, true)) {
                val expected = PairingVerdict.Finished(PairingOutcome.YouDeclined)
                assertEquals(expected, classify(request(failed(code), true, localAnswer = false, localCodeExpired = expired)))
                assertEquals(expected, classify(window(failed(code), true, localAnswer = false, localCodeExpired = expired)))
            }
        }
    }

    @Test
    fun localExpiryIsCodeExpired() {
        for (code in listOf(ErrorCode.PAIRING_REJECTED, ErrorCode.TIMEOUT)) {
            val expected = PairingVerdict.Finished(PairingOutcome.CodeExpired)
            assertEquals(expected, classify(request(failed(code), true, localCodeExpired = true)))
            assertEquals(expected, classify(window(failed(code), true, localCodeExpired = true)))
        }
    }

    @Test
    fun rejectionAfterLocalYesIsOtherDeclined() {
        val rejected = failed(ErrorCode.PAIRING_REJECTED)
        // A "yes" here means the other device said no, even if the code ran out meanwhile.
        for (expired in listOf(false, true)) {
            assertEquals(
                PairingVerdict.Finished(PairingOutcome.OtherDeclined(listed)),
                classify(request(rejected, true, localAnswer = true, localCodeExpired = expired)),
            )
            assertEquals(
                PairingVerdict.Finished(PairingOutcome.OtherDeclined(null)),
                classify(window(rejected, true, localAnswer = true, localCodeExpired = expired)),
            )
        }
    }

    @Test
    fun rejectionBeforeLocalAnswerIsOtherDeclined() {
        for (code in listOf(ErrorCode.PAIRING_REJECTED, ErrorCode.TIMEOUT)) {
            assertEquals(PairingVerdict.Finished(PairingOutcome.OtherDeclined(listed)), classify(request(failed(code), true)))
            assertEquals(PairingVerdict.Finished(PairingOutcome.OtherDeclined(null)), classify(window(failed(code), true)))
        }
    }

    /**
     * The invariant: an attempt that showed a code never makes this device
     * ready to pair again by itself, whatever ended it.
     */
    @Test
    fun attemptThatReachedACodeNeverWaitsAgain() {
        val results = ErrorCode.entries.map(::failed) + PairingResult.Cancelled
        for (result in results) {
            for (answer in listOf(null, true, false)) {
                for (expired in listOf(false, true)) {
                    for (reason in everyCancelReason) {
                        val asked = classify(request(result, true, answer, expired, reason))
                        assertFalse("request $result $reason", asked.waitsAgain)
                        for (elapsed in listOf(false, true)) {
                            for (listening in listOf(false, true)) {
                                val waited = classify(window(result, true, answer, expired, elapsed, listening, reason))
                                assertFalse("window $result $reason", waited.waitsAgain)
                            }
                        }
                        // A failure after a code always gets a card with Done.
                        if (result is PairingResult.Failed) assertTrue("request $result gave $asked", asked is PairingVerdict.Finished)
                    }
                }
            }
        }
    }

    // Requests before a code

    @Test
    fun notAuthorizedOnARequestIsNotWaitingAndWaitsAgain() {
        assertEquals(
            PairingVerdict.WaitAgain(PairingOutcome.NotWaiting(listed)),
            classify(request(failed(ErrorCode.NOT_AUTHORIZED))),
        )
    }

    @Test
    fun busyAndUnreachableRequestsWaitAgain() {
        assertEquals(PairingVerdict.WaitAgain(PairingOutcome.Busy(listed)), classify(request(failed(ErrorCode.BUSY))))
        assertEquals(PairingVerdict.WaitAgain(PairingOutcome.Unreachable(listed)), classify(request(failed(ErrorCode.CONNECTION_LOST))))
        assertEquals(PairingVerdict.WaitAgain(PairingOutcome.Unreachable(listed)), classify(request(failed(ErrorCode.TIMEOUT))))
        assertEquals(
            PairingVerdict.WaitAgain(PairingOutcome.Failed(ErrorCode.INCOMPATIBLE_VERSION, listed)),
            classify(request(failed(ErrorCode.INCOMPATIBLE_VERSION))),
        )
    }

    @Test
    fun securityFailureBeforeACodeNeedsATap() {
        for (code in listOf(ErrorCode.PEER_IDENTITY_MISMATCH, ErrorCode.HANDSHAKE_FAILED, ErrorCode.PROTOCOL_VIOLATION, ErrorCode.LIMIT_EXCEEDED)) {
            val verdict = classify(request(failed(code)))
            assertEquals(PairingVerdict.Finished(PairingOutcome.Failed(code, listed)), verdict)
            assertEquals(true, verdict.outcome?.text?.isSecurity)
        }
        // No danger-coloured failure is ever a passing note.
        for (code in ErrorCode.entries.filter { ErrorCatalog.text(it).isSecurity }) {
            assertFalse("$code", classify(request(failed(code))).waitsAgain)
        }
    }

    // This device's window before a code

    @Test
    fun strayOnTheWindowIsUsedUpAndNeedsStartAgain() {
        // A transfer to a window is denied with not_authorized; anything else
        // that ends a listening window before a code used it up too.
        for (code in ErrorCode.entries) {
            assertEquals("$code", PairingVerdict.Paused(PairingOutcome.UsedUp), classify(window(failed(code))))
        }
    }

    @Test
    fun windowTimeoutAtExpiryIsTimeUp() {
        assertEquals(PairingVerdict.Paused(PairingOutcome.TimeUp), classify(window(failed(ErrorCode.TIMEOUT), windowElapsed = true)))
    }

    @Test
    fun handshakeTimeoutBeforeExpiryIsUsedUp() {
        assertEquals(PairingVerdict.Paused(PairingOutcome.UsedUp), classify(window(failed(ErrorCode.TIMEOUT), windowElapsed = false)))
    }

    @Test
    fun windowThatNeverListenedCouldNotWait() {
        val message = ErrorCatalog.text(ErrorCode.INTERNAL).message
        assertEquals(
            PairingVerdict.Paused(PairingOutcome.CouldNotWait(message)),
            classify(window(failed(ErrorCode.INTERNAL), listening = false)),
        )
        assertEquals(
            PairingOutcome.CouldNotWait("a pairing window is already open"),
            PairingClassifier.classifyWindowError(DropumiException.Busy("a pairing window is already open")),
        )
    }

    // Cancels

    @Test
    fun selfMadeCancelsAreSilent() {
        for (reachedCode in listOf(false, true)) {
            for (reason in PairingCancelReason.entries) {
                assertEquals(PairingVerdict.Silent, classify(window(PairingResult.Cancelled, reachedCode, cancelReason = reason)))
            }
            for (reason in listOf(PairingCancelReason.LEAVING, PairingCancelReason.BACKGROUND, PairingCancelReason.DIALING)) {
                assertEquals(PairingVerdict.Silent, classify(request(PairingResult.Cancelled, reachedCode, cancelReason = reason)))
            }
        }
    }

    @Test
    fun userCancelOnRequestWaitsAgainWithANote() {
        val verdict = classify(request(PairingResult.Cancelled, cancelReason = PairingCancelReason.USER))
        assertEquals(PairingVerdict.WaitAgain(PairingOutcome.Cancelled), verdict)
        assertEquals("Pairing cancelled.", verdict.outcome?.noteText)
    }

    @Test
    fun userCancelAfterACodeNeedsATap() {
        assertEquals(
            PairingVerdict.Finished(PairingOutcome.Cancelled),
            classify(request(PairingResult.Cancelled, reachedCode = true, cancelReason = PairingCancelReason.USER)),
        )
    }

    @Test
    fun cancelThisDeviceDidNotAskForNeedsATap() {
        for (reachedCode in listOf(false, true)) {
            assertEquals(PairingVerdict.Finished(PairingOutcome.Cancelled), classify(request(PairingResult.Cancelled, reachedCode)))
            assertEquals(PairingVerdict.Finished(PairingOutcome.Cancelled), classify(window(PairingResult.Cancelled, reachedCode)))
        }
    }

    // Requests that never started

    @Test
    fun vanishedInstanceIsNoLongerNearby() {
        val gone = DropumiException.NotFound("that device is no longer nearby")
        assertEquals(PairingOutcome.NoLongerNearby(listed), PairingClassifier.classifyRequestError(gone, listed))
        val busy = DropumiException.Busy("another operation is running")
        val outcome = PairingClassifier.classifyRequestError(busy, listed)
        assertEquals(PairingOutcome.CouldNotStart("another operation is running"), outcome)
        assertEquals("Pairing didn't start. Another operation is running.", outcome.noteText)
    }

    // Texts

    @Test
    fun everyOutcomeHasPlainText() {
        for (outcome in everyOutcome) {
            assertTrue("$outcome", outcome.text.title.isNotEmpty() && outcome.text.message.isNotEmpty())
            for (text in outcome.allText) {
                for (word in listOf("window", "SAS", "fingerprint", "supported")) {
                    assertFalse("$outcome: $text", text.contains(word, ignoreCase = true))
                }
            }
        }
        // Notes are the outcomes that leave this device ready again.
        val notes = listOf(
            PairingOutcome.NotWaiting(listed),
            PairingOutcome.Busy(listed),
            PairingOutcome.Unreachable(listed),
            PairingOutcome.NoLongerNearby(listed),
            PairingOutcome.CouldNotStart("x"),
            PairingOutcome.Cancelled,
            PairingOutcome.Failed(ErrorCode.INTERNAL, listed),
        )
        for (note in notes) assertTrue("$note", note.noteText?.isNotEmpty() == true)
        for (card in listOf(PairingOutcome.YouDeclined, PairingOutcome.CodeExpired, PairingOutcome.OtherDeclined(null), PairingOutcome.TimeUp, PairingOutcome.UsedUp)) {
            assertNull("$card", card.noteText)
        }
    }

    @Test
    fun textsAreTheAgreedCopy() {
        val paired = peer("Studio Mac", trusted = true, alias = "Maya's phone")
        assertEquals("Paired with “Maya's phone”", PairingOutcome.Paired(paired).text.title)
        assertEquals(
            "“Maya's phone” isn't waiting to pair. Open Pair a device on “Maya's phone” too, then tap it here " +
                "again. If you both tapped at once, let only one of you tap.",
            PairingOutcome.NotWaiting(listed).noteText,
        )
        assertEquals("“Maya's phone” didn't confirm the code", PairingOutcome.OtherDeclined(listed).text.title)
        assertEquals("The other device didn't confirm the code", PairingOutcome.OtherDeclined(null).text.title)
        assertEquals(
            ErrorText("Time's up", "No device asked to pair in 5 minutes. Nothing was saved.", false),
            PairingOutcome.TimeUp.text,
        )
        assertEquals(
            ErrorText("Pairing didn't start", "A device connected, but not to pair. Nothing was saved.", false),
            PairingOutcome.UsedUp.text,
        )
        val mismatch = PairingOutcome.Failed(ErrorCode.PEER_IDENTITY_MISMATCH, listed).text
        assertEquals(ErrorCatalog.text(ErrorCode.PEER_IDENTITY_MISMATCH).title, mismatch.title)
        assertEquals(ErrorCatalog.text(ErrorCode.PEER_IDENTITY_MISMATCH).message + " Nothing was saved.", mismatch.message)
        assertTrue(mismatch.isSecurity)
        assertEquals(
            "Pairing didn't start. " + ErrorCatalog.text(ErrorCode.INTERNAL).message,
            PairingOutcome.Failed(ErrorCode.INTERNAL, listed).noteText,
        )
    }

    @Test
    fun youDeclinedNeverSaysDidNotConfirm() {
        for (text in PairingOutcome.YouDeclined.allText) {
            assertFalse(text.contains("didn't confirm", ignoreCase = true))
            assertFalse(text.contains("did not confirm", ignoreCase = true))
        }
    }

    /** Both devices get PAIRING_REJECTED, whoever said no. */
    @Test
    fun pairingRejectedTextBlamesNoDevice() {
        val text = ErrorCatalog.text(ErrorCode.PAIRING_REJECTED)
        assertEquals("Pairing not confirmed", text.title)
        assertEquals("The code was not confirmed on both devices. Nothing was saved.", text.message)
        assertFalse(text.message.contains("other device", ignoreCase = true))
    }

    @Test
    fun windowSideOutcomesNeverUseAClaimedName() {
        val claim = "Mallory's laptop"
        val results = ErrorCode.entries.map(::failed) + PairingResult.Cancelled
        for (result in results) {
            for (reachedCode in listOf(false, true)) {
                for (answer in listOf(null, true, false)) {
                    for (reason in everyCancelReason) {
                        val verdict = classify(window(result, reachedCode, answer, cancelReason = reason, name = claim))
                        for (text in verdict.outcome?.allText.orEmpty()) assertFalse("$result: $text", text.contains("Mallory"))
                    }
                }
            }
        }
        assertEquals(
            PairingVerdict.Finished(PairingOutcome.OtherDeclined(null)),
            classify(window(failed(ErrorCode.PAIRING_REJECTED), reachedCode = true, localAnswer = true, name = claim)),
        )
        // Once paired, the card names the local alias, not the claim.
        val paired = peer(claim, trusted = true, alias = "Desk laptop")
        assertEquals("Paired with “Desk laptop”", PairingOutcome.Paired(paired).text.title)
    }
}
