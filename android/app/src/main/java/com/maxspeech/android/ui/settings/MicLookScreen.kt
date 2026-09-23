package com.maxspeech.android.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.OverlayStyle
import com.maxspeech.android.ui.components.DesktopBarCount
import com.maxspeech.android.ui.components.DesktopBarGap
import com.maxspeech.android.ui.components.DesktopBarMaxH
import com.maxspeech.android.ui.components.DesktopBarWidth
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.components.KeyboardBarCount
import com.maxspeech.android.ui.components.KeyboardBarMaxH
import com.maxspeech.android.ui.components.WindowsWaveform
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Turquoise
import kotlin.math.roundToInt

private val MicColorPresets = listOf(
    Color(0xFF2DD4BF),
    Color(0xFFF97316),
    Color(0xFF60A5FA),
    Color(0xFFF472B6),
    Color(0xFFA78BFA),
    Color(0xFFFACC15),
    Color(0xFFFFFFFF),
)

@Composable
fun MicLookScreen(
    settings: AppSettings,
    onBack: () -> Unit,
    onSize: (Float) -> Unit,
    onAlpha: (Float) -> Unit,
    onWave: (Float) -> Unit,
    onColor: (Long) -> Unit,
    onStyle: (OverlayStyle) -> Unit,
) {
    val c = LocalMsColors.current
    val micColor = Color(settings.overlayMicColor.toInt())
    val scale = settings.overlaySize
    val alpha = settings.overlayAlpha
    val waveScale = settings.overlayWaveScale
    val dock = settings.overlayStyle == OverlayStyle.KeyboardBar
    val previewLevels = remember(dock) {
        val n = if (dock) KeyboardBarCount else DesktopBarCount
        List(n) { i -> 0.22f + (i % 5) * 0.14f }
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding(),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(
                    Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = "Back",
                    tint = c.text,
                )
            }
            Text("Mic look", style = DisplayLarge, color = c.text)
        }

        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                .padding(bottom = 120.dp),
        ) {
            Text(
                "Preview",
                color = c.textDim,
                fontSize = 13.sp,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.padding(top = 8.dp, bottom = 10.dp),
            )
            GlassSurface(Modifier.fillMaxWidth()) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(vertical = 24.dp, horizontal = 14.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(18.dp),
                ) {
                    if (dock) {
                        KeyboardPreview(
                            levels = previewLevels,
                            scale = scale,
                            alpha = alpha,
                            waveScale = waveScale,
                            micColor = micColor,
                        )
                    } else {
                        FloatingPreview(
                            levels = previewLevels,
                            scale = scale,
                            alpha = alpha,
                            waveScale = waveScale,
                            micColor = micColor,
                        )
                    }
                }
            }

            Spacer(Modifier.height(22.dp))
            GlassSurface(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    SliderBlock(
                        title = "Size",
                        caption = "${(settings.overlaySize * 100).roundToInt()}%",
                        value = settings.overlaySize,
                        range = 0.55f..1.45f,
                        onChange = onSize,
                    )
                    SliderBlock(
                        title = "Transparency",
                        caption = "${((1f - settings.overlayAlpha) * 100).roundToInt()}% transparent",
                        value = 1f - settings.overlayAlpha,
                        range = 0f..0.75f,
                        onChange = { onAlpha(1f - it) },
                    )
                    SliderBlock(
                        title = "Wave emphasis",
                        caption = "${(settings.overlayWaveScale * 100).roundToInt()}% bar height",
                        value = settings.overlayWaveScale,
                        range = 0.55f..1.55f,
                        onChange = onWave,
                    )

                    Text("Mic color", color = c.text, modifier = Modifier.padding(top = 10.dp))
                    Text(
                        "Idle mic icon tint.",
                        color = c.textDim,
                        fontSize = 12.sp,
                        modifier = Modifier.padding(bottom = 8.dp),
                    )
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        MicColorPresets.forEach { swatch ->
                            val argb = swatch.toArgb().toLong() and 0xFFFFFFFFL
                            val on = settings.overlayMicColor == argb ||
                                (settings.overlayMicColor.toInt() == swatch.toArgb())
                            Box(
                                Modifier
                                    .size(36.dp)
                                    .clip(CircleShape)
                                    .background(swatch, CircleShape)
                                    .border(
                                        width = if (on) 2.5.dp else 1.dp,
                                        color = if (on) Turquoise else Color.White.copy(alpha = 0.2f),
                                        shape = CircleShape,
                                    )
                                    .clickable { onColor(argb) },
                            )
                        }
                    }

                    Text("Hue", color = c.text, modifier = Modifier.padding(top = 14.dp))
                    val hsv = FloatArray(3)
                    android.graphics.Color.colorToHSV(micColor.toArgb(), hsv)
                    Slider(
                        value = hsv[0],
                        onValueChange = { hue ->
                            val next = Color.hsv(
                                hue,
                                hsv[1].coerceAtLeast(0.45f),
                                hsv[2].coerceAtLeast(0.75f),
                            )
                            onColor(next.toArgb().toLong() and 0xFFFFFFFFL)
                        },
                        valueRange = 0f..360f,
                        colors = SliderDefaults.colors(
                            thumbColor = micColor,
                            activeTrackColor = micColor,
                            inactiveTrackColor = c.hairline,
                        ),
                    )
                }
            }

            Spacer(Modifier.height(22.dp))
            Text(
                "Extras",
                color = c.textDim,
                fontSize = 13.sp,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.padding(bottom = 10.dp),
            )
            ExtrasStylePicker(
                style = settings.overlayStyle,
                micColor = micColor,
                sizeScale = scale,
                surfaceAlpha = alpha,
                waveScale = waveScale,
                onStyle = onStyle,
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ExtrasStylePicker(
    style: OverlayStyle,
    micColor: Color,
    sizeScale: Float,
    surfaceAlpha: Float,
    waveScale: Float,
    onStyle: (OverlayStyle) -> Unit,
) {
    val c = LocalMsColors.current
    var open by remember { mutableStateOf(false) }
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val label = when (style) {
        OverlayStyle.Floating -> "Floating bubble"
        OverlayStyle.KeyboardBar -> "Keyboard bar"
    }
    val hint = when (style) {
        OverlayStyle.Floating -> "Drag anywhere · 6-bar pill"
        OverlayStyle.KeyboardBar -> "Above keyboard · 12-bar strip"
    }
    val pickLevels6 = remember { List(DesktopBarCount) { i -> 0.28f + (i % 4) * 0.12f } }
    val pickLevels12 = remember { List(KeyboardBarCount) { i -> 0.26f + (i % 5) * 0.11f } }

    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(c.surface)
            .border(1.dp, c.hairline, RoundedCornerShape(18.dp))
            .clickable { open = true }
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("Presentation", color = c.textDim, fontSize = 12.sp)
                Text(label, color = c.text, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                Text(hint, color = c.textDim, fontSize = 12.sp, modifier = Modifier.padding(top = 2.dp))
            }
            Icon(Icons.Filled.KeyboardArrowDown, contentDescription = null, tint = c.textDim)
        }
        // Selected style = live mini preview of that widget.
        Box(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(14.dp))
                .background(c.bgSoft)
                .padding(horizontal = 12.dp, vertical = 14.dp),
            contentAlignment = Alignment.Center,
        ) {
            when (style) {
                OverlayStyle.Floating -> FloatingPreview(
                    levels = pickLevels6,
                    scale = sizeScale * 0.72f,
                    alpha = surfaceAlpha,
                    waveScale = waveScale,
                    micColor = micColor,
                    compact = true,
                )
                OverlayStyle.KeyboardBar -> KeyboardPreview(
                    levels = pickLevels12,
                    scale = sizeScale * 0.72f,
                    alpha = surfaceAlpha,
                    waveScale = waveScale,
                    micColor = micColor,
                    compact = true,
                )
            }
        }
    }

    if (open) {
        ModalBottomSheet(
            onDismissRequest = { open = false },
            sheetState = sheetState,
            containerColor = c.surface,
            tonalElevation = 0.dp,
        ) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp)
                    .padding(bottom = 28.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text("Extras", color = c.text, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
                Text(
                    "Pick a presentation — preview shows how it looks over other apps.",
                    color = c.textDim,
                    fontSize = 13.sp,
                )
                StyleOptionCard(
                    title = "Floating bubble",
                    subtitle = "Draggable mic. Compact 6-bar pill when listening.",
                    selected = style == OverlayStyle.Floating,
                    onClick = {
                        onStyle(OverlayStyle.Floating)
                        open = false
                    },
                ) {
                    FloatingPreview(
                        levels = pickLevels6,
                        scale = 0.62f,
                        alpha = surfaceAlpha,
                        waveScale = waveScale,
                        micColor = micColor,
                        compact = true,
                    )
                }
                StyleOptionCard(
                    title = "Keyboard bar",
                    subtitle = "Fades in above the keyboard. Wide 12-bar strip with X and ✓.",
                    selected = style == OverlayStyle.KeyboardBar,
                    onClick = {
                        onStyle(OverlayStyle.KeyboardBar)
                        open = false
                    },
                ) {
                    KeyboardPreview(
                        levels = pickLevels12,
                        scale = 0.62f,
                        alpha = surfaceAlpha,
                        waveScale = waveScale,
                        micColor = micColor,
                        compact = true,
                    )
                }
            }
        }
    }
}

@Composable
private fun StyleOptionCard(
    title: String,
    subtitle: String,
    selected: Boolean,
    onClick: () -> Unit,
    preview: @Composable () -> Unit,
) {
    val c = LocalMsColors.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(if (selected) Turquoise.copy(alpha = 0.12f) else c.bgSoft)
            .border(
                width = if (selected) 1.5.dp else 1.dp,
                color = if (selected) Turquoise.copy(alpha = 0.55f) else c.hairline,
                shape = RoundedCornerShape(18.dp),
            )
            .clickable(onClick = onClick)
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(
                    title,
                    color = if (selected) Turquoise else c.text,
                    fontSize = 15.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(subtitle, color = c.textDim, fontSize = 12.sp, modifier = Modifier.padding(top = 3.dp))
            }
            if (selected) {
                Icon(
                    Icons.Filled.Check,
                    contentDescription = null,
                    tint = Turquoise,
                    modifier = Modifier.size(20.dp),
                )
            }
        }
        Box(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(12.dp))
                .background(Color(0xFF0A0A0A).copy(alpha = 0.55f))
                .padding(horizontal = 10.dp, vertical = 12.dp),
            contentAlignment = Alignment.Center,
        ) {
            preview()
        }
    }
}

@Composable
private fun FloatingPreview(
    levels: List<Float>,
    scale: Float,
    alpha: Float,
    waveScale: Float,
    micColor: Color,
    compact: Boolean = false,
) {
    val c = LocalMsColors.current
    val pillShape = RoundedCornerShape(percent = 50)
    val btn = 38.dp * scale
    val waveH = DesktopBarMaxH * scale * waveScale
    val barW = DesktopBarWidth * scale
    val barGap = DesktopBarGap * scale
    val waveW = barW * DesktopBarCount + barGap * (DesktopBarCount - 1)
    val micSize = 54.dp * scale

    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(if (compact) 12.dp else 18.dp),
    ) {
        Box(
            Modifier
                .size(micSize)
                .clip(CircleShape)
                .background(
                    Brush.linearGradient(listOf(Color(0xFF121212), Color(0xFF0A0A0A), Color(0xFF050505))),
                    CircleShape,
                )
                .border(1.dp, Color.White.copy(alpha = 0.10f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.Mic,
                contentDescription = null,
                tint = micColor.copy(alpha = alpha),
                modifier = Modifier.size(micSize * 0.42f),
            )
        }

        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp * scale),
        ) {
            Box(
                Modifier
                    .size(btn)
                    .clip(CircleShape)
                    .background(c.glassFill.copy(alpha = 0.35f), CircleShape)
                    .border(1.dp, Color.White.copy(alpha = 0.12f), CircleShape),
            )
            Box(
                Modifier
                    .height((btn * 0.92f).coerceAtLeast(waveH + 10.dp * scale))
                    .clip(pillShape)
                    .background(
                        Brush.linearGradient(listOf(Color(0xFF0A0A0A), Color(0xFF080808), Color(0xFF040404))),
                        pillShape,
                    )
                    .border(1.dp, Color.Black.copy(alpha = 0.9f), pillShape)
                    .padding(horizontal = 12.dp * scale, vertical = 5.dp * scale),
                contentAlignment = Alignment.Center,
            ) {
                WindowsWaveform(
                    levels = levels,
                    barCount = DesktopBarCount,
                    maxBarHeight = waveH,
                    barWidth = barW,
                    barGap = barGap,
                    modifier = Modifier.height(waveH).width(waveW),
                )
            }
            Box(
                Modifier
                    .size(btn)
                    .clip(CircleShape)
                    .background(Turquoise.copy(alpha = alpha), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Text("✓", color = Color.White, fontSize = (16 * scale).sp)
            }
        }
    }
}

@Composable
private fun KeyboardPreview(
    levels: List<Float>,
    scale: Float,
    alpha: Float,
    waveScale: Float,
    micColor: Color,
    compact: Boolean = false,
) {
    val c = LocalMsColors.current
    val shape = RoundedCornerShape(16.dp)
    val btn = 36.dp * scale
    val waveH = KeyboardBarMaxH * scale * waveScale * 0.95f
    val shrink = 0.95f

    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 14.dp),
    ) {
        if (!compact) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(48.dp * scale * shrink)
                    .clip(shape)
                    .background(Color(0xFF121214), shape)
                    .border(1.dp, Color.White.copy(alpha = 0.10f), shape)
                    .padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Box(
                    Modifier
                        .size(30.dp * scale)
                        .clip(CircleShape)
                        .background(Color(0xFF1A1A1C), CircleShape)
                        .border(1.dp, micColor.copy(alpha = 0.45f), CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        Icons.Filled.Mic,
                        null,
                        tint = micColor.copy(alpha = alpha),
                        modifier = Modifier.size(15.dp * scale),
                    )
                }
                Text("Tap to dictate", color = c.text.copy(alpha = 0.85f), fontSize = 13.sp)
            }
        }

        Row(
            Modifier
                .fillMaxWidth()
                .height(60.dp * scale * shrink)
                .clip(shape)
                .background(Color(0xFF0C0C0E), shape)
                .border(1.dp, Color.White.copy(alpha = 0.12f), shape)
                .padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Box(
                Modifier
                    .size(btn)
                    .clip(CircleShape)
                    .background(c.glassFill.copy(alpha = 0.35f), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Text("✕", color = Color.White, fontSize = 13.sp)
            }
            BoxWithConstraints(
                Modifier
                    .weight(1f)
                    .height(waveH + 10.dp)
                    .clip(RoundedCornerShape(percent = 50))
                    .background(Color(0xFF050505), RoundedCornerShape(percent = 50))
                    .padding(horizontal = 10.dp, vertical = 5.dp),
                contentAlignment = Alignment.Center,
            ) {
                val n = KeyboardBarCount
                val gap = 3.5.dp
                val barW = ((maxWidth - gap * (n - 1)) / n).coerceIn(2.5.dp, 10.dp)
                WindowsWaveform(
                    levels = levels,
                    barCount = n,
                    maxBarHeight = waveH,
                    barWidth = barW,
                    barGap = gap,
                    modifier = Modifier.height(waveH),
                )
            }
            Box(
                Modifier
                    .size(btn)
                    .clip(CircleShape)
                    .background(Turquoise.copy(alpha = alpha), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Text("✓", color = Color.White, fontSize = 13.sp)
            }
        }
    }
}

@Composable
private fun SliderBlock(
    title: String,
    caption: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    onChange: (Float) -> Unit,
) {
    val c = LocalMsColors.current
    Text(title, color = c.text)
    Text(caption, color = c.textDim, fontSize = 12.sp)
    Slider(
        value = value,
        onValueChange = onChange,
        valueRange = range,
        colors = SliderDefaults.colors(thumbColor = Turquoise, activeTrackColor = Turquoise),
    )
}
