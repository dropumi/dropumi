// SPDX-License-Identifier: MPL-2.0
//
// The app's view of the Rust core. The app model depends on this protocol
// only, so UI tests can run against a fake core. Calls are synchronous and
// may read small files: never call them on the main thread.

import DropumiBindings
import Foundation

/// A running send, pairing or pairing window.
public protocol CoreOperation: AnyObject, Sendable {
    func id() -> UInt64
    func cancel()
}

/// The Rust core, as the app uses it.
public protocol CoreClient: AnyObject, Sendable {
    /// Event batches, in order. One consumer.
    var events: AsyncStream<[NodeEvent]> { get }

    func identity() throws -> IdentityInfo
    func setDeviceName(_ name: String) throws -> IdentityInfo
    func resetIdentity() throws -> IdentityInfo

    func trustedDevices() throws -> [TrustedDeviceInfo]
    func trustFingerprint(_ fingerprint: String, alias: String?) throws -> TrustedDeviceInfo
    func renameTrusted(_ fingerprint: String, alias: String) throws -> Bool
    func untrust(_ fingerprint: String) throws -> Bool

    func startDiscovery() throws
    func stopDiscovery()
    func nearbyPeers() -> [NearbyPeer]

    func startReceiving(_ options: ReceiveOptions) throws -> ReceiverInfo
    func stopAccepting()
    func stopReceiving()
    func setReceivePolicy(_ policy: ReceivePolicy, accept: AcceptMode) throws -> ReceiverInfo
    func cancelIncoming(_ transferId: String) -> Bool

    func answerOffer(_ promptId: UInt64, accept: Bool) throws
    func answerCode(_ promptId: UInt64, confirmed: Bool) throws
    func pendingPrompts() -> [Prompt]

    func send(_ target: SendTarget, items: [SendItem], allowUnverified: Bool) throws -> CoreOperation
    func pair(_ target: SendTarget) throws -> CoreOperation
    func openPairingWindow(minutes: UInt32) throws -> CoreOperation

    func snapshot() -> NodeSnapshot
    func shutdown()
}

extension OperationHandle: CoreOperation {}
