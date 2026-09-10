package com.maxspeech.android.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.maxspeech.android.MaxSpeechApp
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppProfileEntity
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthUser
import com.maxspeech.android.data.EnhanceSpeed
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.PlanCalculator
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.UiTheme
import com.maxspeech.android.pipeline.DictationUi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

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
    val authBusy: Boolean = false,
    val authError: String? = null,
    val authInfo: String? = null,
    val toast: String? = null,
)

class AppViewModel(app: Application) : AndroidViewModel(app) {
    private val ms = app as MaxSpeechApp
    private val authBusy = MutableStateFlow(false)
    private val authError = MutableStateFlow<String?>(null)
    private val authInfo = MutableStateFlow<String?>(null)
    private val toast = MutableStateFlow<String?>(null)
    private val words = MutableStateFlow(0)
    private val apps = MutableStateFlow(0)

    val state: StateFlow<UiState> = combine(
        combine(ms.settings.flow, ms.auth.user, ms.db.historyDao().observe(), ms.db.profileDao().observe()) { s, u, h, p ->
            Quad(s, u, h, p)
        },
        combine(ms.dictation.ui, authBusy, authError, authInfo, toast) { d, b, e, i, t ->
            Quint(d, b, e, i, t)
        },
        combine(words, apps, ms.db.snippetDao().observe(), ms.db.dictionaryDao().observe()) { w, a, sn, dict ->
            Extra(w, a, sn, dict)
        },
    ) { quad, quint, extra ->
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
            authBusy = quint.b,
            authError = quint.e,
            authInfo = quint.i,
            toast = quint.t,
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, UiState())

    init {
        refreshUsage()
    }

    fun refreshUsage() {
        viewModelScope.launch {
            words.value = ms.db.usageDao().wordsSince(PlanCalculator.weekStartUtc())
            apps.value = ms.db.historyDao().distinctApps()
        }
    }

    fun holdStart() {
        val pkg = TextInjector.foregroundPackage() ?: "com.maxspeech.android"
        ms.dictation.start(pkg, paste = false)
    }

    fun holdEnd() {
        ms.dictation.stopAndFinish()
        refreshUsage()
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
    fun setSpeed(s: EnhanceSpeed) = viewModelScope.launch { ms.settings.setEnhanceSpeed(s) }
    fun setLlm(v: String) = viewModelScope.launch { ms.settings.setLlmKey(v) }

    fun toggle(key: String, on: Boolean) = viewModelScope.launch {
        when (key) {
            "overlay" -> ms.settings.setOverlayEnabled(on)
            "confirm" -> ms.settings.setOverlayConfirm(on)
            "enhance" -> ms.settings.setAiEnhance(on)
            "multi" -> ms.settings.setMultilingual(on)
            "space" -> ms.settings.setTrailingSpace(on)
            "haptics" -> ms.settings.setHaptics(on)
            "sound" -> ms.settings.setSoundCue(on)
            "live" -> ms.settings.setLiveTranscript(on)
        }
    }

    fun toggleLanguage(code: String) = viewModelScope.launch {
        val cur = ms.settings.snapshot().languages.toMutableList()
        if (cur.contains(code)) {
            if (cur.size > 1) cur.remove(code)
        } else if (cur.size < 5) {
            cur += code
        }
        ms.settings.setLanguages(cur)
    }

    fun deleteHistory(id: Long) = viewModelScope.launch {
        ms.db.historyDao().delete(id)
        refreshUsage()
    }

    fun toggleProfile(p: AppProfileEntity) = viewModelScope.launch {
        ms.db.profileDao().update(p.copy(enabled = !p.enabled))
    }

    fun signIn(email: String, password: String) = viewModelScope.launch {
        authBusy.value = true
        authError.value = null
        runCatching { ms.auth.signIn(email, password) }
            .onFailure { authError.value = it.message }
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
            .onFailure { authError.value = it.message }
        authBusy.value = false
    }

    fun continueLocal() = viewModelScope.launch {
        authBusy.value = true
        ms.auth.signInLocal()
        authBusy.value = false
    }

    fun signOut() = viewModelScope.launch { ms.auth.signOut() }

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
