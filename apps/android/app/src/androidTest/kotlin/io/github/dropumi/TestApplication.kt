// SPDX-License-Identifier: MPL-2.0
//
// The Application of every instrumented test run. Its graph is chosen by the
// runner argument `dropumi_app` when it is first used: the platform builds
// the Application before the runner receives its arguments, so the choice
// cannot be made in newApplication. Only `am instrument` arguments select
// it; no intent or exported component of an app APK can.
//
// - absent or `fake`: the fake core (`:testsupport`) on temporary folders,
//   for UI state tests ("UI state test (fake core)").
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

class TestApplication : Application(), AppGraphHost {
    /** The fake core behind the graph, for tests that push events. */
    lateinit var fakeCore: FakeCoreClient
        private set

    override val graph: AppGraph by lazy {
        when (val choice = InstrumentationRegistry.getArguments().getString(APP_ARGUMENT) ?: FAKE) {
            FAKE -> fakeGraph()
            else -> throw IllegalArgumentException("unknown $APP_ARGUMENT: $choice")
        }
    }

    private fun fakeGraph(): AppGraph {
        val paths = AppPaths.under(File(noBackupFilesDir, "uitest-" + UUID.randomUUID()))
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val core = FakeCoreClient().also { fakeCore = it }
        val pendingShare = PendingShare()
        val loader = AppLoader(scope, Dispatchers.IO) {
            AppModel(
                core,
                MemorySettingsStore(Build.VERSION.SDK_INT),
                HistoryStore(paths.historyFile),
                scope,
                Dispatchers.IO,
                pendingShare = pendingShare,
            )
        }
        return AppGraph(loader, pendingShare, UriGuard(this, listOf(paths.configDir, paths.appDataDir)))
    }

    companion object {
        const val APP_ARGUMENT = "dropumi_app"
        const val FAKE = "fake"
    }
}
