package com.maxspeech.android.pipeline

import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppDatabase
import com.maxspeech.android.data.AuthRepository
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.PlanCalculator
import com.maxspeech.android.data.SettingsRepository
import com.maxspeech.android.data.UsageEntity
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

enum class DictationPhase { Idle, Listening, Processing, Confirm, Error, Limit }

data class DictationUi(
    val phase: DictationPhase = DictationPhase.Idle,
    val levels: List<Float> = List(20) { 0.14f },
    val liveText: String = "",
    val finalText: String = "",
    val originalText: String = "",
    val error: String? = null,
    val targetApp: String = "",
)

class DictationController(
    private val context: Context,
    private val db: AppDatabase,
    private val settings: SettingsRepository,
    private val auth: AuthRepository,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val audio = AudioCapture()
    private val stt = DeepgramClient()

    private val _ui = MutableStateFlow(DictationUi())
    val ui = _ui.asStateFlow()

    private var listenJob: Job? = null
    private var pcmJob: Job? = null
    private val finals = StringBuilder()
    private var sessionApp: String = ""
    var pasteIntoFocusedApp: Boolean = false

    fun start(targetApp: String = "", paste: Boolean = false) {
        if (_ui.value.phase == DictationPhase.Listening) return
        pasteIntoFocusedApp = paste
        sessionApp = targetApp.ifBlank { TextInjector.foregroundPackage() ?: "MaxSpeech" }
        listenJob?.cancel()
        finals.clear()
        listenJob = scope.launch {
            val snap = settings.snapshot()
            val user = auth.current()
            val used = db.usageDao().wordsSince(PlanCalculator.weekStartUtc())
            val plan = PlanCalculator.from(user, used)
            if (!plan.canDictate) {
                _ui.value = DictationUi(phase = DictationPhase.Limit, error = "Weekly word limit reached")
                delay(2400)
                _ui.value = DictationUi()
                return@launch
            }
            haptic()
            _ui.value = DictationUi(phase = DictationPhase.Listening, targetApp = sessionApp)
            val lang = if (snap.multilingual && com.maxspeech.android.data.SttLanguages.multilingualAllowed(plan.tier)) {
                "multi"
            } else {
                snap.languages.firstOrNull() ?: "en"
            }
            val dict = db.dictionaryDao().all()
            val keys = com.maxspeech.android.data.Secrets.deepgramKeys(snap.deepgramKey.ifBlank { null })
            try {
                stt.connect(keys, lang, DeepgramClient.BUILTIN_KEYTERMS + dict)
                stt.awaitOpen()
                launch {
                    stt.chunks.collect { chunk ->
                        if (chunk.isFinal) {
                            if (finals.isNotEmpty()) finals.append(' ')
                            finals.append(chunk.text.trim())
                        }
                        val shown = if (chunk.isFinal) finals.toString() else listOf(finals.toString(), chunk.text)
                            .filter { it.isNotBlank() }
                            .joinToString(" ")
                        _ui.value = _ui.value.copy(liveText = shown)
                    }
                }
                pcmJob = launch {
                    audio.start(
                        onPcm = { stt.sendPcm(it) },
                        onLevel = { lvl ->
                            val next = _ui.value.levels.toMutableList()
                            next.removeAt(0)
                            next += lvl
                            _ui.value = _ui.value.copy(levels = next)
                        },
                    )
                }
                launch {
                    delay(120_000)
                    if (_ui.value.phase == DictationPhase.Listening) {
                        finishInternal(confirmOnly = true)
                    }
                }
                pcmJob?.join()
            } catch (e: Throwable) {
                _ui.value = DictationUi(phase = DictationPhase.Error, error = e.message ?: "Mic / STT failed")
                delay(2200)
                _ui.value = DictationUi()
            }
        }
    }

    fun cancel() {
        listenJob?.cancel()
        pcmJob?.cancel()
        audio.stop()
        stt.close()
        _ui.value = DictationUi()
    }

    fun stopAndFinish() {
        scope.launch { finishInternal(confirmOnly = true) }
    }

    fun confirmPaste() {
        scope.launch {
            val text = _ui.value.finalText
            if (text.isNotBlank() && pasteIntoFocusedApp) {
                TextInjector.insert(context, text)
            }
            _ui.value = DictationUi()
        }
    }

    private suspend fun finishInternal(confirmOnly: Boolean) {
        val snap = settings.snapshot()
        audio.stop()
        stt.finish()
        delay(280)
        val raw = _ui.value.liveText.ifBlank { finals.toString() }.trim()
        if (raw.isBlank()) {
            _ui.value = DictationUi()
            return
        }
        _ui.value = _ui.value.copy(phase = DictationPhase.Processing, originalText = raw)
        var out = raw
        delay(420)
        if (snap.trailingSpace && !out.endsWith(" ")) out = "$out "
        val enhanced = out.trim() != raw.trim()
        withContext(Dispatchers.IO) {
            db.historyDao().insert(
                HistoryEntity(
                    text = out.trim(),
                    appName = friendlyApp(sessionApp),
                    enhanced = enhanced,
                ),
            )
            db.usageDao().insert(UsageEntity(wordCount = PlanCalculator.wordCount(out)))
        }
        val confirm = confirmOnly && (snap.overlayConfirm || !pasteIntoFocusedApp)
        _ui.value = _ui.value.copy(
            phase = if (confirm) DictationPhase.Confirm else DictationPhase.Idle,
            finalText = out.trim(),
            originalText = raw,
            liveText = out.trim(),
        )
        if (!confirm && pasteIntoFocusedApp) {
            TextInjector.insert(context, out.trim())
            delay(400)
            _ui.value = DictationUi()
        }
        if (!confirm && !pasteIntoFocusedApp) {
            delay(600)
        }
    }

    private suspend fun resolveTone(pkg: String, override: String): String {
        if (override.isNotBlank() && override != "default") return override
        val profiles = db.profileDao().all().filter { it.enabled }
        val hit = profiles
            .filter { pkg.contains(it.packagePattern, ignoreCase = true) }
            .maxByOrNull { it.packagePattern.length }
        return hit?.tone ?: "default"
    }

    private fun haptic() {
        scope.launch {
            val on = settings.snapshot().haptics
            if (!on) return@launch
            val vib = if (Build.VERSION.SDK_INT >= 31) {
                val mgr = context.getSystemService(VibratorManager::class.java)
                mgr.defaultVibrator
            } else {
                @Suppress("DEPRECATION")
                context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            }
            if (Build.VERSION.SDK_INT >= 26) {
                vib.vibrate(VibrationEffect.createOneShot(28, VibrationEffect.DEFAULT_AMPLITUDE))
            }
        }
    }

    companion object {
        fun friendlyApp(pkg: String): String = when {
            pkg.contains("gm") -> "Gmail"
            pkg.contains("whatsapp") -> "WhatsApp"
            pkg.contains("messaging") -> "Messages"
            pkg.contains("chrome") -> "Chrome"
            pkg.contains("slack") -> "Slack"
            pkg.contains("discord") -> "Discord"
            pkg.contains("instagram") -> "Instagram"
            pkg.contains("linkedin") -> "LinkedIn"
            pkg.contains("telegram") -> "Telegram"
            pkg.contains("docs") -> "Docs"
            pkg.contains("notion") -> "Notion"
            pkg.contains("maxspeech") -> "MaxSpeech"
            else -> pkg.substringAfterLast('.').replaceFirstChar { it.uppercase() }
        }
    }
}
