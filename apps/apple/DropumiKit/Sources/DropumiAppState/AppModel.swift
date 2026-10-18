// SPDX-License-Identifier: MPL-2.0
//
// The app's state, driven by core events. UI-free and testable with a fake
// core. Rules it keeps (ADR-0004, ADR-0009):
//
// - It never answers a prompt by itself; prompts close only when the core
//   says so (`PromptClosed`).
// - A transfer is a success only when the core reports
//   `TransferFinished(Completed)`; a saved file alone is not.
// - Peers are shown alias first; a name a peer claims is labelled as such;
//   a nearby name that matches a paired device's hint is only "looks like".
// - Every core call runs off the main thread.

import DropumiBindings
import DropumiCore
import DropumiShare
import Foundation
import Observation

/// A notice at the top of the screen.
public struct Banner: Equatable, Identifiable, Sendable {
    public enum Style: Sendable { case info, warning, danger }
    public enum Action: Equatable, Sendable { case retryReceiving, openEveryone, openSettings }
    public let id = UUID()
    public let style: Style
    public let title: String
    public let message: String
    public let action: Action?

    public init(style: Style, title: String, message: String, action: Action? = nil) {
        self.style = style
        self.title = title
        self.message = message
        self.action = action
    }

    public static func == (a: Banner, b: Banner) -> Bool { a.id == b.id }
}

/// A file chosen for sending.
public struct OutgoingFile: Equatable, Identifiable, Sendable {
    public let id: UUID
    public let url: URL
    public let name: String
    public let size: UInt64
    /// A security-scoped URL whose access the app holds until it is sent.
    public let securityScoped: Bool
    /// A copy the app made (Photos, a drop, the share inbox), deleted after.
    public let staged: Bool

    public init(url: URL, name: String? = nil, size: UInt64? = nil, securityScoped: Bool, staged: Bool) {
        self.id = UUID()
        self.url = url
        self.name = name ?? url.lastPathComponent
        self.size = size ?? (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize).flatMap { UInt64($0) } ?? 0
        self.securityScoped = securityScoped
        self.staged = staged
    }

    /// Releases what holding this file costs: the security-scoped access, or
    /// the staged copy.
    func release() {
        if securityScoped {
            url.stopAccessingSecurityScopedResource()
        }
        if staged {
            try? FileManager.default.removeItem(at: url)
        }
    }
}

/// Pairing in progress.
public struct PairingSession: Equatable, Sendable {
    public enum Role: Equatable, Sendable {
        /// This device asked another one.
        case initiator(name: String)
        /// This device waits for another one.
        case window
    }
    public let operation: UInt64
    public let role: Role
    public var port: UInt16?
    public var expiresAt: Date?
    public var result: PairingResult?
}

/// Something the UI should show next.
public enum Route: Equatable, Sendable {
    case send
}

@MainActor
@Observable
public final class AppModel {
    public let core: CoreClient
    public let paths: AppPaths
    private let settings: SettingsStore
    private let historyStore: HistoryStore
    private let inbox: ShareInbox?
    private let clock: @Sendable () -> Date

    public private(set) var identity: IdentityInfo?
    public private(set) var trusted: [TrustedDeviceInfo] = []
    public private(set) var nearby: [NearbyPeer] = []
    public private(set) var discoveryProblem: String?
    public private(set) var localNetworkDenied = false
    public private(set) var receiver: ReceiverInfo?
    public private(set) var receiverStopping = false
    public private(set) var visibility: ReceiveVisibility
    public private(set) var autoAcceptPaired: Bool
    public private(set) var prompts: [Prompt] = []
    public private(set) var answering: Set<UInt64> = []
    public private(set) var active: [TransferRecord] = []
    public private(set) var history: [HistoryEntry]
    public private(set) var pairing: PairingSession?
    public private(set) var draft: [OutgoingFile] = []
    public private(set) var isInBackground = false
    public var banner: Banner?
    public var route: Route?
    public var appearance: Appearance {
        didSet { settings.appearance = appearance }
    }
    public var onboarded: Bool {
        didSet { settings.onboarded = onboarded }
    }

    private var browseClients = 0
    private var browsing = false
    private var discoveryTask: Task<Void, Never>?
    private var visibilityTask: Task<Void, Never>?
    private var operations: [UInt64: CoreOperation] = [:]
    private var held: [String: [OutgoingFile]] = [:]
    private var eventTask: Task<Void, Never>?
    private var lastUnpairedNotice: Date?

    public init(
        core: CoreClient,
        paths: AppPaths,
        settings: SettingsStore = SettingsStore(),
        inbox: ShareInbox? = nil,
        clock: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.core = core
        self.paths = paths
        self.settings = settings
        self.historyStore = HistoryStore(file: paths.historyFile)
        self.inbox = inbox
        self.clock = clock
        self.visibility = settings.visibility
        self.autoAcceptPaired = settings.autoAcceptPaired
        self.appearance = settings.appearance
        self.onboarded = settings.onboarded
        self.history = HistoryStore(file: paths.historyFile).load()
    }

    // MARK: Lifecycle

    /// Loads the identity and starts listening for events. Receiving starts
    /// once the user finished onboarding.
    public func start() async {
        guard eventTask == nil else { return }
        let events = core.events
        eventTask = Task { [weak self] in
            for await batch in events {
                self?.apply(batch)
            }
        }
        await refreshIdentity()
        await refreshTrusted()
        if onboarded {
            await applyVisibility()
        }
        takeSharedItems()
    }

    /// Ends onboarding with the chosen visibility and starts receiving
    /// (unless the choice is Off).
    public func finishOnboarding(visibility choice: ReceiveVisibility? = nil) async {
        if let choice {
            visibility = choice
            settings.visibility = choice
        }
        onboarded = true
        await applyVisibility()
    }

    /// iOS: the app went to the background. Stop accepting new connections
    /// (the listener closes and the advertisement is withdrawn); running
    /// transfers finish. Called before `start()`, it keeps `start()` from
    /// receiving.
    public func enterBackground() {
        isInBackground = true
        // Queued behind the receiver changes already under way, so a
        // receiver that is still starting stops accepting once it is up.
        let previous = visibilityTask
        visibilityTask = Task { [weak self] in
            await previous?.value
            await self?.stopAcceptingInBackground()
        }
    }

    private func stopAcceptingInBackground() async {
        // Back in front meanwhile: `becomeActive()` keeps the receiver.
        guard isInBackground, receiver != nil else { return }
        let core = core
        await work { core.stopAccepting() }
    }

    /// iOS: the app is in front again.
    public func becomeActive() async {
        isInBackground = false
        if onboarded {
            await applyVisibility()
        }
        takeSharedItems()
    }

    public func shutdown() {
        eventTask?.cancel()
        let core = core
        Task.detached { core.shutdown() }
    }

    /// Shuts the core down and waits until it has (at most a few seconds).
    public func shutdownAndWait() async {
        eventTask?.cancel()
        clearDraft()
        let core = core
        await work { core.shutdown() }
    }

    // MARK: Identity and trust

    public func refreshIdentity() async {
        do {
            identity = try await work { [core] in try core.identity() }
        } catch {
            show(error, title: String(localized: "This device's identity could not be loaded"))
        }
    }

    public func renameDevice(_ name: String) async {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        let wasReceiving = receiver != nil
        if wasReceiving { await stopReceivingAndWait() }
        let before = identity?.name
        do {
            identity = try await work { [core] in try core.setDeviceName(trimmed) }
        } catch {
            // The core may have saved the name and only failed to read it back.
            await reloadIdentity()
            if identity?.name == before {
                show(error, title: String(localized: "The name was not changed"))
            }
        }
        if wasReceiving { await applyVisibility() }
    }

    public func resetIdentity() async {
        let wasReceiving = receiver != nil
        if wasReceiving { await stopReceivingAndWait() }
        let before = identity?.fingerprint
        do {
            identity = try await work { [core] in try core.resetIdentity() }
            showNewIdentity()
        } catch {
            // The core may have replaced the key and only failed to read it back.
            await reloadIdentity()
            if let now = identity?.fingerprint, now != before {
                showNewIdentity()
            } else {
                show(error, title: String(localized: "The identity was not reset"))
            }
        }
        if wasReceiving { await applyVisibility() }
    }

    private func showNewIdentity() {
        banner = Banner(style: .warning, title: String(localized: "New identity"),
                        message: String(localized: "Devices you paired with must pair again."))
    }

    /// Reloads the identity without a banner; keeps the old one on failure.
    private func reloadIdentity() async {
        if let now = try? await work({ [core] in try core.identity() }) {
            identity = now
        }
    }

    public func refreshTrusted() async {
        trusted = (try? await work { [core] in try core.trustedDevices() }) ?? []
    }

    public func trust(fingerprint: String, alias: String?) async -> Bool {
        do {
            _ = try await work { [core] in try core.trustFingerprint(fingerprint, alias: alias) }
            await refreshTrusted()
            return true
        } catch {
            // The core may have saved the device and only failed to read it back.
            await refreshTrusted()
            let wanted = Self.compactFingerprint(fingerprint)
            if trusted.contains(where: { Self.compactFingerprint($0.fingerprint) == wanted }) {
                return true
            }
            show(error, title: String(localized: "Not added"))
            return false
        }
    }

    private static func compactFingerprint(_ value: String) -> String {
        String(value.uppercased().filter { $0.isLetter || $0.isNumber })
    }

    public func rename(_ device: TrustedDeviceInfo, to alias: String) async {
        let fp = device.fingerprint
        _ = try? await work { [core] in try core.renameTrusted(fp, alias: alias) }
        await refreshTrusted()
    }

    public func forget(_ device: TrustedDeviceInfo) async {
        let fp = device.fingerprint
        _ = try? await work { [core] in try core.untrust(fp) }
        await refreshTrusted()
    }

    // MARK: Receiving

    /// Changes who may send to this device.
    public func setVisibility(_ value: ReceiveVisibility) async {
        visibility = value
        settings.visibility = value
        await applyVisibility()
    }

    public func setAutoAcceptPaired(_ value: Bool) async {
        autoAcceptPaired = value
        settings.autoAcceptPaired = value
        await applyVisibility()
    }

    /// Makes the receiver match `visibility` and the accept setting. Calls
    /// run one after another (launch and "became active" both ask), so the
    /// receiver is never started twice; `enterBackground()` queues behind
    /// them too.
    public func applyVisibility() async {
        let previous = visibilityTask
        let task = Task { [weak self] in
            await previous?.value
            await self?.applyVisibilityNow()
        }
        visibilityTask = task
        await task.value
    }

    private func applyVisibilityNow() async {
        guard !isInBackground else { return }
        let accept: AcceptMode = autoAcceptPaired ? .autoTrusted : .prompt
        guard let policy = visibility.policy else {
            if receiver != nil {
                let core = core
                await work { core.stopReceiving() }
                // Turning receiving back on waits for ReceiverStopped, then
                // starts a new receiver.
                receiver = nil
                receiverStopping = true
            }
            return
        }
        do {
            if receiver != nil {
                receiver = try await work { [core] in try core.setReceivePolicy(policy, accept: accept) }
            } else if !receiverStopping {
                let options = ReceiveOptions(policy: policy, accept: accept, port: nil, advertise: true)
                receiver = try await work { [core] in try core.startReceiving(options) }
            }
        } catch {
            show(error, title: String(localized: "Receiving could not start"), action: .retryReceiving)
        }
    }

    private func stopReceivingAndWait() async {
        let core = core
        await work { core.stopReceiving() }
        receiver = nil
        receiverStopping = true
        for _ in 0..<50 where receiverStopping {
            try? await Task.sleep(for: .milliseconds(100))
        }
    }

    /// Addresses other devices can type to reach this one.
    public var connectionDetails: [String] {
        guard let port = receiver?.port else { return [] }
        return LocalAddresses.ipv4().map { "\($0.text):\(port)" }
    }

    // MARK: Discovery

    /// Call when a device list appears; browsing runs only while one is
    /// visible.
    public func beginBrowsing() {
        browseClients += 1
        if browseClients == 1 { syncDiscovery() }
    }

    public func endBrowsing() {
        browseClients = max(0, browseClients - 1)
        if browseClients == 0 { syncDiscovery() }
    }

    /// Starts or stops discovery to match `browseClients`. Calls run one
    /// after another, so a quick appear/disappear never leaves discovery
    /// running (or stopped) by mistake.
    private func syncDiscovery() {
        let previous = discoveryTask
        let core = core
        discoveryTask = Task { [weak self] in
            await previous?.value
            guard let self else { return }
            let wanted = self.browseClients > 0
            guard wanted != self.browsing else { return }
            self.browsing = wanted
            if wanted {
                do {
                    try await self.work { try core.startDiscovery() }
                    self.discoveryProblem = nil
                    self.nearby = await self.work { core.nearbyPeers() }
                } catch {
                    self.browsing = false
                    self.discoveryProblem = ErrorCatalog.message(for: error)
                }
            } else {
                await self.work { core.stopDiscovery() }
            }
        }
    }

    // MARK: Draft (files to send)

    public func addToDraft(_ files: [OutgoingFile]) {
        let existing = Set(draft.map(\.url))
        for file in files where !existing.contains(file.url) {
            draft.append(file)
        }
    }

    public func removeFromDraft(_ file: OutgoingFile) {
        draft.removeAll { $0.id == file.id }
        file.release()
    }

    public func clearDraft() {
        for file in draft { file.release() }
        draft.removeAll()
    }

    /// Picks up items handed over by the Share Extension.
    public func takeSharedItems() {
        guard let inbox else { return }
        inbox.removeStale()
        var taken: [OutgoingFile] = []
        for batch in inbox.pendingBatches() {
            let destination = paths.outgoingDir.appendingPathComponent("shared-\(batch.id)", isDirectory: true)
            guard let files = try? inbox.take(batch, into: destination) else {
                inbox.discard(batch)
                continue
            }
            for (file, item) in zip(files, batch.manifest.items) {
                taken.append(OutgoingFile(url: file, name: item.name, size: item.size, securityScoped: false, staged: true))
            }
        }
        if !taken.isEmpty {
            addToDraft(taken)
            route = .send
        }
    }

    // MARK: Sending

    /// Sends the draft. `allowUnverified` lets a receiver that is not paired
    /// take the files after both users compare a code.
    public func sendDraft(to target: SendTarget, targetName: String, allowUnverified: Bool) async {
        let files = draft
        guard !files.isEmpty else { return }
        let items = files.map { SendItem.path(path: $0.url.path) }
        do {
            let op = try await work { [core] in try core.send(target, items: items, allowUnverified: allowUnverified) }
            let id = op.id()
            operations[id] = op
            let record = TransferRecord(
                id: "out-\(id)", direction: .outgoing, operation: id, targetName: targetName,
                fileNames: files.map(\.name), totalBytes: files.reduce(0) { $0 + $1.size },
                phase: .connecting, startedAt: clock())
            active.insert(record, at: 0)
            held[record.id] = files
            draft.removeAll()
        } catch {
            show(error, title: String(localized: "Not sent"))
        }
    }

    public func cancel(_ record: TransferRecord) {
        if let op = record.operation, let handle = operations[op] {
            Task.detached { handle.cancel() }
        } else if let id = record.transferId {
            let core = core
            Task.detached { _ = core.cancelIncoming(id) }
        }
    }

    // MARK: Prompts

    /// The prompt to show now.
    public var currentPrompt: Prompt? { prompts.first }

    public func answer(_ prompt: Prompt, yes: Bool) async {
        let id = prompt.promptId
        guard !answering.contains(id) else { return }
        answering.insert(id)
        defer { answering.remove(id) }
        do {
            switch prompt {
            case .offer:
                try await work { [core] in try core.answerOffer(id, accept: yes) }
            case .code:
                try await work { [core] in try core.answerCode(id, confirmed: yes) }
            }
        } catch DropumiError.PromptNotFound {
            prompts.removeAll { $0.promptId == id }
        } catch {
            show(error, title: String(localized: "The answer was not sent"))
        }
    }

    // MARK: Pairing

    public func pair(with target: SendTarget, name: String) async {
        do {
            let op = try await work { [core] in try core.pair(target) }
            let id = op.id()
            operations[id] = op
            pairing = PairingSession(operation: id, role: .initiator(name: name), port: nil, expiresAt: nil, result: nil)
        } catch {
            show(error, title: String(localized: "Pairing could not start"))
        }
    }

    /// Lets one device pair with this one for five minutes.
    public func openPairingWindow() async {
        do {
            let op = try await work { [core] in try core.openPairingWindow(minutes: 5) }
            let id = op.id()
            operations[id] = op
            pairing = PairingSession(
                operation: id, role: .window, port: nil, expiresAt: clock().addingTimeInterval(300), result: nil)
        } catch {
            show(error, title: String(localized: "Pairing could not start"))
        }
    }

    public func cancelPairing() {
        guard let pairing, let handle = operations[pairing.operation] else {
            self.pairing = nil
            return
        }
        Task.detached { handle.cancel() }
    }

    public func dismissPairing() {
        pairing = nil
    }

    // MARK: History

    public func clearHistory() {
        history.removeAll()
        historyStore.save(history)
    }

    // MARK: Events

    /// Applies a batch of core events.
    public func apply(_ batch: [NodeEvent]) {
        for event in batch {
            apply(event)
        }
    }

    private func apply(_ event: NodeEvent) {
        switch event {
        case let .receiverStarted(info):
            receiver = info
            receiverStopping = false
        case let .receiverStopped(reason):
            receiver = nil
            receiverStopping = false
            if reason == .listenerFailed || reason == .internal, !isInBackground, visibility != .off {
                banner = Banner(style: .warning, title: String(localized: "Receiving stopped"),
                                message: String(localized: "This device stopped listening for files."),
                                action: .retryReceiving)
            } else if reason == .stopped, onboarded, !isInBackground, visibility != .off {
                // A stop that finished after the user turned receiving
                // back on (or the app came back to the front meanwhile).
                Task { await applyVisibility() }
            }
        case let .policyChanged(policy, accept, everyoneUntil):
            receiver = receiver.map {
                ReceiverInfo(port: $0.port, instance: $0.instance, policy: policy, accept: accept, everyoneUntil: everyoneUntil)
            }
            if visibility == .everyone, policy == .trustedOnly {
                visibility = .pairedOnly
                settings.visibility = .pairedOnly
                banner = Banner(style: .info, title: String(localized: "Back to paired devices only"),
                                message: String(localized: "The 10 minutes for everyone ended."))
            }
        case let .discoveryUnavailable(reason):
            discoveryProblem = reason
            localNetworkDenied = reason.localizedCaseInsensitiveContains("local network")
        case let .peerFound(peer):
            if let i = nearby.firstIndex(where: { $0.instance == peer.instance }) {
                nearby[i] = peer
            } else {
                nearby.append(peer)
            }
            nearby.sort { ($0.hintAlias != nil ? 0 : 1, $0.name) < ($1.hintAlias != nil ? 0 : 1, $1.name) }
            discoveryProblem = nil
            localNetworkDenied = false
        case let .peerLost(instance):
            nearby.removeAll { $0.instance == instance }
        case let .connected(operation, peer):
            if let operation { record(for: operation)?.peer = peer }
        case let .sessionCode(operation, peer, code):
            if let operation, let r = record(for: operation) {
                r.peer = peer
                r.sessionCode = code
            }
        case let .offerSent(operation, transferId, peer, _, totalBytes):
            if let r = record(for: operation) {
                r.transferId = transferId
                r.peer = peer
                r.totalBytes = totalBytes
                r.phase = .waitingForAcceptance
            }
        case let .offerReceived(transferId, peer, files, totalBytes):
            let id = "in-\(transferId)"
            guard !active.contains(where: { $0.id == id }) else { break }
            let names = offerFileNames(transferId) ?? []
            let record = TransferRecord(
                id: id, direction: .incoming, operation: nil, targetName: peer.claimedName,
                fileNames: names, totalBytes: totalBytes, phase: .waitingForAnswer, startedAt: clock())
            record.transferId = transferId
            record.peer = peer
            record.fileCount = Int(files)
            active.insert(record, at: 0)
        case let .promptOpened(prompt):
            guard !prompts.contains(where: { $0.promptId == prompt.promptId }) else { break }
            prompts.append(prompt)
            if case let .offer(_, transferId, _, _, files, _, _, _) = prompt,
               let r = active.first(where: { $0.transferId == transferId && $0.direction == .incoming }) {
                r.fileNames = files.map(\.name)
            }
        case let .promptClosed(id, _):
            prompts.removeAll { $0.promptId == id }
        case let .transferAccepted(operation, transferId):
            let r = operation.flatMap { record(for: $0) } ?? incoming(transferId)
            r?.phase = .transferring
        case let .transferProgress(operation, transferId, progress):
            let r = operation.flatMap { record(for: $0) } ?? incoming(transferId)
            r?.record(progress, at: clock())
        case let .fileSent(operation, _, index, status):
            record(for: operation)?.fileResults.append(FileResult(index: index, status: status, savedAs: nil, blake3: nil))
        case let .fileSaved(transferId, index, savedAs, _, blake3):
            incoming(transferId)?.fileResults.append(
                FileResult(index: index, status: .stored, savedAs: savedAs, blake3: blake3))
        case let .fileFailed(transferId, index, status):
            incoming(transferId)?.fileResults.append(FileResult(index: index, status: status, savedAs: nil, blake3: nil))
        case let .transferFinished(operation, direction, result):
            let r = operation.flatMap { record(for: $0) }
                ?? result.transferId.flatMap { incoming($0) }
            if let r {
                finish(r, result: result)
            } else if direction == .incoming {
                // Auto-accepted or never announced: still record the result.
                let r = TransferRecord(
                    id: "in-\(result.transferId ?? UUID().uuidString)", direction: .incoming, operation: nil,
                    targetName: String(localized: "A device"), fileNames: [], totalBytes: result.bytes,
                    phase: .transferring, startedAt: clock())
                finish(r, result: result)
            }
        case let .pairingWindowOpened(operation, port):
            if pairing?.operation == operation { pairing?.port = port }
        case let .pairingFinished(operation, result):
            operations[operation] = nil
            if pairing?.operation == operation {
                pairing?.result = result
            }
            Task { await refreshTrusted() }
        case let .sessionClosed(peer, code):
            if code == "not_authorized" { noticeRefused(peer) }
        case let .warning(message):
            showWarning(message)
        case .eventsDropped:
            break
        case .resyncRequired:
            resync()
        }
    }

    private func record(for operation: UInt64) -> TransferRecord? {
        active.first { $0.operation == operation }
    }

    private func incoming(_ transferId: String) -> TransferRecord? {
        active.first { $0.direction == .incoming && $0.transferId == transferId }
    }

    private func offerFileNames(_ transferId: String) -> [String]? {
        for prompt in prompts {
            if case let .offer(_, id, _, _, files, _, _, _) = prompt, id == transferId {
                return files.map(\.name)
            }
        }
        return nil
    }

    private func finish(_ record: TransferRecord, result: TransferResult) {
        record.phase = .finished(result)
        record.finishedAt = clock()
        if let op = record.operation { operations[op] = nil }
        for file in held.removeValue(forKey: record.id) ?? [] {
            file.release()
        }
        active.removeAll { $0.id == record.id }
        let label = record.peerLabel
        let entry = HistoryEntry(
            id: record.id + "-" + String(Int(clock().timeIntervalSince1970)),
            outgoing: record.direction == .outgoing,
            peerName: label.title, peerVerified: label.verified,
            fileNames: record.fileNames.isEmpty
                ? record.fileResults.compactMap(\.savedAs)
                : record.fileNames,
            totalBytes: record.totalBytes,
            outcome: HistoryEntry.outcome(of: result.status),
            summary: ErrorCatalog.summary(result, direction: record.direction),
            finishedAt: clock(),
            savedAs: record.fileResults.compactMap(\.savedAs))
        history.insert(entry, at: 0)
        historyStore.save(history)
    }

    /// The core refused a connection (`not_authorized`). Rate-limited, and
    /// never in place of another notice: any host on the network can cause it.
    ///
    /// Without a peer the refusal came during the handshake and its reason is
    /// not known: a device that isn't paired, a pairing request while no
    /// pairing window is open (from paired devices too), a paired device
    /// while a window is open, or an unreadable list of paired devices. So the
    /// notice says only that a device was refused, and never offers Everyone
    /// (as on Android).
    private func noticeRefused(_ peer: PeerInfo?) {
        if let peer, peer.trusted { return }
        guard banner == nil else { return }
        let now = clock()
        if let last = lastUnpairedNotice, now.timeIntervalSince(last) < 60 { return }
        lastUnpairedNotice = now
        if peer == nil {
            banner = Banner(
                style: .info, title: String(localized: "A device was refused"),
                message: String(localized: """
                    A device on this network tried to connect and was turned away. \
                    To pair with it, choose Wait for a device under Pair a device first.
                    """))
        } else {
            banner = Banner(style: .info, title: String(localized: "A device that isn't paired tried to send"),
                            message: String(localized: "Pair with it, or let everyone send for 10 minutes."),
                            action: .openEveryone)
        }
    }

    private func resync() {
        let snapshot = core.snapshot()
        receiver = snapshot.receiver
        receiverStopping = snapshot.receiverStopping
        prompts = snapshot.prompts
        nearby = snapshot.nearby
    }

    // MARK: Helpers

    /// A warning from the core (today: a change saved without a confirmed
    /// directory sync). Added to a banner already showing, so a message
    /// such as "New identity" is not lost.
    private func showWarning(_ message: String) {
        let text = message.prefix(1).uppercased() + message.dropFirst()
        if let current = banner {
            banner = Banner(style: current.style, title: current.title,
                            message: current.message + "\n\n" + text, action: current.action)
        } else {
            banner = Banner(style: .warning, title: String(localized: "Saved, but not confirmed on disk"), message: text)
        }
    }

    private func show(_ error: Error, title: String, action: Banner.Action? = nil) {
        banner = Banner(style: .warning, title: title, message: ErrorCatalog.message(for: error), action: action)
    }

    /// Runs a core call off the main thread, at the default priority of the
    /// core's own threads (a higher one would wait on lower-priority threads).
    private nonisolated func work<T: Sendable>(_ body: @escaping @Sendable () throws -> T) async throws -> T {
        try await Task.detached(priority: .medium) { try body() }.value
    }

    private nonisolated func work<T: Sendable>(_ body: @escaping @Sendable () -> T) async -> T {
        await Task.detached(priority: .medium) { body() }.value
    }

    /// Test hook: the operation handles the model holds.
    var operationCount: Int { operations.count }
}

extension Prompt {
    /// The prompt id.
    public var promptId: UInt64 {
        switch self {
        case let .offer(id, _, _, _, _, _, _, _): return id
        case let .code(id, _, _, _, _, _): return id
        }
    }
}
