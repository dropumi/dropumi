// SPDX-License-Identifier: MPL-2.0
//
// The app's live graph: the Rust core with the NSD adapter, UriGuard, the
// staging copy, Save to Downloads, the transfer service starter (ADR-0013
// D7) and the app model, on the given folders and settings. Opening the
// core reads or creates the identity, so the loader does it off the main
// thread when the main screen first asks.
//
// DropumiApplication builds it on the app's standard folders. The
// instrumented tests' interop pass builds the same graph on temporary
// folders (noBackupFilesDir/interop-<uuid>), so it runs the app's own
// wiring and never the app's configuration.

package io.github.dropumi

import android.app.Application
import android.os.Environment
import android.os.StatFs
import android.webkit.MimeTypeMap
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.PendingShare
import io.github.dropumi.appstate.SettingsStore
import io.github.dropumi.core.AppPaths
import io.github.dropumi.core.LiveCoreClient
import io.github.dropumi.core.discovery.LocalAddresses
import io.github.dropumi.core.discovery.NsdDiscovery
import io.github.dropumi.core.files.AndroidDocumentSource
import io.github.dropumi.core.files.AndroidDocuments
import io.github.dropumi.core.files.ReceivedExport
import io.github.dropumi.core.files.StagingCopy
import io.github.dropumi.core.files.UriGuard
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

object LiveGraph {
    /** The graph on [paths], with [settings]. Nothing is opened until the loader loads. */
    fun build(app: Application, paths: AppPaths, settings: SettingsStore): AppGraph {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val pendingShare = PendingShare()
        val guard = UriGuard(app, listOf(paths.configDir, paths.appDataDir))
        val resolver = app.contentResolver
        val loader = AppLoader(scope, Dispatchers.IO) {
            paths.prepare()
            paths.clearOutgoing()
            val addresses = LocalAddresses(app)
            val nsd = NsdDiscovery.create(app, addresses)
            val core = LiveCoreClient(paths.configDir, paths.receiveDir, nsd)
            val source = AndroidDocumentSource(resolver)
            val staging = StagingCopy(source, guard, paths.outgoingDir) { StatFs(paths.outgoingDir.path).availableBytes }
            AppModel(
                core,
                settings,
                HistoryStore(paths.historyFile),
                scope,
                Dispatchers.IO,
                advertising = nsd.adapter,
                localAddresses = addresses::ipv4,
                documents = AndroidDocuments(source, guard, staging),
                pendingShare = pendingShare,
                receiveDir = paths.receiveDir,
                exporter = ReceivedExport(
                    resolver,
                    paths.receiveDir,
                    sharedFreeSpace = ::sharedStorageFreeBytes,
                    mimeLookup = { MimeTypeMap.getSingleton().getMimeTypeFromExtension(it) },
                ),
                services = AndroidServiceStarter(app),
            )
        }
        return AppGraph(scope, loader, pendingShare, guard, ReceivedFileActions(app, paths.receiveDir))
    }

    /** Free bytes in shared storage, where Save writes (API 29+). */
    private fun sharedStorageFreeBytes(): Long =
        @Suppress("DEPRECATION")
        StatFs(Environment.getExternalStorageDirectory().path).availableBytes
}
