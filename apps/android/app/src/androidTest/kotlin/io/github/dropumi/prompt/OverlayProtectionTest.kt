// SPDX-License-Identifier: MPL-2.0
//
// Overlay protection (R23, ADR-0013 D8) at MainActivity's touch entry
// point. UI state tests (fake core). Each test hands
// MainActivity.dispatchTouchEvent the flags Android puts on a touch that
// came through another app's window (FLAG_WINDOW_IS_OBSCURED, and from API
// 29 FLAG_WINDOW_IS_PARTIALLY_OBSCURED), then the same touch without them
// as a control, so a refusal is never the result of a tap that missed.
// What Android does with a real overlay on a given image (whether it sets
// the flags, hides the overlay or blocks the touch) is the job of the
// overlay-helper test, not of these.
package io.github.dropumi.prompt

import android.os.Build
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import android.view.ViewGroup
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.MainActivity
import io.github.dropumi.UseScenario
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.PromptCloseReason
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.testsupport.FakeCoreClient.Call
import io.github.dropumi.testsupport.Fixtures
import io.github.dropumi.testsupport.Scenario
import io.github.dropumi.ui.OverlayNotice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class OverlayProtectionTest {
    private val fake = FakeScenarioRule()
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

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

    private fun arm() {
        compose.mainClock.advanceTimeBy(ARMED_AFTER_MS)
        compose.waitForIdle()
    }

    /** A tap (DOWN, UP) at the centre of [tag], through MainActivity.dispatchTouchEvent, with [flags] on both events. */
    private fun ActivityScenario<MainActivity>.tap(tag: String, flags: Int) {
        val centre = compose.onNodeWithTag(tag).fetchSemanticsNode().boundsInWindow.center
        onActivity { activity ->
            val downTime = SystemClock.uptimeMillis()
            for ((action, time) in listOf(MotionEvent.ACTION_DOWN to downTime, MotionEvent.ACTION_UP to downTime + 40)) {
                val event = touch(downTime, time, action, centre.x, centre.y, flags)
                try {
                    activity.dispatchTouchEvent(event)
                } finally {
                    event.recycle()
                }
            }
        }
        compose.waitForIdle()
    }

    private fun ActivityScenario<MainActivity>.rootFiltersObscuredTouches(): Boolean {
        var filtering = false
        onActivity { filtering = it.findViewById<ViewGroup>(android.R.id.content).getChildAt(0).filterTouchesWhenObscured }
        return filtering
    }

    private fun assertNotice(notice: OverlayNotice) = compose.onNodeWithTag("overlay.notice").assertTextEquals(notice.text)

    /** The obscured flags this image can set: the full one everywhere, the partial one from API 29. */
    private fun obscuredFlags(): List<Int> = buildList {
        add(MotionEvent.FLAG_WINDOW_IS_OBSCURED)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) add(MotionEvent.FLAG_WINDOW_IS_PARTIALLY_OBSCURED)
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun obscuredTouchNeverAcceptsAnOffer() {
        launchShowing("prompt.offer").use { scenario ->
            arm()
            // Accept would act on a clean tap: a refusal below is the overlay's doing.
            compose.onNodeWithTag("offer.accept").assertIsEnabled()
            assertTrue(scenario.rootFiltersObscuredTouches())
            for (flags in obscuredFlags()) {
                scenario.tap("offer.accept", flags)
                val s = model.state.value
                assertTrue("flags $flags reached Accept", s.answering.isEmpty() && s.answered.isEmpty())
                assertTrue(fake.core.calls.none { it is Call.AnswerOffer })
                assertNotice(OverlayNotice.ANSWER)
            }
            scenario.tap("offer.accept", 0)
            compose.waitUntil(TIMEOUT_MS) { Call.AnswerOffer(1u, true) in fake.core.calls }
        }
    }

    @Test
    @UseScenario(Scenario.CODE_PAIRING)
    fun obscuredTouchNeverConfirmsACode() {
        launchShowing("prompt.code").use { scenario ->
            arm()
            compose.onNodeWithTag("code.match").assertIsEnabled()
            for (flags in obscuredFlags()) {
                scenario.tap("code.match", flags)
                assertTrue(model.state.value.answering.isEmpty() && model.state.value.answered.isEmpty())
            }
            assertTrue(fake.core.calls.none { it is Call.AnswerCode })
            scenario.tap("code.match", 0)
            compose.waitUntil(TIMEOUT_MS) { Call.AnswerCode(2u, true) in fake.core.calls }
        }
    }

    /** Who can send → Everyone is confirmed in an in-window panel that drops obscured touches too. */
    @Test
    fun obscuredTouchNeverEnablesEveryone() {
        launchShowing("tab.settings").use { scenario ->
            compose.onNodeWithTag("tab.settings").performClick()
            compose.onNodeWithText("Everyone · 10 min").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("confirmation") }
            assertTrue(scenario.rootFiltersObscuredTouches())
            val hides = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
            if (hides) assertTrue("overlays hidden while it shows", ourWindowDump().contains(HIDE_OVERLAYS))
            for (flags in obscuredFlags()) {
                scenario.tap("confirmation.confirm", flags)
                assertTrue(exists("confirmation"))
                assertEquals(ReceiveVisibility.PAIRED_ONLY, model.state.value.visibility)
                assertNotice(OverlayNotice.ANSWER)
            }
            assertTrue(fake.core.calls.none { it is Call.StartReceiving && it.options.policy is ReceivePolicy.Everyone })
            assertTrue(fake.core.calls.none { it is Call.SetReceivePolicy && it.policy is ReceivePolicy.Everyone })
            scenario.tap("confirmation.confirm", 0)
            compose.waitUntil(TIMEOUT_MS) { model.state.value.visibility == ReceiveVisibility.EVERYONE }
            compose.waitUntil(TIMEOUT_MS) { !exists("confirmation") }
            assertFalse(scenario.rootFiltersObscuredTouches())
            if (hides) compose.waitUntil(TIMEOUT_MS) { !ourWindowDump().contains(HIDE_OVERLAYS) }
        }
    }

    /** The Send button ignores a click from an obscured gesture and says why; nothing is sent. */
    @Test
    fun obscuredTouchNeverSends() {
        fake.app.graph.pendingShare.put(listOf(SHARED))
        fake.core.emit(NodeEvent.PeerFound(Fixtures.nearby("Maya's phone", "dropumi-00000001")))
        launchShowing("send.target").use { scenario ->
            compose.onNodeWithTag("send.target").performClick()
            // Send would act on a clean tap (a device chosen, the draft ready).
            compose.waitUntil(TIMEOUT_MS) {
                compose.onNodeWithTag("send.go").fetchSemanticsNode().config.getOrNull(SemanticsProperties.Disabled) == null
            }
            // On a phone the button can lie below the fold; a raw touch needs it on screen.
            compose.onNodeWithTag("send.go").performScrollTo()
            assertFalse("Send is not protected by the drop", scenario.rootFiltersObscuredTouches())
            for (flags in obscuredFlags()) {
                scenario.tap("send.go", flags)
                assertTrue(fake.core.calls.none { it is Call.Send })
                assertNotice(OverlayNotice.SEND)
            }
            scenario.tap("send.go", 0)
            compose.waitUntil(TIMEOUT_MS) { fake.core.calls.any { it is Call.Send } }
        }
    }

    /** Elsewhere an obscured touch works (an always-on overlay must not lock the app); a notice says so. */
    @Test
    fun obscuredTouchElsewhereGoesThroughWithANotice() {
        launchShowing("tab.settings").use { scenario ->
            assertFalse(scenario.rootFiltersObscuredTouches())
            scenario.tap("tab.settings", MotionEvent.FLAG_WINDOW_IS_OBSCURED)
            compose.waitUntil(TIMEOUT_MS) { exists("settings.visibility") }
            assertNotice(OverlayNotice.GENERAL)
            scenario.tap("tab.nearby", 0)
            compose.waitUntil(TIMEOUT_MS) { !exists("overlay.notice") }
        }
    }

    /** The prompt's protection ends when the core closes the prompt. */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun protectionEndsWithThePrompt() {
        launchShowing("prompt.offer").use { scenario ->
            assertTrue(scenario.rootFiltersObscuredTouches())
            fake.core.emit(NodeEvent.PromptClosed(1u, PromptCloseReason.WITHDRAWN))
            compose.waitUntil(TIMEOUT_MS) { !exists("prompt.offer") }
            assertFalse(scenario.rootFiltersObscuredTouches())
        }
    }

    /**
     * API 31+: other apps' overlay windows are hidden while a prompt shows,
     * and only then (an always-on overlay stays elsewhere). API 26-30 has no
     * way to hide them (the disclosed residual): the flag never appears
     * there. Read from the window manager's own dump of this window.
     */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun overlayWindowsHiddenOnlyWhilePromptShows() {
        val hides = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        launchShowing("prompt.offer").use {
            assertEquals(hides, ourWindowDump().contains(HIDE_OVERLAYS))
            fake.core.emit(NodeEvent.PromptClosed(1u, PromptCloseReason.WITHDRAWN))
            compose.waitUntil(TIMEOUT_MS) { !exists("prompt.offer") }
            compose.waitUntil(TIMEOUT_MS) { !ourWindowDump().contains(HIDE_OVERLAYS) }
        }
    }

    /** This app's MainActivity window, as `dumpsys window windows` prints it. */
    private fun ourWindowDump(): String {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val pfd = instrumentation.uiAutomation.executeShellCommand("dumpsys window windows")
        val dump = ParcelFileDescriptor.AutoCloseInputStream(pfd).bufferedReader().use { it.readText() }
        val marker = instrumentation.targetContext.packageName + "/" + MainActivity::class.java.name + "}:"
        val start = dump.indexOf(marker)
        check(start >= 0) { "MainActivity's window is not in the window dump" }
        val end = dump.indexOf("\n  Window #", start).takeIf { it > 0 } ?: dump.length
        return dump.substring(start, end)
    }

    private companion object {
        const val TIMEOUT_MS = 10_000L
        const val HIDE_OVERLAYS = "HIDE_NON_SYSTEM_OVERLAY_WINDOWS"
        const val ARMED_AFTER_MS = 700L
        const val SHARED = "content://io.github.dropumi.test.documents/document/1"

        fun touch(downTime: Long, time: Long, action: Int, x: Float, y: Float, flags: Int): MotionEvent {
            val properties = MotionEvent.PointerProperties().apply {
                id = 0
                toolType = MotionEvent.TOOL_TYPE_FINGER
            }
            val coords = MotionEvent.PointerCoords().apply {
                this.x = x
                this.y = y
                pressure = 1f
                size = 1f
            }
            return MotionEvent.obtain(
                downTime, time, action, 1, arrayOf(properties), arrayOf(coords),
                0, 0, 1f, 1f, 0, 0, InputDevice.SOURCE_TOUCHSCREEN, flags,
            )
        }
    }
}
