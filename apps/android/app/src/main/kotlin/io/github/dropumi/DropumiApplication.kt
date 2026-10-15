// SPDX-License-Identifier: MPL-2.0
//
// The composition root: the folders (ADR-0013 D3/D4), the Rust core with the
// NSD adapter, UriGuard and the app model. Opening the core reads or creates
// the identity, so the loader does it off the main thread when the main
// screen first asks.

package io.github.dropumi

import android.app.Application
import android.os.Build
import android.os.Environment
import android.os.StatFs
import android.webkit.MimeTypeMap
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.PendingShare
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

class DropumiApplication : Application(), AppGraphHost {
    override val graph: AppGraph by lazy { build() }

    private fun build(): AppGraph {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val paths = AppPaths.standard(this)
        val settings = PrefsSettingsStore(getSharedPreferences(SETTINGS_FILE, MODE_PRIVATE), Build.VERSION.SDK_INT)
        val pendingShare = PendingShare()
        val guard = UriGuard(this, listOf(paths.configDir, paths.appDataDir))
        val loader = AppLoader(scope, Dispatchers.IO) {
            paths.prepare()
            paths.clearOutgoing()
            val addresses = LocalAddresses(this)
            val nsd = NsdDiscovery.create(this, addresses)
            val core = LiveCoreClient(paths.configDir, paths.receiveDir, nsd)
            val source = AndroidDocumentSource(contentResolver)
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
                    contentResolver,
                    paths.receiveDir,
                    sharedFreeSpace = ::sharedStorageFreeBytes,
                    mimeLookup = { MimeTypeMap.getSingleton().getMimeTypeFromExtension(it) },
                ),
            )
        }
        return AppGraph(scope, loader, pendingShare, guard, ReceivedFileActions(this, paths.receiveDir))
    }

    /** Free bytes in shared storage, where Save writes (API 29+). */
    private fun sharedStorageFreeBytes(): Long =
        @Suppress("DEPRECATION")
        StatFs(Environment.getExternalStorageDirectory().path).availableBytes

    private companion object {
        const val SETTINGS_FILE = "settings"
    }
}
