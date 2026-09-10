package com.maxspeech.android.ui

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.GraphicEq
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material3.Icon
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.overlay.OverlayService
import com.maxspeech.android.ui.history.HistoryScreen
import com.maxspeech.android.ui.home.HomeScreen
import com.maxspeech.android.ui.onboarding.OnboardingScreen
import com.maxspeech.android.ui.settings.SettingsScreen
import com.maxspeech.android.ui.style.StyleScreen
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.MaxSpeechTheme
import com.maxspeech.android.ui.theme.Turquoise
import android.provider.Settings as AndroidSettings

enum class Tab { Home, History, Dictate, Style, Settings }

@Composable
fun MaxSpeechRoot(vm: AppViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    MaxSpeechTheme(state.settings.theme, state.settings.glassAlpha) {
        val ctx = LocalContext.current
        var tab by remember { mutableStateOf(Tab.Home) }
        var micOk by remember { mutableStateOf(false) }
        var overlayOk by remember {
            mutableStateOf(AndroidSettings.canDrawOverlays(ctx))
        }
        val snack = remember { SnackbarHostState() }
        val micLauncher = rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission(),
        ) { granted -> micOk = granted }
        val notifLauncher = rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission(),
        ) { }

        LaunchedEffect(Unit) {
            if (Build.VERSION.SDK_INT >= 33) {
                notifLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }

        LaunchedEffect(state.settings.overlayEnabled, overlayOk) {
            overlayOk = AndroidSettings.canDrawOverlays(ctx)
            val intent = Intent(ctx, OverlayService::class.java)
            if (state.settings.overlayEnabled && overlayOk && state.settings.onboarded) {
                ctx.startForegroundService(intent)
            } else {
                ctx.stopService(intent)
            }
        }
        LaunchedEffect(state.toast) {
            state.toast?.let { snack.showSnackbar(it) }
        }

        val showOnboarding = !state.settings.onboarded
        Box(Modifier.fillMaxSize()) {
            if (showOnboarding) {
                OnboardingScreen(
                    busy = state.authBusy,
                    error = state.authError,
                    info = state.authInfo,
                    authed = state.user != null,
                    onSignIn = { e, p -> vm.signIn(e, p) },
                    onSignUp = { e, p, u -> vm.signUp(e, p, u) },
                    onLocal = { vm.continueLocal() },
                    onMic = { micLauncher.launch(Manifest.permission.RECORD_AUDIO) },
                    onOverlay = {
                        ctx.startActivity(
                            Intent(
                                AndroidSettings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                Uri.parse("package:${ctx.packageName}"),
                            ),
                        )
                    },
                    onA11y = {
                        ctx.startActivity(Intent(AndroidSettings.ACTION_ACCESSIBILITY_SETTINGS))
                    },
                    onDone = { vm.finishOnboarding() },
                    micGranted = micOk,
                    overlayGranted = overlayOk,
                    a11yGranted = TextInjector.isAccessibilityOn(),
                )
            } else {
                Column(Modifier.fillMaxSize().background(LocalMsColors.current.bg)) {
                    Box(Modifier.weight(1f)) {
                        when (tab) {
                            Tab.Home, Tab.Dictate -> HomeScreen(
                                name = state.user?.username ?: "there",
                                history = state.history,
                                wordsUsed = state.plan.wordsUsed,
                                wordsLimit = state.plan.weeklyLimit,
                                appCount = state.appCount,
                                chips = listOf("Casual", "Email", "WhatsApp"),
                                selectedChip = state.settings.toneOverride.replaceFirstChar { it.uppercase() }
                                    .let { if (it == "Formal") "Email" else it },
                                onChip = { chip ->
                                    vm.setChip(
                                        when (chip) {
                                            "Email" -> "formal"
                                            "WhatsApp" -> "casual"
                                            else -> chip
                                        },
                                    )
                                },
                                ui = state.dictation,
                                onHoldStart = {
                                    if (!micOk) micLauncher.launch(Manifest.permission.RECORD_AUDIO)
                                    else vm.holdStart()
                                },
                                onHoldEnd = vm::holdEnd,
                                onCancel = vm::cancelDictation,
                                onConfirm = vm::confirmDictation,
                                onOpenHistory = { tab = Tab.History },
                                onOpenSettings = { tab = Tab.Settings },
                                onOpenStyle = { tab = Tab.Style },
                                onCopy = vm::copied,
                            )
                            Tab.History -> HistoryScreen(state.history, vm::deleteHistory)
                            Tab.Style -> StyleScreen(
                                selected = state.settings.toneOverride,
                                onSelect = vm::setChip,
                                profiles = state.profiles,
                                onToggle = vm::toggleProfile,
                                snippets = state.snippets,
                                dictionary = state.dictionary,
                                onAddSnippet = vm::addSnippet,
                                onDeleteSnippet = vm::deleteSnippet,
                                onAddWord = vm::addWord,
                                onDeleteWord = vm::deleteWord,
                            )
                            Tab.Settings -> SettingsScreen(
                                settings = state.settings,
                                user = state.user,
                                plan = state.plan,
                                a11yOn = TextInjector.isAccessibilityOn(),
                                overlayOn = state.settings.overlayEnabled && overlayOk,
                                onGlass = vm::setGlass,
                                onBlur = vm::setBlur,
                                onTheme = vm::setTheme,
                                onToggle = { k, v ->
                                    vm.toggle(k, v)
                                    if (k == "overlay" && v && !AndroidSettings.canDrawOverlays(ctx)) {
                                        ctx.startActivity(
                                            Intent(
                                                AndroidSettings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                                Uri.parse("package:${ctx.packageName}"),
                                            ),
                                        )
                                    }
                                },
                                onSpeed = vm::setSpeed,
                                onLanguage = vm::toggleLanguage,
                                onLlmKey = vm::setLlm,
                                onSignOut = vm::signOut,
                                onOpenOverlaySettings = {
                                    if (!AndroidSettings.canDrawOverlays(ctx)) {
                                        ctx.startActivity(
                                            Intent(
                                                AndroidSettings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                                Uri.parse("package:${ctx.packageName}"),
                                            ),
                                        )
                                    }
                                },
                                onOpenA11ySettings = {
                                    ctx.startActivity(Intent(AndroidSettings.ACTION_ACCESSIBILITY_SETTINGS))
                                },
                            )
                        }
                    }
                    GlassTabBar(tab) { tab = it }
                }
            }
            SnackbarHost(snack, Modifier.align(Alignment.BottomCenter).padding(bottom = 88.dp))
        }
    }
}

@Composable
private fun GlassTabBar(current: Tab, onSelect: (Tab) -> Unit) {
    val c = LocalMsColors.current
    val shape = RoundedCornerShape(28.dp)
    Row(
        Modifier
            .navigationBarsPadding()
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .fillMaxWidth()
            .height(62.dp)
            .clip(shape)
            .background(c.glassFill)
            .border(Dp.Hairline, c.hairline, shape)
            .padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.SpaceEvenly,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TabBtn("Home", Icons.Outlined.Home, current == Tab.Home) { onSelect(Tab.Home) }
        TabBtn("History", Icons.Outlined.GraphicEq, current == Tab.History) { onSelect(Tab.History) }
        TabBtn("Dictate", Icons.Outlined.Mic, current == Tab.Dictate, accent = true) { onSelect(Tab.Dictate) }
        TabBtn("Style", Icons.Outlined.AutoAwesome, current == Tab.Style) { onSelect(Tab.Style) }
        TabBtn("Settings", Icons.Outlined.Settings, current == Tab.Settings) { onSelect(Tab.Settings) }
    }
}

@Composable
private fun TabBtn(label: String, icon: ImageVector, active: Boolean, accent: Boolean = false, onClick: () -> Unit) {
    val c = LocalMsColors.current
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier
            .clip(CircleShape)
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        Icon(
            icon,
            label,
            tint = when {
                accent && active -> Turquoise
                active -> c.text
                else -> c.textDim
            },
            modifier = Modifier.size(22.dp),
        )
        Text(label, fontSize = 10.sp, color = if (active) c.text else c.textDim)
    }
}
