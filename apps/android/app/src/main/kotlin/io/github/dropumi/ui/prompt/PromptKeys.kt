// SPDX-License-Identifier: MPL-2.0
//
// Hardware keys while a prompt is on screen (ADR-0013 D8). MainActivity
// asks this before the key reaches Compose:
// - Escape declines (on its ACTION_UP). Compose would never see it: the
//   prompt has no initial focus, and the platform's Escape-to-Back fallback
//   is not relied on.
// - Enter, NumPad Enter, DPAD centre and Space act only on a button the user
//   moved focus to with a navigation key (Tab, the arrows, Page Up/Down,
//   Home/End) while this prompt was on screen. Leaving touch mode with such
//   a key lets the system focus a button by itself; an activation key alone
//   must never answer the prompt through that focus.
package io.github.dropumi.ui.prompt

/** A key, as far as the prompt is concerned. */
enum class PromptKey {
    ESCAPE,

    /** Enter, NumPad Enter, DPAD centre, Space. */
    ACTIVATE,

    /** Tab, the arrows, Page Up/Down, Home/End. */
    NAVIGATE,
    OTHER,
}

/** What MainActivity does with a key while a prompt shows. */
enum class PromptKeyAction {
    /** Let it through to the window. */
    PASS,

    /** Swallow it. */
    CONSUME,

    /** Swallow it and decline the prompt (AppModel.back: ignored while an answer is in flight). */
    DECLINE,
}

class PromptKeys {
    private var promptId: ULong? = null
    private var navigated = false

    /**
     * @param promptId the prompt on screen.
     * @param down the key's ACTION_DOWN (otherwise its ACTION_UP).
     * @param canceled the key event was cancelled.
     */
    fun onKey(promptId: ULong, key: PromptKey, down: Boolean, canceled: Boolean): PromptKeyAction {
        if (promptId != this.promptId) {
            this.promptId = promptId
            navigated = false
        }
        return when (key) {
            PromptKey.ESCAPE -> if (!down && !canceled) PromptKeyAction.DECLINE else PromptKeyAction.CONSUME
            PromptKey.ACTIVATE -> if (navigated) PromptKeyAction.PASS else PromptKeyAction.CONSUME
            PromptKey.NAVIGATE -> {
                if (down) navigated = true
                PromptKeyAction.PASS
            }
            PromptKey.OTHER -> PromptKeyAction.PASS
        }
    }
}
