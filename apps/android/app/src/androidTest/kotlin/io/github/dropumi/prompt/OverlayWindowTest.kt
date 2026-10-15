// SPDX-License-Identifier: MPL-2.0
//
// Overlay protection (R23, ADR-0013 D8) against a real overlay window of
// another app. UI state tests (fake core).
//
// The window comes from the test-only :overlayhelper APK (another UID).
// scripts/android/ui-tests.sh installs it before the run, writes a random
// per-run token into the helper's private files and passes the same token
// as the runner argument `overlay_token`; its exit trap resets the helper's
// SYSTEM_ALERT_WINDOW app-op and uninstalls it. The helper's service is
// exported only to the shell (DUMP-guarded) and draws only with that token.
// Without the helper or the token these tests fail; they never skip.
//
// Touches are injected through the input system (UiAutomation), so Android
// decides what reaches the app, as it would for a finger:
// - API 26-30: a touch through an overlay arrives flagged
//   FLAG_WINDOW_IS_OBSCURED (29-30 also PARTIALLY_OBSCURED for a touch
//   outside a partial overlay) and Dropumi drops it on a prompt or a
//   confirmation, and ignores it on Send;
// - API 31+: other apps' overlays are hidden while a prompt or a
//   confirmation shows; elsewhere the system lets a touch through an
//   overlay only up to 0.8 opacity, flagged obscured.
// What happened on the image is reported as the instrumentation status
// `dropumi_overlay` (am instrument -r), so a run records it per image.
package io.github.dropumi.prompt

import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.view.InputDevice
import android.view.MotionEvent
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
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
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.PromptCloseReason
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.shell
import io.github.dropumi.testsupport.FakeCoreClient.Call
import io.github.dropumi.testsupport.Fixtures
import io.github.dropumi.testsupport.Scenario
import io.github.dropumi.ui.OverlayNotice
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class OverlayWindowTest {
    private val fake = FakeScenarioRule()
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    private lateinit var token: String

    /**
     * The overlay window: not there, there but not drawn yet, drawn on
     * screen, or hidden because an app asked for overlays to be hidden
     * (API 31+, the window manager's force-hide flag).
     */
    private enum class Overlay { ABSENT, PENDING, SHOWN, HIDDEN }

    @Before
    fun requireHelper() {
        token = InstrumentationRegistry.getArguments().getString(TOKEN_ARGUMENT)
            ?: throw AssertionError("no $TOKEN_ARGUMENT: run these tests with scripts/android/ui-tests.sh")
        if (!shell("pm path $HELPER").trim().startsWith("package:")) {
            fail("the overlay helper ($HELPER) is not installed: run these tests with scripts/android/ui-tests.sh")
        }
        shell("appops set $HELPER SYSTEM_ALERT_WINDOW allow")
    }

    @After
    fun removeOverlay() {
        shell("am stopservice -n $HELPER/.OverlayService")
        compose.waitUntil(TIMEOUT_MS) { overlay() == Overlay.ABSENT }
        shell("appops set $HELPER SYSTEM_ALERT_WINDOW default")
    }

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

    /** [tag]'s bounds on the screen, in pixels. */
    private fun ActivityScenario<MainActivity>.screenBounds(tag: String): Rect {
        val inWindow = compose.onNodeWithTag(tag).fetchSemanticsNode().boundsInWindow
        val origin = IntArray(2)
        onActivity { it.window.decorView.getLocationOnScreen(origin) }
        return inWindow.translate(origin[0].toFloat(), origin[1].toFloat())
    }

    /** The helper's window, from the window manager's own dump. */
    private fun overlay(): Overlay {
        val dump = shell("dumpsys window windows")
        val start = dump.indexOf(" u0 $HELPER}")
        if (start < 0) return Overlay.ABSENT
        val end = dump.indexOf("Window #", start).takeIf { it > 0 } ?: dump.length
        val window = dump.substring(start, end)
        return when {
            FORCE_HIDDEN.containsMatchIn(window) -> Overlay.HIDDEN
            window.contains("Surface: shown=true") -> Overlay.SHOWN
            else -> Overlay.PENDING
        }
    }

    /**
     * Asks the helper for an overlay (`over-accept` or `partial` at [bounds],
     * or `full-dim`) and waits until the window manager has settled it:
     * shown, or hidden (API 31+, while a prompt or a confirmation shows).
     */
    private fun showOverlay(layout: String, bounds: Rect? = null): Overlay {
        val place = bounds?.let {
            " --ei left ${it.left.toInt()} --ei top ${it.top.toInt()} --ei width ${it.width.toInt()} --ei height ${it.height.toInt()}"
        }.orEmpty()
        val out = shell("am start-foreground-service -n $HELPER/.OverlayService --es layout $layout --es token $token$place")
        assertFalse("the helper's service did not start: $out", out.contains("Error"))
        var state = Overlay.ABSENT
        compose.waitUntil(TIMEOUT_MS) {
            state = overlay()
            state == Overlay.SHOWN || state == Overlay.HIDDEN
        }
        return state
    }

    /** A tap at the centre of [bounds] (screen pixels), through the input system as a finger's would go. */
    private fun injectTap(bounds: Rect) {
        val x = bounds.center.x
        val y = bounds.center.y
        val down = SystemClock.uptimeMillis()
        for ((action, time) in listOf(MotionEvent.ACTION_DOWN to down, MotionEvent.ACTION_UP to down + TAP_MS)) {
            val event = MotionEvent.obtain(down, time, action, x, y, 0)
            event.source = InputDevice.SOURCE_TOUCHSCREEN
            try {
                assertTrue("the tap was not injected", instrumentation.uiAutomation.injectInputEvent(event, true))
            } finally {
                event.recycle()
            }
        }
        instrumentation.waitForIdleSync()
        compose.waitForIdle()
    }

    private fun report(outcome: String) {
        instrumentation.sendStatus(STATUS_IN_PROGRESS, Bundle().apply { putString(STATUS_KEY, "API ${Build.VERSION.SDK_INT}: $outcome") })
    }

    /** The overlay notice on a prompt or a confirmation ("Close it to answer"). */
    private fun answerNoticeShown(): Boolean =
        compose.onAllNodesWithTag("overlay.notice").fetchSemanticsNodes().any {
            it.config.getOrNull(SemanticsProperties.Text)?.joinToString { t -> t.text } == OverlayNotice.ANSWER.text
        }

    /**
     * A prompt for each layout: a pass-through overlay over Accept (every
     * level), and from API 29 a partial overlay over the peer's name with
     * the tap on Accept outside it. API 26-28 have no flag for a partial
     * overlay; that layout is the residual the threat model discloses for
     * them, so it is not tapped there. A tap never accepts while the overlay
     * is on screen: it is dropped (arrives obscured), or the system blocks
     * it. Only an overlay the system hid (31+) lets the tap through, because
     * the user then sees the real prompt.
     */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun obscuredTouchNeverAccepts() {
        launchShowing("prompt.offer").use { scenario ->
            val layouts = buildList {
                add("over-accept")
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) add("partial") else report("partial: not run, no partial flag before API 29 (disclosed residual)")
            }
            var promptId = 1uL
            for (layout in layouts) {
                if (promptId > 1uL) {
                    fake.core.emit(NodeEvent.PromptClosed(promptId - 1uL, PromptCloseReason.WITHDRAWN), NodeEvent.PromptOpened(Fixtures.offer(id = promptId)))
                    compose.waitUntil(TIMEOUT_MS) { (model.state.value.currentPrompt as? Prompt.Offer)?.id == promptId }
                }
                arm()
                compose.onNodeWithTag("offer.accept").assertIsEnabled()
                val accept = scenario.screenBounds("offer.accept")
                val covered = if (layout == "over-accept") accept else scenario.screenBounds("prompt.peer")
                val state = showOverlay(layout, covered)
                injectTap(accept)
                val accepted = Call.AnswerOffer(promptId, true) in fake.core.calls
                val outcome = when {
                    state == Overlay.HIDDEN && accepted -> "overlay hidden while the prompt shows; the tap reached Accept"
                    accepted -> throw AssertionError("$layout: a tap through a visible overlay accepted the offer")
                    answerNoticeShown() -> "touch arrived obscured and was dropped"
                    else -> "touch blocked by the system"
                }
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
                    assertEquals("$layout on API ${Build.VERSION.SDK_INT}", "touch arrived obscured and was dropped", outcome)
                }
                report("obscuredTouchNeverAccepts $layout ($state): $outcome")
                shell("am stopservice -n $HELPER/.OverlayService")
                compose.waitUntil(TIMEOUT_MS) { overlay() == Overlay.ABSENT }
                promptId++
            }
        }
    }

    /**
     * API 31+: an overlay is hidden while a prompt shows, and shows again
     * once the core closes the prompt. API 26-30 cannot hide it (the
     * disclosed residual): there it stays on screen throughout.
     */
    @Test
    fun overlayHiddenWhilePromptShown() {
        val hides = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        launchShowing("tab.nearby").use {
            assertEquals(Overlay.SHOWN, showOverlay("full-dim"))
            fake.core.emit(NodeEvent.PromptOpened(Fixtures.offer(id = 7uL)))
            compose.waitUntil(TIMEOUT_MS) { exists("prompt.offer") }
            if (hides) compose.waitUntil(TIMEOUT_MS) { overlay() == Overlay.HIDDEN }
            compose.waitForIdle()
            assertEquals(if (hides) Overlay.HIDDEN else Overlay.SHOWN, overlay())
            fake.core.emit(NodeEvent.PromptClosed(7uL, PromptCloseReason.WITHDRAWN))
            compose.waitUntil(TIMEOUT_MS) { !exists("prompt.offer") }
            compose.waitUntil(TIMEOUT_MS) { overlay() == Overlay.SHOWN }
            report(if (hides) "overlayHiddenWhilePromptShown: hidden, then shown again" else "overlayHiddenWhilePromptShown: not hidden (API 26-30 residual)")
        }
    }

    /** Who can send → Everyone: a tap through an overlay on Confirm never changes the policy. */
    @Test
    fun obscuredTouchNeverEnablesEveryone() {
        launchShowing("tab.settings").use { scenario ->
            compose.onNodeWithTag("tab.settings").performClick()
            compose.onNodeWithText("Everyone · 10 min").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("confirmation") }
            val confirm = scenario.screenBounds("confirmation.confirm")
            val state = showOverlay("over-accept", confirm)
            injectTap(confirm)
            val enabled = model.state.value.visibility == ReceiveVisibility.EVERYONE ||
                fake.core.calls.any {
                    (it is Call.SetReceivePolicy && it.policy is ReceivePolicy.Everyone) ||
                        (it is Call.StartReceiving && it.options.policy is ReceivePolicy.Everyone)
                }
            val outcome = when {
                state == Overlay.HIDDEN && enabled -> "overlay hidden while the confirmation shows; the tap reached Confirm"
                enabled -> throw AssertionError("a tap through a visible overlay let everyone send")
                answerNoticeShown() -> "touch arrived obscured and was dropped"
                else -> "touch blocked by the system"
            }
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
                assertEquals("touch arrived obscured and was dropped", outcome)
                assertTrue(exists("confirmation"))
            }
            report("obscuredTouchNeverEnablesEveryone ($state): $outcome")
        }
    }

    /** A tap through an overlay on Send never sends; where it reaches the app, it says why. */
    @Test
    fun obscuredTouchNeverSends() {
        fake.app.graph.pendingShare.put(listOf(SHARED))
        fake.core.emit(NodeEvent.PeerFound(Fixtures.nearby("Maya's phone", "dropumi-00000001")))
        launchShowing("send.target").use { scenario ->
            compose.onNodeWithTag("send.target").performClick()
            compose.waitUntil(TIMEOUT_MS) {
                compose.onNodeWithTag("send.go").fetchSemanticsNode().config.getOrNull(SemanticsProperties.Disabled) == null
            }
            compose.onNodeWithTag("send.go").performScrollTo()
            compose.waitForIdle()
            val send = scenario.screenBounds("send.go")
            val state = showOverlay("over-accept", send)
            assertEquals("Send is not a surface that hides overlays", Overlay.SHOWN, state)
            injectTap(send)
            assertTrue("a tap through an overlay sent", fake.core.calls.none { it is Call.Send })
            val noticed = compose.onAllNodesWithText(OverlayNotice.SEND.text).fetchSemanticsNodes().isNotEmpty()
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) assertTrue("the Send notice shows", noticed)
            report("obscuredTouchNeverSends: " + if (noticed) "touch arrived obscured and was ignored" else "touch blocked by the system")
        }
    }

    /**
     * An always-on, see-through overlay (a screen dimmer) never locks the
     * app: taps between Nearby, Transfers and Settings still work, and only
     * the non-blocking notice shows.
     */
    @Test
    fun normalScreenUsableUnderOverlay() {
        launchShowing("tab.nearby").use { scenario ->
            assertEquals(Overlay.SHOWN, showOverlay("full-dim"))
            for ((tab, shows) in listOf("tab.transfers" to "transfers.empty", "tab.settings" to "settings.visibility", "tab.nearby" to "nearby.empty")) {
                injectTap(scenario.screenBounds(tab))
                compose.waitUntil(TIMEOUT_MS) { exists(shows) }
            }
            compose.onNodeWithText(OverlayNotice.GENERAL.text).assertExists()
            assertFalse(exists("prompt.offer"))
            report("normalScreenUsableUnderOverlay: taps went through, with the notice")
        }
    }

    private companion object {
        const val HELPER = "io.github.dropumi.overlayhelper"
        const val TOKEN_ARGUMENT = "overlay_token"
        const val TIMEOUT_MS = 15_000L
        const val ARMED_AFTER_MS = 700L
        const val TAP_MS = 50L
        const val SHARED = "content://io.github.dropumi.test.documents/document/1"

        /** The window manager's force-hide flag for other apps' overlays (its name differs by API level). */
        val FORCE_HIDDEN = Regex("""ForceHid\w*NonSystemOverlayWindow=true""")

        /** Not a test result: AndroidJUnitRunner's "in progress" code, which result parsers ignore. */
        const val STATUS_IN_PROGRESS = 2
        const val STATUS_KEY = "dropumi_overlay"
    }
}
