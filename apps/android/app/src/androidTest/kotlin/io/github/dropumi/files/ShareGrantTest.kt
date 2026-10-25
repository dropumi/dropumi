// SPDX-License-Identifier: MPL-2.0
//
// The read grant of a share outlives ShareActivity (ADR-0013 D6). Another
// app (ForeignSharer and ForeignProvider: the test package's own UID)
// shares a document it alone can open; ShareActivity hands it over and
// finishes at once, and the app must still read it afterwards, because
// ShareActivity passed the grant on to MainActivity. The app runs on the
// fake core (the draft); the read itself goes through the app's real
// document path (AndroidDocumentSource, ContentUriFile and UriGuard).

package io.github.dropumi.files

import android.app.Activity
import android.content.Intent
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.MainActivity
import io.github.dropumi.ShareActivity
import io.github.dropumi.core.files.AndroidDocumentSource
import io.github.dropumi.core.files.ContentUriFile
import io.github.dropumi.foreign.ForeignProvider
import io.github.dropumi.foreign.ForeignSharer
import io.github.dropumi.shell
import java.io.ByteArrayOutputStream
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ShareGrantTest {
    private val fake = FakeScenarioRule()
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    /** The app's activities of [type] in [stage]. */
    private fun activities(type: Class<out Activity>, stage: Stage): List<Activity> {
        var found: List<Activity> = emptyList()
        instrumentation.runOnMainSync {
            found = ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(stage).filter { type.isInstance(it) }
        }
        return found
    }

    /**
     * Whether ShareActivity is still in the system's activity history (the
     * grant it received is released with its record).
     */
    private fun shareActivityRecorded(): Boolean =
        HISTORY_RECORD.findAll(shell("dumpsys activity activities")).any { it.value.contains(ShareActivity::class.java.name) }

    private fun readAll(file: ContentUriFile): ByteArray {
        val reader = file.openReader()
        val out = ByteArrayOutputStream()
        while (true) {
            val chunk = reader.read(CHUNK)
            if (chunk.isEmpty()) break
            out.write(chunk)
        }
        return out.toByteArray()
    }

    @Test
    fun uriGrantSurvivesShareActivityFinish() {
        val uri = ForeignProvider.documentUri(instrumentation.context)
        val app = fake.app
        val source = AndroidDocumentSource(app.contentResolver)
        val guard = app.graph.uriGuard

        // Control: without a grant the app cannot open the other app's document.
        try {
            app.contentResolver.openFileDescriptor(uri, "r")?.close()
            fail("the document was readable without a grant")
        } catch (_: SecurityException) {
        }

        instrumentation.context.startActivity(
            Intent().setClassName(instrumentation.context, ForeignSharer::class.java.name).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        try {
            compose.waitUntil(TIMEOUT_MS) { fake.app.graph.loader.model?.state?.value?.draft?.size == 1 }
            compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag("send.go").fetchSemanticsNodes().isNotEmpty() }
            compose.waitUntil(TIMEOUT_MS) { !shareActivityRecorded() }
            assertEquals(1, activities(MainActivity::class.java, Stage.RESUMED).size)

            val created = ContentUriFile.create(source, uri.toString(), guard)
            val file = (created as? ContentUriFile.Created.Allowed)?.file ?: throw AssertionError("refused: $created")
            assertEquals(ForeignProvider.DISPLAY_NAME, file.displayName())
            assertArrayEquals(ForeignProvider.content(), readAll(file))
        } finally {
            for (stage in listOf(Stage.RESUMED, Stage.PAUSED, Stage.STARTED, Stage.STOPPED, Stage.CREATED)) {
                activities(MainActivity::class.java, stage).forEach { activity -> instrumentation.runOnMainSync { activity.finish() } }
            }
            compose.waitUntil(TIMEOUT_MS) {
                listOf(Stage.RESUMED, Stage.PAUSED, Stage.STARTED, Stage.STOPPED, Stage.CREATED)
                    .all { activities(MainActivity::class.java, it).isEmpty() }
            }
        }
    }

    private companion object {
        const val TIMEOUT_MS = 15_000L
        const val CHUNK = 8_192u

        /** A history entry: `* Hist #0: ActivityRecord{...}` (the spacing differs by API level). */
        val HISTORY_RECORD = Regex("""Hist\s+#\d+: ActivityRecord\{[^}]*\}""")
    }
}
