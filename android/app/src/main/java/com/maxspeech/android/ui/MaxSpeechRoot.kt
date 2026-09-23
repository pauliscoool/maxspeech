package com.maxspeech.android.ui

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Settings
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import com.maxspeech.android.MaxSpeechApp
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.overlay.OverlayService
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.ui.about.AboutScreen
import com.maxspeech.android.ui.components.FloatingGlassBar
import com.maxspeech.android.ui.components.MsSpinner
import com.maxspeech.android.ui.components.PageSlide
import com.maxspeech.android.ui.components.SplashPane
import com.maxspeech.android.ui.components.ThemeWipe
import com.maxspeech.android.ui.home.HomeScreen
import com.maxspeech.android.ui.onboarding.OnboardingScreen
import com.maxspeech.android.ui.settings.MicLookScreen
import com.maxspeech.android.ui.settings.SettingsScreen
import com.maxspeech.android.ui.style.StyleScreen
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.MaxSpeechTheme
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise
import android.provider.Settings as AndroidSettings
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

enum class Tab { Home, Style, Settings, About }

private enum class SettingsSub { Main, MicLook }

@Composable
fun MaxSpeechRoot(vm: AppViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    MaxSpeechTheme(state.settings.theme, state.settings.glassAlpha, state.settings.blurStrength) {
        val ctx = LocalContext.current
        var tab by remember { mutableStateOf(Tab.Home) }
        var settingsSub by remember { mutableStateOf(SettingsSub.Main) }
        var micOk by remember { mutableStateOf(TextInjector.micGranted(ctx)) }
        var overlayOk by remember { mutableStateOf(TextInjector.overlayGranted(ctx)) }
        var a11yOk by remember { mutableStateOf(TextInjector.isAccessibilityOn(ctx)) }
        val snack = remember { SnackbarHostState() }
        val lifecycle = LocalLifecycleOwner.current.lifecycle

        val micLauncher = rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission(),
        ) { granted ->
            micOk = granted || TextInjector.micGranted(ctx)
        }
        val notifLauncher = rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission(),
        ) { }

        fun refreshPerms() {
            micOk = TextInjector.micGranted(ctx)
            overlayOk = TextInjector.overlayGranted(ctx)
            a11yOk = TextInjector.isAccessibilityOn(ctx)
        }

        LaunchedEffect(lifecycle) {
            lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
                refreshPerms()
                vm.refreshUsage()
            }
        }

        LaunchedEffect(state.toast) {
            state.toast?.let { snack.showSnackbar(it) }
        }

        val signedIn = state.user != null && state.user?.local != true

        // Floating mic only after cloud sign-in + overlay permission.
        LaunchedEffect(overlayOk, signedIn, a11yOk) {
            if (!signedIn) {
                MaxSpeechApp.instance.dictation.cancel()
                MaxSpeechApp.instance.floatingMic.hide()
                OverlayService.stop(ctx)
                return@LaunchedEffect
            }
            if (!overlayOk) {
                snack.showSnackbar("Allow “Display over other apps” so the mic can appear while you type")
                ctx.startActivity(
                    Intent(
                        AndroidSettings.ACTION_MANAGE_OVERLAY_PERMISSION,
                        Uri.parse("package:${ctx.packageName}"),
                    ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
                return@LaunchedEffect
            }
            MaxSpeechApp.instance.floatingMic.ensureShown(ctx)
            OverlayService.start(ctx)
            if (Build.VERSION.SDK_INT >= 33 && !TextInjector.notificationGranted(ctx)) {
                notifLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
            if (!a11yOk) {
                snack.showSnackbar("Turn on MaxSpeech in Accessibility — required for the mic while typing")
            }
        }
        LaunchedEffect(a11yOk, signedIn) {
            if (signedIn && !a11yOk) {
                // Give the snack a beat, then open Accessibility so Paul can flip MaxSpeech ON.
                kotlinx.coroutines.delay(900)
                if (!TextInjector.isAccessibilityOn(ctx)) {
                    ctx.startActivity(
                        Intent(AndroidSettings.ACTION_ACCESSIBILITY_SETTINGS)
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    )
                }
            }
        }

        Box(Modifier.fillMaxSize().background(LocalMsColors.current.bg)) {
            when {
                !state.ready -> {
                    SplashPane {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Text("MaxSpeech", style = DisplayLarge, color = LocalMsColors.current.text)
                            Spacer(Modifier.height(18.dp))
                            MsSpinner()
                            Spacer(Modifier.height(12.dp))
                            Text("Loading…", color = LocalMsColors.current.textDim, fontSize = 14.sp)
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
                        onStyleGroups = vm::saveStyleGroups,
                        onDone = { vm.finishOnboarding() },
                        micGranted = micOk,
                        overlayGranted = overlayOk,
                        a11yGranted = a11yOk,
                        styleMessaging = state.settings.styleToneMessaging,
                        styleEmail = state.settings.styleToneEmail,
                        styleWork = state.settings.styleToneWork,
                        styleSocial = state.settings.styleToneSocial,
                    )
                }
                else -> {
                    val dictation by MaxSpeechApp.instance.dictation.ui.collectAsStateWithLifecycle()
                    val showUpgrade = state.plan.tier == com.maxspeech.android.data.PlanTier.Free
                    Box(Modifier.fillMaxSize()) {
                        Column(Modifier.fillMaxSize()) {
                            if (showUpgrade) {
                                UpgradeNowBanner(
                                    onUpgrade = {
                                        ctx.startActivity(
                                            Intent(
                                                Intent.ACTION_VIEW,
                                                Uri.parse("https://maxspeech.vercel.app"),
                                            ),
                                        )
                                    },
                                )
                            }
                            Box(Modifier.weight(1f).fillMaxWidth()) {
                                PageSlide(tab, Modifier.fillMaxSize()) { current ->
                                    when (current) {
                                        Tab.Home -> HomeScreen(
                                            name = state.user?.username ?: "there",
                                            history = state.history,
                                            wordsUsed = state.plan.wordsUsed,
                                            wordsLimit = state.plan.weeklyLimit,
                                            appCount = state.appCount,
                                            overlayReady = overlayOk,
                                            a11yReady = a11yOk,
                                            settings = state.settings,
                                            onOpenSettings = { tab = Tab.Settings },
                                            onOpenStyleEdit = { tab = Tab.Style },
                                            onSelectStyleGroup = vm::setHomeStyleGroup,
                                            onCopy = vm::copied,
                                            onDelete = vm::deleteHistory,
                                            onRetranscribe = vm::retranscribe,
                                            onRetryFailed = vm::retryFailedHistory,
                                        )
                                        Tab.Style -> StyleScreen(
                                            settings = state.settings,
                                            selected = state.settings.toneOverride,
                                            onSelect = vm::setChip,
                                            profiles = state.profiles,
                                            onToggle = vm::toggleProfile,
                                            onProfileTone = vm::setProfileTone,
                                            snippets = state.snippets,
                                            dictionary = state.dictionary,
                                            onAddSnippet = vm::addSnippet,
                                            onDeleteSnippet = vm::deleteSnippet,
                                            onAddWord = vm::addWord,
                                            onDeleteWord = vm::deleteWord,
                                            onStyleGroups = vm::saveStyleGroups,
                                        )
                                        Tab.Settings -> {
                                            when (settingsSub) {
                                                SettingsSub.MicLook -> MicLookScreen(
                                                    settings = state.settings,
                                                    onBack = { settingsSub = SettingsSub.Main },
                                                    onSize = vm::setOverlaySize,
                                                    onAlpha = vm::setOverlayAlpha,
                                                    onWave = vm::setOverlayWaveScale,
                                                    onColor = vm::setOverlayMicColor,
                                                    onStyle = vm::setOverlayStyle,
                                                )
                                                SettingsSub.Main -> SettingsScreen(
                                                    settings = state.settings,
                                                    user = state.user,
                                                    plan = state.plan,
                                                    a11yOn = a11yOk,
                                                    overlayOn = state.settings.overlayEnabled && overlayOk,
                                                    onTheme = vm::setTheme,
                                                    onEnhanceSpeed = vm::setEnhanceSpeed,
                                                    onResetMicPosition = vm::resetOverlayPosition,
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
                                                        ctx.startActivity(
                                                            Intent(AndroidSettings.ACTION_ACCESSIBILITY_SETTINGS),
                                                        )
                                                    },
                                                    onOpenMicLook = { settingsSub = SettingsSub.MicLook },
                                                )
                                            }
                                        }
                                        Tab.About -> AboutScreen()
                                    }
                                }
                                Column(
                                    Modifier
                                        .align(Alignment.BottomCenter)
                                        .fillMaxWidth(),
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                ) {
                                    if (dictation.phase == DictationPhase.Error) {
                                        DictationRetryPill(
                                            message = dictation.error ?: "Something went wrong",
                                            onRetry = vm::retryDictation,
                                            onDismiss = vm::cancelDictation,
                                            modifier = Modifier
                                                .padding(horizontal = 20.dp)
                                                .padding(bottom = 10.dp),
                                        )
                                    }
                                    GlassTabBar(
                                        current = tab,
                                        onSelect = {
                                            if (it != Tab.Settings) settingsSub = SettingsSub.Main
                                            tab = it
                                        },
                                    )
                                }
                            }
                        }
                    }
                }
            }
            ThemeWipe(state.settings.theme)
            SnackbarHost(snack, Modifier.align(Alignment.BottomCenter).padding(bottom = 100.dp))
        }
    }
}

@Composable
private fun UpgradeNowBanner(onUpgrade: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(Orange)
            .statusBarsPadding()
            .clickable(onClick = onUpgrade)
            .padding(horizontal = 16.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                "Free plan — limited words",
                color = Color.White,
                fontSize = 13.sp,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                "Upgrade for more dictation every week",
                color = Color.White.copy(alpha = 0.88f),
                fontSize = 11.sp,
            )
        }
        Text(
            "Upgrade now",
            color = Color.White,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold,
            modifier = Modifier
                .clip(RoundedCornerShape(20.dp))
                .background(Color.Black.copy(alpha = 0.28f))
                .padding(horizontal = 12.dp, vertical = 7.dp),
        )
    }
}

@Composable
private fun DictationRetryPill(
    message: String,
    onRetry: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = LocalMsColors.current
    val shape = RoundedCornerShape(28.dp)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(Color(0xF0121212), shape)
            .border(1.dp, Color.White.copy(alpha = 0.10f), shape)
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            text = message,
            color = c.text,
            fontSize = 13.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Text(
            text = "Dismiss",
            color = c.textDim,
            fontSize = 13.sp,
            modifier = Modifier
                .clip(RoundedCornerShape(16.dp))
                .clickable(onClick = onDismiss)
                .padding(horizontal = 8.dp, vertical = 6.dp),
        )
        Text(
            text = "Retry",
            color = Color.White,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier
                .clip(RoundedCornerShape(16.dp))
                .background(Orange)
                .clickable(onClick = onRetry)
                .padding(horizontal = 14.dp, vertical = 8.dp),
        )
    }
}

@Composable
private fun GlassTabBar(
    current: Tab,
    onSelect: (Tab) -> Unit,
) {
    val c = LocalMsColors.current
    val density = LocalDensity.current
    val light = c.bg.luminance() > 0.5f
    val activeFill = if (light) Color.White else Color(0xFF2A2A2E)
    val tabs = remember {
        listOf(
            Triple(Tab.Home, "Home", Icons.Outlined.Home),
            Triple(Tab.Style, "Style", Icons.Outlined.AutoAwesome),
            Triple(Tab.Settings, "Settings", Icons.Outlined.Settings),
            Triple(Tab.About, "About", Icons.Outlined.Info),
        )
    }
    var tabLefts by remember { mutableStateOf(FloatArray(tabs.size)) }
    var tabWidths by remember { mutableStateOf(FloatArray(tabs.size)) }
    val selected = tabs.indexOfFirst { it.first == current }.coerceAtLeast(0)
    val targetLeft = tabLefts.getOrElse(selected) { 0f }
    val targetWidth = tabWidths.getOrElse(selected) { 0f }
    val pillLeft by animateFloatAsState(
        targetValue = targetLeft,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessMediumLow,
        ),
        label = "tabPillX",
    )
    val pillWidth by animateFloatAsState(
        targetValue = targetWidth,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = Spring.StiffnessMediumLow,
        ),
        label = "tabPillW",
    )
    // Slight vertical hop when the pill lands on a new tab.
    val pillHop = remember { Animatable(0f) }
    LaunchedEffect(selected) {
        pillHop.snapTo(0f)
        pillHop.animateTo(
            targetValue = -7f,
            animationSpec = spring(
                dampingRatio = Spring.DampingRatioMediumBouncy,
                stiffness = 900f,
            ),
        )
        pillHop.animateTo(
            targetValue = 0f,
            animationSpec = spring(
                dampingRatio = 0.42f,
                stiffness = 520f,
            ),
        )
    }
    val pillShape = RoundedCornerShape(percent = 50)

    FloatingGlassBar(
        modifier = Modifier
            .navigationBarsPadding()
            .padding(horizontal = 22.dp, vertical = 10.dp)
            .fillMaxWidth(),
        shape = RoundedCornerShape(percent = 50),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 6.dp, vertical = 10.dp),
        ) {
            if (pillWidth > 1f) {
                Box(Modifier.matchParentSize()) {
                    Box(
                        Modifier
                            .offset {
                                IntOffset(
                                    pillLeft.roundToInt(),
                                    pillHop.value.roundToInt(),
                                )
                            }
                            .width(with(density) { pillWidth.toDp() })
                            .fillMaxHeight()
                            .clip(pillShape)
                            .background(activeFill, pillShape),
                    )
                }
            }
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                tabs.forEachIndexed { index, (tab, label, icon) ->
                    TabBtn(
                        label = label,
                        icon = icon,
                        active = current == tab,
                        onClick = { onSelect(tab) },
                        modifier = Modifier
                            .weight(1f)
                            .onGloballyPositioned { coords ->
                                val left = coords.positionInParent().x
                                val width = coords.size.width.toFloat()
                                if (tabLefts.getOrElse(index) { -1f } != left ||
                                    tabWidths.getOrElse(index) { -1f } != width
                                ) {
                                    tabLefts = tabLefts.copyOf().also { it[index] = left }
                                    tabWidths = tabWidths.copyOf().also { it[index] = width }
                                }
                            },
                    )
                }
            }
        }
    }
}

@Composable
private fun TabBtn(
    label: String,
    icon: ImageVector,
    active: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val light = LocalMsColors.current.bg.luminance() > 0.5f
    val activeTint = Turquoise
    val idleTint = if (light) Color(0xFF1C1C1E) else Color.White
    val jump = remember { Animatable(0f) }
    val scale = remember { Animatable(1f) }
    LaunchedEffect(active) {
        if (active) {
            jump.snapTo(0f)
            scale.snapTo(1f)
            launch {
                jump.animateTo(
                    -9f,
                    spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = 1000f),
                )
                jump.animateTo(
                    0f,
                    spring(dampingRatio = 0.38f, stiffness = 480f),
                )
            }
            launch {
                scale.animateTo(
                    1.14f,
                    spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = 900f),
                )
                scale.animateTo(
                    1f,
                    spring(dampingRatio = 0.45f, stiffness = 500f),
                )
            }
        }
    }

    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        modifier = modifier
            .clip(RoundedCornerShape(percent = 50))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 4.dp)
            .graphicsLayer {
                translationY = jump.value
                scaleX = scale.value
                scaleY = scale.value
            },
    ) {
        Icon(
            icon,
            contentDescription = label,
            tint = if (active) activeTint else idleTint.copy(alpha = 0.78f),
            modifier = Modifier.size(22.dp),
        )
        Spacer(Modifier.height(4.dp))
        Text(
            text = label,
            fontSize = 11.sp,
            lineHeight = 13.sp,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
            fontWeight = if (active) FontWeight.SemiBold else FontWeight.Medium,
            color = if (active) activeTint else idleTint,
        )
    }
}

private fun Color.luminance(): Float {
    return 0.2126f * red + 0.7152f * green + 0.0722f * blue
}
