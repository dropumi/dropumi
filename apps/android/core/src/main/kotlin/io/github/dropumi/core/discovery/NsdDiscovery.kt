// SPDX-License-Identifier: MPL-2.0
//
// The PlatformDiscovery the core calls (each call on a thread of its own).
// No stack trace reaches Rust (R24): for any exception that is not a
// CallbackException the generated bindings would send its stack trace to
// Rust, and a trace can carry addresses or names. So every failure becomes
// CallbackException.Failed with a fixed phrase.

package io.github.dropumi.core.discovery

import android.content.Context
import android.os.Build
import io.github.dropumi.ffi.BrowseSink
import io.github.dropumi.ffi.BrowseSinkInterface
import io.github.dropumi.ffi.CallbackException
import io.github.dropumi.ffi.PlatformDiscovery
import io.github.dropumi.ffi.ServiceAd

class NsdDiscovery(val adapter: NsdAdapterCore) : PlatformDiscovery {
    override fun advertise(ad: ServiceAd): String = guarded(ADVERTISE_FAILED) { adapter.advertise(ad) }

    override fun withdraw(instance: String) = guarded(WITHDRAW_FAILED) { adapter.withdraw(instance) }

    override fun startBrowse(sink: BrowseSink) = startBrowsing(sink)

    /** [startBrowse] for any sink (the generated BrowseSink wraps a Rust object). */
    fun startBrowsing(sink: BrowseSinkInterface) = guarded(BROWSE_FAILED) { adapter.startBrowse(sink) }

    override fun stopBrowse() = guarded(STOP_FAILED) { adapter.stopBrowse() }

    private inline fun <T> guarded(phrase: String, body: () -> T): T = try {
        body()
    } catch (e: NsdFailure) {
        throw CallbackException.Failed(e.phrase)
    } catch (_: Throwable) {
        throw CallbackException.Failed(phrase)
    }

    companion object {
        const val ADVERTISE_FAILED = "advertising failed"
        const val WITHDRAW_FAILED = "withdrawing the advertisement failed"
        const val BROWSE_FAILED = "browsing could not start"
        const val STOP_FAILED = "browsing could not stop"

        /**
         * The adapter for this device. The Connectivity module version is not
         * read: no API 33+ pair is on the gate's allowlist (spike S-AN2 has not
         * run), so the gate is closed there either way. Reading it needs a
         * `<queries>` entry for the module package, which comes with the first
         * 33+ allowlist entry.
         */
        fun create(context: Context, addresses: LocalAddresses): NsdDiscovery {
            val executor = NsdAdapterCore.newExecutor()
            val adapter = NsdAdapterCore(
                port = AndroidNsdPort(context, executor),
                gate = HostLabelGate.OBSERVED,
                sdkInt = Build.VERSION.SDK_INT,
                moduleVersion = null,
                ipv4Addresses = addresses::ipv4,
                executor = executor,
            )
            return NsdDiscovery(adapter)
        }
    }
}
