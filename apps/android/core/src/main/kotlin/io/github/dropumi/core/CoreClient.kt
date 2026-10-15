// SPDX-License-Identifier: MPL-2.0
//
// The app's view of the Rust core (the Android counterpart of the Apple
// app's CoreClient). AppModel depends on this interface only, so its tests
// run against a fake core that never constructs an FFI object. Calls block
// and may read small files: never call them on the main thread.

package io.github.dropumi.core

import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.IdentityInfo
import io.github.dropumi.ffi.NearbyPeer
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.NodeSnapshot
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.ReceiveOptions
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.ReceiverInfo
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TrustedDeviceInfo
import kotlinx.coroutines.flow.Flow

/** A running send, pairing or pairing window. */
interface CoreOperation {
    /** The operation id that events name. */
    val id: ULong

    /** Cancels it; the result still arrives as an event. */
    fun cancel()
}

/** The Rust core, as the app uses it. */
interface CoreClient {
    /** Event batches, in order. Collect it once. */
    val events: Flow<List<NodeEvent>>

    fun identity(): IdentityInfo
    fun setDeviceName(name: String): IdentityInfo
    fun resetIdentity(): IdentityInfo

    fun trustedDevices(): List<TrustedDeviceInfo>
    fun trustFingerprint(fingerprint: String, alias: String?): TrustedDeviceInfo
    fun renameTrusted(fingerprint: String, alias: String): Boolean
    fun untrust(fingerprint: String): Boolean

    fun startDiscovery()
    fun stopDiscovery()
    fun nearbyPeers(): List<NearbyPeer>

    fun startReceiving(options: ReceiveOptions): ReceiverInfo
    fun stopAccepting()
    fun stopReceiving()
    fun setReceivePolicy(policy: ReceivePolicy, accept: AcceptMode): ReceiverInfo
    fun cancelIncoming(transferId: String): Boolean

    fun answerOffer(promptId: ULong, accept: Boolean)
    fun answerCode(promptId: ULong, confirmed: Boolean)
    fun pendingPrompts(): List<Prompt>

    /** A page of an offer's files (an offer prompt carries the first 200). */
    fun offerFiles(promptId: ULong, offset: UInt, limit: UInt): List<OfferedFile>

    fun send(target: SendTarget, items: List<SendItem>, allowUnverified: Boolean): CoreOperation
    fun pair(target: SendTarget): CoreOperation
    fun openPairingWindow(minutes: UInt): CoreOperation

    fun snapshot(): NodeSnapshot

    /** Stops the node; blocks for up to a few seconds. Tests only (ADR-0013 D7). */
    fun shutdown()
}
