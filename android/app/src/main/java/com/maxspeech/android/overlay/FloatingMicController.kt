package com.maxspeech.android.overlay

import android.content.Context
import android.graphics.PixelFormat
import android.os.Build
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.WindowManager
import android.widget.FrameLayout
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
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
import com.maxspeech.android.data.OverlayStyle
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.ui.components.BlurFade
import com.maxspeech.android.ui.overlay.KeyboardDockBar
import com.maxspeech.android.ui.overlay.OverlayCapsule
import com.maxspeech.android.ui.theme.MaxSpeechTheme
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Floating mic overlay. Anchored by right edge so expand/collapse does not jitter.
 * Position is persisted after the user long-presses and drags.
 * Hold ~3s then drag onto the bottom X to snooze for 10 minutes.
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
    private var snoozeJob: Job? = null

    private var windowManager: WindowManager? = null
    private var host: FrameLayout? = null
    private var composeView: ComposeView? = null
    private var layoutParams: WindowManager.LayoutParams? = null
    private var dismissHost: FrameLayout? = null
    private var dismissCompose: ComposeView? = null
    private var dismissParams: WindowManager.LayoutParams? = null
    /** True after the user long-press-dragged (or a saved position was loaded). */
    private var userPlaced = false
    private var attached = false
    /** Right edge of the widget in screen px — stays fixed across expand/collapse. */
    private var rightEdgeX = 0f
    private var centerY = 0f
    private var contentW = 0
    private var contentH = 0
    /** Ignore intermediate AnimatedContent size frames that cause layout thrash. */
    private var lastAppliedW = 0
    private var lastAppliedH = 0
    private var sizeSettleJob: Job? = null
    /** True while the user is actively dragging — skip size-settle WM updates. */
    private var dragging = false
    /** Allow free drag even while keyboard-docked (dismiss-to-X flow). */
    private var allowDockDrag = false
    /** Finger offset from the window's top-left when drag began (screen px). */
    private var dragGrabX = 0f
    private var dragGrabY = 0f
    private var lastLayoutX = Int.MIN_VALUE
    private var lastLayoutY = Int.MIN_VALUE
    /** When true, park the widget just above the IME instead of free-float. */
    private var keyboardDock = false
    private var lastImeTop = -1
    /** Last valid keyboard top — avoids mid-screen flash when imeTop flickers. */
    private var lastGoodImeTop = -1
    private var dismissArmed = false
    /** Dock held past move-threshold but not yet armed for dismiss. */
    private var pendingDockDismiss = false
    private var fingerX = 0f
    private var fingerY = 0f
    private val _dismissHot = MutableStateFlow(false)
    private val dismissHot = _dismissHot.asStateFlow()
    private val _snoozedUntil = MutableStateFlow(0L)
    private val snoozedUntilFlow = _snoozedUntil.asStateFlow()

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
        snoozeJob?.cancel()
        snoozeJob = null
        hideDismissTarget()
        detach()
    }

    /** Park the bubble at the default right-side spot and save it. */
    fun resetToDefault() {
        placeAtDefault()
        userPlaced = true
        applyLayout()
        persistPosition()
    }

    private fun attach() {
        if (host != null) return
        registry.currentState = Lifecycle.State.STARTED
        registry.currentState = Lifecycle.State.RESUMED
        val wm = app.getSystemService(Context.WINDOW_SERVICE) as WindowManager
        windowManager = wm
        val dm = app.resources.displayMetrics
        val bubble = (56 * dm.density).toInt()
        contentW = bubble
        contentH = bubble
        // Fresh safe default every attach — right side, above keyboard zone.
        placeAtDefault()
        userPlaced = false

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
            PixelFormat.TRANSLUCENT,
        )
        params.gravity = Gravity.TOP or Gravity.START
        clampAndApply(params)
        if (Build.VERSION.SDK_INT >= 28) {
            params.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        layoutParams = params

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

        scope.launch {
            val snap = runCatching { app.settings.snapshot() }.getOrNull() ?: return@launch
            _snoozedUntil.value = snap.overlaySnoozeUntil
            scheduleSnoozeWake()
            val x = snap.overlayCenterX
            val y = snap.overlayCenterY
            // Ignore corrupt / edge-stuck saves (e.g. half off the bottom wall).
            val sane = x in 0.08f..0.92f && y in 0.12f..0.78f
            if (sane) {
                // Saved value is content-center X; convert to right-edge anchor.
                rightEdgeX = x * dm.widthPixels + contentW / 2f
                centerY = y * dm.heightPixels
                userPlaced = true
                applyLayout()
                Log.i(TAG, "restored pos frac=($x,$y)")
            } else if (x >= 0f || y >= 0f) {
                Log.w(TAG, "discarding bad saved pos frac=($x,$y)")
                runCatching { app.settings.setOverlayCenter(0.82f, 0.48f) }
            }
        }

        compose.setContent {
            val settings by MaxSpeechApp.instance.settings.flow.collectAsState(initial = AppSettings())
            val ui by MaxSpeechApp.instance.dictation.ui.collectAsState()
            val mainUi by MaxSpeechApp.instance.mainUiResumed.collectAsState()
            val focus by TextInjector.inputFocus.collectAsState()
            val user by MaxSpeechApp.instance.auth.user.collectAsState(initial = null)
            val signedIn = user != null && user?.local != true
            val ownPkg = app.packageName
            val focusPkg = focus.packageName
            val overOwnApp = mainUi ||
                focusPkg == ownPkg ||
                focusPkg == "$ownPkg.debug"
            val sessionActive = when (ui.phase) {
                DictationPhase.Listening,
                DictationPhase.Processing,
                DictationPhase.Confirm,
                DictationPhase.Error,
                -> true
                else -> false
            }
            val snoozeUntil by snoozedUntilFlow.collectAsState()
            val now = System.currentTimeMillis()
            val snoozed = now < snoozeUntil || now < settings.overlaySnoozeUntil
            val imeUp = focus.imeTop > 80 || lastGoodImeTop > 80
            // Cloud login only; show over a focused editable (or mid-session).
            // When the keyboard is up, keep showing so the selected style stays visible.
            val showBubble = signedIn &&
                !overOwnApp &&
                !focus.password &&
                !snoozed &&
                (sessionActive || focus.typing || (imeUp && focus.editable))
            if (showBubble) {
                Log.d(TAG, "show bubble phase=${ui.phase} editable=${focus.editable} ime=${focus.imeTop} pkg=$focusPkg")
            }
            MaxSpeechTheme(
                theme = settings.theme,
                glassAlpha = settings.glassAlpha,
                blurStrength = settings.blurStrength,
            ) {
                // Respect Mic Look style — Floating vs Keyboard bar.
                val preferDock = settings.overlayStyle == OverlayStyle.KeyboardBar
                // Keep dock UI while dragging that style to the dismiss X.
                val dock = preferDock || (allowDockDrag && keyboardDock)
                // Wait for a real IME top before showing the dock — prevents bottom→top jump.
                val dockReady = !preferDock || focus.imeTop > 80 || (sessionActive && lastGoodImeTop > 80) || allowDockDrag
                LaunchedEffect(preferDock, showBubble, focus.imeTop, contentH, allowDockDrag) {
                    if (!showBubble) return@LaunchedEffect
                    if (allowDockDrag) return@LaunchedEffect
                    applyOverlayStyle(preferDock, focus.imeTop)
                }
                LaunchedEffect(settings.overlaySnoozeUntil) {
                    if (settings.overlaySnoozeUntil != _snoozedUntil.value) {
                        _snoozedUntil.value = settings.overlaySnoozeUntil
                        scheduleSnoozeWake()
                    }
                }
                BlurFade(visible = showBubble && dockReady) {
                    if (dock) {
                        KeyboardDockBar(
                            ui = ui,
                            sizeScale = settings.overlaySize,
                            surfaceAlpha = settings.overlayAlpha,
                            micColor = Color(settings.overlayMicColor.toInt()),
                            waveScale = settings.overlayWaveScale,
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
                            onRetry = { MaxSpeechApp.instance.dictation.retry() },
                            onDragStart = { rawX, rawY -> beginDrag(rawX, rawY, fromDock = true) },
                            onDragTo = { rawX, rawY -> dragTo(rawX, rawY) },
                            onDragEnd = { endDrag() },
                            onDismissArmed = { armed -> setDismissArmed(armed) },
                            onContentSize = { w, h -> onContentMeasured(w, h) },
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 4.dp, vertical = 2.dp),
                        )
                    } else {
                        OverlayCapsule(
                            ui = ui,
                            sizeScale = settings.overlaySize,
                            surfaceAlpha = settings.overlayAlpha,
                            micColor = Color(settings.overlayMicColor.toInt()),
                            waveScale = settings.overlayWaveScale,
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
                            onRetry = { MaxSpeechApp.instance.dictation.retry() },
                            onDragStart = { rawX, rawY -> beginDrag(rawX, rawY, fromDock = false) },
                            onDragTo = { rawX, rawY -> dragTo(rawX, rawY) },
                            onDragEnd = { endDrag() },
                            onDismissArmed = { armed -> setDismissArmed(armed) },
                            onContentSize = { w, h -> onContentMeasured(w, h) },
                            modifier = Modifier.padding(4.dp),
                        )
                    }
                }
            }
        }
        // Position is restored from prefs / defaults only — never jumps to the keyboard.
        Log.i(TAG, "attach: window added")
    }

    private fun placeAtDefault() {
        val dm = app.resources.displayMetrics
        val bubble = (56 * dm.density).toInt().coerceAtLeast(48)
        contentW = bubble
        contentH = bubble
        // Right side, mid-upper — always fully on-screen and easy to hit.
        rightEdgeX = dm.widthPixels - (20 * dm.density)
        centerY = dm.heightPixels * 0.48f
    }

    private fun onContentMeasured(w: Int, h: Int) {
        if (w <= 0 || h <= 0) return
        if (w == contentW && h == contentH) return
        contentW = w
        contentH = h
        // Never fight the finger mid-drag.
        if (dragging) return
        // Debounce — AnimatedContent emits many intermediate sizes; only settle once.
        sizeSettleJob?.cancel()
        sizeSettleJob = scope.launch {
            kotlinx.coroutines.delay(90)
            if (dragging) return@launch
            if (contentW == lastAppliedW && contentH == lastAppliedH) return@launch
            lastAppliedW = contentW
            lastAppliedH = contentH
            if (keyboardDock && !allowDockDrag) {
                placeAboveIme(lastImeTop)
            } else {
                applyLayout()
            }
        }
    }

    private fun applyOverlayStyle(dock: Boolean, imeTop: Int) {
        keyboardDock = dock
        lastImeTop = imeTop
        if (dock) {
            placeAboveIme(imeTop)
        } else {
            lastGoodImeTop = -1
            val params = layoutParams ?: return
            params.width = WindowManager.LayoutParams.WRAP_CONTENT
            params.height = WindowManager.LayoutParams.WRAP_CONTENT
            applyLayout()
        }
    }

    /** Park a full-width strip just above the software keyboard. */
    private fun placeAboveIme(imeTop: Int) {
        val params = layoutParams ?: return
        val dm = app.resources.displayMetrics
        val padH = (12 * dm.density).toInt().coerceAtLeast(8)
        val gap = (8 * dm.density).toInt()
        val w = (dm.widthPixels - padH * 2).coerceAtLeast(1)
        val h = contentH.coerceAtLeast((48 * dm.density).toInt())
        contentW = w
        val minIme = h + gap + (24 * dm.density).toInt()
        val effectiveIme = when {
            imeTop > minIme -> {
                lastGoodImeTop = imeTop
                imeTop
            }
            lastGoodImeTop > minIme -> lastGoodImeTop
            else -> {
                // No reliable keyboard yet — skip so we don't flash mid-screen.
                return
            }
        }
        val top = (effectiveIme - h - gap)
            .coerceIn(padH, (dm.heightPixels - h - padH).coerceAtLeast(padH))
        if (params.width == w && lastLayoutX == padH && kotlin.math.abs(lastLayoutY - top) <= 1) {
            return
        }
        params.width = w
        params.height = WindowManager.LayoutParams.WRAP_CONTENT
        params.x = padH
        params.y = top
        lastLayoutX = padH
        lastLayoutY = top
        lastImeTop = effectiveIme
        rightEdgeX = (padH + w).toFloat()
        centerY = top + h / 2f
        runCatching { windowManager?.updateViewLayout(host, params) }
    }

    private fun setDismissArmed(armed: Boolean) {
        dismissArmed = armed
        if (armed) {
            allowDockDrag = true
            // Shrink dock strip to a free-float window so it can travel to the X.
            if (keyboardDock) {
                val params = layoutParams
                if (params != null) {
                    params.width = WindowManager.LayoutParams.WRAP_CONTENT
                    params.height = WindowManager.LayoutParams.WRAP_CONTENT
                    runCatching { windowManager?.updateViewLayout(host, params) }
                }
            }
            showDismissTarget()
            // Dock: drag only starts after the 3s arm — kick it off now.
            if (pendingDockDismiss && !dragging) {
                beginDragInternal(fingerX, fingerY, fromDock = true)
            }
        } else if (!dragging) {
            hideDismissTarget()
            allowDockDrag = false
            pendingDockDismiss = false
            _dismissHot.value = false
        }
    }

    /**
     * Screen-space drag. Local Compose deltas jitter because moving the overlay
     * window shifts the pointer's local coordinates every frame.
     */
    private fun beginDrag(rawX: Float, rawY: Float, fromDock: Boolean) {
        fingerX = rawX
        fingerY = rawY
        if (fromDock && !dismissArmed) {
            // Wait for the 3s arm before the dock can leave the IME.
            pendingDockDismiss = true
            return
        }
        beginDragInternal(rawX, rawY, fromDock)
    }

    private fun beginDragInternal(rawX: Float, rawY: Float, fromDock: Boolean) {
        val params = layoutParams ?: return
        sizeSettleJob?.cancel()
        dragging = true
        pendingDockDismiss = false
        if (fromDock) allowDockDrag = true
        dragGrabX = rawX - params.x
        dragGrabY = rawY - params.y
        fingerX = rawX
        fingerY = rawY
        userPlaced = true
    }

    private fun dragTo(rawX: Float, rawY: Float) {
        fingerX = rawX
        fingerY = rawY
        if (!dragging) {
            if (pendingDockDismiss && dismissArmed) {
                beginDragInternal(rawX, rawY, fromDock = true)
            } else {
                return
            }
        }
        if (keyboardDock && !allowDockDrag) return
        val params = layoutParams ?: return
        val dm = app.resources.displayMetrics
        val pad = (10 * dm.density).toInt().coerceAtLeast(8)
        val w = contentW.coerceAtLeast(1)
        val h = contentH.coerceAtLeast(1)
        var left = kotlin.math.round(rawX - dragGrabX).toInt()
        var top = kotlin.math.round(rawY - dragGrabY).toInt()
        // While armed, allow traveling into the bottom dismiss zone.
        val maxTop = if (dismissArmed) {
            (dm.heightPixels - h / 2).coerceAtLeast(pad)
        } else {
            (dm.heightPixels - h - pad).coerceAtLeast(pad)
        }
        left = left.coerceIn(pad, (dm.widthPixels - w - pad).coerceAtLeast(pad))
        top = top.coerceIn(pad, maxTop)
        _dismissHot.value = dismissArmed && overDismissTarget(rawX, rawY)
        if (left == lastLayoutX && top == lastLayoutY) return
        params.x = left
        params.y = top
        lastLayoutX = left
        lastLayoutY = top
        rightEdgeX = left + w.toFloat()
        centerY = top + h / 2f
        runCatching { windowManager?.updateViewLayout(host, params) }
    }

    private fun endDrag() {
        val wasDragging = dragging || pendingDockDismiss
        if (!wasDragging) return
        val shouldSnooze = dismissArmed && overDismissTarget(fingerX, fingerY)
        dragging = false
        pendingDockDismiss = false
        hideDismissTarget()
        dismissArmed = false
        allowDockDrag = false
        _dismissHot.value = false
        if (shouldSnooze) {
            snoozeForMinutes(10)
            return
        }
        if (keyboardDock) {
            placeAboveIme(lastImeTop)
        } else {
            persistPosition()
        }
    }

    private fun snoozeForMinutes(minutes: Int) {
        val until = System.currentTimeMillis() + minutes * 60_000L
        _snoozedUntil.value = until
        Log.i(TAG, "overlay snoozed until $until (${minutes}m)")
        scope.launch {
            runCatching { app.settings.setOverlaySnoozeUntil(until) }
        }
        scheduleSnoozeWake()
    }

    private fun scheduleSnoozeWake() {
        snoozeJob?.cancel()
        val until = _snoozedUntil.value
        if (until <= System.currentTimeMillis()) return
        snoozeJob = scope.launch {
            val wait = (until - System.currentTimeMillis()).coerceAtLeast(0L)
            delay(wait)
            _snoozedUntil.value = 0L
            runCatching { app.settings.setOverlaySnoozeUntil(0L) }
            Log.i(TAG, "overlay snooze expired — showing again")
        }
    }

    private fun dismissTargetCenter(): Pair<Float, Float> {
        val dm = app.resources.displayMetrics
        return dm.widthPixels / 2f to (dm.heightPixels - 72 * dm.density)
    }

    private fun overDismissTarget(x: Float, y: Float): Boolean {
        val (cx, cy) = dismissTargetCenter()
        val r = 48 * app.resources.displayMetrics.density
        val dx = x - cx
        val dy = y - cy
        return dx * dx + dy * dy <= r * r
    }

    private fun showDismissTarget() {
        if (dismissHost != null) return
        val wm = windowManager ?: return
        val dm = app.resources.displayMetrics
        val size = (72 * dm.density).toInt()
        val (cx, cy) = dismissTargetCenter()
        val params = WindowManager.LayoutParams(
            size,
            size,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
            PixelFormat.TRANSLUCENT,
        )
        params.gravity = Gravity.TOP or Gravity.START
        params.x = (cx - size / 2f).toInt()
        params.y = (cy - size / 2f).toInt()
        if (Build.VERSION.SDK_INT >= 28) {
            params.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }
        dismissParams = params
        val frame = FrameLayout(app).apply {
            setViewTreeLifecycleOwner(this@FloatingMicController)
            setViewTreeSavedStateRegistryOwner(this@FloatingMicController)
            setViewTreeViewModelStoreOwner(this@FloatingMicController)
        }
        val compose = ComposeView(app).apply {
            setViewTreeLifecycleOwner(this@FloatingMicController)
            setViewTreeSavedStateRegistryOwner(this@FloatingMicController)
            setViewTreeViewModelStoreOwner(this@FloatingMicController)
            setContent {
                val hot by dismissHot.collectAsState()
                Box(
                    Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) {
                    Box(
                        Modifier
                            .size(if (hot) 68.dp else 56.dp)
                            .clip(CircleShape)
                            .background(
                                if (hot) Color(0xFFE11D48) else Color(0xCC1C1C1E),
                                CircleShape,
                            )
                            .border(
                                1.5.dp,
                                if (hot) Color.White.copy(alpha = 0.55f) else Color.White.copy(alpha = 0.22f),
                                CircleShape,
                            ),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            Icons.Filled.Close,
                            contentDescription = "Hide for 10 minutes",
                            tint = Color.White,
                            modifier = Modifier.size(if (hot) 30.dp else 26.dp),
                        )
                    }
                }
            }
        }
        frame.addView(compose)
        dismissCompose = compose
        runCatching { wm.addView(frame, params) }
            .onSuccess { dismissHost = frame }
            .onFailure { Log.w(TAG, "dismiss target failed", it) }
    }

    private fun hideDismissTarget() {
        dismissCompose?.let { runCatching { it.disposeComposition() } }
        dismissCompose = null
        dismissHost?.let { runCatching { windowManager?.removeViewImmediate(it) } }
        dismissHost = null
        dismissParams = null
    }

    private fun persistPosition() {
        userPlaced = true
        if (!dragging) applyLayout()
        val dm = app.resources.displayMetrics
        val centerX = rightEdgeX - contentW / 2f
        val xFrac = (centerX / dm.widthPixels.toFloat()).coerceIn(0f, 1f)
        val yFrac = (centerY / dm.heightPixels.toFloat()).coerceIn(0f, 1f)
        scope.launch {
            runCatching { app.settings.setOverlayCenter(xFrac, yFrac) }
                .onFailure { Log.w(TAG, "persistPosition failed", it) }
        }
    }

    /** Keep the entire widget inside the screen; right edge is the stable park point. */
    private fun clampAndApply(params: WindowManager.LayoutParams) {
        val dm = app.resources.displayMetrics
        val pad = (10 * dm.density).toInt().coerceAtLeast(8)
        val w = contentW.coerceAtLeast(1)
        val h = contentH.coerceAtLeast(1)
        val maxRight = (dm.widthPixels - pad).toFloat()
        val minRight = (pad + w).toFloat()
        rightEdgeX = rightEdgeX.coerceIn(minRight, maxRight)
        var left = kotlin.math.round(rightEdgeX - w).toInt()
        var top = kotlin.math.round(centerY - h / 2f).toInt()
        val maxY = (dm.heightPixels - h - pad).coerceAtLeast(pad)
        top = top.coerceIn(pad, maxY)
        left = left.coerceIn(pad, (dm.widthPixels - w - pad).coerceAtLeast(pad))
        params.x = left
        params.y = top
        lastLayoutX = left
        lastLayoutY = top
        // Re-sync anchors from the clamped rect (no center rewrite fight).
        rightEdgeX = left + w.toFloat()
        centerY = top + h / 2f
    }

    private fun applyLayout() {
        if (dragging) return
        if (keyboardDock && !allowDockDrag) {
            placeAboveIme(lastImeTop)
            return
        }
        val params = layoutParams ?: return
        clampAndApply(params)
        runCatching { windowManager?.updateViewLayout(host, params) }
    }

    private fun detach() {
        sizeSettleJob?.cancel()
        sizeSettleJob = null
        placeJob?.cancel()
        placeJob = null
        hideDismissTarget()
        runCatching { composeView?.disposeComposition() }
        composeView = null
        host?.let { runCatching { windowManager?.removeViewImmediate(it) } }
        host = null
        layoutParams = null
        attached = false
        contentW = 0
        contentH = 0
        lastAppliedW = 0
        lastAppliedH = 0
        if (registry.currentState.isAtLeast(Lifecycle.State.CREATED)) {
            registry.currentState = Lifecycle.State.CREATED
        }
    }

    companion object {
        private const val TAG = "FloatingMic"
    }
}
