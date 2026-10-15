// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi

import android.content.ClipData
import android.content.Intent
import android.net.Uri
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.appstate.AppLoader
import java.io.File
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ShareTargetTest {
    /**
     * MainActivity is exported as the launcher, so any app can start it with
     * share extras. It never takes items from its intent: no draft appears.
     */
    @Test
    fun mainActivityIgnoresExternalShareExtras() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val secret = File(context.noBackupFilesDir, "uitest-secret.txt").apply { writeText("never sent") }
        val fileUri = Uri.fromFile(secret)
        val contentUri = Uri.parse("content://com.example.files/document/1")
        val intent = Intent(Intent.ACTION_SEND)
            .setClass(context, MainActivity::class.java)
            .setDataAndType(fileUri, "*/*")
            .putExtra(Intent.EXTRA_STREAM, fileUri)
            .putExtra(Intent.EXTRA_TEXT, "content://com.example.files/document/2")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        intent.clipData = ClipData.newRawUri(null, contentUri)
        try {
            ActivityScenario.launch<MainActivity>(intent).use { scenario ->
                scenario.onActivity { }
                val graph = (context.applicationContext as AppGraphHost).graph
                val model = runBlocking {
                    withTimeout(30_000) { graph.loader.state.first { it is AppLoader.State.Ready } as AppLoader.State.Ready }
                }.model
                runBlocking { model.takeSharedItems() }
                assertTrue(model.state.value.draft.isEmpty())
                assertNull(model.state.value.route)
            }
        } finally {
            secret.delete()
        }
    }
}
