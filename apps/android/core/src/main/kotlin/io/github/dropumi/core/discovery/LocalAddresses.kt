// SPDX-License-Identifier: MPL-2.0
//
// This device's IPv4 addresses on local networks: the advertisement's IPv4
// check and the "ip:port" that This device shows for manual addressing.
// Only Wi-Fi and Ethernet networks count, never one that is also cellular,
// and not via the default network: on Wi-Fi without internet with mobile
// data on, the default network can be cellular (NET-5). Loopback and
// link-local addresses are left out. The process is never bound to a
// network.

package io.github.dropumi.core.discovery

import android.content.Context
import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import java.net.Inet4Address
import java.net.InetAddress
import java.util.concurrent.ConcurrentHashMap

/** One network as the system reports it. */
data class LanCandidate(
    val wifi: Boolean = false,
    val ethernet: Boolean = false,
    val cellular: Boolean = false,
    val addresses: List<InetAddress> = emptyList(),
)

object LocalAddressRules {
    /** Dotted-quad IPv4 addresses of Wi-Fi and Ethernet networks that are not also cellular. */
    fun ipv4(networks: Collection<LanCandidate>): List<String> = networks
        .filter { (it.wifi || it.ethernet) && !it.cellular }
        .flatMap { it.addresses }
        .filterIsInstance<Inet4Address>()
        .filterNot { it.isLoopbackAddress || it.isLinkLocalAddress || it.isAnyLocalAddress }
        .mapNotNull { it.hostAddress }
        .distinct()
}

/** Follows the Wi-Fi and Ethernet networks for the life of the process. */
class LocalAddresses(context: Context) {
    private val transports = ConcurrentHashMap<Network, NetworkCapabilities>()
    private val links = ConcurrentHashMap<Network, LinkProperties>()

    /** One callback object per request: the system refuses a callback registered twice. */
    private fun newCallback() = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            transports[network] = capabilities
        }

        override fun onLinkPropertiesChanged(network: Network, properties: LinkProperties) {
            links[network] = properties
        }

        override fun onLost(network: Network) {
            transports.remove(network)
            links.remove(network)
        }
    }

    init {
        val connectivity = requireNotNull(context.getSystemService(ConnectivityManager::class.java)) { "no connectivity service" }
        for (transport in listOf(NetworkCapabilities.TRANSPORT_WIFI, NetworkCapabilities.TRANSPORT_ETHERNET)) {
            val request = NetworkRequest.Builder()
                .addTransportType(transport)
                // A LAN without internet counts too (NET-5).
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                .build()
            connectivity.registerNetworkCallback(request, newCallback())
        }
    }

    fun ipv4(): List<String> = LocalAddressRules.ipv4(
        transports.mapNotNull { (network, capabilities) ->
            val properties = links[network] ?: return@mapNotNull null
            LanCandidate(
                wifi = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI),
                ethernet = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
                cellular = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR),
                addresses = properties.linkAddresses.map { it.address },
            )
        },
    )
}
