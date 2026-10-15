// SPDX-License-Identifier: MPL-2.0
//
// A scripted core for app-state tests and UI state tests. It records every
// call and lets a test push events. It never answers prompts or creates
// trust by itself: a UI state test that shows a transfer finishing does so
// only because the test pushed that event, and such tests are labelled
// "UI state test (fake core)".
//
// The app's Debug builds compile this file into the app for UI-test
// scenarios (Release builds exclude it, so the app never contains a fake
// core). The types are explicitly nonisolated for that reason: the app's
// code is main-actor by default.

import DropumiBindings
import DropumiCore
import Foundation

nonisolated public final class FakeOperation: CoreOperation, @unchecked Sendable {
    private let value: UInt64
    private let lock = NSLock()
    private var cancelledFlag = false

    public init(id: UInt64) {
        self.value = id
    }

    public func id() -> UInt64 { value }

    public func cancel() {
        lock.lock()
        cancelledFlag = true
        lock.unlock()
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
        case startReceiving(ReceiveOptions)
        case stopAccepting
        case stopReceiving
        case setReceivePolicy(ReceivePolicy, AcceptMode)
        case answerOffer(UInt64, Bool)
        case answerCode(UInt64, Bool)
        case send(SendTarget, paths: [String], allowUnverified: Bool)
        case pair(SendTarget)
        case openPairingWindow
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
    private var nextOperation: UInt64 = 1
    private var operationsById: [UInt64: FakeOperation] = [:]
    private var trustedList: [TrustedDeviceInfo] = []
    private var deviceName = "Dropumi-1A2B"
    private var receiving: ReceiverInfo?
    private var nearbyTable: [NearbyPeer] = []
    private var startGate: CallGate?
    /// Errors to throw from the next calls, by method name.
    public var failures: [String: DropumiError] = [:]

    public static let fingerprint = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRST"

    public init() {
        (events, continuation) = AsyncStream.makeStream(of: [NodeEvent].self)
    }

    /// Delivers events as the core would. Found and lost devices also
    /// update what `nearbyPeers()` returns, as in the core.
    public func emit(_ batch: NodeEvent...) {
        lock.lock()
        for event in batch {
            switch event {
            case let .peerFound(peer):
                nearbyTable.removeAll { $0.instance == peer.instance }
                nearbyTable.append(peer)
            case let .peerLost(instance):
                nearbyTable.removeAll { $0.instance == instance }
            default:
                break
            }
        }
        lock.unlock()
        continuation.yield(batch)
    }

    public var calls: [Call] {
        lock.lock()
        defer { lock.unlock() }
        return log
    }

    public func operation(_ id: UInt64) -> FakeOperation? {
        lock.lock()
        defer { lock.unlock() }
        return operationsById[id]
    }

    private func note(_ call: Call) {
        lock.lock()
        log.append(call)
        lock.unlock()
    }

    private func fail(_ method: String) throws {
        lock.lock()
        let error = failures.removeValue(forKey: method)
        lock.unlock()
        if let error { throw error }
    }

    private func newOperation() -> FakeOperation {
        lock.lock()
        defer { lock.unlock() }
        let op = FakeOperation(id: nextOperation)
        operationsById[nextOperation] = op
        nextOperation += 1
        return op
    }

    public func identity() throws -> IdentityInfo {
        try fail("identity")
        lock.lock()
        defer { lock.unlock() }
        return IdentityInfo(
            name: deviceName, fingerprint: Self.fingerprint,
            fingerprintDisplay: "ABCD-EFGH-IJKL-MNOP-QRST-UVWX", fingerprintHint: "ABCDEFGH")
    }

    public func setDeviceName(_ name: String) throws -> IdentityInfo {
        try fail("setDeviceName")
        lock.lock()
        deviceName = name
        lock.unlock()
        return try identity()
    }

    public func resetIdentity() throws -> IdentityInfo { try identity() }

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

    /// Holds the next `startReceiving` until the test opens the gate.
    public func holdNextStartReceiving() -> CallGate {
        let gate = CallGate()
        lock.lock()
        startGate = gate
        lock.unlock()
        return gate
    }

    public func startReceiving(_ options: ReceiveOptions) throws -> ReceiverInfo {
        try fail("startReceiving")
        note(.startReceiving(options))
        lock.lock()
        let gate = startGate
        startGate = nil
        lock.unlock()
        gate?.hold()
        let info = ReceiverInfo(
            port: 47321, instance: options.advertise ? "dropumi-0000abcd" : nil, policy: options.policy,
            accept: options.accept, everyoneUntil: nil)
        lock.lock()
        receiving = info
        lock.unlock()
        return info
    }

    public func stopAccepting() { note(.stopAccepting) }

    public func stopReceiving() {
        note(.stopReceiving)
        lock.lock()
        receiving = nil
        lock.unlock()
    }

    public func setReceivePolicy(_ policy: ReceivePolicy, accept: AcceptMode) throws -> ReceiverInfo {
        note(.setReceivePolicy(policy, accept))
        return ReceiverInfo(port: 47321, instance: nil, policy: policy, accept: accept, everyoneUntil: nil)
    }

    public func cancelIncoming(_ transferId: String) -> Bool {
        note(.cancelIncoming(transferId))
        return true
    }

    public func answerOffer(_ promptId: UInt64, accept: Bool) throws {
        try fail("answerOffer")
        note(.answerOffer(promptId, accept))
    }

    public func answerCode(_ promptId: UInt64, confirmed: Bool) throws {
        try fail("answerCode")
        note(.answerCode(promptId, confirmed))
    }

    public func pendingPrompts() -> [Prompt] { [] }

    public func send(_ target: SendTarget, items: [SendItem], allowUnverified: Bool) throws -> CoreOperation {
        try fail("send")
        let paths = items.compactMap { item -> String? in
            if case let .path(path) = item { return path }
            return nil
        }
        note(.send(target, paths: paths, allowUnverified: allowUnverified))
        return newOperation()
    }

    public func pair(_ target: SendTarget) throws -> CoreOperation {
        note(.pair(target))
        return newOperation()
    }

    public func openPairingWindow(minutes: UInt32) throws -> CoreOperation {
        note(.openPairingWindow)
        return newOperation()
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

    public static func code(id: UInt64 = 2, operation: UInt64? = 1, code: String = "654321") -> Prompt {
        .code(
            id: id, operation: operation, peer: peer(), code: code, purpose: .pairing,
            expiresAtMs: UInt64(Date().addingTimeInterval(120).timeIntervalSince1970 * 1000))
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
