package com.maxspeech.android.ui.settings

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.BuildConfig
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthUser
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.SttLanguages
import com.maxspeech.android.data.UiTheme
import com.maxspeech.android.ui.components.GlassChip
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall
import com.maxspeech.android.ui.theme.Turquoise
import kotlin.math.roundToInt

@OptIn(ExperimentalLayoutApi::class)
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
    onLanguage: (String) -> Unit,
    onSignOut: () -> Unit,
    onOpenOverlaySettings: () -> Unit,
    onOpenA11ySettings: () -> Unit,
    onOpenAppInfo: () -> Unit,
    signingOut: Boolean = false,
) {
    val c = LocalMsColors.current
    val ctx = LocalContext.current
    val multiOk = SttLanguages.multilingualAllowed(plan.tier)
    val multiOn = settings.multilingual && multiOk
    fun open(url: String) {
        ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
    }
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(top = 12.dp, bottom = 120.dp),
    ) {
        Text("Settings", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(18.dp))

        Section("Account")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text(user?.username.orEmpty(), color = c.text, fontSize = 16.sp)
                Text(user?.email.orEmpty(), color = c.textDim, fontSize = 13.sp)
                Text(
                    "${plan.tier.label} · ${plan.wordsUsed} / ${plan.weeklyLimit} words this week",
                    color = c.turquoise,
                    fontSize = 13.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
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
            }
        }

        Section("Theme")
        GlassSurface(Modifier.fillMaxWidth()) {
            Row(
                Modifier.padding(12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                ThemeCard("Dark", "Black glass", UiTheme.Dark, settings.theme, Modifier.weight(1f), onTheme)
                ThemeCard("Gray", "Soft charcoal", UiTheme.Gray, settings.theme, Modifier.weight(1f), onTheme)
                ThemeCard("Light", "Daylight", UiTheme.Light, settings.theme, Modifier.weight(1f), onTheme)
            }
        }

        Section("Overlay")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                ToggleRow("Floating capsule", "Pop up over the app you’re typing in", overlayOn) {
                    onToggle("overlay", it)
                    if (it) onOpenOverlaySettings()
                }
                ToggleRow("Confirm before paste", "Review text before it goes into the field", settings.overlayConfirm) {
                    onToggle("confirm", it)
                }
                Text(
                    if (overlayOn) "Overlay is on — the capsule appears over chat fields."
                    else "Turn on “Display over other apps” so the capsule can pop up over WhatsApp, Gmail, Messages.",
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
                if (!overlayOn) {
                    Text(
                        "Continue — allow overlay",
                        color = c.turquoise,
                        modifier = Modifier.padding(top = 8.dp).clickable(onClick = onOpenOverlaySettings),
                    )
                }
                Text(
                    if (a11yOn) "Accessibility is on — paste goes into the focused app."
                    else "Sideloaded apps hide Accessibility until you unlock it:\n" +
                        "1. Settings → search Apps → MaxSpeech (search or scroll).\n" +
                        "2. Top-right ⋮ → Allow restricted settings. Don’t tap Clear cache.\n" +
                        "3. Open Accessibility, find MaxSpeech, turn it on, then Proceed.\n" +
                        "Turn off Wispr Flow / Whisper Flow Accessibility while using MaxSpeech — two overlays will fight and the app can close.",
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
                if (!a11yOn) {
                    Text(
                        "1. Open app info",
                        color = c.turquoise,
                        modifier = Modifier.padding(top = 8.dp).clickable(onClick = onOpenAppInfo),
                    )
                }
                Text(
                    if (a11yOn) "Accessibility settings" else "2. Open Accessibility",
                    color = c.turquoise,
                    modifier = Modifier.padding(top = 8.dp).clickable(onClick = onOpenA11ySettings),
                )
            }
        }

        Section("Dictation")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                ToggleRow("Show live transcript", "Show words on the capsule while speaking", settings.liveTranscript) {
                    onToggle("live", it)
                }
                ToggleRow("AI enhance", "Clean grammar, fillers, and self-corrections", settings.aiEnhance) {
                    onToggle("enhance", it)
                }
                ToggleRow("Trailing space", "Add a space after each insertion so you can keep typing", settings.trailingSpace) {
                    onToggle("space", it)
                }
                ToggleRow("Start / stop sound", "Chime when dictation starts and stops", settings.soundCue) {
                    onToggle("sound", it)
                }
                ToggleRow("Haptics", "Light vibration when a session starts", settings.haptics) {
                    onToggle("haptics", it)
                }
                ToggleRow(
                    "Multilingual",
                    if (multiOk) "Code-switch between languages in one dictation (pick up to 5). Off = one language."
                    else "Starter+ only. Free plan can use one language at a time.",
                    multiOn,
                    enabled = multiOk,
                ) { onToggle("multi", it) }
                Text(
                    if (multiOn) "Languages (up to 5)" else "Language",
                    color = c.text,
                    modifier = Modifier.padding(top = 12.dp),
                )
                Text(
                    if (multiOn) "Code-switch mid-sentence. For clearest English-only dictation, turn Multilingual off and select English."
                    else "Speech recognition language for live dictation.",
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 4.dp, bottom = 8.dp),
                )
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    SttLanguages.ALL.forEach { lang ->
                        val active = settings.languages.contains(lang.code)
                        val locked = multiOn && !active && settings.languages.size >= SttLanguages.MAX
                        GlassChip(lang.label, selected = active) {
                            if (!locked) onLanguage(lang.code)
                        }
                    }
                }
                Row(
                    Modifier.fillMaxWidth().padding(top = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("Max recording", color = c.text)
                        Text("Hard cap: 2 minutes wall-clock per session", color = c.textDim, fontSize = 12.sp)
                    }
                    Text("2 min", color = c.turquoise, fontSize = 14.sp)
                }
            }
        }

        Section("About")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(8.dp)) {
                AboutRow("Version", BuildConfig.VERSION_NAME)
                AboutRow("Website", "maxspeech.vercel.app") { open("https://maxspeech.vercel.app") }
                AboutRow("Android install", "Sideload guide") { open("https://maxspeech.vercel.app/android") }
                AboutRow("Terms", "Terms of Service") { open("https://maxspeech.vercel.app/terms.html") }
                AboutRow("Privacy", "Privacy Policy") { open("https://maxspeech.vercel.app/privacy.html") }
                AboutRow("GitHub", "pauliscoool/maxspeech") { open("https://github.com/pauliscoool/maxspeech") }
            }
        }

        Section("Danger zone")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text("Log out", color = c.text)
                Text(
                    "Signs you out of MaxSpeech cloud and returns you to sign in.",
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 4.dp, bottom = 12.dp),
                )
                GlassSurface(
                    Modifier
                        .fillMaxWidth()
                        .clickable(enabled = !signingOut, onClick = onSignOut),
                    shape = RoundedCornerShape(22.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        if (signingOut) "Signing out…" else "Log out",
                        color = c.error,
                        modifier = Modifier.padding(vertical = 12.dp),
                    )
                }
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun ThemeCard(
    title: String,
    desc: String,
    value: UiTheme,
    selected: UiTheme,
    modifier: Modifier,
    onTheme: (UiTheme) -> Unit,
) {
    val c = LocalMsColors.current
    val on = selected == value
    Column(
        modifier
            .background(if (on) c.turquoise.copy(alpha = 0.16f) else c.bgSoft, RoundedCornerShape(16.dp))
            .clickable { onTheme(value) }
            .padding(12.dp),
    ) {
        Text(title, color = if (on) c.turquoise else c.text, fontSize = 14.sp)
        Text(desc, color = c.textDim, fontSize = 11.sp, modifier = Modifier.padding(top = 4.dp))
    }
}

@Composable
private fun AboutRow(title: String, subtitle: String, onClick: (() -> Unit)? = null) {
    val c = LocalMsColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, color = c.text, fontSize = 15.sp)
            Text(subtitle, color = c.textDim, fontSize = 12.sp)
        }
        if (onClick != null) Text("Open", color = c.turquoise, fontSize = 13.sp)
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
private fun ToggleRow(
    title: String,
    desc: String,
    checked: Boolean,
    enabled: Boolean = true,
    onChange: (Boolean) -> Unit,
) {
    val c = LocalMsColors.current
    Row(
        Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f).padding(end = 12.dp)) {
            Text(title, color = c.text)
            Text(desc, color = c.textDim, fontSize = 12.sp, modifier = Modifier.padding(top = 2.dp))
        }
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            enabled = enabled,
            colors = SwitchDefaults.colors(checkedTrackColor = Turquoise),
        )
    }
}
