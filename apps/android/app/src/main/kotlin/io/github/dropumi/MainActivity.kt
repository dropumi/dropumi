// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi

import android.annotation.SuppressLint
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.lifecycle.lifecycleScope
import io.github.dropumi.appstate.promptId
import io.github.dropumi.ui.Confirmations
import io.github.dropumi.ui.DropumiApp
import io.github.dropumi.ui.GateAction
import io.github.dropumi.ui.GateKey
import io.github.dropumi.ui.KeyGate
import io.github.dropumi.ui.KeySurface
import io.github.dropumi.ui.Router
import io.github.dropumi.ui.TouchDecision
import io.github.dropumi.ui.TouchGuard
import io.github.dropumi.ui.TouchPhase
import kotlinx.coroutines.launch

/**
 * The only activity that draws Dropumi's screens. It is the launcher entry,
 * so any app can start it; it never takes share items from its intent: they
 * come only from the in-process hand-off that ShareActivity writes.
 *
 * Prompt safety (ADR-0013 D8) that only the activity can do:
 * - every touch passes [touchGuard] first (overlay protection, R23); while a
 *   prompt or a confirmation shows, the Compose root also filters obscured
 *   touches and, on API 31+, other apps' overlay windows are hidden;
 * - while a prompt or a confirmation shows, Escape declines or cancels it,
 *   and an activation key acts only on a button the user moved focus to
 *   ([keyGate]), before any key reaches Compose;
 * - the prompt on screen re-arms when the activity resumes and when its
 *   window regains focus, and the user's focus move is forgotten then.
 * It never shows over the lock screen and never turns the screen on. When it
 * pauses (not for a configuration change) running transfers get the
 * transfer service; ProcessVisibility, not the activity, decides foreground
 * and background.
 */
class MainActivity : ComponentActivity() {
    private val graph: AppGraph get() = (application as AppGraphHost).graph

    /** The in-window confirmations (trust and policy changes) on screen, if any. */
    val confirmations = Confirmations()

    /** Overlay protection: what happens to touches that came through another app's window. */
    val touchGuard = TouchGuard()

    /** Escape and activation keys on a prompt or a confirmation. */
    private val keyGate = KeyGate()

    private var protecting = false
    private var darkBars: Boolean? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        // The launch screen is navy: light system bar icons until the theme is known.
        applySystemBars(dark = true)
        super.onCreate(savedInstanceState)
        graph.loader.load()
        setContent {
            val router = rememberSaveable(saver = Router.Saver) { Router() }
            DropumiApp(
                graph,
                router,
                confirmations,
                touchGuard,
                keyGate,
                onDarkTheme = ::applySystemBars,
                onProtecting = ::protect,
            )
        }
    }

    override fun onResume() {
        super.onResume()
        // A key meant for what was in front cannot land on a button.
        keyGate.reset()
        val model = graph.loader.model ?: return
        // A prompt on screen re-arms: a tap meant for what was in front cannot land on it.
        model.promptResumed()
        // Shared items handed over while the activity was away (singleTask:
        // a share brings this instance back instead of starting a new one).
        lifecycleScope.launch { model.takeSharedItems() }
    }

    override fun onPause() {
        super.onPause()
        // Leaving, not rotating: while Dropumi is still visible, the transfer
        // service starts for running transfers that do not have it yet, so
        // they finish after the user has left (ADR-0013 D7).
        if (!isChangingConfigurations) graph.loader.model?.keepTransfersRunning()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        // Another window took the keys, or gave them back: the user moves focus again.
        keyGate.reset()
        // Back from the notification shade, a system dialog or another window: re-arm.
        if (hasFocus) graph.loader.model?.promptFocused()
    }

    override fun dispatchTouchEvent(ev: MotionEvent): Boolean =
        when (touchGuard.onTouch(ev.touchPhase(), ev.isObscured())) {
            TouchDecision.DISPATCH -> super.dispatchTouchEvent(ev)
            TouchDecision.DROP -> true
            TouchDecision.CANCEL_AND_DROP -> {
                // A cancel without the obscured flags, so the Compose root's
                // own filter lets it end the press it saw start.
                val cancel = MotionEvent.obtain(ev.downTime, ev.eventTime, MotionEvent.ACTION_CANCEL, ev.x, ev.y, 0)
                cancel.source = ev.source
                try {
                    super.dispatchTouchEvent(cancel)
                } finally {
                    cancel.recycle()
                }
                true
            }
        }

    // android.app.Activity.dispatchKeyEvent is public platform API, the
    // documented place to see a key before the views do. Lint reports it
    // because androidx.core's ComponentActivity marks its own override of it
    // @RestrictTo; overriding and calling super is still the platform
    // contract, so only that check is suppressed, only here.
    @SuppressLint("RestrictedApi")
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val surface = keySurface() ?: return super.dispatchKeyEvent(event)
        val down = event.action == KeyEvent.ACTION_DOWN
        return when (keyGate.onKey(surface, GateKey.of(event.keyCode), down, event.isCanceled)) {
            GateAction.PASS -> super.dispatchKeyEvent(event)
            // A button that gains focus during this dispatch is the user's move.
            GateAction.NAVIGATE -> keyGate.navigating { super.dispatchKeyEvent(event) }
            GateAction.CONSUME -> true
            GateAction.DISMISS -> {
                dismiss(surface)
                true
            }
        }
    }

    /** What keys act on: the prompt on screen, or else the confirmation on screen. */
    private fun keySurface(): KeySurface? {
        val prompt = graph.loader.model?.state?.value?.currentPrompt
        if (prompt != null) return KeySurface.Prompt(prompt.promptId)
        return confirmations.current?.let { KeySurface.Panel(it) }
    }

    /** Escape: declines the prompt (ignored while an answer is in flight) or cancels the confirmation. */
    private fun dismiss(surface: KeySurface) {
        when (surface) {
            is KeySurface.Prompt -> {
                val model = graph.loader.model ?: return
                lifecycleScope.launch { model.back() }
            }
            is KeySurface.Panel -> if (confirmations.current === surface.confirmation) confirmations.cancel()
        }
    }

    /** A prompt or a confirmation is on screen (or no longer is). */
    private fun protect(on: Boolean) {
        if (protecting == on) return
        protecting = on
        touchGuard.protecting = on
        // A second layer behind dispatchTouchEvent: the Compose root drops
        // obscured touches itself while something needs an answer.
        composeRoot()?.filterTouchesWhenObscured = on
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // HIDE_OVERLAY_WINDOWS: other apps' overlays are hidden only now,
            // so an always-on overlay (a screen dimmer) stays elsewhere.
            window.setHideOverlayWindows(on)
        }
    }

    /** The view setContent installed. */
    private fun composeRoot(): View? = findViewById<ViewGroup>(android.R.id.content)?.getChildAt(0)

    /** Edge to edge, with system bar icons that match the theme the user picked. */
    private fun applySystemBars(dark: Boolean) {
        if (darkBars == dark) return
        darkBars = dark
        val bars = if (dark) {
            SystemBarStyle.dark(Color.TRANSPARENT)
        } else {
            SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
        }
        // Before API 27 the navigation bar's icons cannot be dark: a scrim keeps them readable.
        val navigation = if (dark) SystemBarStyle.dark(DARK_SCRIM) else SystemBarStyle.light(LIGHT_SCRIM, DARK_SCRIM)
        enableEdgeToEdge(statusBarStyle = bars, navigationBarStyle = navigation)
    }

    private companion object {
        // The scrims androidx.activity uses by default.
        val LIGHT_SCRIM = Color.argb(0xe6, 0xFF, 0xFF, 0xFF)
        val DARK_SCRIM = Color.argb(0x80, 0x1b, 0x1b, 0x1b)

        fun MotionEvent.touchPhase(): TouchPhase = when (actionMasked) {
            MotionEvent.ACTION_DOWN -> TouchPhase.DOWN
            MotionEvent.ACTION_POINTER_DOWN -> TouchPhase.POINTER_DOWN
            MotionEvent.ACTION_MOVE -> TouchPhase.MOVE
            MotionEvent.ACTION_UP -> TouchPhase.UP
            MotionEvent.ACTION_CANCEL -> TouchPhase.CANCEL
            else -> TouchPhase.OTHER
        }

        /**
         * The touch passed through another app's window over the touched
         * point, or (API 29+) part of this window is covered elsewhere. API
         * 26-28 has only the first flag.
         */
        fun MotionEvent.isObscured(): Boolean {
            if (flags and MotionEvent.FLAG_WINDOW_IS_OBSCURED != 0) return true
            return Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q &&
                flags and MotionEvent.FLAG_WINDOW_IS_PARTIALLY_OBSCURED != 0
        }
    }
}
