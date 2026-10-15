// SPDX-License-Identifier: MPL-2.0

package io.github.dropumi

import android.content.SharedPreferences
import io.github.dropumi.appstate.Appearance
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.SettingsStore

/** Settings in SharedPreferences (excluded from backups like everything else, ADR-0013 D3). */
class PrefsSettingsStore(private val prefs: SharedPreferences, private val sdkInt: Int) : SettingsStore {
    override var visibility: ReceiveVisibility
        get() = ReceiveVisibility.restored(prefs.getString(VISIBILITY, null))
        set(value) = prefs.edit().putString(VISIBILITY, value.name).apply()

    override var autoAcceptPaired: Boolean
        get() = prefs.getBoolean(AUTO_ACCEPT, false)
        set(value) = prefs.edit().putBoolean(AUTO_ACCEPT, value).apply()

    override var onboarded: Boolean
        get() = prefs.getBoolean(ONBOARDED, false)
        set(value) = prefs.edit().putBoolean(ONBOARDED, value).apply()

    override var appearance: Appearance
        get() = Appearance.restored(prefs.getString(APPEARANCE, null), sdkInt)
        set(value) = prefs.edit().putString(APPEARANCE, value.name).apply()

    private companion object {
        const val VISIBILITY = "visibility"
        const val AUTO_ACCEPT = "autoAcceptPaired"
        const val ONBOARDED = "onboarded"
        const val APPEARANCE = "appearance"
    }
}
