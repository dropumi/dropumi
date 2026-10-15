// SPDX-License-Identifier: MPL-2.0
//
// Leaving the app with the real core (ADR-0013 D7; plan §6.2): the app
// model's enterBackground (what ProcessLifecycleOwner's ON_STOP calls) stops
// accepting, and a transfer already under way still finishes, while a new
// connection is refused. Two real cores on loopback, on fresh folders under
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
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TransferStatus
import java.io.File
import java.util.concurrent.Executors
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

    private companion object {
        const val WAIT_MS = 30_000L
    }
}
