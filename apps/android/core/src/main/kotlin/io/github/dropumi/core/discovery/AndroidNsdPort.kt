// SPDX-License-Identifier: MPL-2.0
//
// NsdPort over Android's NsdManager. Each adapter listener gets a fresh
// platform listener (NSD refuses a reused one). From API 33 the Executor
// overloads deliver callbacks on the adapter's own thread; below 33 they
// arrive on a thread NSD chooses, and the adapter hops to its thread.

package io.github.dropumi.core.discovery

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executor

class AndroidNsdPort(context: Context, private val executor: Executor) : NsdPort {
    private val nsd: NsdManager = requireNotNull(context.getSystemService(NsdManager::class.java)) { "no NSD service" }
    private val registrations = ConcurrentHashMap<NsdPort.RegistrationListener, NsdManager.RegistrationListener>()
    private val discoveries = ConcurrentHashMap<NsdPort.DiscoveryListener, NsdManager.DiscoveryListener>()
    private val infoCallbacks = ConcurrentHashMap<NsdPort.InfoCallback, NsdManager.ServiceInfoCallback>()

    override val hasServiceInfoCallback: Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE

    override fun register(service: NsdService, listener: NsdPort.RegistrationListener) {
        val info = NsdServiceInfo().apply {
            serviceName = service.name
            serviceType = service.type
            port = service.port
            // Lossless: the values come from Rust strings.
            for ((key, value) in service.attributes) setAttribute(key, value?.let { String(it, Charsets.UTF_8) })
        }
        val platform = object : NsdManager.RegistrationListener {
            override fun onServiceRegistered(info: NsdServiceInfo) = listener.registered(info.serviceName.orEmpty())
            override fun onRegistrationFailed(info: NsdServiceInfo, code: Int) = listener.registrationFailed(code)
            override fun onServiceUnregistered(info: NsdServiceInfo) = listener.unregistered()
            override fun onUnregistrationFailed(info: NsdServiceInfo, code: Int) = listener.unregistrationFailed(code)
        }
        registrations[listener] = platform
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            nsd.registerService(info, NsdManager.PROTOCOL_DNS_SD, executor, platform)
        } else {
            nsd.registerService(info, NsdManager.PROTOCOL_DNS_SD, platform)
        }
    }

    override fun unregister(listener: NsdPort.RegistrationListener) {
        registrations.remove(listener)?.let(nsd::unregisterService)
    }

    override fun discover(type: String, listener: NsdPort.DiscoveryListener) {
        val platform = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) = Unit
            override fun onDiscoveryStopped(serviceType: String) = Unit
            override fun onStartDiscoveryFailed(serviceType: String, code: Int) = listener.startFailed(code)
            override fun onStopDiscoveryFailed(serviceType: String, code: Int) = Unit
            override fun onServiceFound(info: NsdServiceInfo) = listener.found(info.toService())
            override fun onServiceLost(info: NsdServiceInfo) = listener.lost(info.toService())
        }
        discoveries[listener] = platform
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            nsd.discoverServices(type, NsdManager.PROTOCOL_DNS_SD, null as android.net.Network?, executor, platform)
        } else {
            nsd.discoverServices(type, NsdManager.PROTOCOL_DNS_SD, platform)
        }
    }

    override fun stopDiscovery(listener: NsdPort.DiscoveryListener) {
        discoveries.remove(listener)?.let(nsd::stopServiceDiscovery)
    }

    override fun resolve(service: NsdService, listener: NsdPort.ResolveListener) {
        val platform = object : NsdManager.ResolveListener {
            override fun onServiceResolved(info: NsdServiceInfo) = listener.resolved(info.toService())
            override fun onResolveFailed(info: NsdServiceInfo, code: Int) = listener.failed(info.toService(), code)
        }
        val info = service.toInfo()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            nsd.resolveService(info, executor, platform)
        } else {
            @Suppress("DEPRECATION")
            nsd.resolveService(info, platform)
        }
    }

    override fun registerInfoCallback(service: NsdService, callback: NsdPort.InfoCallback) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            throw IllegalStateException("service-info callbacks need API 34")
        }
        val platform = object : NsdManager.ServiceInfoCallback {
            override fun onServiceUpdated(info: NsdServiceInfo) = callback.updated(info.toService())
            override fun onServiceLost() = callback.lost()
            override fun onServiceInfoCallbackRegistrationFailed(code: Int) = callback.registrationFailed(code)
            override fun onServiceInfoCallbackUnregistered() = Unit
        }
        infoCallbacks[callback] = platform
        nsd.registerServiceInfoCallback(service.toInfo(), executor, platform)
    }

    override fun unregisterInfoCallback(callback: NsdPort.InfoCallback) {
        val platform = infoCallbacks.remove(callback) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) nsd.unregisterServiceInfoCallback(platform)
    }

    private fun NsdService.toInfo() = NsdServiceInfo().apply {
        serviceName = name
        serviceType = type
    }

    private fun NsdServiceInfo.toService(): NsdService {
        val addresses = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            hostAddresses.mapNotNull { it.hostAddress }
        } else {
            @Suppress("DEPRECATION")
            listOfNotNull(host?.hostAddress)
        }
        return NsdService(
            name = serviceName.orEmpty(),
            type = serviceType.orEmpty(),
            port = port,
            attributes = attributes.orEmpty(),
            addresses = addresses,
        )
    }
}
