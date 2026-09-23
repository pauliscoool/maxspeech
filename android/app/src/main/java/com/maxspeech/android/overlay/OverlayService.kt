package com.maxspeech.android.overlay

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.provider.Settings
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.lifecycleScope
import com.maxspeech.android.MainActivity
import com.maxspeech.android.MaxSpeechApp
import com.maxspeech.android.R
import com.maxspeech.android.a11y.TextInjector
import kotlinx.coroutines.launch

/**
 * Keep-alive foreground service so the floating mic window is not killed in the background.
 * The actual bubble is drawn by [FloatingMicController].
 */
class OverlayService : LifecycleService() {
    private var recording = false

    override fun onCreate() {
        super.onCreate()
        if (!promoteForeground()) {
            Log.e(TAG, "onCreate: promoteForeground failed — stopping")
            stopSelf()
            return
        }
        ensureMicIfSignedIn()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_STOP -> {
                MaxSpeechApp.instance.floatingMic.hide()
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_MIC_ON -> {
                recording = true
                if (!promoteForeground()) {
                    recording = false
                    return START_NOT_STICKY
                }
            }
            ACTION_MIC_OFF -> {
                recording = false
                promoteForeground()
            }
            else -> {
                if (!promoteForeground()) {
                    stopSelf()
                    return START_NOT_STICKY
                }
            }
        }
        ensureMicIfSignedIn()
        return START_STICKY
    }

    private fun ensureMicIfSignedIn() {
        lifecycleScope.launch {
            val user = runCatching { MaxSpeechApp.instance.auth.current() }.getOrNull()
            val signedIn = user != null && !user.local
            if (!signedIn) {
                Log.i(TAG, "ensureMic: skipped — not signed in")
                MaxSpeechApp.instance.floatingMic.hide()
                return@launch
            }
            MaxSpeechApp.instance.floatingMic.ensureShown(this@OverlayService)
        }
    }

    override fun onDestroy() {
        // Keep the bubble if the user still has overlay permission — MainActivity can reattach.
        super.onDestroy()
    }

    private fun promoteForeground(): Boolean {
        if (!Settings.canDrawOverlays(this)) {
            Log.w(TAG, "promoteForeground: no overlay permission")
            return false
        }
        return runCatching {
            val notif = buildNotification()
            val micOk = recording && TextInjector.micGranted(this)
            if (Build.VERSION.SDK_INT >= 34) {
                var type = ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
                if (micOk) type = type or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
                startForeground(NOTIF_ID, notif, type)
            } else {
                startForeground(NOTIF_ID, notif)
            }
            Log.i(TAG, "promoteForeground: ok recording=$recording")
            true
        }.getOrElse {
            Log.e(TAG, "promoteForeground failed", it)
            // Last resort without typed FGS (older / OEM quirks).
            runCatching {
                @Suppress("DEPRECATION")
                startForeground(NOTIF_ID, buildNotification())
                true
            }.getOrElse { e2 ->
                Log.e(TAG, "promoteForeground fallback failed", e2)
                false
            }
        }
    }

    private fun buildNotification(): Notification {
        val nm = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Floating mic", NotificationManager.IMPORTANCE_LOW).apply {
                    setShowBadge(false)
                    description = "Keeps the MaxSpeech mic available while you type"
                },
            )
        }
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_mic)
            .setContentTitle(getString(R.string.overlay_notification_title))
            .setContentText(
                getString(
                    if (recording) R.string.overlay_notification_listening
                    else R.string.overlay_notification_idle,
                ),
            )
            .setContentIntent(open)
            .setOngoing(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
    }

    companion object {
        private const val TAG = "OverlayService"
        const val CHANNEL = "maxspeech_overlay"
        const val NOTIF_ID = 42
        const val ACTION_STOP = "com.maxspeech.android.STOP_OVERLAY"
        const val ACTION_MIC_ON = "com.maxspeech.android.OVERLAY_MIC_ON"
        const val ACTION_MIC_OFF = "com.maxspeech.android.OVERLAY_MIC_OFF"

        fun start(context: Context) {
            val intent = Intent(context, OverlayService::class.java)
            runCatching {
                ContextCompat.startForegroundService(context, intent)
                Log.i(TAG, "start requested")
            }.onFailure { Log.e(TAG, "start failed", it) }
        }

        fun stop(context: Context) {
            runCatching {
                MaxSpeechApp.instance.floatingMic.hide()
            }
            runCatching {
                context.startService(
                    Intent(context, OverlayService::class.java).setAction(ACTION_STOP),
                )
            }
            runCatching {
                context.stopService(Intent(context, OverlayService::class.java))
            }
        }

        fun notifyRecording(context: Context, on: Boolean) {
            val intent = Intent(context, OverlayService::class.java)
                .setAction(if (on) ACTION_MIC_ON else ACTION_MIC_OFF)
            runCatching {
                ContextCompat.startForegroundService(context, intent)
            }
        }
    }
}
