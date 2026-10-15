// SPDX-License-Identifier: MPL-2.0
//
// The consent and code screens (ADR-0013 D8). UI state tests (fake core):
// MainActivity runs on the fake core with temporary folders; a prompt shows
// only because the test's scenario or the test itself pushed it, and these
// results say nothing about the real core.
package io.github.dropumi.prompt

import android.view.KeyEvent
import androidx.activity.BackEventCompat
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.requestFocus
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.MainActivity
import io.github.dropumi.UseScenario
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.ffi.CodePurpose
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.PromptCloseReason
import io.github.dropumi.testsupport.FakeCoreClient.Call
import io.github.dropumi.testsupport.Fixtures
import io.github.dropumi.testsupport.Scenario
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PromptScreenTest {
    private val fake = FakeScenarioRule()
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    private fun answers(): List<Call> = fake.core.calls.filter { it is Call.AnswerOffer || it is Call.AnswerCode }

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    /** Launches MainActivity and waits for [tag] and for the window's focus (focus re-arms the prompt). */
    private fun launchShowing(tag: String): ActivityScenario<MainActivity> {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(TIMEOUT_MS) { exists(tag) }
        compose.waitUntil(TIMEOUT_MS) {
            var focused = false
            scenario.onActivity { focused = it.hasWindowFocus() }
            focused
        }
        compose.waitForIdle()
        return scenario
    }

    /** Lets the 600 ms arming run out on the test clock. */
    private fun arm() {
        compose.mainClock.advanceTimeBy(ARMED_AFTER_MS)
        compose.waitForIdle()
    }

    private fun waitForCall(call: Call) = compose.waitUntil(TIMEOUT_MS) { call in fake.core.calls }

    private fun key(code: Int) = instrumentation.sendKeyDownUpSync(code)

    /** The test tag of the focused node, if any. */
    private fun focusedTag(): String? = compose.onAllNodes(isFocused()).fetchSemanticsNodes()
        .firstOrNull()?.config?.getOrNull(SemanticsProperties.TestTag)

    /** Enter, NumPad Enter, DPAD centre and Space, twice each; then the window is idle. */
    private fun pressActivationKeys() {
        for (code in ACTIVATION_KEYS) repeat(2) { key(code) }
        instrumentation.waitForIdleSync()
        compose.waitForIdle()
    }

    private fun assertNothingAnswered(why: String) {
        assertTrue(why, answers().isEmpty())
        assertTrue(why, model.state.value.answering.isEmpty())
    }

    /** Tab until [done] holds for the focused node's tag (bounded). */
    private fun tabUntil(what: String, done: (String?) -> Boolean) {
        val seen = mutableListOf<String?>()
        repeat(MAX_TABS) {
            key(KeyEvent.KEYCODE_TAB)
            compose.waitForIdle()
            seen += focusedTag()
            if (done(seen.last())) return
        }
        fail("$what: focus went $seen")
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun offerUnverifiedShowsClaimsAndFingerprint() {
        launchShowing("prompt.offer").use {
            compose.onNodeWithTag("prompt.peer").assertTextEquals("Unverified device")
            compose.onNodeWithText("claims “Maya's phone”").assertExists()
            compose.onNodeWithText("Not paired").assertExists()
            compose.onNodeWithText("Paired").assertDoesNotExist()
            compose.onNodeWithTag("prompt.fingerprint").assertTextEquals(Fixtures.peer().fingerprintDisplay)
            compose.onNodeWithTag("code.digits").assertTextEquals("123 456")
            compose.onNodeWithText("Accept only if both screens show the same code.").assertExists()
            compose.onNodeWithText("wants to send you 3 files").assertExists()
            compose.onNodeWithText("Lease-draft.pdf").assertExists()
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_PAIRED)
    fun pairedOfferShowsAliasAndNoCode() {
        launchShowing("prompt.offer").use {
            compose.onNodeWithTag("prompt.peer").assertTextEquals("Studio Mac")
            compose.onNodeWithText("claims “MacBook”").assertExists()
            compose.onNodeWithText("Paired").assertExists()
            compose.onNodeWithTag("prompt.fingerprint").assertDoesNotExist()
            compose.onNodeWithTag("code.digits").assertDoesNotExist()
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun acceptDisabledBefore600ms() {
        launchShowing("prompt.offer").use {
            compose.mainClock.autoAdvance = false
            compose.onNodeWithTag("offer.accept").assertIsNotEnabled()
            compose.onNodeWithTag("offer.decline").assertIsEnabled()
            compose.mainClock.advanceTimeBy(500)
            compose.onNodeWithTag("offer.accept").assertIsNotEnabled()
            compose.mainClock.advanceTimeBy(150)
            compose.onNodeWithTag("offer.accept").assertIsEnabled()
            compose.mainClock.autoAdvance = true
            compose.onNodeWithTag("offer.accept").performClick()
            waitForCall(Call.AnswerOffer(1u, true))
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun acceptReArmsAfterResume() {
        launchShowing("prompt.offer").use { scenario ->
            arm()
            compose.onNodeWithTag("offer.accept").assertIsEnabled()
            scenario.moveToState(Lifecycle.State.STARTED)
            scenario.moveToState(Lifecycle.State.RESUMED)
            compose.waitUntil(TIMEOUT_MS) {
                var focused = false
                scenario.onActivity { focused = it.hasWindowFocus() }
                focused
            }
            compose.waitForIdle()
            compose.onNodeWithTag("offer.accept").assertIsNotEnabled()
            arm()
            compose.onNodeWithTag("offer.accept").assertIsEnabled()
            assertTrue(answers().isEmpty())
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun acceptReArmsWhenTheWindowRegainsFocus() {
        launchShowing("prompt.offer").use { scenario ->
            arm()
            compose.onNodeWithTag("offer.accept").assertIsEnabled()
            // What the window gets back from the notification shade or a system dialog.
            scenario.onActivity { it.onWindowFocusChanged(true) }
            compose.waitForIdle()
            compose.onNodeWithTag("offer.accept").assertIsNotEnabled()
            arm()
            compose.onNodeWithTag("offer.accept").assertIsEnabled()
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun backDeclinesOffer() {
        launchShowing("prompt.offer").use {
            key(KeyEvent.KEYCODE_BACK)
            waitForCall(Call.AnswerOffer(1u, false))
            assertEquals(listOf<Call>(Call.AnswerOffer(1u, false)), answers())
            // Only the core closes it.
            compose.onNodeWithTag("prompt.waiting").assertExists()
            compose.onNodeWithTag("offer.decline").assertIsNotEnabled()
        }
    }

    @Test
    @UseScenario(Scenario.CODE_PAIRING)
    fun backDeclinesCode() {
        launchShowing("prompt.code").use {
            key(KeyEvent.KEYCODE_BACK)
            waitForCall(Call.AnswerCode(2u, false))
            assertEquals(listOf<Call>(Call.AnswerCode(2u, false)), answers())
        }
    }

    /** With a callback registered, Back never runs back-to-home: the activity stays resumed. */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun backWithPromptKeepsActivityResumed() {
        launchShowing("prompt.offer").use { scenario ->
            key(KeyEvent.KEYCODE_BACK)
            waitForCall(Call.AnswerOffer(1u, false))
            // A second Back while the answer waits for the core does nothing either.
            key(KeyEvent.KEYCODE_BACK)
            instrumentation.waitForIdleSync()
            assertEquals(Lifecycle.State.RESUMED, scenario.state)
            assertEquals(1, answers().size)
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun cancelledPredictiveBackDoesNothing() {
        launchShowing("prompt.offer").use { scenario ->
            scenario.onActivity {
                val dispatcher = it.onBackPressedDispatcher
                dispatcher.dispatchOnBackStarted(BackEventCompat(10f, 500f, 0f, BackEventCompat.EDGE_LEFT))
                dispatcher.dispatchOnBackProgressed(BackEventCompat(80f, 500f, 0.4f, BackEventCompat.EDGE_LEFT))
                dispatcher.dispatchOnBackCancelled()
            }
            instrumentation.waitForIdleSync()
            compose.waitForIdle()
            assertTrue(answers().isEmpty())
            assertTrue(model.state.value.answering.isEmpty())
            // The same gesture completed declines.
            scenario.onActivity {
                val dispatcher = it.onBackPressedDispatcher
                dispatcher.dispatchOnBackStarted(BackEventCompat(10f, 500f, 0f, BackEventCompat.EDGE_LEFT))
                dispatcher.onBackPressed()
            }
            waitForCall(Call.AnswerOffer(1u, false))
        }
    }

    /** Escape declines with nothing focused: MainActivity maps it before Compose. */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun escapeDeclines() {
        launchShowing("prompt.offer").use {
            assertTrue("nothing is focused", compose.onAllNodes(isFocused()).fetchSemanticsNodes().isEmpty())
            key(KeyEvent.KEYCODE_ESCAPE)
            waitForCall(Call.AnswerOffer(1u, false))
            assertEquals(listOf<Call>(Call.AnswerOffer(1u, false)), answers())
        }
    }

    /**
     * No default action: Enter, NumPad Enter, DPAD centre and Space answer
     * nothing until the user moves focus with a navigation key. Then Enter
     * acts on the button the user moved to.
     */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun enterNeverAccepts() {
        launchShowing("prompt.offer").use {
            arm()
            for (code in listOf(KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER, KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_SPACE)) {
                repeat(2) { key(code) }
            }
            instrumentation.waitForIdleSync()
            compose.waitForIdle()
            assertTrue(answers().isEmpty())
            assertTrue(model.state.value.answering.isEmpty())

            // Tab until Accept has focus (bounded; at least two presses, as
            // leaving touch mode can swallow the first), then Enter accepts:
            // the keys do reach the window.
            var tabs = 0
            val seen = mutableListOf<String?>()
            do {
                key(KeyEvent.KEYCODE_TAB)
                tabs++
                compose.waitForIdle()
                seen += focusedTag()
            } while (!(seen.last() == "offer.accept" && tabs > 1) && tabs < MAX_TABS)
            assertEquals("focus went $seen", "offer.accept", seen.last())
            arm()
            key(KeyEvent.KEYCODE_ENTER)
            waitForCall(Call.AnswerOffer(1u, true))
        }
    }

    /**
     * A keyboard user: outside touch mode the system focuses a control by
     * itself when the window first lays out, which may be Accept. An
     * activation key still answers nothing until the user moves focus.
     */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun enterNeverAcceptsOutsideTouchMode() {
        instrumentation.setInTouchMode(false)
        launchShowing("prompt.offer").use {
            arm()
            for (code in listOf(KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER, KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_SPACE)) {
                repeat(2) { key(code) }
            }
            instrumentation.waitForIdleSync()
            compose.waitForIdle()
            assertTrue("focused: ${focusedTag()}", answers().isEmpty())
            assertTrue(model.state.value.answering.isEmpty())
        }
    }

    /**
     * Outside touch mode Android can focus a button by itself (leaving
     * touch mode, the window regaining focus). [button] gets focus the user
     * did not give it, and then:
     * - no activation key answers;
     * - nor after Page Down, Page Up, Home or End (not navigation keys
     *   here; Compose moves focus up for Page Up, which still does not
     *   count), each pressed with [button] focused that way;
     * - nor after an arrow down, which leaves focus on [button] (the
     *   buttons are at the bottom).
     * Then the user Tabs off the button and back onto it, and Enter answers:
     * the keys do reach the window.
     */
    private fun activationNeedsTheUsersFocusMove(tag: String, button: String, answer: Call) {
        // Before launch: before API 34 a window already shown keeps its
        // touch mode until a key or its next focus change.
        instrumentation.setInTouchMode(false)
        launchShowing(tag).use {
            arm()
            val focusAsTheSystemMay = {
                compose.onNodeWithTag(button).assertIsEnabled().requestFocus()
                compose.waitForIdle()
                assertEquals(button, focusedTag())
            }
            focusAsTheSystemMay()
            pressActivationKeys()
            assertNothingAnswered("answered on focus the user did not move")

            for (code in NON_NAVIGATION_KEYS) {
                focusAsTheSystemMay()
                key(code)
                compose.waitForIdle()
                val focus = focusedTag()
                pressActivationKeys()
                assertNothingAnswered("answered after ${KeyEvent.keyCodeToString(code)} (focus on $focus)")
            }

            focusAsTheSystemMay()
            key(KeyEvent.KEYCODE_DPAD_DOWN)
            compose.waitForIdle()
            assertEquals("an arrow down moved focus", button, focusedTag())
            pressActivationKeys()
            assertNothingAnswered("answered after an arrow that moved no focus")

            tabUntil("Tab off $button") { it != button }
            tabUntil("Tab back to $button") { it == button }
            key(KeyEvent.KEYCODE_ENTER)
            waitForCall(answer)
            assertEquals(listOf(answer), answers())
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun enterNeverAcceptsOnFocusTheUserDidNotMove() {
        activationNeedsTheUsersFocusMove("prompt.offer", "offer.accept", Call.AnswerOffer(1u, true))
    }

    @Test
    @UseScenario(Scenario.CODE_PAIRING)
    fun enterNeverConfirmsACodeOnFocusTheUserDidNotMove() {
        activationNeedsTheUsersFocusMove("prompt.code", "code.match", Call.AnswerCode(2u, true))
    }

    /** Both choices on the code screen are the same size and kind; neither is the default. */
    @Test
    @UseScenario(Scenario.CODE_PAIRING)
    fun codeScreenHasEqualChoices() {
        launchShowing("prompt.code").use {
            compose.onNodeWithText("Pair with this device?").assertExists()
            compose.onNodeWithTag("code.digits").assertTextEquals("654 321")
            compose.onNodeWithTag("prompt.fingerprint").assertExists()
            compose.onNodeWithTag("code.match").assertIsNotEnabled()
            arm()
            val match = compose.onNodeWithTag("code.match").assertIsEnabled().fetchSemanticsNode()
            val mismatch = compose.onNodeWithTag("code.mismatch").assertIsEnabled().fetchSemanticsNode()
            assertTrue(abs(match.size.width - mismatch.size.width) <= 1)
            assertEquals(match.size.height, mismatch.size.height)
            assertEquals(
                match.config.getOrNull(SemanticsProperties.Role),
                mismatch.config.getOrNull(SemanticsProperties.Role),
            )
            compose.onNodeWithTag("code.mismatch").performClick()
            waitForCall(Call.AnswerCode(2u, false))
        }
    }

    @Test
    fun unverifiedReceiverCodeSaysNothingIsSentYet() {
        fake.core.emit(NodeEvent.PromptOpened(Fixtures.code(id = 4u, purpose = CodePurpose.UNVERIFIED_RECEIVER)))
        launchShowing("prompt.code").use {
            compose.onNodeWithText("Check the code").assertExists()
            compose.onNodeWithText(
                "This device isn't paired. Check that its screen shows the same code before anything is sent.",
            ).assertExists()
        }
    }

    /** Peer strings are literal text: never a format string, HTML or Markdown. */
    @Test
    fun literalNamesRendered() {
        val hostile = "%s %n <b>x</b> [a](b)"
        fake.core.emit(
            NodeEvent.PromptOpened(
                Fixtures.offer(peer = Fixtures.peer(name = hostile), files = listOf(OfferedFile(hostile, 5u))),
            ),
        )
        launchShowing("prompt.offer").use {
            compose.onNodeWithText("claims “$hostile”").assertExists()
            compose.onNodeWithText(hostile).assertExists()
        }
    }

    /** "Waiting…" after an answer, until the core closes the prompt; then it goes. */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun promptStaysUntilTheCoreClosesIt() {
        launchShowing("prompt.offer").use {
            compose.onNodeWithTag("offer.decline").performClick()
            waitForCall(Call.AnswerOffer(1u, false))
            compose.waitUntil(TIMEOUT_MS) { exists("prompt.waiting") }
            compose.onNodeWithTag("offer.accept").assertIsNotEnabled()
            fake.core.emit(NodeEvent.PromptClosed(1u, PromptCloseReason.ANSWERED))
            compose.waitUntil(TIMEOUT_MS) { !exists("prompt.offer") }
        }
    }

    /** At zero the prompt reads "Expired"; Accept stays off, nothing is answered, Decline still works. */
    @Test
    fun expiredOfferCannotBeAccepted() {
        val past = (System.currentTimeMillis() - 1_000).toULong()
        fake.core.emit(NodeEvent.PromptOpened(Fixtures.offer(expiresAtMs = past)))
        launchShowing("prompt.offer").use {
            arm()
            compose.onNodeWithTag("prompt.expiry").assertTextEquals("Expired")
            compose.onNodeWithTag("offer.accept").assertIsNotEnabled()
            assertTrue(answers().isEmpty())
            compose.onNodeWithTag("offer.decline").assertIsEnabled().performClick()
            waitForCall(Call.AnswerOffer(1u, false))
        }
    }

    /** The prompt carries 200 files; the rest come from the core in pages of at most 1000 as the list scrolls. */
    @Test
    @UseScenario(Scenario.OFFER_1500_FILES)
    fun offerPagesPast200Files() {
        launchShowing("prompt.offer").use {
            compose.onNodeWithText("wants to send you 1500 files").assertExists()
            val last = hasText("file-1499.bin")
            var rounds = 0
            while (compose.onAllNodes(last).fetchSemanticsNodes().isEmpty() && rounds < MAX_PAGE_ROUNDS) {
                val list = compose.onNodeWithTag("prompt.files")
                val rows = requireNotNull(list.fetchSemanticsNode().config.getOrNull(SemanticsProperties.CollectionInfo)).rowCount
                list.performScrollToIndex(rows - 1)
                compose.waitUntil(TIMEOUT_MS) {
                    val now = compose.onNodeWithTag("prompt.files").fetchSemanticsNode()
                        .config.getOrNull(SemanticsProperties.CollectionInfo)?.rowCount ?: rows
                    now != rows || compose.onAllNodes(last).fetchSemanticsNodes().isNotEmpty()
                }
                rounds++
            }
            compose.onNode(last).assertExists()
            compose.onNodeWithTag("prompt.files.more").assertDoesNotExist()
            assertEquals(
                listOf<Call>(Call.OfferFiles(1u, 200u, 1000u), Call.OfferFiles(1u, 1200u, 300u)),
                fake.core.calls.filterIsInstance<Call.OfferFiles>(),
            )
        }
    }

    private companion object {
        const val TIMEOUT_MS = 10_000L

        /** A little past the 600 ms arming. */
        const val ARMED_AFTER_MS = 700L
        const val MAX_TABS = 12
        const val MAX_PAGE_ROUNDS = 6

        val ACTIVATION_KEYS = listOf(
            KeyEvent.KEYCODE_ENTER,
            KeyEvent.KEYCODE_NUMPAD_ENTER,
            KeyEvent.KEYCODE_DPAD_CENTER,
            KeyEvent.KEYCODE_SPACE,
        )

        val NON_NAVIGATION_KEYS = listOf(
            KeyEvent.KEYCODE_PAGE_DOWN,
            KeyEvent.KEYCODE_PAGE_UP,
            KeyEvent.KEYCODE_MOVE_HOME,
            KeyEvent.KEYCODE_MOVE_END,
        )
    }
}
