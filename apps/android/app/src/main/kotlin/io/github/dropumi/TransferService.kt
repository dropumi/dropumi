// SPDX-License-Identifier: MPL-2.0
//
// The transfer service (ADR-0013 D7): a dataSync foreground service that
// lets running transfers finish after the user leaves Dropumi. The model
// starts it, only while Dropumi is visible (AndroidServiceStarter), and it
// stops itself once the model no longer wants it (the last transfer
// ended). It shows one progress notification (NotificationFactory: counts
// and sizes, never a peer or file name) and holds a partial wake lock while
// it runs, renewed on progress.
//
// - START_NOT_STICKY: after the process dies there is nothing to resume, so
//   the system never restarts it.
// - Every start goes to the foreground first, even when there is nothing to
//   do: a service started with startForegroundService that stops without
//   calling startForeground crashes the app.
// - It stops with the newest start id only, so a start on its way is never
//   cut off; that start goes to the foreground again.
// - The system's dataSync time limit (API 35+, onTimeout) cancels every
//   running send and asks the core to cancel every running receive, and the
//   service stops at once. The core cannot cancel a receive that was
//   already running when accepting stopped (AppModel.transfersTimedOut);
//   that receive goes on without the service.
package io.github.dropumi

import android.Manifest
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.ServiceCompat
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.TransferRecord
import io.github.dropumi.appstate.TransferServiceState
import io.github.dropumi.notify.NotificationFactory
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

class TransferService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var watching: Job? = null
    private var lastStartId = 0
    private var wakeLock: PowerManager.WakeLock? = null
    private var lastBytesDone: ULong? = null

    private val model: AppModel? get() = (application as AppGraphHost).graph.loader.model

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        lastStartId = startId
        val model = model
        if (!enterForeground(model?.state?.value?.active.orEmpty())) {
            model?.transferServiceRefused()
            stopNow()
            return START_NOT_STICKY
        }
        if (model == null || model.state.value.transferService != TransferServiceState.RUNNING) {
            stopNow()
            return START_NOT_STICKY
        }
        renewWakeLock()
        if (watching == null) watching = scope.launch { watch(model) }
        return START_NOT_STICKY
    }

    /** Goes to the foreground with the notification for [active]. False if the system refused it. */
    private fun enterForeground(active: List<TransferRecord>): Boolean {
        NotificationFactory.createChannels(this)
        return try {
            ServiceCompat.startForeground(
                this,
                NotificationFactory.TRANSFER_NOTIFICATION_ID,
                NotificationFactory.transfers(this, active),
                // Service types exist from API 29; before it there is none to give.
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0,
            )
            true
        } catch (_: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException (API 31+; on API 35+
            // also the spent dataSync time limit) is one. The transfers go on
            // without the service.
            false
        } catch (_: SecurityException) {
            false
        }
    }

    /** Follows the model: stops when it no longer wants the service; otherwise keeps the notification current. */
    private suspend fun watch(model: AppModel) = coroutineScope {
        launch {
            model.state.map { it.transferService == TransferServiceState.RUNNING }
                .distinctUntilChanged()
                .collect { running -> if (!running) stopNow() }
        }
        model.state.map { it.active }
            .distinctUntilChanged()
            .conflate()
            .collect { active ->
                if (model.state.value.transferService != TransferServiceState.RUNNING) return@collect
                val done = active.fold(0uL) { sum, record -> sum + (record.progress?.bytesDone ?: 0uL) }
                if (done != lastBytesDone) {
                    lastBytesDone = done
                    renewWakeLock()
                }
                post(active)
                // At most one update a second; the newest state is shown next.
                delay(UPDATE_INTERVAL_MS)
            }
    }

    private fun post(active: List<TransferRecord>) {
        // API 33+: without the permission nothing is shown, and the service
        // still runs (the system lists it in its task manager).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        val manager = getSystemService(NotificationManager::class.java) ?: return
        manager.notify(NotificationFactory.TRANSFER_NOTIFICATION_ID, NotificationFactory.transfers(this, active))
    }

    /** Keeps the CPU awake [WAKE_LOCK_MS] longer; renewed on progress, so a stalled transfer lets it go. */
    private fun renewWakeLock() {
        val lock = wakeLock ?: getSystemService(PowerManager::class.java)
            ?.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, WAKE_LOCK_TAG)
            ?.apply { setReferenceCounted(false) }
            ?.also { wakeLock = it }
        lock?.acquire(WAKE_LOCK_MS)
    }

    private fun releaseWakeLock() {
        wakeLock?.takeIf { it.isHeld }?.release()
    }

    private fun stopNow() {
        watching?.cancel()
        watching = null
        releaseWakeLock()
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelfResult(lastStartId)
    }

    /**
     * The system's dataSync time limit ran out (API 35+): cancel what can be
     * cancelled (AppModel.transfersTimedOut) and stop now.
     */
    override fun onTimeout(startId: Int, fgsType: Int) {
        model?.transfersTimedOut()
        stopNow()
    }

    override fun onDestroy() {
        scope.cancel()
        releaseWakeLock()
        super.onDestroy()
    }

    private companion object {
        const val UPDATE_INTERVAL_MS = 1_000L
        const val WAKE_LOCK_MS = 10 * 60 * 1_000L
        const val WAKE_LOCK_TAG = "dropumi:transfers"
    }
}
