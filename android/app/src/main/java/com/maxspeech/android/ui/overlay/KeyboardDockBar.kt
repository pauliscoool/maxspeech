package com.maxspeech.android.ui.overlay

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.pipeline.DictationUi
import com.maxspeech.android.ui.components.KeyboardBarCount
import com.maxspeech.android.ui.components.KeyboardBarMaxH
import com.maxspeech.android.ui.components.WindowsWaveform
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise

/**
 * Keyboard-docked dictation bar — sits above the IME.
 * Idle: compact mic strip. Active: wide X | 12-bar wave | ✓ strip.
 */
@Composable
fun KeyboardDockBar(
    ui: DictationUi,
    sizeScale: Float,
    surfaceAlpha: Float,
    micColor: Color,
    waveScale: Float,
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
    // ~5% tighter than the original dock strip.
    val scale = (sizeScale * 0.95f).coerceIn(0.55f, 1.55f)
    val alpha = surfaceAlpha.coerceIn(0.25f, 1f)
    val expanded = ui.phase == DictationPhase.Confirm ||
        ui.phase == DictationPhase.Listening ||
        ui.phase == DictationPhase.Processing
    val showRetry = ui.phase == DictationPhase.Error
    val shape = RoundedCornerShape(20.dp)

    Box(
        modifier = modifier
            .fillMaxWidth()
            .onSizeChanged { size ->
                if (size.width > 0 && size.height > 0) onContentSize(size.width, size.height)
            }
            .alpha(alpha),
        contentAlignment = Alignment.Center,
    ) {
        AnimatedContent(
            targetState = expanded,
            transitionSpec = {
                fadeIn(tween(300, easing = FastOutSlowInEasing)) togetherWith
                    fadeOut(tween(240, easing = FastOutSlowInEasing))
            },
            label = "keyboardDockExpand",
        ) { active ->
            if (active) {
                ActiveKeyboardStrip(
                    levels = ui.levels,
                    phase = ui.phase,
                    scale = scale,
                    waveScale = waveScale,
                    shape = shape,
                    onCancel = onCancel,
                    onProceed = {
                        when (ui.phase) {
                            DictationPhase.Confirm -> onConfirm()
                            DictationPhase.Listening -> onHoldEnd()
                            else -> Unit
                        }
                    },
                )
            } else {
                IdleKeyboardStrip(
                    scale = scale,
                    micColor = micColor,
                    shape = shape,
                    showRetry = showRetry,
                    onMic = { if (showRetry) onRetry() else onHoldStart() },
                    onRetry = onRetry,
                    onDragStart = onDragStart,
                    onDragTo = onDragTo,
                    onDragEnd = onDragEnd,
                    onDismissArmed = onDismissArmed,
                )
            }
        }
    }
}

@Composable
private fun IdleKeyboardStrip(
    scale: Float,
    micColor: Color,
    shape: RoundedCornerShape,
    showRetry: Boolean,
    onMic: () -> Unit,
    onRetry: () -> Unit,
    onDragStart: (Float, Float) -> Unit,
    onDragTo: (Float, Float) -> Unit,
    onDragEnd: () -> Unit,
    onDismissArmed: (Boolean) -> Unit,
) {
    val c = LocalMsColors.current
    var moving by remember { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxWidth()
            .height(52.dp * scale)
            .clip(shape)
            .background(
                Brush.linearGradient(listOf(Color(0xFF141416), Color(0xFF0C0C0E), Color(0xFF080808))),
                shape,
            )
            .border(
                1.dp,
                if (moving) micColor.copy(alpha = 0.55f) else Color.White.copy(alpha = 0.10f),
                shape,
            )
            .holdToMove(
                onDragStart = onDragStart,
                onDragTo = onDragTo,
                onDragEnd = onDragEnd,
                onDismissArmed = onDismissArmed,
                onTap = onMic,
                onMoving = { moving = it },
            )
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (showRetry) {
            Box(
                Modifier
                    .clip(RoundedCornerShape(percent = 50))
                    .background(Orange)
                    .clickable(onClick = onRetry)
                    .padding(horizontal = 14.dp, vertical = 8.dp),
            ) {
                Text("Retry", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        Box(
            Modifier
                .size(36.dp * scale)
                .clip(CircleShape)
                .background(Color(0xFF1A1A1C), CircleShape)
                .border(1.dp, micColor.copy(alpha = 0.45f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.Mic,
                contentDescription = "Start dictation",
                tint = micColor,
                modifier = Modifier.size(18.dp * scale),
            )
        }
        Text(
            text = if (showRetry) "Tap mic to try again" else "Tap to dictate",
            color = c.text.copy(alpha = 0.88f),
            fontSize = 15.sp,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun ActiveKeyboardStrip(
    levels: List<Float>,
    phase: DictationPhase,
    scale: Float,
    waveScale: Float,
    shape: RoundedCornerShape,
    onCancel: () -> Unit,
    onProceed: () -> Unit,
) {
    val c = LocalMsColors.current
    val btn = 40.dp * scale
    val waveH = KeyboardBarMaxH * scale * waveScale.coerceIn(0.55f, 1.55f)
    val checkBg = when (phase) {
        DictationPhase.Confirm -> Orange
        DictationPhase.Processing -> Turquoise.copy(alpha = 0.45f)
        else -> Turquoise
    }
    Row(
        Modifier
            .fillMaxWidth()
            .height(64.dp * scale)
            .clip(shape)
            .background(
                Brush.linearGradient(listOf(Color(0xFF121214), Color(0xFF0A0A0C), Color(0xFF060608))),
                shape,
            )
            .border(1.dp, Color.White.copy(alpha = 0.12f), shape)
            .padding(horizontal = 12.dp * scale),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp * scale),
    ) {
        Box(
            Modifier
                .size(btn)
                .clip(CircleShape)
                .background(c.glassFill.copy(alpha = 0.35f), CircleShape)
                .border(1.dp, Color.White.copy(alpha = 0.12f), CircleShape)
                .clickable(onClick = onCancel)
                .semantics { contentDescription = "Cancel dictation" },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.Close,
                contentDescription = null,
                tint = Color.White.copy(alpha = 0.92f),
                modifier = Modifier.size(btn * 0.42f),
            )
        }

        Box(
            Modifier
                .weight(1f)
                .height(waveH + 16.dp * scale)
                .clip(RoundedCornerShape(percent = 50))
                .background(Color(0xFF050505), RoundedCornerShape(percent = 50))
                .border(1.dp, Color.Black.copy(alpha = 0.85f), RoundedCornerShape(percent = 50))
                .padding(horizontal = 14.dp * scale, vertical = 8.dp * scale),
            contentAlignment = Alignment.Center,
        ) {
            BoxWithConstraints(Modifier.fillMaxWidth()) {
                val n = KeyboardBarCount
                val gap = 5.dp * scale
                val barW = ((maxWidth - gap * (n - 1)) / n).coerceIn(4.dp, 16.dp)
                WindowsWaveform(
                    levels = levels,
                    barCount = n,
                    maxBarHeight = waveH,
                    barWidth = barW,
                    barGap = gap,
                    modifier = Modifier.height(waveH),
                )
            }
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
                modifier = Modifier.size(btn * 0.42f),
            )
        }
    }
}
