// SPDX-License-Identifier: MPL-2.0
//
// The real core: the generated DropumiNode behind CoreClient.

package io.github.dropumi.core

import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.DropumiNode
import io.github.dropumi.ffi.EventListener
import io.github.dropumi.ffi.IdentityInfo
import io.github.dropumi.ffi.NearbyPeer
import io.github.dropumi.ffi.NodeConfig
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.NodeSnapshot
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.OperationHandle
import io.github.dropumi.ffi.PlatformDiscovery
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.ReceiveOptions
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.ReceiverInfo
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendOptions
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TrustedDeviceInfo
import java.io.File
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow

/**
 * Hands event batches from the node's event thread to the app. It never
 * blocks and never throws: the channel is unbounded and only `trySend` is
 * used, so the node never waits for the UI (ADR-0009).
 */
class EventBridge : EventListener {
    private val channel = Channel<List<NodeEvent>>(Channel.UNLIMITED)

    /** The batches, in order. One collector. */
    val events: Flow<List<NodeEvent>> = channel.receiveAsFlow()

    override fun onEvents(events: List<NodeEvent>) {
        // Fails only after finish(); the batch is then dropped.
        channel.trySend(events)
    }

    /** Ends the stream (after the node has shut down). */
    fun finish() {
        channel.close()
    }
}

private class LiveOperation(private val handle: OperationHandle) : CoreOperation {
    override val id: ULong = handle.id()

    override fun cancel() = handle.cancel()
}

/**
 * [CoreClient] backed by the Rust node. Opening it reads or creates the
 * identity: construct it off the main thread.
 *
 * @param configDir app-private and excluded from backups (ADR-0013 D3); the
 *   core creates it 0700 and checks its owner and mode.
 * @param receiveDir app-private, created 0700 by [AppPaths.prepare].
 */
class LiveCoreClient(
    configDir: File,
    receiveDir: File,
    discovery: PlatformDiscovery?,
) : CoreClient {
    private val bridge = EventBridge()
    private val node = DropumiNode(NodeConfig(configDir.path, receiveDir.path), bridge, discovery)

    override val events: Flow<List<NodeEvent>> = bridge.events

    override fun identity(): IdentityInfo = node.identity()
    override fun setDeviceName(name: String): IdentityInfo = node.setDeviceName(name)
    override fun resetIdentity(): IdentityInfo = node.resetIdentity()

    override fun trustedDevices(): List<TrustedDeviceInfo> = node.trustedDevices()
    override fun trustFingerprint(fingerprint: String, alias: String?): TrustedDeviceInfo =
        node.trustFingerprint(fingerprint, alias)
    override fun renameTrusted(fingerprint: String, alias: String): Boolean = node.renameTrusted(fingerprint, alias)
    override fun untrust(fingerprint: String): Boolean = node.untrust(fingerprint)

    override fun startDiscovery() = node.startDiscovery()
    override fun stopDiscovery() = node.stopDiscovery()
    override fun nearbyPeers(): List<NearbyPeer> = node.nearbyPeers()

    override fun startReceiving(options: ReceiveOptions): ReceiverInfo = node.startReceiving(options)
    override fun stopAccepting() = node.stopAccepting()
    override fun stopReceiving() = node.stopReceiving()
    override fun setReceivePolicy(policy: ReceivePolicy, accept: AcceptMode): ReceiverInfo =
        node.setReceivePolicy(policy, accept)
    override fun cancelIncoming(transferId: String): Boolean = node.cancelIncoming(transferId)

    override fun answerOffer(promptId: ULong, accept: Boolean) = node.answerOffer(promptId, accept)
    override fun answerCode(promptId: ULong, confirmed: Boolean) = node.answerSas(promptId, confirmed)
    override fun pendingPrompts(): List<Prompt> = node.pendingPrompts()
    override fun offerFiles(promptId: ULong, offset: UInt, limit: UInt): List<OfferedFile> =
        node.offerFiles(promptId, offset, limit)

    override fun send(target: SendTarget, items: List<SendItem>, allowUnverified: Boolean): CoreOperation =
        LiveOperation(node.send(target, items, SendOptions(allowUnverified)))
    override fun pair(target: SendTarget): CoreOperation = LiveOperation(node.pairWith(target))
    override fun openPairingWindow(minutes: UInt): CoreOperation = LiveOperation(node.openPairingWindow(minutes))

    override fun snapshot(): NodeSnapshot = node.snapshot()

    override fun shutdown() {
        node.shutdown()
        bridge.finish()
    }
}
