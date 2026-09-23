package com.maxspeech.android.ui.settings

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.outlined.AccessibilityNew
import androidx.compose.material.icons.outlined.BatteryChargingFull
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material.icons.outlined.RestartAlt
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthUser
import com.maxspeech.android.data.EnhanceSpeed
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.SttLanguages
import com.maxspeech.android.data.UiTheme
import com.maxspeech.android.ui.components.GlassChip
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.TitleSmall
import com.maxspeech.android.ui.theme.Turquoise

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SettingsScreen(
    settings: AppSettings,
    user: AuthUser?,
    plan: PlanStatus,
    a11yOn: Boolean,
    overlayOn: Boolean,
    onTheme: (UiTheme) -> Unit,
    onEnhanceSpeed: (EnhanceSpeed) -> Unit,
    onResetMicPosition: () -> Unit,
    onToggle: (String, Boolean) -> Unit,
    onLanguage: (String) -> Unit,
    onSignOut: () -> Unit,
    onOpenOverlaySettings: () -> Unit,
    onOpenA11ySettings: () -> Unit,
    onOpenMicLook: () -> Unit,
    signingOut: Boolean = false,
) {
    val c = LocalMsColors.current
    val ctx = LocalContext.current
    val multiOk = SttLanguages.multilingualAllowed(plan.tier)
    val multiOn = multiOk && settings.multilingual
    val danger = c.error
    val setupReady = overlayOn && a11yOn
    fun openBattery() {
        runCatching {
            ctx.startActivity(
                Intent(android.provider.Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS),
            )
        }
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

        // 1 — Account
        Section("Account")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text(user?.username.orEmpty(), color = c.text, fontSize = 16.sp, fontWeight = FontWeight.Medium)
                Text(user?.email.orEmpty(), color = c.textDim, fontSize = 13.sp)
                Text(
                    "${plan.tier.label} · ${plan.wordsUsed} / ${plan.weeklyLimit} words this week",
                    color = c.turquoise,
                    fontSize = 13.sp,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }
        }

        // 2 — Theme
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

        // 3 — Permissions (highest functional priority)
        Section("Permissions")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    if (setupReady) {
                        "Ready to dictate in other apps."
                    } else {
                        "Turn these on so MaxSpeech can show the mic and paste."
                    },
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 4.dp),
                )
                SettingsActionButton(
                    icon = Icons.Outlined.Layers,
                    title = "Display over other apps",
                    subtitle = if (overlayOn) "Allowed — floating mic can appear" else "Required — tap to allow overlay",
                    status = if (overlayOn) ActionStatus.Ready else ActionStatus.Needed,
                    onClick = onOpenOverlaySettings,
                )
                SettingsActionButton(
                    icon = Icons.Outlined.AccessibilityNew,
                    title = "Accessibility",
                    subtitle = if (a11yOn) {
                        "On — paste into other apps works"
                    } else {
                        "Required — enable MaxSpeech in Accessibility"
                    },
                    status = if (a11yOn) ActionStatus.Ready else ActionStatus.Needed,
                    onClick = onOpenA11ySettings,
                )
                SettingsActionButton(
                    icon = Icons.Outlined.BatteryChargingFull,
                    title = "Battery optimization",
                    subtitle = "Keep MaxSpeech alive in the background",
                    status = ActionStatus.Neutral,
                    onClick = ::openBattery,
                )
                if (!a11yOn) {
                    Text(
                        "If Accessibility is locked for sideloaded apps: Settings → Apps → MaxSpeech → ⋮ → " +
                            "Allow restricted settings. Don’t Clear cache or data.",
                        color = c.textDim,
                        fontSize = 11.sp,
                        lineHeight = 15.sp,
                        modifier = Modifier.padding(start = 6.dp, end = 6.dp, top = 4.dp, bottom = 2.dp),
                    )
                }
            }
        }

        // 4 — Floating mic
        Section("Floating mic")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                ToggleRow(
                    "Floating capsule",
                    "Show the mic bubble when a text field is focused.",
                    overlayOn,
                ) {
                    onToggle("overlay", it)
                    if (it) onOpenOverlaySettings()
                }
                ToggleRow(
                    "Confirm before paste",
                    "Review text before it goes into the field",
                    settings.overlayConfirm,
                ) {
                    onToggle("confirm", it)
                }
                Spacer(Modifier.height(4.dp))
                SettingsActionButton(
                    icon = Icons.Outlined.Palette,
                    title = "Mic look",
                    subtitle = "Size, transparency, wave, color, style",
                    status = ActionStatus.Neutral,
                    onClick = onOpenMicLook,
                )
                SettingsActionButton(
                    icon = Icons.Outlined.RestartAlt,
                    title = "Reset mic position",
                    subtitle = "Snap the floating bubble back to default",
                    status = ActionStatus.Neutral,
                    onClick = onResetMicPosition,
                )
            }
        }

        // 5 — Dictation
        Section("Dictation")
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                ToggleRow(
                    "AI enhance",
                    "Clean grammar, fillers, and self-corrections",
                    settings.aiEnhance,
                ) { onToggle("enhance", it) }
                if (settings.aiEnhance) {
                    Text("Enhance speed", color = c.text, modifier = Modifier.padding(top = 10.dp))
                    Text(
                        "Fast = quicker paste. Thinking / Ultra take longer for cleaner prose.",
                        color = c.textDim,
                        fontSize = 12.sp,
                        modifier = Modifier.padding(top = 4.dp, bottom = 8.dp),
                    )
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        EnhanceSpeed.entries.forEach { speed ->
                            GlassChip(
                                label = speed.name,
                                selected = settings.enhanceSpeed == speed,
                            ) { onEnhanceSpeed(speed) }
                        }
                    }
                }
                ToggleRow(
                    "Show live transcript",
                    "Show words on the capsule while speaking",
                    settings.liveTranscript,
                ) { onToggle("live", it) }
                ToggleRow(
                    "Trailing space",
                    "Add a space after each insertion so you can keep typing",
                    settings.trailingSpace,
                ) { onToggle("space", it) }
                ToggleRow(
                    "Start / stop sound",
                    "Chime when dictation starts and stops",
                    settings.soundCue,
                ) { onToggle("sound", it) }
                ToggleRow(
                    "Haptics",
                    "Light vibration when a session starts",
                    settings.haptics,
                ) { onToggle("haptics", it) }
                ToggleRow(
                    "Multilingual",
                    if (multiOk) {
                        "Code-switch between languages in one dictation (pick up to 5). Off = one language."
                    } else {
                        "Starter+ only. Free plan can use one language at a time."
                    },
                    multiOn,
                    enabled = multiOk,
                ) { onToggle("multi", it) }
                Text(
                    if (multiOn) "Languages (up to 5)" else "Language",
                    color = c.text,
                    modifier = Modifier.padding(top = 12.dp),
                )
                Text(
                    if (multiOn) {
                        "Code-switch mid-sentence. For clearest English-only dictation, turn Multilingual off and select English."
                    } else {
                        "Speech recognition language for live dictation."
                    },
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 4.dp, bottom = 8.dp),
                )
                LanguagePicker(
                    selectedCodes = settings.languages,
                    multilingual = multiOn,
                    onToggle = onLanguage,
                )
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

        // 6 — Danger zone last
        Text(
            "Danger zone",
            style = TitleSmall,
            color = danger,
            modifier = Modifier.padding(top = 28.dp, bottom = 8.dp),
        )
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(22.dp))
                .background(danger.copy(alpha = 0.08f))
                .border(1.dp, danger.copy(alpha = 0.35f), RoundedCornerShape(22.dp))
                .padding(16.dp),
        ) {
            Text("Log out", color = c.text, fontWeight = FontWeight.Medium)
            Text(
                "Signs you out of MaxSpeech cloud and returns you to sign in.",
                color = c.textDim,
                fontSize = 12.sp,
                modifier = Modifier.padding(top = 4.dp, bottom = 14.dp),
            )
            Box(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(28.dp))
                    .background(danger)
                    .clickable(enabled = !signingOut, onClick = onSignOut)
                    .padding(vertical = 13.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    if (signingOut) "Signing out…" else "Log out",
                    color = Color.White,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 15.sp,
                )
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}

private enum class ActionStatus { Ready, Needed, Neutral }

@Composable
private fun SettingsActionButton(
    icon: ImageVector,
    title: String,
    subtitle: String,
    status: ActionStatus,
    onClick: () -> Unit,
) {
    val c = LocalMsColors.current
    val statusTint = when (status) {
        ActionStatus.Ready -> Turquoise
        ActionStatus.Needed -> Orange
        ActionStatus.Neutral -> c.textDim
    }
    val shape = RoundedCornerShape(16.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.bgSoft)
            .border(1.dp, c.hairline, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            Modifier
                .size(42.dp)
                .clip(CircleShape)
                .background(c.surface),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                icon,
                contentDescription = null,
                tint = when (status) {
                    ActionStatus.Ready -> Turquoise
                    ActionStatus.Needed -> Orange
                    ActionStatus.Neutral -> c.text
                },
                modifier = Modifier.size(22.dp),
            )
        }
        Column(Modifier.weight(1f)) {
            Text(title, color = c.text, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            Text(
                subtitle,
                color = c.textDim,
                fontSize = 12.sp,
                lineHeight = 15.sp,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
        when (status) {
            ActionStatus.Ready -> {
                Text(
                    "On",
                    color = Turquoise,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
            ActionStatus.Needed -> {
                Text(
                    "Set up",
                    color = Orange,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
            ActionStatus.Neutral -> Unit
        }
        Icon(
            Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            tint = statusTint,
            modifier = Modifier.size(22.dp),
        )
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
        Modifier.fillMaxWidth().padding(vertical = 6.dp, horizontal = 6.dp),
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
