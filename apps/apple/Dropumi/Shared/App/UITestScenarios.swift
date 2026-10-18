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
    case localNetworkDenied = "local-network-denied"
    /// A prompt whose core closes it once it is answered, so a UI test sees
    /// an answer as the prompt going away (prompt arming, PROMPT-1).
    case offerClosesOnAnswer = "offer-closes-on-answer"
    case codeClosesOnAnswer = "code-closes-on-answer"

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
        return AppModel(core: FakeCoreClient(), paths: paths, settings: settings, inbox: nil)
    }

    /// The tab the scenario starts on.
    var initialTab: AppTab {
        self == .transfers ? .transfers : .nearby
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
        switch self {
        case .onboarding, .nearbyEmpty:
            break
        case .nearby:
            core.emit(
                .peerFound(peer: Fixtures.nearby("Maya's iPhone", instance: "dropumi-00000001")),
                .peerFound(peer: Fixtures.nearby("Studio Mac", instance: "dropumi-00000002", hintAlias: "Studio Mac")),
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
        }
    }
}
#endif
