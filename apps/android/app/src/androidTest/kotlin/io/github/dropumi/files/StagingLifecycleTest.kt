// SPDX-License-Identifier: MPL-2.0
//
// The app model over the real core sends a document of unknown size from a
// staged copy and deletes the copy's folder when the transfer ends.

package io.github.dropumi.files

import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.MemorySettingsStore
import io.github.dropumi.core.files.AndroidDocumentSource
import io.github.dropumi.core.files.AndroidDocuments
import io.github.dropumi.core.files.StagingCopy
import io.github.dropumi.core.files.UriGuard
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.realcore.TestCore
import io.github.dropumi.realcore.pair
import io.github.dropumi.realcore.targetContext
import java.io.File
import java.util.concurrent.Executors
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class StagingLifecycleTest {
    @Test
    fun stagingFolderRemovedWhenTransferEnds() {
        val main = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        // One io thread: work queued on it runs in order, so the test can
        // wait for the folder deletion without polling.
        val io = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        val scope = CoroutineScope(SupervisorJob() + main)
        TestCore(collectEvents = false).use { sender ->
            TestCore().use { receiver ->
                try {
                    receiver.client.identity()
                    pair(sender, receiver)
                    val port = receiver.receiveFromPaired()
                    val (provider, resolver) = TestDocumentsProvider.attach(targetContext)
                    val bytes = ByteArray(50_000) { (it % 17).toByte() }
                    val uri = provider.add("pipe", TestDocumentsProvider.Doc("notes.txt", null, pipe = bytes))
                    val source = AndroidDocumentSource(resolver)
                    val guard = UriGuard(targetContext, listOf(sender.paths.configDir, sender.paths.appDataDir))
                    val outgoing = sender.paths.outgoingDir
                    val model = AppModel(
                        sender.client,
                        MemorySettingsStore(Build.VERSION.SDK_INT),
                        HistoryStore(sender.paths.historyFile),
                        scope,
                        io,
                        documents = AndroidDocuments(source, guard, StagingCopy(source, guard, outgoing) { Long.MAX_VALUE }),
                    )
                    runBlocking(main) {
                        model.start()
                        assertEquals(1, model.addDocuments(listOf(uri)))
                        model.sendDraft(SendTarget.Address("127.0.0.1:$port", null), "receiver", allowUnverified = false)
                    }
                    assertEquals("one staged copy while it is sent", 1, outgoing.list()?.size)
                    val entry = runBlocking {
                        withTimeout(30_000) { model.state.first { it.history.isNotEmpty() } }.history.first()
                    }
                    assertEquals(HistoryEntry.Outcome.COMPLETED, entry.outcome)
                    // The deletion was queued on io before the history changed.
                    runBlocking { withContext(io) {} }
                    assertTrue("the staged folder is gone", outgoing.list().isNullOrEmpty())
                    assertArrayEquals(bytes, File(receiver.paths.receiveDir, "notes.txt").readBytes())
                } finally {
                    scope.cancel()
                    main.close()
                    io.close()
                }
            }
        }
    }
}
