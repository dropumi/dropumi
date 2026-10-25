// SPDX-License-Identifier: MPL-2.0

package io.github.dropumi.core.discovery

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Where this device's advertisement stands, as the Nearby screen shows it. */
sealed interface AdvertiseState {
    /** Not allowed on this Android version (the R21 gate is closed): browse-only. */
    data object GateClosed : AdvertiseState

    /** Allowed, not advertising. */
    data object Idle : AdvertiseState

    /** Registration was asked for; NSD has not answered. */
    data object Registering : AdvertiseState

    data object Registered : AdvertiseState

    /** It failed after the core recorded it as active, so the core cannot report it. */
    data class Failed(val reason: Reason) : AdvertiseState

    enum class Reason { NO_IPV4_NETWORK, REGISTRATION_FAILED, RENAMED }
}

/** What AppModel needs from the advertiser. */
interface Advertising {
    /** Whether ReceiveOptions.advertise may be true. */
    val gateOpen: Boolean
    val state: StateFlow<AdvertiseState>

    companion object {
        /** No advertiser (and so no advertising). */
        val CLOSED: Advertising = object : Advertising {
            override val gateOpen = false
            override val state: StateFlow<AdvertiseState> =
                MutableStateFlow<AdvertiseState>(AdvertiseState.GateClosed).asStateFlow()
        }
    }
}
