// SPDX-License-Identifier: MPL-2.0
//
// Leaving the app with the real core (ADR-0013 D7; plan §6.2): the app
// model's enterBackground (what ProcessLifecycleOwner's ON_STOP calls) stops
// accepting, and a transfer already under way still finishes, while a new
// connection is refused. Such a transfer cannot be cancelled on this core,
// and Cancel says so. Two real cores on loopback, on fresh folders under
// noBackupFilesDir (never the app's own configuration). No service is
// started.
package io.github.dropumi.realcore

import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.MemorySettingsStore
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.TransferPhase
import io.github.dropumi.ffi.CallbackException
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.ForeignFile
import io.github.dropumi.ffi.ForeignReader
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TransferStatus
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BackgroundReceiveTest {
    private val data = ByteArray(300_000) { (it % 241).toByte() }

    @Test
    fun stopAcceptingOnBackgroundKeepsTransfers() {
        val executor = Executors.newSingleThreadExecutor()
        val main = executor.asCoroutineDispatcher()
        val scope = CoroutineScope(SupervisorJob() + main)
        try {
            TestCore().use { sender ->
                TestCore(collectEvents = false).use { receiver ->
                    sender.client.identity()
                    receiver.client.identity()
                    pair(sender, receiver)
                    // Paired devices only, and every offer asks (the defaults).
                    val settings = MemorySettingsStore(Build.VERSION.SDK_INT).apply { onboarded = true }
                    val model = AppModel(
                        receiver.client,
                        settings,
                        HistoryStore(File(receiver.root, "app/history.json")),
                        scope,
                        Dispatchers.IO,
                        receiveDir = receiver.paths.receiveDir,
                    )
                    // The model's calls run on its own thread, as on the main thread in the app.
                    fun <T> onModel(block: suspend () -> T): T = runBlocking(main) { withTimeout(WAIT_MS) { block() } }

                    onModel { model.start() }
                    val port = requireNotNull(model.state.value.receiver) { "not receiving" }.port
                    val source = File(sender.root, "notes.pdf").also { it.writeBytes(data) }
                    val target = SendTarget.Address("127.0.0.1:$port", null)
                    sender.client.send(target, listOf(SendItem.Path(source.path)), allowUnverified = false)

                    // The offer waits for an answer: the transfer is under way.
                    val prompt = onModel { model.state.first { it.currentPrompt != null }.currentPrompt!! }

                    // The app leaves the screen.
                    onModel {
                        model.enterBackground()
                        model.state.first { it.receiver == null && it.receiverStopping }
                    }

                    // The user answers (from a notification tap in 2c-2; here directly): it still finishes.
                    onModel {
                        val arming = model.state.value.arming
                        model.promptElapsed(requireNotNull(arming.promptId), arming.epoch, PromptArming.ARM_AFTER_MS)
                        model.answer(prompt, true)
                    }
                    assertEquals(TransferStatus.COMPLETED, sender.finished(Direction.OUTGOING).status)
                    val entry = onModel { model.state.first { it.history.isNotEmpty() }.history.first() }
                    assertEquals(HistoryEntry.Outcome.COMPLETED, entry.outcome)
                    val saved = File(receiver.paths.receiveDir, entry.savedAs.single())
                    assertArrayEquals(data, saved.readBytes())

                    // The receiver ended with its transfer, and a new connection is refused.
                    onModel { model.state.first { !it.receiverStopping } }
                    sender.client.send(target, listOf(SendItem.Path(source.path)), allowUnverified = false)
                    assertEquals(TransferStatus.FAILED, sender.finished(Direction.OUTGOING).status)
                    assertTrue("still in the background", model.state.value.isInBackground)
                    assertEquals(null, model.state.value.receiver)
                    assertEquals(1, receiver.paths.receiveDir.listFiles().orEmpty().size)
                }
            }
        } finally {
            scope.cancel()
            executor.shutdown()
        }
    }

    /**
     * Back in front with a receive that was running when the app left: the
     * user taps Cancel. The real core's stopping receiver no longer holds
     * the transfer's canceller (`stop_receiver` in crates/ffi/src/node.rs),
     * so it answers false, and the model says "Could not cancel" instead of
     * nothing; the transfer goes on and completes. This records the core as
     * it is: once the FFI keeps the canceller while the receiver drains,
     * this test must expect the transfer to end CANCELLED instead.
     */
    @Test
    fun cancelAfterLeavingSaysItCouldNotCancel() {
        val executor = Executors.newSingleThreadExecutor()
        val main = executor.asCoroutineDispatcher()
        val scope = CoroutineScope(SupervisorJob() + main)
        val document = HeldDocument(data, first = HELD_AFTER)
        try {
            TestCore().use { sender ->
                TestCore(collectEvents = false).use { receiver ->
                    sender.client.identity()
                    receiver.client.identity()
                    pair(sender, receiver)
                    val settings = MemorySettingsStore(Build.VERSION.SDK_INT).apply { onboarded = true }
                    val model = AppModel(
                        receiver.client,
                        settings,
                        HistoryStore(File(receiver.root, "app/history.json")),
                        scope,
                        Dispatchers.IO,
                        receiveDir = receiver.paths.receiveDir,
                    )
                    fun <T> onModel(block: suspend () -> T): T = runBlocking(main) { withTimeout(WAIT_MS) { block() } }

                    onModel { model.start() }
                    val port = requireNotNull(model.state.value.receiver) { "not receiving" }.port
                    val target = SendTarget.Address("127.0.0.1:$port", null)
                    sender.client.send(target, listOf(SendItem.Stream(document)), allowUnverified = false)
                    val prompt = onModel { model.state.first { it.currentPrompt != null }.currentPrompt!! }

                    // The app leaves the screen, and comes back while the receiver still stops.
                    onModel {
                        model.enterBackground()
                        model.state.first { it.receiver == null && it.receiverStopping }
                        model.becomeActive()
                    }
                    assertTrue(model.state.value.receiverStopping)

                    // The user accepts; data moves until the sender's document holds.
                    onModel {
                        val arming = model.state.value.arming
                        model.promptElapsed(requireNotNull(arming.promptId), arming.epoch, PromptArming.ARM_AFTER_MS)
                        model.answer(prompt, true)
                        model.state.first { s -> s.active.any { it.phase == TransferPhase.Transferring } }
                    }

                    // Cancel: the core cannot, and the model says so.
                    val record = model.state.value.active.single()
                    val banner = onModel {
                        model.cancel(record)
                        model.state.first { it.banner != null }.banner!!
                    }
                    assertEquals("Could not cancel", banner.title)
                    assertEquals(TransferPhase.Transferring, model.state.value.active.single().phase)

                    // The transfer goes on to the end.
                    document.release.countDown()
                    assertEquals(TransferStatus.COMPLETED, sender.finished(Direction.OUTGOING).status)
                    val entry = onModel { model.state.first { it.history.isNotEmpty() }.history.first() }
                    assertEquals(HistoryEntry.Outcome.COMPLETED, entry.outcome)
                    assertArrayEquals(data, File(receiver.paths.receiveDir, entry.savedAs.single()).readBytes())
                }
            }
        } finally {
            document.release.countDown()
            scope.cancel()
            executor.shutdown()
        }
    }

    /**
     * A document read on the core's behalf: its first [first] bytes at once,
     * the rest only after [release] (bounded: a read that waits 30 s fails).
     */
    private class HeldDocument(private val bytes: ByteArray, private val first: Int) : ForeignFile {
        val release = CountDownLatch(1)

        override fun displayName() = "held.bin"
        override fun size(): ULong = bytes.size.toULong()
        override fun modified(): Long? = null
        override fun openReader(): ForeignReader = object : ForeignReader {
            private var at = 0

            override fun read(maxLen: UInt): ByteArray {
                if (at >= first && !release.await(WAIT_MS, TimeUnit.MILLISECONDS)) {
                    throw CallbackException.Failed("test document held too long")
                }
                val limit = if (at < first) first else bytes.size
                val end = minOf(limit, at + maxLen.toInt())
                return bytes.copyOfRange(at, end).also { at = end }
            }

            override fun closeDocument() = Unit
        }
    }

    private companion object {
        const val WAIT_MS = 30_000L

        /** The sender's document holds after this many bytes: the receive is under way, and cannot end. */
        const val HELD_AFTER = 65_536
    }
}
