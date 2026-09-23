package com.maxspeech.android.ui.overlay

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.pipeline.DictationUi
import com.maxspeech.android.ui.components.DesktopBarCount
import com.maxspeech.android.ui.components.DesktopBarGap
import com.maxspeech.android.ui.components.DesktopBarMaxH
import com.maxspeech.android.ui.components.DesktopBarWidth
import com.maxspeech.android.ui.components.WindowsWaveform
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise
import kotlinx.coroutines.withTimeoutOrNull

/** Idle mic disc — ~5% larger than prior 51dp. */
private val BaseMic = 54.dp
/** Side cancel / confirm buttons. */
private val SideBtn = 38.dp
private val WavePadH = 12.dp
private val WavePadV = 5.dp
private val ControlsGap = 8.dp
/** Extra ~5% on top of the user size setting. */
private const val WidgetBump = 1.05f

@Composable
fun OverlayCapsule(
    ui: DictationUi,
    sizeScale: Float,
    surfaceAlpha: Float,
    micColor: Color = Turquoise,
    waveScale: Float = 1f,
    onHoldStart: () -> Unit,
    onHoldEnd: () -> Unit,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    onRetry: () -> Unit = {},
    onDragStart: (rawX: Float, rawY: Float) -> Unit = { _, _ -> },
    onDragTo: (rawX: Float, rawY: Float) -> Unit = { _, _ -> },
    onDragEnd: () -> Unit = {},
    onDismissArmed: (Boolean) -> Unit = {},
    onContentSize: (widthPx: Int, heightPx: Int) -> Unit = { _, _ -> },
    modifier: Modifier = Modifier,
) {
    val scale = (sizeScale * WidgetBump).coerceIn(0.55f, 1.55f)
    val alpha = surfaceAlpha.coerceIn(0.25f, 1f)
    // Error keeps the mic visible — Retry is a side chip, never replaces dictation UI.
    val expanded = ui.phase == DictationPhase.Confirm ||
        ui.phase == DictationPhase.Listening ||
        ui.phase == DictationPhase.Processing
    val showRetryChip = ui.phase == DictationPhase.Error
    var lastSize by remember { mutableStateOf(IntSize.Zero) }

    Box(
        modifier = modifier.onSizeChanged { size ->
            if (size != lastSize && size.width > 0 && size.height > 0) {
                lastSize = size
                onContentSize(size.width, size.height)
            }
        },
        contentAlignment = Alignment.CenterEnd,
    ) {
        val fromRight = TransformOrigin(1f, 0.5f)
        AnimatedContent(
            targetState = expanded,
            transitionSpec = {
                // Soft appear / disappear between idle mic and listening controls.
                val enter = fadeIn(tween(320, easing = FastOutSlowInEasing)) +
                    scaleIn(
                        animationSpec = tween(340, easing = FastOutSlowInEasing),
                        initialScale = 0.92f,
                        transformOrigin = fromRight,
                    )
                val exit = fadeOut(tween(280, easing = FastOutSlowInEasing)) +
                    scaleOut(
                        animationSpec = tween(300, easing = FastOutSlowInEasing),
                        targetScale = 0.92f,
                        transformOrigin = fromRight,
                    )
                enter togetherWith exit
            },
            label = "overlayExpand",
        ) { showControls ->
            if (showControls) {
                ListeningControls(
                    levels = ui.levels,
                    phase = ui.phase,
                    sizeScale = scale,
                    surfaceAlpha = alpha,
                    waveScale = waveScale,
                    onCancel = onCancel,
                    onProceed = {
                        when (ui.phase) {
                            DictationPhase.Confirm -> onConfirm()
                            DictationPhase.Listening -> onHoldEnd()
                            else -> Unit // Processing: ignore — animation already running
                        }
                    },
                    onDragStart = onDragStart,
                    onDragTo = onDragTo,
                    onDragEnd = onDragEnd,
                    onDismissArmed = onDismissArmed,
                )
            } else {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp * scale),
                ) {
                    if (showRetryChip) {
                        Box(
                            Modifier
                                .height(BaseMic * scale * 0.72f)
                                .clip(RoundedCornerShape(percent = 50))
                                .background(Orange, RoundedCornerShape(percent = 50))
                                .clickable(onClick = onRetry)
                                .padding(horizontal = 14.dp * scale)
                                .semantics { contentDescription = "Retry dictation" },
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                text = "Retry",
                                color = Color.White,
                                fontSize = (13 * scale).sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                    FloatingMicBubble(
                        size = BaseMic * scale,
                        surfaceAlpha = alpha,
                        micColor = micColor,
                        // Mic always starts a fresh listen — also clears a prior error.
                        onToggle = {
                            if (showRetryChip) onRetry() else onHoldStart()
                        },
                        onDragStart = onDragStart,
                        onDragTo = onDragTo,
                        onDragEnd = onDragEnd,
                        onDismissArmed = onDismissArmed,
                        errorRing = showRetryChip,
                    )
                }
            }
        }
    }
}

/**
 * Row: [X] [bars] [✓] — buttons are tap-only; hold the middle pill to move.
 */
@Composable
private fun ListeningControls(
    levels: List<Float>,
    phase: DictationPhase,
    sizeScale: Float,
    surfaceAlpha: Float,
    waveScale: Float,
    onCancel: () -> Unit,
    onProceed: () -> Unit,
    onDragStart: (Float, Float) -> Unit,
    onDragTo: (Float, Float) -> Unit,
    onDragEnd: () -> Unit,
    onDismissArmed: (Boolean) -> Unit,
) {
    val c = LocalMsColors.current
    val btn = SideBtn * sizeScale
    val barW = DesktopBarWidth * sizeScale
    val barGap = DesktopBarGap * sizeScale
    val waveH = DesktopBarMaxH * sizeScale * waveScale.coerceIn(0.55f, 1.55f)
    val waveW = barW * DesktopBarCount + barGap * (DesktopBarCount - 1)
    val pillShape = RoundedCornerShape(percent = 50)
    val pillBg = Brush.linearGradient(
        listOf(Color(0xFF0A0A0A), Color(0xFF080808), Color(0xFF040404)),
    )
    val checkBg = when (phase) {
        DictationPhase.Confirm -> Orange
        DictationPhase.Processing -> Turquoise.copy(alpha = 0.45f)
        else -> Turquoise
    }
    var moving by remember { mutableStateOf(false) }

    Row(
        modifier = Modifier.alpha(surfaceAlpha),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ControlsGap * sizeScale),
    ) {
        Box(
            Modifier
                .size(btn)
                .clip(CircleShape)
                .background(c.glassFill.copy(alpha = 0.35f.coerceAtMost(surfaceAlpha)), CircleShape)
                .border(1.dp, Color.White.copy(alpha = 0.12f), CircleShape)
                .clickable(onClick = onCancel)
                .semantics { contentDescription = "Cancel dictation" },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.Close,
                contentDescription = null,
                tint = Color.White.copy(alpha = 0.92f),
                modifier = Modifier.size(btn * 0.45f),
            )
        }

        Box(
            Modifier
                .height((btn * 0.92f).coerceAtLeast(waveH + WavePadV * 2 * sizeScale))
                .clip(pillShape)
                .background(pillBg, pillShape)
                .border(
                    1.dp,
                    if (moving) Turquoise.copy(alpha = 0.55f) else Color.Black.copy(alpha = 0.9f),
                    pillShape,
                )
                .padding(horizontal = WavePadH * sizeScale, vertical = WavePadV * sizeScale)
                .holdToMove(
                    onDragStart = onDragStart,
                    onDragTo = onDragTo,
                    onDragEnd = onDragEnd,
                    onDismissArmed = onDismissArmed,
                    onMoving = { moving = it },
                ),
            contentAlignment = Alignment.Center,
        ) {
            WindowsWaveform(
                levels = levels,
                barCount = DesktopBarCount,
                maxBarHeight = waveH,
                barWidth = barW,
                barGap = barGap,
                modifier = Modifier
                    .height(waveH)
                    .width(waveW),
            )
        }

        Box(
            Modifier
                .size(btn)
                .clip(CircleShape)
                .background(checkBg, CircleShape)
                .then(
                    if (phase == DictationPhase.Listening || phase == DictationPhase.Confirm) {
                        Modifier.clickable(onClick = onProceed)
                    } else {
                        Modifier
                    },
                )
                .semantics {
                    contentDescription = if (phase == DictationPhase.Confirm) {
                        "Confirm dictation"
                    } else {
                        "Finish dictation"
                    }
                },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.Check,
                contentDescription = null,
                tint = Color.White,
                modifier = Modifier.size(btn * 0.45f),
            )
        }
    }
}

@Composable
fun FloatingMicBubble(
    size: Dp,
    surfaceAlpha: Float,
    onToggle: () -> Unit,
    onDragStart: (Float, Float) -> Unit,
    onDragTo: (Float, Float) -> Unit,
    onDragEnd: () -> Unit = {},
    onDismissArmed: (Boolean) -> Unit = {},
    errorRing: Boolean = false,
    micColor: Color = Turquoise,
    modifier: Modifier = Modifier,
) {
    var pressing by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    val pressAlpha by animateFloatAsState(
        targetValue = when {
            moving -> (surfaceAlpha * 0.95f).coerceAtLeast(0.3f)
            pressing -> (surfaceAlpha * 0.88f).coerceAtLeast(0.2f)
            else -> surfaceAlpha
        },
        animationSpec = tween(90),
        label = "micPressAlpha",
    )
    val ring by animateColorAsState(
        targetValue = when {
            moving -> Turquoise.copy(alpha = 0.7f)
            errorRing -> Orange.copy(alpha = 0.85f)
            pressing -> Turquoise.copy(alpha = 0.55f)
            else -> Color.White.copy(alpha = 0.10f)
        },
        animationSpec = tween(120),
        label = "micRing",
    )
    Box(
        modifier = modifier
            .size(size)
            // graphicsLayer scale — does not change layout size (avoids WM thrash).
            .graphicsLayer {
                val s = when {
                    moving -> 1.04f
                    pressing -> 0.96f
                    else -> 1f
                }
                scaleX = s
                scaleY = s
                alpha = pressAlpha
            }
            .clip(CircleShape)
            .background(
                Brush.linearGradient(listOf(Color(0xFF121212), Color(0xFF0A0A0A), Color(0xFF050505))),
                CircleShape,
            )
            .border(1.dp, ring, CircleShape)
            .holdToMove(
                onDragStart = onDragStart,
                onDragTo = onDragTo,
                onDragEnd = onDragEnd,
                onDismissArmed = onDismissArmed,
                onTap = onToggle,
                onPressing = { pressing = it },
                onMoving = { moving = it },
            )
            .semantics { contentDescription = "Start dictation" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            Icons.Filled.Mic,
            contentDescription = null,
            tint = micColor,
            modifier = Modifier.size(size * 0.42f),
        )
    }
}

/**
 * Tap = [onTap]. Hold ~[MoveHoldMs] then drag using **screen** coordinates
 * so moving the overlay window cannot fight local pointer deltas.
 * After [DismissArmMs] of continuous press, [onDismissArmed] fires so a
 * bottom X target can appear for temporary hide.
 */
@Composable
fun Modifier.holdToMove(
    onDragStart: (rawX: Float, rawY: Float) -> Unit,
    onDragTo: (rawX: Float, rawY: Float) -> Unit,
    onDragEnd: () -> Unit = {},
    onDismissArmed: (Boolean) -> Unit = {},
    onTap: (() -> Unit)? = null,
    onPressing: ((Boolean) -> Unit)? = null,
    onMoving: ((Boolean) -> Unit)? = null,
): Modifier {
    val tapState = rememberUpdatedState(onTap)
    val dragStartState = rememberUpdatedState(onDragStart)
    val dragToState = rememberUpdatedState(onDragTo)
    val dragEndState = rememberUpdatedState(onDragEnd)
    val dismissArmState = rememberUpdatedState(onDismissArmed)
    val pressingState = rememberUpdatedState(onPressing)
    val movingState = rememberUpdatedState(onMoving)
    var coords by remember { mutableStateOf<LayoutCoordinates?>(null) }

    return this
        .onGloballyPositioned { coords = it }
        .pointerInput(Unit) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                down.consume()
                pressingState.value?.invoke(true)
                var enteredMove = false
                var dismissArmed = false
                val downAt = System.nanoTime()
                try {
                    val releasedEarly = withTimeoutOrNull(MoveHoldMs) {
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Main)
                            val change = event.changes.firstOrNull { it.id == down.id }
                                ?: return@withTimeoutOrNull true
                            if (!change.pressed) {
                                change.consume()
                                return@withTimeoutOrNull true
                            }
                            change.consume()
                        }
                        @Suppress("UNREACHABLE_CODE")
                        false
                    }

                    if (releasedEarly == true) {
                        tapState.value?.invoke()
                    } else {
                        enteredMove = true
                        movingState.value?.invoke(true)
                        fun screenOf(change: androidx.compose.ui.input.pointer.PointerInputChange): Offset {
                            val c = coords
                            return if (c != null && c.isAttached) {
                                c.localToScreen(change.position)
                            } else {
                                change.position
                            }
                        }
                        val start = screenOf(
                            currentEvent.changes.firstOrNull { it.id == down.id } ?: down,
                        )
                        dragStartState.value(start.x, start.y)
                        while (true) {
                            val event = awaitPointerEvent(PointerEventPass.Main)
                            val change = event.changes.firstOrNull { it.id == down.id }
                                ?: break
                            if (!change.pressed) {
                                change.consume()
                                break
                            }
                            val heldMs = (System.nanoTime() - downAt) / 1_000_000L
                            if (!dismissArmed && heldMs >= DismissArmMs) {
                                dismissArmed = true
                                dismissArmState.value(true)
                            }
                            val screen = screenOf(change)
                            dragToState.value(screen.x, screen.y)
                            change.consume()
                        }
                        dragEndState.value()
                    }
                } finally {
                    if (dismissArmed) dismissArmState.value(false)
                    if (enteredMove) movingState.value?.invoke(false)
                    pressingState.value?.invoke(false)
                }
            }
        }
}

/** Long-press before drag — short enough to feel responsive, long enough vs tap. */
private const val MoveHoldMs = 450L
/** Hold this long before the bottom dismiss-X target appears. */
const val DismissArmMs = 3_000L
