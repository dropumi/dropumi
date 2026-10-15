// SPDX-License-Identifier: MPL-2.0
//
// Overlay (tapjacking) protection, R23 (ADR-0013 D8). Another app's window
// lying over Dropumi can trick a tap into a consent. Android marks such a
// touch: FLAG_WINDOW_IS_OBSCURED when it passed through a window over the
// touched point, and from API 29 FLAG_WINDOW_IS_PARTIALLY_OBSCURED when part
// of Dropumi's window is covered elsewhere. MainActivity reads both flags on
// every touch and asks this guard what to do with it.
//
// Only the protected surfaces act on it, so an always-on overlay (a screen
// dimmer) never locks the rest of the app:
// - a prompt or a trust/policy confirmation on screen: the whole obscured
//   gesture is dropped, from ACTION_DOWN to ACTION_UP or ACTION_CANCEL, and
//   the screen says "Close it to answer";
// - the Send buttons: a click from an obscured gesture is ignored, with
//   "Close it to send";
// - everywhere else the gesture goes through, with a notice that does not
//   block anything.
// On API 31+ MainActivity also hides other apps' overlay windows while a
// prompt or a confirmation shows. API 26-30 has no hiding, and API 26-28
// has no partial flag: that residual risk is disclosed in the threat model.
package io.github.dropumi.ui

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import io.github.dropumi.ui.design.DSType
import kotlinx.coroutines.delay

/** The part of a touch event's action the filter needs (no Android types, for JVM tests). */
enum class TouchPhase { DOWN, POINTER_DOWN, MOVE, UP, CANCEL, OTHER }

/** What MainActivity does with one touch event. */
enum class TouchDecision {
    /** Hand it to the content. */
    DISPATCH,

    /** Swallow it. */
    DROP,

    /** Hand the content an ACTION_CANCEL instead, so a press under way ends without a click; drop the rest. */
    CANCEL_AND_DROP,
}

/**
 * Follows one gesture at a time and decides, per event, whether it reaches
 * the content. A gesture counts as obscured once any of its events carries
 * an obscured flag; that stays known until the next ACTION_DOWN, so a click
 * delivered on ACTION_UP (or just after it) still sees it.
 */
class ObscuredGestureFilter {
    /** Whether the gesture in progress (or the last one, once it ended) was obscured. */
    var gestureObscured = false
        private set

    private var dropping = false

    /**
     * @param obscured the event carries an obscured flag.
     * @param protecting a prompt or a confirmation is on screen.
     */
    fun onTouch(phase: TouchPhase, obscured: Boolean, protecting: Boolean): TouchDecision {
        if (phase == TouchPhase.DOWN) {
            gestureObscured = obscured
            dropping = obscured && protecting
            return if (dropping) TouchDecision.DROP else TouchDecision.DISPATCH
        }
        if (obscured) gestureObscured = true
        val ends = phase == TouchPhase.UP || phase == TouchPhase.CANCEL
        if (dropping) {
            if (ends) dropping = false
            return TouchDecision.DROP
        }
        if (protecting && gestureObscured) {
            // The gesture became obscured, or a prompt appeared under it: the
            // content has seen its start, so it is cancelled there.
            dropping = !ends
            return TouchDecision.CANCEL_AND_DROP
        }
        return TouchDecision.DISPATCH
    }
}

/** What the screen says about an overlay. */
enum class OverlayNotice(val text: String) {
    /** On screens that are not protected; nothing is blocked. */
    GENERAL("Another app is drawing over Dropumi."),

    /** A prompt or a confirmation dropped a gesture. */
    ANSWER("Another app is drawing over Dropumi. Close it to answer."),

    /** A Send button ignored a click. */
    SEND("Another app is drawing over Dropumi. Close it to send."),
}

/**
 * The activity's overlay state, shared with the screens. Touches are fed by
 * MainActivity on the main thread; [protecting] is set from composition.
 */
@Stable
class TouchGuard {
    private val filter = ObscuredGestureFilter()

    /** A prompt or a trust/policy confirmation is on screen. */
    var protecting: Boolean = false

    /** The notice to show, if any. */
    var notice: OverlayNotice? by mutableStateOf(null)
        private set

    /** Changes each time a notice is (re)shown, so the screen can time it from the latest one. */
    var noticeSerial: Int by mutableIntStateOf(0)
        private set

    /** Whether the last gesture (the one a click comes from) was obscured. */
    val lastGestureObscured: Boolean get() = filter.gestureObscured

    /** Decides what happens to one touch event. */
    fun onTouch(phase: TouchPhase, obscured: Boolean): TouchDecision {
        val decision = filter.onTouch(phase, obscured, protecting)
        when {
            // Once per dropped gesture: where its drop starts.
            decision == TouchDecision.CANCEL_AND_DROP -> show(OverlayNotice.ANSWER)
            decision == TouchDecision.DROP && phase == TouchPhase.DOWN -> show(OverlayNotice.ANSWER)
            decision == TouchDecision.DROP -> Unit
            phase == TouchPhase.DOWN && obscured -> show(OverlayNotice.GENERAL)
            // A clean touch: whatever covered the window has gone.
            phase == TouchPhase.DOWN -> notice = null
        }
        return decision
    }

    /**
     * Called by a Send button before it acts. False (and the "Close it to
     * send" notice) when the click came from an obscured gesture.
     */
    fun allowSend(): Boolean {
        if (!filter.gestureObscured) return true
        show(OverlayNotice.SEND)
        return false
    }

    /** Hides [shown] if it is still the notice on screen. */
    fun dismiss(shown: OverlayNotice) {
        if (notice == shown) notice = null
    }

    private fun show(value: OverlayNotice) {
        notice = value
        noticeSerial++
    }
}

/** The activity's [TouchGuard]. */
val LocalTouchGuard = staticCompositionLocalOf<TouchGuard> { error("no TouchGuard") }

/** How long a notice stays after its latest showing. */
private const val NOTICE_MS = 6_000L

/** [notice]'s text, announced by TalkBack; it hides itself [NOTICE_MS] after the latest showing. */
@Composable
fun OverlayNoticeText(guard: TouchGuard, notice: OverlayNotice, color: Color, modifier: Modifier = Modifier) {
    LaunchedEffect(guard.noticeSerial) {
        delay(NOTICE_MS)
        guard.dismiss(notice)
    }
    Text(
        notice.text,
        style = DSType.meta,
        color = color,
        textAlign = TextAlign.Center,
        modifier = modifier.semantics { liveRegion = LiveRegionMode.Polite }.testTag("overlay.notice"),
    )
}
