// SPDX-License-Identifier: MPL-2.0
//
// Nothing on the lock screen (ADR-0013 D8; plan §7.2 item 12). UI state
// tests (fake core): MainActivity runs on the fake core with temporary
// folders, and an offer shows only because the scenario pushed it; these
// results say nothing about the real core. The lock test changes the
// device's lock screen setting for its duration and puts it back.
package io.github.dropumi.prompt

import android.app.KeyguardManager
import android.os.ParcelFileDescriptor
import android.os.PowerManager
import androidx.compose.ui.test.ComposeTimeoutException
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.MainActivity
import io.github.dropumi.UseScenario
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.testsupport.FakeCoreClient.Call
import io.github.dropumi.testsupport.Scenario
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class LockScreenTest {
    private val fake = FakeScenarioRule()
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    private fun answers(): List<Call> = fake.core.calls.filter { it is Call.AnswerOffer || it is Call.AnswerCode }

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    /** Launches MainActivity and waits for [tag] and for the window's focus. */
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

    private fun assertNothingAnswered(why: String) {
        assertTrue(why, answers().isEmpty())
        assertTrue(why, model.state.value.answering.isEmpty())
    }

    /** Runs [command] as the shell user and returns what it printed. */
    private fun shell(command: String): String {
        val pfd = instrumentation.uiAutomation.executeShellCommand(command)
        return ParcelFileDescriptor.AutoCloseInputStream(pfd).bufferedReader().use { it.readText() }
    }

    /** [focus] (an `mCurrentFocus=` line) names the lock screen's window: NotificationShade from API 30, StatusBar before. */
    private fun isLockScreen(focus: String): Boolean = LOCK_SCREEN_WINDOWS.any { focus.trimEnd().endsWith(" $it}") }

    /**
     * The window that has the keys, as `dumpsys window` names it (its
     * `windows` section leaves the line out while the lock screen shows).
     */
    private fun focusedWindow(): String =
        shell("dumpsys window").lineSequence().firstOrNull { it.trim().startsWith("mCurrentFocus=") }.orEmpty()

    /**
     * The window asks for none of the legacy lock-screen flags while a
     * prompt shows. This alone proves nothing on API 27+, where
     * `android:showWhenLocked` and `setShowWhenLocked` set no window flag:
     * [promptNeverShowsOverTheLockScreen] locks the device for that.
     */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun promptWindowRequestsNoLockScreenFlags() {
        launchShowing("prompt.offer").use { scenario ->
            scenario.onActivity {
                @Suppress("DEPRECATION")
                val lockFlags = android.view.WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or
                    android.view.WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON or
                    android.view.WindowManager.LayoutParams.FLAG_DISMISS_KEYGUARD
                assertEquals(0, it.window.attributes.flags and lockFlags)
            }
        }
    }

    /**
     * Never over the lock screen (plan §7.2 item 12), however it would be
     * asked for (the legacy window flags, `android:showWhenLocked`,
     * `setShowWhenLocked`): with an offer waiting, the screen goes off and
     * on again. The lock screen, not the prompt, gets the keys; MainActivity
     * is not resumed and has no window focus; nothing is answered. Then the
     * device is unlocked and the prompt is back, still waiting.
     *
     * Needs a device without a secure lock screen, which `wm
     * dismiss-keyguard` can unlock again; it fails, never skips, on one with
     * a PIN. Where the lock screen is disabled (the AVDs), it is enabled for
     * this test and disabled again after it. API 26-27 have no `locksettings
     * get-disabled`: there the setting shows only by locking, so a lock
     * screen that does not come up is enabled the same way.
     */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun promptNeverShowsOverTheLockScreen() {
        val context = instrumentation.targetContext
        val keyguard = requireNotNull(context.getSystemService(KeyguardManager::class.java))
        val power = requireNotNull(context.getSystemService(PowerManager::class.java))
        assertFalse("needs a device without a secure lock screen (it must be unlocked again)", keyguard.isDeviceSecure)
        // true or false; null where the command does not exist (API 26-27).
        var lockDisabled = when (shell("locksettings get-disabled").trim()) {
            "true" -> true
            "false" -> false
            else -> null
        }
        launchShowing("prompt.offer").use { scenario ->
            try {
                if (lockDisabled == true) shell("locksettings set-disabled false")
                sleepAndWake(power)
                if (lockDisabled == null) {
                    val locks = try {
                        compose.waitUntil(PROBE_MS) { keyguard.isKeyguardLocked }
                        true
                    } catch (_: ComposeTimeoutException) {
                        false
                    }
                    lockDisabled = !locks
                    if (!locks) {
                        shell("locksettings set-disabled false")
                        sleepAndWake(power)
                    }
                }
                // The lock screen comes up and its window gets the keys.
                var seen = ""
                try {
                    compose.waitUntil(LOCK_TIMEOUT_MS) {
                        val focus = focusedWindow()
                        seen = "locked=${keyguard.isKeyguardLocked} ${focus.trim()}"
                        keyguard.isKeyguardLocked && isLockScreen(focus)
                    }
                } catch (e: ComposeTimeoutException) {
                    val state = (shell("dumpsys window") + shell("dumpsys activity activities")).lines()
                        .filter { line -> DIAGNOSTIC_LINES.any { it in line } }
                        .joinToString("\n") { it.trim() }
                    throw AssertionError("the lock screen never had the keys: $seen\n$state", e)
                }
                // The lock screen can have the keys for a moment before an
                // activity shown over it resumes: let the screen settle (no
                // accessibility events for a second; resuming would send
                // some) before checking that nothing did.
                instrumentation.uiAutomation.waitForIdle(SETTLE_MS, LOCK_TIMEOUT_MS)
                val focus = focusedWindow()
                assertTrue("the lock screen lost the keys: ${focus.trim()}", isLockScreen(focus))
                assertNotEquals(Lifecycle.State.RESUMED, scenario.state)
                var focused = true
                scenario.onActivity { focused = it.hasWindowFocus() }
                assertFalse("MainActivity has window focus under the lock screen", focused)
                assertNothingAnswered("answered under the lock screen")
            } finally {
                shell("input keyevent KEYCODE_WAKEUP")
                shell("wm dismiss-keyguard")
                if (lockDisabled == true) shell("locksettings set-disabled true")
            }
            compose.waitUntil(TIMEOUT_MS) { !keyguard.isKeyguardLocked }
            compose.waitUntil(TIMEOUT_MS) {
                var focused = false
                scenario.onActivity { focused = it.hasWindowFocus() }
                focused && scenario.state == Lifecycle.State.RESUMED
            }
            compose.onNodeWithTag("prompt.offer").assertExists()
            compose.onNodeWithTag("offer.decline").assertIsEnabled()
            assertNothingAnswered("answered on unlock")
        }
    }

    /** Turns the screen off, then on again. */
    private fun sleepAndWake(power: PowerManager) {
        shell("input keyevent KEYCODE_SLEEP")
        compose.waitUntil(TIMEOUT_MS) { !power.isInteractive }
        shell("input keyevent KEYCODE_WAKEUP")
        compose.waitUntil(TIMEOUT_MS) { power.isInteractive }
    }

    private companion object {
        const val TIMEOUT_MS = 10_000L

        /** How long a lock screen that is enabled takes at most to come up (API 26-27, which cannot be asked). */
        const val PROBE_MS = 5_000L

        /** Long enough for the lock screen to come up on a slow emulator. */
        const val LOCK_TIMEOUT_MS = 20_000L

        /** How long the screen must stay free of accessibility events to count as settled. */
        const val SETTLE_MS = 1_000L

        /** The lock screen's window: NotificationShade from API 30, StatusBar before. */
        val LOCK_SCREEN_WINDOWS = listOf("NotificationShade", "StatusBar")

        /** What a failure reports from the window and activity dumps. */
        val DIAGNOSTIC_LINES = listOf("mCurrentFocus", "mFocusedApp", "Keyguard", "ccluded", "mResumed", "topResumed")
    }
}
