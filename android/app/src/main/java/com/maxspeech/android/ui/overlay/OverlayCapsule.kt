package com.maxspeech.android.ui.overlay

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.pipeline.DictationUi
import com.maxspeech.android.ui.components.WindowsWaveform
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise
import kotlin.math.hypot

/** Idle mic disc — 20% smaller than the original 64dp. */
private val BaseMic = 51.dp
/** Compact listening controls. */
private val SideBtn = 36.dp
private val WaveBarCount = 5
private val WaveBarW = 3.dp
private val WaveBarGap = 3.dp
private val WavePadH = 10.dp
private val WavePadV = 5.dp
private val WaveH = 16.dp

@Composable
fun OverlayCapsule(
    ui: DictationUi,
    sizeScale: Float,
    surfaceAlpha: Float,
    onHoldStart: () -> Unit,
    onHoldEnd: () -> Unit,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    onDragBy: (dxPx: Float, dyPx: Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val scale = sizeScale.coerceIn(0.55f, 1.45f)
    val alpha = surfaceAlpha.coerceIn(0.25f, 1f)
    when (ui.phase) {
        DictationPhase.Confirm, DictationPhase.Listening, DictationPhase.Processing -> {
            ListeningControls(
                levels = ui.levels,
                phase = ui.phase,
                sizeScale = scale,
                surfaceAlpha = alpha,
                onCancel = onCancel,
                onProceed = {
                    when (ui.phase) {
                        DictationPhase.Confirm -> onConfirm()
                        else -> onHoldEnd()
                    }
                },
                onDragBy = onDragBy,
                modifier = modifier,
            )
        }
        else -> {
            FloatingMicBubble(
                size = BaseMic * scale,
                surfaceAlpha = alpha,
                onToggle = onHoldStart,
                onDragBy = onDragBy,
                modifier = modifier,
            )
        }
    }
}

/**
 * Compact row: [X] [thin 5-bar pill] [✓]
 * Middle width hugs the bars — not a wide Windows-length strip.
 */
@Composable
private fun ListeningControls(
    levels: List<Float>,
    phase: DictationPhase,
    sizeScale: Float,
    surfaceAlpha: Float,
    onCancel: () -> Unit,
    onProceed: () -> Unit,
    onDragBy: (Float, Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = LocalMsColors.current
    val btn = SideBtn * sizeScale
    val waveH = WaveH * sizeScale
    val pillShape = RoundedCornerShape(percent = 50)
    val pillBg = Brush.linearGradient(
        listOf(Color(0xFF0A0A0A), Color(0xFF080808), Color(0xFF040404)),
    )
    val checkBg = if (phase == DictationPhase.Confirm) Orange else Turquoise

    Row(
        modifier = modifier
            .wrapContentWidth()
            .alpha(surfaceAlpha)
            .draggableOverlay(onDragBy = onDragBy),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp * sizeScale),
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
                .wrapContentWidth()
                .height(btn * 0.92f)
                .clip(pillShape)
                .background(pillBg, pillShape)
                .border(1.dp, Color.Black.copy(alpha = 0.9f), pillShape)
                .padding(horizontal = WavePadH * sizeScale, vertical = WavePadV * sizeScale),
            contentAlignment = Alignment.Center,
        ) {
            WindowsWaveform(
                levels = levels,
                barCount = WaveBarCount,
                maxBarHeight = waveH,
                barWidth = WaveBarW * sizeScale,
                barGap = WaveBarGap * sizeScale,
                modifier = Modifier.height(waveH),
            )
        }

        Box(
            Modifier
                .size(btn)
                .clip(CircleShape)
                .background(checkBg, CircleShape)
                .clickable(onClick = onProceed)
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
    onDragBy: (dxPx: Float, dyPx: Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    var pressing by remember { mutableStateOf(false) }
    val pressAlpha by animateFloatAsState(
        targetValue = if (pressing) (surfaceAlpha * 0.88f).coerceAtLeast(0.2f) else surfaceAlpha,
        animationSpec = tween(90),
        label = "micPressAlpha",
    )
    val pressScale by animateFloatAsState(
        targetValue = if (pressing) 0.94f else 1f,
        animationSpec = spring(dampingRatio = 0.7f, stiffness = 500f),
        label = "micPressScale",
    )
    val ring by animateColorAsState(
        targetValue = if (pressing) Turquoise.copy(alpha = 0.55f) else Color.White.copy(alpha = 0.10f),
        animationSpec = tween(120),
        label = "micRing",
    )
    Box(
        modifier = modifier
            .size(size)
            .scale(pressScale)
            .alpha(pressAlpha)
            .clip(CircleShape)
            .background(
                Brush.linearGradient(listOf(Color(0xFF121212), Color(0xFF0A0A0A), Color(0xFF050505))),
                CircleShape,
            )
            .border(1.dp, ring, CircleShape)
            .draggableOverlay(
                onDragBy = onDragBy,
                onTap = onToggle,
                onPressing = { pressing = it },
            )
            .semantics { contentDescription = "Start dictation" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            Icons.Filled.Mic,
            contentDescription = null,
            tint = Turquoise,
            modifier = Modifier.size(size * 0.42f),
        )
    }
}

private fun Modifier.draggableOverlay(
    onDragBy: (dxPx: Float, dyPx: Float) -> Unit,
    onTap: (() -> Unit)? = null,
    onLongPress: (() -> Unit)? = null,
    onPressing: ((Boolean) -> Unit)? = null,
): Modifier = pointerInput(onTap, onLongPress) {
    val tapSlop = 5f
    val longMs = 480L
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        onPressing?.invoke(true)
        val startTime = System.currentTimeMillis()
        var dragDist = 0f
        var dragging = false
        try {
            drag(down.id) { change ->
                val dx = change.position.x - change.previousPosition.x
                val dy = change.position.y - change.previousPosition.y
                dragDist += hypot(dx, dy)
                if (dragDist > tapSlop) {
                    dragging = true
                    onDragBy(dx, dy)
                }
                change.consume()
            }
        } finally {
            onPressing?.invoke(false)
        }
        val held = System.currentTimeMillis() - startTime
        when {
            dragging -> Unit
            held >= longMs && onLongPress != null -> onLongPress()
            else -> onTap?.invoke()
        }
    }
}
