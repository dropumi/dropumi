// SPDX-License-Identifier: MPL-2.0
//
// UI-test scenarios (DEBUG builds only; Release excludes this file): the app
// runs on the fake core with temporary folders, so UI tests can show every
// screen and state without a network. Results from these tests are labelled
// "UI state test (fake core)"; they say nothing about the real core.
//
// Launch with: -dropumi-ui-test -dropumi-scenario <name>

#if DEBUG
import DropumiAppState
import DropumiBindings
import DropumiCore
import Foundation
import Observation
#if os(macOS)
import AppKit

/// Under a test (DEBUG only), the app's windows open on the Mac's built-in
/// display, so automated runs stay off an external display someone is using.
/// Without a built-in display nothing moves.
@MainActor
enum TestWindowPlacement {
    private static var placed = Set<ObjectIdentifier>()
    private static var observer: NSObjectProtocol?

    static func keepOnBuiltInDisplay() {
        guard observer == nil else { return }
        observer = NotificationCenter.default.addObserver(
            forName: NSWindow.didBecomeKeyNotification, object: nil, queue: .main
        ) { note in
            guard let window = note.object as? NSWindow else { return }
            MainActor.assumeIsolated { place(window) }
        }
    }

    private static func place(_ window: NSWindow) {
        guard !placed.contains(ObjectIdentifier(window)), let screen = builtInScreen() else { return }
        placed.insert(ObjectIdentifier(window))
        // SwiftUI sets the window's frame after it first becomes key; move it
        // on the next turn of the run loop, after that.
        DispatchQueue.main.async {
            let area = screen.visibleFrame.insetBy(dx: 24, dy: 24)
            guard !area.contains(window.frame) else { return }
            var frame = window.frame
            frame.size.width = min(frame.width, area.width)
            frame.size.height = min(frame.height, area.height)
            frame.origin = NSPoint(x: area.minX, y: area.maxY - frame.height)
            window.setFrame(frame, display: true)
        }
    }

    private static func builtInScreen() -> NSScreen? {
        NSScreen.screens.first { screen in
            let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber
            return number.map { CGDisplayIsBuiltin($0.uint32Value) != 0 } ?? false
        }
    }
}
#endif

enum UITestScenario: String, CaseIterable {
    case onboarding
    case nearby
    case nearbyEmpty = "nearby-empty"
    case offerUnverified = "offer-unverified"
    case offerPaired = "offer-paired"
    case codePairing = "code-pairing"
    case transfers
    /// Settings with one paired device, for the Paired devices screen.
    case pairedDevices = "paired-devices"
    case localNetworkDenied = "local-network-denied"
    /// A prompt whose core closes it once it is answered, so a UI test sees
    /// an answer as the prompt going away (prompt arming, PROMPT-1).
    case offerClosesOnAnswer = "offer-closes-on-answer"
    case codeClosesOnAnswer = "code-closes-on-answer"
    // Pair a device (opened from Settings), on a fake core that pairs like
    // the real one: showing the screen opens this device's window.
    /// Ready to pair, with two devices nearby. Asking one gets no answer.
    case pairingReady = "pairing-ready"
    /// Ready to pair, with no device nearby.
    case pairingEmpty = "pairing-empty"
    /// A device pairs with this one.
    case pairingPaired = "pairing-paired"
    /// Asking a device that isn't ready to pair: it refuses (NotAuthorized).
    case pairingNotReady = "pairing-not-ready"
    /// Asking a device shows a code. Any answer ends the pairing as the core
    /// reports a "no" from either device (PairingRejected).
    case pairingCodes = "pairing-codes"
    /// The five minutes run out with nobody asking.
    case pairingTimeUp = "pairing-time-up"
    /// A device connects to this one, but not to pair.
    case pairingUsedUp = "pairing-used-up"
    /// Nearby, after the core refused a connection.
    case refusedDevice = "refused-device"
    /// Paired devices, with none.
    case pairedDevicesEmpty = "paired-devices-empty"

    static let argument = "-dropumi-scenario"
    static let armDelayArgument = "-dropumi-arm-delay-ms"

    static func fromLaunchArguments() -> UITestScenario? {
        let arguments = CommandLine.arguments
        guard arguments.contains(TestHost.uiTestArgument),
              let index = arguments.firstIndex(of: argument), index + 1 < arguments.count
        else { return nil }
        return UITestScenario(rawValue: arguments[index + 1])
    }

    /// A longer arming delay for UI tests, so that a press can start reliably
    /// while Accept is still disabled. Never shorter than `promptArmDelay`.
    static var armDelayOverride: Duration? {
        let arguments = CommandLine.arguments
        guard arguments.contains(TestHost.uiTestArgument),
              let index = arguments.firstIndex(of: armDelayArgument), index + 1 < arguments.count,
              let milliseconds = Int(arguments[index + 1])
        else { return nil }
        return max(.milliseconds(milliseconds), promptArmDelay)
    }

    func makeModel() -> AppModel {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("dropumi-ui-\(UUID().uuidString)", isDirectory: true)
        let paths = AppPaths(
            configDir: root.appendingPathComponent("config", isDirectory: true),
            receiveDir: root.appendingPathComponent("inbox", isDirectory: true),
            outgoingDir: root.appendingPathComponent("outgoing", isDirectory: true),
            historyFile: root.appendingPathComponent("history.json"))
        try? paths.prepare()
        let settings = SettingsStore(defaults: TestHost.defaults())
        settings.onboarded = self != .onboarding
        // Pairing as the core does it, in every scenario (only a pairing
        // screen opens a window). Set before the model starts: a pairing
        // screen shown at launch opens its window at once.
        let core = FakeCoreClient()
        core.scriptPairingLikeTheCore()
        guard self == .pairingTimeUp else {
            return AppModel(core: core, paths: paths, settings: settings, inbox: nil)
        }
        let testClock = Self.testClock
        return AppModel(core: core, paths: paths, settings: settings, inbox: nil, clock: { testClock.now })
    }

    /// The model's clock in the time-up scenario. It stands still until the
    /// scenario moves it past the window's end, instead of waiting five
    /// minutes.
    private static let testClock = TestClock()

    /// The tab the scenario starts on.
    var initialTab: AppTab {
        switch self {
        case .transfers: .transfers
        case .pairedDevices: .settings
        default: initialSettingsPath.isEmpty ? .nearby : .settings
        }
    }

    /// The screens the scenario starts on, in Settings.
    var initialSettingsPath: [SettingsRoute] {
        switch self {
        case .pairingReady, .pairingEmpty, .pairingPaired, .pairingNotReady, .pairingCodes, .pairingTimeUp,
             .pairingUsedUp:
            [.pairing]
        case .pairedDevicesEmpty:
            [.trustedDevices]
        default:
            []
        }
    }

    func afterStart(_ model: AppModel) {
        #if os(macOS)
        // UI state tests check screens, not launch behaviour: macOS does not
        // always open the main window at launch (see the device-test
        // findings), so a UI-test launch opens it explicitly.
        MainWindow.show()
        #endif
        guard let core = model.core as? FakeCoreClient else { return }
        let paired = Fixtures.peer(name: "MacBook", trusted: true, alias: "Studio Mac")
        let maya = Fixtures.nearby("Maya's iPhone", instance: "dropumi-00000001")
        let studio = Fixtures.nearby("Studio Mac", instance: "dropumi-00000002", hintAlias: "Studio Mac")
        switch self {
        case .onboarding, .nearbyEmpty, .pairingEmpty, .pairedDevicesEmpty:
            break
        case .nearby:
            core.emit(
                .peerFound(peer: maya),
                .peerFound(peer: studio),
                .peerFound(peer: Fixtures.nearby("Pixel 8", instance: "dropumi-00000003")))
        case .offerUnverified:
            let offer = Fixtures.offer(files: [
                OfferedFile(name: "Beach-trip.mov", size: 18_200_000),
                OfferedFile(name: "IMG_2041.heic", size: 4_100_000),
                OfferedFile(name: "Lease-draft.pdf", size: 2_300_000),
            ])
            core.emit(
                .offerReceived(transferId: "aa", peer: Fixtures.peer(), files: 3, totalBytes: 24_600_000),
                .promptOpened(prompt: offer))
        case .offerPaired:
            core.emit(
                .offerReceived(transferId: "bb", peer: paired, files: 1, totalBytes: 2_300_000),
                .promptOpened(prompt: Fixtures.offer(
                    id: 3, transferId: "bb", peer: paired, code: nil,
                    files: [OfferedFile(name: "Lease-draft.pdf", size: 2_300_000)])))
        case .codePairing:
            core.emit(.promptOpened(prompt: Fixtures.code()))
        case .transfers:
            core.emit(
                .offerReceived(transferId: "cc", peer: paired, files: 1, totalBytes: 4_100_000),
                .transferAccepted(operation: nil, transferId: "cc"),
                .fileSaved(transferId: "cc", index: 0, savedAs: "IMG_2041.heic", size: 4_100_000, blake3: "00"),
                .transferFinished(operation: nil, direction: .incoming, result: Fixtures.result(.completed, transferId: "cc")),
                .offerReceived(transferId: "dd", peer: paired, files: 1, totalBytes: 18_200_000),
                .transferAccepted(operation: nil, transferId: "dd"),
                .transferProgress(
                    operation: nil, transferId: "dd",
                    progress: TransferProgress(bytesDone: 11_300_000, bytesTotal: 18_200_000, filesDone: 0, filesTotal: 1)))
        case .pairedDevices:
            _ = try? core.trustFingerprint("QRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCD", alias: "Studio Mac")
            Task { await model.refreshTrusted() }
        case .localNetworkDenied:
            core.emit(.discoveryUnavailable(reason: "local network access is turned off for Dropumi"))
        case .offerClosesOnAnswer, .codeClosesOnAnswer:
            core.onCall = { [weak core] call in
                switch call {
                case let .answerOffer(id, _), let .answerCode(id, _):
                    core?.emit(.promptClosed(id: id, reason: .answered))
                default:
                    break
                }
            }
            if self == .offerClosesOnAnswer {
                core.emit(
                    .offerReceived(transferId: "aa", peer: Fixtures.peer(), files: 1, totalBytes: 2_300_000),
                    .promptOpened(prompt: Fixtures.offer(
                        files: [OfferedFile(name: "Lease-draft.pdf", size: 2_300_000)])))
            } else {
                core.emit(.promptOpened(prompt: Fixtures.code()))
            }
        case .pairingReady:
            core.emit(.peerFound(peer: maya), .peerFound(peer: studio))
        case .pairingPaired:
            core.emit(.peerFound(peer: maya), .peerFound(peer: studio))
            Self.onceReadyToPair(model) { window in
                // As the core ends a pairing both people confirmed: the
                // device is stored, then the window reports it.
                let peer = Fixtures.peer(trusted: true, alias: "Maya's iPhone")
                _ = try? core.trustFingerprint(peer.fingerprint, alias: peer.alias)
                core.endWindow(window, with: .paired(peer: peer))
            }
        case .pairingNotReady:
            core.emit(.peerFound(peer: maya), .peerFound(peer: studio))
            Self.eachRequest(model) { request in
                // No window is open there: the device refuses the connection.
                core.emit(.pairingFinished(operation: request, result: .failed(error: .notAuthorized)))
            }
        case .pairingCodes:
            core.emit(.peerFound(peer: maya), .peerFound(peer: studio))
            Self.eachRequest(model) { request in
                // The device accepted: both show the code, as the core reports it.
                let peer = Fixtures.peer()
                core.emit(.connected(operation: request, peer: peer))
                core.emit(.sessionCode(operation: request, peer: peer, code: "654321"))
                core.emit(.promptOpened(prompt: Fixtures.code(
                    id: Self.codePromptBase + request, operation: request, peer: peer)))
            }
            // Either answer closes the prompt and ends the pairing.
            core.onCall = { [weak core, base = Self.codePromptBase] call in
                guard case let .answerCode(id, _) = call, id > base else { return }
                core?.emit(
                    .promptClosed(id: id, reason: .answered),
                    .pairingFinished(operation: id - base, result: .failed(error: .pairingRejected)))
            }
        case .pairingTimeUp:
            core.emit(.peerFound(peer: maya), .peerFound(peer: studio))
            Self.onceReadyToPair(model) { window in
                // Nobody asked in five minutes: the core ends the window.
                Self.testClock.advance(by: 5 * 60)
                core.endWindow(window, with: .failed(error: .timeout))
            }
        case .pairingUsedUp:
            core.emit(.peerFound(peer: maya), .peerFound(peer: studio))
            Self.onceReadyToPair(model) { window in
                // A device connected to send, not to pair: refused, and the
                // window ends.
                core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
                core.endWindow(window, with: .failed(error: .notAuthorized))
            }
        case .refusedDevice:
            core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        }
    }

    // MARK: Pairing scripts
    //
    // A scenario delivers a pairing operation's events only once the model
    // holds the operation, as the core's events reach a started operation.
    // The model's observable state says when: no polling and no sleeps.

    /// The code prompts of `pairing-codes` have this id plus their request.
    private static let codePromptBase: UInt64 = 1_000

    /// Runs `body` with this device's window once the model holds it and it
    /// listens. Once only: a window opened again stays ready.
    private static func onceReadyToPair(_ model: AppModel, _ body: @escaping @MainActor (UInt64) -> Void) {
        once({
            guard let session = model.pairing, case .window = session.role, session.result == nil,
                  session.port != nil
            else { return nil }
            return session.operation
        }, then: body)
    }

    /// Runs `body` with each request of this device (a tap on Pair) once the
    /// model holds it.
    private static func eachRequest(
        _ model: AppModel, after previous: UInt64? = nil, _ body: @escaping @MainActor (UInt64) -> Void
    ) {
        once({
            guard let session = model.pairing, case .initiator = session.role, session.result == nil,
                  session.operation != previous
            else { return nil }
            return session.operation
        }, then: { request in
            body(request)
            Self.eachRequest(model, after: request, body)
        })
    }

    /// Runs `body` with the value of `value()` once it is not nil. `value()`
    /// reads the model's observable state, and is read again after each
    /// change to what it read.
    private static func once(
        _ value: @escaping @MainActor () -> UInt64?, then body: @escaping @MainActor (UInt64) -> Void
    ) {
        if let ready = value() {
            body(ready)
            return
        }
        withObservationTracking {
            _ = value()
        } onChange: {
            // Called before the change is made: read it after.
            Task { @MainActor in Self.once(value, then: body) }
        }
    }
}
#endif
