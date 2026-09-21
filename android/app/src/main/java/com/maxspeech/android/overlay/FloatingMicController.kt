package com.maxspeech.android.overlay

import android.content.Context
import android.graphics.PixelFormat
import android.os.Build
import android.provider.Settings
import android.util.Log
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
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.maxspeech.android.MaxSpeechApp
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.ui.overlay.OverlayCapsule
import com.maxspeech.android.ui.theme.MaxSpeechTheme
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

/**
 * Draws the floating mic with TYPE_APPLICATION_OVERLAY from the app process.
 * Kept alive by [OverlayService] when possible; the window itself does not need
 * Accessibility — that is only required for keyboard snap + paste.
 */
class FloatingMicController(private val app: MaxSpeechApp) :
    LifecycleOwner,
    SavedStateRegistryOwner,
    ViewModelStoreOwner {

    private val registry = LifecycleRegistry(this)
    private val savedStateRegistryController = SavedStateRegistryController.create(this)
    private val store = ViewModelStore()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var placeJob: Job? = null

    private var windowManager: WindowManager? = null
    private var host: FrameLayout? = null
    private var composeView: ComposeView? = null
    private var layoutParams: WindowManager.LayoutParams? = null
    private var userMovedX = false
    private var attached = false
    /** Sub-pixel drag accumulation for smoother window moves. */
    private var posX = 0f
    private var posY = 0f

    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry: SavedStateRegistry
        get() = savedStateRegistryController.savedStateRegistry
    override val viewModelStore: ViewModelStore get() = store

    init {
        savedStateRegistryController.performAttach()
        savedStateRegistryController.performRestore(null)
        registry.currentState = Lifecycle.State.CREATED
    }

    fun ensureShown(context: Context = app) {
        if (!Settings.canDrawOverlays(context)) {
            Log.w(TAG, "ensureShown: SYSTEM_ALERT_WINDOW not granted")
            return
        }
        if (attached && host != null) {
            Log.i(TAG, "ensureShown: already attached")
            return
        }
        runCatching { attach() }
            .onSuccess { Log.i(TAG, "ensureShown: attached ok") }
            .onFailure { Log.e(TAG, "ensureShown: attach failed", it) }
    }

    fun hide() {
        placeJob?.cancel()
        placeJob = null
        detach()
    }

    private fun attach() {
        if (host != null) return
        registry.currentState = Lifecycle.State.STARTED
        registry.currentState = Lifecycle.State.RESUMED
        val wm = app.getSystemService(Context.WINDOW_SERVICE) as WindowManager
        windowManager = wm
        val dm = app.resources.displayMetrics
        val bubble = (56 * dm.density).toInt()
        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS or
                WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
            PixelFormat.TRANSLUCENT,
        )
        params.gravity = Gravity.TOP or Gravity.START
        params.x = (dm.widthPixels - bubble - (24 * dm.density).toInt()).coerceAtLeast(0)
        params.y = (dm.heightPixels * 0.58f).toInt()
        posX = params.x.toFloat()
        posY = params.y.toFloat()
        if (Build.VERSION.SDK_INT >= 28) {
            params.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        layoutParams = params

        // Host first with ViewTree owners, then ComposeView, then setContent after
        // the overlay window is attached — avoids ViewTreeLifecycleOwner crashes.
        val frame = FrameLayout(app).apply {
            setViewTreeLifecycleOwner(this@FloatingMicController)
            setViewTreeSavedStateRegistryOwner(this@FloatingMicController)
            setViewTreeViewModelStoreOwner(this@FloatingMicController)
        }
        val compose = ComposeView(app).apply {
            setViewTreeLifecycleOwner(this@FloatingMicController)
            setViewTreeSavedStateRegistryOwner(this@FloatingMicController)
            setViewTreeViewModelStoreOwner(this@FloatingMicController)
        }
        frame.addView(compose)
        composeView = compose
        wm.addView(frame, params)
        host = frame
        attached = true
        compose.setContent {
            val settings by MaxSpeechApp.instance.settings.flow.collectAsState(initial = AppSettings())
            val ui by MaxSpeechApp.instance.dictation.ui.collectAsState()
            val mainUi by MaxSpeechApp.instance.mainUiResumed.collectAsState()
            val dictating = ui.phase != DictationPhase.Idle && ui.phase != DictationPhase.Error
            // Never stack the system bubble on top of MaxSpeech's own home UI.
            val showBubble = dictating || !mainUi
            MaxSpeechTheme(
                theme = settings.theme,
                glassAlpha = settings.glassAlpha,
                blurStrength = settings.blurStrength,
            ) {
                AnimatedVisibility(
                    visible = showBubble,
                    enter = fadeIn(tween(160)) + scaleIn(tween(200), initialScale = 0.88f),
                    exit = fadeOut(tween(120)) + scaleOut(tween(120), targetScale = 0.92f),
                ) {
                    OverlayCapsule(
                        ui = ui,
                        sizeScale = settings.overlaySize,
                        surfaceAlpha = settings.overlayAlpha,
                        onHoldStart = {
                            val pkg = TextInjector.foregroundPackage().orEmpty()
                            OverlayService.notifyRecording(app, true)
                            MaxSpeechApp.instance.dictation.start(pkg, paste = true)
                        },
                        onHoldEnd = { MaxSpeechApp.instance.dictation.stopAndFinish() },
                        onCancel = {
                            MaxSpeechApp.instance.dictation.cancel()
                            OverlayService.notifyRecording(app, false)
                        },
                        onConfirm = {
                            MaxSpeechApp.instance.dictation.confirmPaste()
                            OverlayService.notifyRecording(app, false)
                        },
                        onDragBy = { dx, dy -> applyDrag(dx, dy) },
                        modifier = Modifier.padding(4.dp),
                    )
                }
            }
        }
        startPlacing()
        Log.i(TAG, "attach: window added")
    }

    private fun applyDrag(dx: Float, dy: Float) {
        val params = layoutParams ?: return
        val dm = app.resources.displayMetrics
        val pad = (8 * dm.density)
        val size = (56 * dm.density)
        posX = (posX + dx).coerceIn(pad, (dm.widthPixels - size - pad).coerceAtLeast(pad))
        posY = (posY + dy).coerceIn(pad, (dm.heightPixels - size - pad).coerceAtLeast(pad))
        params.x = kotlin.math.round(posX).toInt()
        params.y = kotlin.math.round(posY).toInt()
        userMovedX = true
        runCatching { windowManager?.updateViewLayout(host, params) }
    }

    private fun startPlacing() {
        if (placeJob?.isActive == true) return
        placeJob = scope.launch {
            combine(app.dictation.ui, TextInjector.inputFocus) { ui, focus -> ui to focus }
                .collect { (ui, focus) ->
                    val params = layoutParams ?: return@collect
                    val dm = app.resources.displayMetrics
                    val bubble = (56 * dm.density).toInt()
                    val pad = (12 * dm.density).toInt()
                    val dictating = ui.phase != DictationPhase.Idle && ui.phase != DictationPhase.Error
                    if (focus.imeTop > bubble) {
                        posY = (focus.imeTop - bubble - pad)
                            .coerceIn(pad, (dm.heightPixels - bubble - pad).coerceAtLeast(pad))
                            .toFloat()
                    } else if (dictating) {
                        posY = dm.heightPixels * 0.55f
                    }
                    if (!userMovedX) {
                        posX = (dm.widthPixels - bubble - (24 * dm.density).toInt())
                            .coerceAtLeast(pad)
                            .toFloat()
                    }
                    params.x = kotlin.math.round(posX).toInt()
                    params.y = kotlin.math.round(posY).toInt()
                    runCatching { windowManager?.updateViewLayout(host, params) }
                }
        }
    }

    private fun detach() {
        runCatching { composeView?.disposeComposition() }
        composeView = null
        host?.let { runCatching { windowManager?.removeViewImmediate(it) } }
        host = null
        layoutParams = null
        attached = false
        userMovedX = false
        if (registry.currentState.isAtLeast(Lifecycle.State.CREATED)) {
            registry.currentState = Lifecycle.State.CREATED
        }
    }

    companion object {
        private const val TAG = "FloatingMic"
    }
}
