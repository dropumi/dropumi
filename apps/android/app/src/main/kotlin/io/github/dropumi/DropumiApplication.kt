// SPDX-License-Identifier: MPL-2.0
//
// The composition root: the folders (ADR-0013 D3/D4), the Rust core and the
// app model. Opening the core reads or creates the identity, so the loader
// does it off the main thread when the main screen first asks.

package io.github.dropumi

import android.app.Application
import android.os.Build
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.core.AppPaths
import io.github.dropumi.core.LiveCoreClient
import io.github.dropumi.core.discovery.LocalAddresses
import io.github.dropumi.core.discovery.NsdDiscovery
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

class DropumiApplication : Application(), AppGraphHost {
    override val graph: AppGraph by lazy { build() }

    private fun build(): AppGraph {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val paths = AppPaths.standard(this)
        val settings = PrefsSettingsStore(getSharedPreferences(SETTINGS_FILE, MODE_PRIVATE), Build.VERSION.SDK_INT)
        val loader = AppLoader(scope, Dispatchers.IO) {
            paths.prepare()
            paths.clearOutgoing()
            val addresses = LocalAddresses(this)
            val nsd = NsdDiscovery.create(this, addresses)
            val core = LiveCoreClient(paths.configDir, paths.receiveDir, nsd)
            AppModel(
                core,
                settings,
                HistoryStore(paths.historyFile),
                scope,
                Dispatchers.IO,
                advertising = nsd.adapter,
                localAddresses = addresses::ipv4,
            )
        }
        return AppGraph(loader)
    }

    private companion object {
        const val SETTINGS_FILE = "settings"
    }
}
