// SPDX-License-Identifier: MPL-2.0
//
// Pairing as the app drives it (a port of the Apple app's PairingFlowTests,
// same names where they fit), against a fake core that pairs like the real
// one (scriptPairingLikeTheCore). Virtual time: runCurrent() lets the model
// apply what the core delivered, and advanceUntilIdle() or advanceTimeBy()
// stand in for the Apple tests' timer.fireAll(). A test that must act while
// a core call is under way runs the model's io work by hand
// (ManualDispatcher). No sleeps, no polls, no retries.
//
// Android differences: browsing is the screen's own (BrowseWhileVisible),
// so the model's screen calls do not browse; the code prompt is drawn over
// the screen and never removes it, so the iOS prompt-over-a-result tests
// are replaced by the tab switch (leavePairingScreen(hidden = true)) and
// the activity's recreation (pairingScreenRecreating).

package io.github.dropumi.testsupport

import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.Banner
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.MemorySettingsStore
import io.github.dropumi.appstate.PairingClassifier
import io.github.dropumi.appstate.PairingFlow
import io.github.dropumi.appstate.PairingOutcome
import io.github.dropumi.appstate.PairingSession
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.ReceiverStopReason
import io.github.dropumi.ffi.SendTarget
import java.io.File
import java.nio.file.Files
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PairingFlowTest {
    private val roots = mutableListOf<File>()
    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun cleanUp() {
        scopes.forEach { it.cancel() }
        roots.forEach { it.deleteRecursively() }
    }

    /** A clock the test sets by hand (the model's clock, in milliseconds). */
    private class TestClock(var now: Long = 1_000_000_000L) {
        fun advance(ms: Long) {
            now += ms
        }
    }

    /** Runs io work only when the test says so, so the test can act while a core call waits. */
    private class ManualDispatcher : CoroutineDispatcher() {
        private val queue = ArrayDeque<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            synchronized(queue) { queue.addLast(block) }
        }

        /** Runs the io work queued so far (and what it queues). */
        fun runPending() {
            while (true) {
                val next = synchronized(queue) { queue.removeFirstOrNull() } ?: return
                next.run()
            }
        }
    }

    private class Setup(val model: AppModel, val core: FakeCoreClient, val clock: TestClock, val io: ManualDispatcher?) {
        val state get() = model.state.value
        val flow get() = model.state.value.pairingFlow
        val phase get() = model.state.value.pairingFlow.phase
    }

    private fun TestScope.modelScope(): CoroutineScope =
        CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)).also(scopes::add)

    /**
     * A model on a fresh temporary folder (never a real configuration).
     * [likeTheCore] false keeps the fake's plain answers: a window that
     * never reports anything by itself. [manualIo]: the model's core calls
     * wait until [settle] runs them.
     */
    private fun TestScope.makeModel(likeTheCore: Boolean = true, manualIo: Boolean = false): Setup {
        val root = Files.createTempDirectory("dropumi-pairing").toFile().also(roots::add)
        val core = FakeCoreClient()
        if (likeTheCore) core.scriptPairingLikeTheCore()
        val clock = TestClock()
        val io = if (manualIo) ManualDispatcher() else null
        val model = AppModel(
            core = core,
            settings = MemorySettingsStore(sdkInt = 36),
            historyStore = HistoryStore(File(root, "app/history.json")),
            scope = modelScope(),
            io = io ?: StandardTestDispatcher(testScheduler),
            clock = { clock.now },
        )
        return Setup(model, core, clock, io)
    }

    /** Lets the model apply what the core delivered (and run its io work, when it is run by hand). */
    private fun TestScope.settle(s: Setup) {
        repeat(MAX_ROUNDS) {
            runCurrent()
            s.io?.runPending()
        }
        runCurrent()
    }

    private fun FakeCoreClient.starts() = calls.count { it is FakeCoreClient.Call.StartReceiving }

    private fun FakeCoreClient.windowOpens() = calls.count { it == FakeCoreClient.Call.OpenPairingWindow }

    private fun FakeCoreClient.requests() = calls.filterIsInstance<FakeCoreClient.Call.Pair>().map { it.target }

    private fun FakeCoreClient.cancels() = calls.filterIsInstance<FakeCoreClient.Call.Cancel>().map { it.operation }

    private val maya = SendTarget.Nearby("dropumi-00000001")
    private val mayaName = "Maya's phone"
    private val studio = SendTarget.Nearby("dropumi-00000002")

    private val Setup.isWaiting: Boolean get() = phase is PairingFlow.Phase.Waiting

    /** An onboarded model (Paired only unless told otherwise), receiving, with its events read. */
    private suspend fun TestScope.started(
        visibility: ReceiveVisibility = ReceiveVisibility.PAIRED_ONLY,
        likeTheCore: Boolean = true,
    ): Setup {
        val s = makeModel(likeTheCore)
        s.model.start()
        s.model.finishOnboarding(visibility)
        runCurrent()
        return s
    }

    /** A model on the pairing screen, ready to pair: its window listens. Returns the window's operation too. */
    private suspend fun TestScope.onThePairScreen(visibility: ReceiveVisibility = ReceiveVisibility.PAIRED_ONLY): Pair<Setup, ULong> {
        val s = started(visibility)
        s.model.enterPairingScreen()
        runCurrent()
        val window = requireNotNull(s.core.windowOperation)
        assertEquals(PairingFlow(PairingFlow.Phase.Waiting(s.clock.now + WINDOW_MS)), s.flow)
        return s to window
    }

    /** Taps Pair next to Maya's phone and returns the request once it is under way. */
    private fun TestScope.askMaya(s: Setup): ULong {
        s.model.startPairing(maya, mayaName)
        settle(s)
        assertEquals(PairingFlow.Phase.Asking(mayaName), s.phase)
        val request = requireNotNull(s.state.pairing).operation
        assertEquals(maya, s.core.requests().last())
        return request
    }

    // Receiving hold (the rest are in AppModelTest)

    /** Off chosen during a window is kept; the core resumes the receiver with Paired only, and the model then stops it. */
    @Test
    fun offChosenDuringAWindowStopsTheResumedReceiver() = runTest {
        val s = started()
        s.model.openPairingWindow()
        runCurrent()
        val window = requireNotNull(s.core.windowOperation)
        s.model.setVisibility(ReceiveVisibility.OFF)
        assertFalse("the core holds no running receiver to stop", FakeCoreClient.Call.StopReceiving in s.core.calls)
        s.core.endWindow(window, PairingResult.Cancelled)
        runCurrent()
        assertTrue("Off stops the resumed receiver", FakeCoreClient.Call.StopReceiving in s.core.calls)
        assertNull(s.state.receiver)
        assertEquals(1, s.core.starts())
        assertFalse(s.state.pairingWindowActive)
    }

    /** A resume that fails ends the hold and says so like any other stop; the bounded wait then does nothing. */
    @Test
    fun resumeThatFailsShowsReceivingStopped() = runTest {
        val s = started()
        s.model.openPairingWindow()
        runCurrent()
        val window = requireNotNull(s.core.windowOperation)
        s.core.endWindow(window, PairingResult.Cancelled, resume = false)
        runCurrent()
        s.core.emit(NodeEvent.ReceiverStopped(ReceiverStopReason.INTERNAL))
        runCurrent()
        assertFalse(s.state.pairingWindowActive)
        val banner = requireNotNull(s.state.banner)
        assertEquals("Receiving stopped", banner.title)
        assertEquals(Banner.Action.RETRY_RECEIVING, banner.action)
        advanceUntilIdle()
        assertEquals("nothing restarts by itself", 1, s.core.starts())
    }

    /** A window the core refuses ends the hold at once, says why on the pairing screen, and a change made then applies. */
    @Test
    fun refusedWindowEndsTheHoldAndSaysWhyInline() = runTest {
        val s = started()
        val refusal = DropumiException.Busy("a transfer is running")
        s.core.failures["openPairingWindow"] = refusal
        s.model.enterPairingScreen()
        runCurrent()
        assertFalse(s.state.pairingWindowActive)
        assertEquals(PairingFlow.Phase.Paused(PairingClassifier.classifyWindowError(refusal)), s.phase)
        assertNull("the screen says it, not a banner", s.state.banner)
        s.model.setVisibility(ReceiveVisibility.OFF)
        assertTrue(FakeCoreClient.Call.StopReceiving in s.core.calls)
    }

    // The pairing screen

    /** Showing the pairing screen makes this device ready to pair at once; receiving pauses without a banner. */
    @Test
    fun enteringThePairScreenOpensAWindow() = runTest {
        val (s, _) = onThePairScreen()
        assertEquals(1, s.core.windowOpens())
        assertEquals(PairingSession.Role.Window, s.state.pairing?.role)
        assertEquals(FakeCoreClient.DEFAULT_PORT, s.state.pairing?.port)
        assertTrue(s.state.pairingWindowActive)
        assertNull(s.state.banner)
    }

    /** Leaving closes this device's window; receiving resumes. */
    @Test
    fun leavingThePairScreenClosesTheWindow() = runTest {
        val (s, window) = onThePairScreen()
        s.model.leavePairingScreen()
        runCurrent()
        assertEquals(listOf(window), s.core.cancels())
        assertNull(s.core.windowOperation)
        assertEquals(PairingFlow(), s.flow)
        assertFalse(s.state.pairingWindowActive)
        assertNotNull(s.state.receiver)
        assertNull(s.state.banner)
    }

    /** Two pairing screens on show share one window; it closes when the last goes. */
    @Test
    fun twoVisibleScreensShareOneWindow() = runTest {
        val (s, window) = onThePairScreen()
        s.model.enterPairingScreen()
        runCurrent()
        assertEquals(1, s.core.windowOpens())
        s.model.leavePairingScreen()
        runCurrent()
        assertTrue(s.core.cancels().isEmpty())
        assertTrue(s.isWaiting)
        s.model.leavePairingScreen()
        runCurrent()
        assertEquals(listOf(window), s.core.cancels())
    }

    /** Leaving and coming straight back: the next window opens only once the one before has ended. */
    @Test
    fun quickLeaveAndReturnOpensTheNextWindowOnlyAfterTheLastEnds() = runTest {
        val (s, window) = onThePairScreen()
        s.model.leavePairingScreen()
        s.model.enterPairingScreen()
        assertEquals(PairingFlow.Phase.Opening, s.phase)
        runCurrent()
        val calls = s.core.calls
        val closed = calls.indexOf(FakeCoreClient.Call.Cancel(window))
        val reopened = calls.lastIndexOf(FakeCoreClient.Call.OpenPairingWindow)
        assertTrue(closed in 0 until reopened)
        assertEquals(2, s.core.windowOpens())
        val next = requireNotNull(s.core.windowOperation)
        assertNotEquals(window, next)
        assertEquals(next, s.state.pairing?.operation)
        assertTrue(s.isWaiting)
        assertNull(s.state.banner)
    }

    /**
     * Android: a rotation recreates the activity, and the screen goes and
     * comes back at once. This device stays ready to pair with the same
     * window; nothing is closed or opened.
     */
    @Test
    fun recreatingTheActivityKeepsTheSameWindow() = runTest {
        val (s, window) = onThePairScreen()
        val before = s.flow
        s.model.pairingScreenRecreating()
        runCurrent()
        s.model.enterPairingScreen()
        advanceUntilIdle()
        assertTrue(s.core.cancels().isEmpty())
        assertEquals(1, s.core.windowOpens())
        assertEquals(window, s.core.windowOperation)
        assertEquals(before, s.flow)
        assertTrue(s.state.pairingWindowActive)
    }

    /** Android: a screen gone for a recreation that never comes back counts as left (bounded). */
    @Test
    fun recreatedScreenThatNeverComesBackClosesTheWindow() = runTest {
        val (s, window) = onThePairScreen()
        s.model.pairingScreenRecreating()
        runCurrent()
        assertTrue("not before the wait is over", s.core.cancels().isEmpty())
        advanceUntilIdle()
        assertEquals(listOf(window), s.core.cancels())
        assertNull(s.core.windowOperation)
        assertFalse(s.state.pairingWindowActive)
    }

    /**
     * Android: another tab hides the screen. Its window closes, but a result
     * on it is still there when the tab comes back, and no window opens
     * behind it.
     */
    @Test
    fun anotherTabKeepsTheResultForTheReturn() = runTest {
        val (s, window) = onThePairScreen()
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED))
        runCurrent()
        assertEquals(PairingFlow.Phase.Paused(PairingOutcome.UsedUp), s.phase)
        s.model.leavePairingScreen(hidden = true)
        runCurrent()
        s.model.enterPairingScreen()
        advanceUntilIdle()
        assertEquals(PairingFlow.Phase.Paused(PairingOutcome.UsedUp), s.phase)
        assertEquals(1, s.core.windowOpens())
        s.model.startWaiting()
        runCurrent()
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /** Android: another tab while ready to pair closes the window; back on the tab, this device is ready again. */
    @Test
    fun anotherTabWhileReadyClosesTheWindowAndTheReturnReopensIt() = runTest {
        val (s, window) = onThePairScreen()
        s.model.leavePairingScreen(hidden = true)
        runCurrent()
        assertEquals(listOf(window), s.core.cancels())
        s.model.enterPairingScreen()
        runCurrent()
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /**
     * A request started just before the screen appeared (Nearby's "Pair with
     * this device"): the screen shows it, and a window opens only if it ends
     * before a code.
     */
    @Test
    fun requestStartedBeforeTheScreenAppearsOpensNoWindow() = runTest {
        val s = started()
        s.model.startPairing(maya, mayaName)
        s.model.enterPairingScreen()
        runCurrent()
        assertEquals(0, s.core.windowOpens())
        assertEquals(PairingFlow.Phase.Asking(mayaName), s.phase)
        val request = requireNotNull(s.state.pairing).operation
        s.core.emit(NodeEvent.PairingFinished(request, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED)))
        runCurrent()
        assertEquals(1, s.core.windowOpens())
        assertEquals(PairingOutcome.NotWaiting(mayaName), s.flow.note)
        assertTrue("not ready to pair: ${s.phase}", s.isWaiting)
    }

    /** The screen goes away during a code comparison: it goes on, and its outcome is shown when the screen is back. */
    @Test
    fun leavingDuringACodeComparisonKeepsThePairing() = runTest {
        val (s, window) = onThePairScreen()
        assertNotNull(s.core.attemptOnWindow())
        runCurrent()
        assertEquals(PairingFlow.Phase.Comparing, s.phase)
        s.model.leavePairingScreen()
        runCurrent()
        assertTrue(s.core.cancels().isEmpty())
        assertEquals(PairingFlow.Phase.Comparing, s.phase)
        val paired = Fixtures.peer(trusted = true, alias = mayaName)
        s.core.endWindow(window, PairingResult.Paired(paired))
        runCurrent()
        s.model.enterPairingScreen()
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.Paired(paired)), s.phase)
        assertEquals(1, s.core.windowOpens())
    }

    /** An outcome seen on the screen is not shown again: back on the screen later, this device is ready to pair again. */
    @Test
    fun returningAfterASeenOutcomeWaitsAgain() = runTest {
        val (s, window) = onThePairScreen()
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED))
        runCurrent()
        assertEquals(PairingFlow.Phase.Paused(PairingOutcome.UsedUp), s.phase)
        s.model.leavePairingScreen()
        assertEquals(PairingFlow(), s.flow)
        s.model.enterPairingScreen()
        runCurrent()
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /** Stop ends being ready without a card or a banner, and nothing opens the window again. */
    @Test
    fun stopEndsBeingReadyQuietly() = runTest {
        val (s, window) = onThePairScreen()
        s.model.stopPairing()
        // The screen then goes away.
        s.model.leavePairingScreen()
        runCurrent()
        assertEquals(listOf(window), s.core.cancels())
        assertEquals(PairingFlow(), s.flow)
        assertNull(s.state.banner)
        assertFalse(s.state.pairingWindowActive)
        advanceUntilIdle()
        assertEquals(1, s.core.windowOpens())
    }

    // Taps

    /** Tapping a device closes this device's window first, then asks it. */
    @Test
    fun tappingADeviceClosesThisDevicesWindowBeforeAsking() = runTest {
        val (s, window) = onThePairScreen()
        s.model.startPairing(maya, mayaName)
        assertEquals(PairingFlow.Phase.Switching(mayaName), s.phase)
        runCurrent()
        val calls = s.core.calls
        val closed = calls.indexOf(FakeCoreClient.Call.Cancel(window))
        val asked = calls.indexOf(FakeCoreClient.Call.Pair(maya))
        assertTrue(closed in 0 until asked)
        assertFalse(s.core.pairCalledWhileWindowOpen)
        assertEquals(PairingFlow(PairingFlow.Phase.Asking(mayaName)), s.flow)
        assertEquals(1, s.core.windowOpens())
        assertNull(s.state.banner)
    }

    /** A second tap, on the same device or another, starts nothing more. */
    @Test
    fun doubleTapStartsOneRequest() = runTest {
        val (s, window) = onThePairScreen()
        s.model.startPairing(maya, mayaName)
        s.model.startPairing(maya, mayaName)
        runCurrent()
        s.model.startPairing(studio, "Studio Mac")
        runCurrent()
        assertEquals(listOf<SendTarget>(maya), s.core.requests())
        assertEquals(listOf(window), s.core.cancels())
        assertEquals(PairingFlow.Phase.Asking(mayaName), s.phase)
    }

    /** A tap while this device's window is still opening: the window closes as soon as the core has opened it, then the device is asked. */
    @Test
    fun tapWhileTheWindowIsOpeningCancelsItOnceRegistered() = runTest {
        val s = makeModel(manualIo = true)
        launch {
            s.model.start()
            s.model.finishOnboarding()
        }
        settle(s)
        assertNotNull(s.state.receiver)
        s.model.enterPairingScreen()
        // The window's core call is queued, not run.
        runCurrent()
        assertEquals(0, s.core.windowOpens())
        s.model.startPairing(maya, mayaName)
        assertEquals(PairingFlow.Phase.Switching(mayaName), s.phase)
        settle(s)
        assertEquals(1, s.core.windowOpens())
        assertEquals(1, s.core.cancels().size)
        assertNull(s.core.windowOperation)
        assertEquals(listOf<SendTarget>(maya), s.core.requests())
        assertFalse(s.core.pairCalledWhileWindowOpen)
        assertEquals(PairingFlow.Phase.Asking(mayaName), s.phase)
    }

    /**
     * Pair another device, then a tap while the next window still waits for
     * the receiver the last one paused: the window never opens, and the
     * device is asked. How the last window ended (paired) does not count for
     * this one.
     */
    @Test
    fun tapWhileTheNextWindowWaitsForTheResumeStillAsks() = runTest {
        val s = started()
        val paused = requireNotNull(s.state.receiver)
        s.model.enterPairingScreen()
        runCurrent()
        val window = requireNotNull(s.core.windowOperation)
        val paired = Fixtures.peer(trusted = true, alias = "Studio Mac")
        s.core.endWindow(window, PairingResult.Paired(paired), resume = false)
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.Paired(paired)), s.phase)
        s.model.startWaiting()
        s.model.startPairing(maya, mayaName)
        assertEquals(PairingFlow.Phase.Switching(mayaName), s.phase)
        runCurrent()
        assertTrue("the request waits for the window to be gone", s.core.requests().isEmpty())
        s.core.emit(NodeEvent.ReceiverStarted(paused))
        runCurrent()
        assertEquals(1, s.core.windowOpens())
        assertEquals(listOf<SendTarget>(maya), s.core.requests())
        assertEquals(PairingFlow.Phase.Asking(mayaName), s.phase)
    }

    /** A tap is ignored while a device is pairing with this one: that comparison goes on. */
    @Test
    fun tapIsIgnoredWhileAnotherDeviceIsPairingWithThisOne() = runTest {
        val (s, window) = onThePairScreen()
        s.core.emit(NodeEvent.Connected(window, Fixtures.peer(name = "Studio Mac")))
        runCurrent()
        assertEquals(PairingFlow.Phase.Comparing, s.phase)
        s.model.startPairing(maya, mayaName)
        runCurrent()
        assertTrue(s.core.requests().isEmpty())
        assertTrue(s.core.cancels().isEmpty())
        assertEquals(PairingFlow.Phase.Comparing, s.phase)
    }

    /** A window that never reports its end does not block the request: it goes ahead after a bounded wait, and not before. */
    @Test
    fun windowThatNeverReportsItsEndStillLetsTheRequestGo() = runTest {
        val s = started(likeTheCore = false)
        s.model.enterPairingScreen()
        runCurrent()
        val window = requireNotNull(s.state.pairing).operation
        s.core.emit(NodeEvent.PairingWindowOpened(window, FakeCoreClient.DEFAULT_PORT))
        runCurrent()
        s.model.startPairing(maya, mayaName)
        runCurrent()
        advanceTimeBy(WINDOW_CLOSE_WAIT_MS - 1)
        runCurrent()
        assertTrue("not before the wait is over", s.core.requests().isEmpty())
        advanceTimeBy(1)
        runCurrent()
        assertEquals(listOf(window), s.core.cancels())
        assertEquals(listOf<SendTarget>(maya), s.core.requests())
        assertEquals(PairingFlow.Phase.Asking(mayaName), s.phase)
    }

    /** Cancel while this device's window closes: the device is not asked, and this one is ready to pair again, with a note. */
    @Test
    fun cancelWhileSwitchingDoesNotDial() = runTest {
        val (s, _) = onThePairScreen()
        s.model.startPairing(maya, mayaName)
        s.model.cancelPairing()
        runCurrent()
        assertTrue(s.core.requests().isEmpty())
        assertEquals(PairingOutcome.Cancelled, s.flow.note)
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /** Cancel while asking: the request is cancelled, and this device is ready to pair again with "Pairing cancelled.". */
    @Test
    fun cancelWhileAskingGoesBackToReadyWithANote() = runTest {
        val (s, window) = onThePairScreen()
        val request = askMaya(s)
        s.model.cancelPairing()
        runCurrent()
        assertEquals(listOf(window, request), s.core.cancels())
        assertEquals(PairingOutcome.Cancelled, s.flow.note)
        assertEquals("Pairing cancelled.", s.flow.note?.noteText)
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /** The device tapped changed in the list (it may have just got ready to pair): a note, and this device is ready again. */
    @Test
    fun vanishedDeviceGetsANoteAndThisDeviceWaitsAgain() = runTest {
        val (s, _) = onThePairScreen()
        s.core.failures["pair"] = DropumiException.NotFound("that device is no longer nearby")
        s.model.startPairing(maya, mayaName)
        runCurrent()
        assertEquals(PairingOutcome.NoLongerNearby(mayaName), s.flow.note)
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
        assertNull(s.state.banner)
    }

    /** Without the pairing screen (the interop tests) a request that cannot start shows the banner as before. */
    @Test
    fun pairWithoutTheScreenKeepsTheBannerOnFailure() = runTest {
        val s = started()
        s.core.failures["pair"] = DropumiException.Busy("another operation is running")
        s.model.pair(maya, mayaName)
        assertEquals("Pairing could not start", s.state.banner?.title)
        assertEquals(PairingFlow(), s.flow)
        assertEquals(0, s.core.windowOpens())
    }

    /** Without the pairing screen, the request's result is kept on the session (the interop tests read it). */
    @Test
    fun pairWithoutTheScreenKeepsItsResult() = runTest {
        val s = started()
        s.model.pair(SendTarget.Address("192.168.1.20:47321", null), "CLI")
        val request = requireNotNull(s.state.pairing).operation
        val peer = Fixtures.peer(name = "CLI", trusted = true)
        s.core.emit(NodeEvent.PairingFinished(request, PairingResult.Paired(peer)))
        runCurrent()
        assertEquals(PairingResult.Paired(peer), s.state.pairing?.result)
        assertEquals(0, s.core.windowOpens())
    }

    // Outcomes

    /** The device asked was not waiting to pair: a note that says what to do (never "Only paired devices"), and this device is ready again. */
    @Test
    fun notWaitingDeviceGetsAClearNoteAndThisDeviceWaitsAgain() = runTest {
        val (s, _) = onThePairScreen()
        val request = askMaya(s)
        s.core.emit(NodeEvent.PairingFinished(request, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED)))
        runCurrent()
        val note = requireNotNull(s.flow.note)
        assertEquals(PairingOutcome.NotWaiting(mayaName), note)
        val text = requireNotNull(note.noteText)
        assertTrue(text.contains("isn't waiting to pair"))
        assertFalse(text.contains("Only paired devices"))
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /** A connection that is not a pairing request uses up the window: "Pairing didn't start", with Start again, and no reopening by itself. */
    @Test
    fun strayConnectionEndsWithStartAgainAndNoReopen() = runTest {
        val (s, window) = onThePairScreen()
        s.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED))
        runCurrent()
        assertEquals(PairingFlow(PairingFlow.Phase.Paused(PairingOutcome.UsedUp)), s.flow)
        assertNull(s.state.banner)
        advanceUntilIdle()
        assertEquals(1, s.core.windowOpens())
        s.model.startWaiting()
        runCurrent()
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /**
     * The screen left and came back while its window was closing, and a
     * stray connection used that window up before the close landed: the
     * result shows, and the window asked for on return never opens behind it.
     */
    @Test
    fun strayWhileTheNextWindowWaitsOpensNoWindowBehindTheResult() = runTest {
        val (s, window) = onThePairScreen()
        s.model.leavePairingScreen()
        s.model.enterPairingScreen()
        s.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED))
        runCurrent()
        advanceUntilIdle()
        assertEquals(PairingFlow(PairingFlow.Phase.Paused(PairingOutcome.UsedUp)), s.flow)
        assertEquals(1, s.core.windowOpens())
        assertNull(s.core.windowOperation)
        assertFalse(s.state.pairingWindowActive)
        s.model.startWaiting()
        runCurrent()
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
    }

    /** An attempt that reached a code never makes this device ready again by itself, on either side. */
    @Test
    fun attemptThatShowedACodeNeverReopensTheWindow() = runTest {
        val (s, window) = onThePairScreen()
        assertNotNull(s.core.attemptOnWindow())
        runCurrent()
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.CONNECTION_LOST))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.ConnectionLost(null)), s.phase)
        advanceUntilIdle()
        assertEquals(1, s.core.windowOpens())
        // Try again, then this device asks; a refusal after the code is a result too.
        s.model.startWaiting()
        runCurrent()
        val request = askMaya(s)
        s.core.emit(NodeEvent.SessionCode(request, Fixtures.peer(), "123456"))
        s.core.emit(NodeEvent.PairingFinished(request, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED)))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.Failed(ErrorCode.NOT_AUTHORIZED, mayaName)), s.phase)
        advanceUntilIdle()
        assertEquals(2, s.core.windowOpens())
    }

    /**
     * "You said the codes don't match" is for the device whose user said
     * no; the other device reads that its partner didn't confirm. The core
     * reports both as the same rejection.
     */
    @Test
    fun youDeclinedIsToldApartFromTheyDeclined() = runTest {
        // This device asked, and its user said no.
        val (asker, _) = onThePairScreen()
        val request = askMaya(asker)
        val prompt = Fixtures.code(id = 7u, operation = request)
        asker.core.emit(NodeEvent.PromptOpened(prompt))
        runCurrent()
        assertEquals(PairingFlow.Phase.Comparing, asker.phase)
        asker.model.answer(prompt, false)
        asker.core.emit(NodeEvent.PairingFinished(request, PairingResult.Failed(ErrorCode.PAIRING_REJECTED)))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.YouDeclined), asker.phase)

        // The device asked: its user said the codes match.
        val (asked, window) = onThePairScreen()
        val theirs = requireNotNull(asked.core.attemptOnWindow())
        runCurrent()
        armCodePrompt(asked.model, theirs)
        asked.model.answer(theirs, true)
        asked.core.endWindow(window, PairingResult.Failed(ErrorCode.PAIRING_REJECTED))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.OtherDeclined(null)), asked.phase)
        assertEquals("The other device didn't confirm the code", PairingOutcome.OtherDeclined(null).text.title)
    }

    /** What the UI does 600 ms after the code prompt appears: only then does a Codes match count. */
    private fun armCodePrompt(model: AppModel, prompt: Prompt) {
        val arming = model.state.value.arming
        assertEquals((prompt as Prompt.Code).id, arming.promptId)
        model.promptElapsed(prompt.id, arming.epoch, PromptArming.ARM_AFTER_MS)
    }

    /** The result can arrive before the answer's call returns; the answer still counts. */
    @Test
    fun answerCountsEvenIfTheResultArrivesFirst() = runTest {
        val s = makeModel(manualIo = true)
        launch {
            s.model.start()
            s.model.finishOnboarding()
        }
        settle(s)
        s.model.enterPairingScreen()
        settle(s)
        val window = requireNotNull(s.core.windowOperation)
        val prompt = requireNotNull(s.core.attemptOnWindow())
        settle(s)
        launch { s.model.answer(prompt, false) }
        // The answer's core call is queued, not run.
        runCurrent()
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.PAIRING_REJECTED))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.YouDeclined), s.phase)
        settle(s)
        assertEquals(FakeCoreClient.Call.AnswerCode(prompt.promptIdOf(), false), s.core.calls.last { it is FakeCoreClient.Call.AnswerCode })
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.YouDeclined), s.phase)
        assertNull(s.state.banner)
    }

    private fun Prompt.promptIdOf(): ULong = (this as Prompt.Code).id

    /** Nobody confirmed the code before it ran out here: "The code expired". */
    @Test
    fun expiredCodeIsCodeExpired() = runTest {
        val (s, _) = onThePairScreen()
        val request = askMaya(s)
        val prompt = Fixtures.code(id = 7u, operation = request, expiresAtMs = (s.clock.now + 60_000).toULong())
        s.core.emit(NodeEvent.PromptOpened(prompt))
        runCurrent()
        s.clock.advance(60_000)
        s.core.emit(NodeEvent.PairingFinished(request, PairingResult.Failed(ErrorCode.PAIRING_REJECTED)))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.CodeExpired), s.phase)
    }

    /** The five minutes ran out with nobody asking: "Time's up", with Start again; the window does not open again by itself. */
    @Test
    fun windowExpiryOffersStartAgainAndDoesNotReopen() = runTest {
        val (s, window) = onThePairScreen()
        s.clock.advance(WINDOW_MS)
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.TIMEOUT))
        runCurrent()
        assertEquals(PairingFlow.Phase.Paused(PairingOutcome.TimeUp), s.phase)
        assertNull(s.state.banner)
        advanceUntilIdle()
        assertEquals(1, s.core.windowOpens())
        assertFalse(s.state.pairingWindowActive)
    }

    /**
     * Events of a request reported before its call returned are kept and
     * applied once it has: here the code was shown, so a rejection is a
     * result, not a note.
     */
    @Test
    fun pairingResultBeforeTheCallReturnsIsKept() = runTest {
        val (s, window) = onThePairScreen()
        val peer = Fixtures.peer()
        s.core.emitBeforeReturning("pair") { op ->
            listOf(
                NodeEvent.Connected(op, peer),
                NodeEvent.SessionCode(op, peer, "123456"),
                NodeEvent.PairingFinished(op, PairingResult.Failed(ErrorCode.PAIRING_REJECTED)),
            )
        }
        s.model.startPairing(maya, mayaName)
        runCurrent()
        val request = requireNotNull(s.state.pairing).operation
        assertNotEquals(window, request)
        assertEquals(PairingResult.Failed(ErrorCode.PAIRING_REJECTED), s.state.pairing?.result)
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.OtherDeclined(mayaName)), s.phase)
        assertEquals(1, s.core.windowOpens())
    }

    /**
     * A code prompt opened before the request's call returned counts for an
     * end kept meanwhile: that end is a result, and this device does not get
     * ready again by itself.
     */
    @Test
    fun codePromptBeforeTheCallReturnsCountsForAKeptEnd() = runTest {
        val (s, _) = onThePairScreen()
        s.core.emitBeforeReturning("pair") { op ->
            listOf(
                NodeEvent.PromptOpened(Fixtures.code(id = 7u, operation = op)),
                NodeEvent.PairingFinished(op, PairingResult.Failed(ErrorCode.CONNECTION_LOST)),
            )
        }
        s.model.startPairing(maya, mayaName)
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.ConnectionLost(mayaName)), s.phase)
        advanceUntilIdle()
        assertEquals(1, s.core.windowOpens())
    }

    /** The window's port reported before its call returned is kept. */
    @Test
    fun windowPortBeforeTheCallReturnsIsKept() = runTest {
        val s = started(likeTheCore = false)
        s.core.emitBeforeReturning("openPairingWindow") { op -> listOf(NodeEvent.PairingWindowOpened(op, 47_000u)) }
        s.model.enterPairingScreen()
        runCurrent()
        val window = requireNotNull(s.state.pairing).operation
        assertEquals(PairingSession.Role.Window, s.state.pairing?.role)
        assertEquals(47_000.toUShort(), s.state.pairing?.port)
        assertNotNull(window)
        assertEquals(PairingFlow.Phase.Waiting(s.clock.now + WINDOW_MS), s.phase)
    }

    // Background, notices and security

    /** Going to the background closes a window nobody is pairing through; back in front on the pairing screen, this device is ready again. */
    @Test
    fun backgroundClosesTheWindowAndReturningReopensIt() = runTest {
        val (s, window) = onThePairScreen()
        // As the core does when no transfer runs: the receiver ends once it stops accepting.
        s.core.stopAcceptingEndsReceiver = true
        s.model.enterBackground()
        runCurrent()
        assertEquals(listOf(window), s.core.cancels())
        assertNull(s.core.windowOperation)
        assertEquals(1, s.core.windowOpens())
        s.model.becomeActive()
        runCurrent()
        assertEquals(2, s.core.windowOpens())
        assertTrue(s.isWaiting)
        assertNull(s.state.banner)
    }

    /** A code comparison under way is kept in the background. */
    @Test
    fun backgroundDuringACodeComparisonKeepsIt() = runTest {
        val (s, _) = onThePairScreen()
        assertNotNull(s.core.attemptOnWindow())
        runCurrent()
        s.model.enterBackground()
        runCurrent()
        assertTrue(s.core.cancels().isEmpty())
        assertEquals(PairingFlow.Phase.Comparing, s.phase)
        s.model.becomeActive()
        runCurrent()
        assertEquals(1, s.core.windowOpens())
        assertEquals(PairingFlow.Phase.Comparing, s.phase)
    }

    /** A device that is not pairing uses the window up before the core has the background's close: back in front, "Pairing didn't start" stays. */
    @Test
    fun strayBeforeTheBackgroundCloseStillNeedsStartAgain() = runTest {
        val (s, window) = onThePairScreen()
        s.model.enterBackground()
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED))
        runCurrent()
        assertEquals(PairingFlow.Phase.Paused(PairingOutcome.UsedUp), s.phase)
        s.model.becomeActive()
        advanceUntilIdle()
        assertEquals(PairingFlow.Phase.Paused(PairingOutcome.UsedUp), s.phase)
        assertEquals(1, s.core.windowOpens())
    }

    /** A device gets as far as the code before the core has the background's close: back in front, its result stays. */
    @Test
    fun codeBeforeTheBackgroundCloseKeepsTheResult() = runTest {
        val (s, window) = onThePairScreen()
        s.model.enterBackground()
        assertNotNull(s.core.attemptOnWindow())
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.CONNECTION_LOST))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.ConnectionLost(null)), s.phase)
        s.model.becomeActive()
        advanceUntilIdle()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.ConnectionLost(null)), s.phase)
        assertEquals(1, s.core.windowOpens())
    }

    /** Back in front with the pairing screen gone: no window opens. */
    @Test
    fun returningWithTheScreenGoneOpensNoWindow() = runTest {
        val (s, _) = onThePairScreen()
        s.model.enterBackground()
        s.model.leavePairingScreen()
        runCurrent()
        s.model.becomeActive()
        runCurrent()
        assertEquals(1, s.core.windowOpens())
        assertEquals(PairingFlow(), s.flow)
    }

    /** While this device waits to pair, a refused connection is told on the pairing screen, not in a banner; away from it, the banner is back and offers Pair a device. */
    @Test
    fun refusalIsNotShownWhileThisDeviceWaitsToPair() = runTest {
        val (s, _) = onThePairScreen()
        s.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
        runCurrent()
        assertNull(s.state.banner)
        s.model.leavePairingScreen()
        runCurrent()
        s.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
        runCurrent()
        assertEquals("A device was refused", s.state.banner?.title)
        assertEquals(Banner.Action.OPEN_PAIRING, s.state.banner?.action)
    }

    /** The screen gone but this device's window still opening: no banner until the window has ended. */
    @Test
    fun refusalIsNotShownWhileThisDevicesWindowIsOpening() = runTest {
        val s = makeModel(manualIo = true)
        launch {
            s.model.start()
            s.model.finishOnboarding()
        }
        settle(s)
        s.model.enterPairingScreen()
        // The window's core call is queued, not run.
        runCurrent()
        s.model.leavePairingScreen()
        s.model.apply(listOf(NodeEvent.SessionClosed(null, "not_authorized")))
        assertNull(s.state.banner)
        settle(s)
        assertNull(s.core.windowOperation)
        s.model.apply(listOf(NodeEvent.SessionClosed(null, "not_authorized")))
        assertEquals(Banner.Action.OPEN_PAIRING, s.state.banner?.action)
    }

    /** The screen gone but a device still comparing a code through this device's window: no banner while the window is open. */
    @Test
    fun refusalIsNotShownWhileThisDevicesWindowIsOpen() = runTest {
        val (s, window) = onThePairScreen()
        assertNotNull(s.core.attemptOnWindow())
        runCurrent()
        s.model.leavePairingScreen()
        runCurrent()
        s.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
        runCurrent()
        assertNull(s.state.banner)
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.PAIRING_REJECTED))
        runCurrent()
        s.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
        runCurrent()
        assertEquals(Banner.Action.OPEN_PAIRING, s.state.banner?.action)
    }

    /** Reset right after leaving the pairing screen: the core refuses it while the window is open, so the reset waits for the window to end. */
    @Test
    fun resetIdentityRightAfterLeavingWaitsForTheWindowToClose() = runTest {
        val (s, window) = onThePairScreen(ReceiveVisibility.OFF)
        s.model.leavePairingScreen()
        s.model.resetIdentity()
        assertEquals(FakeCoreClient.RESET_FINGERPRINT, s.state.identity?.fingerprint)
        assertEquals("New identity", s.state.banner?.title)
        val calls = s.core.calls
        val closed = calls.indexOf(FakeCoreClient.Call.Cancel(window))
        val reset = calls.indexOf(FakeCoreClient.Call.ResetIdentity)
        assertTrue(closed in 0 until reset)
    }

    /** During a request the identity and the name stay: the user finishes or stops pairing first. */
    @Test
    fun resetIdentityDuringARequestSaysFinishPairingFirst() = runTest {
        val (s, _) = onThePairScreen()
        askMaya(s)
        s.model.resetIdentity()
        assertEquals("The identity was not reset", s.state.banner?.title)
        assertEquals("Pairing is under way. Finish or stop it first.", s.state.banner?.message)
        assertFalse(FakeCoreClient.Call.ResetIdentity in s.core.calls)
        assertEquals(FakeCoreClient.FINGERPRINT, s.state.identity?.fingerprint)
        s.model.renameDevice("Kitchen tablet")
        assertEquals("The name was not changed", s.state.banner?.title)
        assertEquals("Pairing is under way. Finish or stop it first.", s.state.banner?.message)
        assertEquals("Dropumi-1A2B", s.state.identity?.name)
    }

    /** Whatever the core reports and the screen does, the model never answers a code or stores trust by itself. */
    @Test
    fun theModelNeverAnswersACodeOrTrustsByItself() = runTest {
        val (s, window) = onThePairScreen()
        assertNotNull(s.core.attemptOnWindow())
        runCurrent()
        s.model.leavePairingScreen()
        s.model.pairingScreenRecreating()
        s.model.enterPairingScreen()
        s.model.enterBackground()
        s.model.becomeActive()
        s.clock.advance(WINDOW_MS)
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.TIMEOUT))
        runCurrent()
        s.model.startWaiting()
        runCurrent()
        val request = askMaya(s)
        s.core.emit(NodeEvent.PromptOpened(Fixtures.code(id = 9u, operation = request)))
        s.core.emit(NodeEvent.PairingFinished(request, PairingResult.Paired(Fixtures.peer(trusted = true))))
        advanceUntilIdle()
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.AnswerCode })
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.Trust })
    }

    /** A device that connected is not shown as paired until the core says so, even if it claims a paired device's name. */
    @Test
    fun aConnectedPeerIsNeverShownAsPairedBeforePairingFinished() = runTest {
        val (s, window) = onThePairScreen()
        assertNotNull(s.core.attemptOnWindow(peer = Fixtures.peer(name = "Studio Mac")))
        runCurrent()
        assertEquals(PairingFlow(PairingFlow.Phase.Comparing), s.flow)
        assertNull(s.state.pairing?.result)
        assertTrue(s.state.trusted.isEmpty())
        val paired = Fixtures.peer(name = "Studio Mac", trusted = true, alias = "Studio Mac")
        s.core.endWindow(window, PairingResult.Paired(paired))
        runCurrent()
        assertEquals(PairingFlow.Phase.Finished(PairingOutcome.Paired(paired)), s.phase)
    }

    /**
     * Only the pairing screen opens a window: its appearing, Start again, a
     * request of this device that ended before a code, and the app back in
     * front on it. Nothing else does.
     */
    @Test
    fun aWindowOpensOnlyFromThePairScreenOrStartAgain() = runTest {
        val s = started()
        s.model.enterBackground()
        s.model.becomeActive()
        s.model.setVisibility(ReceiveVisibility.EVERYONE)
        s.model.setVisibility(ReceiveVisibility.PAIRED_ONLY)
        s.model.startWaiting()
        s.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
        s.model.pair(maya, mayaName)
        val request = requireNotNull(s.state.pairing).operation
        s.core.emit(NodeEvent.PairingFinished(request, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED)))
        advanceUntilIdle()
        assertEquals(0, s.core.windowOpens())
        // The screen appears, and says what happened to the request.
        s.model.enterPairingScreen()
        runCurrent()
        assertEquals(1, s.core.windowOpens())
        assertEquals(PairingOutcome.NotWaiting(mayaName), s.flow.note)
        // A stray uses the window up: only Start again opens the next.
        val window = requireNotNull(s.core.windowOperation)
        s.core.endWindow(window, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED))
        runCurrent()
        s.model.becomeActive()
        advanceUntilIdle()
        assertEquals(1, s.core.windowOpens())
        s.model.startWaiting()
        runCurrent()
        assertEquals(2, s.core.windowOpens())
    }

    private companion object {
        /** Bound on the io and model rounds a test runs by hand. */
        const val MAX_ROUNDS = 16

        /** The window's five minutes. */
        const val WINDOW_MS = 300_000L

        /** AppModel's wait for its window to report its end after a cancel. */
        const val WINDOW_CLOSE_WAIT_MS = 12_000L
    }
}
