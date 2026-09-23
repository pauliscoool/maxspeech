package com.maxspeech.android.ui.style

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import coil.ImageLoader
import coil.compose.AsyncImage
import coil.decode.SvgDecoder
import coil.request.ImageRequest
import com.maxspeech.android.data.BrandMark
import com.maxspeech.android.data.StyleGroups
import kotlin.math.cos
import kotlin.math.sin

/**
 * Slow orbiting brand marks using real Simple Icons SVGs (not random favicons /
 * default Android package icons).
 */
@Composable
fun LogoSwirl(
    brands: List<BrandMark>,
    modifier: Modifier = Modifier,
    size: Dp = 168.dp,
    logoSize: Dp = 40.dp,
) {
    val context = LocalContext.current
    val density = LocalDensity.current
    val imageLoader = remember {
        ImageLoader.Builder(context)
            .components { add(SvgDecoder.Factory()) }
            .build()
    }
    val spin by rememberInfiniteTransition(label = "swirl").animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(
            animation = tween(28_000, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "swirlAngle",
    )
    val breathe by rememberInfiniteTransition(label = "breathe").animateFloat(
        initialValue = 0.96f,
        targetValue = 1.04f,
        animationSpec = infiniteRepeatable(
            animation = tween(3_600, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "swirlScale",
    )
    val radiusPx = with(density) { (size / 2f - logoSize / 2f - 4.dp).toPx() }

    Box(
        modifier
            .size(size)
            .scale(breathe),
        contentAlignment = Alignment.Center,
    ) {
        brands.forEachIndexed { index, brand ->
            val angle = Math.toRadians((spin + index * (360f / brands.size.coerceAtLeast(1))).toDouble())
            val x = (cos(angle) * radiusPx).toFloat()
            val y = (sin(angle) * radiusPx * 0.72f).toFloat()
            val depth = ((sin(angle) + 1.0) / 2.0).toFloat()
            Box(
                Modifier
                    .offset(
                        x = with(density) { x.toDp() },
                        y = with(density) { y.toDp() },
                    )
                    .size(logoSize * (0.82f + depth * 0.28f))
                    .alpha(0.62f + depth * 0.38f)
                    .clip(CircleShape)
                    .background(Color.White)
                    .border(1.5.dp, Color.White.copy(alpha = 0.35f), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                val bundled = painterResource(brand.logoRes)
                AsyncImage(
                    model = ImageRequest.Builder(context)
                        .data(StyleGroups.logoData(brand))
                        .crossfade(true)
                        .build(),
                    contentDescription = brand.label,
                    imageLoader = imageLoader,
                    placeholder = bundled,
                    error = bundled,
                    fallback = bundled,
                    contentScale = ContentScale.Fit,
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(5.dp),
                )
            }
        }
    }
}
