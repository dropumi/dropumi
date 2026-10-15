// SPDX-License-Identifier: MPL-2.0
//
// The Share Extension (D1). It never loads the Rust core and never touches
// the network. It copies the shared items into the App Group inbox
// (`inbox/.staging-<id>/`), writes a manifest with names and sizes, and
// renames the folder into place in one step, so the app never sees half a
// batch. iOS gives extensions no public way to open their app, so it ends
// with "Open Dropumi to choose a device".

import DropumiDesign
import DropumiShare
import SwiftUI
import UIKit
import UniformTypeIdentifiers

final class ShareViewController: UIViewController {
    private let state = ShareState()

    override func viewDidLoad() {
        super.viewDidLoad()
        DSFont.register()
        let host = UIHostingController(rootView: ShareView(state: state, done: { [weak self] in self?.done() }, cancel: { [weak self] in self?.cancel() }))
        addChild(host)
        host.view.frame = view.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(host.view)
        host.didMove(toParent: self)
        let providers = (extensionContext?.inputItems as? [NSExtensionItem] ?? [])
            .flatMap { $0.attachments ?? [] }
        state.start(providers)
    }

    private func done() {
        extensionContext?.completeRequest(returningItems: nil)
    }

    private func cancel() {
        state.abandon()
        extensionContext?.cancelRequest(withError: CocoaError(.userCancelled))
    }
}

/// What the extension is doing.
@Observable
final class ShareState {
    enum Phase: Equatable {
        case copying(done: Int, total: Int)
        case ready(count: Int)
        case failed(String)
    }

    private(set) var phase: Phase = .copying(done: 0, total: 0)
    private var batch: StagedBatch?
    private var work: Task<Void, Never>?

    func start(_ providers: [NSItemProvider]) {
        guard !providers.isEmpty else {
            phase = .failed(String(localized: "There was nothing to share."))
            return
        }
        guard providers.count <= ShareInbox.maxItems else {
            phase = .failed(String(localized: "Share at most \(ShareInbox.maxItems) items at a time."))
            return
        }
        guard let inbox = ShareInbox.shared() else {
            phase = .failed(String(localized: "Dropumi can't take shared files right now. Open Dropumi and choose files there."))
            return
        }
        let batch: StagedBatch
        do {
            batch = try inbox.beginBatch()
        } catch {
            phase = .failed(String(localized: "The files could not be prepared."))
            return
        }
        self.batch = batch
        phase = .copying(done: 0, total: providers.count)
        work = Task {
            var copied = 0
            for provider in providers {
                if Task.isCancelled { return }
                if await Self.copy(provider, into: batch) {
                    copied += 1
                }
                phase = .copying(done: copied, total: providers.count)
            }
            guard copied == providers.count else {
                batch.abandon()
                phase = .failed(String(localized: "Some items could not be copied, so nothing was handed to Dropumi. Try again, or share fewer items."))
                return
            }
            do {
                _ = try await Task.detached { try batch.commit() }.value
                self.batch = nil
                phase = .ready(count: copied)
            } catch {
                batch.abandon()
                phase = .failed(String(localized: "The files could not be handed to Dropumi."))
            }
        }
    }

    func abandon() {
        work?.cancel()
        batch?.abandon()
        batch = nil
    }

    /// Copies one item while its file is still valid (inside the provider's
    /// callback, on the provider's queue).
    nonisolated private static func copy(_ provider: NSItemProvider, into batch: StagedBatch) async -> Bool {
        guard let type = provider.registeredContentTypes.first(where: { $0.conforms(to: .data) || $0.conforms(to: .item) }) else {
            return false
        }
        let suggested = provider.suggestedName
        return await withCheckedContinuation { continuation in
            _ = provider.loadFileRepresentation(for: type, openInPlace: false) { url, _, error in
                guard let url, error == nil else {
                    continuation.resume(returning: false)
                    return
                }
                let name = Self.name(suggested: suggested, file: url, type: type)
                let ok = (try? batch.add(copyOf: url, suggestedName: name)) != nil
                continuation.resume(returning: ok)
            }
        }
    }

    /// The provider's suggested name, with the file's extension when the
    /// suggestion has none ("IMG_2041" + ".heic").
    nonisolated private static func name(suggested: String?, file: URL, type: UTType) -> String {
        guard let suggested, !suggested.isEmpty else { return file.lastPathComponent }
        if !(suggested as NSString).pathExtension.isEmpty { return suggested }
        let ext = file.pathExtension.isEmpty ? (type.preferredFilenameExtension ?? "") : file.pathExtension
        return ext.isEmpty ? suggested : "\(suggested).\(ext)"
    }
}
