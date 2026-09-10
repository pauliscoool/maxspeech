package com.maxspeech.android.ui.settings

import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthUser
import com.maxspeech.android.data.EnhanceSpeed
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.UiTheme
import com.maxspeech.android.ui.components.GlassChip
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall
import com.maxspeech.android.ui.theme.Turquoise
import kotlin.math.roundToInt

private val LANGS = listOf(
    "en" to "English", "es" to "Spanish", "fr" to "French", "de" to "German",
    "pt" to "Portuguese", "ja" to "Japanese", "ko" to "Korean", "zh" to "Chinese",
    "hi" to "Hindi", "ar" to "Arabic", "it" to "Italian", "ru" to "Russian",
)

@Composable
fun SettingsScreen(
    settings: AppSettings,
    user: AuthUser?,
    plan: PlanStatus,
    a11yOn: Boolean,
    overlayOn: Boolean,
    onGlass: (Float) -> Unit,
    onBlur: (Float) -> Unit,
    onTheme: (UiTheme) -> Unit,
    onToggle: (String, Boolean) -> Unit,
    onSpeed: (EnhanceSpeed) -> Unit,
    onLanguage: (String) -> Unit,
    onLlmKey: (String) -> Unit,
    onSignOut: () -> Unit,
    onOpenOverlaySettings: () -> Unit,
    onOpenA11ySettings: () -> Unit,
) {
    val c = LocalMsColors.current
    val ctx = LocalContext.current
    var llm by remember(settings.llmKey) { mutableStateOf(settings.llmKey) }
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
    ) {
        Text("Settings", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(18.dp))

        Section("Account")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text(user?.username ?: "Local", color = c.text, fontSize = 16.sp)
                Text(
                    user?.email ?: "Continue locally",
                    color = c.textDim,
                    fontSize = 13.sp,
                )
                Text(
                    "${plan.tier.label} · ${plan.wordsUsed} / ${plan.weeklyLimit} words this week",
                    color = c.turquoise,
                    fontSize = 13.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
                if (user != null && !user.local) {
                    TextButton(onClick = onSignOut) { Text("Sign out", color = c.error) }
                }
            }
        }

        Section("Look")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Transparency", color = c.text)
                Text("${(settings.glassAlpha * 100).roundToInt()}% glass", color = c.textDim, fontSize = 12.sp)
                Slider(
                    value = settings.glassAlpha,
                    onValueChange = onGlass,
                    colors = SliderDefaults.colors(thumbColor = Turquoise, activeTrackColor = Turquoise),
                )
                Text("Blur", color = c.text)
                Slider(
                    value = settings.blurStrength,
                    onValueChange = onBlur,
                    colors = SliderDefaults.colors(thumbColor = Turquoise, activeTrackColor = Turquoise),
                )
                Text("Theme", color = c.text)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    UiTheme.entries.forEach { t ->
                        GlassChip(t.name, selected = settings.theme == t) { onTheme(t) }
                    }
                }
            }
        }

        Section("Overlay")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                ToggleRow("Floating capsule", overlayOn) { onToggle("overlay", it); if (it) onOpenOverlaySettings() }
                ToggleRow("Confirm before paste", settings.overlayConfirm) { onToggle("confirm", it) }
                Text(
                    if (a11yOn) "Accessibility is on — paste goes into the focused app."
                    else "Turn on Accessibility so MaxSpeech can paste into other apps.",
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
                TextButton(onClick = onOpenA11ySettings) { Text("Open Accessibility settings", color = c.turquoise) }
                TextButton(onClick = {
                    ctx.startActivity(
                        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
                            data = Uri.parse("package:${ctx.packageName}")
                        },
                    )
                }) { Text("Unrestricted battery", color = c.turquoise) }
            }
        }

        Section("Dictation")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                ToggleRow("AI enhance", settings.aiEnhance) { onToggle("enhance", it) }
                Text("Enhance speed", color = c.text, modifier = Modifier.padding(top = 8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(vertical = 8.dp)) {
                    EnhanceSpeed.entries.forEach { s ->
                        GlassChip(s.name, selected = settings.enhanceSpeed == s) { onSpeed(s) }
                    }
                }
                ToggleRow("Multilingual", settings.multilingual) { onToggle("multi", it) }
                ToggleRow("Trailing space", settings.trailingSpace) { onToggle("space", it) }
                ToggleRow("Haptics", settings.haptics) { onToggle("haptics", it) }
                ToggleRow("Sound cue", settings.soundCue) { onToggle("sound", it) }
                ToggleRow("Live transcript", settings.liveTranscript) { onToggle("live", it) }
                Text("Languages", color = c.textDim, fontSize = 12.sp, modifier = Modifier.padding(top = 8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                    LANGS.take(6).forEach { (code, label) ->
                        GlassChip(label, selected = settings.languages.contains(code)) { onLanguage(code) }
                    }
                }
            }
        }

        Section("API keys")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text("Enhancement key (OpenAI)", color = c.text)
                Text("Optional. Without it, MaxSpeech still transcribes — it just won’t rewrite tone.", color = c.textDim, fontSize = 12.sp)
                Spacer(Modifier.height(8.dp))
                BasicTextField(
                    value = llm,
                    onValueChange = { llm = it; onLlmKey(it) },
                    singleLine = true,
                    cursorBrush = SolidColor(c.turquoise),
                    textStyle = androidx.compose.ui.text.TextStyle(color = c.text, fontSize = 14.sp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(c.bgSoft, RoundedCornerShape(12.dp))
                        .padding(12.dp),
                    decorationBox = { inner ->
                        if (llm.isEmpty()) Text("sk-…", color = c.textDim, fontSize = 14.sp)
                        inner()
                    },
                )
            }
        }

        Section("About")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text("MaxSpeech for Android 0.1.0", color = c.text)
                Text("Cloud STT · APK under 100 MB · maxspeech.vercel.app/android", color = c.textDim, fontSize = 12.sp)
            }
        }
        Spacer(Modifier.height(96.dp))
    }
}

@Composable
private fun Section(title: String) {
    Text(
        title,
        style = TitleSmall,
        color = LocalMsColors.current.textDim,
        modifier = Modifier.padding(top = 22.dp, bottom = 8.dp),
    )
}

@Composable
private fun ToggleRow(title: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    val c = LocalMsColors.current
    Row(
        Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, color = c.text, modifier = Modifier.weight(1f))
        Switch(checked, onChange, colors = SwitchDefaults.colors(checkedTrackColor = Turquoise))
    }
}
