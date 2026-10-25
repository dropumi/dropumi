// SPDX-License-Identifier: MPL-2.0
//
// Lets running transfers finish when the user leaves Dropumi
// (BGContinuedProcessingTask). The task is requested while Dropumi is in
// front, as soon as a transfer starts, because the user started it. iOS may
// refuse or end it early; then the transfers are cancelled cleanly so the
// other device sees "cancelled", not a timeout. The Transfers screen tells
// iPhone and iPad users that leaving Dropumi may stop a transfer. Device
// behaviour: not tested yet (spike S-A3).

import BackgroundTasks
import DropumiAppState
import SwiftUI

@MainActor
final class BackgroundTransfers {
    static let shared = BackgroundTransfers()

    private var task: BGContinuedProcessingTask?
    private var requested = false
    private var progressLoop: Task<Void, Never>?

    /// Call when the set of running transfers changes.
    func update(_ model: AppModel) {
        if model.active.isEmpty {
            finish(success: true)
        } else if !requested && !model.isInBackground {
            request(model)
        }
    }

    private func request(_ model: AppModel) {
        requested = true
        let base = Bundle.main.bundleIdentifier ?? "io.github.dropumi.Dropumi"
        let identifier = "\(base).transfer.\(UUID().uuidString)"
        let registered = BGTaskScheduler.shared.register(forTaskWithIdentifier: identifier, using: .main) { [weak self] task in
            MainActor.assumeIsolated {
                guard let task = task as? BGContinuedProcessingTask, let self else {
                    task.setTaskCompleted(success: false)
                    return
                }
                self.run(task, model: model)
            }
        }
        // If iOS refuses, do not ask again until the transfers are done.
        guard registered else { return }
        let request = BGContinuedProcessingTaskRequest(
            identifier: identifier,
            title: String(localized: "Dropumi transfer"),
            subtitle: String(localized: "Finishing files with a nearby device"))
        request.strategy = .fail
        Task {
            // If iOS refuses (for example, too many running), transfers go
            // on while Dropumi stays open.
            try? await BGTaskScheduler.shared.submitTaskRequest(request)
        }
    }

    private func run(_ task: BGContinuedProcessingTask, model: AppModel) {
        self.task = task
        task.progress.totalUnitCount = 1000
        task.expirationHandler = Self.expiration(model)
        progressLoop = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, let task = self.task else { return }
                let records = model.active
                if records.isEmpty {
                    self.finish(success: true)
                    return
                }
                let done = records.compactMap(\.progress).reduce(UInt64(0)) { $0 + $1.bytesDone }
                let total = records.compactMap(\.progress).reduce(UInt64(0)) { $0 + $1.bytesTotal }
                task.progress.completedUnitCount = total > 0 ? Int64(Double(done) / Double(total) * 1000) : 0
                try? await Task.sleep(for: .milliseconds(500))
            }
        }
    }

    /// iOS is ending the time it gave: cancel cleanly. Built outside the
    /// main actor because iOS may call it on any queue.
    nonisolated private static func expiration(_ model: AppModel) -> @Sendable () -> Void {
        {
            Task { @MainActor in
                for record in model.active {
                    model.cancel(record)
                }
                BackgroundTransfers.shared.finish(success: false)
            }
        }
    }

    private func finish(success: Bool) {
        progressLoop?.cancel()
        progressLoop = nil
        task?.setTaskCompleted(success: success)
        task = nil
        requested = false
    }
}

/// iOS hooks on the ready app.
struct PlatformHooks: ViewModifier {
    let model: AppModel

    func body(content: Content) -> some View {
        content
            .onChange(of: model.active.count) {
                BackgroundTransfers.shared.update(model)
            }
    }
}
