// SPDX-License-Identifier: MPL-2.0

import DropumiBindings
import DropumiTestSupport
import Foundation
import Testing
@testable import DropumiAppState

private let listed = "Maya's iPhone"

/// Every `ErrorCode` the core can report.
private let everyErrorCode: [ErrorCode] = [
    .incompatibleVersion, .handshakeFailed, .peerIdentityMismatch, .unverifiedPeer, .notAuthorized,
    .pairingRejected, .sasRejected, .protocolViolation, .limitExceeded, .integrity, .insufficientSpace,
    .storage, .sourceChanged, .connectionLost, .timeout, .busy, .remoteError, .internal,
]

private let everyCancelReason: [PairingCancelReason?] = [nil, .user, .leaving, .background, .dialing]

/// This device asked `listed`.
private func request(
    _ result: PairingResult, reachedCode: Bool = false, localAnswer: Bool? = nil, localCodeExpired: Bool = false,
    cancelReason: PairingCancelReason? = nil
) -> PairingFacts {
    PairingFacts(
        role: .initiator(name: listed), result: result, reachedCode: reachedCode, localAnswer: localAnswer,
        localCodeExpired: localCodeExpired, cancelReason: cancelReason, name: listed)
}

/// This device's window, open and listening unless told otherwise.
private func window(
    _ result: PairingResult, reachedCode: Bool = false, localAnswer: Bool? = nil, localCodeExpired: Bool = false,
    windowElapsed: Bool = false, listening: Bool = true, cancelReason: PairingCancelReason? = nil,
    name: String? = nil
) -> PairingFacts {
    PairingFacts(
        role: .window, result: result, reachedCode: reachedCode, localAnswer: localAnswer,
        localCodeExpired: localCodeExpired, windowElapsed: windowElapsed, listening: listening,
        cancelReason: cancelReason, name: name)
}

private func classify(_ facts: PairingFacts) -> PairingVerdict {
    PairingClassifier.classify(facts)
}

private extension PairingVerdict {
    var waitsAgain: Bool {
        if case .waitAgain = self { return true } else { return false }
    }

    var isFinished: Bool {
        if case .finished = self { return true } else { return false }
    }

    var outcome: PairingOutcome? {
        switch self {
        case .silent: return nil
        case let .waitAgain(note): return note
        case let .paused(outcome), let .finished(outcome): return outcome
        }
    }
}

private extension PairingOutcome {
    /// Everything the user can read for it.
    var allText: [String] { [text.title, text.message, noteText].compactMap { $0 } }
}

/// One of each outcome, with a failure for every error code.
private let everyOutcome: [PairingOutcome] = [
    .paired(Fixtures.peer(name: listed, trusted: true, alias: listed)),
    .youDeclined, .codeExpired, .otherDeclined(name: listed), .otherDeclined(name: nil),
    .connectionLost(name: listed), .connectionLost(name: nil), .notWaiting(name: listed), .busy(name: listed),
    .unreachable(name: listed), .noLongerNearby(name: listed), .couldNotStart("another operation is running"),
    .timeUp, .usedUp, .couldNotWait("address in use"), .cancelled,
] + everyErrorCode.flatMap { [PairingOutcome.failed($0, name: listed), .failed($0, name: nil)] }

@Suite struct PairingClassifierTests {
    // MARK: Results after a code

    @Test func pairedIsFinishedAndNeverReopens() {
        let peer = Fixtures.peer(name: listed, trusted: true, alias: listed)
        for reachedCode in [false, true] {
            for reason in everyCancelReason {
                #expect(classify(request(.paired(peer: peer), reachedCode: reachedCode, cancelReason: reason))
                    == .finished(.paired(peer)))
                #expect(classify(window(.paired(peer: peer), reachedCode: reachedCode, cancelReason: reason))
                    == .finished(.paired(peer)))
            }
        }
    }

    @Test func localNoIsYouDeclined() {
        for code in [ErrorCode.pairingRejected, .sasRejected] {
            for expired in [false, true] {
                let result = PairingResult.failed(error: code)
                #expect(classify(request(result, reachedCode: true, localAnswer: false, localCodeExpired: expired))
                    == .finished(.youDeclined))
                #expect(classify(window(result, reachedCode: true, localAnswer: false, localCodeExpired: expired))
                    == .finished(.youDeclined))
            }
        }
    }

    @Test func localExpiryIsCodeExpired() {
        for code in [ErrorCode.pairingRejected, .timeout] {
            let result = PairingResult.failed(error: code)
            #expect(classify(request(result, reachedCode: true, localCodeExpired: true)) == .finished(.codeExpired))
            #expect(classify(window(result, reachedCode: true, localCodeExpired: true)) == .finished(.codeExpired))
        }
    }

    @Test func rejectionAfterLocalYesIsOtherDeclined() {
        let rejected = PairingResult.failed(error: .pairingRejected)
        // A "yes" here means the other device said no, even if the code ran
        // out meanwhile.
        for expired in [false, true] {
            #expect(classify(request(rejected, reachedCode: true, localAnswer: true, localCodeExpired: expired))
                == .finished(.otherDeclined(name: listed)))
            #expect(classify(window(rejected, reachedCode: true, localAnswer: true, localCodeExpired: expired))
                == .finished(.otherDeclined(name: nil)))
        }
    }

    @Test func rejectionBeforeLocalAnswerIsOtherDeclined() {
        for code in [ErrorCode.pairingRejected, .timeout] {
            let result = PairingResult.failed(error: code)
            #expect(classify(request(result, reachedCode: true)) == .finished(.otherDeclined(name: listed)))
            #expect(classify(window(result, reachedCode: true)) == .finished(.otherDeclined(name: nil)))
        }
    }

    /// The invariant: an attempt that showed a code never makes this device
    /// ready to pair again by itself, whatever ended it.
    @Test func attemptThatReachedACodeNeverWaitsAgain() {
        let results = everyErrorCode.map { PairingResult.failed(error: $0) } + [.cancelled]
        for result in results {
            for answer in [nil, true, false] as [Bool?] {
                for expired in [false, true] {
                    for reason in everyCancelReason {
                        let asked = classify(request(
                            result, reachedCode: true, localAnswer: answer, localCodeExpired: expired,
                            cancelReason: reason))
                        #expect(!asked.waitsAgain, "request \(result) \(String(describing: reason))")
                        for elapsed in [false, true] {
                            for listening in [false, true] {
                                let waited = classify(window(
                                    result, reachedCode: true, localAnswer: answer, localCodeExpired: expired,
                                    windowElapsed: elapsed, listening: listening, cancelReason: reason))
                                #expect(!waited.waitsAgain, "window \(result) \(String(describing: reason))")
                            }
                        }
                        if case .failed = result {
                            // A failure after a code always gets a card with Done.
                            #expect(asked.isFinished, "request \(result) gave \(asked)")
                        }
                    }
                }
            }
        }
    }

    // MARK: Requests before a code

    @Test func notAuthorizedOnARequestIsNotWaitingAndWaitsAgain() {
        #expect(classify(request(.failed(error: .notAuthorized))) == .waitAgain(note: .notWaiting(name: listed)))
    }

    @Test func busyAndUnreachableRequestsWaitAgain() {
        #expect(classify(request(.failed(error: .busy))) == .waitAgain(note: .busy(name: listed)))
        #expect(classify(request(.failed(error: .connectionLost))) == .waitAgain(note: .unreachable(name: listed)))
        #expect(classify(request(.failed(error: .timeout))) == .waitAgain(note: .unreachable(name: listed)))
        #expect(classify(request(.failed(error: .incompatibleVersion)))
            == .waitAgain(note: .failed(.incompatibleVersion, name: listed)))
    }

    @Test func securityFailureBeforeACodeNeedsATap() {
        for code in [ErrorCode.peerIdentityMismatch, .handshakeFailed, .protocolViolation, .limitExceeded] {
            let verdict = classify(request(.failed(error: code)))
            #expect(verdict == .finished(.failed(code, name: listed)))
            #expect(verdict.outcome?.text.isSecurity == true)
        }
        // No danger-coloured failure is ever a passing note.
        for code in everyErrorCode where ErrorCatalog.text(for: code).isSecurity {
            #expect(!classify(request(.failed(error: code))).waitsAgain, "\(code)")
        }
    }

    // MARK: This device's window before a code

    @Test func strayOnTheWindowIsUsedUpAndNeedsStartAgain() {
        // A transfer to a window is denied with not_authorized; anything else
        // that ends a listening window before a code used it up too.
        for code in everyErrorCode {
            let verdict = classify(window(.failed(error: code)))
            #expect(verdict == .paused(.usedUp), "\(code)")
        }
    }

    @Test func windowTimeoutAtExpiryIsTimeUp() {
        #expect(classify(window(.failed(error: .timeout), windowElapsed: true)) == .paused(.timeUp))
    }

    @Test func handshakeTimeoutBeforeExpiryIsUsedUp() {
        #expect(classify(window(.failed(error: .timeout), windowElapsed: false)) == .paused(.usedUp))
    }

    @Test func windowThatNeverListenedCouldNotWait() {
        let message = ErrorCatalog.text(for: .internal).message
        #expect(classify(window(.failed(error: .internal), listening: false)) == .paused(.couldNotWait(message)))
        #expect(PairingClassifier.classify(windowError: DropumiError.Busy(detail: "a pairing window is already open"))
            == .couldNotWait("a pairing window is already open"))
    }

    // MARK: Cancels

    @Test func selfMadeCancelsAreSilent() {
        for reachedCode in [false, true] {
            for reason in [PairingCancelReason.user, .leaving, .background, .dialing] {
                #expect(classify(window(.cancelled, reachedCode: reachedCode, cancelReason: reason)) == .silent)
            }
            for reason in [PairingCancelReason.leaving, .background, .dialing] {
                #expect(classify(request(.cancelled, reachedCode: reachedCode, cancelReason: reason)) == .silent)
            }
        }
    }

    @Test func userCancelOnRequestWaitsAgainWithANote() {
        let verdict = classify(request(.cancelled, cancelReason: .user))
        #expect(verdict == .waitAgain(note: .cancelled))
        #expect(verdict.outcome?.noteText == "Pairing cancelled.")
    }

    @Test func userCancelAfterACodeNeedsATap() {
        #expect(classify(request(.cancelled, reachedCode: true, cancelReason: .user)) == .finished(.cancelled))
    }

    @Test func cancelThisDeviceDidNotAskForNeedsATap() {
        for reachedCode in [false, true] {
            #expect(classify(request(.cancelled, reachedCode: reachedCode)) == .finished(.cancelled))
            #expect(classify(window(.cancelled, reachedCode: reachedCode)) == .finished(.cancelled))
        }
    }

    // MARK: Requests that never started

    @Test func vanishedInstanceIsNoLongerNearby() {
        let gone = DropumiError.NotFound(detail: "that device is no longer nearby")
        #expect(PairingClassifier.classify(requestError: gone, name: listed) == .noLongerNearby(name: listed))
        let busy = DropumiError.Busy(detail: "another operation is running")
        let outcome = PairingClassifier.classify(requestError: busy, name: listed)
        #expect(outcome == .couldNotStart("another operation is running"))
        #expect(outcome.noteText == "Pairing didn't start. Another operation is running.")
    }

    // MARK: Texts

    @Test func everyOutcomeHasPlainText() {
        for outcome in everyOutcome {
            #expect(!outcome.text.title.isEmpty && !outcome.text.message.isEmpty, "\(outcome)")
            for text in outcome.allText {
                for word in ["window", "SAS", "fingerprint", "supported"] {
                    #expect(!text.localizedCaseInsensitiveContains(word), "\(outcome): \(text)")
                }
            }
        }
        // Notes are the outcomes that leave this device ready again.
        let notes: [PairingOutcome] = [
            .notWaiting(name: listed), .busy(name: listed), .unreachable(name: listed),
            .noLongerNearby(name: listed), .couldNotStart("x"), .cancelled, .failed(.internal, name: listed),
        ]
        for note in notes {
            #expect(note.noteText?.isEmpty == false, "\(note)")
        }
        for card in [PairingOutcome.youDeclined, .codeExpired, .otherDeclined(name: nil), .timeUp, .usedUp] {
            #expect(card.noteText == nil, "\(card)")
        }
    }

    @Test func textsAreTheAgreedCopy() {
        let peer = Fixtures.peer(name: "Studio Mac", trusted: true, alias: "Maya's iPhone")
        #expect(PairingOutcome.paired(peer).text.title == "Paired with “Maya's iPhone”")
        #expect(PairingOutcome.notWaiting(name: listed).noteText == """
            “Maya's iPhone” isn't waiting to pair. Open Pair a device on “Maya's iPhone” too, then tap it here \
            again. If you both tapped at once, let only one of you tap.
            """)
        #expect(PairingOutcome.otherDeclined(name: listed).text.title == "“Maya's iPhone” didn't confirm the code")
        #expect(PairingOutcome.otherDeclined(name: nil).text.title == "The other device didn't confirm the code")
        #expect(PairingOutcome.timeUp.text == ErrorText(
            title: "Time's up", message: "No device asked to pair in 5 minutes. Nothing was saved.", isSecurity: false))
        #expect(PairingOutcome.usedUp.text == ErrorText(
            title: "Pairing didn't start", message: "A device connected, but not to pair. Nothing was saved.",
            isSecurity: false))
        let mismatch = PairingOutcome.failed(.peerIdentityMismatch, name: listed).text
        #expect(mismatch.title == ErrorCatalog.text(for: .peerIdentityMismatch).title)
        #expect(mismatch.message == ErrorCatalog.text(for: .peerIdentityMismatch).message + " Nothing was saved.")
        #expect(mismatch.isSecurity)
        #expect(PairingOutcome.failed(.internal, name: listed).noteText
            == "Pairing didn't start. " + ErrorCatalog.text(for: .internal).message)
    }

    @Test func youDeclinedNeverSaysDidNotConfirm() {
        for text in PairingOutcome.youDeclined.allText {
            #expect(!text.localizedCaseInsensitiveContains("didn't confirm"))
            #expect(!text.localizedCaseInsensitiveContains("did not confirm"))
        }
    }

    /// Both devices get `pairingRejected`, whoever said no.
    @Test func pairingRejectedTextBlamesNoDevice() {
        let text = ErrorCatalog.text(for: .pairingRejected)
        #expect(text.title == "Pairing not confirmed")
        #expect(text.message == "The code was not confirmed on both devices. Nothing was saved.")
        #expect(!text.message.localizedCaseInsensitiveContains("other device"))
    }

    @Test func windowSideOutcomesNeverUseAClaimedName() {
        let claim = "Mallory's Mac"
        let results = everyErrorCode.map { PairingResult.failed(error: $0) } + [.cancelled]
        for result in results {
            for reachedCode in [false, true] {
                for answer in [nil, true, false] as [Bool?] {
                    for reason in everyCancelReason {
                        let verdict = classify(window(
                            result, reachedCode: reachedCode, localAnswer: answer, cancelReason: reason, name: claim))
                        for text in verdict.outcome?.allText ?? [] {
                            #expect(!text.contains("Mallory"), "\(result): \(text)")
                        }
                    }
                }
            }
        }
        #expect(classify(window(.failed(error: .pairingRejected), reachedCode: true, localAnswer: true, name: claim))
            == .finished(.otherDeclined(name: nil)))
        // Once paired, the card names the local alias, not the claim.
        let peer = Fixtures.peer(name: claim, trusted: true, alias: "Desk Mac")
        let title = PairingOutcome.paired(peer).text.title
        #expect(title == "Paired with “Desk Mac”")
    }
}
