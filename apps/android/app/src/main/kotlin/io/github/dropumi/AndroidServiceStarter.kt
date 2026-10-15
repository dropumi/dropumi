// SPDX-License-Identifier: MPL-2.0
//
// Starts the app's services for the model (the ServiceStarter port,
// ADR-0013 D7). A start carries its documents' read grants in ClipData with
// FLAG_GRANT_READ_URI_PERMISSION, so the service holds them after the
// activity that received them is gone. The label is empty: no file name is
// put into it.
package io.github.dropumi

import android.content.ClipData
import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import androidx.core.net.toUri
import io.github.dropumi.appstate.GrantSpec
import io.github.dropumi.appstate.ServiceKind
import io.github.dropumi.appstate.ServiceStart
import io.github.dropumi.appstate.ServiceStarter
import io.github.dropumi.notify.NotificationFactory

class AndroidServiceStarter(private val context: Context) : ServiceStarter {
    override fun start(kind: ServiceKind, grants: List<GrantSpec>): ServiceStart {
        val intent = when (kind) {
            ServiceKind.TRANSFER -> Intent(context, TransferService::class.java)
        }
        val readable = grants.filter { it.read }.map { it.uri.toUri() }
        if (readable.isNotEmpty()) {
            intent.clipData = ClipData.newRawUri("", readable.first()).apply {
                readable.drop(1).forEach { addItem(ClipData.Item(it)) }
            }
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        return try {
            ContextCompat.startForegroundService(context, intent)
            ServiceStart.STARTED
        } catch (_: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException (API 31+): Dropumi is
            // no longer visible. The transfer goes on without the service.
            ServiceStart.REFUSED
        } catch (_: SecurityException) {
            ServiceStart.REFUSED
        }
    }

    override val notificationsVisible: Boolean get() = NotificationFactory.notificationsVisible(context)
}
