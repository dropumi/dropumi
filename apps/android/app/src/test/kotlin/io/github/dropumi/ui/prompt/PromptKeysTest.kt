// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui.prompt

import org.junit.Assert.assertEquals
import org.junit.Test

/** ADR-0013 D8: Escape declines; activation keys act only after the user moved focus. */
class PromptKeysTest {
    @Test
    fun escapeDeclinesOnRelease() {
        val keys = PromptKeys()
        assertEquals(PromptKeyAction.CONSUME, keys.onKey(1u, PromptKey.ESCAPE, down = true, canceled = false))
        assertEquals(PromptKeyAction.DECLINE, keys.onKey(1u, PromptKey.ESCAPE, down = false, canceled = false))
        assertEquals(PromptKeyAction.CONSUME, keys.onKey(1u, PromptKey.ESCAPE, down = false, canceled = true))
    }

    @Test
    fun activationKeysNeverActWithoutNavigation() {
        val keys = PromptKeys()
        repeat(3) {
            assertEquals(PromptKeyAction.CONSUME, keys.onKey(1u, PromptKey.ACTIVATE, down = true, canceled = false))
            assertEquals(PromptKeyAction.CONSUME, keys.onKey(1u, PromptKey.ACTIVATE, down = false, canceled = false))
        }
        assertEquals(PromptKeyAction.PASS, keys.onKey(1u, PromptKey.NAVIGATE, down = true, canceled = false))
        assertEquals(PromptKeyAction.PASS, keys.onKey(1u, PromptKey.ACTIVATE, down = true, canceled = false))
        assertEquals(PromptKeyAction.PASS, keys.onKey(1u, PromptKey.ACTIVATE, down = false, canceled = false))
    }

    @Test
    fun navigationCountsOnlyForThePromptItHappenedOn() {
        val keys = PromptKeys()
        keys.onKey(1u, PromptKey.NAVIGATE, down = true, canceled = false)
        assertEquals(PromptKeyAction.CONSUME, keys.onKey(2u, PromptKey.ACTIVATE, down = true, canceled = false))
    }

    @Test
    fun otherKeysPass() {
        assertEquals(PromptKeyAction.PASS, PromptKeys().onKey(1u, PromptKey.OTHER, down = true, canceled = false))
    }
}
