// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.discovery

import io.github.dropumi.ffi.BrowseSinkInterface
import io.github.dropumi.ffi.ServiceAd
import io.github.dropumi.ffi.TxtEntry
import java.util.Collections

/** Records what the adapter asks of NSD; the test fires NSD's callbacks. */
class FakeNsdPort(override val hasServiceInfoCallback: Boolean = false) : NsdPort {
    val registered = Collections.synchronizedList(mutableListOf<Pair<NsdService, NsdPort.RegistrationListener>>())
    val unregistered = Collections.synchronizedList(mutableListOf<NsdPort.RegistrationListener>())
    val discoveries = Collections.synchronizedList(mutableListOf<NsdPort.DiscoveryListener>())
    val stopped = Collections.synchronizedList(mutableListOf<NsdPort.DiscoveryListener>())
    val resolves = Collections.synchronizedList(mutableListOf<Pair<NsdService, NsdPort.ResolveListener>>())
    val infoCallbacks = Collections.synchronizedList(mutableListOf<Pair<NsdService, NsdPort.InfoCallback>>())
    val infoUnregistered = Collections.synchronizedList(mutableListOf<NsdPort.InfoCallback>())

    /** Thrown by every call when set. */
    var failure: RuntimeException? = null

    private fun maybeFail() {
        failure?.let { throw it }
    }

    override fun register(service: NsdService, listener: NsdPort.RegistrationListener) {
        maybeFail()
        registered += service to listener
    }

    override fun unregister(listener: NsdPort.RegistrationListener) {
        maybeFail()
        unregistered += listener
    }

    override fun discover(type: String, listener: NsdPort.DiscoveryListener) {
        maybeFail()
        discoveries += listener
    }

    override fun stopDiscovery(listener: NsdPort.DiscoveryListener) {
        maybeFail()
        stopped += listener
    }

    override fun resolve(service: NsdService, listener: NsdPort.ResolveListener) {
        maybeFail()
        resolves += service to listener
    }

    override fun registerInfoCallback(service: NsdService, callback: NsdPort.InfoCallback) {
        maybeFail()
        infoCallbacks += service to callback
    }

    override fun unregisterInfoCallback(callback: NsdPort.InfoCallback) {
        maybeFail()
        infoUnregistered += callback
    }

    /** The services with a live service-info callback (34+ path). */
    fun liveInfoNames(): List<String> =
        infoCallbacks.filter { (_, cb) -> cb !in infoUnregistered }.map { it.first.name }
}

/** Records what the adapter reports to the core, and on which thread. */
class RecordingSink : BrowseSinkInterface {
    val events = Collections.synchronizedList(mutableListOf<String>())
    val txt = Collections.synchronizedList(mutableListOf<List<TxtEntry>>())
    val threads = Collections.synchronizedSet(mutableSetOf<String>())

    override fun failed(reason: String) {
        threads += Thread.currentThread().name
        events += "failed"
    }

    override fun lost(instance: String) {
        threads += Thread.currentThread().name
        events += "lost $instance"
    }

    override fun resolved(instance: String, txt: List<TxtEntry>, addresses: List<String>, port: UShort) {
        threads += Thread.currentThread().name
        this.txt += txt
        events += "resolved $instance"
    }

    fun resolvedNames(): List<String> = events.filter { it.startsWith("resolved ") }.map { it.removePrefix("resolved ") }
}

fun ad(instance: String) = ServiceAd(
    serviceType = "_dropumi._tcp",
    instance = instance,
    port = 47321u,
    txt = listOf(TxtEntry("v", "1".toByteArray()), TxtEntry("n", "Kitchen tablet".toByteArray())),
)

fun service(name: String, attributes: Map<String, ByteArray?> = emptyMap()) =
    NsdService(name, "_dropumi._tcp", 47321, attributes, listOf("192.168.1.20"))

/** A gate open for API 36 with module version 100 only. */
val OPEN_GATE = HostLabelGate(listOf(HostLabelGate.Observed(36, 100L..100L)))

fun adapter(
    port: NsdPort,
    gate: HostLabelGate = OPEN_GATE,
    addresses: List<String> = listOf("192.168.1.5"),
) = NsdAdapterCore(port, gate, sdkInt = 36, moduleVersion = 100L, ipv4Addresses = { addresses })
