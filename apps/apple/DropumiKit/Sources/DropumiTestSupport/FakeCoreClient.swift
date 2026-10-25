// SPDX-License-Identifier: MPL-2.0
//
// A scripted core for app-state tests and UI state tests. It records every
// call and lets a test push events. It never answers prompts or creates
// trust by itself: a UI state test that shows a transfer finishing does so
// only because the test pushed that event, and such tests are labelled
// "UI state test (fake core)".
//
// By default a call only records itself and returns. After
// `scriptPairingLikeTheCore()`, pairing windows and pairing requests behave
// like the real core (crates/ffi/src/node.rs).
//
// The app's Debug builds compile this file into the app for UI-test
// scenarios (Release builds exclude it, so the app never contains a fake
// core). The types are explicitly nonisolated (or main-actor) for that
// reason: the app's code is main-actor by default.

import DropumiAppState
import DropumiBindings
import DropumiCore
import Foundation

nonisolated public final class FakeOperation: CoreOperation, @unchecked Sendable {
    private let value: UInt64
    private let lock = NSLock()
    private var cancelledFlag = false
    private let onCancel: (@Sendable (UInt64) -> Void)?

    /// `onCancel` runs on every `cancel()`, on the caller's thread.
    public init(id: UInt64, onCancel: (@Sendable (UInt64) -> Void)? = nil) {
        self.value = id
        self.onCancel = onCancel
    }

    public func id() -> UInt64 { value }

    public func cancel() {
        lock.lock()
        cancelledFlag = true
        lock.unlock()
        onCancel?(value)
    }

    public var cancelled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancelledFlag
    }
}

/// Holds one fake core call until the test opens it, so a test can act
/// while the call is under way (no sleeps).
nonisolated public final class CallGate: @unchecked Sendable {
    private let release = DispatchSemaphore(value: 0)
    private let arrivals: AsyncStream<Void>
    private let arrival: AsyncStream<Void>.Continuation

    init() {
        (arrivals, arrival) = AsyncStream.makeStream(of: Void.self)
    }

    /// Returns once the held call has started.
    public func reached() async {
        for await _ in arrivals { return }
    }

    /// Lets the held call return.
    public func open() {
        release.signal()
    }

    /// Called by the held call (off the main thread).
    func hold() {
        arrival.yield()
        release.wait()
    }
}

nonisolated public final class FakeCoreClient: CoreClient, @unchecked Sendable {
    public enum Call: Equatable, Sendable {
        case resetIdentity
        case startReceiving(ReceiveOptions)
        case stopAccepting
        case stopReceiving
        case setReceivePolicy(ReceivePolicy, AcceptMode)
        case answerOffer(UInt64, Bool)
        case answerCode(UInt64, Bool)
        case send(SendTarget, paths: [String], allowUnverified: Bool)
        case pair(SendTarget)
        case openPairingWindow
        /// `cancel()` on one of this fake's operations.
        case cancel(UInt64)
        case cancelIncoming(String)
        case trust(String)
        case untrust(String)
        case startDiscovery
        case stopDiscovery
    }

    public let events: AsyncStream<[NodeEvent]>
    private let continuation: AsyncStream<[NodeEvent]>.Continuation
    private let lock = NSLock()
    private var log: [Call] = []
    private var batches = 0
    private var nextOperation: UInt64 = 1
    private var operationsById: [UInt64: FakeOperation] = [:]
    private var kinds: [UInt64: OperationKind] = [:]
    /// Operations whose `PairingFinished` was delivered.
    private var ended: Set<UInt64> = []
    private var trustedList: [TrustedDeviceInfo] = []
    private var deviceName = "Dropumi-1A2B"
    private var receiving: ReceiverInfo?
    private var nearbyTable: [NearbyPeer] = []
    private var gates: [String: CallGate] = [:]
    private var duringCall: [String: @Sendable (UInt64) -> [NodeEvent]] = [:]
    private var scripted = false
    /// The open pairing window: the core's `pairing_window` flag.
    private var openWindow: UInt64?
    /// The receiver the open window paused, resumed when it ends.
    private var pausedReceiver: ReceiverInfo?
    private var pairedDuringWindow = false
    /// Errors to throw from the next calls, by method name.
    public var failures: [String: DropumiError] = [:]
    /// Methods that apply their change and then fail, as the core does when
    /// a change was saved but could not be read back.
    public var appliedButUnreadable: Set<String> = []
    private var currentFingerprint = FakeCoreClient.fingerprint

    public static let fingerprint = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRST"
    /// The fingerprint after `resetIdentity()`.
    public static let resetFingerprint = "QRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCD"

    public init() {
        (events, continuation) = AsyncStream.makeStream(of: [NodeEvent].self)
    }

    /// Delivers events as the core would, and keeps the state the core
    /// keeps with them: found and lost devices update `nearbyPeers()`,
    /// `ReceiverStarted` and `ReceiverStopped` update the receiver, and
    /// `PairingFinished` ends its operation. Ending the open window this way
    /// resumes nothing; `endWindow(_:with:resume:)` resumes like the core.
    public func emit(_ batch: NodeEvent...) {
        deliver(batch)
    }

    private func deliver(_ batch: [NodeEvent]) {
        lock.lock()
        for event in batch {
            switch event {
            case let .peerFound(peer):
                nearbyTable.removeAll { $0.instance == peer.instance }
                nearbyTable.append(peer)
            case let .peerLost(instance):
                nearbyTable.removeAll { $0.instance == instance }
            case let .receiverStarted(info):
                receiving = info
            case .receiverStopped:
                receiving = nil
            case let .pairingFinished(operation, _):
                ended.insert(operation)
                if operation == openWindow {
                    openWindow = nil
                    pausedReceiver = nil
                }
            default:
                break
            }
        }
        batches += 1
        lock.unlock()
        continuation.yield(batch)
    }

    /// How many batches were delivered (by `emit` or by the scripted core),
    /// so a test can wait until the model has applied them all.
    public var emittedBatches: Int {
        lock.withLock { batches }
    }

    public var calls: [Call] {
        lock.lock()
        defer { lock.unlock() }
        return log
    }

    /// Called after each recorded call, outside the lock, so that a UI-test
    /// scenario can react as a core would (for example, close a prompt once
    /// it is answered). Nil by default; the fake core still never answers a
    /// prompt or creates trust by itself.
    public var onCall: (@Sendable (Call) -> Void)? {
        get {
            lock.lock()
            defer { lock.unlock() }
            return callHook
        }
        set {
            lock.lock()
            callHook = newValue
            lock.unlock()
        }
    }
    private var callHook: (@Sendable (Call) -> Void)?

    public func operation(_ id: UInt64) -> FakeOperation? {
        lock.lock()
        defer { lock.unlock() }
        return operationsById[id]
    }

    private func note(_ call: Call) {
        lock.lock()
        log.append(call)
        let hook = callHook
        lock.unlock()
        hook?(call)
    }

    private func fail(_ method: String) throws {
        lock.lock()
        let error = failures.removeValue(forKey: method)
        lock.unlock()
        if let error { throw error }
    }

    /// After the change is applied: fail like a change that was saved but
    /// could not be read back.
    private func failAfterApplying(_ method: String) throws {
        lock.lock()
        let fails = appliedButUnreadable.contains(method)
        lock.unlock()
        if fails {
            throw DropumiError.Config(
                problem: .io, detail: "the change was saved, but it could not be read back: test")
        }
    }

    private func newOperation(_ kind: OperationKind) -> FakeOperation {
        lock.withLock { addOperation(kind) }
    }

    /// Call with the lock held.
    private func addOperation(_ kind: OperationKind) -> FakeOperation {
        let op = FakeOperation(id: nextOperation) { [weak self] id in self?.cancelled(id) }
        operationsById[nextOperation] = op
        kinds[nextOperation] = kind
        nextOperation += 1
        return op
    }

    // MARK: Gates, early events and refusals

    /// Holds the next call of `method` (for example "pair") until the test
    /// opens the returned gate. The call is recorded before it is held.
    public func holdNext(_ method: String) -> CallGate {
        let gate = CallGate()
        lock.withLock { gates[method] = gate }
        return gate
    }

    /// Holds the next `startReceiving` until the test opens the gate.
    public func holdNextStartReceiving() -> CallGate {
        holdNext("startReceiving")
    }

    private func gate(_ method: String) {
        let gate = lock.withLock { gates.removeValue(forKey: method) }
        gate?.hold()
    }

    /// The next call of `method` ("send", "pair" or "openPairingWindow")
    /// delivers these events for its new operation before it returns, as
    /// the core may (a refused connection can end a request at once).
    public func emitBeforeReturning(_ method: String, _ events: @escaping @Sendable (UInt64) -> [NodeEvent]) {
        lock.withLock { duringCall[method] = events }
    }

    private func started(_ method: String, _ operation: FakeOperation) -> FakeOperation {
        let events = lock.withLock { duringCall.removeValue(forKey: method) }
        if let events { deliver(events(operation.id())) }
        return operation
    }

    /// What the core refuses while a pairing window is open: starting or
    /// changing the receiver (`start_receiver`, `set_receive_policy`) and
    /// identity changes (`check_quiet`). Only a scripted window is ever
    /// open, so the other tests keep the fake's plain answers.
    private func refuseWhileWindowOpen(_ error: DropumiError) throws {
        if lock.withLock({ openWindow != nil }) { throw error }
    }

    private static let notQuiet = "stop receiving and wait for running operations first"

    // MARK: Pairing like the core

    /// Makes pairing behave like the core (`pairing_window`, `pair` and
    /// `finish_pairing` in crates/ffi/src/node.rs):
    ///
    /// - `openPairingWindow` marks the window open, pauses a running
    ///   receiver (`ReceiverStopped(.stopped)`), then reports the window
    ///   listening on that receiver's port (`PairingWindowOpened`), all
    ///   before it returns.
    /// - While the window is open, the core's refusals apply: starting or
    ///   changing the receiver, a second window, renaming and resetting the
    ///   identity. `pair` is accepted, as in the core (see
    ///   `pairCalledWhileWindowOpen`).
    /// - Cancelling the window ends it (`endWindow(_:with:resume:)` with
    ///   `.cancelled`); cancelling a request delivers
    ///   `PairingFinished(.cancelled)`. An operation that already ended
    ///   ignores a cancel.
    public func scriptPairingLikeTheCore() {
        lock.withLock { scripted = true }
    }

    /// The open window's operation, or nil.
    public var windowOperation: UInt64? {
        lock.withLock { openWindow }
    }

    /// Whether `pair` was called while this device's window was open.
    public var pairCalledWhileWindowOpen: Bool {
        lock.withLock { pairedDuringWindow }
    }

    /// Ends a window like the core: `PairingFinished` first, then the
    /// receiver it paused resumes on its old settings (`ReceiverStarted`).
    /// With `resume: false` nothing follows `PairingFinished`, so a test can
    /// deliver a failed resume (`ReceiverStopped(.internal)`), a late one,
    /// or none. Does nothing if the operation already ended.
    public func endWindow(_ operation: UInt64, with result: PairingResult, resume: Bool = true) {
        let (ends, paused) = lock.withLock {
            (ended.insert(operation).inserted, openWindow == operation ? pausedReceiver : nil)
        }
        guard ends else { return }
        deliver([.pairingFinished(operation: operation, result: result)])
        if resume, let paused {
            deliver([.receiverStarted(info: paused)])
        }
    }

    /// A device connects to the open window and gets as far as the code, as
    /// the core reports it: `Connected`, `SessionCode`, then the code
    /// prompt, all for the window's operation. Returns the prompt, or nil
    /// (and delivers nothing) when no window is open.
    @discardableResult
    public func attemptOnWindow(
        peer: PeerInfo = Fixtures.peer(), code: String = "654321", promptId: UInt64 = 2,
        expiresAt: Date = Date().addingTimeInterval(120)
    ) -> Prompt? {
        guard let window = windowOperation else { return nil }
        let prompt = Fixtures.code(id: promptId, operation: window, peer: peer, code: code, expiresAt: expiresAt)
        deliver([.connected(operation: window, peer: peer)])
        deliver([.sessionCode(operation: window, peer: peer, code: code)])
        deliver([.promptOpened(prompt: prompt)])
        return prompt
    }

    /// Every `cancel()` on one of this fake's operations.
    private func cancelled(_ operation: UInt64) {
        note(.cancel(operation))
        let (isScripted, kind, isWindow) = lock.withLock {
            (scripted, kinds[operation], openWindow == operation)
        }
        guard isScripted else { return }
        if isWindow {
            endWindow(operation, with: .cancelled)
        } else if kind == .pair, lock.withLock({ ended.insert(operation).inserted }) {
            deliver([.pairingFinished(operation: operation, result: .cancelled)])
        }
    }

    // MARK: CoreClient

    public func identity() throws -> IdentityInfo {
        try fail("identity")
        lock.lock()
        defer { lock.unlock() }
        let short = Array(currentFingerprint.prefix(24))
        let display = stride(from: 0, to: short.count, by: 4)
            .map { String(short[$0..<min($0 + 4, short.count)]) }
            .joined(separator: "-")
        return IdentityInfo(
            name: deviceName, fingerprint: currentFingerprint,
            fingerprintDisplay: display, fingerprintHint: String(currentFingerprint.prefix(8)))
    }

    public func setDeviceName(_ name: String) throws -> IdentityInfo {
        try fail("setDeviceName")
        gate("setDeviceName")
        try refuseWhileWindowOpen(.Busy(detail: Self.notQuiet))
        lock.lock()
        deviceName = name
        lock.unlock()
        try failAfterApplying("setDeviceName")
        return try identity()
    }

    public func resetIdentity() throws -> IdentityInfo {
        try fail("resetIdentity")
        note(.resetIdentity)
        gate("resetIdentity")
        try refuseWhileWindowOpen(.Busy(detail: Self.notQuiet))
        lock.lock()
        currentFingerprint = currentFingerprint == Self.fingerprint ? Self.resetFingerprint : Self.fingerprint
        lock.unlock()
        try failAfterApplying("resetIdentity")
        return try identity()
    }

    public func trustedDevices() throws -> [TrustedDeviceInfo] {
        lock.lock()
        defer { lock.unlock() }
        return trustedList
    }

    public func trustFingerprint(_ fingerprint: String, alias: String?) throws -> TrustedDeviceInfo {
        try fail("trustFingerprint")
        note(.trust(fingerprint))
        let device = TrustedDeviceInfo(
            fingerprint: fingerprint, fingerprintDisplay: String(fingerprint.prefix(24)), alias: alias,
            method: .manual, addedAt: 1, lastSeen: nil)
        lock.lock()
        trustedList.append(device)
        lock.unlock()
        try failAfterApplying("trustFingerprint")
        return device
    }

    public func renameTrusted(_ fingerprint: String, alias: String) throws -> Bool { true }

    public func untrust(_ fingerprint: String) throws -> Bool {
        note(.untrust(fingerprint))
        lock.lock()
        defer { lock.unlock() }
        let before = trustedList.count
        trustedList.removeAll { $0.fingerprint == fingerprint }
        return trustedList.count != before
    }

    public func startDiscovery() throws {
        try fail("startDiscovery")
        note(.startDiscovery)
    }

    public func stopDiscovery() { note(.stopDiscovery) }
    public func nearbyPeers() -> [NearbyPeer] {
        lock.lock()
        defer { lock.unlock() }
        return nearbyTable
    }

    public func startReceiving(_ options: ReceiveOptions) throws -> ReceiverInfo {
        try fail("startReceiving")
        note(.startReceiving(options))
        gate("startReceiving")
        try refuseWhileWindowOpen(.Busy(detail: "a pairing window is open"))
        let info = ReceiverInfo(
            port: 47321, instance: options.advertise ? "dropumi-0000abcd" : nil, policy: options.policy,
            accept: options.accept, everyoneUntil: nil)
        lock.lock()
        receiving = info
        lock.unlock()
        return info
    }

    public func stopAccepting() {
        note(.stopAccepting)
        gate("stopAccepting")
    }

    /// While a window is open there is no running receiver to stop, as in
    /// the core: the paused one still resumes when the window ends.
    public func stopReceiving() {
        note(.stopReceiving)
        gate("stopReceiving")
        lock.lock()
        receiving = nil
        lock.unlock()
    }

    public func setReceivePolicy(_ policy: ReceivePolicy, accept: AcceptMode) throws -> ReceiverInfo {
        note(.setReceivePolicy(policy, accept))
        gate("setReceivePolicy")
        try refuseWhileWindowOpen(.InvalidState(detail: "not receiving"))
        lock.withLock {
            // The core keeps the change, so a window resumes the receiver
            // with it.
            receiving = receiving.map {
                ReceiverInfo(port: $0.port, instance: $0.instance, policy: policy, accept: accept, everyoneUntil: nil)
            }
        }
        return ReceiverInfo(port: 47321, instance: nil, policy: policy, accept: accept, everyoneUntil: nil)
    }

    public func cancelIncoming(_ transferId: String) -> Bool {
        note(.cancelIncoming(transferId))
        return true
    }

    public func answerOffer(_ promptId: UInt64, accept: Bool) throws {
        try fail("answerOffer")
        note(.answerOffer(promptId, accept))
        gate("answerOffer")
    }

    public func answerCode(_ promptId: UInt64, confirmed: Bool) throws {
        try fail("answerCode")
        note(.answerCode(promptId, confirmed))
        gate("answerCode")
    }

    public func pendingPrompts() -> [Prompt] { [] }

    public func send(_ target: SendTarget, items: [SendItem], allowUnverified: Bool) throws -> CoreOperation {
        try fail("send")
        let paths = items.compactMap { item -> String? in
            if case let .path(path) = item { return path }
            return nil
        }
        note(.send(target, paths: paths, allowUnverified: allowUnverified))
        gate("send")
        return started("send", newOperation(.send))
    }

    public func pair(_ target: SendTarget) throws -> CoreOperation {
        try fail("pair")
        lock.withLock {
            if openWindow != nil { pairedDuringWindow = true }
        }
        note(.pair(target))
        gate("pair")
        return started("pair", newOperation(.pair))
    }

    public func openPairingWindow(minutes: UInt32) throws -> CoreOperation {
        try fail("openPairingWindow")
        note(.openPairingWindow)
        gate("openPairingWindow")
        // As in the core: the window is marked open before the receiver it
        // pauses reports its stop, so a restart in between is refused.
        let (op, paused, isScripted) = try lock.withLock { () throws -> (FakeOperation, ReceiverInfo?, Bool) in
            if openWindow != nil { throw DropumiError.Busy(detail: "a pairing window is already open") }
            let op = addOperation(.pairingWindow)
            guard scripted else { return (op, nil, false) }
            openWindow = op.id()
            pausedReceiver = receiving
            receiving = nil
            return (op, pausedReceiver, true)
        }
        if isScripted {
            if paused != nil {
                deliver([.receiverStopped(reason: .stopped)])
            }
            // The paused receiver's port, or the core's default one.
            deliver([.pairingWindowOpened(operation: op.id(), port: paused?.port ?? 47321)])
        }
        return started("openPairingWindow", op)
    }

    public func snapshot() -> NodeSnapshot {
        lock.lock()
        defer { lock.unlock() }
        return NodeSnapshot(
            receiver: receiving, receiverStopping: false, prompts: [], nearby: [], operations: [],
            incomingTransfers: [])
    }

    public func shutdown() {
        continuation.finish()
    }
}

/// Fires the model's timers only when the test says so, so tests never
/// sleep.
@MainActor public final class ManualTimer: ModelTimer {
    private var scheduled: [(delay: Duration, fire: @MainActor () -> Void)] = []
    private var watchers: [(delay: Duration, resume: CheckedContinuation<Void, Never>)] = []

    public init() {}

    /// The delays of the timers not fired yet, in the order they were set.
    public var pending: [Duration] {
        scheduled.map { $0.delay }
    }

    public func schedule(after delay: Duration, _ fire: @escaping @MainActor () -> Void) {
        scheduled.append((delay, fire))
        let due = watchers.filter { $0.delay == delay }
        watchers.removeAll { $0.delay == delay }
        for watcher in due {
            watcher.resume.resume()
        }
    }

    /// Returns once a timer with this delay is pending, so a test fires it
    /// only after the model has started waiting.
    public func waitForPending(_ delay: Duration) async {
        if pending.contains(delay) { return }
        await withCheckedContinuation { watchers.append((delay, $0)) }
    }

    /// Fires every pending timer, in order. Timers they set stay pending.
    public func fireAll() {
        let due = scheduled
        scheduled.removeAll()
        for timer in due {
            timer.fire()
        }
    }
}

/// A clock the test sets by hand; hand the model `{ clock.now }`.
nonisolated public final class TestClock: @unchecked Sendable {
    private let lock = NSLock()
    private var current: Date

    public init(_ start: Date = Date()) {
        current = start
    }

    public var now: Date {
        get { lock.withLock { current } }
        set { lock.withLock { current = newValue } }
    }

    /// Moves the clock forward by `seconds`.
    public func advance(by seconds: TimeInterval) {
        lock.withLock { current = current.addingTimeInterval(seconds) }
    }
}

/// Builders for test fixtures.
nonisolated public enum Fixtures {
    public static func peer(name: String = "Maya's iPhone", trusted: Bool = false, alias: String? = nil) -> PeerInfo {
        PeerInfo(
            fingerprint: "ZYXWVUTSRQPONMLKJIHGFEDCBA765432ZYXWVUTSRQPONMLKJIHG",
            fingerprintDisplay: "ZYXW-VUTS-RQPO-NMLK-JIHG-FEDC",
            trusted: trusted, alias: alias, claimedName: name)
    }

    public static func offer(
        id: UInt64 = 1, transferId: String = "aa", peer: PeerInfo = peer(), code: String? = "123456",
        files: [OfferedFile] = [OfferedFile(name: "Beach-trip.mov", size: 18_200_000)]
    ) -> Prompt {
        .offer(
            id: id, transferId: transferId, peer: peer, code: code, files: files,
            fileCount: UInt32(files.count), totalBytes: files.reduce(0) { $0 + $1.size },
            expiresAtMs: UInt64(Date().addingTimeInterval(120).timeIntervalSince1970 * 1000))
    }

    public static func code(
        id: UInt64 = 2, operation: UInt64? = 1, peer: PeerInfo = peer(), code: String = "654321",
        expiresAt: Date = Date().addingTimeInterval(120)
    ) -> Prompt {
        .code(
            id: id, operation: operation, peer: peer, code: code, purpose: .pairing,
            expiresAtMs: UInt64(expiresAt.timeIntervalSince1970 * 1000))
    }

    public static func result(_ status: TransferStatus, transferId: String? = "aa") -> TransferResult {
        TransferResult(
            status: status, transferId: transferId, filesStored: status == .completed ? 1 : 0,
            filesFailed: 0, bytes: 10, elapsedMs: 5, rejectCause: nil, cancelledBy: nil, cancelCause: nil,
            error: status == .failed ? .connectionLost : nil)
    }

    public static func nearby(_ name: String, instance: String, hintAlias: String? = nil) -> NearbyPeer {
        NearbyPeer(instance: instance, name: name, fingerprintHint: "ZYXWVUTS", compatible: true, hintAlias: hintAlias)
    }
}
