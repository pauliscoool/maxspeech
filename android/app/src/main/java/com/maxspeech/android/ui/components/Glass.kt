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
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall

@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(22.dp),
    contentAlignment: Alignment = Alignment.TopStart,
    content: @Composable BoxScope.() -> Unit,
) {
    val c = LocalMsColors.current
    Box(
        modifier = modifier
            .clip(shape)
            .background(c.glassFill)
            .border(Dp.Hairline, c.hairline, shape),
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
    Box(
        modifier = Modifier
            .clip(shape)
            .background(if (selected) c.turquoise.copy(alpha = 0.22f) else c.glassFill)
            .border(Dp.Hairline, if (selected) c.turquoise.copy(alpha = 0.45f) else c.hairline, shape)
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
    Box(
        modifier = modifier
            .clip(CircleShape)
            .background(c.glassFill)
            .border(Dp.Hairline, c.hairline, CircleShape)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
        content = content,
    )
}
