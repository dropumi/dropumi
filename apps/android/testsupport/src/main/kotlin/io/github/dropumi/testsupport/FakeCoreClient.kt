// SPDX-License-Identifier: MPL-2.0
//
// A scripted core for app-state tests and UI state tests (a port of the
// Apple app's FakeCoreClient). It records every call and lets a test push
// events. It never answers prompts or creates trust by itself: a UI state
// test that shows a transfer finishing does so only because the test pushed
// that event, and such tests are labelled "UI state test (fake core)".
//
// This module reaches the test APKs only (testImplementation and
// androidTestImplementation); no app APK contains it (ADR-0013 D9).

package io.github.dropumi.testsupport

import io.github.dropumi.core.CoreClient
import io.github.dropumi.core.CoreOperation
import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.ConfigProblem
import io.github.dropumi.ffi.DropumiException
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
import io.github.dropumi.ffi.TrustMethod
import io.github.dropumi.ffi.TrustedDeviceInfo
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow

class FakeOperation(override val id: ULong) : CoreOperation {
    @Volatile
    var cancelled = false
        private set

    override fun cancel() {
        cancelled = true
    }
}

/**
 * Holds one fake core call until the test opens it, so a test can act while
 * the call is under way (no sleeps). The held call blocks its thread, so
 * run the model's io work on a real thread pool in such a test.
 */
class CallGate internal constructor() {
    private val arrived = CompletableDeferred<Unit>()
    private val release = CountDownLatch(1)

    /** Returns once the held call has started. */
    suspend fun reached() = arrived.await()

    /** Lets the held call return. */
    fun open() = release.countDown()

    /** Called by the held call (off the test's thread). */
    internal fun hold() {
        arrived.complete(Unit)
        if (!release.await(GATE_LIMIT_SECONDS, TimeUnit.SECONDS)) throw AssertionError("the gate was never opened")
    }

    private companion object {
        const val GATE_LIMIT_SECONDS = 30L
    }
}

class FakeCoreClient : CoreClient {
    sealed interface Call {
        data class StartReceiving(val options: ReceiveOptions) : Call
        data object StopAccepting : Call
        data object StopReceiving : Call
        data class SetReceivePolicy(val policy: ReceivePolicy, val accept: AcceptMode) : Call
        data class AnswerOffer(val promptId: ULong, val accept: Boolean) : Call
        data class AnswerCode(val promptId: ULong, val confirmed: Boolean) : Call
        data class OfferFiles(val promptId: ULong, val offset: UInt, val limit: UInt) : Call
        data class Send(val target: SendTarget, val items: List<SendItem>, val allowUnverified: Boolean) : Call {
            /** The path items, in order. */
            val paths: List<String> get() = items.filterIsInstance<SendItem.Path>().map { it.path }
        }
        data class Pair(val target: SendTarget) : Call
        data object OpenPairingWindow : Call
        data class CancelIncoming(val transferId: String) : Call
        data class Trust(val fingerprint: String) : Call
        data class Untrust(val fingerprint: String) : Call
        data object StartDiscovery : Call
        data object StopDiscovery : Call
    }

    private val channel = Channel<List<NodeEvent>>(Channel.UNLIMITED)
    override val events: Flow<List<NodeEvent>> = channel.receiveAsFlow()

    private val lock = Any()
    private val log = mutableListOf<Call>()
    private var nextOperation = 1uL
    private val operationsById = mutableMapOf<ULong, FakeOperation>()
    private val trustedList = mutableListOf<TrustedDeviceInfo>()
    private var deviceName = "Dropumi-1A2B"
    private var receiving: ReceiverInfo? = null
    private val nearbyTable = mutableListOf<NearbyPeer>()
    private val gates = mutableMapOf<String, CallGate>()
    private val duringCall = mutableMapOf<String, (ULong) -> List<NodeEvent>>()
    private var currentFingerprint = FINGERPRINT

    /** Errors to throw from the next call of each method, by method name. */
    val failures: MutableMap<String, DropumiException> = mutableMapOf()

    /**
     * Methods that apply their change and then fail, as the core does when a
     * change was saved but could not be read back.
     */
    val appliedButUnreadable: MutableSet<String> = mutableSetOf()

    /** Delivers events as the core would. Found and lost devices also update [nearbyPeers]. */
    fun emit(vararg batch: NodeEvent) {
        synchronized(lock) {
            for (event in batch) {
                when (event) {
                    is NodeEvent.PeerFound -> {
                        nearbyTable.removeAll { it.instance == event.peer.instance }
                        nearbyTable.add(event.peer)
                    }
                    is NodeEvent.PeerLost -> nearbyTable.removeAll { it.instance == event.instance }
                    else -> Unit
                }
            }
        }
        channel.trySend(batch.toList())
    }

    val calls: List<Call> get() = synchronized(lock) { log.toList() }

    fun operation(id: ULong): FakeOperation? = synchronized(lock) { operationsById[id] }

    /** Holds the next call of [method] until the test opens the returned gate. */
    fun holdNext(method: String): CallGate = CallGate().also { synchronized(lock) { gates[method] = it } }

    fun holdNextStartReceiving(): CallGate = holdNext("startReceiving")

    /**
     * The next call of [method] (`send`, `pair` or `openPairingWindow`)
     * emits these events for its new operation before it returns, as the
     * core may (a refused connection can finish a send at once).
     */
    fun emitBeforeReturning(method: String, events: (operation: ULong) -> List<NodeEvent>) =
        synchronized(lock) { duringCall[method] = events }

    private fun started(method: String, operation: FakeOperation): FakeOperation {
        val events = synchronized(lock) { duringCall.remove(method) }
        if (events != null) emit(*events(operation.id).toTypedArray())
        return operation
    }

    private fun note(call: Call) = synchronized(lock) { log.add(call) }

    private fun fail(method: String) {
        val error = synchronized(lock) { failures.remove(method) }
        if (error != null) throw error
    }

    private fun gate(method: String) {
        synchronized(lock) { gates.remove(method) }?.hold()
    }

    /** After the change is applied: fail like a change that was saved but could not be read back. */
    private fun failAfterApplying(method: String) {
        if (synchronized(lock) { method in appliedButUnreadable }) {
            throw DropumiException.Config(ConfigProblem.IO, "the change was saved, but it could not be read back: test")
        }
    }

    private fun newOperation(): FakeOperation = synchronized(lock) {
        FakeOperation(nextOperation).also {
            operationsById[nextOperation] = it
            nextOperation++
        }
    }

    override fun identity(): IdentityInfo {
        fail("identity")
        return synchronized(lock) {
            val short = currentFingerprint.take(24)
            IdentityInfo(deviceName, currentFingerprint, short.chunked(4).joinToString("-"), currentFingerprint.take(8))
        }
    }

    override fun setDeviceName(name: String): IdentityInfo {
        fail("setDeviceName")
        gate("setDeviceName")
        synchronized(lock) { deviceName = name }
        failAfterApplying("setDeviceName")
        return identity()
    }

    override fun resetIdentity(): IdentityInfo {
        fail("resetIdentity")
        synchronized(lock) {
            currentFingerprint = if (currentFingerprint == FINGERPRINT) RESET_FINGERPRINT else FINGERPRINT
        }
        failAfterApplying("resetIdentity")
        return identity()
    }

    override fun trustedDevices(): List<TrustedDeviceInfo> = synchronized(lock) { trustedList.toList() }

    override fun trustFingerprint(fingerprint: String, alias: String?): TrustedDeviceInfo {
        fail("trustFingerprint")
        note(Call.Trust(fingerprint))
        val device = TrustedDeviceInfo(fingerprint, fingerprint.take(24), alias, TrustMethod.MANUAL, 1uL, null)
        synchronized(lock) { trustedList.add(device) }
        failAfterApplying("trustFingerprint")
        return device
    }

    override fun renameTrusted(fingerprint: String, alias: String): Boolean {
        fail("renameTrusted")
        return synchronized(lock) {
            val index = trustedList.indexOfFirst { it.fingerprint == fingerprint }
            if (index >= 0) trustedList[index] = trustedList[index].copy(alias = alias)
            index >= 0
        }
    }

    override fun untrust(fingerprint: String): Boolean {
        fail("untrust")
        note(Call.Untrust(fingerprint))
        return synchronized(lock) { trustedList.removeAll { it.fingerprint == fingerprint } }
    }

    override fun startDiscovery() {
        fail("startDiscovery")
        note(Call.StartDiscovery)
    }

    override fun stopDiscovery() {
        note(Call.StopDiscovery)
    }

    override fun nearbyPeers(): List<NearbyPeer> = synchronized(lock) { nearbyTable.toList() }

    override fun startReceiving(options: ReceiveOptions): ReceiverInfo {
        fail("startReceiving")
        note(Call.StartReceiving(options))
        gate("startReceiving")
        val info = ReceiverInfo(
            port = 47321u,
            instance = if (options.advertise) "dropumi-0000abcd" else null,
            policy = options.policy,
            accept = options.accept,
            everyoneUntil = null,
        )
        synchronized(lock) { receiving = info }
        return info
    }

    override fun stopAccepting() {
        note(Call.StopAccepting)
    }

    override fun stopReceiving() {
        note(Call.StopReceiving)
        synchronized(lock) { receiving = null }
    }

    override fun setReceivePolicy(policy: ReceivePolicy, accept: AcceptMode): ReceiverInfo {
        note(Call.SetReceivePolicy(policy, accept))
        return ReceiverInfo(47321u, null, policy, accept, null)
    }

    override fun cancelIncoming(transferId: String): Boolean {
        note(Call.CancelIncoming(transferId))
        return true
    }

    override fun answerOffer(promptId: ULong, accept: Boolean) {
        fail("answerOffer")
        note(Call.AnswerOffer(promptId, accept))
        gate("answerOffer")
    }

    override fun answerCode(promptId: ULong, confirmed: Boolean) {
        fail("answerCode")
        note(Call.AnswerCode(promptId, confirmed))
        gate("answerCode")
    }

    override fun pendingPrompts(): List<Prompt> = emptyList()

    /** Offers built with [Fixtures.offer] list `file-<n>.bin` of 1 byte each. */
    override fun offerFiles(promptId: ULong, offset: UInt, limit: UInt): List<OfferedFile> {
        fail("offerFiles")
        note(Call.OfferFiles(promptId, offset, limit))
        return (offset until offset + limit).map { OfferedFile("file-$it.bin", 1uL) }
    }

    override fun send(target: SendTarget, items: List<SendItem>, allowUnverified: Boolean): CoreOperation {
        fail("send")
        note(Call.Send(target, items, allowUnverified))
        return started("send", newOperation())
    }

    override fun pair(target: SendTarget): CoreOperation {
        fail("pair")
        note(Call.Pair(target))
        gate("pair")
        return started("pair", newOperation())
    }

    override fun openPairingWindow(minutes: UInt): CoreOperation {
        fail("openPairingWindow")
        note(Call.OpenPairingWindow)
        gate("openPairingWindow")
        return started("openPairingWindow", newOperation())
    }

    override fun snapshot(): NodeSnapshot = synchronized(lock) {
        NodeSnapshot(receiving, false, emptyList(), emptyList(), emptyList(), emptyList())
    }

    override fun shutdown() {
        channel.close()
    }

    companion object {
        const val FINGERPRINT = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRST"

        /** The fingerprint after resetIdentity(). */
        const val RESET_FINGERPRINT = "QRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCD"
    }
}
