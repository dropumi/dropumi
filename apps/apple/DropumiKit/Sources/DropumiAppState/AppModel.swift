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
    public enum Action: Equatable, Sendable { case retryReceiving, openEveryone, openSettings, openPairing }
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
    /// The device on the other end, once connected: only a claim until
    /// `PairingFinished(.paired)`, and never shown as paired before.
    public var peer: PeerInfo?
    /// A code was shown for this attempt (`SessionCode` or a code prompt).
    public var reachedCode = false
    /// When that code expires.
    public var codeExpiresAt: Date?

    init(operation: UInt64, role: Role, expiresAt: Date? = nil) {
        self.operation = operation
        self.role = role
        self.expiresAt = expiresAt
    }
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
    /// What the pairing screen shows.
    public private(set) var pairingFlow = PairingFlow()
    /// This device's pairing window is opening or open, or the core is
    /// resuming the receiver it paused: receiving settings apply after it.
    public private(set) var receivingPausedForPairing = false
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
    private let timing: PairingTiming
    private let timer: any ModelTimer

    // The pairing hold. This device's pairing window is tracked by its own
    // operation, never by the pairing shown on screen. Opening a window
    // pauses a running receiver (ReceiverStopped); when the window ends
    // (PairingFinished) the core resumes that receiver with the settings it
    // had (ReceiverStarted, or ReceiverStopped if it could not). The core
    // refuses receiver changes until then, so the model leaves the receiver
    // alone, and then makes it match the settings of the moment.

    /// Window opens under way.
    private var windowStarts = 0
    /// The open window's operation; cleared only by its PairingFinished.
    private var windowOperation: UInt64?
    /// A receiver stopped while the window was opening or open: the core
    /// resumes it afterwards.
    private var windowPausedReceiver = false
    /// The window ended and the core is resuming the receiver it paused.
    private var resumeExpected = false
    private var resumeEpoch = 0
    private var holdActive: Bool { windowStarts > 0 || windowOperation != nil || resumeExpected }

    // The pairing screen. Showing it makes this device ready to pair (it
    // opens a window); tapping a device closes that window and asks the
    // device. Each verdict comes from PairingClassifier.

    /// Pairing screens on show.
    private var pairingScreenClients = 0
    /// The window was closed for the background: it opens again when the
    /// app is back in front, if the screen is still shown.
    private var reopenOnActive = false
    /// An outcome the pairing screen shows when it is back: it came while
    /// no pairing screen was shown, or a prompt covered the screen showing
    /// it (iOS).
    private var outcomeKeptWhileAway = false
    /// A tap is closing this device's window or starting the request.
    private var pairStarting = false
    /// Cancel was tapped while `pairStarting`.
    private var requestAborted = false
    /// A window opening: waiting for the one before, or the core call.
    private var windowOpenTask: Task<Void, Never>?
    /// Counts window opens, so a close knows a newer one began.
    private var windowAttempt = 0
    /// Why the opening window is to be closed once it is registered.
    private var pendingWindowCancel: PairingCancelReason?
    /// The opening window was closed and then asked for again: it opens
    /// once the first has ended.
    private var reopenAfterOpening = false
    /// How the last window ended; unknown again once the next one starts
    /// opening.
    private var lastWindowResult: PairingResult?
    /// Why this device cancelled each of its pairing operations.
    private var cancelReasons: [UInt64: PairingCancelReason] = [:]
    /// This device's answer to each code, written before it is sent: the
    /// result can arrive before the call returns.
    private var codeAnswers: [UInt64: Bool] = [:]
    /// Window opens and requests whose core call has not returned yet.
    private var pairingStartsUnderWay = 0
    /// Events for an operation whose start has not returned yet.
    private var early: [UInt64: [NodeEvent]] = [:]
    private static let earlyOperations = 16
    private static let earlyEventsPerOperation = 64
    private var pairingTasks: [UUID: Task<Void, Never>] = [:]

    private struct Waiter {
        let id: Int
        let ready: @MainActor () -> Bool
        let resume: (Bool) -> Void
    }
    private var waiters: [Waiter] = []
    private var lastWaiter = 0
    /// Test hook: batches applied from the core's event stream.
    private(set) var streamBatches = 0

    public init(
        core: CoreClient,
        paths: AppPaths,
        settings: SettingsStore = SettingsStore(),
        inbox: ShareInbox? = nil,
        clock: @escaping @Sendable () -> Date = { Date() },
        timing: PairingTiming = PairingTiming(),
        timer: any ModelTimer = SystemTimer()
    ) {
        self.core = core
        self.paths = paths
        self.settings = settings
        self.historyStore = HistoryStore(file: paths.historyFile)
        self.inbox = inbox
        self.clock = clock
        self.timing = timing
        self.timer = timer
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
                self?.streamBatches += 1
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
    /// receiving. A pairing window nobody is pairing through closes (it
    /// opens again when the app is back in front on the pairing screen); a
    /// code comparison under way is kept.
    public func enterBackground() {
        isInBackground = true
        if windowActive, !windowEngaged, windowCloseReason == nil {
            reopenOnActive = pairingScreenClients > 0
            beginClosingWindow(.background)
        }
        // Queued behind the receiver changes already under way, so a
        // receiver that is still starting stops accepting once it is up.
        chainReceiver { await $0.stopAcceptingInBackground() }
    }

    private func stopAcceptingInBackground() async {
        // Back in front meanwhile: `becomeActive()` keeps the receiver.
        guard isInBackground, receiver != nil else { return }
        let core = core
        await work { core.stopAccepting() }
    }

    /// iOS: the app is in front again. Still on the pairing screen, this
    /// device gets ready to pair again, unless a result came meanwhile.
    public func becomeActive() async {
        isInBackground = false
        if reopenOnActive {
            reopenOnActive = false
            requestWindow()
        }
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
        guard await pairingLeavesIdentityAlone(title: String(localized: "The name was not changed")) else { return }
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
        guard await pairingLeavesIdentityAlone(title: String(localized: "The identity was not reset")) else { return }
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

    /// The core refuses identity changes while pairing (`check_quiet`).
    /// Waits for a window this device is closing (the pairing screen was
    /// just left) and for the receiver it paused; says so, and returns
    /// false, while pairing is under way.
    private func pairingLeavesIdentityAlone(title: String) async -> Bool {
        if windowCloseReason != nil {
            _ = await waitUntil(timeout: timing.windowCloseWait) { [weak self] in self?.windowGone ?? true }
        }
        if pairStarting || requestRunning || windowActive {
            banner = Banner(style: .warning, title: title,
                            message: String(localized: "Pairing is under way. Finish or stop it first."))
            return false
        }
        if holdActive {
            _ = await waitUntil(timeout: timing.resumeWait) { [weak self] in self?.holdActive != true }
        }
        return true
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

    /// Changes who may send to this device. During the pairing hold the
    /// change is kept and applied when the hold ends (the core resumes the
    /// paused receiver with its old settings; the model then changes it).
    public func setVisibility(_ value: ReceiveVisibility) async {
        visibility = value
        settings.visibility = value
        await applyVisibility()
    }

    /// Like `setVisibility(_:)`, applied after the pairing hold.
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
        await chainReceiver { await $0.applyVisibilityNow() }.value
    }

    /// Queues `change` behind the receiver changes (and window opens)
    /// already under way, so they run one after another.
    @discardableResult
    private func chainReceiver(_ change: @escaping @MainActor (AppModel) async -> Void) -> Task<Void, Never> {
        let previous = visibilityTask
        let task = Task { [weak self] in
            await previous?.value
            guard let self else { return }
            await change(self)
        }
        visibilityTask = task
        return task
    }

    /// Publishes the pairing hold. When it ends, the receiver is made to
    /// match the settings of the moment (unless `reconcile` is false: the
    /// caller does that itself).
    private func syncPairingHold(reconcile: Bool = true) {
        let held = holdActive
        let was = receivingPausedForPairing
        if held != was {
            receivingPausedForPairing = held
            if was, reconcile {
                chainReceiver { await $0.matchReceiver() }
            }
        }
        checkWaiters()
    }

    /// Makes a running receiver (or none) match the settings now. The core
    /// also starts a receiver by itself: when a pairing window ends it
    /// resumes the one the window paused, with the settings it had then,
    /// and this may be after the user chose Off or left the app.
    private func matchReceiver() async {
        guard !holdActive, onboarded else { return }
        if isInBackground {
            await stopAcceptingInBackground()
            return
        }
        if let receiver, matchesSettings(receiver) { return }
        await applyVisibilityNow()
    }

    /// Whether a running receiver already has the chosen policy kind and
    /// accept mode.
    private func matchesSettings(_ info: ReceiverInfo) -> Bool {
        guard info.accept == acceptMode else { return false }
        switch (visibility, info.policy) {
        case (.pairedOnly, .trustedOnly), (.everyone, .everyone): return true
        default: return false
        }
    }

    private var acceptMode: AcceptMode { autoAcceptPaired ? .autoTrusted : .prompt }

    private func applyVisibilityNow() async {
        guard !isInBackground else { return }
        // The core refuses receiver changes during the pairing hold, and
        // resumes the receiver with its old settings afterwards; the change
        // waits until then (syncPairingHold, ReceiverStarted).
        guard !holdActive else { return }
        let accept = acceptMode
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

    /// Sends the user's answer. A code answer is noted first: the pairing
    /// can end before the call returns, and only this device's answer tells
    /// "you said no" from "the other device said no".
    public func answer(_ prompt: Prompt, yes: Bool) async {
        let id = prompt.promptId
        guard !answering.contains(id) else { return }
        answering.insert(id)
        defer { answering.remove(id) }
        var pairingOperation: UInt64?
        if case let .code(_, operation?, _, _, .pairing, _) = prompt {
            pairingOperation = operation
            codeAnswers[operation] = yes
        }
        do {
            switch prompt {
            case .offer:
                try await work { [core] in try core.answerOffer(id, accept: yes) }
            case .code:
                try await work { [core] in try core.answerCode(id, confirmed: yes) }
            }
        } catch DropumiError.PromptNotFound {
            if let pairingOperation { codeAnswers[pairingOperation] = nil }
            prompts.removeAll { $0.promptId == id }
        } catch {
            if let pairingOperation { codeAnswers[pairingOperation] = nil }
            show(error, title: String(localized: "The answer was not sent"))
        }
    }

    // MARK: Pairing

    /// The pairing screen appeared. The first one browses and makes this
    /// device ready to pair (it opens a window), unless this device is
    /// already pairing, or an outcome that came meanwhile (or that a prompt
    /// covered) is still to be seen.
    public func enterPairingScreen() {
        pairingScreenClients += 1
        beginBrowsing()
        guard pairingScreenClients == 1 else { return }
        let away = outcomeKeptWhileAway
        outcomeKeptWhileAway = false
        // A request started just before the screen appeared, or a device
        // pairing with this one: the screen shows it.
        if pairStarting || requestRunning || windowEngaged { return }
        switch pairingFlow.phase {
        case .finished, .paused:
            if away { return }
        default:
            break
        }
        pairingFlow = PairingFlow(note: away ? pairingFlow.note : nil)
        requestWindow()
    }

    /// The pairing screen went away. The last one stops browsing and closes
    /// this device's window, unless a device is pairing through it (the iOS
    /// code prompt covers the screen). A request keeps running; an outcome
    /// that comes meanwhile is shown when the screen is back, and so is one
    /// a prompt covered.
    public func leavePairingScreen() {
        guard pairingScreenClients > 0 else { return }
        pairingScreenClients -= 1
        endBrowsing()
        guard pairingScreenClients == 0 else { return }
        reopenOnActive = false
        if windowActive, !windowEngaged {
            beginClosingWindow(.leaving)
        }
        switch pairingFlow.phase {
        case .finished, .paused:
            if currentPrompt != nil {
                // On iOS any prompt (an offer too) covers the screen, which
                // then goes away: the result waits for a tap when it is
                // back.
                outcomeKeptWhileAway = true
            } else {
                pairingFlow = PairingFlow()
            }
        case .opening, .waiting:
            pairingFlow.phase = .idle
        case .idle, .switching, .asking, .comparing:
            break
        }
    }

    /// Start again, Try again, Pair another device: this device gets ready
    /// to pair again.
    public func startWaiting() {
        if pairing?.result != nil { pairing = nil }
        outcomeKeptWhileAway = false
        pairingFlow = PairingFlow()
        requestWindow()
    }

    /// Stop: this device is no longer ready to pair. The screen then goes
    /// away.
    public func stopPairing() {
        reopenOnActive = false
        pairingFlow = PairingFlow()
        if windowActive {
            beginClosingWindow(.user)
        }
    }

    /// Asks another device to pair (a tap). A tap while this device is
    /// already pairing is ignored. This device's own window closes first:
    /// it is the one asking now.
    public func startPairing(with target: SendTarget, name: String) {
        guard beginRequest(name: name) else { return }
        track { await $0.runRequest(target, name: name) }
    }

    /// `startPairing(with:name:)`, returning once the core has answered.
    /// Without the pairing screen, a request that cannot start shows a
    /// banner.
    public func pair(with target: SendTarget, name: String) async {
        guard beginRequest(name: name) else { return }
        await runRequest(target, name: name)
    }

    /// The part of a tap that runs at once, so a second tap finds it.
    private func beginRequest(name: String) -> Bool {
        guard !pairStarting, !requestRunning, !windowEngaged else { return false }
        pairStarting = true
        requestAborted = false
        outcomeKeptWhileAway = false
        pairingFlow = PairingFlow(phase: windowActive ? .switching(name: name) : .asking(name: name))
        return true
    }

    private func runRequest(_ target: SendTarget, name: String) async {
        defer { pairStarting = false }
        // The core takes a request while the window is open; it is closed
        // first so that nothing else uses it meanwhile.
        if windowActive, case .paired? = await closeOwnWindow(.dialing) {
            // A device paired with this one meanwhile: that is shown.
            return
        }
        if requestAborted {
            pairStarting = false
            pairingFlow = PairingFlow(note: .cancelled)
            requestWindow()
            return
        }
        pairingFlow.phase = .asking(name: name)
        pairingStartsUnderWay += 1
        defer { endPairingStart() }
        do {
            let op = try await work { [core] in try core.pair(target) }
            let id = op.id()
            operations[id] = op
            pairing = PairingSession(operation: id, role: .initiator(name: name))
            pairStarting = false
            // Prompts first: a kept end is then classified with the code.
            codePromptsShown(for: id)
            replayEarly(id)
            if requestAborted { cancelOperation(id, .user) }
        } catch {
            pairStarting = false
            if pairingScreenClients > 0 {
                let note = requestAborted ? .cancelled : PairingClassifier.classify(requestError: error, name: name)
                pairingFlow = PairingFlow(note: note)
                requestWindow()
            } else {
                pairingFlow = PairingFlow()
                show(error, title: String(localized: "Pairing could not start"))
            }
        }
    }

    /// Cancel, while asking a device: this device gets ready to pair again,
    /// with "Pairing cancelled.". On this device's window: Stop.
    public func cancelPairing() {
        if pairStarting {
            requestAborted = true
            return
        }
        guard let pairing, pairing.result == nil, operations[pairing.operation] != nil else {
            self.pairing = nil
            // Nothing is asking any more: this device gets ready again.
            switch pairingFlow.phase {
            case .switching, .asking:
                pairingFlow = PairingFlow(note: .cancelled)
                requestWindow()
            case .idle, .opening, .waiting, .comparing, .paused, .finished:
                break
            }
            return
        }
        cancelOperation(pairing.operation, .user)
    }

    /// Done: the outcome was seen.
    public func dismissPairing() {
        if pairing?.result != nil { pairing = nil }
        outcomeKeptWhileAway = false
        pairingFlow = PairingFlow()
    }

    /// Lets one device pair with this one for five minutes, without the
    /// pairing screen (which opens its own window), and returns once the
    /// core has answered. The core pauses a running receiver before the
    /// call returns and resumes it when the window ends; receiving settings
    /// wait until then (the pairing hold).
    public func openPairingWindow() async {
        guard !pairStarting, !requestRunning else { return }
        if windowOpenTask == nil, windowOperation.map({ cancelReasons[$0] != nil }) ?? true {
            pairingFlow = PairingFlow(phase: .opening)
            startWindowOpen(forScreen: false)
        }
        await windowOpenTask?.value
    }

    /// The pairing screen wants this device ready to pair. Only these ask:
    /// the screen appearing, Start again, a request of this device that
    /// ended before a code, and the app back in front on the screen.
    private func requestWindow() {
        guard pairingScreenClients > 0, onboarded, !pairStarting, !requestRunning else { return }
        guard !isInBackground else {
            reopenOnActive = true
            return
        }
        if let window = windowOperation, cancelReasons[window] == nil {
            // Already open (without the screen).
            if windowEngaged {
                pairingFlow.phase = .comparing
            } else if let session = pairing, session.operation == window, session.port != nil,
                      let expiresAt = session.expiresAt {
                pairingFlow.phase = .waiting(expiresAt: expiresAt)
            } else {
                pairingFlow.phase = .opening
            }
            return
        }
        pairingFlow.phase = .opening
        if windowOpenTask != nil {
            // One is opening; if it is being closed, the next opens after it.
            if pendingWindowCancel != nil { reopenAfterOpening = true }
            return
        }
        startWindowOpen(forScreen: true)
    }

    @discardableResult
    private func startWindowOpen(forScreen: Bool) -> Task<Void, Never> {
        windowAttempt += 1
        pendingWindowCancel = nil
        reopenAfterOpening = false
        // A close of this window must not report how an earlier one ended.
        lastWindowResult = nil
        let task = Task { [weak self] in
            guard let self else { return }
            await self.openWindowWhenFree(forScreen: forScreen)
            self.windowOpenTask = nil
            self.checkWaiters()
            if self.reopenAfterOpening {
                self.reopenAfterOpening = false
                self.requestWindow()
            }
        }
        windowOpenTask = task
        checkWaiters()
        return task
    }

    private func openWindowWhenFree(forScreen: Bool) async {
        // The window before may still be ending, or resuming the receiver
        // it paused.
        _ = await waitUntil(timeout: timing.resumeWait) { [weak self] in
            guard let self else { return true }
            return self.windowOperation == nil && !self.resumeExpected
        }
        // Closed (or no longer wanted) before it opened.
        guard pendingWindowCancel == nil, !forScreen || wantsWindow else { return }
        guard windowOperation == nil else {
            // The window before never reported its end.
            pairingFlow.phase = .paused(.couldNotWait(ErrorCatalog.text(for: .busy).message))
            return
        }
        windowStarts += 1
        syncPairingHold()
        // Behind the receiver changes under way, which the core refuses once
        // the window is open.
        await chainReceiver { await $0.openWindowNow(forScreen: forScreen) }.value
    }

    private func openWindowNow(forScreen: Bool) async {
        defer {
            windowStarts -= 1
            syncPairingHold()
        }
        // A receiver that is still stopping would stop during the window and
        // pass for its pause.
        if receiverStopping {
            _ = await waitUntil(timeout: timing.stopWait) { [weak self] in self?.receiverStopping != true }
        }
        // Only a stop from here on is this window's pause.
        if windowOperation == nil { windowPausedReceiver = false }
        let minutes = timing.windowMinutes
        pairingStartsUnderWay += 1
        defer { endPairingStart() }
        do {
            let op = try await work { [core] in try core.openPairingWindow(minutes: minutes) }
            let id = op.id()
            operations[id] = op
            windowOperation = id
            lastWindowResult = nil
            pairing = PairingSession(
                operation: id, role: .window, expiresAt: clock().addingTimeInterval(TimeInterval(minutes) * 60))
            // Prompts first: a kept end is then classified with the code.
            codePromptsShown(for: id)
            replayEarly(id)
            // Closed while it opened, or no longer wanted: closed now.
            let reason = pendingWindowCancel ?? (forScreen && !wantsWindow ? .leaving : nil)
            pendingWindowCancel = nil
            if let reason { cancelOperation(id, reason) }
        } catch {
            // Refused before the core paused anything.
            if windowOperation == nil { windowPausedReceiver = false }
            // Unless it was closed meanwhile, the screen says why.
            if pendingWindowCancel == nil {
                pairingFlow.phase = .paused(PairingClassifier.classify(windowError: error))
                if pairingScreenClients == 0 {
                    show(error, title: String(localized: "Pairing could not start"))
                }
            }
        }
    }

    /// Starts closing this device's window, opening or open. At once, so a
    /// window asked for next finds this one closing.
    private func beginClosingWindow(_ reason: PairingCancelReason) {
        reopenAfterOpening = false
        if let window = windowOperation {
            cancelOperation(window, reason)
        }
        if windowOpenTask != nil, pendingWindowCancel == nil {
            // Closed as soon as the core has opened it.
            pendingWindowCancel = reason
        }
    }

    /// Closes this device's window, opening or open, and waits until it has
    /// ended (bounded). Returns how it ended, or nil if it did not say in
    /// time or a newer window began meanwhile.
    private func closeOwnWindow(_ reason: PairingCancelReason) async -> PairingResult? {
        beginClosingWindow(reason)
        let attempt = windowAttempt
        _ = await waitUntil(timeout: timing.windowCloseWait) { [weak self] in
            guard let self else { return true }
            return self.windowGone || self.windowAttempt != attempt
        }
        return windowGone ? lastWindowResult : nil
    }

    /// Cancels one of this device's pairing operations, once, noting why.
    /// Tracked, so the test hook `idle()` waits until the core has it.
    private func cancelOperation(_ operation: UInt64, _ reason: PairingCancelReason) {
        guard cancelReasons[operation] == nil, let handle = operations[operation] else { return }
        cancelReasons[operation] = reason
        track { await $0.work { handle.cancel() } }
    }

    /// The window's operation ended: the core resumes the receiver it
    /// paused, if any, and the hold lasts until it has.
    private func windowEnded(_ result: PairingResult?) {
        windowOperation = nil
        lastWindowResult = result
        if windowPausedReceiver {
            windowPausedReceiver = false
            // The resume can be applied first, when the end was reported
            // before the window's call returned.
            if receiver == nil {
                resumeExpected = true
                // Bounded: if the core's resume never comes (or was not for
                // this window), the settings still apply.
                resumeEpoch += 1
                let epoch = resumeEpoch
                timer.schedule(after: timing.resumeWait) { [weak self] in
                    guard let self, self.resumeExpected, self.resumeEpoch == epoch else { return }
                    self.resumeExpected = false
                    self.syncPairingHold()
                }
            }
        }
        syncPairingHold()
    }

    /// The pairing screen is shown, in front, and nothing else is pairing.
    private var wantsWindow: Bool {
        pairingScreenClients > 0 && !isInBackground && onboarded && !pairStarting && !requestRunning
    }

    /// This device's request is running.
    private var requestRunning: Bool {
        guard let pairing, pairing.result == nil, case .initiator = pairing.role else { return false }
        return true
    }

    /// No window of this device is opening or open.
    private var windowGone: Bool { windowOperation == nil && windowOpenTask == nil }

    private var windowActive: Bool { !windowGone }

    /// A device is pairing through this device's window: it connected, or
    /// a code was shown.
    private var windowEngaged: Bool {
        guard let window = windowOperation, let pairing, pairing.operation == window else { return false }
        return pairing.peer != nil || pairing.reachedCode
    }

    /// Why this device is closing its window, if it is.
    private var windowCloseReason: PairingCancelReason? {
        if let window = windowOperation, let reason = cancelReasons[window] { return reason }
        return windowOpenTask != nil ? pendingWindowCancel : nil
    }

    /// Runs pairing work in the background; the test hook `idle()` waits
    /// for it.
    private func track(_ body: @escaping @MainActor (AppModel) async -> Void) {
        let id = UUID()
        pairingTasks[id] = Task { [weak self] in
            guard let self else { return }
            await body(self)
            self.pairingTasks[id] = nil
        }
    }

    // MARK: Pairing events

    /// Keeps an event for a pairing operation whose start has not returned
    /// yet: the core reports some before it does. Returns whether the event
    /// was taken (kept, or dropped past the bounds; the end is always kept).
    private func keepEarly(_ event: NodeEvent, for operation: UInt64) -> Bool {
        guard pairingStartsUnderWay > 0, operations[operation] == nil else { return false }
        var kept = early[operation] ?? []
        let isEnd: Bool
        if case .pairingFinished = event { isEnd = true } else { isEnd = false }
        let room = kept.count < Self.earlyEventsPerOperation
            && (early[operation] != nil || early.count < Self.earlyOperations)
        if isEnd || room {
            kept.append(event)
            early[operation] = kept
        }
        return true
    }

    private func replayEarly(_ operation: UInt64) {
        for event in early.removeValue(forKey: operation) ?? [] {
            apply(event)
        }
    }

    private func endPairingStart() {
        pairingStartsUnderWay -= 1
        // Events kept for an operation that never came.
        if pairingStartsUnderWay == 0 { early.removeAll() }
    }

    /// Code prompts the core opened for this operation before its start
    /// returned.
    private func codePromptsShown(for operation: UInt64) {
        for case let .code(_, op?, peer, _, _, expiresAtMs) in prompts where op == operation {
            pairingEngaged(operation, peer: peer, code: true, codeExpiresAt: Self.date(ms: expiresAtMs))
        }
    }

    private static func date(ms: UInt64) -> Date {
        Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
    }

    /// A device is pairing through the current attempt: it connected, or a
    /// code was shown. Its name stays a claim.
    private func pairingEngaged(_ operation: UInt64, peer: PeerInfo, code: Bool, codeExpiresAt: Date? = nil) {
        guard var session = pairing, session.operation == operation, session.result == nil else { return }
        session.peer = peer
        if code { session.reachedCode = true }
        if let codeExpiresAt { session.codeExpiresAt = codeExpiresAt }
        pairing = session
        // An attempt this device is ending shows its own next step.
        guard cancelReasons[operation] == nil else { return }
        switch pairingFlow.phase {
        case .finished, .paused:
            break
        case .idle, .opening, .waiting, .switching, .asking, .comparing:
            pairingFlow.phase = .comparing
        }
    }

    /// This device's window listens.
    private func windowListening(_ operation: UInt64, port: UInt16) {
        guard var session = pairing, session.operation == operation, case .window = session.role else { return }
        let expiresAt = clock().addingTimeInterval(TimeInterval(timing.windowMinutes) * 60)
        session.port = port
        session.expiresAt = expiresAt
        pairing = session
        if case .opening = pairingFlow.phase, cancelReasons[operation] == nil {
            pairingFlow.phase = .waiting(expiresAt: expiresAt)
        }
    }

    private func pairingFinished(_ operation: UInt64, _ result: PairingResult) {
        operations[operation] = nil
        let running = pairing.map { $0.operation == operation && $0.result == nil } ?? false
        if pairing?.operation == operation {
            pairing?.result = result
        }
        if operation == windowOperation { windowEnded(result) }
        if running, let session = pairing {
            let facts = facts(of: session, result: result)
            applyVerdict(PairingClassifier.classify(facts), facts)
        }
        codeAnswers[operation] = nil
        cancelReasons[operation] = nil
        track { await $0.refreshTrusted() }
    }

    private func facts(of session: PairingSession, result: PairingResult) -> PairingFacts {
        let now = clock()
        let answer = codeAnswers[session.operation]
        var name: String?
        var windowElapsed = false
        switch session.role {
        case let .initiator(listed):
            name = listed
        case .window:
            windowElapsed = session.expiresAt.map { now >= $0.addingTimeInterval(-2) } ?? false
        }
        let codeExpired = answer == nil && (session.codeExpiresAt.map { now >= $0.addingTimeInterval(-1) } ?? false)
        return PairingFacts(
            role: session.role, result: result, reachedCode: session.reachedCode, localAnswer: answer,
            localCodeExpired: codeExpired, windowElapsed: windowElapsed, listening: session.port != nil,
            cancelReason: cancelReasons[session.operation], name: name)
    }

    private func applyVerdict(_ verdict: PairingVerdict, _ facts: PairingFacts) {
        let shown = pairingScreenClients > 0
        switch verdict {
        case .silent:
            // This device ended it: a tap goes on to ask, and the screen
            // after Stop, or with no screen, shows nothing.
            guard facts.cancelReason == .user || !shown else { break }
            switch pairingFlow.phase {
            case .opening, .waiting, .comparing:
                pairingFlow.phase = .idle
            case .idle, .switching, .asking, .paused, .finished:
                break
            }
        case let .waitAgain(note):
            pairingFlow = PairingFlow(note: note)
            if shown {
                requestWindow()
            } else {
                outcomeKeptWhileAway = true
            }
        case let .paused(outcome):
            pairingFlow = PairingFlow(phase: .paused(outcome))
            // A card waits for a tap, also when the app comes back to the
            // front (the end can come after the background closed it).
            reopenOnActive = false
            if !shown { outcomeKeptWhileAway = true }
        case let .finished(outcome):
            pairingFlow = PairingFlow(phase: .finished(outcome))
            reopenOnActive = false
            if !shown { outcomeKeptWhileAway = true }
        }
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
        checkWaiters()
    }

    private func apply(_ event: NodeEvent) {
        switch event {
        case let .receiverStarted(info):
            receiverStarted(info)
        case let .receiverStopped(reason):
            receiverStopped(reason)
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
            guard let operation else { break }
            if let r = record(for: operation) {
                r.peer = peer
            } else if pairing?.operation == operation {
                pairingEngaged(operation, peer: peer, code: false)
            } else {
                _ = keepEarly(event, for: operation)
            }
        case let .sessionCode(operation, peer, code):
            guard let operation else { break }
            if let r = record(for: operation) {
                r.peer = peer
                r.sessionCode = code
            } else if pairing?.operation == operation {
                pairingEngaged(operation, peer: peer, code: true)
            } else {
                _ = keepEarly(event, for: operation)
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
            // A code for a start that has not returned is found when it does.
            if case let .code(_, operation?, peer, _, _, expiresAtMs) = prompt {
                pairingEngaged(operation, peer: peer, code: true, codeExpiresAt: Self.date(ms: expiresAtMs))
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
            if pairing?.operation == operation {
                windowListening(operation, port: port)
            } else {
                _ = keepEarly(event, for: operation)
            }
        case let .pairingFinished(operation, result):
            if operations[operation] == nil, pairing?.operation != operation, operation != windowOperation,
               keepEarly(event, for: operation) {
                break
            }
            pairingFinished(operation, result)
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

    /// A receiver started. The model's own starts already set it (this event
    /// can come before or after the call returns); one the model already
    /// asked to stop is ignored. The core also starts one by itself after a
    /// pairing window, with the settings it had: it is made to match the
    /// settings now.
    private func receiverStarted(_ info: ReceiverInfo) {
        if receiverStopping, !resumeExpected { return }
        receiver = info
        receiverStopping = false
        if resumeExpected {
            resumeExpected = false
            syncPairingHold()
        } else {
            chainReceiver { await $0.matchReceiver() }
        }
    }

    private func receiverStopped(_ reason: ReceiverStopReason) {
        // While a window opens or is open, this is its pause: the core
        // resumes this receiver when the window ends.
        if windowStarts > 0 || windowOperation != nil { windowPausedReceiver = true }
        receiver = nil
        receiverStopping = false
        // The resume after a window failed: said below, like any other stop.
        if resumeExpected {
            resumeExpected = false
            syncPairingHold(reconcile: false)
        }
        if reason == .listenerFailed || reason == .internal, !isInBackground, visibility != .off {
            banner = Banner(style: .warning, title: String(localized: "Receiving stopped"),
                            message: String(localized: "This device stopped listening for files."),
                            action: .retryReceiving)
        } else if reason == .stopped, onboarded, !isInBackground, visibility != .off, !holdActive {
            // A stop that finished after the user turned receiving
            // back on (or the app came back to the front meanwhile). A
            // pairing window's pause is not restarted here: the core
            // resumes the receiver after the window.
            chainReceiver { await $0.applyVisibilityNow() }
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
    /// notice says only that a device was refused and offers Pair a device;
    /// as on Android, it never offers Everyone.
    private func noticeRefused(_ peer: PeerInfo?) {
        if let peer, peer.trusted { return }
        // While the pairing screen is shown or this device's window is
        // opening or open, the pairing screen says what happened to it.
        if peer == nil, pairingScreenClients > 0 || windowActive { return }
        guard banner == nil else { return }
        let now = clock()
        if let last = lastUnpairedNotice, now.timeIntervalSince(last) < 60 { return }
        lastUnpairedNotice = now
        if peer == nil {
            banner = Banner(
                style: .info, title: String(localized: "A device was refused"),
                message: String(localized: """
                    A device on this network tried to connect and was turned away. \
                    If it was trying to pair, open Pair a device on both devices.
                    """),
                action: .openPairing)
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
        resyncPairing(snapshot)
        // A receiver the core started (or stopped) may have been among the
        // dropped events.
        chainReceiver { await $0.matchReceiver() }
    }

    /// Pairing ends that were among the dropped events, so the window's
    /// hold always ends.
    private func resyncPairing(_ snapshot: NodeSnapshot) {
        let session = pairing.flatMap { $0.result == nil ? $0.operation : nil }
        for op in Set([windowOperation, session].compactMap { $0 }) {
            let info = snapshot.operations.first { $0.id == op }
            if case let .pairing(result)? = info?.result {
                apply(.pairingFinished(operation: op, result: result))
            } else if info == nil, op == windowOperation {
                // Only finished operations are forgotten.
                windowEnded(nil)
            }
        }
        if resumeExpected, snapshot.receiver != nil {
            resumeExpected = false
            syncPairingHold(reconcile: false)
        }
    }

    // MARK: Waiting

    /// Waits until `ready()` holds or `timeout` has passed (nil: no limit),
    /// and returns whether it holds. Nothing polls: the condition is checked
    /// after each batch of events and wherever the pairing hold or a window
    /// open changes (`checkWaiters()`); the timeout comes from the model's
    /// timer.
    private func waitUntil(timeout: Duration?, _ ready: @escaping @MainActor () -> Bool) async -> Bool {
        if ready() { return true }
        lastWaiter += 1
        let id = lastWaiter
        return await withCheckedContinuation { continuation in
            waiters.append(Waiter(id: id, ready: ready, resume: { continuation.resume(returning: $0) }))
            if let timeout {
                timer.schedule(after: timeout) { [weak self] in self?.endWait(id, ready: false) }
            }
        }
    }

    /// Resumes every wait whose condition now holds, each exactly once.
    private func checkWaiters() {
        for waiter in waiters where waiter.ready() {
            endWait(waiter.id, ready: true)
        }
    }

    private func endWait(_ id: Int, ready: Bool) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        waiters.remove(at: index).resume(ready)
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

    /// Test hook: waits until the receiver changes, discovery changes,
    /// window opens and pairing work queued so far (and those they queue)
    /// have run.
    func idle() async {
        var done = Set<Task<Void, Never>>()
        while true {
            let queued = [visibilityTask, discoveryTask, windowOpenTask].compactMap { $0 } + Array(pairingTasks.values)
            guard let next = queued.first(where: { !done.contains($0) }) else { return }
            await next.value
            done.insert(next)
        }
    }

    /// Test hook: waits until the model has applied every batch the core
    /// delivered (`emitted()`, counted by the fake core) and has run the
    /// work they queued. Work that makes the core deliver more is waited
    /// for too, for at most 16 rounds.
    func settle(emitted: () -> Int) async {
        for _ in 0..<16 {
            let wanted = emitted()
            if eventTask != nil {
                _ = await waitUntil(timeout: nil) { [weak self] in (self?.streamBatches ?? .max) >= wanted }
            }
            await idle()
            if emitted() == wanted { return }
        }
    }
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
