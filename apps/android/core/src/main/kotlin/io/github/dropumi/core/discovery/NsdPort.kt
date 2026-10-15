// SPDX-License-Identifier: MPL-2.0
//
// The part of Android's NsdManager the adapter uses, in plain Kotlin types,
// so NsdAdapterCore runs on the host JVM with a fake (AndroidNsdPort is the
// real one). Listener objects are the adapter's own; a port maps each to a
// fresh platform listener, because NSD refuses a reused one.

package io.github.dropumi.core.discovery

/** A service as NSD reports it. */
data class NsdService(
    val name: String,
    val type: String,
    val port: Int = 0,
    /** TXT entries; NSD reports a key without a value as null. */
    val attributes: Map<String, ByteArray?> = emptyMap(),
    /** Addresses as text, as NSD reports them (Rust filters them). */
    val addresses: List<String> = emptyList(),
)

interface NsdPort {
    /** API 34+: services are followed with a service-info callback instead of a one-shot resolve. */
    val hasServiceInfoCallback: Boolean

    fun register(service: NsdService, listener: RegistrationListener)
    fun unregister(listener: RegistrationListener)
    fun discover(type: String, listener: DiscoveryListener)
    fun stopDiscovery(listener: DiscoveryListener)

    /** Below 34: one resolve at a time. */
    fun resolve(service: NsdService, listener: ResolveListener)

    /** API 34+. */
    fun registerInfoCallback(service: NsdService, callback: InfoCallback)
    fun unregisterInfoCallback(callback: InfoCallback)

    interface RegistrationListener {
        fun registered(name: String)
        fun registrationFailed(code: Int)
        fun unregistered()
        fun unregistrationFailed(code: Int)
    }

    interface DiscoveryListener {
        fun startFailed(code: Int)
        fun found(service: NsdService)
        fun lost(service: NsdService)
    }

    interface ResolveListener {
        fun resolved(service: NsdService)
        fun failed(service: NsdService, code: Int)
    }

    interface InfoCallback {
        fun updated(service: NsdService)
        fun lost()
        fun registrationFailed(code: Int)
    }

    companion object {
        /** NsdManager.FAILURE_ALREADY_ACTIVE. */
        const val FAILURE_ALREADY_ACTIVE = 3
    }
}
