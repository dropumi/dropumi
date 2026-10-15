// SPDX-License-Identifier: MPL-2.0
//
// The NSD adapter against the platform's NsdManager inside one emulator
// guest (label: "emulator, same guest, no LAN"). The gate is opened for
// this test only; the app's gate stays closed (HostLabelGate.OBSERVED).

package io.github.dropumi.discovery

import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.core.discovery.AndroidNsdPort
import io.github.dropumi.core.discovery.HostLabelGate
import io.github.dropumi.core.discovery.NsdAdapterCore
import io.github.dropumi.ffi.BrowseSinkInterface
import io.github.dropumi.ffi.ServiceAd
import io.github.dropumi.ffi.TxtEntry
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NsdInGuestTest {
    private class Sink(private val wanted: String) : BrowseSinkInterface {
        val found = CountDownLatch(1)
        @Volatile var txt: List<TxtEntry> = emptyList()
        @Volatile var port: UShort = 0u
        @Volatile var thread: String = ""

        override fun failed(reason: String) = Unit
        override fun lost(instance: String) = Unit
        override fun resolved(instance: String, txt: List<TxtEntry>, addresses: List<String>, port: UShort) {
            if (instance != wanted) return
            this.txt = txt
            this.port = port
            thread = Thread.currentThread().name
            found.countDown()
        }
    }

    @Test
    fun nsdSelfRegistrationResolvesInGuest() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val sdk = Build.VERSION.SDK_INT
        val module = if (sdk < HostLabelGate.MODULE_SDK) null else 0L
        val gate = HostLabelGate(listOf(HostLabelGate.Observed(sdk, module?.let { it..it })))
        val executor = NsdAdapterCore.newExecutor()
        val adapter = NsdAdapterCore(AndroidNsdPort(context, executor), gate, sdk, module, { listOf("10.0.2.15") }, executor)
        val instance = "dropumi-test-" + UUID.randomUUID().toString().take(8)
        val sink = Sink(instance)
        adapter.startBrowse(sink)
        try {
            adapter.advertise(
                ServiceAd(
                    NsdAdapterCore.SERVICE_TYPE,
                    instance,
                    47999u,
                    listOf(TxtEntry("v", "1".toByteArray()), TxtEntry("n", "Guest test".toByteArray())),
                ),
            )
            assertTrue("own service resolved on API $sdk", sink.found.await(30, TimeUnit.SECONDS))
            assertEquals(47999, sink.port.toInt())
            val txt = sink.txt.associate { it.key to it.value }
            assertArrayEquals("1".toByteArray(), txt["v"])
            assertArrayEquals("Guest test".toByteArray(), txt["n"])
            assertEquals(NsdAdapterCore.THREAD_NAME, sink.thread)
        } finally {
            // The executor is not shut down: NSD still delivers the stop and
            // unregister callbacks to it, and a refused callback would crash
            // the process (its thread is a daemon, as in the app).
            adapter.withdraw(instance)
            adapter.stopBrowse()
        }
    }
}
