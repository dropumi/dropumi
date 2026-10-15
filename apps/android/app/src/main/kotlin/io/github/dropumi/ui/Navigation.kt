// SPDX-License-Identifier: MPL-2.0
//
// Where the user is: three tabs (Nearby, Transfers, Settings), each with its
// own stack of screens. No navigation library: a tab and a list per tab,
// saved across configuration changes. Peer strings are never saved into
// the instance state (a chosen device is forgotten on recreation).
package io.github.dropumi.ui

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.setValue

enum class Tab { NEARBY, TRANSFERS, SETTINGS }

/** A screen above a tab's root. */
sealed interface Screen {
    /** Send, optionally with a nearby device (its discovery instance) chosen. */
    data class Send(val preselected: String? = null) : Screen
    data object ConnectByAddress : Screen
    data object ThisDevice : Screen
    data object PairedDevices : Screen
    data object Pairing : Screen
    data object Licences : Screen

    /** One licence notice, by its asset name (one of [LICENCE_ASSETS]). */
    data class Licence(val asset: String) : Screen
}

/** The notices the Licences screen lists (app assets). */
val LICENCE_ASSETS = listOf(
    "licenses/rust-crates.txt",
    "licenses/android-libraries.txt",
    "licenses/OFL.txt",
    "licenses/LUCIDE-ISC.txt",
)

@Stable
class Router(initialTab: Tab = Tab.NEARBY, initialStacks: Map<Tab, List<Screen>> = emptyMap()) {
    var tab: Tab by mutableStateOf(initialTab)
        private set
    private var stacks: Map<Tab, List<Screen>> by mutableStateOf(initialStacks)

    /** The screens above [tab]'s root, bottom first. */
    fun stack(tab: Tab): List<Screen> = stacks[tab].orEmpty()

    /** The screen showing in the current tab, or null for its root. */
    val current: Screen? get() = stack(tab).lastOrNull()

    fun select(tab: Tab) {
        this.tab = tab
    }

    fun push(screen: Screen) {
        stacks = stacks + (tab to (stack(tab) + screen))
    }

    /** Goes back one screen in the current tab; false at its root. */
    fun pop(): Boolean {
        val stack = stack(tab)
        if (stack.isEmpty()) return false
        stacks = stacks + (tab to stack.dropLast(1))
        return true
    }

    /** Opens Send in the Nearby tab (replacing a Send already open). */
    fun openSend(preselected: String? = null) {
        val nearby = stack(Tab.NEARBY)
        stacks = stacks + (Tab.NEARBY to if (nearby.lastOrNull() is Screen.Send) nearby.dropLast(1) + Screen.Send(preselected) else listOf(Screen.Send(preselected)))
        tab = Tab.NEARBY
    }

    fun openPairing() {
        stacks = stacks + (Tab.SETTINGS to listOf(Screen.Pairing))
        tab = Tab.SETTINGS
    }

    fun openThisDevice() {
        stacks = stacks + (Tab.SETTINGS to listOf(Screen.ThisDevice))
        tab = Tab.SETTINGS
    }

    /** After a send starts: Transfers, with the Nearby and Settings stacks closed. */
    fun openTransfers() {
        stacks = emptyMap()
        tab = Tab.TRANSFERS
    }

    companion object {
        private fun token(screen: Screen): String = when (screen) {
            // The chosen device is a peer string: not saved.
            is Screen.Send -> "send"
            Screen.ConnectByAddress -> "address"
            Screen.ThisDevice -> "thisDevice"
            Screen.PairedDevices -> "paired"
            Screen.Pairing -> "pairing"
            Screen.Licences -> "licences"
            is Screen.Licence -> "licence:" + LICENCE_ASSETS.indexOf(screen.asset)
        }

        private fun screen(token: String): Screen? = when (token) {
            "send" -> Screen.Send()
            "address" -> Screen.ConnectByAddress
            "thisDevice" -> Screen.ThisDevice
            "paired" -> Screen.PairedDevices
            "pairing" -> Screen.Pairing
            "licences" -> Screen.Licences
            else -> token.removePrefix("licence:").toIntOrNull()?.let(LICENCE_ASSETS::getOrNull)?.let(Screen::Licence)
        }

        /** Saves the tab and each tab's stack as plain tokens. */
        val Saver: Saver<Router, Any> = Saver(
            save = { router ->
                arrayListOf(router.tab.name) + Tab.entries.map { t -> router.stack(t).joinToString(",", transform = ::token) }
            },
            restore = { saved ->
                @Suppress("UNCHECKED_CAST")
                val list = saved as List<String>
                val tab = Tab.entries.firstOrNull { it.name == list.firstOrNull() } ?: Tab.NEARBY
                val stacks = Tab.entries.mapIndexed { i, t ->
                    t to list.getOrNull(i + 1).orEmpty().split(',').filter { it.isNotEmpty() }.mapNotNull(::screen)
                }.toMap()
                Router(tab, stacks)
            },
        )
    }
}
