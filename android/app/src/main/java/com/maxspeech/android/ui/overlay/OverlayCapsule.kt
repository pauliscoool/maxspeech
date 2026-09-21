package com.maxspeech.android.ui.overlay

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
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
import com.maxspeech.android.ui.theme.Turquoise
import kotlin.math.hypot

/** Windows pill scaled ~20% down; bars halved (10 vs desktop 20). */
private val BasePillW = 118.dp
private val BasePillH = 29.dp
private val BaseMic = 51.dp

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
            WindowsListeningPill(
                levels = ui.levels,
                phase = ui.phase,
                sizeScale = scale,
                surfaceAlpha = alpha,
                onTap = {
                    when (ui.phase) {
                        DictationPhase.Confirm -> onConfirm()
                        else -> onHoldEnd()
                    }
                },
                onLongCancel = onCancel,
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

@Composable
private fun WindowsListeningPill(
    levels: List<Float>,
    phase: DictationPhase,
    sizeScale: Float,
    surfaceAlpha: Float,
    onTap: () -> Unit,
    onLongCancel: () -> Unit,
    onDragBy: (Float, Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    var pressing by remember { mutableStateOf(false) }
    val pressAlpha by animateFloatAsState(
        targetValue = if (pressing) (surfaceAlpha * 0.88f).coerceAtLeast(0.2f) else surfaceAlpha,
        animationSpec = tween(90),
        label = "pillPressAlpha",
    )
    val pressScale by animateFloatAsState(
        targetValue = if (pressing) 0.97f else 1f,
        animationSpec = spring(dampingRatio = 0.75f, stiffness = 480f),
        label = "pillPressScale",
    )
    val shape = RoundedCornerShape(percent = 50)
    val pillBg = Brush.linearGradient(
        listOf(Color(0xFF0A0A0A), Color(0xFF080808), Color(0xFF040404)),
    )
    Box(
        modifier = modifier
            .scale(pressScale)
            .width(BasePillW * sizeScale)
            .height(BasePillH * sizeScale)
            .alpha(pressAlpha)
            .clip(shape)
            .background(pillBg, shape)
            .border(1.5.dp, Color.Black.copy(alpha = 0.95f * pressAlpha), shape)
            .draggableOverlay(
                onDragBy = onDragBy,
                onTap = onTap,
                onLongPress = onLongCancel,
                onPressing = { pressing = it },
            )
            .semantics {
                contentDescription = when (phase) {
                    DictationPhase.Confirm -> "Confirm dictation"
                    else -> "Stop dictation"
                }
            }
            .padding(horizontal = 10.dp * sizeScale, vertical = 4.dp * sizeScale),
        contentAlignment = Alignment.Center,
    ) {
        WindowsWaveform(
            levels = levels,
            barCount = 10,
            maxBarHeight = 14.dp * sizeScale,
            modifier = Modifier.height(14.dp * sizeScale),
        )
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
