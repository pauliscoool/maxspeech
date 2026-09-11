package com.maxspeech.android.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.maxspeech.android.MaxSpeechApp
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppProfileEntity
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthUser
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.PlanCalculator
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.SttLanguages
import com.maxspeech.android.data.UiTheme
import com.maxspeech.android.pipeline.DictationUi
import kotlinx.coroutines.delay
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
    val snippets: List<com.maxspeech.android.data.SnippetEntity> = emptyList(),
    val dictionary: List<String> = emptyList(),
    val dictation: DictationUi = DictationUi(),
    val plan: PlanStatus = PlanStatus(com.maxspeech.android.data.PlanTier.Free, 0, 0),
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
    private val ms = app as MaxSpeechApp
    private val authBusy = MutableStateFlow(false)
    private val authError = MutableStateFlow<String?>(null)
    private val authInfo = MutableStateFlow<String?>(null)
    private val toast = MutableStateFlow<String?>(null)
    private val boot = MutableStateFlow(false)

    val state: StateFlow<UiState> = combine(
        combine(ms.settings.flow, ms.auth.user, ms.db.historyDao().observe(), ms.db.profileDao().observe()) { s, u, h, p ->
            Quad(s, u, h, p)
        },
        combine(ms.dictation.ui, authBusy, authError, authInfo, toast) { d, b, e, i, t ->
            Quint(d, b, e, i, t)
        },
        combine(
            ms.db.usageDao().observeWordsSince(PlanCalculator.weekStartUtc()),
            ms.db.historyDao().observeDistinctApps(),
            ms.db.snippetDao().observe(),
            ms.db.dictionaryDao().observe(),
        ) { w, a, sn, dict ->
            Extra(w, a, sn, dict)
        },
        combine(
            ms.db.historyDao().observeEnhanced(),
            ms.db.historyDao().observeEdited(),
            boot,
        ) { enh, ed, ready -> Triple(enh, ed, ready) },
    ) { quad, quint, extra, counts ->
        val user = quad.u
        UiState(
            settings = quad.s,
            user = user,
            history = quad.h,
            profiles = quad.p,
            snippets = extra.sn,
            dictionary = extra.dict,
            dictation = quint.d,
            plan = PlanCalculator.from(user, extra.w),
            appCount = extra.a,
            enhancedCount = counts.first,
            editedCount = counts.second,
            authBusy = quint.b,
            authError = quint.e,
            authInfo = quint.i,
            toast = quint.t,
            ready = counts.third,
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, UiState())

    init {
        viewModelScope.launch {
            val started = System.currentTimeMillis()
            ms.settings.snapshot()
            val user = ms.auth.current()
            if (user?.local == true) ms.auth.signOut()
            val wait = max(0L, 520L - (System.currentTimeMillis() - started))
            delay(wait)
            boot.value = true
        }
    }

    fun refreshUsage() {
        // Live Room flows already push counts; this keeps week-window queries fresh on resume.
        viewModelScope.launch {
            ms.settings.snapshot()
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
                if (confirm) authInfo.value = "Check your email to confirm, then sign in."
            }
            .onFailure { authError.value = it.message ?: "Could not create account" }
        authBusy.value = false
    }

    fun resetPassword(email: String) = viewModelScope.launch {
        authBusy.value = true
        authError.value = null
        authInfo.value = null
        runCatching { ms.auth.requestPasswordReset(email) }
            .onSuccess { authInfo.value = "Check your email for a reset link, then sign in." }
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
        val t = trigger.trim(); val e = expansion.trim()
        if (t.isEmpty() || e.isEmpty()) return@launch
        ms.db.snippetDao().upsert(com.maxspeech.android.data.SnippetEntity(t, e))
    }

    fun deleteSnippet(trigger: String) = viewModelScope.launch {
        ms.db.snippetDao().delete(trigger)
    }

    fun addWord(word: String) = viewModelScope.launch {
        val w = word.trim()
        if (w.isNotEmpty()) ms.db.dictionaryDao().insert(com.maxspeech.android.data.DictionaryEntity(w))
    }

    fun deleteWord(word: String) = viewModelScope.launch {
        ms.db.dictionaryDao().delete(word)
    }

    private data class Quad<A, B, C, D>(val s: A, val u: B, val h: C, val p: D)
    private data class Quint<A, B, C, D, E>(val d: A, val b: B, val e: C, val i: D, val t: E)
    private data class Extra<A, B, C, D>(val w: A, val a: B, val sn: C, val dict: D)
}
