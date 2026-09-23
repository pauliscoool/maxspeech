package com.maxspeech.android.data

import android.content.Context
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.floatPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

private val Context.dataStore by preferencesDataStore("maxspeech_settings")

enum class UiTheme { Dark, Gray, Light }
enum class EnhanceSpeed { Fast, Thinking, Ultra }

/** How the dictation control sits over other apps. */
enum class OverlayStyle {
    /** Draggable bubble — current default. */
    Floating,
    /** Full-width bar docked just above the keyboard. */
    KeyboardBar,
}

data class AppSettings(
    val glassAlpha: Float = 0.50f,
    val blurStrength: Float = 0.70f,
    val theme: UiTheme = UiTheme.Dark,
    val overlayEnabled: Boolean = true,
    val overlayConfirm: Boolean = true,
    val overlayStyle: OverlayStyle = OverlayStyle.Floating,
    /** Floating mic / pill scale. 0.80 = 20% smaller than the original. */
    val overlaySize: Float = 0.80f,
    /** Floating surface opacity. 0.80 = 20% transparent. */
    val overlayAlpha: Float = 0.80f,
    /** Waveform bar height multiplier on the listening pill. */
    val overlayWaveScale: Float = 1f,
    /** ARGB color for the idle mic icon (default MaxSpeech turquoise). */
    val overlayMicColor: Long = 0xFF2DD4BFL,
    /** Saved bubble center as 0..1 of screen size. Negative = unset. */
    val overlayCenterX: Float = -1f,
    val overlayCenterY: Float = -1f,
    /** Epoch millis until which the overlay stays hidden (drag-to-X snooze). */
    val overlaySnoozeUntil: Long = 0L,
    val aiEnhance: Boolean = true,
    val enhanceSpeed: EnhanceSpeed = EnhanceSpeed.Thinking,
    val multilingual: Boolean = false,
    val languages: List<String> = listOf("en"),
    val trailingSpace: Boolean = true,
    val soundCue: Boolean = false,
    val haptics: Boolean = true,
    val liveTranscript: Boolean = true,
    val onboarded: Boolean = false,
    val localMode: Boolean = false,
    val deepgramKey: String = "",
    val llmKey: String = "",
    val toneOverride: String = "default",
    /** Per app-group voice defaults from onboarding / Style. */
    val styleToneMessaging: String = "casual",
    val styleToneEmail: String = "formal",
    val styleToneWork: String = "default",
    val styleToneSocial: String = "casual",
    val styleGroupsConfigured: Boolean = false,
    val homeStyleGroup: String = StyleGroupId.Messaging.key,
)

class SettingsRepository(private val context: Context) {
    val flow: Flow<AppSettings> = context.dataStore.data.map { it.toSettings() }

    suspend fun snapshot(): AppSettings = context.dataStore.data.first().toSettings()

    suspend fun ensureDefaults() {
        context.dataStore.edit { prefs ->
            // One-time: turn on floating keyboard mic for existing installs.
            if (prefs[Keys.overlayBubbleV1] != true) {
                prefs[Keys.overlay] = true
                prefs[Keys.overlayBubbleV1] = true
            }
        }
    }

    suspend fun setGlassAlpha(value: Float) = set(Keys.glass, value.coerceIn(0f, 1f))
    suspend fun setBlur(value: Float) = set(Keys.blur, value.coerceIn(0f, 1f))
    suspend fun setTheme(theme: UiTheme) = set(Keys.theme, theme.name.lowercase())
    suspend fun setOverlayEnabled(on: Boolean) = set(Keys.overlay, on)
    suspend fun setOverlayConfirm(on: Boolean) = set(Keys.confirm, on)
    suspend fun setOverlayStyle(style: OverlayStyle) = set(Keys.overlayStyle, style.name.lowercase())
    suspend fun setOverlaySize(value: Float) = set(Keys.overlaySize, value.coerceIn(0.55f, 1.45f))
    suspend fun setOverlayAlpha(value: Float) = set(Keys.overlayAlpha, value.coerceIn(0.25f, 1f))
    suspend fun setOverlayWaveScale(value: Float) = set(Keys.overlayWave, value.coerceIn(0.55f, 1.55f))
    suspend fun setOverlayMicColor(argb: Long) = set(Keys.overlayMicColor, argb.toString())
    suspend fun setOverlayCenter(xFrac: Float, yFrac: Float) {
        context.dataStore.edit {
            it[Keys.overlayCenterX] = xFrac.coerceIn(0f, 1f)
            it[Keys.overlayCenterY] = yFrac.coerceIn(0f, 1f)
        }
    }

    /** Hide the overlay until [untilEpochMs]. Pass 0 to clear. */
    suspend fun setOverlaySnoozeUntil(untilEpochMs: Long) =
        set(Keys.overlaySnoozeUntil, untilEpochMs.coerceAtLeast(0L).toString())
    suspend fun setAiEnhance(on: Boolean) = set(Keys.enhance, on)
    suspend fun setEnhanceSpeed(speed: EnhanceSpeed) = set(Keys.speed, speed.name.lowercase())
    suspend fun setMultilingual(on: Boolean) = set(Keys.multi, on)
    suspend fun setLanguages(codes: List<String>) = set(Keys.langs, codes.joinToString(","))
    suspend fun setTrailingSpace(on: Boolean) = set(Keys.space, on)
    suspend fun setSoundCue(on: Boolean) = set(Keys.sound, on)
    suspend fun setHaptics(on: Boolean) = set(Keys.haptics, on)
    suspend fun setLiveTranscript(on: Boolean) = set(Keys.live, on)
    suspend fun setOnboarded(on: Boolean) = set(Keys.onboarded, on)
    suspend fun setLocalMode(on: Boolean) = set(Keys.local, on)
    suspend fun setDeepgramKey(value: String) = set(Keys.dg, value.trim())
    suspend fun setLlmKey(value: String) = set(Keys.llm, value.trim())
    suspend fun setToneOverride(tone: String) = set(Keys.tone, tone)

    suspend fun setStyleGroupTones(
        messaging: String,
        email: String,
        work: String,
        social: String,
    ) {
        context.dataStore.edit {
            it[Keys.styleToneMessaging] = messaging.lowercase()
            it[Keys.styleToneEmail] = email.lowercase()
            it[Keys.styleToneWork] = work.lowercase()
            it[Keys.styleToneSocial] = social.lowercase()
            it[Keys.styleGroupsConfigured] = true
        }
    }

    suspend fun setHomeStyleGroup(id: StyleGroupId) = set(Keys.homeStyleGroup, id.key)
    suspend fun setStyleGroupsConfigured(on: Boolean) = set(Keys.styleGroupsConfigured, on)

    private suspend fun set(key: Preferences.Key<Float>, value: Float) {
        context.dataStore.edit { it[key] = value }
    }

    private suspend fun set(key: Preferences.Key<Boolean>, value: Boolean) {
        context.dataStore.edit { it[key] = value }
    }

    private suspend fun set(key: Preferences.Key<String>, value: String) {
        context.dataStore.edit { it[key] = value }
    }

    private object Keys {
        val glass = floatPreferencesKey("glass_alpha")
        val blur = floatPreferencesKey("blur_strength")
        val theme = stringPreferencesKey("ui_theme")
        val overlay = booleanPreferencesKey("overlay_enabled")
        val confirm = booleanPreferencesKey("overlay_confirm")
        val overlayStyle = stringPreferencesKey("overlay_style")
        val overlaySize = floatPreferencesKey("overlay_size")
        val overlayAlpha = floatPreferencesKey("overlay_alpha")
        val overlayWave = floatPreferencesKey("overlay_wave_scale")
        val overlayMicColor = stringPreferencesKey("overlay_mic_color")
        val overlayCenterX = floatPreferencesKey("overlay_center_x")
        val overlayCenterY = floatPreferencesKey("overlay_center_y")
        val overlaySnoozeUntil = stringPreferencesKey("overlay_snooze_until")
        val enhance = booleanPreferencesKey("ai_enhance")
        val speed = stringPreferencesKey("enhance_speed")
        val multi = booleanPreferencesKey("stt_multilingual")
        val langs = stringPreferencesKey("stt_languages")
        val space = booleanPreferencesKey("trailing_space")
        val sound = booleanPreferencesKey("sound_cue")
        val haptics = booleanPreferencesKey("haptics")
        val live = booleanPreferencesKey("show_live_transcript")
        val onboarded = booleanPreferencesKey("onboarded")
        val local = booleanPreferencesKey("local_mode")
        val dg = stringPreferencesKey("deepgram_api_key")
        val llm = stringPreferencesKey("llm_api_key")
        val tone = stringPreferencesKey("tone_override")
        val styleToneMessaging = stringPreferencesKey("style_tone_messaging")
        val styleToneEmail = stringPreferencesKey("style_tone_email")
        val styleToneWork = stringPreferencesKey("style_tone_work")
        val styleToneSocial = stringPreferencesKey("style_tone_social")
        val styleGroupsConfigured = booleanPreferencesKey("style_groups_configured")
        val homeStyleGroup = stringPreferencesKey("home_style_group")
        val overlayBubbleV1 = booleanPreferencesKey("overlay_keyboard_bubble_v1")
    }

    private fun Preferences.toSettings(): AppSettings {
        val langs = this[Keys.langs]
            ?.split(",")
            ?.map { it.trim() }
            ?.filter { it.isNotEmpty() }
            .orEmpty()
            .ifEmpty { listOf("en") }
        return AppSettings(
            glassAlpha = this[Keys.glass] ?: 0.50f,
            blurStrength = this[Keys.blur] ?: 0.70f,
            theme = when (this[Keys.theme]) {
                "gray" -> UiTheme.Gray
                "light" -> UiTheme.Light
                else -> UiTheme.Dark
            },
            overlayEnabled = this[Keys.overlay] ?: true,
            overlayConfirm = this[Keys.confirm] ?: true,
            overlayStyle = when (this[Keys.overlayStyle]) {
                "keyboardbar", "keyboard_bar" -> OverlayStyle.KeyboardBar
                else -> OverlayStyle.Floating
            },
            overlaySize = this[Keys.overlaySize] ?: 0.80f,
            overlayAlpha = this[Keys.overlayAlpha] ?: 0.80f,
            overlayWaveScale = this[Keys.overlayWave] ?: 1f,
            overlayMicColor = this[Keys.overlayMicColor]?.toLongOrNull() ?: 0xFF2DD4BFL,
            overlayCenterX = this[Keys.overlayCenterX] ?: -1f,
            overlayCenterY = this[Keys.overlayCenterY] ?: -1f,
            overlaySnoozeUntil = this[Keys.overlaySnoozeUntil]?.toLongOrNull() ?: 0L,
            aiEnhance = this[Keys.enhance] ?: true,
            enhanceSpeed = when (this[Keys.speed]) {
                "fast" -> EnhanceSpeed.Fast
                "ultra" -> EnhanceSpeed.Ultra
                else -> EnhanceSpeed.Thinking
            },
            multilingual = this[Keys.multi] ?: false,
            languages = langs,
            trailingSpace = this[Keys.space] ?: true,
            soundCue = this[Keys.sound] ?: false,
            haptics = this[Keys.haptics] ?: true,
            liveTranscript = this[Keys.live] ?: true,
            onboarded = this[Keys.onboarded] ?: false,
            localMode = this[Keys.local] ?: false,
            deepgramKey = this[Keys.dg].orEmpty(),
            llmKey = this[Keys.llm].orEmpty(),
            toneOverride = this[Keys.tone] ?: "default",
            styleToneMessaging = this[Keys.styleToneMessaging] ?: "casual",
            styleToneEmail = this[Keys.styleToneEmail] ?: "formal",
            styleToneWork = this[Keys.styleToneWork] ?: "default",
            styleToneSocial = this[Keys.styleToneSocial] ?: "casual",
            styleGroupsConfigured = this[Keys.styleGroupsConfigured] ?: false,
            homeStyleGroup = this[Keys.homeStyleGroup] ?: StyleGroupId.Messaging.key,
        )
    }
}
