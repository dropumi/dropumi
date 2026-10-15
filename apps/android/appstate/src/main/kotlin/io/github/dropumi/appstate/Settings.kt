// SPDX-License-Identifier: MPL-2.0

package io.github.dropumi.appstate

import io.github.dropumi.ffi.ReceivePolicy

/** Who may send to this device ("Who can send to this device"). */
enum class ReceiveVisibility {
    /** Not receiving: no listener, no advertisement. */
    OFF,

    /** Paired devices only (`TrustedOnly`). The default. */
    PAIRED_ONLY,

    /** Any device may ask, for 10 minutes; unverified offers show a code. */
    EVERYONE,
    ;

    val policy: ReceivePolicy?
        get() = when (this) {
            OFF -> null
            PAIRED_ONLY -> ReceivePolicy.TrustedOnly
            EVERYONE -> ReceivePolicy.Everyone(EVERYONE_MINUTES)
        }

    companion object {
        /** The Everyone window length. */
        const val EVERYONE_MINUTES: UInt = 10u

        /**
         * The stored choice as restored at launch: Paired only by default, and
         * never Everyone, whose window is short and must be opened on purpose.
         */
        fun restored(stored: String?): ReceiveVisibility = if (stored == OFF.name) OFF else PAIRED_ONLY
    }
}

/** The colour scheme the user picked. */
enum class Appearance {
    SYSTEM,
    LIGHT,
    DARK,
    ;

    companion object {
        /** The first API level with a system dark theme. */
        private const val SYSTEM_DARK_THEME_SDK = 29

        /** "Follow system" exists only where the system has a dark theme. */
        fun choices(sdkInt: Int): List<Appearance> =
            if (sdkInt >= SYSTEM_DARK_THEME_SDK) listOf(SYSTEM, LIGHT, DARK) else listOf(LIGHT, DARK)

        fun default(sdkInt: Int): Appearance = if (sdkInt >= SYSTEM_DARK_THEME_SDK) SYSTEM else LIGHT

        fun restored(stored: String?, sdkInt: Int): Appearance {
            val value = entries.firstOrNull { it.name == stored } ?: return default(sdkInt)
            return if (value in choices(sdkInt)) value else default(sdkInt)
        }
    }
}

/**
 * The user's settings. Implementations restore [visibility] with
 * [ReceiveVisibility.restored] and [appearance] with [Appearance.restored].
 */
interface SettingsStore {
    var visibility: ReceiveVisibility

    /** Off by default. */
    var autoAcceptPaired: Boolean
    var onboarded: Boolean
    var appearance: Appearance
}

/** Settings held in memory, with the same restore rules (tests and test applications). */
class MemorySettingsStore(private val sdkInt: Int) : SettingsStore {
    private var storedVisibility: String? = null
    private var storedAppearance: String? = null

    override var visibility: ReceiveVisibility
        get() = ReceiveVisibility.restored(storedVisibility)
        set(value) {
            storedVisibility = value.name
        }

    override var autoAcceptPaired: Boolean = false
    override var onboarded: Boolean = false

    override var appearance: Appearance
        get() = Appearance.restored(storedAppearance, sdkInt)
        set(value) {
            storedAppearance = value.name
        }
}
