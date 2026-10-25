// SPDX-License-Identifier: MPL-2.0
//
// The composition root: the app's live graph (LiveGraph) on its standard
// folders (ADR-0013 D3/D4) and its saved settings. The process's foreground
// and background drive receiving (ProcessVisibility).

package io.github.dropumi

import android.app.Application
import android.os.Build
import io.github.dropumi.core.AppPaths

class DropumiApplication : Application(), AppGraphHost {
    override val graph: AppGraph by lazy {
        LiveGraph.build(
            this,
            AppPaths.standard(this),
            PrefsSettingsStore(getSharedPreferences(SETTINGS_FILE, MODE_PRIVATE), Build.VERSION.SDK_INT),
        )
    }

    override fun onCreate() {
        super.onCreate()
        ProcessVisibility.install(this)
    }

    private companion object {
        const val SETTINGS_FILE = "settings"
    }
}
