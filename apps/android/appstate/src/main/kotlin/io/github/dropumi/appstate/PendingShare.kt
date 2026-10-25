// SPDX-License-Identifier: MPL-2.0
//
// The in-process hand-off from the share target to the main screen
// (ADR-0013 D6). Only ShareActivity puts items here, after its early
// UriGuard filter; AppModel takes them into the draft. MainActivity never
// takes share items from its own intent, so another app cannot inject a
// draft by starting it.

package io.github.dropumi.appstate

class PendingShare {
    private val items = mutableListOf<String>()

    /** Adds content URIs, keeping at most [MAX_ITEMS] waiting. */
    @Synchronized
    fun put(uris: List<String>) {
        for (uri in uris) {
            if (items.size >= MAX_ITEMS) break
            if (uri !in items) items += uri
        }
    }

    /** Takes everything waiting. */
    @Synchronized
    fun take(): List<String> = items.toList().also { items.clear() }

    companion object {
        /** The share target takes at most this many items at once. */
        const val MAX_ITEMS = 100
    }
}
