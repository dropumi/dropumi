// SPDX-License-Identifier: MPL-2.0
//
// Mac notifications, asked for in context (the first time the window is
// closed, when Dropumi starts working from the menu bar). They are fed by
// ReceiverNotifications, which watches the model for the app's lifetime:
// - "Incoming files" when an offer arrives while the window is closed or
//   Dropumi is in the background. Its only action is to show Dropumi; there
//   is no Accept in the notification.
// - "Files received" only for TransferFinished(Completed).

import AppKit
import DropumiAppState
import DropumiBindings
import Observation
import UserNotifications

final class Notifier: NSObject, UNUserNotificationCenterDelegate {
    static let shared = Notifier()

    private let center = UNUserNotificationCenter.current()

    /// The first time the window closes: explain the menu bar, and ask for
    /// notifications so incoming files can be announced.
    func explainMenuBar() {
        let center = center
        Task {
            guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }
            let content = UNMutableNotificationContent()
            content.title = String(localized: "Dropumi is still receiving")
            content.body = String(localized: "Find it in the menu bar. Choose Quit Dropumi completely there to stop.")
            try? await center.add(UNNotificationRequest(identifier: "menu-bar", content: content, trigger: nil))
        }
    }

    func offer(_ prompt: Prompt) {
        guard case let .offer(id, _, peer, _, _, fileCount, _, _) = prompt else { return }
        guard !NSApp.isActive else { return }
        let label = PeerLabel(peer: peer)
        let content = UNMutableNotificationContent()
        content.title = String(localized: "Incoming files")
        content.body = label.verified
            ? String(localized: "\(label.title) wants to send you \(fileCount) files.")
            : String(localized: "An unverified device wants to send you \(fileCount) files.")
        content.sound = .default
        post(UNNotificationRequest(identifier: "offer-\(id)", content: content, trigger: nil))
    }

    func closed(promptId: UInt64) {
        center.removeDeliveredNotifications(withIdentifiers: ["offer-\(promptId)"])
    }

    func finished(_ entry: HistoryEntry) {
        guard entry.outcome == .completed, !entry.outgoing, !NSApp.isActive else { return }
        let content = UNMutableNotificationContent()
        content.title = String(localized: "Files received")
        content.body = String(localized: "All \(entry.fileNames.count) files were saved and checked.")
        post(UNNotificationRequest(identifier: "done-\(entry.id)", content: content, trigger: nil))
    }

    private func post(_ request: UNNotificationRequest) {
        let center = center
        Task {
            let settings = await center.notificationSettings()
            guard settings.authorizationStatus == .authorized || settings.authorizationStatus == .provisional else { return }
            try? await center.add(request)
        }
    }

    // MARK: UNUserNotificationCenterDelegate

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        await MainActor.run { MainWindow.show() }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }
}

/// Feeds the notifier from the app model. It observes the model directly,
/// not through a view: the window's views are gone while the window is
/// closed, which is exactly when Dropumi runs from the menu bar and these
/// notifications matter.
final class ReceiverNotifications {
    enum Event: Equatable {
        case offer(Prompt)
        case closed(promptId: UInt64)
        case finished(HistoryEntry)
    }

    private let post: @MainActor (Event) -> Void
    private var task: Task<Void, Never>?

    init(post: @escaping @MainActor (Event) -> Void = ReceiverNotifications.postToNotifier) {
        self.post = post
    }

    /// Follows the current model for as long as the app runs, including a
    /// model that replaces it (a new receive folder).
    func follow(_ current: @escaping @MainActor @Sendable () -> AppModel?) {
        guard task == nil else { return }
        let snapshots = Observations { Snapshot(current()) }
        task = Task { [weak self] in
            var last: Snapshot?
            for await snapshot in snapshots {
                guard let self else { return }
                handle(snapshot, after: last)
                last = snapshot
            }
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }

    private func handle(_ new: Snapshot, after old: Snapshot?) {
        // A new model starts with no prompts, and its history was read from
        // disk: never announce that history.
        let sameModel = old?.model == new.model
        let before = sameModel ? old?.prompts ?? [] : []
        let beforeIds = Set(before.map(\.promptId))
        let nowIds = Set(new.prompts.map(\.promptId))
        for prompt in new.prompts where !beforeIds.contains(prompt.promptId) {
            post(.offer(prompt))
        }
        for prompt in before where !nowIds.contains(prompt.promptId) {
            post(.closed(promptId: prompt.promptId))
        }
        if sameModel, let latest = new.latest, latest.id != old?.latest?.id {
            post(.finished(latest))
        }
    }

    static func postToNotifier(_ event: Event) {
        switch event {
        case let .offer(prompt): Notifier.shared.offer(prompt)
        case let .closed(id): Notifier.shared.closed(promptId: id)
        case let .finished(entry): Notifier.shared.finished(entry)
        }
    }
}

private struct Snapshot: Sendable {
    let model: ObjectIdentifier?
    let prompts: [Prompt]
    let latest: HistoryEntry?

    init(_ model: AppModel?) {
        self.model = model.map(ObjectIdentifier.init)
        prompts = model?.prompts ?? []
        latest = model?.history.first
    }
}
