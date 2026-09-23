package com.maxspeech.android.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.ui.theme.LocalGlassAlpha
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall

/** Plain solid fill — no frost / liquid sheen. */
fun Modifier.plainSurface(
    shape: Shape,
    fill: Color,
    hairline: Color,
    elevation: Dp = 6.dp,
): Modifier = this
    .shadow(
        elevation = elevation,
        shape = shape,
        ambientColor = Color.Black.copy(alpha = 0.40f),
        spotColor = Color.Black.copy(alpha = 0.22f),
    )
    .clip(shape)
    .background(fill, shape)
    .border(Dp.Hairline, hairline, shape)

@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(22.dp),
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val c = LocalMsColors.current
    Box(
        modifier = modifier.plainSurface(shape, c.surface, c.hairline),
        contentAlignment = contentAlignment,
        content = content,
    )
}

/** Desktop `.surface-card` — solid surface, soft shadow. */
@Composable
fun SurfaceCard(
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(18.dp),
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val c = LocalMsColors.current
    Box(
        modifier = modifier.plainSurface(shape, c.surface, c.hairline, elevation = 8.dp),
        contentAlignment = contentAlignment,
        content = content,
    )
}

@Composable
fun GlassChip(
    label: String,
    selected: Boolean = false,
    onClick: () -> Unit,
) {
    val c = LocalMsColors.current
    val shape = RoundedCornerShape(50)
    val fill = if (selected) c.turquoise.copy(alpha = 0.18f) else c.surface2
    val line = if (selected) c.turquoise.copy(alpha = 0.55f) else c.hairline
    Box(
        modifier = Modifier
            .plainSurface(shape, fill, line, elevation = 2.dp)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    ) {
        Text(
            label,
            style = TitleSmall,
            color = if (selected) c.turquoise else c.text,
            fontSize = 14.sp,
        )
    }
}

@Composable
fun GlassIconButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val c = LocalMsColors.current
    Box(
        modifier = modifier
            .plainSurface(CircleShape, c.surface2, c.hairline, elevation = 2.dp)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
        content = content,
    )
}

@Composable
fun GlassScrim(modifier: Modifier = Modifier) {
    val c = LocalMsColors.current
    val glass = LocalGlassAlpha.current
    Box(
        modifier.background(
            Brush.verticalGradient(
                0f to c.bg.copy(alpha = 0.15f * glass),
                0.45f to c.bg.copy(alpha = 0.42f * glass),
                1f to c.bg.copy(alpha = 0.78f),
            ),
        ),
    )
}

/** Bottom tab bar — solid pill, no liquid frost. */
@Composable
fun FloatingGlassBar(
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(50),
    contentAlignment: Alignment = Alignment.Center,
    content: @Composable BoxScope.() -> Unit,
) {
    val c = LocalMsColors.current
    val fill = if (c.bg.luminance() > 0.5f) {
        Color.White
    } else {
        Color(0xFF1C1C1E)
    }
    val rim = if (c.bg.luminance() > 0.5f) {
        Color.Black.copy(alpha = 0.08f)
    } else {
        Color.White.copy(alpha = 0.10f)
    }
    Box(
        modifier = modifier.plainSurface(shape, fill, rim, elevation = 16.dp),
        contentAlignment = contentAlignment,
        content = content,
    )
}

private fun Color.luminance(): Float {
    val r = red
    val g = green
    val b = blue
    return 0.2126f * r + 0.7152f * g + 0.0722f * b
}
