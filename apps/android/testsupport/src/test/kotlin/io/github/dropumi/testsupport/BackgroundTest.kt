// SPDX-License-Identifier: MPL-2.0
//
// Leaving the app (ADR-0013 D7): the process lifecycle stops accepting once,
// and the transfer service keeps running transfers going. The service
// starter is a fake that starts nothing (FakeServiceStarter); Android's own
// rules for starting it are tested on devices (TransferServiceTest). Virtual
// time; no sleeps or polls.

package io.github.dropumi.testsupport

import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.Banner
import io.github.dropumi.appstate.GrantSpec
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.MemorySettingsStore
import io.github.dropumi.appstate.OutgoingFile
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.ServiceKind
import io.github.dropumi.appstate.ServiceStart
import io.github.dropumi.appstate.TransferPhase
import io.github.dropumi.appstate.TransferServiceState
import io.github.dropumi.core.files.Documents
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.ForeignFile
import io.github.dropumi.ffi.ForeignReader
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.ReceiverStopReason
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TransferStatus
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class BackgroundTest {
    private val roots = mutableListOf<File>()
    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun cleanUp() {
        scopes.forEach { it.cancel() }
        roots.forEach { it.deleteRecursively() }
    }

    private class Setup(val model: AppModel, val core: FakeCoreClient, val services: FakeServiceStarter, val root: File)

    /** A scope on the test's scheduler, not a child of the test (the model's event collector never ends). */
    private fun TestScope.modelScope(): CoroutineScope =
        CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)).also(scopes::add)

    /** An onboarded model on a fake core and a fresh temporary folder (never a real configuration). */
    private fun TestScope.makeModel(
        services: FakeServiceStarter = FakeServiceStarter(),
        documents: Documents? = null,
        core: FakeCoreClient = FakeCoreClient(),
    ): Setup {
        val root = Files.createTempDirectory("dropumi-background").toFile().also(roots::add)
        val model = AppModel(
            core,
            MemorySettingsStore(sdkInt = 36).apply { onboarded = true },
            HistoryStore(File(root, "app/history.json")),
            modelScope(),
            StandardTestDispatcher(testScheduler),
            clock = { testScheduler.currentTime },
            documents = documents,
            services = services,
        )
        return Setup(model, core, services, root)
    }

    private fun FakeCoreClient.count(call: FakeCoreClient.Call) = calls.count { it == call }

    private fun FakeCoreClient.starts() = calls.count { it is FakeCoreClient.Call.StartReceiving }

    /** Sends one file the app may read by path; returns the send's operation id. */
    private suspend fun Setup.sendOneFile(name: String = "x.bin"): ULong {
        val file = File(root, name).also { it.writeBytes(byteArrayOf(1)) }
        model.addToDraft(listOf(OutgoingFile(name, 1u, OutgoingFile.Source.Path(file.path))))
        model.sendDraft(SendTarget.Nearby("i"), "Maya", allowUnverified = false)
        return requireNotNull(model.state.value.active.first().operation)
    }

    private fun finished(operation: ULong?, direction: Direction, transferId: String = "aa") =
        NodeEvent.TransferFinished(operation, direction, Fixtures.result(TransferStatus.COMPLETED, transferId))

    /** An incoming offer (with its prompt) from a device that is not paired. */
    private fun Setup.offerArrives(transferId: String = "aa", promptId: ULong = 1u) = model.apply(
        listOf(
            NodeEvent.OfferReceived(transferId, Fixtures.peer(), 1u, 10u),
            NodeEvent.PromptOpened(Fixtures.offer(id = promptId, transferId = transferId)),
        ),
    )

    /** What the UI does 600 ms after a prompt appears. */
    private fun AppModel.armCurrentPrompt() {
        val arming = state.value.arming
        promptElapsed(requireNotNull(arming.promptId), arming.epoch, PromptArming.ARM_AFTER_MS)
    }

    // The process lifecycle

    /**
     * ProcessLifecycleOwner's ON_STOP stops accepting once, through the
     * loader, and never stops receiving: running transfers finish.
     */
    @Test
    fun backgroundCallsStopAcceptingOnce() = runTest {
        val s = makeModel()
        val loader = AppLoader(modelScope(), StandardTestDispatcher(testScheduler)) { s.model }
        loader.load()
        advanceUntilIdle()
        assertNotNull(s.model.state.value.receiver)
        loader.processStopped()
        loader.processStopped()
        s.model.enterBackground()
        advanceUntilIdle()
        assertEquals(1, s.core.count(FakeCoreClient.Call.StopAccepting))
        assertEquals(0, s.core.count(FakeCoreClient.Call.StopReceiving))
        assertTrue(s.model.state.value.isInBackground)
    }

    /** ON_STOP while the core still opens: the model starts in the background and never receives. */
    @Test
    fun backgroundWhileLoadingNeverStartsReceiving() = runTest {
        val s = makeModel()
        val loader = AppLoader(modelScope(), StandardTestDispatcher(testScheduler)) { s.model }
        loader.load()
        loader.processStopped()
        advanceUntilIdle()
        assertEquals(0, s.core.starts())
        loader.processStarted()
        advanceUntilIdle()
        assertEquals(1, s.core.starts())
    }

    /**
     * Back in front before the stopping receiver has ended (a transfer kept
     * it running): the model does not change it (the core would refuse:
     * not receiving) and shows no error; once the core reports it stopped, a
     * new receiver starts. Regression test: becoming active used to call
     * setReceivePolicy on the stopping receiver and show "Receiving could not
     * start".
     */
    @Test
    fun backInFrontWaitsForTheStoppingReceiverThenReceivesAgain() = runTest {
        val s = makeModel()
        s.model.start()
        assertEquals(1, s.core.starts())
        s.model.enterBackground()
        advanceUntilIdle()
        assertNull("not accepting, as the core is now", s.model.state.value.receiver)
        assertTrue(s.model.state.value.receiverStopping)
        s.model.becomeActive()
        advanceUntilIdle()
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.SetReceivePolicy })
        assertEquals(1, s.core.starts())
        assertNull(s.model.state.value.banner)
        s.core.emit(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED))
        advanceUntilIdle()
        assertEquals(2, s.core.starts())
        assertNotNull(s.model.state.value.receiver)
    }

    /**
     * The core ends an idle receiver as soon as it stops accepting, and its
     * ReceiverStopped can come before the call returns. The model must not
     * mark the receiver as stopping after that, or it would wait for an end
     * that already came and never receive again.
     */
    @Test
    fun receiverThatEndsAtOnceIsStartedAgainInFront() = runTest {
        val s = makeModel()
        s.core.stopAcceptingEndsReceiver = true
        s.model.start()
        advanceUntilIdle()
        s.model.enterBackground()
        advanceUntilIdle()
        assertEquals(1, s.core.count(FakeCoreClient.Call.StopAccepting))
        assertFalse(s.model.state.value.receiverStopping)
        s.model.becomeActive()
        advanceUntilIdle()
        assertEquals(2, s.core.starts())
        assertNotNull(s.model.state.value.receiver)
    }

    /** The stopping receiver ends while the app is still away: nothing starts until it is in front. */
    @Test
    fun receiverThatEndsInTheBackgroundStartsOnlyInFront() = runTest {
        val s = makeModel()
        s.model.start()
        s.model.enterBackground()
        advanceUntilIdle()
        s.core.emit(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED))
        advanceUntilIdle()
        assertEquals(1, s.core.starts())
        assertNull(s.model.state.value.banner)
        s.model.becomeActive()
        advanceUntilIdle()
        assertEquals(2, s.core.starts())
    }

    // The transfer service

    /**
     * A refused start (an auto-accepted transfer that begins just after the
     * app left, API 31+) keeps the transfer: nothing is cancelled, it
     * finishes, and meanwhile its card says it may pause.
     */
    @Test
    fun transferServiceStartRefusedKeepsTransfer() = runTest {
        val s = makeModel(FakeServiceStarter(result = ServiceStart.REFUSED))
        s.model.start()
        val op = s.sendOneFile()
        assertEquals(1, s.services.starts.size)
        val state = s.model.state.value
        assertEquals(TransferServiceState.REFUSED, state.transferService)
        assertTrue(state.transfersMayPause)
        assertEquals(1, state.active.size)
        s.model.apply(listOf(NodeEvent.TransferAccepted(op, "aa")))
        advanceUntilIdle()
        assertFalse(requireNotNull(s.core.operation(op)).cancelled)
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.CancelIncoming })
        s.model.apply(listOf(finished(op, Direction.OUTGOING)))
        assertEquals(TransferServiceState.OFF, s.model.state.value.transferService)
        assertFalse(s.model.state.value.transfersMayPause)
        assertEquals(HistoryEntry.Outcome.COMPLETED, s.model.state.value.history.single().outcome)
    }

    /** Send starts the service with a read grant for every document, so the read survives the activity. */
    @Test
    fun sendStartsTransferServiceWithDocumentGrants() = runTest {
        val s = makeModel(documents = StreamDocuments())
        s.model.addDocuments(listOf("content://com.example.files/a.txt", "content://com.example.files/b.txt"))
        val file = File(s.root, "c.bin").also { it.writeBytes(byteArrayOf(1)) }
        s.model.addToDraft(listOf(OutgoingFile("c.bin", 1u, OutgoingFile.Source.Path(file.path))))
        s.model.sendDraft(SendTarget.Nearby("i"), "Maya", allowUnverified = false)
        val start = s.services.starts.single()
        assertEquals(ServiceKind.TRANSFER, start.kind)
        assertEquals(
            listOf(GrantSpec("content://com.example.files/a.txt", true), GrantSpec("content://com.example.files/b.txt", true)),
            start.grants,
        )
        assertEquals(TransferServiceState.RUNNING, s.model.state.value.transferService)
    }

    /** A send that already ended before send() returned (a refused connection) starts nothing. */
    @Test
    fun sendThatEndedAtOnceStartsNoService() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        s.core.emitBeforeReturning("send") { op -> listOf(finished(op, Direction.OUTGOING)) }
        s.sendOneFileThatMayEnd()
        advanceUntilIdle()
        assertTrue(s.model.state.value.active.isEmpty())
        assertTrue(s.services.starts.isEmpty())
        assertEquals(TransferServiceState.OFF, s.model.state.value.transferService)
    }

    private suspend fun Setup.sendOneFileThatMayEnd() {
        val file = File(root, "x.bin").also { it.writeBytes(byteArrayOf(1)) }
        model.addToDraft(listOf(OutgoingFile("x.bin", 1u, OutgoingFile.Source.Path(file.path))))
        model.sendDraft(SendTarget.Nearby("i"), "Maya", allowUnverified = false)
    }

    /** Accept (the user's tap, so in front) starts the service; Decline does not. */
    @Test
    fun acceptStartsTransferServiceAndDeclineDoesNot() = runTest {
        val s = makeModel()
        s.offerArrives(transferId = "aa", promptId = 1u)
        s.model.answer(requireNotNull(s.model.state.value.currentPrompt), false)
        assertTrue(s.services.starts.isEmpty())
        val t = makeModel()
        t.offerArrives(transferId = "bb", promptId = 2u)
        t.model.armCurrentPrompt()
        t.model.answer(requireNotNull(t.model.state.value.currentPrompt), true)
        assertEquals(listOf(FakeServiceStarter.Start(ServiceKind.TRANSFER, emptyList())), t.services.starts)
        assertEquals(TransferServiceState.RUNNING, t.model.state.value.transferService)
    }

    /**
     * An auto-accepted transfer (TransferAccepted with no answer) starts the
     * service only while the app is in front; in the background no start is
     * attempted at all.
     */
    @Test
    fun transferServiceStartsOnlyInFront() = runTest {
        val s = makeModel()
        s.model.start()
        s.model.enterBackground()
        advanceUntilIdle()
        s.model.apply(listOf(NodeEvent.OfferReceived("aa", Fixtures.peer(trusted = true), 1u, 10u), NodeEvent.TransferAccepted(null, "aa")))
        assertTrue(s.services.starts.isEmpty())
        assertEquals(TransferServiceState.OFF, s.model.state.value.transferService)
        s.model.becomeActive()
        advanceUntilIdle()
        assertEquals(1, s.services.starts.size)
        assertEquals(TransferServiceState.RUNNING, s.model.state.value.transferService)
    }

    /** OfferSent in front starts the service once; later events do not start it again. */
    @Test
    fun runningServiceIsNotStartedAgain() = runTest {
        val s = makeModel()
        val op = s.sendOneFile()
        s.model.apply(listOf(NodeEvent.OfferSent(op, "aa", Fixtures.peer(), 1u, 1u), NodeEvent.TransferAccepted(op, "aa")))
        s.model.keepTransfersRunning()
        assertEquals(1, s.services.starts.size)
    }

    /** The service runs while any transfer runs, and is told to stop when the last one ends. */
    @Test
    fun transferServiceStopsWhenTheLastTransferEnds() = runTest {
        val s = makeModel()
        val first = s.sendOneFile("a.bin")
        val second = s.sendOneFile("b.bin")
        assertEquals(TransferServiceState.RUNNING, s.model.state.value.transferService)
        s.model.apply(listOf(finished(first, Direction.OUTGOING)))
        assertEquals(TransferServiceState.RUNNING, s.model.state.value.transferService)
        s.model.apply(listOf(finished(second, Direction.OUTGOING, transferId = "bb")))
        assertEquals(TransferServiceState.OFF, s.model.state.value.transferService)
    }

    /** A refused start does not stop a service that already runs (the running one keeps the transfers). */
    @Test
    fun refusedStartKeepsARunningService() = runTest {
        val s = makeModel()
        s.sendOneFile("a.bin")
        s.services.result = ServiceStart.REFUSED
        s.sendOneFile("b.bin")
        assertEquals(2, s.services.starts.size)
        assertEquals(TransferServiceState.RUNNING, s.model.state.value.transferService)
        assertFalse(s.model.state.value.transfersMayPause)
    }

    /**
     * Leaving (MainActivity pausing) starts the service for transfers that
     * move, never for an offer that only waits for this user's answer.
     */
    @Test
    fun leavingStartsTheServiceOnlyForMovingTransfers() = runTest {
        val s = makeModel()
        s.offerArrives()
        s.model.keepTransfersRunning()
        assertTrue(s.services.starts.isEmpty())
        s.model.enterBackground()
        s.model.apply(listOf(NodeEvent.TransferAccepted(null, "aa")))
        assertTrue("no start attempt from the background", s.services.starts.isEmpty())
        s.model.keepTransfersRunning()
        assertEquals(1, s.services.starts.size)
    }

    /** Refused, then back in front: the start is tried again there, and the card's note goes. */
    @Test
    fun refusedServiceIsStartedAgainInFront() = runTest {
        val s = makeModel(FakeServiceStarter(result = ServiceStart.REFUSED))
        s.model.start()
        s.sendOneFile()
        assertTrue(s.model.state.value.transfersMayPause)
        s.model.enterBackground()
        advanceUntilIdle()
        s.services.result = ServiceStart.STARTED
        s.model.becomeActive()
        advanceUntilIdle()
        assertEquals(2, s.services.starts.size)
        assertEquals(TransferServiceState.RUNNING, s.model.state.value.transferService)
        assertFalse(s.model.state.value.transfersMayPause)
    }

    /** The service could not go to the foreground: the transfer goes on and its card says it may pause. */
    @Test
    fun serviceThatCouldNotStartLeavesTheTransferRunning() = runTest {
        val s = makeModel()
        val op = s.sendOneFile()
        s.model.transferServiceRefused()
        assertEquals(TransferServiceState.REFUSED, s.model.state.value.transferService)
        assertTrue(s.model.state.value.transfersMayPause)
        assertFalse(requireNotNull(s.core.operation(op)).cancelled)
    }

    /**
     * The system's time limit for the service ran out (API 35+). That only
     * happens in the background, so accepting has already stopped. Every
     * running send is cancelled, and the core is asked to cancel every
     * running receive; an offer still waiting for this user's answer stays.
     * The core cannot cancel a receive once its receiver stops accepting (it
     * drops the canceller; the fake does the same), so that receive goes on
     * without the service and its card says it may pause. No notice: the
     * user did not ask to cancel. Regression test: this used to run only in
     * front, against a fake that always cancelled.
     */
    @Test
    fun timeoutCancelsSendsAndAsksTheCoreToCancelReceives() = runTest {
        val s = makeModel()
        s.model.start()
        val op = s.sendOneFile()
        s.model.apply(
            listOf(
                NodeEvent.OfferReceived("bb", Fixtures.peer(trusted = true), 1u, 10u),
                NodeEvent.TransferAccepted(null, "bb"),
                NodeEvent.OfferReceived("cc", Fixtures.peer(), 1u, 10u),
            ),
        )
        s.model.enterBackground()
        advanceUntilIdle()
        assertEquals(1, s.core.count(FakeCoreClient.Call.StopAccepting))
        s.model.transfersTimedOut()
        advanceUntilIdle()
        assertTrue(requireNotNull(s.core.operation(op)).cancelled)
        assertEquals(listOf(FakeCoreClient.Call.CancelIncoming("bb")), s.core.calls.filterIsInstance<FakeCoreClient.Call.CancelIncoming>())
        val state = s.model.state.value
        assertEquals(TransferServiceState.REFUSED, state.transferService)
        assertEquals(TransferPhase.Transferring, state.active.single { it.transferId == "bb" }.phase)
        assertTrue(state.transfersMayPause)
        assertNull(state.banner)
        assertEquals(TransferPhase.WaitingForAnswer, state.active.single { it.transferId == "cc" }.phase)
    }

    // Cancelling a receive

    /** A receive that moves (an auto-accepted offer from a paired device). */
    private fun Setup.receiveMoves(transferId: String = "aa") = model.apply(
        listOf(NodeEvent.OfferReceived(transferId, Fixtures.peer(trusted = true), 1u, 10u), NodeEvent.TransferAccepted(null, transferId)),
    )

    /** While the receiver accepts, the core cancels the receive: no notice (its TransferFinished ends the card). */
    @Test
    fun cancelWhileReceivingShowsNoNotice() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        s.receiveMoves()
        s.model.cancel(s.model.state.value.active.single())
        advanceUntilIdle()
        assertEquals(listOf(FakeCoreClient.Call.CancelIncoming("aa")), s.core.calls.filterIsInstance<FakeCoreClient.Call.CancelIncoming>())
        assertNull(s.model.state.value.banner)
    }

    /**
     * Back in front with a receive that was running when the app left: the
     * receiver is still stopping, and the core can no longer cancel the
     * transfer (it drops the canceller; the fake does the same). Cancel says
     * so instead of doing nothing, and the transfer stays on screen.
     * Regression test: the core's false answer used to be ignored.
     */
    @Test
    fun cancelAfterLeavingSaysItCouldNotCancel() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        s.receiveMoves()
        s.model.enterBackground()
        advanceUntilIdle()
        s.model.becomeActive()
        advanceUntilIdle()
        assertTrue(s.model.state.value.receiverStopping)
        s.model.cancel(s.model.state.value.active.single())
        advanceUntilIdle()
        assertEquals(listOf(FakeCoreClient.Call.CancelIncoming("aa")), s.core.calls.filterIsInstance<FakeCoreClient.Call.CancelIncoming>())
        val banner = requireNotNull(s.model.state.value.banner)
        assertEquals(Banner.Style.WARNING, banner.style)
        assertEquals("Could not cancel", banner.title)
        assertEquals(TransferPhase.Transferring, s.model.state.value.active.single().phase)
    }

    /** The core said no because the receive had just ended: nothing to say. */
    @Test
    fun cancelOfAReceiveThatEndedMeanwhileSaysNothing() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        s.receiveMoves()
        s.model.enterBackground()
        advanceUntilIdle()
        s.model.cancel(s.model.state.value.active.single())
        // The receive ends before the core call runs.
        s.model.apply(listOf(finished(null, Direction.INCOMING)))
        advanceUntilIdle()
        assertEquals(1, s.core.calls.count { it is FakeCoreClient.Call.CancelIncoming })
        assertTrue(s.model.state.value.active.isEmpty())
        assertNull(s.model.state.value.banner)
    }

    // Notifications

    /**
     * The card that asks for notifications shows only after a transfer
     * starts while they cannot be seen, goes once they can, and stays away
     * after "Not now".
     */
    @Test
    fun notificationCardShowsAfterFirstTransferWhileHidden() = runTest {
        val s = makeModel(FakeServiceStarter(notificationsVisible = false))
        s.model.start()
        assertFalse(s.model.state.value.askNotifications)
        s.sendOneFile("a.bin")
        assertTrue(s.model.state.value.askNotifications)
        s.model.refreshNotifications()
        assertTrue(s.model.state.value.askNotifications)
        s.services.notificationsVisible = true
        s.model.refreshNotifications()
        assertFalse(s.model.state.value.askNotifications)

        val t = makeModel(FakeServiceStarter(notificationsVisible = false))
        t.sendOneFile("a.bin")
        t.model.dismissNotificationHint()
        assertFalse(t.model.state.value.askNotifications)
        t.sendOneFile("b.bin")
        assertFalse("not after Not now", t.model.state.value.askNotifications)
    }

    /** Visible notifications: no card. */
    @Test
    fun noNotificationCardWhenNotificationsShow() = runTest {
        val s = makeModel()
        s.sendOneFile()
        assertFalse(s.model.state.value.askNotifications)
    }

    /** content:// documents served as streams; any other scheme is refused. */
    private class StreamDocuments : Documents {
        override fun describe(uri: String): Documents.Described? =
            if (uri.startsWith("content://")) Documents.Described(uri.substringAfterLast('/'), 4u) else null

        override fun prepare(uri: String): Documents.Prepared = Documents.Prepared.Stream(
            SendItem.Stream(
                object : ForeignFile {
                    override fun displayName() = "shared-file"
                    override fun size(): ULong = 4u
                    override fun modified(): Long? = null
                    override fun openReader(): ForeignReader = throw UnsupportedOperationException()
                },
            ),
        )
    }
}
