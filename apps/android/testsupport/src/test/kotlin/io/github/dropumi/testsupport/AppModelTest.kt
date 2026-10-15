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
import io.github.dropumi.appstate.PeerLabel
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.promptId
import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.ConfigProblem
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.DropumiException
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.PromptCloseReason
import io.github.dropumi.ffi.ReceiveOptions
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.ReceiverStopReason
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TransferStatus
import java.io.File
import java.nio.file.Files
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
        assertNotNull(s.model.state.value.receiver)
        // Receiver changes run one after another; this one runs last.
        s.model.applyVisibility()
        assertTrue(FakeCoreClient.Call.StopAccepting in s.core.calls)
        assertFalse(FakeCoreClient.Call.StopReceiving in s.core.calls)
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

    /** ADR-0013 D3: the name onboarding suggests is only the core's own `Dropumi-XXXX`. */
    @Test
    fun defaultNameComesOnlyFromCore() = runTest {
        val s = makeModel()
        assertNull(s.model.state.value.suggestedDeviceName)
        s.model.refreshIdentity()
        assertEquals("Dropumi-1A2B", s.model.state.value.suggestedDeviceName)
    }
}
