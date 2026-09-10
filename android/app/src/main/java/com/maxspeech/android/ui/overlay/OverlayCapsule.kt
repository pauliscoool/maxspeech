package com.maxspeech.android.ui.overlay

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.pipeline.DictationUi
import com.maxspeech.android.ui.components.RibbonWaveform
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun OverlayCapsule(
    ui: DictationUi,
    glassAlpha: Float,
    onHoldStart: () -> Unit,
    onHoldEnd: () -> Unit,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
) {
    val c = LocalMsColors.current
    val shape = RoundedCornerShape(28.dp)
    when (ui.phase) {
        DictationPhase.Confirm, DictationPhase.Listening, DictationPhase.Processing -> {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.padding(8.dp),
            ) {
                Box(
                    Modifier
                        .size(44.dp)
                        .clip(CircleShape)
                        .background(c.glassFill.copy(alpha = glassAlpha))
                        .border(1.dp, c.hairline, CircleShape)
                        .pointerInput(Unit) { detectTapGestures { onCancel() } },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Filled.Close, contentDescription = "Cancel", tint = c.text)
                }
                Box(
                    Modifier
                        .widthIn(min = 148.dp)
                        .height(44.dp)
                        .clip(shape)
                        .background(c.glassFill.copy(alpha = glassAlpha))
                        .border(1.dp, c.hairline, shape)
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
                        .pointerInput(Unit) { detectTapGestures { if (ui.phase == DictationPhase.Confirm) onConfirm() else onHoldEnd() } },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Filled.Check, contentDescription = "Confirm", tint = Color.White)
                }
            }
        }
        else -> {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .widthIn(min = 260.dp)
                    .height(52.dp)
                    .clip(shape)
                    .background(c.capsule, shape)
                    .pointerInput(Unit) {
                        detectTapGestures(
                            onPress = {
                                onHoldStart()
                                try {
                                    tryAwaitRelease()
                                } finally {
                                    onHoldEnd()
                                }
                            },
                        )
                    }
                    .padding(horizontal = 18.dp),
            ) {
                Text(
                    text = ui.error ?: "Hold to speak",
                    color = Color.White.copy(alpha = 0.92f),
                    fontSize = 16.sp,
                    modifier = Modifier.weight(1f),
                )
                Icon(Icons.Filled.Mic, contentDescription = "Microphone", tint = Color.White)
            }
        }
    }
}
