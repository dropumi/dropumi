// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * ADR-0013 D8, plan §7.2 items 3 and 5: on a prompt or a trust/policy
 * confirmation, Escape dismisses, and an activation key acts only on a
 * button the user moved focus to with a navigation key.
 */
class KeyGateTest {
    private val prompt = KeySurface.Prompt(1u)

    private fun KeyGate.key(key: GateKey, down: Boolean = true, surface: KeySurface = prompt, canceled: Boolean = false) =
        onKey(surface, key, down, canceled)

    /** A navigation key, dispatched as MainActivity does: [moves] runs inside the dispatch. */
    private fun KeyGate.navigate(surface: KeySurface = prompt, moves: () -> Unit = {}) {
        assertEquals(GateAction.NAVIGATE, key(GateKey.NAVIGATE, surface = surface))
        navigating(moves)
        assertEquals(GateAction.PASS, key(GateKey.NAVIGATE, down = false, surface = surface))
    }

    private fun KeyGate.assertEnterConsumed(surface: KeySurface = prompt) {
        assertEquals(GateAction.CONSUME, key(GateKey.ACTIVATE, surface = surface))
        assertEquals(GateAction.CONSUME, key(GateKey.ACTIVATE, down = false, surface = surface))
    }

    private fun KeyGate.assertEnterPasses(surface: KeySurface = prompt) {
        assertEquals(GateAction.PASS, key(GateKey.ACTIVATE, surface = surface))
        assertEquals(GateAction.PASS, key(GateKey.ACTIVATE, down = false, surface = surface))
    }

    @Test
    fun escapeDismissesOnRelease() {
        val gate = KeyGate()
        assertEquals(GateAction.CONSUME, gate.key(GateKey.ESCAPE))
        assertEquals(GateAction.DISMISS, gate.key(GateKey.ESCAPE, down = false))
        assertEquals(GateAction.CONSUME, gate.key(GateKey.ESCAPE, down = false, canceled = true))
    }

    @Test
    fun escapeDismissesAConfirmationToo() {
        val panel = KeySurface.Panel(confirmation())
        assertEquals(GateAction.DISMISS, KeyGate().key(GateKey.ESCAPE, down = false, surface = panel))
    }

    @Test
    fun activationKeysNeverActWithoutNavigation() {
        val gate = KeyGate()
        repeat(3) { gate.assertEnterConsumed() }
    }

    @Test
    fun focusTheUserMovedToAButtonUnlocksIt() {
        val gate = KeyGate()
        gate.navigate { gate.onButtonFocus(prompt, ACCEPT, focused = true) }
        gate.assertEnterPasses()
    }

    /** Page Down, an arrow at the edge: the key moved no focus, so nothing is unlocked. */
    @Test
    fun navigationKeyThatMovesNoFocusUnlocksNothing() {
        val gate = KeyGate()
        repeat(3) { gate.navigate() }
        gate.assertEnterConsumed()
    }

    /** Leaving touch mode or regaining window focus can focus Accept by itself: that is not the user's move. */
    @Test
    fun focusTheSystemGaveNeverUnlocks() {
        val gate = KeyGate()
        gate.onButtonFocus(prompt, ACCEPT, focused = true)
        gate.assertEnterConsumed()
        // Nor does a later navigation key that leaves focus where the system put it.
        gate.navigate()
        gate.assertEnterConsumed()
    }

    @Test
    fun systemFocusAfterTheUsersMoveLocksAgain() {
        val gate = KeyGate()
        gate.navigate { gate.onButtonFocus(prompt, DECLINE, focused = true) }
        gate.assertEnterPasses()
        // Focus moves to Accept outside any navigation key.
        gate.onButtonFocus(prompt, DECLINE, focused = false)
        gate.onButtonFocus(prompt, ACCEPT, focused = true)
        gate.assertEnterConsumed()
    }

    @Test
    fun losingFocusLocksAgain() {
        val gate = KeyGate()
        gate.navigate { gate.onButtonFocus(prompt, ACCEPT, focused = true) }
        gate.onButtonFocus(prompt, ACCEPT, focused = false)
        gate.assertEnterConsumed()
    }

    @Test
    fun resetLocksAgain() {
        val gate = KeyGate()
        gate.navigate { gate.onButtonFocus(prompt, ACCEPT, focused = true) }
        gate.reset()
        gate.assertEnterConsumed()
    }

    @Test
    fun navigationCountsOnlyForTheSurfaceItHappenedOn() {
        val gate = KeyGate()
        gate.navigate { gate.onButtonFocus(prompt, ACCEPT, focused = true) }
        gate.assertEnterConsumed(surface = KeySurface.Prompt(2u))
        // And not back on the first prompt either.
        gate.assertEnterConsumed()
    }

    @Test
    fun focusReportedForAnotherSurfaceUnlocksNothing() {
        val gate = KeyGate()
        val panel = KeySurface.Panel(confirmation())
        gate.navigate { gate.onButtonFocus(panel, CONFIRM, focused = true) }
        gate.assertEnterConsumed()
    }

    /** Two confirmations that read the same are still two questions. */
    @Test
    fun aNewConfirmationIsANewSurface() {
        val first = KeySurface.Panel(confirmation())
        val second = KeySurface.Panel(confirmation())
        val gate = KeyGate()
        gate.navigate(first) { gate.onButtonFocus(first, CONFIRM, focused = true) }
        gate.assertEnterPasses(first)
        gate.assertEnterConsumed(second)
    }

    @Test
    fun otherKeysPass() {
        assertEquals(GateAction.PASS, KeyGate().key(GateKey.OTHER))
    }

    /** Only Tab and the arrows are navigation: a focus move by Page Up/Down (Compose makes one) or Home/End never counts. */
    @Test
    fun keyCodes() {
        assertEquals(GateKey.ESCAPE, GateKey.of(KeyEvent.KEYCODE_ESCAPE))
        for (code in listOf(KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER, KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_SPACE)) {
            assertEquals(GateKey.ACTIVATE, GateKey.of(code))
        }
        val navigation = listOf(
            KeyEvent.KEYCODE_TAB,
            KeyEvent.KEYCODE_DPAD_UP,
            KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_DPAD_LEFT,
            KeyEvent.KEYCODE_DPAD_RIGHT,
        )
        for (code in navigation) assertEquals(GateKey.NAVIGATE, GateKey.of(code))
        val other = listOf(
            KeyEvent.KEYCODE_PAGE_UP,
            KeyEvent.KEYCODE_PAGE_DOWN,
            KeyEvent.KEYCODE_MOVE_HOME,
            KeyEvent.KEYCODE_MOVE_END,
            KeyEvent.KEYCODE_A,
        )
        for (code in other) assertEquals(GateKey.OTHER, GateKey.of(code))
    }

    private companion object {
        const val ACCEPT = "offer.accept"
        const val DECLINE = "offer.decline"
        const val CONFIRM = "confirmation.confirm"

        fun confirmation() = Confirmation("Let everyone send?", "message", "Allow") {}
    }
}
