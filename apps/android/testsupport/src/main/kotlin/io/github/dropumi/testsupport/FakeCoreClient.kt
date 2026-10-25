// SPDX-License-Identifier: MPL-2.0
//
// A scripted core for app-state tests and UI state tests (a port of the
// Apple app's FakeCoreClient). It records every call and lets a test push
// events. It never answers prompts or creates trust by itself: a UI state
// test that shows a transfer finishing does so only because the test pushed
// that event, and such tests are labelled "UI state test (fake core)".
//
// By default a call only records itself and returns. After
// scriptPairingLikeTheCore(), pairing windows and pairing requests behave
// like the real core (crates/ffi/src/node.rs), as in the Apple app's fake.
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
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.PeerInfo
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.ReceiveOptions
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.ReceiverInfo
import io.github.dropumi.ffi.ReceiverStopReason
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

/** [onCancel] runs on every [cancel], on the caller's thread. */
class FakeOperation(override val id: ULong, private val onCancel: ((ULong) -> Unit)? = null) : CoreOperation {
    @Volatile
    var cancelled = false
        private set

    override fun cancel() {
        cancelled = true
        onCancel?.invoke(id)
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
        data class Cancel(val operation: ULong) : Call
        data object ResetIdentity : Call
    }

    private enum class Kind { SEND, PAIR, WINDOW }

    private val channel = Channel<List<NodeEvent>>(Channel.UNLIMITED)
    override val events: Flow<List<NodeEvent>> = channel.receiveAsFlow()

    private val lock = Any()
    private val log = mutableListOf<Call>()
    private var nextOperation = 1uL
    private val operationsById = mutableMapOf<ULong, FakeOperation>()
    private val trustedList = mutableListOf<TrustedDeviceInfo>()
    private var deviceName = "Dropumi-1A2B"
    private var receiving: ReceiverInfo? = null

    /**
     * Whether the receiver runs and accepts, as in the core: from
     * startReceiving until stopAccepting or stopReceiving. Only then does
     * cancelIncoming find a transfer. The core drops the receiver's
     * canceller once the receiver stops accepting (`stop_receiver` in
     * crates/ffi/src/node.rs), so a transfer that keeps a stopping receiver
     * alive cannot be cancelled; this follows the core until it keeps the
     * canceller while the receiver drains.
     */
    private var receiverAccepting = false

    private val nearbyTable = mutableListOf<NearbyPeer>()
    private val gates = mutableMapOf<String, CallGate>()
    private val duringCall = mutableMapOf<String, (ULong) -> List<NodeEvent>>()
    private var currentFingerprint = FINGERPRINT
    private var scripted = false
    private var openWindow: ULong? = null
    private var pausedReceiver: ReceiverInfo? = null
    private var pairedDuringWindow = false
    private val ended = mutableSetOf<ULong>()
    private val kinds = mutableMapOf<ULong, Kind>()
    private var callHook: ((Call) -> Unit)? = null

    /** Errors to throw from the next call of each method, by method name. */
    val failures: MutableMap<String, DropumiException> = mutableMapOf()

    /**
     * Methods that apply their change and then fail, as the core does when a
     * change was saved but could not be read back.
     */
    val appliedButUnreadable: MutableSet<String> = mutableSetOf()

    /**
     * When true, stopAccepting ends the receiver at once, as the core does
     * when no transfer is running, and reports it (ReceiverStopped) before
     * the call returns: the earliest the event can come.
     */
    @Volatile
    var stopAcceptingEndsReceiver = false

    /**
     * Delivers events as the core would. Found and lost devices also update
     * [nearbyPeers]. After [scriptPairingLikeTheCore] it keeps the state the
     * core keeps with them too: ReceiverStarted and ReceiverStopped update
     * the receiver, and PairingFinished ends its operation. Ending the open
     * window this way resumes nothing; [endWindow] resumes like the core.
     */
    fun emit(vararg batch: NodeEvent) {
        synchronized(lock) {
            for (event in batch) {
                when (event) {
                    is NodeEvent.PeerFound -> {
                        nearbyTable.removeAll { it.instance == event.peer.instance }
                        nearbyTable.add(event.peer)
                    }
                    is NodeEvent.PeerLost -> nearbyTable.removeAll { it.instance == event.instance }
                    is NodeEvent.ReceiverStarted -> if (scripted) receiving = event.info
                    is NodeEvent.ReceiverStopped -> if (scripted) receiving = null
                    is NodeEvent.PairingFinished -> if (scripted) {
                        ended.add(event.operation)
                        if (event.operation == openWindow) {
                            openWindow = null
                            pausedReceiver = null
                        }
                    }
                    else -> Unit
                }
            }
        }
        channel.trySend(batch.toList())
    }

    /**
     * Called after each recorded call, outside the lock, so that a UI state
     * scenario can react as a core would (for example, close a prompt once
     * it is answered). Null by default; the fake core still never answers a
     * prompt or creates trust by itself.
     */
    var onCall: ((Call) -> Unit)?
        get() = synchronized(lock) { callHook }
        set(value) = synchronized(lock) { callHook = value }

    // Pairing like the core

    /**
     * Makes pairing behave like the core (`pairing_window`, `pair` and
     * `finish_pairing` in crates/ffi/src/node.rs):
     *
     * - openPairingWindow marks the window open, pauses a running receiver
     *   (ReceiverStopped(STOPPED)), then reports the window listening on
     *   that receiver's port (PairingWindowOpened), all before it returns.
     * - While the window is open, the core's refusals apply: starting or
     *   changing the receiver, a second window, renaming and resetting the
     *   identity. pair is accepted, as in the core ([pairCalledWhileWindowOpen]).
     * - Cancelling the window ends it ([endWindow] with Cancelled);
     *   cancelling a request delivers PairingFinished(Cancelled). An
     *   operation that already ended ignores a cancel.
     */
    fun scriptPairingLikeTheCore() = synchronized(lock) { scripted = true }

    /** The open window's operation, or null. */
    val windowOperation: ULong? get() = synchronized(lock) { openWindow }

    /** Whether pair was called while this device's window was open. */
    val pairCalledWhileWindowOpen: Boolean get() = synchronized(lock) { pairedDuringWindow }

    /**
     * Ends a window like the core: PairingFinished first, then the receiver
     * it paused resumes on its old settings (ReceiverStarted). With [resume]
     * false nothing follows PairingFinished, so a test can deliver a failed
     * resume, a late one, or none. Does nothing if the operation already ended.
     */
    fun endWindow(operation: ULong, result: PairingResult, resume: Boolean = true) {
        val (ends, paused) = synchronized(lock) {
            ended.add(operation) to (if (openWindow == operation) pausedReceiver else null)
        }
        if (!ends) return
        emit(NodeEvent.PairingFinished(operation, result))
        if (resume && paused != null) emit(NodeEvent.ReceiverStarted(paused))
    }

    /**
     * A device connects to the open window and gets as far as the code, as
     * the core reports it: Connected, SessionCode, then the code prompt, all
     * for the window's operation. Returns the prompt, or null (and delivers
     * nothing) when no window is open.
     */
    fun attemptOnWindow(
        peer: PeerInfo = Fixtures.peer(),
        code: String = "654321",
        promptId: ULong = 2u,
        expiresAtMs: ULong = Fixtures.inTwoMinutes(),
    ): Prompt? {
        val window = windowOperation ?: return null
        val prompt = Fixtures.code(id = promptId, operation = window, code = code, peer = peer, expiresAtMs = expiresAtMs)
        emit(NodeEvent.Connected(window, peer))
        emit(NodeEvent.SessionCode(window, peer, code))
        emit(NodeEvent.PromptOpened(prompt))
        return prompt
    }

    /** Every cancel() on one of this fake's operations. */
    private fun cancelled(operation: ULong) {
        note(Call.Cancel(operation))
        val (isScripted, kind, isWindow) = synchronized(lock) {
            Triple(scripted, kinds[operation], openWindow == operation)
        }
        if (!isScripted) return
        if (isWindow) {
            endWindow(operation, PairingResult.Cancelled)
        } else if (kind == Kind.PAIR && synchronized(lock) { ended.add(operation) }) {
            emit(NodeEvent.PairingFinished(operation, PairingResult.Cancelled))
        }
    }

    /** Throws [error] while this device's window is open, as the core refuses then. */
    private fun refuseWhileWindowOpen(error: DropumiException) {
        if (synchronized(lock) { openWindow != null }) throw error
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

    private fun note(call: Call) {
        val hook = synchronized(lock) {
            log.add(call)
            callHook
        }
        hook?.invoke(call)
    }

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

    private fun newOperation(kind: Kind): FakeOperation = synchronized(lock) {
        FakeOperation(nextOperation, ::cancelled).also {
            operationsById[nextOperation] = it
            kinds[nextOperation] = kind
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
        refuseWhileWindowOpen(DropumiException.Busy(NOT_QUIET))
        synchronized(lock) { deviceName = name }
        failAfterApplying("setDeviceName")
        return identity()
    }

    override fun resetIdentity(): IdentityInfo {
        fail("resetIdentity")
        note(Call.ResetIdentity)
        refuseWhileWindowOpen(DropumiException.Busy(NOT_QUIET))
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
        refuseWhileWindowOpen(DropumiException.Busy("a pairing window is open"))
        val info = ReceiverInfo(
            port = 47321u,
            instance = if (options.advertise) "dropumi-0000abcd" else null,
            policy = options.policy,
            accept = options.accept,
            everyoneUntil = null,
        )
        synchronized(lock) {
            receiving = info
            receiverAccepting = true
        }
        return info
    }

    override fun stopAccepting() {
        note(Call.StopAccepting)
        synchronized(lock) { receiverAccepting = false }
        if (stopAcceptingEndsReceiver) {
            synchronized(lock) { receiving = null }
            emit(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED))
        }
    }

    override fun stopReceiving() {
        note(Call.StopReceiving)
        synchronized(lock) {
            receiving = null
            receiverAccepting = false
        }
    }

    override fun setReceivePolicy(policy: ReceivePolicy, accept: AcceptMode): ReceiverInfo {
        note(Call.SetReceivePolicy(policy, accept))
        refuseWhileWindowOpen(DropumiException.InvalidState("not receiving"))
        synchronized(lock) {
            // The core keeps the change, so a window resumes the receiver with it.
            if (scripted) receiving = receiving?.copy(policy = policy, accept = accept, everyoneUntil = null)
        }
        return ReceiverInfo(47321u, null, policy, accept, null)
    }

    /** True only while the receiver accepts (see [receiverAccepting]), as the core answers. */
    override fun cancelIncoming(transferId: String): Boolean {
        note(Call.CancelIncoming(transferId))
        return synchronized(lock) { receiverAccepting }
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
        return started("send", newOperation(Kind.SEND))
    }

    override fun pair(target: SendTarget): CoreOperation {
        fail("pair")
        synchronized(lock) { if (openWindow != null) pairedDuringWindow = true }
        note(Call.Pair(target))
        gate("pair")
        return started("pair", newOperation(Kind.PAIR))
    }

    override fun openPairingWindow(minutes: UInt): CoreOperation {
        fail("openPairingWindow")
        note(Call.OpenPairingWindow)
        gate("openPairingWindow")
        // As in the core: the window is marked open before the receiver it
        // pauses reports its stop, so a restart in between is refused.
        refuseWhileWindowOpen(DropumiException.Busy("a pairing window is already open"))
        val op = newOperation(Kind.WINDOW)
        val (isScripted, paused) = synchronized(lock) {
            if (scripted) {
                openWindow = op.id
                pausedReceiver = receiving
                receiving = null
            }
            scripted to pausedReceiver
        }
        if (isScripted) {
            if (paused != null) emit(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED))
            // The paused receiver's port, or the core's default one.
            emit(NodeEvent.PairingWindowOpened(op.id, paused?.port ?: DEFAULT_PORT))
        }
        return started("openPairingWindow", op)
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

        /** The port the fake's receiver and windows listen on. */
        val DEFAULT_PORT: UShort = 47321u

        private const val NOT_QUIET = "stop receiving and wait for running operations first"
    }
}
