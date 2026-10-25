// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.overlayhelper

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Color
import android.graphics.PixelFormat
import android.os.Build
import android.os.IBinder
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import java.io.File
import java.security.MessageDigest

/**
 * Test only. Draws one pass-through overlay window over other apps, for the
 * overlay-protection tests (threat model R23):
 *
 * ```
 * am start-foreground-service -n io.github.dropumi.overlayhelper/.OverlayService \
 *   --es layout <over-accept|partial|full-dim> --es token <t> \
 *   [--ei left <px> --ei top <px> --ei width <px> --ei height <px>]
 * am stopservice -n io.github.dropumi.overlayhelper/.OverlayService
 * ```
 *
 * - `over-accept` and `partial` draw an opaque rectangle at the given screen
 *   bounds (required); the test chooses them (over Accept, or over the code
 *   or the peer name).
 * - `full-dim` covers the whole screen at half opacity, below the system's
 *   maximum obscuring opacity on API 31+, so touches still reach the app.
 *
 * Every window is `FLAG_NOT_TOUCHABLE`: touches pass through to the window
 * below, which is what an attacker's overlay does. It draws nothing unless
 * the `token` extra equals the per-run token in this app's `files/token`
 * (written by the test script with `run-as`, at least 32 characters).
 */
class OverlayService : Service() {
    private var overlay: View? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // A service started with startForegroundService must call
        // startForeground, even when it then refuses to draw.
        startInForeground()
        removeOverlay()
        val request = intent?.let { parse(it) }
        if (request == null || !tokenMatches(intent.getStringExtra(EXTRA_TOKEN))) {
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        show(request)
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        removeOverlay()
        super.onDestroy()
    }

    private fun startInForeground() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Test overlay", NotificationManager.IMPORTANCE_LOW),
        )
        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.stat_sys_warning)
            .setContentTitle("Dropumi test overlay")
            .setOngoing(true)
            .build()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        } else {
            0
        }
        ServiceCompat.startForeground(this, NOTIFICATION_ID, notification, type)
    }

    private fun tokenMatches(given: String?): Boolean {
        if (given == null) return false
        val expected = try {
            File(filesDir, TOKEN_FILE).readText().trim()
        } catch (_: Exception) {
            return false
        }
        if (expected.length < MIN_TOKEN_LENGTH) return false
        return MessageDigest.isEqual(expected.toByteArray(), given.toByteArray())
    }

    private fun parse(intent: Intent): Request? {
        val layout = intent.getStringExtra(EXTRA_LAYOUT) ?: return null
        return when (layout) {
            "full-dim" -> Request(fullScreen = true, left = 0, top = 0, width = 0, height = 0)
            "over-accept", "partial" -> {
                val width = intent.getIntExtra("width", -1)
                val height = intent.getIntExtra("height", -1)
                val left = intent.getIntExtra("left", -1)
                val top = intent.getIntExtra("top", -1)
                if (width <= 0 || height <= 0 || left < 0 || top < 0) return null
                Request(fullScreen = false, left = left, top = top, width = width, height = height)
            }
            else -> null
        }
    }

    private fun show(request: Request) {
        val params = WindowManager.LayoutParams(
            if (request.fullScreen) WindowManager.LayoutParams.MATCH_PARENT else request.width,
            if (request.fullScreen) WindowManager.LayoutParams.MATCH_PARENT else request.height,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        )
        params.gravity = Gravity.TOP or Gravity.START
        params.x = request.left
        params.y = request.top
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            params.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        params.alpha = if (request.fullScreen) FULL_DIM_ALPHA else 1f
        val view = View(this).apply {
            setBackgroundColor(if (request.fullScreen) Color.BLACK else Color.MAGENTA)
        }
        getSystemService(WindowManager::class.java).addView(view, params)
        overlay = view
    }

    private fun removeOverlay() {
        val view = overlay ?: return
        overlay = null
        getSystemService(WindowManager::class.java).removeView(view)
    }

    private class Request(
        val fullScreen: Boolean,
        val left: Int,
        val top: Int,
        val width: Int,
        val height: Int,
    )

    private companion object {
        const val CHANNEL_ID = "overlay"
        const val NOTIFICATION_ID = 1
        const val EXTRA_LAYOUT = "layout"
        const val EXTRA_TOKEN = "token"
        const val TOKEN_FILE = "token"
        const val MIN_TOKEN_LENGTH = 32

        /** Below the system's default maximum obscuring opacity (0.8) on API 31+. */
        const val FULL_DIM_ALPHA = 0.5f
    }
}
