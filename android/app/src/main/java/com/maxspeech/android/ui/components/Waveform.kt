package com.maxspeech.android.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun RibbonWaveform(
    levels: List<Float>,
    modifier: Modifier = Modifier,
) {
    Canvas(modifier.height(28.dp).fillMaxWidth()) {
        if (levels.isEmpty()) return@Canvas
        val n = levels.size
        val gap = 3.dp.toPx()
        val barW = 3.dp.toPx()
        val total = n * barW + (n - 1) * gap
        val startX = (size.width - total) / 2f
        val brush = Brush.horizontalGradient(
            listOf(Turquoise, Color(0xFF86EFD0), Color(0xFFFDBA74), Orange),
        )
        levels.forEachIndexed { i, raw ->
            val h = (raw.coerceIn(0.12f, 1f) * size.height).coerceAtLeast(4f)
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
