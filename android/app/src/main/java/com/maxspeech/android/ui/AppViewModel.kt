package com.maxspeech.android.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.maxspeech.android.MaxSpeechApp
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppProfileEntity
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthUser
import com.maxspeech.android.data.DictionaryEntity
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.PlanCalculator
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.PlanTier
import com.maxspeech.android.data.SnippetEntity
import com.maxspeech.android.data.SttLanguages
import com.maxspeech.android.data.UiTheme
import com.maxspeech.android.pipeline.DictationUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlin.math.max

data class UiState(
    val settings: AppSettings = AppSettings(),
    val user: AuthUser? = null,
    val history: List<HistoryEntity> = emptyList(),
    val profiles: List<AppProfileEntity> = emptyList(),
    val snippets: List<SnippetEntity> = emptyList(),
    val dictionary: List<String> = emptyList(),
    val dictation: DictationUi = DictationUi(),
    val plan: PlanStatus = PlanStatus(PlanTier.Free, 0, 0),
    val appCount: Int = 0,
    val enhancedCount: Int = 0,
    val editedCount: Int = 0,
    val authBusy: Boolean = false,
    val authError: String? = null,
    val authInfo: String? = null,
    val toast: String? = null,
    val ready: Boolean = false,
)

class AppViewModel(app: Application) : AndroidViewModel(app) {
    private val ms = getApplication<MaxSpeechApp>()
    private val authBusy = MutableStateFlow(false)
    private val authError = MutableStateFlow<String?>(null)
    private val authInfo = MutableStateFlow<String?>(null)
    private val toast = MutableStateFlow<String?>(null)
    private val boot = MutableStateFlow(false)

    // Flat combines only — nested combine + R8 previously produced ClassCastException on launch.
    private val core = combine(
        ms.settings.flow,
        ms.auth.user,
        ms.db.historyDao().observe(),
        ms.db.profileDao().observe(),
    ) { settings, user, history, profiles ->
        Core(settings, user, history, profiles)
    }

    private val extras = combine(
        ms.db.usageDao().observeWordsSince(PlanCalculator.weekStartUtc()),
        ms.db.historyDao().observeDistinctApps(),
        ms.db.snippetDao().observe(),
        ms.db.dictionaryDao().observe(),
    ) { words, apps, snippets, dictionary ->
        Extras(words, apps, snippets, dictionary)
    }

    private val counts = combine(
        ms.db.historyDao().observeEnhanced(),
        ms.db.historyDao().observeEdited(),
        boot,
    ) { enhanced, edited, ready ->
        Counts(enhanced, edited, ready)
    }

    private val flash = combine(authBusy, authError, authInfo, toast) { busy, err, info, t ->
        Flash(busy, err, info, t)
    }

    val state: StateFlow<UiState> = combine(
        core,
        extras,
        counts,
        flash,
        ms.dictation.ui,
    ) { c, e, k, f, dictation ->
        UiState(
            settings = c.settings,
            user = c.user,
            history = c.history,
            profiles = c.profiles,
            snippets = e.snippets,
            dictionary = e.dictionary,
            dictation = dictation,
            plan = PlanCalculator.from(c.user, e.words),
            appCount = e.apps,
            enhancedCount = k.enhanced,
            editedCount = k.edited,
            authBusy = f.busy,
            authError = f.error,
            authInfo = f.info,
            toast = f.toast,
            ready = k.ready,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), UiState())

    init {
        viewModelScope.launch {
            runCatching {
                val started = System.currentTimeMillis()
                ms.settings.snapshot()
                val user = ms.auth.current()
                if (user?.local == true) ms.auth.signOut()
                val wait = max(0L, 520L - (System.currentTimeMillis() - started))
                kotlinx.coroutines.delay(wait)
            }
            boot.value = true
        }
    }

    fun refreshUsage() {
        viewModelScope.launch {
            runCatching { ms.settings.snapshot() }
        }
    }

    fun holdStart() {
        val pkg = TextInjector.foregroundPackage() ?: "com.maxspeech.android"
        ms.dictation.start(pkg, paste = false)
    }

    fun holdEnd() {
        ms.dictation.stopAndFinish()
    }

    fun cancelDictation() = ms.dictation.cancel()

    fun confirmDictation() {
        viewModelScope.launch {
            val text = ms.dictation.ui.value.finalText
            if (text.isNotBlank()) TextInjector.insert(getApplication(), text)
            ms.dictation.confirmPaste()
        }
    }

    fun setChip(tone: String) = viewModelScope.launch { ms.settings.setToneOverride(tone.lowercase()) }

    fun setGlass(v: Float) = viewModelScope.launch { ms.settings.setGlassAlpha(v) }
    fun setBlur(v: Float) = viewModelScope.launch { ms.settings.setBlur(v) }
    fun setTheme(t: UiTheme) = viewModelScope.launch { ms.settings.setTheme(t) }

    fun toggle(key: String, on: Boolean) = viewModelScope.launch {
        when (key) {
            "overlay" -> ms.settings.setOverlayEnabled(on)
            "confirm" -> ms.settings.setOverlayConfirm(on)
            "enhance" -> ms.settings.setAiEnhance(on)
            "multi" -> {
                ms.settings.setMultilingual(on)
                if (!on) {
                    val cur = ms.settings.snapshot().languages
                    ms.settings.setLanguages(
                        if (cur.contains("en")) listOf("en") else listOf(cur.firstOrNull() ?: "en"),
                    )
                }
            }
            "space" -> ms.settings.setTrailingSpace(on)
            "haptics" -> ms.settings.setHaptics(on)
            "sound" -> ms.settings.setSoundCue(on)
            "live" -> ms.settings.setLiveTranscript(on)
        }
    }

    fun toggleLanguage(code: String) = viewModelScope.launch {
        val allowed = SttLanguages.allowed()
        if (code !in allowed) return@launch
        val snap = ms.settings.snapshot()
        val user = ms.auth.current()
        val multi = snap.multilingual && SttLanguages.multilingualAllowed(PlanCalculator.from(user, 0).tier)
        if (!multi) {
            ms.settings.setLanguages(listOf(code))
            return@launch
        }
        val cur = snap.languages.filter { it in allowed }.toMutableList()
        if (cur.contains(code)) {
            if (cur.size > 1) cur.remove(code)
        } else if (cur.size < SttLanguages.MAX) {
            cur += code
        }
        ms.settings.setLanguages(cur.ifEmpty { SttLanguages.DEFAULT })
    }

    fun deleteHistory(id: Long) = viewModelScope.launch {
        ms.db.historyDao().delete(id)
    }

    fun toggleProfile(p: AppProfileEntity) = viewModelScope.launch {
        ms.db.profileDao().update(p.copy(enabled = !p.enabled))
    }

    fun signIn(email: String, password: String) = viewModelScope.launch {
        authBusy.value = true
        authError.value = null
        authInfo.value = null
        runCatching { ms.auth.signIn(email, password) }
            .onFailure { authError.value = it.message ?: "Sign in failed" }
        authBusy.value = false
    }

    fun signUp(email: String, password: String, username: String) = viewModelScope.launch {
        authBusy.value = true
        authError.value = null
        authInfo.value = null
        runCatching { ms.auth.signUp(email, password, username) }
            .onSuccess { (_, confirm) ->
                if (confirm) {
                    authInfo.value =
                        "Check your email to confirm. The link opens the MaxSpeech website, then sign in here."
                }
            }
            .onFailure { authError.value = it.message ?: "Could not create account" }
        authBusy.value = false
    }

    fun resetPassword(email: String) = viewModelScope.launch {
        authBusy.value = true
        authError.value = null
        authInfo.value = null
        runCatching { ms.auth.requestPasswordReset(email) }
            .onSuccess {
                authInfo.value =
                    "Check your email. The reset link opens the MaxSpeech website, then sign in here."
            }
            .onFailure { authError.value = it.message ?: "Could not send reset email" }
        authBusy.value = false
    }

    fun clearAuthFlash() {
        authError.value = null
        authInfo.value = null
    }

    fun signOut() = viewModelScope.launch {
        authBusy.value = true
        ms.auth.signOut()
        ms.settings.setOnboarded(false)
        authBusy.value = false
    }

    fun finishOnboarding() = viewModelScope.launch { ms.settings.setOnboarded(true) }

    fun copied(text: String) {
        toast.value = "Copied"
        viewModelScope.launch {
            kotlinx.coroutines.delay(1200)
            if (toast.value == "Copied") toast.value = null
        }
    }

    fun addSnippet(trigger: String, expansion: String) = viewModelScope.launch {
        val t = trigger.trim()
        val e = expansion.trim()
        if (t.isEmpty() || e.isEmpty()) return@launch
        ms.db.snippetDao().upsert(SnippetEntity(t, e))
    }

    fun deleteSnippet(trigger: String) = viewModelScope.launch {
        ms.db.snippetDao().delete(trigger)
    }

    fun addWord(word: String) = viewModelScope.launch {
        val w = word.trim()
        if (w.isNotEmpty()) ms.db.dictionaryDao().insert(DictionaryEntity(w))
    }

    fun deleteWord(word: String) = viewModelScope.launch {
        ms.db.dictionaryDao().delete(word)
    }

    private data class Core(
        val settings: AppSettings,
        val user: AuthUser?,
        val history: List<HistoryEntity>,
        val profiles: List<AppProfileEntity>,
    )

    private data class Extras(
        val words: Int,
        val apps: Int,
        val snippets: List<SnippetEntity>,
        val dictionary: List<String>,
    )

    private data class Counts(
        val enhanced: Int,
        val edited: Int,
        val ready: Boolean,
    )

    private data class Flash(
        val busy: Boolean,
        val error: String?,
        val info: String?,
        val toast: String?,
    )
}
