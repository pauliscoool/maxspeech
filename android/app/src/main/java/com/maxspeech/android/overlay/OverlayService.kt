package com.maxspeech.android.overlay

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.PixelFormat
import android.os.Build
import android.provider.Settings
import android.view.Gravity
import android.view.WindowManager
import android.widget.FrameLayout
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.lifecycleScope
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
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.ui.overlay.OverlayCapsule
import com.maxspeech.android.ui.theme.MaxSpeechTheme
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

class OverlayService : LifecycleService(), SavedStateRegistryOwner, ViewModelStoreOwner {
    private val savedStateRegistryController = SavedStateRegistryController.create(this)
    override val savedStateRegistry: SavedStateRegistry
        get() = savedStateRegistryController.savedStateRegistry
    private val overlayViewModelStore = ViewModelStore()
    override val viewModelStore: ViewModelStore
        get() = overlayViewModelStore

    private var windowManager: WindowManager? = null
    private var host: FrameLayout? = null
    private var composeView: ComposeView? = null
    private var layoutParams: WindowManager.LayoutParams? = null
    private var placeJob: Job? = null
    private var recording = false
    private var foregroundReady = false
    /** User dragged the bubble — keep X; still snap Y above the keyboard. */
    private var userMovedX = false

    init {
        savedStateRegistryController.performAttach()
    }

    override fun onCreate() {
        savedStateRegistryController.performRestore(null)
        super.onCreate()
        if (!promoteForeground()) {
            stopSelf()
        }
    }

    override fun onDestroy() {
        placeJob?.cancel()
        placeJob = null
        detachOverlay()
        overlayViewModelStore.clear()
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        when (intent?.action) {
            ACTION_STOP -> {
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_MIC_ON -> {
                recording = true
                if (!promoteForeground()) {
                    recording = false
                    return START_NOT_STICKY
                }
                attachOverlay()
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
                attachOverlay()
            }
        }
        return START_NOT_STICKY
    }

    private fun promoteForeground(): Boolean {
        if (!Settings.canDrawOverlays(this)) return false
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
            foregroundReady = true
            true
        }.getOrElse {
            foregroundReady = false
            false
        }
    }

    private fun buildNotification(): Notification {
        val nm = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= 26) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Overlay", NotificationManager.IMPORTANCE_LOW).apply {
                    setShowBadge(false)
                    description = "Keeps the dictation mic available when the keyboard is open"
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

    private fun attachOverlay() {
        if (!foregroundReady || !Settings.canDrawOverlays(this)) {
            stopSelf()
            return
        }
        if (host != null) {
            startPlacing()
            return
        }
        runCatching {
            val wm = getSystemService(WINDOW_SERVICE) as WindowManager
            windowManager = wm
            val dm = resources.displayMetrics
            val bubble = (72 * dm.density).toInt()
            val params = WindowManager.LayoutParams(
                WindowManager.LayoutParams.WRAP_CONTENT,
                WindowManager.LayoutParams.WRAP_CONTENT,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                    WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                    WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
                PixelFormat.TRANSLUCENT,
            )
            params.gravity = Gravity.TOP or Gravity.START
            params.x = (dm.widthPixels - bubble - (20 * dm.density).toInt()).coerceAtLeast(0)
            params.y = (dm.heightPixels * 0.55f).toInt()
            layoutParams = params
            val compose = ComposeView(this).apply {
                setViewTreeLifecycleOwner(this@OverlayService)
                setViewTreeSavedStateRegistryOwner(this@OverlayService)
                setViewTreeViewModelStoreOwner(this@OverlayService)
                setContent {
                    val settings by MaxSpeechApp.instance.settings.flow.collectAsState(
                        initial = AppSettings(),
                    )
                    val ui by MaxSpeechApp.instance.dictation.ui.collectAsState()
                    val focus by TextInjector.inputFocus.collectAsState()
                    val dictating = ui.phase != DictationPhase.Idle && ui.phase != DictationPhase.Error
                    val keyboardUp = focus.imeTop > 0
                    val a11yOn = TextInjector.isAccessibilityOn(this@OverlayService)
                    // With Accessibility: pop up when the keyboard is up (or while dictating).
                    // Without it yet: keep the bubble visible so the control is discoverable.
                    val visible = dictating || keyboardUp || !a11yOn
                    MaxSpeechTheme(
                        theme = settings.theme,
                        glassAlpha = settings.glassAlpha,
                        blurStrength = settings.blurStrength,
                    ) {
                        AnimatedVisibility(
                            visible = visible,
                            enter = fadeIn(tween(180)) + scaleIn(tween(200), initialScale = 0.85f),
                            exit = fadeOut(tween(140)) + scaleOut(tween(140), targetScale = 0.9f),
                        ) {
                            OverlayCapsule(
                                ui = ui,
                                glassAlpha = settings.glassAlpha,
                                onHoldStart = {
                                    val pkg = TextInjector.foregroundPackage().orEmpty()
                                    notifyRecording(this@OverlayService, true)
                                    MaxSpeechApp.instance.dictation.start(pkg, paste = true)
                                },
                                onHoldEnd = { MaxSpeechApp.instance.dictation.stopAndFinish() },
                                onCancel = {
                                    MaxSpeechApp.instance.dictation.cancel()
                                    notifyRecording(this@OverlayService, false)
                                },
                                onConfirm = {
                                    MaxSpeechApp.instance.dictation.confirmPaste()
                                    notifyRecording(this@OverlayService, false)
                                },
                                onDragBy = { dx, dy -> applyDrag(dx, dy) },
                                modifier = Modifier.padding(4.dp),
                            )
                        }
                    }
                }
            }
            composeView = compose
            val frame = FrameLayout(this).apply { addView(compose) }
            wm.addView(frame, params)
            host = frame
            startPlacing()
        }.onFailure {
            detachOverlay()
            stopSelf()
        }
    }

    private fun applyDrag(dx: Float, dy: Float) {
        val params = layoutParams ?: return
        val dm = resources.displayMetrics
        val pad = (8 * dm.density).toInt()
        val size = (72 * dm.density).toInt()
        params.x = (params.x + dx.toInt()).coerceIn(pad, (dm.widthPixels - size - pad).coerceAtLeast(pad))
        params.y = (params.y + dy.toInt()).coerceIn(pad, (dm.heightPixels - size - pad).coerceAtLeast(pad))
        userMovedX = true
        runCatching { windowManager?.updateViewLayout(host, params) }
    }

    private fun startPlacing() {
        if (placeJob?.isActive == true) return
        val app = MaxSpeechApp.instance
        placeJob = lifecycleScope.launch {
            combine(app.dictation.ui, TextInjector.inputFocus) { ui, focus -> ui to focus }
                .collect { (ui, focus) ->
                    val params = layoutParams ?: return@collect
                    val dm = resources.displayMetrics
                    val bubble = (72 * dm.density).toInt()
                    val pad = (12 * dm.density).toInt()
                    val dictating = ui.phase != DictationPhase.Idle && ui.phase != DictationPhase.Error
                    // Park just above the keyboard when it opens; keep user's X if they dragged.
                    val y = when {
                        focus.imeTop > bubble -> focus.imeTop - bubble - pad
                        dictating -> (dm.heightPixels * 0.55f).toInt()
                        else -> params.y
                    }
                    params.y = y.coerceIn(pad, (dm.heightPixels - bubble - pad).coerceAtLeast(pad))
                    if (!userMovedX) {
                        params.x = (dm.widthPixels - bubble - (20 * dm.density).toInt()).coerceAtLeast(pad)
                    }
                    runCatching { windowManager?.updateViewLayout(host, params) }
                }
        }
    }

    private fun detachOverlay() {
        runCatching { composeView?.disposeComposition() }
        composeView = null
        host?.let { runCatching { windowManager?.removeViewImmediate(it) } }
        host = null
        layoutParams = null
        userMovedX = false
    }

    companion object {
        const val CHANNEL = "maxspeech_overlay"
        const val NOTIF_ID = 42
        const val ACTION_STOP = "com.maxspeech.android.STOP_OVERLAY"
        const val ACTION_MIC_ON = "com.maxspeech.android.OVERLAY_MIC_ON"
        const val ACTION_MIC_OFF = "com.maxspeech.android.OVERLAY_MIC_OFF"

        fun start(context: Context) {
            val intent = Intent(context, OverlayService::class.java)
            runCatching {
                ContextCompat.startForegroundService(context, intent)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, OverlayService::class.java))
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
