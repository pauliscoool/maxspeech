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
import com.maxspeech.android.pipeline.AudioCapture
import com.maxspeech.android.ui.theme.Orange

/** Overlay compact pill uses 6 bands; keyboard dock uses a wider 12-bar ribbon. */
const val KeyboardBarCount = 12
val KeyboardBarWidth = 5.dp
val KeyboardBarGap = 4.dp
val KeyboardBarMaxH = 26.dp

/** Overlay bar count — compact six-bar pill. */
const val DesktopBarCount = AudioCapture.BAR_COUNT
val DesktopBarWidth = 4.dp
val DesktopBarGap = 3.dp
/** Taller track so levels can actually jump. */
val DesktopBarMaxH = 22.dp

/** In-app ribbon (full width). */
@Composable
fun RibbonWaveform(
    levels: List<Float>,
    modifier: Modifier = Modifier,
) {
    WindowsWaveform(
        levels = levels,
        barCount = DesktopBarCount,
        maxBarHeight = 28.dp,
        modifier = modifier.height(28.dp).fillMaxWidth(),
    )
}

/**
 * Calm teal bars — shared ribbon wash, left-aligned inside exact wave width.
 */
@Composable
fun WindowsWaveform(
    levels: List<Float>,
    barCount: Int = DesktopBarCount,
    maxBarHeight: Dp = DesktopBarMaxH,
    barWidth: Dp = DesktopBarWidth,
    barGap: Dp = DesktopBarGap,
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
        val brush = Brush.horizontalGradient(
            listOf(
                Color(0xFF2DD4BF),
                Color(0xFF5EEAD4),
                Color(0xFF86EFD0),
                Color(0xFFFDBA74),
                Orange,
            ),
            startX = 0f,
            endX = size.width,
        )
        val denom = ((n - 1) / 2f).coerceAtLeast(1f)
        for (i in 0 until n) {
            val src = if (levels.size == n) {
                levels[i]
            } else {
                val t = if (n == 1) 0f else i / (n - 1).toFloat()
                val idx = (t * (levels.size - 1)).toInt().coerceIn(0, levels.lastIndex)
                levels[idx]
            }
            val mid = 1f - (kotlin.math.abs(i - (n - 1) / 2f) / denom) * 0.18f
            // Use full canvas height so loud levels nearly fill the pill.
            val h = (src.coerceIn(0.08f, 1f) * mid * size.height).coerceAtLeast(3f)
            val x = i * (barW + gap)
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
