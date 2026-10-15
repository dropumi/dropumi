// SPDX-License-Identifier: MPL-2.0
//
// When a prompt's Accept (or "Codes match") may act (ADR-0013 D8). It arms
// 600 ms after the prompt appears, and re-arms (off, then 600 ms again)
// when the activity resumes and when its window regains focus, so a tap
// meant for something else cannot land on a prompt that just appeared or
// came back. Decline is never held back.
//
// A pure state machine. The UI drives the clock: for the shown prompt it
// waits ARM_AFTER_MS and reports `elapsed(promptId, epoch, ms)`. Every
// change starts a new epoch, so a timer that started before a resume or a
// new prompt cannot arm the prompt now showing. AppModel keeps this state
// and ignores an Accept for a prompt that is not armed, whatever the UI
// sends.

package io.github.dropumi.appstate

data class PromptArming(
    /** The prompt on screen, if any. */
    val promptId: ULong? = null,
    /** Changes whenever arming starts over. */
    val epoch: Int = 0,
    val armed: Boolean = false,
) {
    /** The prompt now on screen (null when none is). Starts over for a new prompt. */
    fun shown(id: ULong?): PromptArming = if (id == promptId) this else PromptArming(id, epoch + 1, false)

    /** The activity resumed: starts over. */
    fun resumed(): PromptArming = PromptArming(promptId, epoch + 1, false)

    /** The window regained focus: starts over. */
    fun focused(): PromptArming = PromptArming(promptId, epoch + 1, false)

    /** [ms] passed since [epoch] began for [id]; arms once that is at least [ARM_AFTER_MS]. */
    fun elapsed(id: ULong, epoch: Int, ms: Long): PromptArming =
        if (id == promptId && epoch == this.epoch && ms >= ARM_AFTER_MS) copy(armed = true) else this

    /** Whether an Accept for prompt [id] may act now. */
    fun canAccept(id: ULong): Boolean = armed && promptId == id

    companion object {
        const val ARM_AFTER_MS = 600L
    }
}
