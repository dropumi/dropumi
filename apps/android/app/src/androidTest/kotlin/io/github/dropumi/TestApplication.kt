// SPDX-License-Identifier: MPL-2.0
//
// The Application of every instrumented test run. Its graph is chosen by the
// runner argument `dropumi_app` when it is first used: the platform builds
// the Application before the runner receives its arguments, so the choice
// cannot be made in newApplication. Only `am instrument` arguments select
// it; no intent or exported component of an app APK can.
//
// - absent or `fake`: the fake core (`:testsupport`) on temporary folders,
//   for UI state tests ("UI state test (fake core)"). FakeScenarioRule
//   replaces it with a fresh one before each such test.
// Any other value fails the run.

package io.github.dropumi

import android.app.Application
import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.MemorySettingsStore
import io.github.dropumi.appstate.PendingShare
import io.github.dropumi.core.AppPaths
import io.github.dropumi.core.files.UriGuard
import io.github.dropumi.testsupport.FakeCoreClient
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel

class TestApplication : Application(), AppGraphHost {
    /** The fake core behind the graph, for tests that push events. */
    lateinit var fakeCore: FakeCoreClient
        private set

    /** The fake graph's documents: they accept every URI and record every read attempt. */
    lateinit var documents: RecordingDocuments
        private set

    /** The fake graph's settings (in memory). */
    lateinit var settings: MemorySettingsStore
        private set

    private val choice: String by lazy { InstrumentationRegistry.getArguments().getString(APP_ARGUMENT) ?: FAKE }

    @Volatile
    private var current: AppGraph? = null
    private var currentRoot: File? = null

    override val graph: AppGraph
        get() = current ?: synchronized(this) {
            current ?: when (choice) {
                FAKE -> fakeGraph { _, _ -> }
                else -> throw IllegalArgumentException("unknown $APP_ARGUMENT: $choice")
            }.also { current = it }
        }

    /**
     * Replaces the fake graph with a fresh one (a new fake core, settings and
     * temporary folders) set up by [configure] before anything uses it. The
     * old graph's scope is cancelled and its folders deleted. Call it before
     * launching MainActivity, with no activity of an earlier test alive.
     */
    fun resetFakeGraph(configure: (FakeCoreClient, MemorySettingsStore) -> Unit): AppGraph = synchronized(this) {
        check(choice == FAKE) { "resetFakeGraph needs the fake core ($APP_ARGUMENT=$FAKE)" }
        current?.scope?.cancel()
        currentRoot?.deleteRecursively()
        fakeGraph(configure).also { current = it }
    }

    private fun fakeGraph(configure: (FakeCoreClient, MemorySettingsStore) -> Unit): AppGraph {
        val root = File(noBackupFilesDir, "uitest-" + UUID.randomUUID()).also { currentRoot = it }
        val paths = AppPaths.under(root)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val core = FakeCoreClient().also { fakeCore = it }
        val docs = RecordingDocuments().also { documents = it }
        val store = MemorySettingsStore(Build.VERSION.SDK_INT).also { settings = it }
        configure(core, store)
        val pendingShare = PendingShare()
        val loader = AppLoader(scope, Dispatchers.IO) {
            AppModel(
                core,
                store,
                HistoryStore(paths.historyFile),
                scope,
                Dispatchers.IO,
                documents = docs,
                pendingShare = pendingShare,
            )
        }
        return AppGraph(
            scope,
            loader,
            pendingShare,
            UriGuard(this, listOf(paths.configDir, paths.appDataDir)),
            ReceivedFileActions(this, paths.receiveDir),
        )
    }

    companion object {
        const val APP_ARGUMENT = "dropumi_app"
        const val FAKE = "fake"
    }
}
