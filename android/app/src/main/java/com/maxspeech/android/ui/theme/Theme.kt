package com.maxspeech.android.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.UiTheme

val Turquoise = Color(0xFF2DD4BF)
val TurquoiseDim = Color(0xFF14B8A6)
val Orange = Color(0xFFF97316)
val Hairline = Color.White.copy(alpha = 0.12f)

data class MsColors(
    val bg: Color,
    val bgSoft: Color,
    val surface: Color,
    val surface2: Color,
    val text: Color,
    val textDim: Color,
    val glassFill: Color,
    val hairline: Color,
    val turquoise: Color,
    val orange: Color,
    val error: Color,
    val capsule: Brush,
)

val LocalMsColors = staticCompositionLocalOf { darkColors(0.50f) }
val LocalGlassAlpha = staticCompositionLocalOf { 0.50f }

fun darkColors(glass: Float) = MsColors(
    bg = Color(0xFF000000),
    bgSoft = Color(0xFF0A0A0A),
    surface = Color(0xFF111111),
    surface2 = Color(0xFF1A1A1A),
    text = Color.White,
    textDim = Color(0xFFA3A3A3),
    glassFill = Color.White.copy(alpha = 0.10f * (glass / 0.5f).coerceIn(0.15f, 2.2f)),
    hairline = Color.White.copy(alpha = 0.12f),
    turquoise = Turquoise,
    orange = Orange,
    error = Color(0xFFEF4444),
    capsule = Brush.linearGradient(listOf(Color(0xFF5B8CFF), Turquoise, Color(0xFF86EFD0), Orange)),
)

fun grayColors(glass: Float) = darkColors(glass).copy(
    bg = Color(0xFF1C1C1E),
    bgSoft = Color(0xFF232326),
    surface = Color(0xFF2C2C2E),
    surface2 = Color(0xFF3A3A3C),
)

fun lightColors(glass: Float) = MsColors(
    bg = Color(0xFFEEF0F3),
    bgSoft = Color(0xFFE4E7EB),
    surface = Color(0xFFFAFBFC),
    surface2 = Color(0xFFF3F4F6),
    text = Color(0xFF141418),
    textDim = Color(0xFF5C5F6A),
    glassFill = Color.White.copy(alpha = 0.55f * (glass / 0.5f).coerceIn(0.2f, 1.6f)),
    hairline = Color(0x140F172A),
    turquoise = Color(0xFF0D9488),
    orange = Color(0xFFEA580C),
    error = Color(0xFFDC2626),
    capsule = Brush.linearGradient(listOf(Color(0xFF5B8CFF), Color(0xFF0D9488), Orange)),
)

val DisplayLarge = TextStyle(
    fontFamily = FontFamily.SansSerif,
    fontWeight = FontWeight.Bold,
    fontSize = 32.sp,
    letterSpacing = (-0.8).sp,
    lineHeight = 38.sp,
)

val TitleSmall = TextStyle(
    fontFamily = FontFamily.SansSerif,
    fontWeight = FontWeight.Medium,
    fontSize = 13.sp,
    letterSpacing = 0.2.sp,
)

@Composable
fun MaxSpeechTheme(
    theme: UiTheme,
    glassAlpha: Float,
    content: @Composable () -> Unit,
) {
    val colors = when (theme) {
        UiTheme.Dark -> darkColors(glassAlpha)
        UiTheme.Gray -> grayColors(glassAlpha)
        UiTheme.Light -> lightColors(glassAlpha)
    }
    val scheme = if (theme == UiTheme.Light) {
        lightColorScheme(
            background = colors.bg,
            surface = colors.surface,
            primary = colors.turquoise,
            onPrimary = Color.White,
            onBackground = colors.text,
            onSurface = colors.text,
        )
    } else {
        darkColorScheme(
            background = colors.bg,
            surface = colors.surface,
            primary = colors.turquoise,
            onPrimary = Color.Black,
            onBackground = colors.text,
            onSurface = colors.text,
        )
    }
    CompositionLocalProvider(
        LocalMsColors provides colors,
        LocalGlassAlpha provides glassAlpha,
    ) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
