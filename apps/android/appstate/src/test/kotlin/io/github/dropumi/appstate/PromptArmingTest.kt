// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.appstate

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PromptArmingTest {
    private fun armed(id: ULong): PromptArming {
        val shown = PromptArming().shown(id)
        return shown.elapsed(id, shown.epoch, PromptArming.ARM_AFTER_MS)
    }

    @Test
    fun armsOnlyAfter600Milliseconds() {
        val shown = PromptArming().shown(1u)
        assertFalse(shown.canAccept(1u))
        assertFalse(shown.elapsed(1u, shown.epoch, PromptArming.ARM_AFTER_MS - 1).canAccept(1u))
        assertTrue(shown.elapsed(1u, shown.epoch, PromptArming.ARM_AFTER_MS).canAccept(1u))
    }

    @Test
    fun promptArmingRearmsOnNewPromptId() {
        val first = armed(1u)
        assertTrue(first.canAccept(1u))
        val next = first.shown(2u)
        assertFalse(next.canAccept(2u))
        assertFalse(next.canAccept(1u))
        // The first prompt's timer cannot arm the second one.
        assertFalse(next.elapsed(1u, first.epoch, PromptArming.ARM_AFTER_MS).canAccept(2u))
        assertTrue(next.elapsed(2u, next.epoch, PromptArming.ARM_AFTER_MS).canAccept(2u))
        // The same prompt shown again keeps its state.
        assertTrue(first.shown(1u).canAccept(1u))
    }

    @Test
    fun promptArmingRearmsOnResume() {
        val before = armed(1u)
        val resumed = before.resumed()
        assertFalse(resumed.canAccept(1u))
        // A timer that started before the resume does not count.
        assertFalse(resumed.elapsed(1u, before.epoch, PromptArming.ARM_AFTER_MS).canAccept(1u))
        assertTrue(resumed.elapsed(1u, resumed.epoch, PromptArming.ARM_AFTER_MS).canAccept(1u))
    }

    @Test
    fun promptArmingRearmsOnWindowFocus() {
        val before = armed(1u)
        val focused = before.focused()
        assertFalse(focused.canAccept(1u))
        assertFalse(focused.elapsed(1u, before.epoch, PromptArming.ARM_AFTER_MS).canAccept(1u))
        assertTrue(focused.elapsed(1u, focused.epoch, PromptArming.ARM_AFTER_MS).canAccept(1u))
    }

    @Test
    fun noPromptMeansNothingToAccept() {
        val none = armed(1u).shown(null)
        assertFalse(none.canAccept(1u))
    }
}
