// SPDX-License-Identifier: MPL-2.0
//
// The real core: the generated `DropumiNode` behind `CoreClient`.

import DropumiBindings
import Foundation

/// Forwards event batches from the node's event thread into an AsyncStream.
/// It never blocks: the node must never wait for the UI.
final class EventBridge: EventListener, @unchecked Sendable {
    let stream: AsyncStream<[NodeEvent]>
    private let continuation: AsyncStream<[NodeEvent]>.Continuation

    init() {
        (stream, continuation) = AsyncStream.makeStream(of: [NodeEvent].self, bufferingPolicy: .unbounded)
    }

    func onEvents(events: [NodeEvent]) throws {
        continuation.yield(events)
    }

    func finish() {
        continuation.finish()
    }
}

/// `CoreClient` backed by the Rust node.
public final class LiveCoreClient: CoreClient, @unchecked Sendable {
    private let node: DropumiNode
    private let bridge: EventBridge

    public var events: AsyncStream<[NodeEvent]> { bridge.stream }

    /// Opens the node. `configDir` must be app-private and excluded from
    /// backups (ADR-0014); `receiveDir` must exist.
    public init(configDir: URL, receiveDir: URL?, discovery: PlatformDiscovery?) throws {
        let bridge = EventBridge()
        self.bridge = bridge
        self.node = try DropumiNode(
            config: NodeConfig(configDir: configDir.path, receiveDir: receiveDir?.path),
            listener: bridge,
            discovery: discovery
        )
    }

    public func identity() throws -> IdentityInfo { try node.identity() }
    public func setDeviceName(_ name: String) throws -> IdentityInfo { try node.setDeviceName(name: name) }
    public func resetIdentity() throws -> IdentityInfo { try node.resetIdentity() }

    public func trustedDevices() throws -> [TrustedDeviceInfo] { try node.trustedDevices() }
    public func trustFingerprint(_ fingerprint: String, alias: String?) throws -> TrustedDeviceInfo {
        try node.trustFingerprint(fingerprint: fingerprint, alias: alias)
    }
    public func renameTrusted(_ fingerprint: String, alias: String) throws -> Bool {
        try node.renameTrusted(fingerprint: fingerprint, alias: alias)
    }
    public func untrust(_ fingerprint: String) throws -> Bool { try node.untrust(fingerprint: fingerprint) }

    public func startDiscovery() throws { try node.startDiscovery() }
    public func stopDiscovery() { node.stopDiscovery() }
    public func nearbyPeers() -> [NearbyPeer] { node.nearbyPeers() }

    public func startReceiving(_ options: ReceiveOptions) throws -> ReceiverInfo {
        try node.startReceiving(options: options)
    }
    public func stopAccepting() { node.stopAccepting() }
    public func stopReceiving() { node.stopReceiving() }
    public func setReceivePolicy(_ policy: ReceivePolicy, accept: AcceptMode) throws -> ReceiverInfo {
        try node.setReceivePolicy(policy: policy, accept: accept)
    }
    public func cancelIncoming(_ transferId: String) -> Bool { node.cancelIncoming(transferId: transferId) }

    public func answerOffer(_ promptId: UInt64, accept: Bool) throws {
        try node.answerOffer(promptId: promptId, accept: accept)
    }
    public func answerCode(_ promptId: UInt64, confirmed: Bool) throws {
        try node.answerSas(promptId: promptId, confirmed: confirmed)
    }
    public func pendingPrompts() -> [Prompt] { node.pendingPrompts() }

    public func send(_ target: SendTarget, items: [SendItem], allowUnverified: Bool) throws -> CoreOperation {
        try node.send(target: target, items: items, options: SendOptions(allowUnverified: allowUnverified))
    }
    public func pair(_ target: SendTarget) throws -> CoreOperation { try node.pairWith(target: target) }
    public func openPairingWindow(minutes: UInt32) throws -> CoreOperation {
        try node.openPairingWindow(minutes: minutes)
    }

    public func snapshot() -> NodeSnapshot { node.snapshot() }

    public func shutdown() {
        node.shutdown()
        bridge.finish()
    }
}
