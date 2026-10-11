package com.maxspeech.android.ui.home

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.StyleGroupId
import com.maxspeech.android.data.StyleGroups
import com.maxspeech.android.ui.style.LogoSwirl
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun HomeStyleHero(
    settings: AppSettings,
    onSelectGroup: (StyleGroupId) -> Unit,
    onEdit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = LocalMsColors.current
    val current = StyleGroupId.parse(settings.homeStyleGroup)

    Box(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(c.surface)
            .border(1.dp, c.hairline, RoundedCornerShape(20.dp))
            .padding(16.dp),
    ) {
        Column(Modifier.fillMaxWidth()) {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                StyleGroupId.entries.forEach { id ->
                    val on = id == current
                    Text(
                        id.title,
                        color = if (on) Turquoise else c.textDim,
                        fontSize = 12.sp,
                        fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium,
                        modifier = Modifier
                            .clip(RoundedCornerShape(20.dp))
                            .background(if (on) Turquoise.copy(alpha = 0.16f) else Color.Transparent)
                            .clickable { onSelectGroup(id) }
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                    )
                }
            }
            Spacer(Modifier.height(12.dp))
            AnimatedContent(
                targetState = current,
                transitionSpec = {
                    val forward = targetState.ordinal >= initialState.ordinal
                    val enterSlide = if (forward) {
                        slideInHorizontally(tween(420, easing = FastOutSlowInEasing)) { it / 3 }
                    } else {
                        slideInHorizontally(tween(420, easing = FastOutSlowInEasing)) { -it / 3 }
                    }
                    val exitSlide = if (forward) {
                        slideOutHorizontally(tween(340, easing = FastOutSlowInEasing)) { -it / 3 }
                    } else {
                        slideOutHorizontally(tween(340, easing = FastOutSlowInEasing)) { it / 3 }
                    }
                    (fadeIn(tween(380, easing = FastOutSlowInEasing)) + enterSlide) togetherWith
                        (fadeOut(tween(300, easing = FastOutSlowInEasing)) + exitSlide)
                },
                label = "homeStyleGroup",
                modifier = Modifier.fillMaxWidth(),
            ) { group ->
                GroupPanel(group = group, settings = settings)
            }
        }
        Box(
            Modifier
                .align(Alignment.BottomEnd)
                .size(36.dp)
                .clip(CircleShape)
                .background(c.surface2)
                .clickable(onClick = onEdit),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.Edit,
                contentDescription = "Edit style groups",
                tint = c.text,
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

@Composable
private fun GroupPanel(group: StyleGroupId, settings: AppSettings) {
    val c = LocalMsColors.current
    val def = StyleGroups.def(group)
    val tone = when (group) {
        StyleGroupId.Messaging -> settings.styleToneMessaging
        StyleGroupId.Email -> settings.styleToneEmail
        StyleGroupId.Work -> settings.styleToneWork
        StyleGroupId.Social -> settings.styleToneSocial
    }
    Column(Modifier.fillMaxWidth().padding(end = 44.dp)) {
        Text(
            "${group.title} · ${StyleGroups.labelForTone(tone)}",
            color = c.text,
            fontSize = 16.sp,
            fontWeight = FontWeight.SemiBold,
        )
        Spacer(Modifier.height(2.dp))
        Text(
            group.subtitle + "  ·  " + def.brands.take(4).joinToString(", ") { it.label },
            color = c.textDim,
            fontSize = 12.sp,
            maxLines = 2,
        )
    }
}
