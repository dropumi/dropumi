// SPDX-License-Identifier: MPL-2.0
//
// The composition root: picks the folders (ADR-0014), opens the Rust core
// with the Bonjour adapter, and builds the app model. Opening the core reads
// or creates the identity, so it runs off the main thread.

import DropumiAppState
import DropumiCore
import DropumiShare
import Foundation
import Observation

@Observable
final class AppLoader {
    enum State {
        case loading
        case ready(AppModel)
        case failed(String)
    }

    private(set) var state: State = .loading
    /// Where the user is (owned here so menu commands can reach it).
    let router = Router()
    /// Files opened with Dropumi before the core was ready.
    private var pendingFiles: [URL] = []
    /// iOS: whether the app was last seen in the background (D5). The app
    /// can go there while the core is still opening; the new model starts
    /// in the background then, so it does not receive until it is in front.
    var isInBackground = false

    var model: AppModel? {
        if case let .ready(model) = state { return model }
        return nil
    }

    func load() async {
        guard case .loading = state else { return }
        #if DEBUG
        if let scenario = UITestScenario.fromLaunchArguments() {
            let model = scenario.makeModel()
            if isInBackground { model.enterBackground() }
            router.tab = scenario.initialTab
            state = .ready(model)
            await model.start()
            scenario.afterStart(model)
            return
        }
        #endif
        let isTest = TestHost.isActive
        #if os(macOS)
        let chosenReceiveDir = isTest ? nil : ReceiveLocation.restore()
        #else
        let chosenReceiveDir: URL? = nil
        #endif
        do {
            let (paths, core) = try await Task.detached(priority: .medium) {
                let paths = isTest ? try AppPaths.temporary() : try AppPaths.standard(receiveDir: chosenReceiveDir)
                try paths.prepare()
                paths.clearOutgoing()
                let core = try LiveCoreClient(
                    configDir: paths.configDir, receiveDir: paths.receiveDir, discovery: BonjourDiscovery())
                paths.protectConfig()
                return (paths, core)
            }.value
            let settings = isTest ? SettingsStore(defaults: TestHost.defaults()) : SettingsStore()
            #if os(iOS)
            let inbox = isTest ? nil : ShareInbox.shared()
            #else
            let inbox: ShareInbox? = nil
            #endif
            let model = AppModel(core: core, paths: paths, settings: settings, inbox: inbox)
            if isInBackground { model.enterBackground() }
            state = .ready(model)
            await model.start()
            let files = pendingFiles
            pendingFiles = []
            files.forEach(open)
        } catch {
            state = .failed(ErrorCatalog.message(for: error))
        }
    }

    /// A file opened with Dropumi ("Open in Dropumi" in Files, or the
    /// Dock on the Mac): add it to the files to send.
    func open(_ url: URL) {
        guard url.isFileURL else { return }
        guard let model else {
            pendingFiles.append(url)
            return
        }
        let files = FileImport.picked([url])
        guard !files.isEmpty else { return }
        model.addToDraft(files)
        router.openSend()
    }

    /// Closes the core and opens it again (a new receive folder on the Mac).
    func reload() async {
        if let model {
            await model.shutdownAndWait()
        }
        state = .loading
        await load()
    }
}
