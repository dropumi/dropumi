// SPDX-License-Identifier: MPL-2.0
//
// Every Dropumi notification is built here (plan §6.6): today the transfer
// service's progress notification on the "Transfers" channel. Each one is
// VISIBILITY_PRIVATE with a generic public version for the lock screen, has
// no actions, and opens MainActivity through an immutable PendingIntent.
// Nothing in it comes from a peer.
package io.github.dropumi.notify

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import io.github.dropumi.MainActivity
import io.github.dropumi.R
import io.github.dropumi.appstate.TransferRecord

object NotificationFactory {
    const val TRANSFERS_CHANNEL = "transfers"

    /** The transfer service's notification (one for all running transfers). */
    const val TRANSFER_NOTIFICATION_ID = 1

    /** Creates the channels (low importance: no sound, no heads-up). Safe to call again. */
    fun createChannels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(TRANSFERS_CHANNEL, TransferNotificationText.CHANNEL_NAME, NotificationManager.IMPORTANCE_LOW).apply {
                setShowBadge(false)
                lockscreenVisibility = Notification.VISIBILITY_PRIVATE
            },
        )
    }

    /**
     * Whether Dropumi's notifications can be seen: not blocked for the app
     * (on API 33+ that includes the notification permission) or for the
     * Transfers channel. A channel not created yet is not blocked.
     */
    fun notificationsVisible(context: Context): Boolean {
        val manager = NotificationManagerCompat.from(context)
        if (!manager.areNotificationsEnabled()) return false
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return false
        }
        val channel = manager.getNotificationChannel(TRANSFERS_CHANNEL) ?: return true
        return channel.importance != NotificationManager.IMPORTANCE_NONE
    }

    /** The transfer service's notification for [active]: counts and sizes only. */
    fun transfers(context: Context, active: List<TransferRecord>): Notification {
        val content = TransferNotificationText.of(active)
        val public = NotificationCompat.Builder(context, TRANSFERS_CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_dropumi)
            .setContentTitle(TransferNotificationText.PUBLIC_TITLE)
            .setContentIntent(openApp(context))
            .build()
        return NotificationCompat.Builder(context, TRANSFERS_CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_dropumi)
            .setContentTitle(content.title)
            .setContentText(content.text)
            .setProgress(TransferNotificationText.PROGRESS_MAX, content.progress ?: 0, content.progress == null)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(public)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setShowWhen(false)
            .setContentIntent(openApp(context))
            // Shown at once on API 31+, which may otherwise hold a service's notification back for 10 s.
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .build()
    }

    /** Opens Dropumi in front (MainActivity is singleTask). */
    private fun openApp(context: Context): PendingIntent = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
}
