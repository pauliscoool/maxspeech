package com.maxspeech.android.overlay

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.graphics.PixelFormat
import android.os.Build
import android.view.Gravity
import android.view.WindowManager
import android.widget.FrameLayout
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.ComposeView
import androidx.core.app.NotificationCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.maxspeech.android.MainActivity
import com.maxspeech.android.MaxSpeechApp
import com.maxspeech.android.R
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.ui.overlay.OverlayCapsule
import com.maxspeech.android.ui.theme.MaxSpeechTheme

class OverlayService : LifecycleService(), SavedStateRegistryOwner, ViewModelStoreOwner {
    private val savedStateRegistryController = SavedStateRegistryController.create(this)
    override val savedStateRegistry: SavedStateRegistry
        get() = savedStateRegistryController.savedStateRegistry
    private val overlayViewModelStore = ViewModelStore()
    override val viewModelStore: ViewModelStore
        get() = overlayViewModelStore

    private var windowManager: WindowManager? = null
    private var host: FrameLayout? = null

    init {
        savedStateRegistryController.performAttach()
    }

    override fun onCreate() {
        savedStateRegistryController.performRestore(null)
        super.onCreate()
        startForegroundInternal()
    }

    override fun onDestroy() {
        detachOverlay()
        overlayViewModelStore.clear()
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_STOP -> stopSelf()
            else -> attachOverlay()
        }
        return START_STICKY
    }

    private fun startForegroundInternal() {
        val nm = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Overlay", NotificationManager.IMPORTANCE_LOW),
            )
        }
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val notif = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setContentTitle(getString(R.string.overlay_notification_title))
            .setContentText(getString(R.string.overlay_notification_idle))
            .setContentIntent(open)
            .setOngoing(true)
            .setSilent(true)
            .build()
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(
                42,
                notif,
                android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
                    or android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            )
        } else {
            startForeground(42, notif)
        }
    }

    private fun attachOverlay() {
        if (host != null) return
        val wm = getSystemService(WINDOW_SERVICE) as WindowManager
        windowManager = wm
        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            PixelFormat.TRANSLUCENT,
        )
        params.gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
        params.y = 140
        val compose = ComposeView(this).apply {
            setViewTreeLifecycleOwner(this@OverlayService)
            setViewTreeSavedStateRegistryOwner(this@OverlayService)
            setViewTreeViewModelStoreOwner(this@OverlayService)
            setContent {
                val settings by MaxSpeechApp.instance.settings.flow.collectAsState(
                    initial = AppSettings(),
                )
                val ui by MaxSpeechApp.instance.dictation.ui.collectAsState()
                MaxSpeechTheme(theme = settings.theme, glassAlpha = settings.glassAlpha) {
                    OverlayCapsule(
                        ui = ui,
                        glassAlpha = settings.glassAlpha,
                        onHoldStart = {
                            val pkg = TextInjector.foregroundPackage().orEmpty()
                            MaxSpeechApp.instance.dictation.start(pkg, paste = true)
                        },
                        onHoldEnd = { MaxSpeechApp.instance.dictation.stopAndFinish() },
                        onCancel = { MaxSpeechApp.instance.dictation.cancel() },
                        onConfirm = { MaxSpeechApp.instance.dictation.confirmPaste() },
                    )
                }
            }
        }
        val frame = FrameLayout(this).apply { addView(compose) }
        host = frame
        wm.addView(frame, params)
    }

    private fun detachOverlay() {
        host?.let { runCatching { windowManager?.removeView(it) } }
        host = null
    }

    companion object {
        const val CHANNEL = "maxspeech_overlay"
        const val ACTION_STOP = "com.maxspeech.android.STOP_OVERLAY"
    }
}
