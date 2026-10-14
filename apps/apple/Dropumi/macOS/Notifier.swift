// SPDX-License-Identifier: MPL-2.0
//
// Mac notifications, asked for in context (the first time the window is
// closed, when Dropumi starts working from the menu bar):
// - "Incoming files" when an offer arrives while the window is closed or
//   Dropumi is in the background. Its only action is to show Dropumi; there
//   is no Accept in the notification.
// - "Files received" only for TransferFinished(Completed).

import AppKit
import DropumiAppState
import DropumiBindings
import SwiftUI
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

/// Mac hooks on the ready app: notifications for offers and finished
/// transfers.
struct PlatformHooks: ViewModifier {
    let model: AppModel

    func body(content: Content) -> some View {
        content
            .onChange(of: model.prompts.map(\.promptId)) { old, new in
                for prompt in model.prompts where !old.contains(prompt.promptId) {
                    Notifier.shared.offer(prompt)
                }
                for id in old where !new.contains(id) {
                    Notifier.shared.closed(promptId: id)
                }
            }
            .onChange(of: model.history.first?.id) {
                if let entry = model.history.first {
                    Notifier.shared.finished(entry)
                }
            }
    }
}
