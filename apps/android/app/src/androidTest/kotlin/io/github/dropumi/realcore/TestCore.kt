// SPDX-License-Identifier: MPL-2.0
//
// A real Rust core for instrumented tests, on fresh folders under
// noBackupFilesDir/test-<uuid> (app-private, so the core's owner check
// passes). Never the app's own configuration (noBackupFilesDir/dropumi).

package io.github.dropumi.realcore

import android.content.Context
import android.system.Os
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.core.AppPaths
import io.github.dropumi.core.LiveCoreClient
import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.PlatformDiscovery
import io.github.dropumi.ffi.ReceiveOptions
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.TransferResult
import java.io.File
import java.util.UUID
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

val targetContext: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

/** The permission bits of `path` (no links followed). */
fun modeOf(file: File): Int = Os.lstat(file.path).st_mode and 0b111_111_111

const val MODE_0700 = 0b111_000_000
const val MODE_0600 = 0b110_000_000

class TestCore(discovery: PlatformDiscovery? = null) : AutoCloseable {
    val root: File = File(targetContext.noBackupFilesDir, "test-" + UUID.randomUUID()).also { it.mkdirs() }
    val paths: AppPaths = AppPaths.under(root).also { it.prepare() }
    val client = LiveCoreClient(paths.configDir, paths.receiveDir, discovery)

    @PublishedApi
    internal val queue = LinkedBlockingQueue<NodeEvent>()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    init {
        scope.launch { client.events.collect { queue.addAll(it) } }
    }

    /** The next event of type [T] matching [match] (others are skipped); fails after 30 s. */
    inline fun <reified T : NodeEvent> await(what: String, match: (T) -> Boolean = { true }): T {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(30)
        while (true) {
            val left = deadline - System.nanoTime()
            val event = queue.poll(left.coerceAtLeast(0), TimeUnit.NANOSECONDS)
                ?: throw AssertionError("timed out waiting for $what")
            if (event is T && match(event)) return event
        }
    }

    fun finished(direction: Direction): TransferResult =
        await<NodeEvent.TransferFinished>("a finished $direction transfer") { it.direction == direction }.result

    /** Starts receiving from paired devices on loopback, accepting them without a prompt. */
    fun receiveFromPaired(): UShort = client.startReceiving(
        ReceiveOptions(ReceivePolicy.TrustedOnly, AcceptMode.AUTO_TRUSTED, 0u, false),
    ).port

    override fun close() {
        try {
            client.shutdown()
        } finally {
            scope.cancel()
            root.deleteRecursively()
        }
    }
}

/** Each trusts the other's key, as pairing would. */
fun pair(a: TestCore, b: TestCore) {
    a.client.trustFingerprint(b.client.identity().fingerprint, "B")
    b.client.trustFingerprint(a.client.identity().fingerprint, "A")
}
