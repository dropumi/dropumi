// SPDX-License-Identifier: MPL-2.0
//
// "Add files": the system document picker (ACTION_OPEN_DOCUMENT, several at
// once), the only picker the app uses. Picked items go to AppModel, which
// describes each one through UriGuard before it joins the draft.
package io.github.dropumi.ui.screens

import android.content.ActivityNotFoundException
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.ui.Router
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

@Stable
class DocumentPicking internal constructor() {
    /** Items picked and still being described. */
    var preparing by mutableIntStateOf(0)
        internal set

    internal var openSendAfter = false
    internal var launch: () -> Unit = {}

    /** Opens the picker; with [openSend], Send opens once something is picked. */
    fun pick(openSend: Boolean) {
        openSendAfter = openSend
        launch()
    }
}

@Composable
fun rememberDocumentPicking(model: AppModel, router: Router, scope: CoroutineScope): DocumentPicking {
    val picking = remember { DocumentPicking() }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        if (picking.openSendAfter) router.openSend()
        picking.preparing += uris.size
        scope.launch {
            try {
                model.addDocuments(uris.map { it.toString() })
            } finally {
                picking.preparing -= uris.size
            }
        }
    }
    SideEffect {
        picking.launch = {
            try {
                launcher.launch(arrayOf("*/*"))
            } catch (_: ActivityNotFoundException) {
                model.notice("Files can't be chosen", "No app on this device can pick files.")
            }
        }
    }
    return picking
}
