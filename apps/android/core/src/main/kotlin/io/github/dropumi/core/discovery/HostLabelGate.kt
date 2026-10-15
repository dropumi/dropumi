// SPDX-License-Identifier: MPL-2.0
//
// The R21 gate (ADR-0013 D5): advertising is allowed only on an (API level,
// Connectivity module version) pair that was observed to publish an SRV
// host label that is random per advertisement (discovery.md section 3).
// Keys are exact observed versions or ranges whose ends were both observed,
// never "this version or later": a module update can change the behaviour.
// Below API 33, NSD is the system's mdnsd and there is no module version.

package io.github.dropumi.core.discovery

class HostLabelGate(private val observed: List<Observed>) {
    /** An observed pair. [modules] is null below API 33 and required from 33. */
    data class Observed(val sdkInt: Int, val modules: LongRange?)

    /** Whether this device may advertise. An unreadable module version (null) keeps it closed from 33. */
    fun allows(sdkInt: Int, moduleVersion: Long?): Boolean = observed.any { entry ->
        entry.sdkInt == sdkInt && when {
            sdkInt < MODULE_SDK -> entry.modules == null
            else -> entry.modules != null && moduleVersion != null && moduleVersion in entry.modules
        }
    }

    companion object {
        /** The first API level whose NSD runs in the Connectivity module. */
        const val MODULE_SDK = 33

        /**
         * The pairs observed so far: none. Spike S-AN2 has not recorded a host
         * label that is random per advertisement on any image or device, so
         * every Android device is browse-only (it sees others; peers reach it
         * after pairing or by address).
         */
        val OBSERVED = HostLabelGate(emptyList())
    }
}
