// SPDX-License-Identifier: MPL-2.0
//
// Keys on the in-window confirmations for trust and policy changes
// (ADR-0013 D8; plan §7.2 items 3 and 5). UI state tests (fake core):
// MainActivity runs on the fake core with temporary folders, and these
// results say nothing about the real core. Who can send → Everyone stands
// for every confirmation: they share one panel and one key path.
package io.github.dropumi.prompt

import android.view.KeyEvent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.requestFocus
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.MainActivity
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.testsupport.FakeCoreClient.Call
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ConfirmationKeysTest {
    private val fake = FakeScenarioRule()
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    private fun key(code: Int) = instrumentation.sendKeyDownUpSync(code)

    private fun focusedTag(): String? = compose.onAllNodes(isFocused()).fetchSemanticsNodes()
        .firstOrNull()?.config?.getOrNull(SemanticsProperties.TestTag)

    /** Settings, then "Everyone · 10 min": the confirmation panel shows. */
    private fun openEveryone(): ActivityScenario<MainActivity> {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(TIMEOUT_MS) { exists("tab.settings") }
        compose.waitUntil(TIMEOUT_MS) {
            var focused = false
            scenario.onActivity { focused = it.hasWindowFocus() }
            focused
        }
        compose.onNodeWithTag("tab.settings").performClick()
        compose.onNodeWithText("Everyone · 10 min").performClick()
        compose.waitUntil(TIMEOUT_MS) { exists("confirmation") }
        compose.waitForIdle()
        return scenario
    }

    private fun pressActivationKeys() {
        for (code in ACTIVATION_KEYS) repeat(2) { key(code) }
        instrumentation.waitForIdleSync()
        compose.waitForIdle()
    }

    private fun assertEveryoneNotEnabled(why: String) {
        assertTrue(why, exists("confirmation"))
        assertEquals(why, ReceiveVisibility.PAIRED_ONLY, model.state.value.visibility)
        assertTrue(why, fake.core.calls.none { it is Call.SetReceivePolicy && it.policy is ReceivePolicy.Everyone })
        assertTrue(why, fake.core.calls.none { it is Call.StartReceiving && it.options.policy is ReceivePolicy.Everyone })
    }

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

    /**
     * A keyboard user taps "Everyone · 10 min", then a key leaves touch
     * mode, and Android may focus Confirm by itself. No activation key
     * confirms: not with nothing focused, not with Confirm focused that way,
     * and not after Page Down, Page Up, Home or End (not navigation keys
     * here). Then the user Tabs off Confirm and back onto it, and Enter
     * confirms: the keys do reach the window.
     */
    @Test
    fun enterNeverConfirmsOutsideTouchMode() {
        openEveryone().use {
            instrumentation.setInTouchMode(false)
            compose.waitForIdle()
            pressActivationKeys()
            assertEveryoneNotEnabled("confirmed with no focus moved (focus on ${focusedTag()})")

            val focusAsTheSystemMay = {
                compose.onNodeWithTag(CONFIRM).requestFocus()
                compose.waitForIdle()
                assertEquals(CONFIRM, focusedTag())
            }
            focusAsTheSystemMay()
            pressActivationKeys()
            assertEveryoneNotEnabled("confirmed on focus the user did not move")

            for (code in NON_NAVIGATION_KEYS) {
                focusAsTheSystemMay()
                key(code)
                compose.waitForIdle()
                val focus = focusedTag()
                pressActivationKeys()
                assertEveryoneNotEnabled("confirmed after ${KeyEvent.keyCodeToString(code)} (focus on $focus)")
            }

            tabUntil("Tab off Confirm") { it != CONFIRM }
            tabUntil("Tab back to Confirm") { it == CONFIRM }
            key(KeyEvent.KEYCODE_ENTER)
            compose.waitUntil(TIMEOUT_MS) { model.state.value.visibility == ReceiveVisibility.EVERYONE }
            compose.waitUntil(TIMEOUT_MS) { !exists("confirmation") }
        }
    }

    /** Escape cancels with nothing focused: MainActivity maps it before Compose. */
    @Test
    fun escapeCancelsConfirmation() {
        openEveryone().use {
            assertTrue("nothing is focused", compose.onAllNodes(isFocused()).fetchSemanticsNodes().isEmpty())
            key(KeyEvent.KEYCODE_ESCAPE)
            compose.waitUntil(TIMEOUT_MS) { !exists("confirmation") }
            assertEquals(ReceiveVisibility.PAIRED_ONLY, model.state.value.visibility)
            assertTrue(fake.core.calls.none { it is Call.SetReceivePolicy && it.policy is ReceivePolicy.Everyone })
        }
    }

    private companion object {
        const val TIMEOUT_MS = 10_000L
        const val MAX_TABS = 12
        const val CONFIRM = "confirmation.confirm"

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
