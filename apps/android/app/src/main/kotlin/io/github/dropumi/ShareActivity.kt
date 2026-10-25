// SPDX-License-Identifier: MPL-2.0
//
// The share target (ADR-0013 D6): exported for ACTION_SEND and
// ACTION_SEND_MULTIPLE, with no UI of its own. It reads EXTRA_STREAM only
// (EXTRA_TEXT is ignored), drops what UriGuard's scheme and own-provider
// checks refuse (an early filter: every read runs the whole guard again),
// takes at most 100 items, puts them into the in-process hand-off and
// opens MainActivity in Dropumi's own task. Nothing is sent until the user
// picks a device and taps Send. A prompt never appears in another app's
// task: prompts are drawn only by MainActivity.

package io.github.dropumi

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.core.content.IntentCompat
import io.github.dropumi.appstate.PendingShare

class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val graph = (application as AppGraphHost).graph
        val uris = streams(intent)
            .filter { graph.uriGuard.checkUri(it.toString()) == null }
            .distinct()
            .take(PendingShare.MAX_ITEMS)
        if (uris.isNotEmpty()) {
            graph.pendingShare.put(uris.map(Uri::toString))
            openMain(uris)
        }
        finish()
    }

    /** EXTRA_STREAM of a single or multiple share; nothing else of the intent is read. */
    private fun streams(intent: Intent?): List<Uri> = try {
        when (intent?.action) {
            Intent.ACTION_SEND ->
                listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
            Intent.ACTION_SEND_MULTIPLE ->
                IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
                    .orEmpty().filterNotNull()
            else -> emptyList()
        }
    } catch (_: RuntimeException) {
        // A malformed extra (bad parcel): nothing is taken.
        emptyList()
    }

    /**
     * Opens MainActivity. The URIs go along in ClipData with a read grant only
     * to extend the sharer's grant past this activity; MainActivity never
     * reads them from its intent (it takes them from the hand-off).
     */
    private fun openMain(uris: List<Uri>) {
        val main = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val clip = ClipData.newRawUri(null, uris.first()).apply { uris.drop(1).forEach { addItem(ClipData.Item(it)) } }
        val granting = Intent(main).apply {
            clipData = clip
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        try {
            startActivity(granting)
        } catch (_: SecurityException) {
            // The sharer gave no grant to pass on; the items are read with
            // whatever access Dropumi has.
            startActivity(main)
        }
    }
}
