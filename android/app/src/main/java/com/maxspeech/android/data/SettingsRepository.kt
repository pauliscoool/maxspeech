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

data class AppSettings(
    val glassAlpha: Float = 0.50f,
    val blurStrength: Float = 0.70f,
    val theme: UiTheme = UiTheme.Dark,
    val overlayEnabled: Boolean = true,
    val overlayConfirm: Boolean = true,
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
)

class SettingsRepository(private val context: Context) {
    val flow: Flow<AppSettings> = context.dataStore.data.map { it.toSettings() }

    suspend fun snapshot(): AppSettings = context.dataStore.data.first().toSettings()

    suspend fun ensureDefaults() {
        snapshot()
    }

    suspend fun setGlassAlpha(value: Float) = set(Keys.glass, value.coerceIn(0f, 1f))
    suspend fun setBlur(value: Float) = set(Keys.blur, value.coerceIn(0f, 1f))
    suspend fun setTheme(theme: UiTheme) = set(Keys.theme, theme.name.lowercase())
    suspend fun setOverlayEnabled(on: Boolean) = set(Keys.overlay, on)
    suspend fun setOverlayConfirm(on: Boolean) = set(Keys.confirm, on)
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
        )
    }
}
