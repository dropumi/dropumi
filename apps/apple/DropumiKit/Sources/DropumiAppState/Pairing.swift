// SPDX-License-Identifier: MPL-2.0
//
// Pairing as people see it: how an attempt ended, in plain words, and what
// the pairing screen does next. Pure (no I/O, no core calls), so every rule
// is tested on its own. Rules it keeps:
//
// - Only `PairingFinished(.paired)` means paired.
// - An attempt that reached a code never makes this device ready to pair
//   again without a tap.
// - A window never names the device that connected to it: any name it has
//   is only a claim.
// - The texts never say "window", "SAS" or "fingerprint".

import DropumiBindings
import Foundation

/// Why this device cancelled one of its own pairing operations.
public enum PairingCancelReason: Equatable, Sendable {
    /// Stop or Cancel.
    case user
    /// The pairing screen went away.
    case leaving
    /// The app went to the background.
    case background
    /// This device closed its window to ask another device.
    case dialing
}

/// How a pairing attempt ended, or why it did not start.
public enum PairingOutcome: Equatable, Sendable {
    /// Both people confirmed the code; the device is now paired.
    case paired(PeerInfo)
    /// The user of this device said the codes don't match.
    case youDeclined
    /// Nobody confirmed the code before it expired.
    case codeExpired
    /// The other device said no, stopped, or did not answer in time. Nil
    /// when its name is not known or only claimed.
    case otherDeclined(name: String?)
    /// The connection dropped after the code appeared.
    case connectionLost(name: String?)
    /// The device asked is not ready to pair.
    case notWaiting(name: String)
    /// The device asked is busy.
    case busy(name: String)
    /// The device asked could not be reached.
    case unreachable(name: String)
    /// The device tapped changed in the list (it may have just got ready).
    case noLongerNearby(name: String)
    /// The core did not start the request; its reason.
    case couldNotStart(String)
    /// Nobody asked to pair before this device's time ran out.
    case timeUp
    /// A device connected to this one, but not to pair.
    case usedUp
    /// This device could not get ready to pair; the core's reason (the card's
    /// text does not show it).
    case couldNotWait(String)
    /// Cancelled on this device.
    case cancelled
    /// Any other failure.
    case failed(ErrorCode, name: String?)
}

extension PairingOutcome {
    /// The result card: a title, a message, and whether it signals a possible
    /// attack. Names are claims or aliases; show them verbatim.
    public var text: ErrorText {
        switch self {
        case let .paired(peer):
            let alias = PeerLabel(peer: peer).title
            return ErrorText(
                title: String(localized: "Paired with “\(alias)”"),
                message: String(localized: "You can now send files to each other without a code. Each time, Dropumi checks that it's the same device."),
                isSecurity: false)
        case .youDeclined:
            return ErrorText(
                title: String(localized: "You said the codes don't match"),
                message: String(localized: "Nothing was saved. If the codes were different, another device may have answered. Pair only when both devices are in front of you."),
                isSecurity: false)
        case .codeExpired:
            return ErrorText(title: String(localized: "The code expired"),
                             message: String(localized: "Nobody tapped Codes match in time. Nothing was saved."),
                             isSecurity: false)
        case let .otherDeclined(name):
            return ErrorText(
                title: name.map { String(localized: "“\($0)” didn't confirm the code") }
                    ?? String(localized: "The other device didn't confirm the code"),
                message: String(localized: "It said the codes don't match, stopped, or didn't answer in time. Nothing was saved."),
                isSecurity: false)
        case .connectionLost:
            return ErrorText(title: String(localized: "Connection lost"),
                             message: String(localized: "The connection dropped before pairing finished. Nothing was saved."),
                             isSecurity: false)
        case .notWaiting, .busy, .unreachable, .noLongerNearby:
            // Shown as notes; as a card they keep the note's words.
            return ErrorText(title: Self.didNotStart, message: noteText ?? "", isSecurity: false)
        case let .couldNotStart(reason):
            return ErrorText(title: Self.didNotStart, message: Self.sentence(reason), isSecurity: false)
        case .timeUp:
            return ErrorText(title: String(localized: "Time's up"),
                             message: String(localized: "No device asked to pair in 5 minutes. Nothing was saved."),
                             isSecurity: false)
        case .usedUp:
            return ErrorText(title: Self.didNotStart,
                             message: String(localized: "A device connected, but not to pair. Nothing was saved."),
                             isSecurity: false)
        case .couldNotWait:
            return ErrorText(
                title: String(localized: "This device can't get ready to pair"),
                message: String(localized: "It may be busy with a transfer. Try again in a moment. You can still pair with a device in the list."),
                isSecurity: false)
        case .cancelled:
            return ErrorText(title: String(localized: "Pairing cancelled"),
                             message: String(localized: "Nothing was saved."),
                             isSecurity: false)
        case let .failed(code, _):
            let text = ErrorCatalog.text(for: code)
            let saved = String(localized: "Nothing was saved.")
            // The catalog's pairing text says it already.
            let message = text.message.hasSuffix(saved) ? text.message : text.message + " " + saved
            return ErrorText(title: text.title, message: message, isSecurity: text.isSecurity)
        }
    }

    /// The note shown above the device list when this device is ready to
    /// pair again. Nil for outcomes shown only as a card.
    public var noteText: String? {
        switch self {
        case let .notWaiting(name):
            return String(localized: "“\(name)” isn't waiting to pair. Open Pair a device on “\(name)” too, then tap it here again. If you both tapped at once, let only one of you tap.")
        case let .busy(name):
            return String(localized: "“\(name)” is busy. It may be sending or receiving files. Try again in a moment.")
        case let .unreachable(name):
            return String(localized: "Couldn't reach “\(name)”. Check that both devices are on the same Wi-Fi network.")
        case let .noLongerNearby(name):
            return String(localized: "“\(name)” changed in the list. It may have just got ready to pair. Tap it again.")
        case let .couldNotStart(reason):
            return String(localized: "Pairing didn't start. \(Self.sentence(reason))")
        case .cancelled:
            return String(localized: "Pairing cancelled.")
        case let .failed(code, _):
            return String(localized: "Pairing didn't start. \(ErrorCatalog.text(for: code).message)")
        case .paired, .youDeclined, .codeExpired, .otherDeclined, .connectionLost, .timeUp, .usedUp, .couldNotWait:
            return nil
        }
    }

    private static var didNotStart: String { String(localized: "Pairing didn't start") }

    /// The core's short reasons ("another operation is running") as a
    /// sentence.
    private static func sentence(_ reason: String) -> String {
        let text = reason.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let last = text.last else { return text }
        let capitalized = text.prefix(1).uppercased() + text.dropFirst()
        return ".!?".contains(last) ? capitalized : capitalized + "."
    }
}

/// What the pairing screen shows.
public struct PairingFlow: Equatable, Sendable {
    public enum Phase: Equatable, Sendable {
        case idle
        /// This device is getting ready to pair.
        case opening
        /// Ready to pair until `expiresAt`.
        case waiting(expiresAt: Date)
        /// Closing this device's window to ask `name`.
        case switching(name: String)
        /// Asking `name` to pair.
        case asking(name: String)
        /// A code is on screen.
        case comparing
        /// Not ready to pair until the user taps.
        case paused(PairingOutcome)
        /// An attempt ended.
        case finished(PairingOutcome)
    }

    public var phase: Phase
    /// A note above the device list (the device is ready again).
    public var note: PairingOutcome?

    public init(phase: Phase = .idle, note: PairingOutcome? = nil) {
        self.phase = phase
        self.note = note
    }
}

/// What the model knows when a pairing operation ends.
public struct PairingFacts: Equatable, Sendable {
    public var role: PairingSession.Role
    public var result: PairingResult
    /// A code was seen for this operation (`SessionCode` or a code prompt).
    public var reachedCode: Bool
    /// What the user of this device answered on the code prompt.
    public var localAnswer: Bool?
    /// The code's time ran out here without an answer.
    public var localCodeExpired: Bool
    /// Window: its time is (about) up.
    public var windowElapsed: Bool
    /// Window: `PairingWindowOpened` was seen.
    public var listening: Bool
    /// Why this device cancelled the operation, if it did.
    public var cancelReason: PairingCancelReason?
    /// Request: the name in the list. Window: nil (only ever a claim).
    public var name: String?

    public init(
        role: PairingSession.Role, result: PairingResult, reachedCode: Bool = false, localAnswer: Bool? = nil,
        localCodeExpired: Bool = false, windowElapsed: Bool = false, listening: Bool = false,
        cancelReason: PairingCancelReason? = nil, name: String? = nil
    ) {
        self.role = role
        self.result = result
        self.reachedCode = reachedCode
        self.localAnswer = localAnswer
        self.localCodeExpired = localCodeExpired
        self.windowElapsed = windowElapsed
        self.listening = listening
        self.cancelReason = cancelReason
        self.name = name
    }
}

/// What the pairing screen does with an ended operation.
public enum PairingVerdict: Equatable, Sendable {
    /// Nothing to show: this device ended it itself.
    case silent
    /// Get ready to pair again, with a note. Only for a request that ended
    /// before a code.
    case waitAgain(note: PairingOutcome?)
    /// A card with one button; not ready to pair until it is tapped.
    case paused(PairingOutcome)
    /// A result card with Done and Try again.
    case finished(PairingOutcome)
}

/// Turns the facts of an ended pairing operation into what the user sees.
public enum PairingClassifier {
    public static func classify(_ facts: PairingFacts) -> PairingVerdict {
        let code: ErrorCode
        switch facts.result {
        case let .paired(peer):
            return .finished(.paired(peer))
        case .cancelled:
            return cancelled(facts)
        case let .failed(error):
            code = error
        }
        switch facts.role {
        case .window:
            // A window never names the device that connected: only a claim.
            if facts.reachedCode { return .finished(afterCode(code, facts, name: nil)) }
            return .paused(windowBeforeCode(code, facts))
        case let .initiator(listed):
            let name = facts.name ?? listed
            if facts.reachedCode { return .finished(afterCode(code, facts, name: name)) }
            return requestBeforeCode(code, name: name)
        }
    }

    /// A request that threw before it started. A vanished list entry is not
    /// an error the user caused.
    public static func classify(requestError error: Error, name: String) -> PairingOutcome {
        if let error = error as? DropumiError, case .NotFound = error {
            return .noLongerNearby(name: name)
        }
        return .couldNotStart(ErrorCatalog.message(for: error))
    }

    /// Opening this device's window threw.
    public static func classify(windowError error: Error) -> PairingOutcome {
        .couldNotWait(ErrorCatalog.message(for: error))
    }

    private static func cancelled(_ facts: PairingFacts) -> PairingVerdict {
        switch facts.cancelReason {
        case .leaving, .background, .dialing:
            return .silent
        case .user:
            // Stop on a window: the screen has already gone.
            guard case .initiator = facts.role else { return .silent }
            // Cancel while asking makes this device ready again; once a code
            // was shown, only a tap does.
            return facts.reachedCode ? .finished(.cancelled) : .waitAgain(note: .cancelled)
        case nil:
            // Not cancelled by this app: say so, and wait for a tap.
            return .finished(.cancelled)
        }
    }

    private static func afterCode(_ code: ErrorCode, _ facts: PairingFacts, name: String?) -> PairingOutcome {
        // The core reports every "no" (and a code that ran out) as a
        // rejection on both devices; only this device's answer tells who.
        if code == .pairingRejected || code == .sasRejected || code == .timeout {
            if facts.localAnswer == false { return .youDeclined }
            if facts.localAnswer == nil, facts.localCodeExpired { return .codeExpired }
            return .otherDeclined(name: name)
        }
        if code == .connectionLost { return .connectionLost(name: name) }
        return .failed(code, name: name)
    }

    private static func windowBeforeCode(_ code: ErrorCode, _ facts: PairingFacts) -> PairingOutcome {
        if code == .timeout, facts.windowElapsed { return .timeUp }
        if !facts.listening { return .couldNotWait(ErrorCatalog.text(for: code).message) }
        // Something else used the window (a transfer, a stalled handshake).
        // It is one-shot: it opens again only on "Start again".
        return .usedUp
    }

    private static func requestBeforeCode(_ code: ErrorCode, name: String) -> PairingVerdict {
        switch code {
        case .notAuthorized:
            return .waitAgain(note: .notWaiting(name: name))
        case .busy:
            return .waitAgain(note: .busy(name: name))
        case .connectionLost, .timeout:
            return .waitAgain(note: .unreachable(name: name))
        default:
            // A possible attack is never a passing note.
            if ErrorCatalog.text(for: code).isSecurity { return .finished(.failed(code, name: name)) }
            return .waitAgain(note: .failed(code, name: name))
        }
    }
}

/// How long the model waits for the core while pairing.
public struct PairingTiming: Sendable {
    /// How long this device stays ready to pair.
    public var windowMinutes: UInt32
    /// For this device's window to report its end after a cancel.
    public var windowCloseWait: Duration
    /// For the paused receiver to resume after a window.
    public var resumeWait: Duration
    /// For a stopping receiver before a window opens.
    public var stopWait: Duration

    public init(
        windowMinutes: UInt32 = 5, windowCloseWait: Duration = .seconds(12), resumeWait: Duration = .seconds(30),
        stopWait: Duration = .seconds(5)
    ) {
        self.windowMinutes = windowMinutes
        self.windowCloseWait = windowCloseWait
        self.resumeWait = resumeWait
        self.stopWait = stopWait
    }
}

/// Runs a closure on the main actor after a delay. Tests fire it by hand, so
/// they never sleep.
public protocol ModelTimer: Sendable {
    @MainActor func schedule(after delay: Duration, _ fire: @escaping @MainActor () -> Void)
}

/// The real clock.
public struct SystemTimer: ModelTimer {
    public init() {}

    @MainActor public func schedule(after delay: Duration, _ fire: @escaping @MainActor () -> Void) {
        Task { @MainActor in
            try? await Task.sleep(for: delay)
            fire()
        }
    }
}
