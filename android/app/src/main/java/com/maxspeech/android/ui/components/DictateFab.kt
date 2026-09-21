package com.maxspeech.android.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise

/**
 * Always-visible mic control for in-app dictation.
 * Tap toggles: idle → listen, listening → finish, confirm → paste/confirm.
 */
@Composable
fun DictateFab(
    phase: DictationPhase,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val active = phase == DictationPhase.Listening ||
        phase == DictationPhase.Processing ||
        phase == DictationPhase.Confirm
    val scale by animateFloatAsState(
        targetValue = if (active) 1.06f else 1f,
        animationSpec = tween(180),
        label = "dictateFabScale",
    )
    val bg = when (phase) {
        DictationPhase.Confirm -> Orange
        DictationPhase.Listening, DictationPhase.Processing -> Orange
        else -> Turquoise
    }
    val icon = when (phase) {
        DictationPhase.Confirm -> Icons.Filled.Check
        DictationPhase.Listening, DictationPhase.Processing -> Icons.Filled.Stop
        else -> Icons.Filled.Mic
    }
    val label = when (phase) {
        DictationPhase.Confirm -> "Confirm dictation"
        DictationPhase.Listening, DictationPhase.Processing -> "Stop dictation"
        else -> "Start dictation"
    }
    Box(
        modifier = modifier
            .size(72.dp)
            .scale(scale)
            .clip(CircleShape)
            .background(bg)
            .clickable(onClick = onToggle)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(32.dp))
    }
}
