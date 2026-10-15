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
    /// The app delegate, with no window or view, opens the core and feeds
    /// its offers to the notifications, also after the core reopens.
    @Test(.timeLimit(.minutes(1)))
    func launchOpensTheCoreAndAnnouncesOffersWithoutAWindow() async throws {
        let (events, sink) = AsyncStream.makeStream(of: ReceiverNotifications.Event.self)
        var announced = events.makeAsyncIterator()
        let delegate = AppDelegate(notifications: ReceiverNotifications { sink.yield($0) })
        defer { delegate.notifications.stop() }
        delegate.applicationDidFinishLaunching(Notification(name: NSApplication.didFinishLaunchingNotification))
        await delegate.launch?.value
        let model = try #require(delegate.loader.model, "the app delegate opens the core, with no view")
        #expect(model.paths.configDir.path.hasPrefix(FileManager.default.temporaryDirectory.path))

        let offer = Fixtures.offer(id: 1)
        model.apply([.promptOpened(prompt: offer)])
        #expect(await announced.next() == .offer(offer))

        // A new receive folder reopens the core: the old offer is withdrawn.
        await delegate.loader.reload()
        #expect(await announced.next() == .closed(promptId: 1))
        let reopened = try #require(delegate.loader.model)
        #expect(reopened !== model)
        await reopened.shutdownAndWait()
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
        let model = try fakeModel()
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

        // A replacement model (a new receive folder): the old model's open
        // offer is withdrawn first, even when the new core reuses its id.
        let pending = Fixtures.offer(id: 8, transferId: "cc")
        model.apply([.promptOpened(prompt: pending)])
        #expect(await announced.next() == .offer(pending))
        let next = try fakeModel()
        holder.model = next
        let reused = Fixtures.offer(id: 8, transferId: "dd")
        next.apply([.promptOpened(prompt: reused)])
        #expect(await announced.next() == .closed(promptId: 8))
        #expect(await announced.next() == .offer(reused))

        // Quitting withdraws what is still announced.
        notifications.withdrawAll()
        #expect(await announced.next() == .closed(promptId: 8))
    }

    @Test func menuBarReportsAFailedStart() throws {
        guard case let .failed(message) = MenuBarPhase(.failed("The configuration folder is not private.")) else {
            Issue.record("a failed start must not look like one still starting")
            return
        }
        #expect(message == "The configuration folder is not private.")
        guard case .starting = MenuBarPhase(.loading) else {
            Issue.record("loading shows Starting…")
            return
        }
    }

    private func fakeModel() throws -> AppModel {
        AppModel(
            core: FakeCoreClient(), paths: try AppPaths.temporary(),
            settings: SettingsStore(defaults: TestHost.defaults()))
    }
}
