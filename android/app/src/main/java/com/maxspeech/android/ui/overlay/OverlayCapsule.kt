package com.maxspeech.android.ui.overlay

import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.pipeline.DictationUi
import com.maxspeech.android.ui.components.RibbonWaveform
import com.maxspeech.android.ui.components.liquidGlass
import com.maxspeech.android.ui.theme.LocalBlurStrength
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise
import kotlin.math.hypot

@Composable
fun OverlayCapsule(
    ui: DictationUi,
    glassAlpha: Float,
    onHoldStart: () -> Unit,
    onHoldEnd: () -> Unit,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    onDragBy: (dxPx: Float, dyPx: Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = LocalMsColors.current
    val blur = LocalBlurStrength.current
    val shape = RoundedCornerShape(28.dp)
    when (ui.phase) {
        DictationPhase.Confirm, DictationPhase.Listening, DictationPhase.Processing -> {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = modifier.draggableOverlay(onDragBy),
            ) {
                Box(
                    Modifier
                        .size(44.dp)
                        .liquidGlass(CircleShape, c.glassFill.copy(alpha = glassAlpha.coerceIn(0.15f, 1f)), c.hairline, blur)
                        .clickable(onClick = onCancel),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Filled.Close, contentDescription = "Cancel", tint = c.text)
                }
                Box(
                    Modifier
                        .widthIn(min = 148.dp)
                        .height(44.dp)
                        .liquidGlass(shape, c.glassFill.copy(alpha = glassAlpha.coerceIn(0.15f, 1f)), c.hairline, blur)
                        .padding(horizontal = 16.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    RibbonWaveform(ui.levels)
                }
                Box(
                    Modifier
                        .size(44.dp)
                        .clip(CircleShape)
                        .background(if (ui.phase == DictationPhase.Confirm) Orange else Turquoise)
                        .clickable {
                            if (ui.phase == DictationPhase.Confirm) onConfirm() else onHoldEnd()
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Filled.Check, contentDescription = "Confirm", tint = Color.White)
                }
            }
        }
        else -> {
            FloatingMicBubble(
                listening = false,
                onToggle = onHoldStart,
                onDragBy = onDragBy,
                modifier = modifier,
            )
        }
    }
}

/** Compact draggable mic that pops up over the keyboard. Tap to dictate. */
@Composable
fun FloatingMicBubble(
    listening: Boolean,
    onToggle: () -> Unit,
    onDragBy: (dxPx: Float, dyPx: Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val bg = if (listening) {
        Brush.linearGradient(listOf(Orange, Color(0xFFFB923C)))
    } else {
        Brush.linearGradient(listOf(Color(0xFF5B8CFF), Turquoise, Color(0xFF86EFD0)))
    }
    Box(
        modifier = modifier
            .size(64.dp)
            .clip(CircleShape)
            .background(bg, CircleShape)
            .draggableOverlay(onDragBy, onTap = onToggle)
            .semantics {
                contentDescription = if (listening) "Stop dictation" else "Start dictation"
            },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            if (listening) Icons.Filled.Stop else Icons.Filled.Mic,
            contentDescription = null,
            tint = Color.White,
            modifier = Modifier.size(30.dp),
        )
    }
}

private fun Modifier.draggableOverlay(
    onDragBy: (dxPx: Float, dyPx: Float) -> Unit,
    onTap: (() -> Unit)? = null,
): Modifier = pointerInput(onTap) {
    val tapSlop = 18f
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        var dragDist = 0f
        drag(down.id) { change ->
            val dx = change.position.x - change.previousPosition.x
            val dy = change.position.y - change.previousPosition.y
            dragDist += hypot(dx, dy)
            if (dragDist > tapSlop) {
                onDragBy(dx, dy)
            }
            change.consume()
        }
        if (dragDist <= tapSlop) {
            onTap?.invoke()
        }
    }
}
