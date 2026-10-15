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

enum UITestScenario: String, CaseIterable {
    case onboarding
    case nearby
    case nearbyEmpty = "nearby-empty"
    case offerUnverified = "offer-unverified"
    case offerPaired = "offer-paired"
    case codePairing = "code-pairing"
    case transfers
    case localNetworkDenied = "local-network-denied"

    static let argument = "-dropumi-scenario"

    static func fromLaunchArguments() -> UITestScenario? {
        let arguments = CommandLine.arguments
        guard arguments.contains(TestHost.uiTestArgument),
              let index = arguments.firstIndex(of: argument), index + 1 < arguments.count
        else { return nil }
        return UITestScenario(rawValue: arguments[index + 1])
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
        }
    }
}
#endif
