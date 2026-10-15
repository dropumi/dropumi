// SPDX-License-Identifier: MPL-2.0
//
// Hardware keys while a prompt or a trust/policy confirmation is on screen
// (ADR-0013 D8; plan §7.2 items 3 and 5). MainActivity asks this before the
// key reaches Compose:
// - Escape dismisses on its ACTION_UP: it declines the prompt, or cancels
//   the confirmation. Compose would never see it (nothing is focused when
//   either opens), and the platform's Escape-to-Back fallback is not relied
//   on.
// - Enter, NumPad Enter, DPAD centre and Space act only on a button the user
//   moved focus to: a button of the surface on screen that gained focus
//   while MainActivity was dispatching a navigation key (Tab or an arrow)
//   for that surface, and has kept it since. Focus that Android gives by
//   itself (leaving touch mode, the window regaining focus) never counts,
//   and neither does a navigation key that moved no focus (an arrow at the
//   edge). Page Up/Down, Home and End are not navigation keys here: a focus
//   move they cause (Compose moves focus up or down for Page Up/Down) never
//   counts either.
// MainActivity also forgets the user's move when it resumes or its window
// focus changes, as the arming does: a key meant for what was in front
// cannot land on a button.
package io.github.dropumi.ui

import android.view.KeyEvent
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged

/** What the keys act on: a prompt, or one confirmation panel. */
sealed interface KeySurface {
    data class Prompt(val id: ULong) : KeySurface

    /** Two confirmations that read the same are still two questions: equal only to itself. */
    class Panel(val confirmation: Confirmation) : KeySurface {
        override fun equals(other: Any?): Boolean = other is Panel && other.confirmation === confirmation

        override fun hashCode(): Int = System.identityHashCode(confirmation)
    }
}

/** A key, as far as the gate is concerned. */
enum class GateKey {
    ESCAPE,

    /** Enter, NumPad Enter, DPAD centre, Space. */
    ACTIVATE,

    /** Tab and the arrows: the keys whose focus moves count as the user's. */
    NAVIGATE,
    OTHER,
    ;

    companion object {
        fun of(keyCode: Int): GateKey = when (keyCode) {
            KeyEvent.KEYCODE_ESCAPE -> ESCAPE
            KeyEvent.KEYCODE_ENTER,
            KeyEvent.KEYCODE_NUMPAD_ENTER,
            KeyEvent.KEYCODE_DPAD_CENTER,
            KeyEvent.KEYCODE_SPACE,
            -> ACTIVATE
            KeyEvent.KEYCODE_TAB,
            KeyEvent.KEYCODE_DPAD_UP,
            KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_DPAD_LEFT,
            KeyEvent.KEYCODE_DPAD_RIGHT,
            -> NAVIGATE
            else -> OTHER
        }
    }
}

/** What MainActivity does with a key while a prompt or a confirmation shows. */
enum class GateAction {
    /** Let it through to the window. */
    PASS,

    /** Let it through to the window inside [KeyGate.navigating]. */
    NAVIGATE,

    /** Swallow it. */
    CONSUME,

    /** Swallow it and decline the prompt or cancel the confirmation. */
    DISMISS,
}

class KeyGate {
    private var surface: KeySurface? = null
    private var navigating = false

    /** The button of [surface] the user moved focus to, while it keeps focus. */
    private var userFocused: String? = null

    /**
     * @param surface what is on screen: the prompt, or else the confirmation.
     * @param down the key's ACTION_DOWN (otherwise its ACTION_UP).
     * @param canceled the key event was cancelled.
     */
    fun onKey(surface: KeySurface, key: GateKey, down: Boolean, canceled: Boolean): GateAction {
        if (surface != this.surface) {
            this.surface = surface
            userFocused = null
        }
        return when (key) {
            GateKey.ESCAPE -> if (!down && !canceled) GateAction.DISMISS else GateAction.CONSUME
            GateKey.ACTIVATE -> if (userFocused != null) GateAction.PASS else GateAction.CONSUME
            GateKey.NAVIGATE -> if (down) GateAction.NAVIGATE else GateAction.PASS
            GateKey.OTHER -> GateAction.PASS
        }
    }

    /**
     * Runs [dispatch], the window's handling of a navigation key that
     * [onKey] answered with [GateAction.NAVIGATE]. A button that gains focus
     * during it (Compose reports focus changes synchronously) is the user's
     * move.
     */
    fun <T> navigating(dispatch: () -> T): T {
        navigating = true
        try {
            return dispatch()
        } finally {
            navigating = false
        }
    }

    /** A button of [surface], named [button], gained or lost focus. */
    fun onButtonFocus(surface: KeySurface, button: String, focused: Boolean) {
        if (surface != this.surface) return
        if (focused) {
            userFocused = if (navigating) button else null
        } else if (userFocused == button) {
            userFocused = null
        }
    }

    /** Forgets the user's move: the next activation key needs a new one. */
    fun reset() {
        userFocused = null
    }
}

/** Reports this button's focus to [gate], as [surface]'s button [button]. */
fun Modifier.gatedButton(gate: KeyGate, surface: KeySurface, button: String): Modifier =
    onFocusChanged { gate.onButtonFocus(surface, button, it.isFocused) }

val LocalKeyGate = staticCompositionLocalOf<KeyGate> { error("no KeyGate") }
