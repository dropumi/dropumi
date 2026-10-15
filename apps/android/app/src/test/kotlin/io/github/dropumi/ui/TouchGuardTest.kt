// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** R23 (ADR-0013 D8): what happens to obscured touches, per surface. */
class TouchGuardTest {
    private fun ObscuredGestureFilter.gesture(
        vararg events: Pair<TouchPhase, Boolean>,
        protecting: Boolean,
    ): List<TouchDecision> = events.map { (phase, obscured) -> onTouch(phase, obscured, protecting) }

    @Test
    fun obscuredGestureIsDroppedWholeWhileProtecting() {
        val decisions = ObscuredGestureFilter().gesture(
            TouchPhase.DOWN to true,
            TouchPhase.MOVE to false,
            TouchPhase.UP to false,
            protecting = true,
        )
        assertEquals(List(3) { TouchDecision.DROP }, decisions)
    }

    @Test
    fun cleanGestureReachesTheContentWhileProtecting() {
        val filter = ObscuredGestureFilter()
        val decisions = filter.gesture(TouchPhase.DOWN to false, TouchPhase.UP to false, protecting = true)
        assertEquals(List(2) { TouchDecision.DISPATCH }, decisions)
        assertFalse(filter.gestureObscured)
    }

    /** A second finger through an overlay cancels the press the first one started; the rest is dropped. */
    @Test
    fun gestureThatBecomesObscuredIsCancelledThenDropped() {
        val decisions = ObscuredGestureFilter().gesture(
            TouchPhase.DOWN to false,
            TouchPhase.POINTER_DOWN to true,
            TouchPhase.MOVE to false,
            TouchPhase.UP to false,
            protecting = true,
        )
        assertEquals(
            listOf(TouchDecision.DISPATCH, TouchDecision.CANCEL_AND_DROP, TouchDecision.DROP, TouchDecision.DROP),
            decisions,
        )
    }

    /** A prompt that appears under an obscured gesture cancels it there; the gesture's UP never lands. */
    @Test
    fun promptAppearingUnderAnObscuredGestureCancelsIt() {
        val filter = ObscuredGestureFilter()
        assertEquals(TouchDecision.DISPATCH, filter.onTouch(TouchPhase.DOWN, obscured = true, protecting = false))
        assertEquals(TouchDecision.CANCEL_AND_DROP, filter.onTouch(TouchPhase.UP, obscured = true, protecting = true))
        // The next gesture starts clean.
        assertEquals(TouchDecision.DISPATCH, filter.onTouch(TouchPhase.DOWN, obscured = false, protecting = true))
    }

    /** Elsewhere an obscured gesture goes through: an always-on overlay must not lock the app. */
    @Test
    fun obscuredGesturePassesThroughWhenNotProtecting() {
        val filter = ObscuredGestureFilter()
        val decisions = filter.gesture(TouchPhase.DOWN to true, TouchPhase.MOVE to true, TouchPhase.UP to true, protecting = false)
        assertEquals(List(3) { TouchDecision.DISPATCH }, decisions)
        assertTrue("a click on UP still sees it", filter.gestureObscured)
    }

    @Test
    fun guardShowsTheNoticeForTheSurface() {
        val guard = TouchGuard()
        guard.onTouch(TouchPhase.DOWN, obscured = true)
        assertEquals(OverlayNotice.GENERAL, guard.notice)
        guard.onTouch(TouchPhase.UP, obscured = true)
        guard.onTouch(TouchPhase.DOWN, obscured = false)
        assertNull("a clean touch clears it", guard.notice)

        guard.protecting = true
        assertEquals(TouchDecision.DROP, guard.onTouch(TouchPhase.DOWN, obscured = true))
        assertEquals(OverlayNotice.ANSWER, guard.notice)
        val serial = guard.noticeSerial
        guard.onTouch(TouchPhase.MOVE, obscured = true)
        guard.onTouch(TouchPhase.UP, obscured = true)
        assertEquals("shown once per gesture", serial, guard.noticeSerial)
    }

    @Test
    fun sendIgnoresAClickFromAnObscuredGesture() {
        val guard = TouchGuard()
        guard.onTouch(TouchPhase.DOWN, obscured = true)
        guard.onTouch(TouchPhase.UP, obscured = true)
        assertFalse(guard.allowSend())
        assertEquals(OverlayNotice.SEND, guard.notice)
        guard.onTouch(TouchPhase.DOWN, obscured = false)
        guard.onTouch(TouchPhase.UP, obscured = false)
        assertTrue(guard.allowSend())
    }

    @Test
    fun dismissHidesOnlyTheNoticeShown() {
        val guard = TouchGuard()
        guard.onTouch(TouchPhase.DOWN, obscured = true)
        guard.dismiss(OverlayNotice.SEND)
        assertEquals(OverlayNotice.GENERAL, guard.notice)
        guard.dismiss(OverlayNotice.GENERAL)
        assertNull(guard.notice)
    }
}
