// SPDX-License-Identifier: MPL-2.0
//
// The Mac receives from the menu bar with the window closed, so neither the
// core nor its notifications may depend on the window's views.

import AppKit
import DropumiAppState
import DropumiCore
import Foundation
import Observation
import Testing
@testable import Dropumi

@MainActor
@Observable
private final class ModelHolder {
    var model: AppModel?
}

@MainActor
struct MenuBarReceiverTests {
    @Test func coreOpensAtLaunchWithoutAWindow() async throws {
        let delegate = AppDelegate()
        defer { delegate.notifications.stop() }
        delegate.applicationDidFinishLaunching(Notification(name: NSApplication.didFinishLaunchingNotification))
        await delegate.launch?.value
        let model = try #require(delegate.loader.model, "the app delegate opens the core, with no view")
        #expect(model.paths.configDir.path.hasPrefix(FileManager.default.temporaryDirectory.path))
        await model.shutdownAndWait()
    }

    @Test(.timeLimit(.minutes(1)))
    func offersAndReceivedFilesAnnouncedWithoutAView() async throws {
        let (events, sink) = AsyncStream.makeStream(of: ReceiverNotifications.Event.self)
        var announced = events.makeAsyncIterator()
        let notifications = ReceiverNotifications { sink.yield($0) }
        defer { notifications.stop() }
        let holder = ModelHolder()
        notifications.follow { holder.model }

        // History read from disk is never announced.
        let model = AppModel(
            core: FakeCoreClient(), paths: try AppPaths.temporary(),
            settings: SettingsStore(defaults: TestHost.defaults()))
        model.apply([.transferFinished(operation: nil, direction: .incoming, result: Fixtures.result(.completed))])
        holder.model = model

        let offer = Fixtures.offer(id: 7)
        model.apply([.promptOpened(prompt: offer)])
        #expect(await announced.next() == .offer(offer))

        model.apply([.promptClosed(id: 7, reason: .answered)])
        #expect(await announced.next() == .closed(promptId: 7))

        model.apply([.transferFinished(
            operation: nil, direction: .incoming, result: Fixtures.result(.completed, transferId: "bb"))])
        let finished = try #require(model.history.first)
        #expect(await announced.next() == .finished(finished))

        // A replacement model (a new receive folder) is followed too.
        let next = AppModel(
            core: FakeCoreClient(), paths: try AppPaths.temporary(),
            settings: SettingsStore(defaults: TestHost.defaults()))
        holder.model = next
        let second = Fixtures.offer(id: 8, transferId: "cc")
        next.apply([.promptOpened(prompt: second)])
        #expect(await announced.next() == .offer(second))
    }
}
