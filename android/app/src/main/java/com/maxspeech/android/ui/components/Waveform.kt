package com.maxspeech.android.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.maxspeech.android.ui.theme.Orange

/** In-app ribbon (full width). */
@Composable
fun RibbonWaveform(
    levels: List<Float>,
    modifier: Modifier = Modifier,
) {
    WindowsWaveform(
        levels = levels,
        barCount = 20,
        maxBarHeight = 28.dp,
        modifier = modifier.height(28.dp).fillMaxWidth(),
    )
}

/**
 * Windows-style calm teal bars — shared ribbon wash.
 * Overlay uses [barCount]=10 (half of desktop's 20).
 */
@Composable
fun WindowsWaveform(
    levels: List<Float>,
    barCount: Int = 10,
    maxBarHeight: Dp = 14.dp,
    barWidth: Dp = 3.dp,
    barGap: Dp = 3.dp,
    modifier: Modifier = Modifier,
) {
    val n = barCount.coerceAtLeast(1)
    val waveW = barWidth * n + barGap * (n - 1)
    Canvas(
        modifier
            .width(waveW)
            .height(maxBarHeight),
    ) {
        if (levels.isEmpty()) return@Canvas
        val gap = barGap.toPx()
        val barW = barWidth.toPx()
        val total = n * barW + (n - 1) * gap
        val startX = (size.width - total) / 2f
        val brush = Brush.horizontalGradient(
            listOf(
                Color(0xFF2DD4BF),
                Color(0xFF5EEAD4),
                Color(0xFF86EFD0),
                Color(0xFFFDBA74),
                Orange,
            ),
        )
        for (i in 0 until n) {
            val src = if (levels.size == n) {
                levels[i]
            } else {
                val t = i / (n - 1).coerceAtLeast(1).toFloat()
                val idx = (t * (levels.size - 1)).toInt().coerceIn(0, levels.lastIndex)
                levels[idx]
            }
            val mid = 1f - (kotlin.math.abs(i - (n - 1) / 2f) / ((n - 1) / 2f).coerceAtLeast(1f)) * 0.18f
            val h = (src.coerceIn(0.12f, 1f) * mid * size.height).coerceAtLeast(3f)
            val x = startX + i * (barW + gap)
            val y = (size.height - h) / 2f
            drawRoundRect(
                brush = brush,
                topLeft = Offset(x, y),
                size = Size(barW, h),
                cornerRadius = CornerRadius(barW / 2, barW / 2),
            )
        }
    }
}
