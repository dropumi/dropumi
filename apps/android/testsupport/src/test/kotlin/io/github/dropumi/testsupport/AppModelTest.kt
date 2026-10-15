// SPDX-License-Identifier: MPL-2.0
//
// The Apple app's AppModelTests, ported one to one (same names), plus the
// Android prompt rules (arming, Back). Virtual time; no sleeps or polls.

package io.github.dropumi.testsupport

import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.ErrorCatalog
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.MemorySettingsStore
import io.github.dropumi.appstate.OutgoingFile
import io.github.dropumi.appstate.PendingShare
import io.github.dropumi.appstate.Route
import io.github.dropumi.appstate.PeerLabel
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.TransferPhase
import io.github.dropumi.appstate.promptId
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.core.discovery.Advertising
import io.github.dropumi.core.discovery.HostLabelGate
import io.github.dropumi.core.discovery.NsdAdapterCore
import io.github.dropumi.core.discovery.NsdPort
import io.github.dropumi.core.discovery.NsdService
import io.github.dropumi.core.files.Documents
import io.github.dropumi.core.files.ExportResult
import io.github.dropumi.core.files.ReceivedExporter
import io.github.dropumi.core.files.ReceivedFile
import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.ConfigProblem
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.FileStatus
import io.github.dropumi.ffi.ForeignFile
import io.github.dropumi.ffi.ForeignReader
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.PromptCloseReason
import io.github.dropumi.ffi.ReceiveOptions
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.ReceiverInfo
import io.github.dropumi.ffi.ReceiverStopReason
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.ServiceAd
import io.github.dropumi.ffi.TransferStatus
import java.io.File
import java.nio.file.Files
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
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
class AppModelTest {
    private val roots = mutableListOf<File>()
    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun cleanUp() {
        scopes.forEach { it.cancel() }
        roots.forEach { it.deleteRecursively() }
    }

    /**
     * The model's own scope on the test's scheduler. Not a child of the test,
     * so the model's endless event collector does not keep the test open, and
     * not the background scope, so advanceUntilIdle() runs the model's work.
     */
    private fun TestScope.modelScope(): CoroutineScope =
        CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)).also(scopes::add)

    private class Setup(val model: AppModel, val core: FakeCoreClient, val root: File) {
        val historyFile = File(root, "app/history.json")
        val outgoing = File(root, "outgoing").also { it.mkdirs() }
    }

    /**
     * A model on a fake core and a fresh temporary folder (never a real
     * configuration). [io] is the test's own scheduler unless a test needs a
     * fake call to block on a real thread (CallGate).
     */
    private fun TestScope.makeModel(io: CoroutineDispatcher = StandardTestDispatcher(testScheduler)): Setup {
        val root = Files.createTempDirectory("dropumi-appmodel").toFile().also(roots::add)
        val core = FakeCoreClient()
        val model = AppModel(
            core = core,
            settings = MemorySettingsStore(sdkInt = 36),
            historyStore = HistoryStore(File(root, "app/history.json")),
            scope = modelScope(),
            io = io,
            clock = { testScheduler.currentTime },
        )
        return Setup(model, core, root)
    }

    /** What the UI does 600 ms after a prompt appears. */
    private fun AppModel.armCurrentPrompt() {
        val arming = state.value.arming
        promptElapsed(requireNotNull(arming.promptId), arming.epoch, PromptArming.ARM_AFTER_MS)
    }

    private fun FakeCoreClient.starts() = calls.filterIsInstance<FakeCoreClient.Call.StartReceiving>()

    private companion object {
        /** Bound on the io and model rounds a test runs by hand. */
        const val MAX_ROUNDS = 10
    }

    @Test
    fun unverifiedOfferNeverAnsweredWithoutUserAction() = runTest {
        val s = makeModel()
        s.model.apply(
            listOf(
                NodeEvent.OfferReceived("aa", Fixtures.peer(), 1u, 10u),
                NodeEvent.PromptOpened(Fixtures.offer()),
            ),
        )
        advanceUntilIdle()
        assertEquals(1uL, s.model.state.value.currentPrompt?.promptId)
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.AnswerOffer })
    }

    @Test
    fun acceptingAnOfferNeverTrustsTheSender() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer())))
        s.model.armCurrentPrompt()
        s.model.answer(Fixtures.offer(), yes = true)
        assertEquals(listOf(FakeCoreClient.Call.AnswerOffer(1u, true)), s.core.calls)
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.Trust })
    }

    @Test
    fun promptGoesAwayOnlyWhenTheCoreClosesIt() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer())))
        s.model.answer(Fixtures.offer(), yes = false)
        assertEquals("still shown until PromptClosed", 1, s.model.state.value.prompts.size)
        s.model.apply(listOf(NodeEvent.PromptClosed(1u, PromptCloseReason.ANSWERED)))
        assertTrue(s.model.state.value.prompts.isEmpty())
    }

    @Test
    fun answerToAClosedPromptRemovesIt() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer())))
        s.model.armCurrentPrompt()
        s.core.failures["answerOffer"] = DropumiException.PromptNotFound()
        s.model.answer(Fixtures.offer(), yes = true)
        assertTrue(s.model.state.value.prompts.isEmpty())
        assertNull(s.model.state.value.banner)
    }

    @Test
    fun successOnlyAfterFinishedCompleted() = runTest {
        val s = makeModel()
        s.model.apply(
            listOf(
                NodeEvent.OfferReceived("aa", Fixtures.peer(trusted = true, alias = "Studio Mac"), 1u, 10u),
                NodeEvent.TransferAccepted(null, "aa"),
                NodeEvent.FileSaved("aa", 0u, "a.txt", 10u, "00"),
            ),
        )
        assertEquals("a saved file alone is not a finished transfer", 1, s.model.state.value.active.size)
        assertTrue(s.model.state.value.history.isEmpty())
        s.model.apply(listOf(NodeEvent.TransferFinished(null, Direction.INCOMING, Fixtures.result(TransferStatus.COMPLETED))))
        val state = s.model.state.value
        assertTrue(state.active.isEmpty())
        assertEquals(HistoryEntry.Outcome.COMPLETED, state.history.first().outcome)
        assertEquals("Studio Mac", state.history.first().peerName)
        assertEquals(listOf("a.txt"), state.history.first().savedAs)
    }

    @Test
    fun partialTransferIsNotASuccess() = runTest {
        val s = makeModel()
        s.model.apply(
            listOf(
                NodeEvent.OfferReceived("aa", Fixtures.peer(), 2u, 10u),
                NodeEvent.TransferFinished(
                    null,
                    Direction.INCOMING,
                    Fixtures.result(TransferStatus.COMPLETED_WITH_FAILURES),
                ),
            ),
        )
        assertEquals(HistoryEntry.Outcome.PARTIAL, s.model.state.value.history.first().outcome)
    }

    @Test
    fun everyErrorCodeHasCatalogText() {
        for (code in ErrorCode.entries) {
            val text = ErrorCatalog.text(code)
            assertTrue(code.name, text.title.isNotEmpty() && text.message.isNotEmpty())
        }
        assertTrue(ErrorCatalog.text(ErrorCode.PEER_IDENTITY_MISMATCH).isSecurity)
    }

    @Test
    fun unverifiedPeerIsNeverLabelledWithAPairedName() {
        // A peer that claims the name of a paired device stays unverified.
        val label = PeerLabel.of(Fixtures.peer(name = "Studio Mac", trusted = false))
        assertEquals("Unverified device", label.title)
        assertEquals("Studio Mac", label.claim)
        assertFalse(label.verified)
        val paired = PeerLabel.of(Fixtures.peer(name = "MacBook", trusted = true, alias = "Studio Mac"))
        assertEquals("Studio Mac", paired.title)
        assertEquals("MacBook", paired.claim)
        assertTrue(paired.verified)
    }

    @Test
    fun peerStringsAreKeptVerbatim() {
        val label = PeerLabel.of(Fixtures.peer(name = "**bold** [link](https://example.com) %s %n <b>x</b>"))
        assertEquals("**bold** [link](https://example.com) %s %n <b>x</b>", label.claim)
    }

    @Test
    fun fileCountsArePlural() {
        assertEquals("1 file", Formatting.fileCount(1))
        assertEquals("3 files", Formatting.fileCount(3))
    }

    @Test
    fun codesAreGroupedAndSpokenDigitByDigit() {
        assertEquals("123 456", Formatting.code("123456"))
        assertEquals("1 2 3, 4 5 6", Formatting.spokenCode("123456"))
    }

    /**
     * "This device" shows the whole 52-character fingerprint that "Add by
     * fingerprint" needs, grouped in fours; the grouping is only for reading.
     */
    @Test
    fun fullFingerprintIsGroupedInFoursAndRoundTrips() {
        val full = FakeCoreClient.FINGERPRINT
        assertEquals(52, full.length)
        val grouped = Formatting.fingerprint(full)
        val groups = grouped.split("-")
        assertEquals(13, groups.size)
        assertTrue(groups.all { it.length == 4 })
        // The add screen and the core drop spaces and dashes.
        assertEquals(full, grouped.filter { !it.isWhitespace() && it != '-' })
        // It starts with the short form shown elsewhere.
        assertTrue(grouped.startsWith("ABCD-EFGH-IJKL-MNOP-QRST-UVWX"))
    }

    // Changes saved but not confirmed or not read back

    @Test
    fun warningEventShowsABanner() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.Warning("the change was saved, but the configuration directory could not be synced")))
        val banner = requireNotNull(s.model.state.value.banner)
        assertEquals("Saved, but not confirmed on disk", banner.title)
        assertTrue(banner.message.startsWith("The change was saved"))
    }

    @Test
    fun warningIsAddedToTheBannerAlreadyShowing() = runTest {
        val s = makeModel()
        s.model.refreshIdentity()
        s.model.resetIdentity()
        assertEquals("New identity", s.model.state.value.banner?.title)
        s.model.apply(listOf(NodeEvent.Warning("the change was saved, but the configuration directory could not be synced")))
        val banner = requireNotNull(s.model.state.value.banner)
        assertEquals("New identity", banner.title)
        assertTrue(banner.message.contains("must pair again"))
        assertTrue(banner.message.contains("The change was saved"))
    }

    @Test
    fun resetSavedButNotReadBackIsReportedAsDone() = runTest {
        val s = makeModel()
        s.model.refreshIdentity()
        s.core.appliedButUnreadable += "resetIdentity"
        s.model.resetIdentity()
        assertEquals(FakeCoreClient.RESET_FINGERPRINT, s.model.state.value.identity?.fingerprint)
        assertEquals("New identity", s.model.state.value.banner?.title)
    }

    @Test
    fun resetThatFailsKeepsTheIdentityAndSaysSo() = runTest {
        val s = makeModel()
        s.model.refreshIdentity()
        s.core.failures["resetIdentity"] = DropumiException.Config(ConfigProblem.CORRUPT, "bad")
        s.model.resetIdentity()
        assertEquals(FakeCoreClient.FINGERPRINT, s.model.state.value.identity?.fingerprint)
        assertEquals("The identity was not reset", s.model.state.value.banner?.title)
    }

    @Test
    fun renameSavedButNotReadBackIsNotReportedAsFailed() = runTest {
        val s = makeModel()
        s.model.refreshIdentity()
        s.core.appliedButUnreadable += "setDeviceName"
        s.model.renameDevice("Kitchen tablet")
        assertEquals("Kitchen tablet", s.model.state.value.identity?.name)
        assertNotEquals("The name was not changed", s.model.state.value.banner?.title)
    }

    @Test
    fun trustSavedButNotReadBackCountsAsAdded() = runTest {
        val s = makeModel()
        s.core.appliedButUnreadable += "trustFingerprint"
        val fp = Fixtures.PEER_FINGERPRINT
        assertTrue(s.model.trust(fp, "Maya"))
        assertTrue(s.model.state.value.trusted.any { it.fingerprint == fp })
        assertNotEquals("Not added", s.model.state.value.banner?.title)
    }

    @Test
    fun trustThatFailsIsNotAdded() = runTest {
        val s = makeModel()
        s.core.failures["trustFingerprint"] = DropumiException.Config(ConfigProblem.IO, "disk full")
        assertFalse(s.model.trust(Fixtures.PEER_FINGERPRINT, null))
        assertEquals("Not added", s.model.state.value.banner?.title)
    }

    @Test
    fun backgroundStopsAcceptingButDoesNotStopReceiving() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        assertNotNull(s.model.state.value.receiver)
        s.model.enterBackground()
        // The stop is queued behind receiver changes; this waits for it.
        s.model.applyVisibility()
        assertTrue(FakeCoreClient.Call.StopAccepting in s.core.calls)
        assertFalse(FakeCoreClient.Call.StopReceiving in s.core.calls)
    }

    /**
     * The app went to the background while the core was still opening (the
     * loader hands the model over before start()). It does not receive until
     * it is in front again.
     */
    @Test
    fun backgroundBeforeStartNeverStartsReceiving() = runTest {
        val s = makeModelOnboarded()
        s.model.enterBackground()
        s.model.start()
        assertTrue(s.core.starts().isEmpty())
        assertNull(s.model.state.value.receiver)
        s.model.becomeActive()
        assertEquals(1, s.core.starts().size)
        assertNotNull(s.model.state.value.receiver)
    }

    /** A model whose settings say onboarding is done (as at a later launch). */
    private fun TestScope.makeModelOnboarded(): Setup {
        val root = Files.createTempDirectory("dropumi-appmodel").toFile().also(roots::add)
        val core = FakeCoreClient()
        val settings = MemorySettingsStore(sdkInt = 36).apply { onboarded = true }
        val model = AppModel(
            core,
            settings,
            HistoryStore(File(root, "app/history.json")),
            modelScope(),
            StandardTestDispatcher(testScheduler),
            clock = { testScheduler.currentTime },
        )
        return Setup(model, core, root)
    }

    /** The app goes to the background while the receiver is still starting: once it is up, it stops accepting. */
    @Test
    fun backgroundWhileTheReceiverStartsStopsAcceptingOnceItIsUp() = runTest {
        val s = makeModel(io = Dispatchers.IO)
        val gate = s.core.holdNextStartReceiving()
        val onboarding = async { s.model.finishOnboarding() }
        gate.reached()
        s.model.enterBackground()
        gate.open()
        onboarding.await()
        assertEquals("the receiver started", 1, s.core.starts().size)
        // Receiver changes run one after another; this one runs last.
        s.model.applyVisibility()
        assertTrue(FakeCoreClient.Call.StopAccepting in s.core.calls)
        assertFalse(FakeCoreClient.Call.StopReceiving in s.core.calls)
        // Not accepting: the receiver is stopping until its transfers end.
        assertNull(s.model.state.value.receiver)
        assertTrue(s.model.state.value.receiverStopping)
    }

    @Test
    fun pairedOnlyIsTheDefaultAndAutoAcceptIsOff() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        // Not advertised until the NSD host-label gate opens (ADR-0013 D5).
        assertEquals(
            FakeCoreClient.Call.StartReceiving(ReceiveOptions(ReceivePolicy.TrustedOnly, AcceptMode.PROMPT, null, false)),
            s.core.calls.first(),
        )
        assertEquals(ReceiveVisibility.PAIRED_ONLY, s.model.state.value.visibility)
        assertFalse(s.model.state.value.autoAcceptPaired)
    }

    @Test
    fun everyoneRevertsWhenTheCoreEndsTheWindow() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        s.model.setVisibility(ReceiveVisibility.EVERYONE)
        s.model.apply(listOf(NodeEvent.PolicyChanged(ReceivePolicy.TrustedOnly, AcceptMode.PROMPT, null)))
        assertEquals(ReceiveVisibility.PAIRED_ONLY, s.model.state.value.visibility)
    }

    @Test
    fun overlappingRequestsStartTheReceiverOnce() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        val launch = async { s.model.becomeActive() }
        val active = async { s.model.becomeActive() }
        launch.await()
        active.await()
        assertEquals("one receiver; later requests only update its policy", 1, s.core.starts().size)
    }

    @Test
    fun receivingResumesWhenAStopFinishesLate() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        s.model.setVisibility(ReceiveVisibility.OFF)
        s.model.setVisibility(ReceiveVisibility.PAIRED_ONLY)
        assertTrue(
            "a stopping receiver is never reconfigured",
            s.core.calls.none { it is FakeCoreClient.Call.SetReceivePolicy },
        )
        s.model.apply(listOf(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED)))
        advanceUntilIdle()
        assertEquals(2, s.core.starts().size)
        assertNotNull(s.model.state.value.receiver)
    }

    @Test
    fun offStopsTheReceiver() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        s.model.setVisibility(ReceiveVisibility.OFF)
        assertTrue(FakeCoreClient.Call.StopReceiving in s.core.calls)
    }

    @Test
    fun sendingKeepsStagedCopiesUntilTheTransferEnds() = runTest {
        val s = makeModel()
        val folder = File(s.outgoing, "copy-1").also { it.mkdirs() }
        val copy = File(folder, "photo.heic").also { it.writeBytes(byteArrayOf(1, 2, 3)) }
        s.model.addToDraft(listOf(OutgoingFile("photo.heic", 3u, OutgoingFile.Source.Path(copy.path, folder.path))))
        s.model.sendDraft(SendTarget.Nearby("dropumi-00000001"), "Pixel 8", allowUnverified = false)
        advanceUntilIdle()
        assertTrue(s.model.state.value.draft.isEmpty())
        assertEquals(1, s.model.state.value.active.size)
        assertTrue("the core still reads it", copy.exists())
        val send = s.core.calls.last() as FakeCoreClient.Call.Send
        assertEquals(listOf(copy.path), send.paths)
        assertFalse(send.allowUnverified)
        s.model.apply(listOf(NodeEvent.TransferFinished(1u, Direction.OUTGOING, Fixtures.result(TransferStatus.COMPLETED))))
        advanceUntilIdle()
        assertFalse(copy.exists())
        assertFalse(folder.exists())
    }

    @Test
    fun cancellingAnOutgoingTransferCancelsItsOperation() = runTest {
        val s = makeModel()
        val file = File(s.outgoing, "x.bin").also { it.writeBytes(byteArrayOf(1)) }
        s.model.addToDraft(listOf(OutgoingFile("x.bin", 1u, OutgoingFile.Source.Path(file.path))))
        s.model.sendDraft(SendTarget.Nearby("i"), "X", allowUnverified = false)
        val record = s.model.state.value.active.first()
        s.model.cancel(record)
        advanceUntilIdle()
        assertEquals(true, s.core.operation(1u)?.cancelled)
    }

    @Test
    fun historyPersistsAndClears() = runTest {
        val s = makeModel()
        s.model.apply(
            listOf(
                NodeEvent.OfferReceived("aa", Fixtures.peer(), 1u, 10u),
                NodeEvent.TransferFinished(null, Direction.INCOMING, Fixtures.result(TransferStatus.COMPLETED)),
            ),
        )
        advanceUntilIdle()
        assertEquals(1, HistoryStore(s.historyFile).load().size)
        s.model.clearHistory()
        advanceUntilIdle()
        assertTrue(HistoryStore(s.historyFile).load().isEmpty())
    }

    @Test
    fun historyIsCapped() {
        val root = Files.createTempDirectory("dropumi-history").toFile().also(roots::add)
        val store = HistoryStore(File(root, "history.json"), limit = 3)
        val entries = (0 until 5).map {
            HistoryEntry("$it", true, "p", true, emptyList(), 0u, HistoryEntry.Outcome.COMPLETED, "", 0, emptyList())
        }
        assertTrue(store.save(entries))
        assertEquals(listOf("0", "1", "2"), store.load().map { it.id })
    }

    @Test
    fun unpairedSenderNoticeIsRateLimited() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.SessionClosed(Fixtures.peer(), "not_authorized")))
        val first = s.model.state.value.banner
        assertNotNull(first)
        s.model.apply(listOf(NodeEvent.SessionClosed(Fixtures.peer(), "not_authorized")))
        assertEquals(first, s.model.state.value.banner)
    }

    /**
     * The core reports a refusal during the handshake with no peer, whatever
     * the reason: a device that isn't paired, but also a pairing request
     * while no window is open (paired devices too), a paired device while a
     * window is open, or an unreadable list of paired devices. Found on the
     * Tablet AVD (Android 16) with the real core and the CLI as the sender.
     * The notice says only that a device was refused, and never offers
     * Everyone (that would point a paired device's user at the wrong fix).
     */
    @Test
    fun refusedConnectionWithoutAPeerIsNeutral() = runTest {
        val s = makeModel()
        // For example a paired phone that asks to pair while this device has no window open.
        s.model.apply(listOf(NodeEvent.SessionClosed(null, "not_authorized")))
        val banner = requireNotNull(s.model.state.value.banner)
        assertEquals("A device was refused", banner.title)
        assertNull("no Everyone action", banner.action)
        assertFalse(banner.message.contains("everyone", ignoreCase = true))
        assertFalse(banner.message.contains("isn't paired"))
        s.model.dismissBanner()
        // Other closed sessions say nothing.
        s.model.apply(listOf(NodeEvent.SessionClosed(null, "handshake_failed")))
        assertNull(s.model.state.value.banner)
    }

    /** A paired peer is never "not paired". */
    @Test
    fun refusedPairedPeerShowsNoNotice() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.SessionClosed(Fixtures.peer(trusted = true, alias = "Maya"), "not_authorized")))
        assertNull(s.model.state.value.banner)
    }

    /** Any host on the network can cause a refusal: it never takes the place of another notice. */
    @Test
    fun refusedNoticeNeverReplacesAnotherBanner() = runTest {
        val s = makeModel()
        s.model.notice("Not saved", "There is not enough free space.")
        s.model.apply(listOf(NodeEvent.SessionClosed(null, "not_authorized")))
        s.model.apply(listOf(NodeEvent.SessionClosed(Fixtures.peer(), "not_authorized")))
        assertEquals("Not saved", s.model.state.value.banner?.title)
    }

    @Test
    fun nearbyDevicesThatLookLikePairedOnesComeFirst() = runTest {
        val s = makeModel()
        s.model.apply(
            listOf(
                NodeEvent.PeerFound(Fixtures.nearby("Zed", "a")),
                NodeEvent.PeerFound(Fixtures.nearby("Amy", "b")),
                NodeEvent.PeerFound(Fixtures.nearby("Mac", "c", hintAlias = "Studio Mac")),
            ),
        )
        assertEquals(listOf("c", "b", "a"), s.model.state.value.nearby.map { it.instance })
        s.model.apply(listOf(NodeEvent.PeerLost("c")))
        assertEquals(listOf("b", "a"), s.model.state.value.nearby.map { it.instance })
    }

    // Android: prompt arming and Back (ADR-0013 D8)

    /** AppModel enforces the 600 ms arming too: an early Accept does nothing; Decline always works. */
    @Test
    fun acceptBeforeArmingIsIgnored() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer())))
        s.model.answer(Fixtures.offer(), yes = true)
        assertTrue(s.core.calls.isEmpty())
        // A timer from before a resume cannot arm it either.
        val stale = s.model.state.value.arming
        s.model.promptResumed()
        s.model.promptElapsed(stale.promptId!!, stale.epoch, PromptArming.ARM_AFTER_MS)
        s.model.answer(Fixtures.offer(), yes = true)
        assertTrue(s.core.calls.isEmpty())
        s.model.armCurrentPrompt()
        s.model.answer(Fixtures.offer(), yes = true)
        assertEquals(listOf(FakeCoreClient.Call.AnswerOffer(1u, true)), s.core.calls)
    }

    /** An Accept for a prompt that is not the one on screen is ignored. */
    @Test
    fun acceptForAPromptNotOnScreenIsIgnored() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer(id = 1u)), NodeEvent.PromptOpened(Fixtures.code(id = 2u))))
        s.model.armCurrentPrompt()
        s.model.answer(Fixtures.code(id = 2u), yes = true)
        assertTrue(s.core.calls.isEmpty())
        // Declining it is always allowed.
        s.model.answer(Fixtures.code(id = 2u), yes = false)
        assertEquals(listOf(FakeCoreClient.Call.AnswerCode(2u, false)), s.core.calls)
    }

    @Test
    fun backOnOfferDeclines() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer())))
        s.model.back()
        assertEquals(listOf(FakeCoreClient.Call.AnswerOffer(1u, false)), s.core.calls)
        assertEquals("only the core closes it", 1, s.model.state.value.prompts.size)
    }

    @Test
    fun backOnCodeDeclines() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.code())))
        s.model.back()
        assertEquals(listOf(FakeCoreClient.Call.AnswerCode(2u, false)), s.core.calls)
    }

    @Test
    fun backWhileAnsweringIsIgnored() = runTest {
        val s = makeModel(io = Dispatchers.IO)
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer())))
        s.model.armCurrentPrompt()
        val gate = s.core.holdNext("answerOffer")
        val accepting = async { s.model.answer(Fixtures.offer(), yes = true) }
        gate.reached()
        s.model.back()
        gate.open()
        accepting.await()
        assertEquals(listOf(FakeCoreClient.Call.AnswerOffer(1u, true)), s.core.calls)
    }

    @Test
    fun backWithoutAPromptDoesNothing() = runTest {
        val s = makeModel()
        s.model.back()
        assertTrue(s.core.calls.isEmpty())
    }

    /**
     * ADR-0013 D8: once an answer reached the core, the prompt shows "Waiting…"
     * until the core closes it; it takes no second answer and Back does nothing.
     */
    @Test
    fun answeredPromptWaitsUntilTheCoreClosesIt() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer())))
        s.model.answer(Fixtures.offer(), yes = false)
        assertEquals(setOf(1uL), s.model.state.value.answered)
        assertEquals("still shown until PromptClosed", 1, s.model.state.value.prompts.size)
        s.model.back()
        s.model.armCurrentPrompt()
        s.model.answer(Fixtures.offer(), yes = true)
        assertEquals(listOf(FakeCoreClient.Call.AnswerOffer(1u, false)), s.core.calls)
        s.model.apply(listOf(NodeEvent.PromptClosed(1u, PromptCloseReason.ANSWERED)))
        assertTrue(s.model.state.value.prompts.isEmpty())
        assertTrue(s.model.state.value.answered.isEmpty())
    }

    /** An answer the core did not take can be given again; the prompt is not marked answered. */
    @Test
    fun answerThatFailedCanBeGivenAgain() = runTest {
        val s = makeModel()
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.code())))
        s.core.failures["answerCode"] = DropumiException.Internal("test")
        s.model.answer(Fixtures.code(), yes = false)
        assertTrue(s.model.state.value.answered.isEmpty())
        assertNotNull(s.model.state.value.banner)
        s.model.back()
        assertEquals(listOf(FakeCoreClient.Call.AnswerCode(2u, false)), s.core.calls)
        assertEquals(setOf(2uL), s.model.state.value.answered)
    }

    /** Offers past the 200 files a prompt carries are read in pages of at most 1000 (the core's cap). */
    @Test
    fun offerFilesArePagedAtMostAThousandAtATime() = runTest {
        val s = makeModel()
        val inline = (0 until 200).map { OfferedFile("file-$it.bin", 1u) }
        s.model.apply(listOf(NodeEvent.PromptOpened(Fixtures.offer(files = inline, fileCount = 1500u))))
        val page = s.model.offerFiles(1u, offset = 200, limit = 5000)
        assertEquals(1000, page?.size)
        assertEquals("file-200.bin", page?.first()?.name)
        assertEquals(listOf(FakeCoreClient.Call.OfferFiles(1u, 200u, 1000u)), s.core.calls)
        assertEquals(emptyList<OfferedFile>(), s.model.offerFiles(1u, offset = 1200, limit = 0))
        assertEquals("nothing asked for nothing", 1, s.core.calls.size)
    }

    /** A page of a prompt the core no longer holds is null, never an error banner. */
    @Test
    fun offerFilesOfAClosedPromptAreNull() = runTest {
        val s = makeModel()
        s.core.failures["offerFiles"] = DropumiException.PromptNotFound()
        assertNull(s.model.offerFiles(9u, offset = 200, limit = 10))
        assertNull(s.model.state.value.banner)
    }

    /** ADR-0013 D3: the name onboarding suggests is only the core's own `Dropumi-XXXX`. */
    @Test
    fun defaultNameComesOnlyFromCore() = runTest {
        val s = makeModel()
        assertNull(s.model.state.value.suggestedDeviceName)
        s.model.refreshIdentity()
        assertEquals("Dropumi-1A2B", s.model.state.value.suggestedDeviceName)
    }

    // Android: the NSD host-label gate (ADR-0013 D5)

    /** Records registrations so a test can answer for NSD. */
    private class RegistrationPort : NsdPort {
        val registrations = mutableListOf<NsdPort.RegistrationListener>()
        override val hasServiceInfoCallback = false
        override fun register(service: NsdService, listener: NsdPort.RegistrationListener) {
            registrations += listener
        }
        override fun unregister(listener: NsdPort.RegistrationListener) = Unit
        override fun discover(type: String, listener: NsdPort.DiscoveryListener) = Unit
        override fun stopDiscovery(listener: NsdPort.DiscoveryListener) = Unit
        override fun resolve(service: NsdService, listener: NsdPort.ResolveListener) = Unit
        override fun registerInfoCallback(service: NsdService, callback: NsdPort.InfoCallback) = Unit
        override fun unregisterInfoCallback(callback: NsdPort.InfoCallback) = Unit
    }

    private fun TestScope.makeModel(advertising: Advertising): Setup {
        val root = Files.createTempDirectory("dropumi-appmodel").toFile().also(roots::add)
        val core = FakeCoreClient()
        val model = AppModel(
            core,
            MemorySettingsStore(sdkInt = 36),
            HistoryStore(File(root, "app/history.json")),
            modelScope(),
            StandardTestDispatcher(testScheduler),
            clock = { testScheduler.currentTime },
            advertising = advertising,
        )
        return Setup(model, core, root)
    }

    /** Where the gate is closed, the core is told not to advertise, and the UI says browse-only. */
    @Test
    fun gateClosedReceivesWithAdvertiseFalse() = runTest {
        val adapter = NsdAdapterCore(RegistrationPort(), HostLabelGate.OBSERVED, 36, null, { listOf("192.168.1.5") })
        val s = makeModel(adapter)
        s.model.start()
        s.model.finishOnboarding()
        val start = s.core.starts().single()
        assertFalse(start.options.advertise)
        assertEquals(AdvertiseState.GateClosed, s.model.state.value.advertising)
    }

    /** A rename after advertise() returned cannot reach the core; it reaches the model's state. */
    @Test
    fun nsdLateRegistrationFailureReachesAppModel() = runTest {
        val port = RegistrationPort()
        val gate = HostLabelGate(listOf(HostLabelGate.Observed(36, 100L..100L)))
        val adapter = NsdAdapterCore(port, gate, 36, 100L, { listOf("192.168.1.5") })
        val s = makeModel(adapter)
        s.model.start()
        s.model.finishOnboarding()
        assertTrue(s.core.starts().single().options.advertise)
        // What the core does inside startReceiving.
        adapter.advertise(ServiceAd("_dropumi._tcp", "dropumi-11111111", 47321u, emptyList()))
        adapter.awaitIdle()
        runCurrent()
        assertEquals(AdvertiseState.Registering, s.model.state.value.advertising)
        port.registrations.single().registered("dropumi-11111111 (2)")
        adapter.awaitIdle()
        runCurrent()
        assertEquals(AdvertiseState.Failed(AdvertiseState.Reason.RENAMED), s.model.state.value.advertising)
    }

    @Test
    fun connectionDetailsListEveryLanAddressWithThePort() = runTest {
        val root = Files.createTempDirectory("dropumi-appmodel").toFile().also(roots::add)
        val core = FakeCoreClient()
        val model = AppModel(
            core,
            MemorySettingsStore(sdkInt = 36),
            HistoryStore(File(root, "app/history.json")),
            modelScope(),
            StandardTestDispatcher(testScheduler),
            localAddresses = { listOf("192.168.1.5", "10.0.0.7") },
        )
        assertTrue("not receiving", model.connectionDetails().isEmpty())
        model.finishOnboarding()
        assertEquals(listOf("192.168.1.5:47321", "10.0.0.7:47321"), model.connectionDetails())
    }

    // Android: the share hand-off and content:// documents (ADR-0013 D6)

    /** Documents served by a test: `content://` ones are allowed, a `pipe` one is staged. */
    private class FakeDocuments(private val stagedFolder: File) : Documents {
        val prepared = mutableListOf<String>()

        override fun describe(uri: String): Documents.Described? =
            if (uri.startsWith("content://")) Documents.Described(uri.substringAfterLast('/'), 4u) else null

        override fun prepare(uri: String): Documents.Prepared {
            prepared += uri
            if (uri.endsWith("pipe")) {
                val copy = File(stagedFolder.also { it.mkdirs() }, "pipe").also { it.writeBytes(byteArrayOf(1)) }
                return Documents.Prepared.Staged(SendItem.Path(copy.path), stagedFolder)
            }
            return Documents.Prepared.Stream(
                SendItem.Stream(
                    object : ForeignFile {
                        override fun displayName() = "note.txt"
                        override fun size(): ULong = 4u
                        override fun modified(): Long? = null
                        override fun openReader(): ForeignReader = throw UnsupportedOperationException()
                    },
                ),
            )
        }
    }

    private fun TestScope.makeModel(documents: Documents, pendingShare: PendingShare): Setup {
        val root = Files.createTempDirectory("dropumi-appmodel").toFile().also(roots::add)
        val core = FakeCoreClient()
        val model = AppModel(
            core,
            MemorySettingsStore(sdkInt = 36),
            HistoryStore(File(root, "app/history.json")),
            modelScope(),
            StandardTestDispatcher(testScheduler),
            clock = { testScheduler.currentTime },
            documents = documents,
            pendingShare = pendingShare,
        )
        return Setup(model, core, root)
    }

    @Test
    fun sharedItemsArePickedUpAndOfferedForSending() = runTest {
        val pending = PendingShare()
        val s = makeModel(FakeDocuments(Files.createTempDirectory("staged").toFile().also(roots::add)), pending)
        pending.put(listOf("content://com.example.files/note.txt"))
        s.model.takeSharedItems()
        assertEquals(listOf("note.txt"), s.model.state.value.draft.map { it.name })
        assertEquals(Route.SEND, s.model.state.value.route)
        assertTrue(pending.take().isEmpty())
        // Nothing is sent until the user picks a device and taps Send.
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.Send })
    }

    @Test
    fun refusedDocumentsAreLeftOutOfTheDraft() = runTest {
        val pending = PendingShare()
        val s = makeModel(FakeDocuments(Files.createTempDirectory("staged").toFile().also(roots::add)), pending)
        pending.put(listOf("file:///data/user/0/io.github.dropumi/no_backup/dropumi/identity.key"))
        s.model.takeSharedItems()
        assertTrue(s.model.state.value.draft.isEmpty())
        assertNull(s.model.state.value.route)
        assertEquals("Some items can't be sent", s.model.state.value.banner?.title)
    }

    @Test
    fun documentsAreSentAsStreamsAndStagedCopiesAreReleased() = runTest {
        val folder = File(Files.createTempDirectory("outgoing").toFile().also(roots::add), "uuid-1")
        val docs = FakeDocuments(folder)
        val s = makeModel(docs, PendingShare())
        s.model.addDocuments(listOf("content://com.example.files/note.txt", "content://com.example.files/pipe"))
        s.model.sendDraft(SendTarget.Nearby("i"), "X", allowUnverified = false)
        val send = s.core.calls.single() as FakeCoreClient.Call.Send
        assertTrue(send.items[0] is SendItem.Stream)
        assertEquals(listOf(File(folder, "pipe").path), send.paths)
        assertTrue("the core still reads the copy", folder.exists())
        s.model.apply(listOf(NodeEvent.TransferFinished(1u, Direction.OUTGOING, Fixtures.result(TransferStatus.COMPLETED))))
        advanceUntilIdle()
        assertFalse(folder.exists())
    }

    // Events that arrive before the call that started their operation returns

    /**
     * A refused connection can end a send before send() returns: the event
     * collector then sees TransferFinished for an operation the model does
     * not know yet. The transfer must still end, and its staged copy go.
     */
    @Test
    fun outgoingFailureBeforeSendReturnsStillFinishes() = runTest {
        val folder = File(Files.createTempDirectory("outgoing").toFile().also(roots::add), "uuid-1")
        val s = makeModel(FakeDocuments(folder), PendingShare())
        s.model.start()
        advanceUntilIdle()
        s.model.addDocuments(listOf("content://com.example.files/pipe"))
        s.core.emitBeforeReturning("send") { op ->
            listOf(
                NodeEvent.TransferFinished(op, Direction.OUTGOING, Fixtures.result(TransferStatus.FAILED, transferId = null)),
            )
        }
        s.model.sendDraft(SendTarget.Address("192.0.2.1:47321", null), "192.0.2.1", allowUnverified = false)
        advanceUntilIdle()
        assertTrue("the send was made", s.core.calls.any { it is FakeCoreClient.Call.Send })
        assertTrue("no transfer is left connecting", s.model.state.value.active.isEmpty())
        assertEquals(HistoryEntry.Outcome.FAILED, s.model.state.value.history.single().outcome)
        assertTrue(s.model.state.value.draft.isEmpty())
        assertFalse("the staged copy is deleted", folder.exists())
    }

    /** Events that arrived early are applied in order once the record exists. */
    @Test
    fun outgoingEventsBeforeSendReturnsAreAppliedInOrder() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        val file = File(s.outgoing, "x.bin").also { it.writeBytes(byteArrayOf(1)) }
        s.model.addToDraft(listOf(OutgoingFile("x.bin", 1u, OutgoingFile.Source.Path(file.path))))
        s.core.emitBeforeReturning("send") { op ->
            listOf(
                NodeEvent.Connected(op, Fixtures.peer(trusted = true, alias = "Maya")),
                NodeEvent.OfferSent(op, "bb", Fixtures.peer(trusted = true, alias = "Maya"), 1u, 1u),
            )
        }
        s.model.sendDraft(SendTarget.Nearby("i"), "Maya", allowUnverified = false)
        advanceUntilIdle()
        val record = s.model.state.value.active.single()
        assertEquals("bb", record.transferId)
        assertEquals(TransferPhase.WaitingForAcceptance, record.phase)
        // Later events still reach it, and it ends.
        s.model.apply(listOf(NodeEvent.TransferFinished(record.operation, Direction.OUTGOING, Fixtures.result(TransferStatus.COMPLETED, "bb"))))
        advanceUntilIdle()
        assertTrue(s.model.state.value.active.isEmpty())
        assertEquals(HistoryEntry.Outcome.COMPLETED, s.model.state.value.history.single().outcome)
    }

    /** What is kept early is bounded, but the event that ends the transfer is never dropped. */
    @Test
    fun earlyEventsAreBoundedButTheEndIsKept() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        val file = File(s.outgoing, "x.bin").also { it.writeBytes(byteArrayOf(1)) }
        s.model.addToDraft(listOf(OutgoingFile("x.bin", 1u, OutgoingFile.Source.Path(file.path))))
        s.core.emitBeforeReturning("send") { op ->
            List(500) { NodeEvent.FileSent(op, "bb", it.toUInt(), FileStatus.STORED) } +
                NodeEvent.TransferFinished(op, Direction.OUTGOING, Fixtures.result(TransferStatus.COMPLETED, "bb"))
        }
        s.model.sendDraft(SendTarget.Nearby("i"), "Maya", allowUnverified = false)
        advanceUntilIdle()
        assertTrue(s.model.state.value.active.isEmpty())
        assertEquals(HistoryEntry.Outcome.COMPLETED, s.model.state.value.history.single().outcome)
    }

    /** A pairing window's port can arrive before openPairingWindow() returns: it is kept. */
    @Test
    fun pairingWindowPortBeforeTheCallReturnsIsKept() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        s.core.emitBeforeReturning("openPairingWindow") { op -> listOf(NodeEvent.PairingWindowOpened(op, 47_999u)) }
        s.model.openPairingWindow()
        advanceUntilIdle()
        assertEquals(47_999.toUShort(), s.model.state.value.pairing?.port)
    }

    /**
     * Browse-only devices are paired by address: during a pairing window the
     * addresses carry the window's port, and there are none before it opens.
     */
    @Test
    fun pairingWindowAddressesUseTheWindowPort() = runTest {
        val root = Files.createTempDirectory("dropumi-appmodel").toFile().also(roots::add)
        val model = AppModel(
            FakeCoreClient(),
            MemorySettingsStore(sdkInt = 36),
            HistoryStore(File(root, "app/history.json")),
            modelScope(),
            StandardTestDispatcher(testScheduler),
            localAddresses = { listOf("192.168.1.5") },
        )
        model.start()
        advanceUntilIdle()
        assertTrue("no window", model.pairingAddresses().isEmpty())
        model.openPairingWindow()
        advanceUntilIdle()
        assertTrue("the window has no port yet", model.pairingAddresses().isEmpty())
        model.apply(listOf(NodeEvent.PairingWindowOpened(requireNotNull(model.state.value.pairing).operation, 47_999u)))
        assertEquals(listOf("192.168.1.5:47999"), model.pairingAddresses())
    }

    /**
     * Opening a pairing window pauses the receiver (ReceiverStopped(STOPPED))
     * and the core resumes it when the window ends. The model must not try to
     * start receiving meanwhile: the core refuses that, and the user would
     * see "Receiving could not start" each time they pair. Found on the
     * Tablet AVD (Android 16) with the real core.
     */
    @Test
    fun pairingWindowPauseDoesNotRestartReceiving() = runTest {
        val s = makeModel()
        s.model.start()
        s.model.finishOnboarding()
        advanceUntilIdle()
        assertEquals(1, s.core.starts().size)
        // The pause can be reported before openPairingWindow returns.
        s.core.emitBeforeReturning("openPairingWindow") { listOf(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED)) }
        s.model.openPairingWindow()
        advanceUntilIdle()
        s.model.apply(listOf(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED)))
        s.model.applyVisibility()
        advanceUntilIdle()
        assertEquals(1, s.core.starts().size)
        assertNull(s.model.state.value.banner)
        assertNull(s.model.state.value.receiver)
    }

    /** The receiver's settings as the core resumes it after a window: those it had (Paired only, ask). */
    private val resumedReceiver = ReceiverInfo(47_321u, null, ReceivePolicy.TrustedOnly, AcceptMode.PROMPT, null)

    /**
     * A model that receives (Paired only) and has opened a pairing window,
     * which paused the receiver as the core does (ReceiverStopped while the
     * call runs). Returns the window's operation.
     */
    private suspend fun TestScope.receivingWithAPairingWindow(): Pair<Setup, ULong> {
        val s = makeModel()
        s.model.start()
        s.model.finishOnboarding()
        advanceUntilIdle()
        assertEquals(1, s.core.starts().size)
        s.core.emitBeforeReturning("openPairingWindow") { listOf(NodeEvent.ReceiverStopped(ReceiverStopReason.STOPPED)) }
        s.model.openPairingWindow()
        advanceUntilIdle()
        assertNull(s.model.state.value.receiver)
        assertTrue(s.model.state.value.pairingWindowActive)
        return s to requireNotNull(s.model.state.value.pairing).operation
    }

    /**
     * Settings chosen during a pairing window are kept, and applied when the
     * core resumes the paused receiver (with the settings it had then).
     */
    @Test
    fun receivingSettingsChosenDuringAPairingWindowApplyWhenItEnds() = runTest {
        val (s, window) = receivingWithAPairingWindow()
        s.model.setVisibility(ReceiveVisibility.EVERYONE)
        s.model.setAutoAcceptPaired(true)
        runCurrent()
        assertEquals(ReceiveVisibility.EVERYONE, s.model.state.value.visibility)
        assertTrue(s.model.state.value.autoAcceptPaired)
        assertNull("nothing refused, nothing failed", s.model.state.value.banner)
        assertEquals("the core refuses receiver changes during the window", 1, s.core.starts().size)
        assertTrue(s.core.calls.none { it is FakeCoreClient.Call.SetReceivePolicy })
        s.model.apply(listOf(NodeEvent.PairingFinished(window, PairingResult.Cancelled)))
        s.model.apply(listOf(NodeEvent.ReceiverStarted(resumedReceiver)))
        runCurrent()
        assertFalse(s.model.state.value.pairingWindowActive)
        assertEquals(
            FakeCoreClient.Call.SetReceivePolicy(ReceivePolicy.Everyone(ReceiveVisibility.EVERYONE_MINUTES), AcceptMode.AUTO_TRUSTED),
            s.core.calls.last(),
        )
        assertEquals(1, s.core.starts().size)
    }

    /**
     * Review of C1 (#1): pair() replaced the window's session, which unlocked
     * the receiving settings while the receiver was still paused; Off then
     * did nothing, and the core resumed the receiver with Paired only. Now a
     * pairing request is refused while the window is open (whose session and
     * Cancel stay on screen), and Off stops the resumed receiver.
     */
    @Test
    fun pairDuringAPairingWindowIsRefusedAndOffStopsTheResumedReceiver() = runTest {
        val (s, window) = receivingWithAPairingWindow()
        s.model.pair(SendTarget.Nearby("dropumi-00000001"), "Maya's phone")
        advanceUntilIdle()
        assertTrue("no pairing request", s.core.calls.none { it is FakeCoreClient.Call.Pair })
        assertEquals(window, s.model.state.value.pairing?.operation)
        assertEquals("Finish pairing first", s.model.state.value.banner?.title)
        assertTrue(s.model.state.value.pairingWindowActive)
        s.model.dismissBanner()
        s.model.setVisibility(ReceiveVisibility.OFF)
        runCurrent()
        assertEquals(ReceiveVisibility.OFF, s.model.state.value.visibility)
        // The window ends; the core resumes the receiver with its old settings.
        s.model.apply(listOf(NodeEvent.PairingFinished(window, PairingResult.Cancelled)))
        s.model.apply(listOf(NodeEvent.ReceiverStarted(resumedReceiver)))
        runCurrent()
        assertTrue("Off stops the resumed receiver", FakeCoreClient.Call.StopReceiving in s.core.calls)
        assertNull(s.model.state.value.receiver)
        assertEquals(1, s.core.starts().size)
    }

    /**
     * Review of C1 (#1): the core reports the window's end before it resumes
     * the receiver. Off chosen in between is applied to the resumed receiver.
     */
    @Test
    fun offChosenBetweenTheWindowsEndAndTheResumeStopsTheReceiver() = runTest {
        val (s, window) = receivingWithAPairingWindow()
        s.model.apply(listOf(NodeEvent.PairingFinished(window, PairingResult.Paired(Fixtures.peer(trusted = true)))))
        runCurrent()
        assertTrue("the core is resuming the receiver", s.model.state.value.pairingWindowActive)
        s.model.setVisibility(ReceiveVisibility.OFF)
        runCurrent()
        assertFalse(FakeCoreClient.Call.StopReceiving in s.core.calls)
        s.model.apply(listOf(NodeEvent.ReceiverStarted(resumedReceiver)))
        runCurrent()
        assertTrue(FakeCoreClient.Call.StopReceiving in s.core.calls)
        assertNull(s.model.state.value.receiver)
        assertFalse(s.model.state.value.pairingWindowActive)
    }

    /**
     * Review of C1 (#1): Everyone chosen between the window's end and the
     * resume used to start a second receiver, which the core refused
     * ("Receiving could not start"). It is applied to the resumed one.
     */
    @Test
    fun everyoneChosenBeforeTheResumeIsAppliedToTheResumedReceiver() = runTest {
        val (s, window) = receivingWithAPairingWindow()
        s.model.apply(listOf(NodeEvent.PairingFinished(window, PairingResult.Cancelled)))
        s.model.setVisibility(ReceiveVisibility.EVERYONE)
        runCurrent()
        assertEquals(1, s.core.starts().size)
        assertNull(s.model.state.value.banner)
        s.model.apply(listOf(NodeEvent.ReceiverStarted(resumedReceiver)))
        runCurrent()
        assertEquals(
            FakeCoreClient.Call.SetReceivePolicy(ReceivePolicy.Everyone(ReceiveVisibility.EVERYONE_MINUTES), AcceptMode.PROMPT),
            s.core.calls.last(),
        )
        assertNull(s.model.state.value.banner)
    }

    /** A receiver the core resumes while the app is in the background stops accepting at once. */
    @Test
    fun receiverResumedInTheBackgroundStopsAccepting() = runTest {
        val (s, window) = receivingWithAPairingWindow()
        s.model.enterBackground()
        runCurrent()
        assertFalse(FakeCoreClient.Call.StopAccepting in s.core.calls)
        s.model.apply(listOf(NodeEvent.PairingFinished(window, PairingResult.Cancelled)))
        s.model.apply(listOf(NodeEvent.ReceiverStarted(resumedReceiver)))
        runCurrent()
        assertTrue(FakeCoreClient.Call.StopAccepting in s.core.calls)
    }

    /** A window that paused nothing: a choice made during it applies when it ends. */
    @Test
    fun windowWithoutAReceiverAppliesTheChoiceWhenItEnds() = runTest {
        val s = makeModel()
        s.model.start()
        s.model.finishOnboarding(ReceiveVisibility.OFF)
        advanceUntilIdle()
        s.model.openPairingWindow()
        advanceUntilIdle()
        val window = requireNotNull(s.model.state.value.pairing).operation
        s.model.setVisibility(ReceiveVisibility.PAIRED_ONLY)
        runCurrent()
        assertTrue("the core refuses a start during the window", s.core.starts().isEmpty())
        s.model.apply(listOf(NodeEvent.PairingFinished(window, PairingResult.Cancelled)))
        runCurrent()
        assertFalse(s.model.state.value.pairingWindowActive)
        assertEquals(1, s.core.starts().size)
        assertNotNull(s.model.state.value.receiver)
    }

    /** If the core's resume is never reported, the hold still ends (bounded) and the settings apply. */
    @Test
    fun pairingHoldEndsWhenTheResumeIsNeverReported() = runTest {
        val (s, window) = receivingWithAPairingWindow()
        s.model.apply(listOf(NodeEvent.PairingFinished(window, PairingResult.Cancelled)))
        runCurrent()
        assertTrue(s.model.state.value.pairingWindowActive)
        advanceUntilIdle()
        assertFalse(s.model.state.value.pairingWindowActive)
        assertEquals("receiving starts again", 2, s.core.starts().size)
    }

    /** A dropped window end (ResyncRequired) cannot keep the settings on hold. */
    @Test
    fun resyncEndsThePairingHoldWhenTheWindowsEndWasDropped() = runTest {
        val (s, _) = receivingWithAPairingWindow()
        // The fake's snapshot lists no operation (the window was forgotten)
        // and a running receiver (it was resumed).
        s.model.apply(listOf(NodeEvent.ResyncRequired))
        advanceUntilIdle()
        assertFalse(s.model.state.value.pairingWindowActive)
        assertNotNull(s.model.state.value.receiver)
        s.model.setVisibility(ReceiveVisibility.OFF)
        assertTrue(FakeCoreClient.Call.StopReceiving in s.core.calls)
    }

    /**
     * Review of C1 (#1): a second tap on "Wait for a device" while the first
     * call runs is refused, and does not end the hold the first one holds.
     */
    @Test
    fun secondWindowWhileTheFirstOpensIsRefusedAndKeepsTheHold() = runTest {
        val s = makeModel(io = Dispatchers.IO)
        s.model.finishOnboarding()
        val gate = s.core.holdNext("openPairingWindow")
        val first = async { s.model.openPairingWindow() }
        gate.reached()
        s.model.openPairingWindow()
        assertEquals("Finish pairing first", s.model.state.value.banner?.title)
        assertTrue(s.model.state.value.pairingWindowActive)
        // The core may be pausing the receiver right now: a change waits.
        s.model.setVisibility(ReceiveVisibility.OFF)
        assertFalse(FakeCoreClient.Call.StopReceiving in s.core.calls)
        gate.open()
        first.await()
        assertEquals(1, s.core.calls.count { it == FakeCoreClient.Call.OpenPairingWindow })
        assertTrue(s.model.state.value.pairingWindowActive)
    }

    /** A window the core refuses ends the hold, and a change made meanwhile applies. */
    @Test
    fun refusedWindowEndsTheHold() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        s.core.failures["openPairingWindow"] = DropumiException.Busy("a transfer is running")
        s.model.openPairingWindow()
        advanceUntilIdle()
        assertFalse(s.model.state.value.pairingWindowActive)
        assertEquals("Pairing could not start", s.model.state.value.banner?.title)
        s.model.setVisibility(ReceiveVisibility.OFF)
        assertTrue(FakeCoreClient.Call.StopReceiving in s.core.calls)
    }

    /** A pairing request that runs refuses a window (one pairing on screen, with its Cancel). */
    @Test
    fun windowIsRefusedWhileAPairingRequestRuns() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        s.model.pair(SendTarget.Nearby("dropumi-00000001"), "Maya's phone")
        val request = requireNotNull(s.model.state.value.pairing).operation
        s.model.openPairingWindow()
        assertTrue(s.core.calls.none { it == FakeCoreClient.Call.OpenPairingWindow })
        assertEquals(request, s.model.state.value.pairing?.operation)
        assertFalse(s.model.state.value.pairingWindowActive)
        // Once it has ended, a window can open.
        s.model.apply(listOf(NodeEvent.PairingFinished(request, PairingResult.Cancelled)))
        s.model.openPairingWindow()
        assertEquals(1, s.core.calls.count { it == FakeCoreClient.Call.OpenPairingWindow })
    }

    /** A start reported after the model asked the receiver to stop does not bring it back. */
    @Test
    fun startReportedAfterAStopIsIgnored() = runTest {
        val s = makeModel()
        s.model.finishOnboarding()
        val info = requireNotNull(s.model.state.value.receiver)
        s.model.setVisibility(ReceiveVisibility.OFF)
        s.model.apply(listOf(NodeEvent.ReceiverStarted(info)))
        advanceUntilIdle()
        assertNull(s.model.state.value.receiver)
        assertTrue(s.model.state.value.receiverStopping)
        assertEquals(1, s.core.calls.count { it == FakeCoreClient.Call.StopReceiving })
    }

    /** Paired devices: a rename or a removal the core refuses says so (Apple gap 2). */
    @Test
    fun pairedDeviceChangesThatFailAreShown() = runTest {
        val s = makeModel()
        assertTrue(s.model.trust(Fixtures.PEER_FINGERPRINT, "Maya"))
        val device = s.model.state.value.trusted.single()
        s.core.failures["renameTrusted"] = DropumiException.Config(ConfigProblem.IO, "disk full")
        assertFalse(s.model.rename(device, "Maya's laptop"))
        assertEquals("Not renamed", s.model.state.value.banner?.title)
        assertEquals(ErrorCatalog.message(DropumiException.Config(ConfigProblem.IO, "x")), s.model.state.value.banner?.message)
        s.model.dismissBanner()
        s.core.failures["untrust"] = DropumiException.Config(ConfigProblem.IO, "disk full")
        assertFalse(s.model.forget(device))
        assertEquals("Not removed", s.model.state.value.banner?.title)
        assertEquals(1, s.model.state.value.trusted.size)
        s.model.dismissBanner()
        assertTrue(s.model.forget(device))
        assertNull(s.model.state.value.banner)
        assertTrue(s.model.state.value.trusted.isEmpty())
    }

    /** A pairing that ends before pair() returns still shows its result. */
    @Test
    fun pairingResultBeforeTheCallReturnsIsKept() = runTest {
        val s = makeModel()
        s.model.start()
        advanceUntilIdle()
        val result = PairingResult.Paired(Fixtures.peer(trusted = true))
        s.core.emitBeforeReturning("pair") { op -> listOf(NodeEvent.PairingFinished(op, result)) }
        s.model.pair(SendTarget.Nearby("i"), "Maya")
        advanceUntilIdle()
        assertEquals(result, s.model.state.value.pairing?.result)
    }

    // Android: received files leave only on request (ADR-0013 D4)

    private class FakeExporter : ReceivedExporter {
        val saved = mutableListOf<List<ReceivedFile>>()
        var result: ExportResult = ExportResult.Saved(listOf("content://media/external/downloads/1"))
        override val savesToDownloads = true

        override fun saveToDownloads(files: List<ReceivedFile>): ExportResult {
            saved += files
            return result
        }

        override fun saveToFolder(treeUri: String, files: List<ReceivedFile>): ExportResult = saveToDownloads(files)
    }

    private fun TestScope.makeModel(
        exporter: ReceivedExporter,
        receiveDir: File,
        io: CoroutineDispatcher = StandardTestDispatcher(testScheduler),
    ): Setup {
        val root = Files.createTempDirectory("dropumi-appmodel").toFile().also(roots::add)
        val core = FakeCoreClient()
        val model = AppModel(
            core,
            MemorySettingsStore(sdkInt = 36),
            HistoryStore(File(root, "app/history.json")),
            modelScope(),
            io,
            clock = { testScheduler.currentTime },
            receiveDir = receiveDir,
            exporter = exporter,
        )
        return Setup(model, core, root)
    }

    /** A received transfer of [name] (10 bytes), as the core reports it. */
    private fun AppModel.receive(receiveDir: File, name: String = "a.txt", transferId: String = "aa"): HistoryEntry {
        File(receiveDir, name).writeBytes(ByteArray(10))
        apply(
            listOf(
                NodeEvent.OfferReceived(transferId, Fixtures.peer(trusted = true, alias = "Studio Mac"), 1u, 10u),
                NodeEvent.FileSaved(transferId, 0u, name, 10u, "00"),
                NodeEvent.TransferFinished(null, Direction.INCOMING, Fixtures.result(TransferStatus.COMPLETED, transferId)),
            ),
        )
        return state.value.history.first()
    }

    /** Runs io work only when the test says so, so the test can act while it waits. */
    private class ManualDispatcher : CoroutineDispatcher() {
        private val queue = ArrayDeque<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            synchronized(queue) { queue.addLast(block) }
        }

        fun runPending() {
            while (true) {
                val next = synchronized(queue) { queue.removeFirstOrNull() } ?: return
                next.run()
            }
        }
    }

    /**
     * Review of C1 (#3): Clear deleted the listed transfers' files one by one
     * and then emptied the whole list, so a transfer that finished meanwhile
     * lost its entry while its files stayed, unreachable. Clear now removes
     * only the entries whose files it deleted.
     */
    @Test
    fun clearKeepsATransferThatFinishesDuringTheClear() = runTest {
        val dir = Files.createTempDirectory("received").toFile().also(roots::add)
        val io = ManualDispatcher()
        val s = makeModel(FakeExporter(), dir, io)
        val first = s.model.receive(dir)
        val clearing = async { s.model.clearHistory() }
        runCurrent()
        assertFalse("the deletion waits for the io dispatcher", clearing.isCompleted)
        val second = s.model.receive(dir, "b.txt", "bb")
        assertNotEquals(first.id, second.id)
        repeat(MAX_ROUNDS) {
            if (!clearing.isCompleted) {
                io.runPending()
                runCurrent()
            }
        }
        assertTrue(clearing.isCompleted)
        assertFalse(File(dir, "a.txt").exists())
        assertTrue("its files stay with their entry", File(dir, "b.txt").exists())
        val left = s.model.state.value.history.single()
        assertEquals(second.id, left.id)
        assertEquals(listOf("b.txt"), left.savedAs)
    }

    /** Clear deletes every listed transfer's copies and then the list. */
    @Test
    fun clearDeletesTheReceivedCopies() = runTest {
        val dir = Files.createTempDirectory("received").toFile().also(roots::add)
        val s = makeModel(FakeExporter(), dir)
        s.model.receive(dir)
        s.model.receive(dir, "b.txt", "bb")
        s.model.clearHistory()
        advanceUntilIdle()
        assertTrue(s.model.state.value.history.isEmpty())
        assertTrue(dir.listFiles().isNullOrEmpty())
        assertNull(s.model.state.value.banner)
    }

    /** A file that cannot be deleted stays listed (with its size), so it can still be opened, saved or deleted. */
    @Test
    fun filesThatCannotBeDeletedStayListed() = runTest {
        val dir = Files.createTempDirectory("received").toFile().also(roots::add)
        val s = makeModel(FakeExporter(), dir)
        s.model.receive(dir)
        assertTrue(dir.setWritable(false))
        try {
            s.model.clearHistory()
            advanceUntilIdle()
        } finally {
            dir.setWritable(true)
        }
        assertTrue(File(dir, "a.txt").exists())
        val entry = s.model.state.value.history.single()
        assertEquals(listOf("a.txt"), entry.savedAs)
        assertEquals(listOf(10uL), entry.savedSizes)
        assertEquals("Not all files were deleted", s.model.state.value.banner?.title)
    }

    /**
     * Review of C1 (#4): the rename's in-progress flag lives in the model,
     * so a screen recreated during a rename shows it and sees it end. A
     * second rename while one runs is ignored.
     */
    @Test
    fun renameInProgressIsHeldByTheModel() = runTest {
        val s = makeModel(io = Dispatchers.IO)
        s.model.start()
        val gate = s.core.holdNext("setDeviceName")
        val renaming = async { s.model.renameDevice("Studio tablet") }
        gate.reached()
        assertTrue(s.model.state.value.renaming)
        s.model.renameDevice("Other name")
        gate.open()
        renaming.await()
        assertFalse(s.model.state.value.renaming)
        assertEquals("Studio tablet", s.model.state.value.identity?.name)
    }

    /** Review of C1 (#4): the onboarding page lives in the model, not in a screen's saved state. */
    @Test
    fun onboardingPageIsHeldByTheModel() = runTest {
        val s = makeModel()
        assertEquals(0, s.model.state.value.onboardingPage)
        s.model.showOnboardingPage(2)
        assertEquals(2, s.model.state.value.onboardingPage)
    }

    @Test
    fun receivedSizesAreKeptForSave() = runTest {
        val dir = Files.createTempDirectory("received").toFile().also(roots::add)
        val s = makeModel(FakeExporter(), dir)
        val entry = s.model.receive(dir)
        assertEquals(listOf("a.txt"), entry.savedAs)
        assertEquals(listOf(10uL), entry.savedSizes)
    }

    /** The first Save says who else can see saved files; Save never deletes the internal copy. */
    @Test
    fun saveShowsTheDisclosureOnceAndKeepsTheInternalCopy() = runTest {
        val dir = Files.createTempDirectory("received").toFile().also(roots::add)
        val exporter = FakeExporter()
        val s = makeModel(exporter, dir)
        val entry = s.model.receive(dir)
        s.model.saveToDownloads(entry)
        assertEquals(listOf(listOf(ReceivedFile("a.txt", 10))), exporter.saved)
        assertEquals("Saved files can be opened by other apps that can see your Downloads.", s.model.state.value.banner?.message)
        s.model.saveToDownloads(entry)
        assertEquals("1 file saved.", s.model.state.value.banner?.message)
        assertTrue(File(dir, "a.txt").exists())
        exporter.result = ExportResult.Failed(ExportResult.Reason.CHANGED, 0)
        s.model.saveToDownloads(entry)
        assertEquals("Not saved", s.model.state.value.banner?.title)
    }

    @Test
    fun saveRefusesFilesWithoutReportedSizes() = runTest {
        val dir = Files.createTempDirectory("received").toFile().also(roots::add)
        val exporter = FakeExporter()
        val s = makeModel(exporter, dir)
        val entry = s.model.receive(dir).copy(savedSizes = emptyList())
        s.model.saveToDownloads(entry)
        assertTrue(exporter.saved.isEmpty())
        assertEquals("Not saved", s.model.state.value.banner?.title)
        s.model.saveToDownloads(entry.copy(savedAs = listOf("../escape"), savedSizes = listOf(1u)))
        assertTrue(exporter.saved.isEmpty())
    }

    @Test
    fun deleteRemovesOnlyTheTransfersInternalCopies() = runTest {
        val dir = Files.createTempDirectory("received").toFile().also(roots::add)
        val s = makeModel(FakeExporter(), dir)
        val other = File(dir, "other.txt").also { it.writeText("keep") }
        val outside = File(dir.parentFile, "outside-${dir.name}.txt").also { it.writeText("keep") }
        val entry = s.model.receive(dir)
        s.model.deleteReceived(entry.copy(savedAs = entry.savedAs + "../${outside.name}"))
        assertFalse(File(dir, "a.txt").exists())
        assertTrue(other.exists())
        assertTrue("a name that is not a single component is left alone", outside.exists())
        outside.delete()
        assertTrue(s.model.state.value.history.first().savedAs.isEmpty())
    }
}
