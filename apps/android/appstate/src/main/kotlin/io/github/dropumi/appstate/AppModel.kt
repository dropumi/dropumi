// SPDX-License-Identifier: MPL-2.0
//
// The app's state, driven by core events: a port of the Apple app's
// AppModel. UI-free and tested on the host JVM with a fake core. Rules it
// keeps (ADR-0004, ADR-0009, ADR-0013 D8):
//
// - It never answers a prompt by itself; a prompt closes only when the core
//   says so (`PromptClosed`).
// - An Accept counts only once the prompt is armed (PromptArming); a
//   Decline always counts. Back declines the prompt on screen.
// - A transfer is a success only when the core reports
//   `TransferFinished(COMPLETED)`; a saved file alone is not.
// - Peers are shown alias first; a name a peer claims is labelled as such;
//   a nearby name that matches a paired device's hint is only "looks like".
// - Every core call runs on the io dispatcher, never on the main thread.
//   State changes happen on the model's scope (the main thread in the app).
// - Services start only while Dropumi is visible (ADR-0013 D7): the
//   transfer service on Send and Accept, on OfferSent and TransferAccepted
//   while in front, and when the activity leaves. A refused start keeps the
//   transfer; its card says it may pause.
// - Pairing (Pairing.kt): only `PairingFinished(Paired)` means paired; the
//   model never answers a code or adds trust by itself. Showing the pairing
//   screen makes this device ready to pair; an attempt that reached a code
//   never makes it ready again without a tap.

package io.github.dropumi.appstate

import io.github.dropumi.core.CoreClient
import io.github.dropumi.core.CoreOperation
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.core.discovery.Advertising
import io.github.dropumi.core.files.DocumentException
import io.github.dropumi.core.files.Documents
import io.github.dropumi.core.files.ExportResult
import io.github.dropumi.core.files.ExportRules
import io.github.dropumi.core.files.ReceivedExporter
import io.github.dropumi.core.files.ReceivedFile
import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.CodePurpose
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.FileStatus
import io.github.dropumi.ffi.IdentityInfo
import io.github.dropumi.ffi.NearbyPeer
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.NodeSnapshot
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.OperationResult
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.PeerInfo
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.ReceiveOptions
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.ReceiverStopReason
import io.github.dropumi.ffi.ReceiverInfo
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TransferResult
import io.github.dropumi.ffi.TrustedDeviceInfo
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.yield

/** A notice at the top of the screen. Two banners are equal only if they are the same notice. */
data class Banner(
    val style: Style,
    val title: String,
    val message: String,
    val action: Action? = null,
    val id: Long = nextId.incrementAndGet(),
) {
    enum class Style { INFO, WARNING, DANGER }
    enum class Action { RETRY_RECEIVING, OPEN_EVERYONE, OPEN_SETTINGS, OPEN_PAIRING }

    private companion object {
        val nextId = AtomicLong()
    }
}

/** A file chosen for sending. */
data class OutgoingFile(
    /** The name to show (the core decides the name it offers). */
    val name: String,
    /** Bytes, when known. */
    val size: ULong?,
    val source: Source,
    val id: String = UUID.randomUUID().toString(),
) {
    sealed interface Source {
        /**
         * A file the app may read by path. [stagedDir] is the app's own copy
         * folder (`outgoing/<uuid>`), deleted once the transfer ends.
         */
        data class Path(val path: String, val stagedDir: String? = null) : Source

        /**
         * A content:// document (share sheet or document picker). It is read
         * through UriGuard when it is sent: as a stream when its size is
         * known, otherwise from a staged copy.
         */
        data class Document(val uri: String) : Source
    }
}

/** Pairing in progress. */
data class PairingSession(
    val operation: ULong,
    val role: Role,
    val port: UShort? = null,
    val expiresAtMs: Long? = null,
    val result: PairingResult? = null,
    /**
     * The device on the other end, once connected: only a claim until
     * `PairingFinished(Paired)`, and never shown as paired before.
     */
    val peer: PeerInfo? = null,
    /** A code was shown for this attempt (`SessionCode` or a code prompt). */
    val reachedCode: Boolean = false,
    /** When that code expires (the core's wall clock, in milliseconds). */
    val codeExpiresAtMs: Long? = null,
) {
    sealed interface Role {
        /** This device asked another one. */
        data class Initiator(val name: String) : Role

        /** This device waits for another one. */
        data object Window : Role
    }
}

/** Something the UI should show next. */
enum class Route { SEND }

/** Everything the screens render. */
data class AppState(
    val identity: IdentityInfo? = null,
    val trusted: List<TrustedDeviceInfo> = emptyList(),
    val nearby: List<NearbyPeer> = emptyList(),
    val discoveryProblem: String? = null,
    val receiver: ReceiverInfo? = null,
    val receiverStopping: Boolean = false,
    val visibility: ReceiveVisibility,
    val autoAcceptPaired: Boolean,
    val prompts: List<Prompt> = emptyList(),
    /** Prompts whose answer is on its way to the core. */
    val answering: Set<ULong> = emptySet(),
    /** Prompts the core took an answer for; they show "Waiting…" until the core closes them. */
    val answered: Set<ULong> = emptySet(),
    val arming: PromptArming = PromptArming(),
    val active: List<TransferRecord> = emptyList(),
    val history: List<HistoryEntry> = emptyList(),
    val pairing: PairingSession? = null,
    /** What the pairing screen shows. */
    val pairingFlow: PairingFlow = PairingFlow(),
    val draft: List<OutgoingFile> = emptyList(),
    val isInBackground: Boolean = false,
    val banner: Banner? = null,
    val route: Route? = null,
    val appearance: Appearance,
    val onboarded: Boolean,
    /** This device's advertisement; GateClosed means browse-only (ADR-0013 D5). */
    val advertising: AdvertiseState = AdvertiseState.GateClosed,
    /**
     * This device's pairing window is opening or open, or the core is
     * resuming the receiver the window paused. Receiving changes made
     * meanwhile are kept and applied once it is over.
     */
    val pairingWindowActive: Boolean = false,
    /** A rename of this device is under way (it stops and restarts the receiver). */
    val renaming: Boolean = false,
    /** The onboarding page on screen; kept here so a recreated screen shows the same one. */
    val onboardingPage: Int = 0,
    /** The transfer service: whether running transfers go on after the user leaves (ADR-0013 D7). */
    val transferService: TransferServiceState = TransferServiceState.OFF,
    /**
     * Show the card that asks to allow notifications: a transfer started
     * while Dropumi's notifications could not be seen, and the user has not
     * dismissed the card.
     */
    val askNotifications: Boolean = false,
) {
    /** The prompt to show now. */
    val currentPrompt: Prompt? get() = prompts.firstOrNull()

    /** The transfer service was refused: running transfers may pause while Dropumi is in the background. */
    val transfersMayPause: Boolean
        get() = transferService == TransferServiceState.REFUSED && active.isNotEmpty()

    /**
     * The name onboarding suggests: only the core's own `Dropumi-XXXX`, never
     * a system, model, Bluetooth or account name (ADR-0013 D3).
     */
    val suggestedDeviceName: String? get() = identity?.name
}

/** The prompt id. */
val Prompt.promptId: ULong
    get() = when (this) {
        is Prompt.Offer -> id
        is Prompt.Code -> id
    }

/**
 * The app model. Construct it off the main thread (it reads the history
 * file); call its methods from [scope]'s thread.
 *
 * @param scope where state changes and event handling run (the main thread in the app).
 * @param io where core calls and file work run.
 * @param advertising the advertiser's gate and state; closed means the
 *   receiver is started with `advertise = false` (browse-only).
 * @param localAddresses this device's IPv4 addresses on local networks.
 * @param documents reads content:// documents through UriGuard; without it
 *   documents are refused.
 * @param pendingShare items the share target handed over.
 * @param receiveDir the app-private folder the core saves received files in.
 * @param exporter saves received files to shared storage, on request only.
 * @param services starts the transfer service; without it every start is refused.
 */
class AppModel(
    private val core: CoreClient,
    private val settings: SettingsStore,
    private val historyStore: HistoryStore,
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher,
    private val clock: () -> Long = System::currentTimeMillis,
    private val advertising: Advertising = Advertising.CLOSED,
    private val localAddresses: () -> List<String> = { emptyList() },
    private val documents: Documents? = null,
    private val pendingShare: PendingShare = PendingShare(),
    private val receiveDir: File? = null,
    private val exporter: ReceivedExporter? = null,
    private val services: ServiceStarter = ServiceStarter.NONE,
) {
    private val _state = MutableStateFlow(
        AppState(
            visibility = settings.visibility,
            autoAcceptPaired = settings.autoAcceptPaired,
            appearance = settings.appearance,
            onboarded = settings.onboarded,
            history = historyStore.load(),
            advertising = advertising.state.value,
        ),
    )
    val state: StateFlow<AppState> = _state.asStateFlow()

    private var eventJob: Job? = null
    private var receiverChain: Job? = null
    private var discoveryChain: Job? = null
    private var browseClients = 0
    private var browsing = false
    private val operations = HashMap<ULong, CoreOperation>()
    /** What a running send holds: its draft files and the staged copies made for it. */
    private val held = HashMap<String, Held>()

    private class Held(val files: List<OutgoingFile>, val stagedFolders: List<String>)

    /** Calls that start an operation (send, pair, pairing window) and have not returned yet. */
    private var startsUnderWay = 0

    /*
     * The pairing window, tracked by its own operation and never from the
     * pairing shown on screen. Opening a window pauses a running receiver
     * (ReceiverStopped); when the window ends (PairingFinished) the core
     * resumes that receiver with the settings it had (ReceiverStarted, or
     * ReceiverStopped if it could not). Until then the model leaves the
     * receiver alone, and then makes it match the settings of the moment.
     */

    /** Window opens under way (the core call, queued behind the receiver changes). */
    private var windowStarts = 0

    /** The open window's operation; cleared only by that operation's PairingFinished. */
    private var windowOperation: ULong? = null

    /** A receiver stopped while the window was opening or open: the core resumes it afterwards. */
    private var windowPausedReceiver = false

    /** The window ended and the core is resuming the receiver it paused. */
    private var resumeExpected = false
    private var resumeEpoch = 0

    /*
     * The pairing screen (a port of the Apple app's design). Showing it
     * makes this device ready to pair (it opens a window); tapping a device
     * closes that window and asks the device. Each verdict comes from
     * PairingClassifier.
     */

    /** Pairing screens on show. */
    private var pairingScreenClients = 0

    /**
     * Pairing screens that went away only because the activity is being
     * recreated (a rotation): they come back at once, so this device stays
     * ready to pair with the same window. Bounded by [RECREATE_WAIT_MS].
     */
    private var screensReturning = 0
    private var returnEpoch = 0

    /** The window was closed for the background: it opens again in front, if the screen is still shown. */
    private var reopenOnActive = false

    /**
     * An outcome the pairing screen shows when it is back: it came while no
     * pairing screen was shown, or the screen was only hidden (another tab).
     */
    private var outcomeKeptWhileAway = false

    /** A tap is closing this device's window or starting the request. */
    private var pairStarting = false

    /** Cancel was tapped while [pairStarting]. */
    private var requestAborted = false

    /** A window opening: waiting for the one before, or the core call. */
    private var windowOpenJob: Job? = null

    /** Counts window opens, so a close knows a newer one began. */
    private var windowAttempt = 0

    /** Why the opening window is to be closed once it is registered. */
    private var pendingWindowCancel: PairingCancelReason? = null

    /** The opening window was closed and then asked for again: it opens once the first has ended. */
    private var reopenAfterOpening = false

    /** How the last window ended; unknown again once the next one starts opening. */
    private var lastWindowResult: PairingResult? = null

    /** Why this device cancelled each of its pairing operations. */
    private val cancelReasons = HashMap<ULong, PairingCancelReason>()

    /** This device's answer to each code, written before it is sent: the result can arrive before the call returns. */
    private val codeAnswers = HashMap<ULong, Boolean>()

    /** Waits for a pairing condition (see [waitUntil]). */
    private class Waiter(val ready: () -> Boolean) {
        val done = CompletableDeferred<Unit>()
    }

    private val waiters = mutableListOf<Waiter>()

    private var renamesUnderWay = 0

    /**
     * Events for an operation the model does not know yet, kept while a
     * start call is under way: the core may report on the operation (a
     * refused connection, a pairing window's port) before the call returns.
     * Applied once the call has registered the operation; dropped when no
     * start call is under way. Bounded by [MAX_EARLY_OPERATIONS] and
     * [MAX_EARLY_EVENTS]; progress is coalesced.
     */
    private val early = LinkedHashMap<ULong, MutableList<NodeEvent>>()
    private var lastUnpairedNoticeMs: Long? = null
    private val historyWrites = Mutex()

    /** The user dismissed the notification card; it does not come back until the next launch. */
    private var notificationHintDismissed = false

    private inline fun update(crossinline change: AppState.() -> AppState) = _state.update { it.change() }

    // Lifecycle

    /** Starts listening for events and loads the identity. Receiving starts once onboarding is done. */
    suspend fun start() {
        if (eventJob != null) return
        eventJob = scope.launch { core.events.collect { apply(it) } }
        // Late advertising failures cannot reach the core; the adapter reports them here.
        scope.launch { advertising.state.collect { update { copy(advertising = it) } } }
        refreshIdentity()
        refreshTrusted()
        if (_state.value.onboarded) applyVisibility()
        takeSharedItems()
    }

    /** Ends onboarding with the chosen visibility and starts receiving (unless the choice is Off). */
    suspend fun finishOnboarding(choice: ReceiveVisibility? = null) {
        if (choice != null) {
            settings.visibility = choice
            update { copy(visibility = choice) }
        }
        settings.onboarded = true
        update { copy(onboarded = true) }
        applyVisibility()
    }

    /**
     * The app left the screen: stop accepting new connections (the listener
     * closes and the advertisement is withdrawn); running transfers finish.
     * Queued behind receiver changes already under way, so a receiver that is
     * still starting stops accepting once it is up. Called before [start],
     * it keeps [start] from receiving. A second call while in the background
     * does nothing: accepting stops once.
     */
    fun enterBackground() {
        if (_state.value.isInBackground) return
        update { copy(isInBackground = true) }
        // A pairing window nobody is pairing through closes (it opens again
        // when the app is back in front on the pairing screen); a code
        // comparison under way is kept.
        if (windowActive && !windowEngaged && windowCloseReason == null) {
            reopenOnActive = screenShown
            beginClosingWindow(PairingCancelReason.BACKGROUND)
        }
        chainReceiver { stopAcceptingInBackground() }
    }

    private suspend fun stopAcceptingInBackground() {
        val s = _state.value
        // Back in front meanwhile: becomeActive() keeps the receiver.
        if (!s.isInBackground || s.receiver == null) return
        // The core's receiver is stopping from now on: it takes no new
        // connections and ends (ReceiverStopped) once its running transfers
        // are done. Coming back to the front waits for that, then starts a
        // new one; a stopping receiver cannot be changed. Marked before the
        // call, so a ReceiverStopped that comes at once is never overwritten.
        update { copy(receiver = null, receiverStopping = true) }
        quietly { core.stopAccepting() }
    }

    /** The app is in front again. Still on the pairing screen, this device gets ready to pair again, unless a result came meanwhile. */
    suspend fun becomeActive() {
        update { copy(isInBackground = false) }
        if (reopenOnActive) {
            reopenOnActive = false
            requestWindow()
        }
        if (_state.value.onboarded) applyVisibility()
        takeSharedItems()
        // In front, a service start is allowed: running transfers get the
        // service a refused start left them without.
        keepTransfersRunning()
        refreshNotifications()
    }

    // Identity and trust

    suspend fun refreshIdentity() {
        try {
            val identity = work { core.identity() }
            update { copy(identity = identity) }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            show(e, "This device's identity could not be loaded")
        }
    }

    /**
     * Renames this device: stops the receiver, saves the name, starts the
     * receiver again. [AppState.renaming] is true meanwhile; a call made
     * while another rename runs is ignored.
     */
    suspend fun renameDevice(name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty() || renamesUnderWay > 0) return
        renamesUnderWay += 1
        update { copy(renaming = true) }
        try {
            renameNow(trimmed)
        } finally {
            renamesUnderWay -= 1
            update { copy(renaming = renamesUnderWay > 0) }
        }
    }

    private suspend fun renameNow(trimmed: String) {
        if (!pairingLeavesIdentityAlone("The name was not changed")) return
        val wasReceiving = _state.value.receiver != null
        if (wasReceiving) stopReceivingAndWait()
        val before = _state.value.identity?.name
        try {
            val identity = work { core.setDeviceName(trimmed) }
            update { copy(identity = identity) }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // The core may have saved the name and only failed to read it back.
            reloadIdentity()
            if (_state.value.identity?.name == before) show(e, "The name was not changed")
        }
        if (wasReceiving) applyVisibility()
    }

    /** Shows onboarding page [page]. */
    fun showOnboardingPage(page: Int) = update { copy(onboardingPage = page) }

    suspend fun resetIdentity() {
        if (!pairingLeavesIdentityAlone("The identity was not reset")) return
        val wasReceiving = _state.value.receiver != null
        if (wasReceiving) stopReceivingAndWait()
        val before = _state.value.identity?.fingerprint
        try {
            val identity = work { core.resetIdentity() }
            update { copy(identity = identity) }
            showNewIdentity()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // The core may have replaced the key and only failed to read it back.
            reloadIdentity()
            val now = _state.value.identity?.fingerprint
            if (now != null && now != before) showNewIdentity() else show(e, "The identity was not reset")
        }
        if (wasReceiving) applyVisibility()
    }

    /**
     * The core refuses identity changes while pairing (`check_quiet`).
     * Waits for a window this device is closing (the pairing screen was just
     * left) and for the receiver it paused; says so, and returns false,
     * while pairing is under way.
     */
    private suspend fun pairingLeavesIdentityAlone(title: String): Boolean {
        if (windowCloseReason != null) waitUntil(WINDOW_CLOSE_WAIT_MS) { windowGone }
        if (pairStarting || requestRunning || windowActive) {
            update { copy(banner = Banner(Banner.Style.WARNING, title, "Pairing is under way. Finish or stop it first.")) }
            return false
        }
        if (pairingHold) waitUntil(RESUME_WAIT_MS) { !pairingHold }
        return true
    }

    private fun showNewIdentity() = update {
        copy(banner = Banner(Banner.Style.WARNING, "New identity", "Devices you paired with must pair again."))
    }

    /** Reloads the identity without a banner; keeps the old one on failure. */
    private suspend fun reloadIdentity() {
        val identity = quietly { core.identity() } ?: return
        update { copy(identity = identity) }
    }

    suspend fun refreshTrusted() {
        val trusted = quietly { core.trustedDevices() } ?: emptyList()
        update { copy(trusted = trusted) }
    }

    /** Adds a paired device by its full fingerprint. Returns whether it is now paired. */
    suspend fun trust(fingerprint: String, alias: String?): Boolean {
        return try {
            work { core.trustFingerprint(fingerprint, alias) }
            refreshTrusted()
            true
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // The core may have saved the device and only failed to read it back.
            refreshTrusted()
            val wanted = compactFingerprint(fingerprint)
            if (_state.value.trusted.any { compactFingerprint(it.fingerprint) == wanted }) return true
            show(e, "Not added")
            false
        }
    }

    /** Renames a paired device. Returns whether it was renamed; a failure shows a banner. */
    suspend fun rename(device: TrustedDeviceInfo, alias: String): Boolean =
        trustChange("Not renamed") { core.renameTrusted(device.fingerprint, alias) }

    /** Removes a paired device. Returns whether it was removed; a failure shows a banner. */
    suspend fun forget(device: TrustedDeviceInfo): Boolean =
        trustChange("Not removed") { core.untrust(device.fingerprint) }

    private suspend fun trustChange(failureTitle: String, change: () -> Boolean): Boolean {
        val done = try {
            work(change)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            show(e, failureTitle)
            null
        }
        refreshTrusted()
        if (done == false) {
            update { copy(banner = Banner(Banner.Style.WARNING, failureTitle, "This device is no longer in the list.")) }
        }
        return done == true
    }

    // Receiving

    /**
     * Changes who may send to this device. During a pairing window the
     * change is kept and applied when the window is over (the core resumes
     * the paused receiver with its old settings; the model then changes it).
     */
    suspend fun setVisibility(value: ReceiveVisibility) {
        settings.visibility = value
        update { copy(visibility = value) }
        applyVisibility()
    }

    /** Like [setVisibility], applied after a pairing window. */
    suspend fun setAutoAcceptPaired(value: Boolean) {
        settings.autoAcceptPaired = value
        update { copy(autoAcceptPaired = value) }
        applyVisibility()
    }

    /** A pairing window is opening or open, or the receiver it paused is being resumed. */
    private val pairingHold: Boolean
        get() = windowStarts > 0 || windowOperation != null || resumeExpected

    /**
     * Publishes [pairingHold]. When it ends, the receiver is made to match
     * the settings of the moment (unless [reconcile] is false: the caller
     * does that itself).
     */
    private fun syncPairingHold(reconcile: Boolean = true) {
        val held = pairingHold
        val was = _state.value.pairingWindowActive
        if (held != was) {
            update { copy(pairingWindowActive = held) }
            if (was && reconcile) chainReceiver { matchReceiver() }
        }
        checkWaiters()
    }

    /**
     * Makes the receiver match the visibility and the accept setting. Calls
     * run one after another (launch and "became active" both ask), so the
     * receiver is never started twice; [enterBackground] queues behind them.
     */
    suspend fun applyVisibility() {
        chainReceiver { applyVisibilityNow() }.join()
    }

    /**
     * Makes a receiver that is running (or none) match the settings now. The
     * core also starts a receiver by itself: when a pairing window ends it
     * resumes the one the window paused, with the settings it had then, and
     * this may be after the user chose Off or left the app.
     */
    private suspend fun matchReceiver() {
        val s = _state.value
        if (pairingHold || !s.onboarded) return
        if (s.isInBackground) {
            stopAcceptingInBackground()
            return
        }
        val info = s.receiver
        if (info != null && info.matches(s)) return
        applyVisibilityNow()
    }

    /** Whether a running receiver already has the chosen policy kind and accept mode. */
    private fun ReceiverInfo.matches(s: AppState): Boolean {
        val policyMatches = when (s.visibility) {
            ReceiveVisibility.OFF -> false
            ReceiveVisibility.PAIRED_ONLY -> policy is ReceivePolicy.TrustedOnly
            ReceiveVisibility.EVERYONE -> policy is ReceivePolicy.Everyone
        }
        return policyMatches && accept == s.acceptMode
    }

    private val AppState.acceptMode: AcceptMode
        get() = if (autoAcceptPaired) AcceptMode.AUTO_TRUSTED else AcceptMode.PROMPT

    private fun chainReceiver(body: suspend () -> Unit): Job {
        val previous = receiverChain
        val job = scope.launch {
            previous?.join()
            body()
        }
        receiverChain = job
        return job
    }

    private suspend fun applyVisibilityNow() {
        val s = _state.value
        if (s.isInBackground) return
        // The core refuses receiver changes while a pairing window holds the
        // receiver, and resumes it with its old settings afterwards; the
        // change waits until then (syncPairingHold, receiverStarted).
        if (pairingHold) return
        val accept = s.acceptMode
        val policy = s.visibility.policy
        if (policy == null) {
            if (s.receiver != null) {
                quietly { core.stopReceiving() }
                // Turning receiving back on waits for ReceiverStopped, then
                // starts a new receiver.
                update { copy(receiver = null, receiverStopping = true) }
            }
            return
        }
        try {
            if (s.receiver != null) {
                val info = work { core.setReceivePolicy(policy, accept) }
                update { copy(receiver = info) }
            } else if (!s.receiverStopping) {
                // Advertised only where the R21 gate is open (ADR-0013 D5);
                // elsewhere the core never calls advertise, and peers reach
                // this device by address or after pairing.
                val options = ReceiveOptions(policy, accept, null, advertising.gateOpen)
                val info = work { core.startReceiving(options) }
                update { copy(receiver = info) }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            show(e, "Receiving could not start", Banner.Action.RETRY_RECEIVING)
        }
    }

    /** Stops the receiver and waits (at most a few seconds) for the core to report it stopped. */
    private suspend fun stopReceivingAndWait() {
        quietly { core.stopReceiving() }
        update { copy(receiver = null, receiverStopping = true) }
        withTimeoutOrNull(STOP_WAIT_MS) { _state.first { !it.receiverStopping } }
    }

    /** Addresses other devices can type to reach this one ("ip:port"); empty when not receiving. */
    fun connectionDetails(): List<String> {
        val port = _state.value.receiver?.port ?: return emptyList()
        return localAddresses().map { "$it:$port" }
    }

    /**
     * Addresses another device can type to pair with this one while the
     * pairing window is open; empty when no window is listening. Where this
     * device is not advertised (ADR-0013 D5), this is how it is reached.
     */
    fun pairingAddresses(): List<String> {
        val session = _state.value.pairing ?: return emptyList()
        if (session.role != PairingSession.Role.Window || session.result != null) return emptyList()
        val port = session.port ?: return emptyList()
        return localAddresses().map { "$it:$port" }
    }

    // Discovery

    /** Call when a device list appears; browsing runs only while one is visible. */
    fun beginBrowsing() {
        browseClients += 1
        if (browseClients == 1) syncDiscovery()
    }

    fun endBrowsing() {
        browseClients = maxOf(0, browseClients - 1)
        if (browseClients == 0) syncDiscovery()
    }

    /**
     * Starts or stops discovery to match the visible lists. Calls run one
     * after another, so a quick appear and disappear never leaves discovery
     * running (or stopped) by mistake.
     */
    private fun syncDiscovery() {
        val previous = discoveryChain
        discoveryChain = scope.launch {
            previous?.join()
            val wanted = browseClients > 0
            if (wanted == browsing) return@launch
            browsing = wanted
            if (wanted) {
                try {
                    work { core.startDiscovery() }
                    val peers = work { core.nearbyPeers() }
                    update { copy(discoveryProblem = null, nearby = sortedNearby(peers)) }
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    browsing = false
                    update { copy(discoveryProblem = ErrorCatalog.message(e)) }
                }
            } else {
                quietly { core.stopDiscovery() }
            }
        }
    }

    // Draft (files to send)

    fun addToDraft(files: List<OutgoingFile>) = update {
        val known = draft.map { it.source }.toMutableSet()
        copy(draft = draft + files.filter { known.add(it.source) })
    }

    fun removeFromDraft(file: OutgoingFile) {
        update { copy(draft = draft.filterNot { it.id == file.id }) }
        release(listOf(file))
    }

    fun clearDraft() {
        val files = _state.value.draft
        update { copy(draft = emptyList()) }
        release(files)
    }

    /**
     * Adds content:// documents to the draft. Each passes UriGuard's scheme
     * and own-provider checks before it is queried; refused ones are left out.
     * Returns how many were added.
     */
    suspend fun addDocuments(uris: List<String>): Int {
        val docs = documents ?: return 0
        val files = work {
            uris.mapNotNull { uri ->
                val described = try {
                    docs.describe(uri)
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    null
                }
                described?.let { OutgoingFile(it.name, it.size, OutgoingFile.Source.Document(uri)) }
            }
        }
        if (files.size < uris.size) {
            update { copy(banner = Banner(Banner.Style.WARNING, "Some items can't be sent", "Dropumi can only send files other apps share with it.")) }
        }
        addToDraft(files)
        return files.size
    }

    /** Picks up items the share target handed over and opens Send. */
    suspend fun takeSharedItems() {
        val shared = pendingShare.take()
        if (shared.isEmpty()) return
        if (addDocuments(shared) > 0) update { copy(route = Route.SEND) }
    }

    // Sending

    /**
     * Sends the draft. [allowUnverified] lets a receiver that is not paired
     * take the files after both users compare a code.
     */
    suspend fun sendDraft(target: SendTarget, targetName: String, allowUnverified: Boolean) {
        val files = _state.value.draft
        if (files.isEmpty()) return
        val staged = mutableListOf<String>()
        var recordId: String? = null
        try {
            val items = files.map { file ->
                when (val source = file.source) {
                    is OutgoingFile.Source.Path -> SendItem.Path(source.path)
                    is OutgoingFile.Source.Document -> prepare(source.uri, staged)
                }
            }
            startOperation({ core.send(target, items, allowUnverified) }) { op ->
                val record = TransferRecord(
                    id = "out-${op.id}",
                    direction = Direction.OUTGOING,
                    operation = op.id,
                    targetName = targetName,
                    fileNames = files.map { it.name },
                    totalBytes = files.fold(0uL) { sum, f -> sum + (f.size ?: 0uL) },
                    phase = TransferPhase.Connecting,
                    startedAtMs = clock(),
                )
                held[record.id] = Held(files, staged.toList())
                recordId = record.id
                update { copy(active = listOf(record) + active, draft = emptyList()) }
            }
        } catch (e: CancellationException) {
            deleteFolders(staged)
            throw e
        } catch (e: Exception) {
            deleteFolders(staged)
            show(e, "Not sent")
            return
        }
        // Send was the user's tap, so Dropumi is in front. The transfer
        // service keeps the send going after the user leaves, and its start
        // carries the documents' read grants, which outlive the activity
        // that received them (ADR-0013 D6). Not for a send that already
        // ended (a refused connection can end it before send returns).
        if (_state.value.active.any { it.id == recordId }) {
            val grants = files.mapNotNull { (it.source as? OutgoingFile.Source.Document)?.uri }.map { GrantSpec(it) }
            startTransferService(grants)
        }
    }

    /** The item for a document: a stream, or a staged copy whose folder is noted in [staged]. */
    private suspend fun prepare(uri: String, staged: MutableList<String>): SendItem {
        val docs = documents ?: throw DocumentException(DocumentException.Reason.REFUSED)
        return when (val prepared = runInterruptible(io) { docs.prepare(uri) }) {
            is Documents.Prepared.Stream -> prepared.item
            is Documents.Prepared.Staged -> {
                staged += prepared.folder.path
                prepared.item
            }
        }
    }


    /**
     * Cancels [record] (the card's Cancel). A send's operation always takes
     * the cancel. A receive is cancelled by the core's receiver, which can
     * say no: once Dropumi has left the screen and stopped accepting, the
     * core no longer holds the canceller of the transfers that keep its
     * stopping receiver alive, and they go on until they end. A notice then
     * says that the transfer could not be cancelled, instead of nothing
     * happening (no notice if the transfer has ended meanwhile).
     */
    fun cancel(record: TransferRecord) = cancel(record, sayIfNotCancelled = true)

    private fun cancel(record: TransferRecord, sayIfNotCancelled: Boolean) {
        val handle = record.operation?.let { operations[it] }
        val transferId = record.transferId
        when {
            handle != null -> scope.launch { quietly { handle.cancel() } }
            transferId != null -> scope.launch {
                val cancelled = quietly { core.cancelIncoming(transferId) } == true
                if (!cancelled && sayIfNotCancelled && _state.value.active.any(incoming(transferId))) noticeNotCancelled()
            }
        }
    }

    private fun noticeNotCancelled() = update {
        copy(
            banner = Banner(
                Banner.Style.WARNING,
                "Could not cancel",
                "Dropumi could not cancel this transfer. A transfer that was already running when you left the app " +
                    "continues until it ends.",
            ),
        )
    }

    // Prompts

    /**
     * Answers [prompt]. An Accept for a prompt that is not on screen and armed
     * is ignored (the UI never sends one; this keeps it so). The prompt stays
     * until the core closes it, and takes no second answer meanwhile.
     */
    suspend fun answer(prompt: Prompt, yes: Boolean) {
        val id = prompt.promptId
        val s = _state.value
        if (id in s.answering || id in s.answered) return
        if (yes && !s.arming.canAccept(id)) return
        // Accept was the user's tap, so Dropumi is in front: the transfer
        // service keeps the receive going after the user leaves.
        if (yes && prompt is Prompt.Offer && s.active.any(incoming(prompt.transferId))) {
            startTransferServiceIfNeeded()
        }
        update { copy(answering = answering + id) }
        // A code answer is noted first: the pairing can end before the call
        // returns, and only this device's answer tells "you said no" from
        // "the other device said no".
        val pairingOperation = (prompt as? Prompt.Code)?.takeIf { it.purpose == CodePurpose.PAIRING }?.operation
        if (pairingOperation != null) codeAnswers[pairingOperation] = yes
        try {
            when (prompt) {
                is Prompt.Offer -> work { core.answerOffer(id, yes) }
                is Prompt.Code -> work { core.answerCode(id, yes) }
            }
            // Waiting for PromptClosed; a prompt closed meanwhile is not marked.
            update { if (prompts.any { it.promptId == id }) copy(answered = answered + id) else this }
        } catch (_: DropumiException.PromptNotFound) {
            if (pairingOperation != null) codeAnswers.remove(pairingOperation)
            setPrompts { filterNot { it.promptId == id } }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            if (pairingOperation != null) codeAnswers.remove(pairingOperation)
            show(e, "The answer was not sent")
        } finally {
            update { copy(answering = answering - id) }
        }
    }

    /** Back (or Escape) while a prompt shows: declines it; ignored while an answer is in flight or given. */
    suspend fun back() {
        val s = _state.value
        val prompt = s.currentPrompt ?: return
        if (prompt.promptId in s.answering || prompt.promptId in s.answered) return
        answer(prompt, false)
    }

    /**
     * A page of [promptId]'s offered files from [offset] (the prompt itself
     * carries the first 200). At most [OFFER_PAGE] files per call, as the
     * core caps it. Null when the core no longer holds the prompt or the page
     * could not be read; nothing is shown for that.
     */
    suspend fun offerFiles(promptId: ULong, offset: Int, limit: Int): List<OfferedFile>? {
        val count = limit.coerceAtMost(OFFER_PAGE)
        if (offset < 0 || count <= 0) return emptyList()
        return quietly { core.offerFiles(promptId, offset.toUInt(), count.toUInt()) }
    }

    /** The activity resumed: the prompt on screen re-arms. */
    fun promptResumed() = update { copy(arming = arming.resumed()) }

    /** The window regained focus: the prompt on screen re-arms. */
    fun promptFocused() = update { copy(arming = arming.focused()) }

    /** The UI's arming timer for [promptId] in [epoch] ran for [elapsedMs]. */
    fun promptElapsed(promptId: ULong, epoch: Int, elapsedMs: Long) =
        update { copy(arming = arming.elapsed(promptId, epoch, elapsedMs)) }

    /** Changes the prompt list; arming follows the prompt on screen. */
    private fun setPrompts(change: List<Prompt>.() -> List<Prompt>) = update {
        val prompts = prompts.change()
        val ids = prompts.mapTo(HashSet()) { it.promptId }
        copy(
            prompts = prompts,
            answered = answered.filterTo(HashSet()) { it in ids },
            arming = arming.shown(prompts.firstOrNull()?.promptId),
        )
    }

    // Pairing

    /** What the pairing screen shows now. */
    private val flow: PairingFlow get() = _state.value.pairingFlow

    private fun setFlow(value: PairingFlow) = update { copy(pairingFlow = value) }

    private fun setPhase(phase: PairingFlow.Phase) = update { copy(pairingFlow = pairingFlow.copy(phase = phase)) }

    /**
     * The pairing screen appeared. The first one makes this device ready to
     * pair (it opens a window), unless this device is already pairing, or an
     * outcome that came meanwhile is still to be seen. A screen that comes
     * back after its activity was recreated ([pairingScreenRecreating])
     * finds everything as it was. Browsing is the screen's own
     * (BrowseWhileVisible), so it follows the activity's lifecycle.
     */
    fun enterPairingScreen() {
        pairingScreenClients += 1
        if (screensReturning > 0) {
            screensReturning -= 1
            return
        }
        if (pairingScreenClients != 1) return
        val away = outcomeKeptWhileAway
        outcomeKeptWhileAway = false
        // A request started just before the screen appeared, or a device
        // pairing with this one: the screen shows it.
        if (pairStarting || requestRunning || windowEngaged) return
        if (showingResult && away) return
        setFlow(PairingFlow(note = if (away) flow.note else null))
        requestWindow()
    }

    /**
     * The pairing screen went away. The last one closes this device's
     * window, unless a device is pairing through it. A request keeps
     * running; an outcome that comes meanwhile is shown when the screen is
     * back. [hidden]: the screen is only hidden (another tab is shown), so
     * an outcome on it is kept for its return.
     */
    fun leavePairingScreen(hidden: Boolean = false) {
        if (pairingScreenClients == 0) return
        pairingScreenClients -= 1
        if (screenShown) return
        lastScreenGone(hidden)
    }

    /**
     * The pairing screen went away only because its activity is being
     * recreated (a rotation, a fold, a theme change): it comes back at once,
     * and this device stays ready to pair with the same window. If it does
     * not come back within [RECREATE_WAIT_MS], it counts as left.
     */
    fun pairingScreenRecreating() {
        if (pairingScreenClients == 0) return
        pairingScreenClients -= 1
        screensReturning += 1
        val epoch = ++returnEpoch
        scope.launch {
            delay(RECREATE_WAIT_MS)
            if (returnEpoch != epoch || screensReturning == 0) return@launch
            screensReturning = 0
            if (pairingScreenClients == 0) lastScreenGone(hidden = false)
        }
    }

    private fun lastScreenGone(hidden: Boolean) {
        reopenOnActive = false
        if (windowActive && !windowEngaged) beginClosingWindow(PairingCancelReason.LEAVING)
        when (flow.phase) {
            is PairingFlow.Phase.Finished, is PairingFlow.Phase.Paused ->
                // A prompt is drawn over the screen and never removes it;
                // checked as on the Apple app all the same.
                if (hidden || _state.value.currentPrompt != null) outcomeKeptWhileAway = true else setFlow(PairingFlow())
            PairingFlow.Phase.Opening, is PairingFlow.Phase.Waiting -> setPhase(PairingFlow.Phase.Idle)
            PairingFlow.Phase.Idle, is PairingFlow.Phase.Switching, is PairingFlow.Phase.Asking, PairingFlow.Phase.Comparing -> Unit
        }
    }

    /** Start again, Try again, Pair another device: this device gets ready to pair again. */
    fun startWaiting() {
        if (_state.value.pairing?.result != null) update { copy(pairing = null) }
        outcomeKeptWhileAway = false
        setFlow(PairingFlow())
        requestWindow()
    }

    /** Stop: this device is no longer ready to pair. The screen then goes away. */
    fun stopPairing() {
        reopenOnActive = false
        setFlow(PairingFlow())
        if (windowActive) beginClosingWindow(PairingCancelReason.USER)
    }

    /**
     * Asks another device to pair (a tap). A tap while this device is
     * already pairing is ignored. This device's own window closes first: it
     * is the one asking now.
     */
    fun startPairing(target: SendTarget, name: String) {
        if (!beginRequest(name)) return
        scope.launch { runRequest(target, name) }
    }

    /**
     * [startPairing], returning once the core has answered (the interop
     * tests call it). Without the pairing screen, a request that cannot
     * start shows a banner.
     */
    suspend fun pair(target: SendTarget, name: String) {
        if (!beginRequest(name)) return
        runRequest(target, name)
    }

    /** The part of a tap that runs at once, so a second tap finds it. */
    private fun beginRequest(name: String): Boolean {
        if (pairStarting || requestRunning || windowEngaged) return false
        pairStarting = true
        requestAborted = false
        outcomeKeptWhileAway = false
        setFlow(PairingFlow(if (windowActive) PairingFlow.Phase.Switching(name) else PairingFlow.Phase.Asking(name)))
        return true
    }

    private suspend fun runRequest(target: SendTarget, name: String) {
        try {
            // The core takes a request while the window is open; it is
            // closed first so that nothing else uses it meanwhile. A device
            // that paired with this one meanwhile is shown instead.
            if (windowActive && closeOwnWindow(PairingCancelReason.DIALING) is PairingResult.Paired) return
            if (requestAborted) {
                pairStarting = false
                setFlow(PairingFlow(note = PairingOutcome.Cancelled))
                requestWindow()
                return
            }
            setPhase(PairingFlow.Phase.Asking(name))
            var request: ULong? = null
            try {
                startOperation({ core.pair(target) }) { op ->
                    request = op.id
                    update { copy(pairing = PairingSession(op.id, PairingSession.Role.Initiator(name))) }
                    pairStarting = false
                    // Prompts first: a kept end is then classified with the code.
                    codePromptsShown(op.id)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                pairStarting = false
                if (screenShown) {
                    val note = if (requestAborted) PairingOutcome.Cancelled else PairingClassifier.classifyRequestError(e, name)
                    setFlow(PairingFlow(note = note))
                    requestWindow()
                } else {
                    setFlow(PairingFlow())
                    show(e, "Pairing could not start")
                }
                return
            }
            val id = request
            if (requestAborted && id != null) cancelOperation(id, PairingCancelReason.USER)
        } finally {
            pairStarting = false
        }
    }

    /**
     * Cancel, while asking a device: this device gets ready to pair again,
     * with "Pairing cancelled.". On this device's window: Stop.
     */
    fun cancelPairing() {
        if (pairStarting) {
            requestAborted = true
            return
        }
        val session = _state.value.pairing
        if (session == null || session.result != null || session.operation !in operations) {
            update { copy(pairing = null) }
            // Nothing is asking any more: this device gets ready again.
            val phase = flow.phase
            if (phase is PairingFlow.Phase.Switching || phase is PairingFlow.Phase.Asking) {
                setFlow(PairingFlow(note = PairingOutcome.Cancelled))
                requestWindow()
            }
            return
        }
        cancelOperation(session.operation, PairingCancelReason.USER)
    }

    /** Done: the outcome was seen. */
    fun dismissPairing() {
        if (_state.value.pairing?.result != null) update { copy(pairing = null) }
        outcomeKeptWhileAway = false
        setFlow(PairingFlow())
    }

    /**
     * Lets one device pair with this one for five minutes, without the
     * pairing screen (which opens its own window), and returns once the
     * core has answered (the interop tests call it). The core pauses a
     * running receiver before the call returns and resumes it when the
     * window ends; receiving settings wait until then (the pairing hold).
     */
    suspend fun openPairingWindow() {
        if (pairStarting || requestRunning) return
        val window = windowOperation
        if (windowOpenJob == null && (window == null || cancelReasons.containsKey(window))) {
            setFlow(PairingFlow(PairingFlow.Phase.Opening))
            startWindowOpen(forScreen = false)
        }
        windowOpenJob?.join()
    }

    /**
     * The pairing screen wants this device ready to pair. Only these ask:
     * the screen appearing, Start again, a request of this device that ended
     * before a code, and the app back in front on the screen.
     */
    private fun requestWindow() {
        val s = _state.value
        if (!screenShown || !s.onboarded || pairStarting || requestRunning) return
        if (s.isInBackground) {
            reopenOnActive = true
            return
        }
        val window = windowOperation
        if (window != null && !cancelReasons.containsKey(window)) {
            // Already open (without the screen).
            val session = s.pairing
            val expires = session?.expiresAtMs
            when {
                windowEngaged -> setPhase(PairingFlow.Phase.Comparing)
                session != null && session.operation == window && session.port != null && expires != null ->
                    setPhase(PairingFlow.Phase.Waiting(expires))
                else -> setPhase(PairingFlow.Phase.Opening)
            }
            return
        }
        setPhase(PairingFlow.Phase.Opening)
        if (windowOpenJob != null) {
            // One is opening; if it is being closed, the next opens after it.
            if (pendingWindowCancel != null) reopenAfterOpening = true
            return
        }
        startWindowOpen(forScreen = true)
    }

    private fun startWindowOpen(forScreen: Boolean) {
        windowAttempt += 1
        pendingWindowCancel = null
        reopenAfterOpening = false
        // A close of this window must not report how an earlier one ended.
        lastWindowResult = null
        // Registered before it runs: on the main thread a launch can run at once.
        val job = scope.launch(start = CoroutineStart.LAZY) {
            val self = coroutineContext[Job]
            try {
                // Off the caller's stack: it may be applying an event.
                yield()
                openWindowWhenFree(forScreen)
            } finally {
                if (windowOpenJob === self) windowOpenJob = null
                checkWaiters()
            }
            if (reopenAfterOpening) {
                reopenAfterOpening = false
                requestWindow()
            }
        }
        windowOpenJob = job
        checkWaiters()
        job.start()
    }

    private suspend fun openWindowWhenFree(forScreen: Boolean) {
        // The window before may still be ending, or resuming the receiver it paused.
        waitUntil(RESUME_WAIT_MS) { windowOperation == null && !resumeExpected }
        // Closed (or no longer wanted) before it opened.
        if (pendingWindowCancel != null || (forScreen && !wantsWindow)) return
        if (windowOperation != null) {
            // The window before never reported its end.
            setPhase(PairingFlow.Phase.Paused(PairingOutcome.CouldNotWait(ErrorCatalog.text(ErrorCode.BUSY).message)))
            return
        }
        windowStarts += 1
        syncPairingHold()
        // Behind the receiver changes under way, which the core refuses once the window is open.
        chainReceiver { openWindowNow(forScreen) }.join()
    }

    private suspend fun openWindowNow(forScreen: Boolean) {
        try {
            // A receiver that is still stopping would stop during the window and pass for its pause.
            if (_state.value.receiverStopping) waitUntil(STOP_WAIT_MS) { !_state.value.receiverStopping }
            // Only a stop from here on is this window's pause.
            if (windowOperation == null) windowPausedReceiver = false
            var opened: ULong? = null
            try {
                startOperation({ core.openPairingWindow(PAIRING_WINDOW_MINUTES) }) { op ->
                    // Set before the events that came early (even its end) are applied.
                    opened = op.id
                    windowOperation = op.id
                    lastWindowResult = null
                    val expires = clock() + PAIRING_WINDOW_MINUTES.toLong() * 60_000
                    update { copy(pairing = PairingSession(op.id, PairingSession.Role.Window, expiresAtMs = expires)) }
                    // Prompts first: a kept end is then classified with the code.
                    codePromptsShown(op.id)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // Refused before the core paused anything.
                if (windowOperation == null) windowPausedReceiver = false
                // Unless it was closed meanwhile, the screen says why.
                if (pendingWindowCancel == null) {
                    setPhase(PairingFlow.Phase.Paused(PairingClassifier.classifyWindowError(e)))
                    if (!screenShown) show(e, "Pairing could not start")
                }
                return
            }
            val id = opened ?: return
            // Closed while it opened, or no longer wanted: closed now.
            val reason = pendingWindowCancel ?: if (forScreen && !wantsWindow) PairingCancelReason.LEAVING else null
            pendingWindowCancel = null
            if (reason != null) cancelOperation(id, reason)
        } finally {
            windowStarts -= 1
            syncPairingHold()
        }
    }

    /** Starts closing this device's window, opening or open. At once, so a window asked for next finds this one closing. */
    private fun beginClosingWindow(reason: PairingCancelReason) {
        reopenAfterOpening = false
        windowOperation?.let { cancelOperation(it, reason) }
        // Closed as soon as the core has opened it.
        if (windowOpenJob != null && pendingWindowCancel == null) pendingWindowCancel = reason
    }

    /**
     * Closes this device's window, opening or open, and waits until it has
     * ended (bounded). Returns how it ended, or null if it did not say in
     * time or a newer window began meanwhile.
     */
    private suspend fun closeOwnWindow(reason: PairingCancelReason): PairingResult? {
        beginClosingWindow(reason)
        val attempt = windowAttempt
        waitUntil(WINDOW_CLOSE_WAIT_MS) { windowGone || windowAttempt != attempt }
        return if (windowGone) lastWindowResult else null
    }

    /** Cancels one of this device's pairing operations, once, noting why. */
    private fun cancelOperation(operation: ULong, reason: PairingCancelReason) {
        if (cancelReasons.containsKey(operation)) return
        val handle = operations[operation] ?: return
        cancelReasons[operation] = reason
        scope.launch { quietly { handle.cancel() } }
    }

    /** The window's operation ended: the core resumes the receiver it paused, if any, and the hold lasts until it has. */
    private fun windowEnded(result: PairingResult?) {
        windowOperation = null
        lastWindowResult = result
        if (windowPausedReceiver) {
            windowPausedReceiver = false
            // The resume can be applied first, when the end was reported
            // before the window's call returned.
            if (_state.value.receiver == null) {
                resumeExpected = true
                // Bounded: if the core's resume never comes (or was not for
                // this window), the settings still apply.
                val epoch = ++resumeEpoch
                scope.launch {
                    delay(RESUME_WAIT_MS)
                    if (resumeExpected && resumeEpoch == epoch) {
                        resumeExpected = false
                        syncPairingHold()
                    }
                }
            }
        }
        syncPairingHold()
    }

    /** A pairing screen is shown, or is coming back after its activity was recreated. */
    private val screenShown: Boolean get() = pairingScreenClients > 0 || screensReturning > 0

    /**
     * The pairing screen is shown, in front, nothing else is pairing, and it
     * shows no result: a result waits for a tap (Start again, Try again), so
     * no window opens behind it.
     */
    private val wantsWindow: Boolean
        get() {
            val s = _state.value
            return screenShown && !s.isInBackground && s.onboarded && !pairStarting && !requestRunning && !showingResult
        }

    /** The pairing screen shows a result card. */
    private val showingResult: Boolean
        get() = flow.phase is PairingFlow.Phase.Finished || flow.phase is PairingFlow.Phase.Paused

    /** This device's request is running. */
    private val requestRunning: Boolean
        get() {
            val session = _state.value.pairing ?: return false
            return session.result == null && session.role is PairingSession.Role.Initiator
        }

    /** No window of this device is opening or open. */
    private val windowGone: Boolean get() = windowOperation == null && windowOpenJob == null

    private val windowActive: Boolean get() = !windowGone

    /** A device is pairing through this device's window: it connected, or a code was shown. */
    private val windowEngaged: Boolean
        get() {
            val window = windowOperation ?: return false
            val session = _state.value.pairing ?: return false
            return session.operation == window && (session.peer != null || session.reachedCode)
        }

    /** Why this device is closing its window, if it is. */
    private val windowCloseReason: PairingCancelReason?
        get() = windowOperation?.let { cancelReasons[it] } ?: if (windowOpenJob != null) pendingWindowCancel else null

    // Pairing events

    /** Code prompts the core opened for this operation before its start returned. */
    private fun codePromptsShown(operation: ULong) {
        for (prompt in _state.value.prompts) {
            if (prompt is Prompt.Code && prompt.operation == operation) {
                pairingEngaged(operation, prompt.peer, code = true, codeExpiresAtMs = prompt.expiresAtMs.toLong())
            }
        }
    }

    /** A device is pairing through the current attempt: it connected, or a code was shown. Its name stays a claim. */
    private fun pairingEngaged(operation: ULong, peer: PeerInfo, code: Boolean, codeExpiresAtMs: Long? = null) {
        val session = _state.value.pairing ?: return
        if (session.operation != operation || session.result != null) return
        update {
            copy(
                pairing = session.copy(
                    peer = peer,
                    reachedCode = session.reachedCode || code,
                    codeExpiresAtMs = codeExpiresAtMs ?: session.codeExpiresAtMs,
                ),
            )
        }
        // An attempt this device is ending shows its own next step.
        if (cancelReasons.containsKey(operation) || showingResult) return
        setPhase(PairingFlow.Phase.Comparing)
    }

    /** This device's window listens. */
    private fun windowListening(operation: ULong, port: UShort) {
        val session = _state.value.pairing ?: return
        if (session.operation != operation || session.role != PairingSession.Role.Window) return
        val expires = clock() + PAIRING_WINDOW_MINUTES.toLong() * 60_000
        update { copy(pairing = session.copy(port = port, expiresAtMs = expires)) }
        if (flow.phase == PairingFlow.Phase.Opening && !cancelReasons.containsKey(operation)) {
            setPhase(PairingFlow.Phase.Waiting(expires))
        }
    }

    private fun pairingFinished(operation: ULong, result: PairingResult) {
        operations.remove(operation)
        val before = _state.value.pairing
        val running = before != null && before.operation == operation && before.result == null
        if (before?.operation == operation) update { copy(pairing = before.copy(result = result)) }
        if (operation == windowOperation) windowEnded(result)
        val session = _state.value.pairing
        if (running && session != null) {
            val facts = facts(session, result)
            applyVerdict(PairingClassifier.classify(facts), facts)
        }
        codeAnswers.remove(operation)
        cancelReasons.remove(operation)
        scope.launch { refreshTrusted() }
    }

    private fun facts(session: PairingSession, result: PairingResult): PairingFacts {
        val now = clock()
        val answer = codeAnswers[session.operation]
        val role = session.role
        val name = (role as? PairingSession.Role.Initiator)?.name
        val windowElapsed = role == PairingSession.Role.Window &&
            (session.expiresAtMs?.let { now >= it - WINDOW_END_SLACK_MS } ?: false)
        val codeExpired = answer == null && (session.codeExpiresAtMs?.let { now >= it - CODE_END_SLACK_MS } ?: false)
        return PairingFacts(
            role = role,
            result = result,
            reachedCode = session.reachedCode,
            localAnswer = answer,
            localCodeExpired = codeExpired,
            windowElapsed = windowElapsed,
            listening = session.port != null,
            cancelReason = cancelReasons[session.operation],
            name = name,
        )
    }

    private fun applyVerdict(verdict: PairingVerdict, facts: PairingFacts) {
        val shown = screenShown
        when (verdict) {
            // This device ended it: a tap goes on to ask, and the screen
            // after Stop, or with no screen, shows nothing.
            PairingVerdict.Silent -> if (facts.cancelReason == PairingCancelReason.USER || !shown) {
                val phase = flow.phase
                if (phase == PairingFlow.Phase.Opening || phase is PairingFlow.Phase.Waiting || phase == PairingFlow.Phase.Comparing) {
                    setPhase(PairingFlow.Phase.Idle)
                }
            }
            is PairingVerdict.WaitAgain -> {
                setFlow(PairingFlow(note = verdict.note))
                if (shown) requestWindow() else outcomeKeptWhileAway = true
            }
            is PairingVerdict.Paused -> {
                setFlow(PairingFlow(PairingFlow.Phase.Paused(verdict.outcome)))
                // A card waits for a tap, also when the app comes back to the
                // front (the end can come after the background closed it).
                reopenOnActive = false
                if (!shown) outcomeKeptWhileAway = true
            }
            is PairingVerdict.Finished -> {
                setFlow(PairingFlow(PairingFlow.Phase.Finished(verdict.outcome)))
                reopenOnActive = false
                if (!shown) outcomeKeptWhileAway = true
            }
        }
    }

    // Waiting

    /**
     * Waits until [ready] holds or [timeoutMs] has passed, and returns
     * whether it holds. Nothing polls: the condition is checked after each
     * event and wherever the pairing hold or a window open changes
     * ([checkWaiters]).
     */
    private suspend fun waitUntil(timeoutMs: Long, ready: () -> Boolean): Boolean {
        if (ready()) return true
        val waiter = Waiter(ready)
        waiters += waiter
        val held = try {
            withTimeoutOrNull(timeoutMs) { waiter.done.await() } != null
        } finally {
            waiters.remove(waiter)
        }
        // Off the stack of whatever met the condition (it may be applying an event).
        yield()
        return held
    }

    /** Resumes every wait whose condition now holds, each exactly once. */
    private fun checkWaiters() {
        for (waiter in waiters.toList()) {
            if (waiter.ready()) {
                waiters.remove(waiter)
                waiter.done.complete(Unit)
            }
        }
    }

    // Settings, banners, routes, history

    fun setAppearance(value: Appearance) {
        settings.appearance = value
        update { copy(appearance = value) }
    }

    fun dismissBanner() = update { copy(banner = null) }

    /** Shows a notice from the screens (for example: no app can open a file). Text is the app's own, never a peer's. */
    fun notice(title: String, message: String) =
        update { copy(banner = Banner(Banner.Style.WARNING, title, message)) }

    /** The UI followed the route. */
    fun routeHandled() = update { copy(route = null) }

    /**
     * Clear (after the UI's confirmation): deletes the received copies of
     * every listed transfer, then removes those entries. A transfer that
     * finishes meanwhile keeps its entry, and an entry keeps the files that
     * could not be deleted, so no received file is left without one.
     */
    suspend fun clearHistory() {
        val entries = _state.value.history
        val left = deleteCopies(entries)
        val cleared = entries.map { it.id }.toSet()
        update {
            copy(
                history = history.mapNotNull { entry ->
                    if (entry.id !in cleared) return@mapNotNull entry
                    left[entry.id]?.let { entry.keeping(it) }
                },
            )
        }
        saveHistory()
        if (left.isNotEmpty()) noticeNotDeleted()
    }

    // Received files (ADR-0013 D4): they leave the app only on request.

    /** Whether Save goes straight to Downloads (API 29+); otherwise the UI asks for a folder first. */
    val savesToDownloads: Boolean get() = exporter?.savesToDownloads == true

    /** Saves a finished transfer's received files to Downloads/Dropumi (API 29+). */
    suspend fun saveToDownloads(entry: HistoryEntry) =
        save(entry, "Saved to Downloads") { exporter, files -> exporter.saveToDownloads(files) }

    /** Saves a finished transfer's received files into the folder the user picked (API 26-28). */
    suspend fun saveToFolder(entry: HistoryEntry, treeUri: String) =
        save(entry, "Saved") { exporter, files -> exporter.saveToFolder(treeUri, files) }

    private suspend fun save(
        entry: HistoryEntry,
        title: String,
        export: (ReceivedExporter, List<ReceivedFile>) -> ExportResult,
    ) {
        val exporter = exporter ?: return
        val files = receivedFiles(entry)
        if (files == null) {
            update { copy(banner = Banner(Banner.Style.WARNING, "Not saved", "These files can't be checked, so they were not saved.")) }
            return
        }
        when (val result = work { export(exporter, files) }) {
            is ExportResult.Saved -> {
                // The first time, say who else can see saved files (G45).
                val message = if (!settings.saveNoteShown) {
                    settings.saveNoteShown = true
                    SAVE_NOTE
                } else {
                    Formatting.fileCount(result.uris.size) + " saved."
                }
                update { copy(banner = Banner(Banner.Style.INFO, title, message)) }
            }
            is ExportResult.Failed -> {
                val why = when (result.reason) {
                    ExportResult.Reason.NOT_A_RECEIVED_FILE -> "The file is no longer in Dropumi."
                    ExportResult.Reason.CHANGED -> "The file changed after it was received, so it was not saved."
                    ExportResult.Reason.NO_SPACE -> "There is not enough free space."
                    ExportResult.Reason.WRITE_FAILED -> "The file could not be written."
                }
                val done = if (result.saved > 0) " ${result.saved} of ${files.size} files were saved." else ""
                update { copy(banner = Banner(Banner.Style.WARNING, "Not saved", why + done)) }
            }
        }
    }

    /** The entry's received files with their reported sizes; null if they cannot be checked. */
    private fun receivedFiles(entry: HistoryEntry): List<ReceivedFile>? {
        if (entry.outgoing || entry.savedAs.isEmpty() || entry.savedSizes.size != entry.savedAs.size) return null
        return entry.savedAs.zip(entry.savedSizes).map { (name, size) ->
            if (!ExportRules.isSingleName(name) || size > Long.MAX_VALUE.toULong()) return null
            ReceivedFile(name, size.toLong())
        }
    }

    /** Deletes a finished transfer's internal copies (after the UI's confirmation). Save never does. */
    suspend fun deleteReceived(entry: HistoryEntry) {
        if (receiveDir == null) return
        val left = deleteCopies(listOf(entry))[entry.id] ?: emptySet()
        update {
            copy(history = history.map { if (it.id == entry.id) it.keeping(left) else it })
        }
        saveHistory()
        if (left.isNotEmpty()) noticeNotDeleted()
    }

    /**
     * Deletes the received copies of [entries] (single names only, inside
     * the receive folder; a link is removed, never its target). Returns, by
     * entry id, the names of the files still there.
     */
    private suspend fun deleteCopies(entries: List<HistoryEntry>): Map<String, Set<String>> {
        val dir = receiveDir ?: return emptyMap()
        return work {
            buildMap {
                for (entry in entries) {
                    val left = entry.savedAs.filterTo(mutableSetOf()) { name ->
                        if (!ExportRules.isSingleName(name)) return@filterTo false
                        val file = File(dir, name)
                        !file.delete() && file.exists()
                    }
                    if (left.isNotEmpty()) put(entry.id, left)
                }
            }
        }
    }

    /** The entry with only the received files named in [names] (their sizes kept with them). */
    private fun HistoryEntry.keeping(names: Set<String>): HistoryEntry {
        val kept = savedAs.indices.filter { savedAs[it] in names }
        return copy(
            savedAs = kept.map { savedAs[it] },
            savedSizes = if (savedSizes.size == savedAs.size) kept.map { savedSizes[it] } else emptyList(),
        )
    }

    private fun noticeNotDeleted() = update {
        copy(banner = Banner(Banner.Style.WARNING, "Not all files were deleted", "The files that are left are still listed. Try again later."))
    }

    // Events

    /** Applies a batch of core events. */
    fun apply(batch: List<NodeEvent>) = batch.forEach(::apply)

    private fun apply(event: NodeEvent) {
        val op = event.operationId
        if (op != null && op !in operations && startsUnderWay > 0) {
            keepEarly(op, event)
            return
        }
        when (event) {
            is NodeEvent.ReceiverStarted -> receiverStarted(event.info)
            is NodeEvent.ReceiverStopped -> receiverStopped(event)
            is NodeEvent.PolicyChanged -> policyChanged(event)
            is NodeEvent.DiscoveryUnavailable -> update { copy(discoveryProblem = event.reason) }
            is NodeEvent.PeerFound -> update {
                copy(
                    nearby = sortedNearby(nearby.filterNot { it.instance == event.peer.instance } + event.peer),
                    discoveryProblem = null,
                )
            }
            is NodeEvent.PeerLost -> update { copy(nearby = nearby.filterNot { it.instance == event.instance }) }
            is NodeEvent.Connected -> event.operation?.let { op ->
                if (hasRecord(op)) {
                    updateRecord({ it.operation == op }) { copy(peer = event.peer) }
                } else {
                    pairingEngaged(op, event.peer, code = false)
                }
            }
            is NodeEvent.SessionCode -> event.operation?.let { op ->
                if (hasRecord(op)) {
                    updateRecord({ it.operation == op }) { copy(peer = event.peer, sessionCode = event.code) }
                } else {
                    pairingEngaged(op, event.peer, code = true)
                }
            }
            is NodeEvent.OfferSent -> {
                updateRecord({ it.operation == event.operation }) {
                    copy(
                        transferId = event.transferId,
                        peer = event.peer,
                        totalBytes = event.totalBytes,
                        phase = TransferPhase.WaitingForAcceptance,
                    )
                }
                transferMovingInFront()
            }
            is NodeEvent.OfferReceived -> offerReceived(event)
            is NodeEvent.PromptOpened -> promptOpened(event.prompt)
            is NodeEvent.PromptClosed -> setPrompts { filterNot { it.promptId == event.id } }
            is NodeEvent.TransferAccepted -> {
                updateRecord(matching(event.operation, event.transferId)) {
                    copy(phase = TransferPhase.Transferring)
                }
                transferMovingInFront()
            }
            is NodeEvent.TransferProgress -> {
                val now = clock()
                updateRecord(matching(event.operation, event.transferId)) { recorded(event.progress, now) }
            }
            is NodeEvent.FileSent -> updateRecord({ it.operation == event.operation }) {
                copy(fileResults = fileResults + FileResult(event.index, event.status, null, null))
            }
            is NodeEvent.FileSaved -> updateRecord(incoming(event.transferId)) {
                copy(fileResults = fileResults + FileResult(event.index, FileStatus.STORED, event.savedAs, event.blake3, event.size))
            }
            is NodeEvent.FileFailed -> updateRecord(incoming(event.transferId)) {
                copy(fileResults = fileResults + FileResult(event.index, event.status, null, null))
            }
            is NodeEvent.TransferFinished -> transferFinished(event)
            is NodeEvent.PairingWindowOpened -> windowListening(event.operation, event.port)
            is NodeEvent.PairingFinished -> pairingFinished(event.operation, event.result)
            is NodeEvent.SessionClosed -> if (event.code == NOT_AUTHORIZED) noticeRefused(event.peer)
            is NodeEvent.Warning -> showWarning(event.message)
            is NodeEvent.EventsDropped -> Unit
            NodeEvent.ResyncRequired -> resync()
        }
        checkWaiters()
    }

    private fun hasRecord(operation: ULong): Boolean = _state.value.active.any { it.operation == operation }

    /**
     * A receiver started. The model's own starts already set it (this event
     * can come before or after the call returns); one the model already asked
     * to stop is ignored. The core also starts one by itself after a pairing
     * window, with the settings it had: it is made to match the settings now.
     */
    private fun receiverStarted(info: ReceiverInfo) {
        if (_state.value.receiverStopping && !resumeExpected) return
        update { copy(receiver = info, receiverStopping = false) }
        if (resumeExpected) {
            resumeExpected = false
            syncPairingHold()
        } else {
            chainReceiver { matchReceiver() }
        }
    }

    private fun receiverStopped(event: NodeEvent.ReceiverStopped) {
        // While a window opens or is open, this is the pause: the core
        // resumes this receiver when the window ends.
        if (windowStarts > 0 || windowOperation != null) windowPausedReceiver = true
        update { copy(receiver = null, receiverStopping = false) }
        // The resume after a window failed: said below, like any other stop.
        if (resumeExpected) {
            resumeExpected = false
            syncPairingHold(reconcile = false)
        }
        val s = _state.value
        val wanted = !s.isInBackground && s.visibility != ReceiveVisibility.OFF
        when {
            event.reason != ReceiverStopReason.STOPPED && wanted -> update {
                copy(
                    banner = Banner(
                        Banner.Style.WARNING,
                        "Receiving stopped",
                        "This device stopped listening for files.",
                        Banner.Action.RETRY_RECEIVING,
                    ),
                )
            }
            // A stop that finished after the user turned receiving back on
            // (or the app came back to the front meanwhile).
            // A pairing window's pause is not restarted here: the core resumes the receiver after it.
            event.reason == ReceiverStopReason.STOPPED && s.onboarded && wanted && !pairingHold ->
                scope.launch { applyVisibility() }
        }
    }

    private fun policyChanged(event: NodeEvent.PolicyChanged) {
        update {
            copy(receiver = receiver?.copy(policy = event.policy, accept = event.accept, everyoneUntil = event.everyoneUntil))
        }
        if (_state.value.visibility == ReceiveVisibility.EVERYONE && event.policy == ReceivePolicy.TrustedOnly) {
            settings.visibility = ReceiveVisibility.PAIRED_ONLY
            update {
                copy(
                    visibility = ReceiveVisibility.PAIRED_ONLY,
                    banner = Banner(Banner.Style.INFO, "Back to paired devices only", "The 10 minutes for everyone ended."),
                )
            }
        }
    }

    private fun offerReceived(event: NodeEvent.OfferReceived) {
        val id = "in-${event.transferId}"
        if (_state.value.active.any { it.id == id }) return
        val names = offerFileNames(event.transferId) ?: emptyList()
        val record = TransferRecord(
            id = id,
            direction = Direction.INCOMING,
            operation = null,
            transferId = event.transferId,
            peer = event.peer,
            targetName = event.peer.claimedName,
            fileNames = names,
            fileCount = event.files.toInt(),
            totalBytes = event.totalBytes,
            phase = TransferPhase.WaitingForAnswer,
            startedAtMs = clock(),
        )
        update { copy(active = listOf(record) + active) }
    }

    private fun promptOpened(prompt: Prompt) {
        if (_state.value.prompts.any { it.promptId == prompt.promptId }) return
        setPrompts { this + prompt }
        when (prompt) {
            is Prompt.Offer -> updateRecord({ it.direction == Direction.INCOMING && it.transferId == prompt.transferId }) {
                copy(fileNames = prompt.files.map { it.name })
            }
            // A code for a start that has not returned is found when it does (codePromptsShown).
            is Prompt.Code -> prompt.operation?.let { op ->
                pairingEngaged(op, prompt.peer, code = true, codeExpiresAtMs = prompt.expiresAtMs.toLong())
            }
        }
    }

    private fun transferFinished(event: NodeEvent.TransferFinished) {
        val result = event.result
        val s = _state.value
        val record = event.operation?.let { op -> s.active.firstOrNull { it.operation == op } }
            ?: result.transferId?.let { id -> s.active.firstOrNull(incoming(id)) }
        when {
            record != null -> finish(record, result)
            event.direction == Direction.INCOMING -> finish(
                // Auto-accepted or never announced: still record the result.
                TransferRecord(
                    id = "in-${result.transferId ?: UUID.randomUUID()}",
                    direction = Direction.INCOMING,
                    operation = null,
                    targetName = "A device",
                    fileNames = emptyList(),
                    totalBytes = result.bytes,
                    phase = TransferPhase.Transferring,
                    startedAtMs = clock(),
                ),
                result,
            )
        }
    }

    private fun matching(operation: ULong?, transferId: String): (TransferRecord) -> Boolean {
        val s = _state.value
        if (operation != null && s.active.any { it.operation == operation }) return { it.operation == operation }
        return incoming(transferId)
    }

    private fun incoming(transferId: String): (TransferRecord) -> Boolean =
        { it.direction == Direction.INCOMING && it.transferId == transferId }

    private inline fun updateRecord(
        crossinline which: (TransferRecord) -> Boolean,
        crossinline change: TransferRecord.() -> TransferRecord,
    ) = update {
        val index = active.indexOfFirst(which)
        if (index < 0) this else copy(active = active.toMutableList().also { it[index] = it[index].change() })
    }

    private fun offerFileNames(transferId: String): List<String>? = _state.value.prompts
        .firstNotNullOfOrNull { p -> (p as? Prompt.Offer)?.takeIf { it.transferId == transferId } }
        ?.files?.map { it.name }

    private fun finish(record: TransferRecord, result: TransferResult) {
        val now = clock()
        val done = record.copy(phase = TransferPhase.Finished(result), finishedAtMs = now)
        record.operation?.let { operations.remove(it) }
        held.remove(record.id)?.let {
            release(it.files)
            deleteFolders(it.stagedFolders)
        }
        val label = done.peerLabel
        val stored = done.fileResults.filter { it.savedAs != null }
        val savedAs = stored.mapNotNull { it.savedAs }
        val entry = HistoryEntry(
            id = "${record.id}-${now / 1000}",
            outgoing = record.direction == Direction.OUTGOING,
            peerName = label.title,
            peerVerified = label.verified,
            fileNames = record.fileNames.ifEmpty { savedAs },
            totalBytes = record.totalBytes,
            outcome = HistoryEntry.outcome(result.status),
            summary = ErrorCatalog.summary(result, record.direction),
            finishedAtMs = now,
            savedAs = savedAs,
            savedSizes = if (stored.all { it.size != null }) stored.mapNotNull { it.size } else emptyList(),
        )
        update {
            val remaining = active.filterNot { it.id == record.id }
            copy(
                active = remaining,
                history = (listOf(entry) + history).take(historyStore.limit),
                // The last transfer ended: the service stops itself.
                transferService = if (remaining.isEmpty()) TransferServiceState.OFF else transferService,
            )
        }
        saveHistory()
    }

    // Transfers after the user leaves (ADR-0013 D7)

    /**
     * Starts the transfer service for running transfers that do not have
     * it yet. Only while Dropumi is visible: MainActivity calls this when it
     * pauses (never for a configuration change), and coming to the front
     * does too. A transfer still waiting for this user's answer does not
     * start it.
     */
    fun keepTransfersRunning() {
        val s = _state.value
        if (s.transferService == TransferServiceState.RUNNING) return
        if (s.active.none { it.phase != TransferPhase.WaitingForAnswer }) return
        startTransferService(emptyList())
    }

    /** OfferSent or TransferAccepted: data is about to move. In front, the transfer service starts for it. */
    private fun transferMovingInFront() {
        if (!_state.value.isInBackground) startTransferServiceIfNeeded()
    }

    private fun startTransferServiceIfNeeded() {
        if (_state.value.transferService != TransferServiceState.RUNNING) startTransferService(emptyList())
    }

    /**
     * Starts the transfer service (again: a new start also carries new
     * [grants]). Refused, the transfers go on without it and their cards say
     * they may pause; nothing is cancelled. A refused start does not stop a
     * service that already runs.
     */
    private fun startTransferService(grants: List<GrantSpec>) {
        if (_state.value.active.isEmpty()) return
        val result = services.start(ServiceKind.TRANSFER, grants)
        update {
            copy(
                transferService = when {
                    active.isEmpty() -> TransferServiceState.OFF
                    result == ServiceStart.STARTED -> TransferServiceState.RUNNING
                    transferService == TransferServiceState.RUNNING -> TransferServiceState.RUNNING
                    else -> TransferServiceState.REFUSED
                },
            )
        }
        // After the first transfer starts, and only on Transfers (never over
        // a prompt): ask to allow notifications if they cannot be seen.
        if (!notificationHintDismissed && !services.notificationsVisible) update { copy(askNotifications = true) }
    }

    /**
     * The transfer service could not go to the foreground and stopped
     * itself. The transfers go on without it.
     */
    fun transferServiceRefused() = update {
        copy(transferService = if (active.isEmpty()) TransferServiceState.OFF else TransferServiceState.REFUSED)
    }

    /**
     * The system's time limit for the transfer service ran out (API 35+;
     * only in the background, so accepting has stopped). Every running send
     * is cancelled, and the core is asked to cancel every running receive;
     * offers still waiting for this user's answer stay. The core cannot
     * cancel a receive that keeps its stopping receiver alive (see
     * [cancel]): that receive goes on without the service, and its card says
     * it may pause. No notice: the user did not ask to cancel.
     */
    fun transfersTimedOut() {
        transferServiceRefused()
        _state.value.active.filter { it.phase != TransferPhase.WaitingForAnswer }
            .forEach { cancel(it, sayIfNotCancelled = false) }
    }

    /** Whether notifications can be seen now (after the permission request, or back from Settings). */
    fun refreshNotifications() {
        if (_state.value.askNotifications && services.notificationsVisible) update { copy(askNotifications = false) }
    }

    /** "Not now" on the notification card: it stays away until the next launch. */
    fun dismissNotificationHint() {
        notificationHintDismissed = true
        update { copy(askNotifications = false) }
    }

    /**
     * The core refused a connection (`not_authorized`). Rate-limited, and
     * never in place of another notice: any host on the network can cause it.
     *
     * Without a peer the refusal came during the handshake, and the reason is
     * not known: a device that isn't paired, a pairing request while no
     * pairing window is open (from paired devices too), a paired device
     * while a window is open, or an unreadable list of paired devices. So the
     * notice says only that a device was refused and offers Pair a device;
     * it never offers Everyone.
     */
    private fun noticeRefused(peer: PeerInfo?) {
        if (peer != null && peer.trusted) return
        // While the pairing screen is shown or this device's window is
        // opening or open, the pairing screen says what happened to it.
        if (peer == null && (screenShown || windowActive)) return
        if (_state.value.banner != null) return
        val now = clock()
        val last = lastUnpairedNoticeMs
        if (last != null && now - last < UNPAIRED_NOTICE_INTERVAL_MS) return
        lastUnpairedNoticeMs = now
        val banner = if (peer == null) {
            Banner(
                Banner.Style.INFO,
                "A device was refused",
                "A device on this network tried to connect and was turned away. " +
                    "If it was trying to pair, open Pair a device on both devices.",
                Banner.Action.OPEN_PAIRING,
            )
        } else {
            Banner(
                Banner.Style.INFO,
                "A device that isn't paired tried to send",
                "Pair with it, or let everyone send for 10 minutes.",
                Banner.Action.OPEN_EVERYONE,
            )
        }
        update { copy(banner = banner) }
    }

    private fun resync() {
        scope.launch {
            val snapshot = quietly { core.snapshot() } ?: return@launch
            update {
                copy(
                    receiver = snapshot.receiver,
                    receiverStopping = snapshot.receiverStopping,
                    nearby = sortedNearby(snapshot.nearby),
                )
            }
            setPrompts { snapshot.prompts }
            resyncPairing(snapshot)
            // A receiver the core started (or stopped) may have been among
            // the dropped events.
            chainReceiver { matchReceiver() }
        }
    }

    /** Pairing ends that were among the dropped events, so the window's hold always ends. */
    private fun resyncPairing(snapshot: NodeSnapshot) {
        val known = snapshot.operations.associateBy { it.id }
        val session = _state.value.pairing?.takeIf { it.result == null }?.operation
        for (op in listOfNotNull(windowOperation, session).distinct()) {
            val info = known[op]
            val result = (info?.result as? OperationResult.Pairing)?.result
            when {
                result != null -> apply(NodeEvent.PairingFinished(op, result))
                // Only finished operations are forgotten.
                info == null && op == windowOperation -> windowEnded(null)
            }
        }
        if (resumeExpected && snapshot.receiver != null) {
            resumeExpected = false
            syncPairingHold(reconcile = false)
        }
    }

    // Operations

    /**
     * Runs [call] (which starts an operation) on the io dispatcher, then
     * [register]s the operation and applies the events that arrived for it
     * before the call returned. Nothing suspends between the call's return,
     * the registration and the replay, so no event is applied in between.
     */
    private suspend fun startOperation(call: () -> CoreOperation, register: (CoreOperation) -> Unit) {
        startsUnderWay += 1
        try {
            val op = work(call)
            operations[op.id] = op
            register(op)
            early.remove(op.id)?.forEach(::apply)
        } finally {
            startsUnderWay -= 1
            if (startsUnderWay == 0) early.clear()
        }
    }

    private fun keepEarly(op: ULong, event: NodeEvent) {
        val kept = early[op] ?: run {
            if (early.size >= MAX_EARLY_OPERATIONS) return
            mutableListOf<NodeEvent>().also { early[op] = it }
        }
        val terminal = event is NodeEvent.TransferFinished || event is NodeEvent.PairingFinished
        when {
            // Only the latest progress matters.
            event is NodeEvent.TransferProgress && kept.lastOrNull() is NodeEvent.TransferProgress ->
                kept[kept.lastIndex] = event
            kept.size < MAX_EARLY_EVENTS || terminal -> kept += event
        }
    }

    // Helpers

    /**
     * A warning from the core (today: a change saved without a confirmed
     * folder sync). Added to a banner already showing, so a message such as
     * "New identity" is not lost.
     */
    private fun showWarning(message: String) {
        val text = message.replaceFirstChar { it.uppercase() }
        update {
            val current = banner
            copy(
                banner = if (current != null) {
                    current.copy(message = current.message + "\n\n" + text)
                } else {
                    Banner(Banner.Style.WARNING, "Saved, but not confirmed on disk", text)
                },
            )
        }
    }

    private fun show(error: Throwable, title: String, action: Banner.Action? = null) = update {
        copy(banner = Banner(Banner.Style.WARNING, title, ErrorCatalog.message(error), action))
    }

    /** Deletes the app's own staged copies of [files], off the main thread. */
    private fun release(files: List<OutgoingFile>) =
        deleteFolders(files.mapNotNull { (it.source as? OutgoingFile.Source.Path)?.stagedDir })

    private fun deleteFolders(folders: List<String>) {
        if (folders.isEmpty()) return
        scope.launch(io) { folders.forEach { File(it).deleteRecursively() } }
    }

    /** Writes the current history, off the main thread; writes never overlap and the last one wins. */
    private fun saveHistory() {
        scope.launch(io) { historyWrites.withLock { historyStore.save(_state.value.history) } }
    }

    /** Runs a core call on the io dispatcher. */
    private suspend fun <T> work(body: () -> T): T = withContext(io) { body() }

    /** Runs a core call on the io dispatcher; a failure gives null. */
    private suspend fun <T> quietly(body: () -> T): T? = try {
        work(body)
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    /** Test hook: the operation handles the model holds. */
    internal val operationCount: Int get() = operations.size

    private companion object {
        const val STOP_WAIT_MS = 5_000L

        /** How long the model waits for the core to resume the receiver a pairing window paused. */
        const val RESUME_WAIT_MS = 30_000L

        /** How long the model waits for this device's window to report its end after a cancel. */
        const val WINDOW_CLOSE_WAIT_MS = 12_000L

        /** How long a pairing screen gone for an activity's recreation has to come back. */
        const val RECREATE_WAIT_MS = 5_000L

        /** A window ending this close to its expiry ended because its time was up. */
        const val WINDOW_END_SLACK_MS = 2_000L

        /** A code ending this close to its expiry ran out. */
        const val CODE_END_SLACK_MS = 1_000L
        const val PAIRING_WINDOW_MINUTES = 5u
        const val UNPAIRED_NOTICE_INTERVAL_MS = 60_000L
        const val NOT_AUTHORIZED = "not_authorized"
        const val SAVE_NOTE = "Saved files can be opened by other apps that can see your Downloads."
        const val MAX_EARLY_OPERATIONS = 16
        const val MAX_EARLY_EVENTS = 64

        /** The most offered files the core returns per offerFiles call. */
        const val OFFER_PAGE = 1_000

        /**
         * The operation (send, pair or pairing window) an event reports on;
         * null for incoming transfers and for events about no operation.
         * Prompts are never held back: a code prompt shows at once.
         */
        val NodeEvent.operationId: ULong?
            get() = when (this) {
                is NodeEvent.Connected -> operation
                is NodeEvent.SessionCode -> operation
                is NodeEvent.OfferSent -> operation
                is NodeEvent.TransferAccepted -> operation
                is NodeEvent.TransferProgress -> operation
                is NodeEvent.FileSent -> operation
                is NodeEvent.TransferFinished -> operation
                is NodeEvent.PairingWindowOpened -> operation
                is NodeEvent.PairingFinished -> operation
                else -> null
            }

        fun compactFingerprint(value: String): String = value.uppercase().filter { it.isLetterOrDigit() }

        /** Devices that look like paired ones first, then by name. */
        fun sortedNearby(peers: List<NearbyPeer>): List<NearbyPeer> =
            peers.sortedWith(compareBy({ if (it.hintAlias != null) 0 else 1 }, { it.name }))
    }
}
