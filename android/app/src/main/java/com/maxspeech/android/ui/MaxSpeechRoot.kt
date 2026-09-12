package com.maxspeech.android.ui

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.GraphicEq
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.overlay.OverlayService
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.ui.components.GlassScrim
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.components.MsSpinner
import com.maxspeech.android.ui.components.PageEnter
import com.maxspeech.android.ui.components.SplashPane
import com.maxspeech.android.ui.components.ThemeWipe
import com.maxspeech.android.ui.history.HistoryScreen
import com.maxspeech.android.ui.home.HomeScreen
import com.maxspeech.android.ui.onboarding.OnboardingScreen
import com.maxspeech.android.ui.settings.SettingsScreen
import com.maxspeech.android.ui.style.StyleScreen
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.MaxSpeechTheme
import com.maxspeech.android.ui.theme.Turquoise
import android.provider.Settings as AndroidSettings

enum class Tab { Home, History, Style, Settings }

@Composable
fun MaxSpeechRoot(vm: AppViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    MaxSpeechTheme(state.settings.theme, state.settings.glassAlpha, state.settings.blurStrength) {
        val ctx = LocalContext.current
        var tab by remember { mutableStateOf(Tab.Home) }
        var micOk by remember { mutableStateOf(TextInjector.micGranted(ctx)) }
        var overlayOk by remember { mutableStateOf(TextInjector.overlayGranted(ctx)) }
        var a11yOk by remember { mutableStateOf(TextInjector.isAccessibilityOn(ctx)) }
        var permEpoch by remember { mutableIntStateOf(0) }
        val snack = remember { SnackbarHostState() }
        val lifecycle = LocalLifecycleOwner.current.lifecycle

        val micLauncher = rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission(),
        ) { granted ->
            micOk = granted || TextInjector.micGranted(ctx)
            permEpoch++
        }
        val notifLauncher = rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission(),
        ) { permEpoch++ }

        fun refreshPerms() {
            micOk = TextInjector.micGranted(ctx)
            overlayOk = TextInjector.overlayGranted(ctx)
            a11yOk = TextInjector.isAccessibilityOn(ctx)
        }

        LaunchedEffect(lifecycle) {
            lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
                refreshPerms()
                vm.refreshUsage()
                permEpoch++
                while (true) {
                    kotlinx.coroutines.delay(700)
                    val beforeMic = micOk
                    val beforeOverlay = overlayOk
                    val beforeA11y = a11yOk
                    refreshPerms()
                    if (beforeMic != micOk || beforeOverlay != overlayOk || beforeA11y != a11yOk) permEpoch++
                }
            }
        }

        LaunchedEffect(state.settings.overlayEnabled, overlayOk, a11yOk, state.settings.onboarded) {
            refreshPerms()
            // Capsule needs overlay + a11y. Starting the FGS earlier caused a restart/close loop
            // while Android was still flipping Accessibility / restricted settings.
            val wantOverlay = state.settings.overlayEnabled && overlayOk && a11yOk && state.settings.onboarded
            if (wantOverlay) {
                if (Build.VERSION.SDK_INT >= 33 && !TextInjector.notificationGranted(ctx)) {
                    notifLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
                kotlinx.coroutines.delay(400)
                refreshPerms()
                if (state.settings.overlayEnabled && TextInjector.overlayGranted(ctx) &&
                    TextInjector.isAccessibilityOn(ctx) && state.settings.onboarded
                ) {
                    OverlayService.start(ctx)
                }
            } else {
                OverlayService.stop(ctx)
            }
        }
        LaunchedEffect(state.toast) {
            state.toast?.let { snack.showSnackbar(it) }
        }

        val signedIn = state.user != null && state.user?.local != true
        Box(Modifier.fillMaxSize().background(LocalMsColors.current.bg)) {
            when {
                !state.ready -> {
                    SplashPane {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Text("MaxSpeech", style = DisplayLarge, color = LocalMsColors.current.text)
                            Spacer(Modifier.height(18.dp))
                            MsSpinner()
                            Spacer(Modifier.height(12.dp))
                            Text("Loading your dictations…", color = LocalMsColors.current.textDim, fontSize = 14.sp)
                        }
                    }
                }
                !signedIn || !state.settings.onboarded -> {
                    OnboardingScreen(
                        busy = state.authBusy,
                        error = state.authError,
                        info = state.authInfo,
                        authed = signedIn,
                        onSignIn = { e, p -> vm.signIn(e, p) },
                        onSignUp = { e, p, u -> vm.signUp(e, p, u) },
                        onForgot = { e -> vm.resetPassword(e) },
                        onClearFlash = { vm.clearAuthFlash() },
                        onMic = { micLauncher.launch(Manifest.permission.RECORD_AUDIO) },
                        onOverlay = {
                            ctx.startActivity(
                                Intent(
                                    AndroidSettings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                    Uri.parse("package:${ctx.packageName}"),
                                ),
                            )
                        },
                        onAppInfo = {
                            ctx.startActivity(
                                Intent(
                                    AndroidSettings.ACTION_APPLICATION_DETAILS_SETTINGS,
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
                        a11yGranted = a11yOk,
                    )
                }
                else -> {
                    Box(Modifier.fillMaxSize()) {
                        PageEnter(tab, Modifier.fillMaxSize()) { current ->
                            when (current) {
                                Tab.Home -> HomeScreen(
                                    name = state.user?.username ?: "there",
                                    history = state.history,
                                    wordsUsed = state.plan.wordsUsed,
                                    wordsLimit = state.plan.weeklyLimit,
                                    appCount = state.appCount,
                                    enhancedCount = state.enhancedCount,
                                    editedCount = state.editedCount,
                                    chips = listOf("Casual", "Email", "WhatsApp"),
                                    selectedChip = when (state.settings.toneOverride.lowercase()) {
                                        "formal" -> "Email"
                                        "whatsapp" -> "WhatsApp"
                                        "casual" -> "Casual"
                                        else -> "Casual"
                                    },
                                    onChip = { chip ->
                                        vm.setChip(
                                            when (chip) {
                                                "Email" -> "formal"
                                                "WhatsApp" -> "whatsapp"
                                                else -> "casual"
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
                                    a11yOn = a11yOk,
                                    overlayOn = state.settings.overlayEnabled && overlayOk,
                                    onGlass = vm::setGlass,
                                    onBlur = vm::setBlur,
                                    onTheme = vm::setTheme,
                                    onToggle = { k, v ->
                                        vm.toggle(k, v)
                                        if (k == "overlay" && v && !overlayOk) {
                                            ctx.startActivity(
                                                Intent(
                                                    AndroidSettings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                                    Uri.parse("package:${ctx.packageName}"),
                                                ),
                                            )
                                        }
                                    },
                                    onLanguage = vm::toggleLanguage,
                                    onSignOut = vm::signOut,
                                    signingOut = state.authBusy,
                                    onOpenOverlaySettings = {
                                        ctx.startActivity(
                                            Intent(
                                                AndroidSettings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                                Uri.parse("package:${ctx.packageName}"),
                                            ),
                                        )
                                    },
                                    onOpenA11ySettings = {
                                        ctx.startActivity(Intent(AndroidSettings.ACTION_ACCESSIBILITY_SETTINGS))
                                    },
                                    onOpenAppInfo = {
                                        ctx.startActivity(
                                            Intent(
                                                AndroidSettings.ACTION_APPLICATION_DETAILS_SETTINGS,
                                                Uri.parse("package:${ctx.packageName}"),
                                            ),
                                        )
                                    },
                                )
                            }
                        }
                        Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth()) {
                            GlassScrim(Modifier.fillMaxWidth().height(28.dp))
                            GlassTabBar(
                                current = tab,
                                dictating = state.dictation.phase == DictationPhase.Listening ||
                                    state.dictation.phase == DictationPhase.Processing,
                                onSelect = { tab = it },
                                onDictate = {
                                    tab = Tab.Home
                                    if (!micOk) micLauncher.launch(Manifest.permission.RECORD_AUDIO)
                                    else vm.holdStart()
                                },
                            )
                        }
                    }
                }
            }
            ThemeWipe(state.settings.theme)
            SnackbarHost(snack, Modifier.align(Alignment.BottomCenter).padding(bottom = 96.dp))
        }
    }
}

@Composable
private fun GlassTabBar(
    current: Tab,
    dictating: Boolean,
    onSelect: (Tab) -> Unit,
    onDictate: () -> Unit,
) {
    GlassSurface(
        modifier = Modifier
            .navigationBarsPadding()
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .fillMaxWidth()
            .height(64.dp),
        shape = RoundedCornerShape(28.dp),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 6.dp),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TabBtn("Home", Icons.Outlined.Home, current == Tab.Home) { onSelect(Tab.Home) }
            TabBtn("History", Icons.Outlined.GraphicEq, current == Tab.History) { onSelect(Tab.History) }
            TabBtn("Dictate", Icons.Outlined.Mic, dictating, accent = true, onClick = onDictate)
            TabBtn("Style", Icons.Outlined.AutoAwesome, current == Tab.Style) { onSelect(Tab.Style) }
            TabBtn("Settings", Icons.Outlined.Settings, current == Tab.Settings) { onSelect(Tab.Settings) }
        }
    }
}

@Composable
private fun TabBtn(
    label: String,
    icon: ImageVector,
    active: Boolean,
    accent: Boolean = false,
    onClick: () -> Unit,
) {
    val c = LocalMsColors.current
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 6.dp),
    ) {
        Icon(
            icon,
            label,
            tint = when {
                accent && active -> Turquoise
                accent -> Turquoise.copy(alpha = 0.85f)
                active -> c.text
                else -> c.textDim
            },
            modifier = Modifier.size(22.dp),
        )
        Text(label, fontSize = 10.sp, color = if (active || accent) c.text else c.textDim)
    }
}
