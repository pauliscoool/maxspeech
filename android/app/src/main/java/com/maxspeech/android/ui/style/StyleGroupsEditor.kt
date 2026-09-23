package com.maxspeech.android.ui.style

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.StyleGroupId
import com.maxspeech.android.data.StyleGroups
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun StyleGroupsEditor(
    messaging: String,
    email: String,
    work: String,
    social: String,
    onChange: (messaging: String, email: String, work: String, social: String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val c = LocalMsColors.current
    val tones = remember {
        mutableStateMapOf(
            StyleGroupId.Messaging to messaging,
            StyleGroupId.Email to email,
            StyleGroupId.Work to work,
            StyleGroupId.Social to social,
        )
    }

    fun emit() {
        onChange(
            tones[StyleGroupId.Messaging] ?: "casual",
            tones[StyleGroupId.Email] ?: "formal",
            tones[StyleGroupId.Work] ?: "default",
            tones[StyleGroupId.Social] ?: "casual",
        )
    }

    Column(modifier, verticalArrangement = Arrangement.spacedBy(if (compact) 14.dp else 22.dp)) {
        StyleGroups.all.forEach { group ->
            val tone = tones[group.id] ?: "default"
            var slider by remember(group.id, tone) {
                mutableFloatStateOf(StyleGroups.sliderFromTone(tone))
            }
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(22.dp))
                    .background(c.bgSoft)
                    .padding(if (compact) 14.dp else 16.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    LogoSwirl(
                        brands = group.brands,
                        size = if (compact) 108.dp else 148.dp,
                        logoSize = if (compact) 28.dp else 36.dp,
                    )
                    Spacer(Modifier.padding(horizontal = 8.dp))
                    Column(Modifier.weight(1f)) {
                        Text(group.id.title, color = c.text, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                        Text(group.id.subtitle, color = c.textDim, fontSize = 12.sp)
                        Spacer(Modifier.height(8.dp))
                        Text(
                            StyleGroups.labelForTone(tone),
                            color = Turquoise,
                            fontSize = 14.sp,
                            fontWeight = FontWeight.Medium,
                        )
                    }
                }
                Spacer(Modifier.height(8.dp))
                Slider(
                    value = slider,
                    onValueChange = {
                        slider = it
                        tones[group.id] = StyleGroups.toneFromSlider(it)
                        emit()
                    },
                    valueRange = 0f..(StyleGroups.tones.lastIndex.toFloat()),
                    steps = StyleGroups.tones.size - 2,
                    colors = SliderDefaults.colors(
                        thumbColor = Turquoise,
                        activeTrackColor = Turquoise,
                        inactiveTrackColor = c.textDim.copy(alpha = 0.25f),
                    ),
                )
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    StyleGroups.tones.forEach { t ->
                        Text(
                            StyleGroups.labelForTone(t).take(3),
                            color = if (t == tone) Turquoise else c.textDim,
                            fontSize = 10.sp,
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun StyleGroupsSetupPane(
    messaging: String,
    email: String,
    work: String,
    social: String,
    onChange: (String, String, String, String) -> Unit,
    title: String = "How should MaxSpeech sound?",
    body: String = "Pick a voice for each world you type in. Logos drift while you decide — you can edit anytime.",
) {
    val c = LocalMsColors.current
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(title, color = c.text, fontSize = 24.sp, fontWeight = FontWeight.SemiBold)
        Spacer(Modifier.height(8.dp))
        Text(body, color = c.textDim, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 8.dp))
        Spacer(Modifier.height(20.dp))
        StyleGroupsEditor(
            messaging = messaging,
            email = email,
            work = work,
            social = social,
            onChange = onChange,
            compact = true,
        )
    }
}
