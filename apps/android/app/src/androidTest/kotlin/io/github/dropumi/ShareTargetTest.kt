// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi

import android.content.ClipData
import android.content.Intent
import android.net.Uri
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.OutgoingFile
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ShareTargetTest {
    /**
     * MainActivity is exported as the launcher, so any app can start it with
     * share extras (EXTRA_STREAM, ClipData, a data URI, a file:// URI). It
     * never takes items from its intent: none of them reaches the draft, and
     * none is read. The graph's Documents accepts every URI and records every
     * describe and prepare, so a URI taken from the intent would show there.
     *
     * Without sleeps: a sentinel is handed over through PendingShare (the
     * path ShareActivity uses) before the launch, so the model's first take
     * (at start, or on resume) picks it up together with anything the
     * activity put there, and describes all of it before the draft shows the
     * sentinel. A second sentinel and an explicit take afterwards catch
     * anything put there later. The draft must then hold the two sentinels
     * only, and Documents must have seen only them.
     */
    @Test
    fun mainActivityIgnoresExternalShareExtras() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val app = context.applicationContext as TestApplication
        val graph = app.graph
        val documents = app.documents
        val secret = File(context.noBackupFilesDir, "uitest-secret.txt").apply { writeText("never sent") }
        val fileUri = Uri.fromFile(secret)
        val intent = Intent(Intent.ACTION_SEND)
            .setClass(context, MainActivity::class.java)
            .setDataAndType(fileUri, "*/*")
            .putExtra(Intent.EXTRA_STREAM, fileUri)
            .putExtra(Intent.EXTRA_TEXT, "content://com.example.files/document/2")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        intent.clipData = ClipData.newRawUri(null, Uri.parse("content://com.example.files/document/1"))
        try {
            // A model left by an earlier test in this process starts from an empty draft.
            graph.loader.model?.let { model ->
                runBlocking(Dispatchers.Main) {
                    model.clearDraft()
                    model.routeHandled()
                }
            }
            graph.pendingShare.take()
            documents.clear()
            graph.pendingShare.put(listOf(BEFORE))

            ActivityScenario.launch<MainActivity>(intent).use { scenario ->
                scenario.onActivity { }
                val model = runBlocking {
                    withTimeout(TIMEOUT_MS) { graph.loader.state.first { it is AppLoader.State.Ready } as AppLoader.State.Ready }
                }.model
                runBlocking {
                    withTimeout(TIMEOUT_MS) { model.state.first { s -> s.draft.any { it.source == document(BEFORE) } } }
                    withContext(Dispatchers.Main) {
                        graph.pendingShare.put(listOf(AFTER))
                        model.takeSharedItems()
                    }
                }
                assertEquals("only the sentinels were read", listOf("describe $BEFORE", "describe $AFTER"), documents.calls)
                assertEquals(listOf(document(BEFORE), document(AFTER)), model.state.value.draft.map { it.source })
                assertTrue("nothing is left waiting", graph.pendingShare.take().isEmpty())
            }
        } finally {
            secret.delete()
        }
    }

    private fun document(uri: String) = OutgoingFile.Source.Document(uri)

    private companion object {
        const val BEFORE = "content://io.github.dropumi.test.sentinel/before"
        const val AFTER = "content://io.github.dropumi.test.sentinel/after"
        const val TIMEOUT_MS = 30_000L
    }
}
