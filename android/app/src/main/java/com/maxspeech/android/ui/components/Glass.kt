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
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.ui.theme.LocalBlurStrength
import com.maxspeech.android.ui.theme.LocalGlassAlpha
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall

fun Modifier.liquidGlass(
    shape: Shape,
    fill: Color,
    hairline: Color,
    blur: Float = 0.7f,
): Modifier {
    val lift = (10.dp * blur.coerceIn(0.2f, 1f))
    return this
        .shadow(lift, shape, ambientColor = Color.Black.copy(alpha = 0.35f), spotColor = Color.White.copy(alpha = 0.08f))
        .clip(shape)
        .drawBehind {
            val outline = shape.createOutline(this.size, layoutDirection, this)
            drawOutline(outline, fill)
            drawOutline(
                outline,
                Brush.verticalGradient(
                    0f to Color.White.copy(alpha = 0.22f),
                    0.35f to Color.White.copy(alpha = 0.06f),
                    1f to Color.Transparent,
                ),
            )
            drawCircle(
                brush = Brush.radialGradient(
                    colors = listOf(Color.White.copy(alpha = 0.28f), Color.Transparent),
                    center = Offset(size.width * 0.18f, size.height * 0.08f),
                    radius = size.minDimension * 0.85f,
                ),
                radius = size.minDimension * 0.85f,
                center = Offset(size.width * 0.18f, size.height * 0.08f),
            )
            drawOutline(outline, hairline.copy(alpha = 0.55f), style = Stroke(width = Dp.Hairline.toPx().coerceAtLeast(1f)))
        }
        .border(Dp.Hairline, hairline, shape)
}

@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(22.dp),
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val c = LocalMsColors.current
    val blur = LocalBlurStrength.current
    Box(
        modifier = modifier.liquidGlass(shape, c.glassFill, c.hairline, blur),
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
    val blur = LocalBlurStrength.current
    val shape = RoundedCornerShape(50)
    val fill = if (selected) c.turquoise.copy(alpha = 0.22f) else c.glassFill
    val line = if (selected) c.turquoise.copy(alpha = 0.45f) else c.hairline
    Box(
        modifier = Modifier
            .liquidGlass(shape, fill, line, blur)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    ) {
        Text(label, style = TitleSmall, color = c.text, fontSize = 14.sp)
    }
}

@Composable
fun GlassIconButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    val c = LocalMsColors.current
    val blur = LocalBlurStrength.current
    Box(
        modifier = modifier
            .liquidGlass(CircleShape, c.glassFill, c.hairline, blur)
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
