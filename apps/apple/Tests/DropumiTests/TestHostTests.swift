// SPDX-License-Identifier: MPL-2.0

import DropumiAppState
import DropumiCore
import Foundation
import Testing
@testable import Dropumi

@MainActor
struct TestHostTests {
    @Test func testHostNeverUsesRealConfigDir() async throws {
        #expect(TestHost.isActive)
        let loader = AppLoader()
        await loader.load()
        let model = try #require(loader.model, "the real core opens in the test host")
        let real = try AppPaths.standard()
        #expect(model.paths.configDir != real.configDir)
        #expect(model.paths.receiveDir != real.receiveDir)
        #expect(model.paths.historyFile != real.historyFile)
        #expect(model.paths.configDir.path.hasPrefix(FileManager.default.temporaryDirectory.path))
        #expect(model.identity?.fingerprint.count == 52)
        await model.shutdownAndWait()
    }

    /// ADR-0014: the identity's folder is excluded from backups, so a
    /// restored backup never clones it.
    @Test func configFolderIsExcludedFromBackups() async throws {
        let loader = AppLoader()
        await loader.load()
        let model = try #require(loader.model)
        let values = try model.paths.configDir.resourceValues(forKeys: [.isExcludedFromBackupKey])
        #expect(values.isExcludedFromBackup == true)
        let mode = try FileManager.default.attributesOfItem(atPath: model.paths.configDir.path)[.posixPermissions] as? NSNumber
        #expect(mode?.intValue == 0o700)
        await model.shutdownAndWait()
    }

    /// ADR-0012 D5: the app went to the background while the core was still
    /// opening. The model starts in the background, so it does not receive
    /// until the app is in front again.
    @Test func backgroundDuringLoadKeepsTheModelInTheBackground() async throws {
        let loader = AppLoader()
        loader.isInBackground = true
        await loader.load()
        let model = try #require(loader.model)
        #expect(model.isInBackground)
        #expect(model.receiver == nil)
        await model.shutdownAndWait()
    }

    /// The hand-over order: an onboarded model begun while the app is in the
    /// background never starts receiving (it would advertise in the
    /// background otherwise). Fails if `enterBackground()` runs after
    /// `start()`.
    @Test func modelBegunInTheBackgroundDoesNotStartReceiving() async throws {
        let loader = AppLoader()
        loader.isInBackground = true
        let core = FakeCoreClient()
        let paths = try AppPaths.temporary()
        try paths.prepare()
        let settings = SettingsStore(defaults: TestHost.defaults())
        settings.onboarded = true
        let model = AppModel(core: core, paths: paths, settings: settings)
        await loader.begin(model)
        #expect(loader.model === model)
        #expect(model.isInBackground)
        #expect(!core.calls.contains { if case .startReceiving = $0 { true } else { false } })
        await model.becomeActive()
        #expect(core.calls.contains { if case .startReceiving = $0 { true } else { false } })
        await model.shutdownAndWait()
    }

    @Test func acceptIsArmedAfter600ms() {
        #expect(promptArmDelay == .milliseconds(600))
    }
}
