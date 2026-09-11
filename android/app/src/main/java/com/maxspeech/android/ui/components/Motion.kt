package com.maxspeech.android.ui.components

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import com.maxspeech.android.data.UiTheme
import com.maxspeech.android.ui.theme.Turquoise
import kotlinx.coroutines.delay

val PageEnterSpec = fadeIn(tween(200)) + slideInVertically(tween(200)) { it / 10 } togetherWith
    fadeOut(tween(120))

@Composable
fun <T> PageEnter(
    target: T,
    modifier: Modifier = Modifier,
    content: @Composable (T) -> Unit,
) {
    AnimatedContent(
        targetState = target,
        modifier = modifier,
        transitionSpec = { PageEnterSpec },
        label = "page-enter",
        content = { content(it) },
    )
}

@Composable
fun ThemeWipe(theme: UiTheme) {
    var previous by remember { mutableStateOf(theme) }
    var token by remember { mutableIntStateOf(0) }
    var show by remember { mutableStateOf(false) }
    LaunchedEffect(theme) {
        if (theme != previous) {
            previous = theme
            token += 1
            show = true
            delay(680)
            show = false
        }
    }
    if (!show) return
    val progress = remember(token) { androidx.compose.animation.core.Animatable(0f) }
    LaunchedEffect(token) {
        progress.snapTo(0f)
        progress.animateTo(1f, tween(650, easing = androidx.compose.animation.core.FastOutSlowInEasing))
    }
    val fill = when (theme) {
        UiTheme.Dark -> Color(0xFF0A0A0A)
        UiTheme.Gray -> Color(0xFF1A1A1C)
        UiTheme.Light -> Color(0xFFF3F4F6)
    }
    Canvas(Modifier.fillMaxSize()) {
        val p = progress.value
        val r = size.maxDimension * 0.08f + size.maxDimension * 1.6f * p
        drawCircle(
            color = fill.copy(alpha = (0.55f * (1f - p)).coerceIn(0f, 0.55f)),
            radius = r,
            center = Offset(size.width / 2f, size.height / 2f),
        )
    }
}

@Composable
fun MsSpinner(modifier: Modifier = Modifier, color: Color = Turquoise) {
    val spin = rememberInfiniteTransition(label = "ms-spin")
    val angle by spin.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(tween(550, easing = LinearEasing), RepeatMode.Restart),
        label = "ms-spin-angle",
    )
    Canvas(modifier.size(22.dp).rotate(angle)) {
        val stroke = Stroke(width = 2.5.dp.toPx(), cap = StrokeCap.Round)
        drawArc(
            color = color,
            startAngle = 0f,
            sweepAngle = 270f,
            useCenter = false,
            style = stroke,
        )
    }
}

@Composable
fun SplashPane(modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit = {}) {
    Box(
        modifier.fillMaxSize().background(Color(0xFF000000)),
        contentAlignment = Alignment.Center,
        content = content,
    )
}
