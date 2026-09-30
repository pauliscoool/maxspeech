package com.maxspeech.android.pipeline

import android.content.Context
import android.content.ClipboardManager
import android.os.SystemClock
import android.util.Log
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
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull
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
    private val enhance = EnhanceClient(context.assets.open("prompt.txt").bufferedReader().use { it.readText() })
    private val session = DictationSession()
    private var finishJob: Job? = null
    private var startedAt = 0L
    private var sessionLanguage = "en"
    private var sessionTone = "default"
    private var sessionDictionary: List<String> = emptyList()
    private var sessionSnippets: List<Pair<String, String>> = emptyList()

    private val _ui = MutableStateFlow(DictationUi())
    val ui = _ui.asStateFlow()

    private var listenJob: Job? = null
    private var pcmJob: Job? = null
    private val transcript = TranscriptAccumulator()
    private var sessionApp: String = ""
    var pasteIntoFocusedApp: Boolean = false

    fun start(targetApp: String = "", paste: Boolean = false) {
        if (_ui.value.phase == DictationPhase.Listening) return
        val token = session.next()
        confirmToken = null
        finishJob?.cancel()
        audio.stop()
        stt.close()
        startedAt = SystemClock.elapsedRealtime()
        pasteIntoFocusedApp = paste
        sessionApp = targetApp.ifBlank { TextInjector.foregroundPackage() ?: "MaxSpeech" }
        listenJob?.cancel()
        transcript.clear()
        listenJob = scope.launch {
            val snap = settings.snapshot()
            val user = auth.current()
            val used = db.usageDao().wordsSince(PlanCalculator.weekStartUtc())
            val plan = PlanCalculator.from(user, used)
            if (!plan.canDictate) {
                _ui.value = DictationUi(phase = DictationPhase.Limit, error = "Weekly word limit reached")
                delay(2400)
                if (session.isCurrent(token)) _ui.value = DictationUi()
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
            sessionDictionary = dict
            sessionLanguage = lang
            sessionTone = resolveTone(sessionApp, snap.toneOverride)
            sessionSnippets = db.snippetDao().all().map { it.trigger to expandTemplate(it.expansion) }
            if (!session.isCurrent(token)) return@launch
            val keys = com.maxspeech.android.data.Secrets.deepgramKeys(snap.deepgramKey.ifBlank { null })
            try {
                stt.connect(keys, lang, DeepgramClient.BUILTIN_KEYTERMS + dict)
                val streamToken = stt.sessionId
                stt.awaitOpen()
                launch {
                    stt.chunks.collect { chunk ->
                        if (!session.isCurrent(token) || chunk.sessionId != streamToken) return@collect
                        transcript.add(chunk.text, chunk.isFinal)
                        _ui.value = _ui.value.copy(liveText = transcript.text())
                    }
                }
                pcmJob = launch {
                    audio.start(
                        onPcm = { if (session.isCurrent(token)) stt.sendPcm(it) },
                        onLevel = onLevel@{ lvl ->
                            if (!session.isCurrent(token)) return@onLevel
                            val next = _ui.value.levels.toMutableList()
                            next.removeAt(0)
                            next += lvl
                            _ui.value = _ui.value.copy(levels = next)
                        },
                    )
                }
                launch {
                    delay(120_000)
                    if (session.isCurrent(token) && _ui.value.phase == DictationPhase.Listening) {
                        stopAndFinish()
                    }
                }
                pcmJob?.join()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (!session.isCurrent(token) || _ui.value.phase != DictationPhase.Listening) return@launch
                _ui.value = DictationUi(phase = DictationPhase.Error, error = e.message ?: "Mic / STT failed")
                delay(2200)
                if (session.isCurrent(token)) _ui.value = DictationUi()
            }
        }
    }

    fun cancel() {
        session.next()
        confirmToken = null
        finishJob?.cancel()
        listenJob?.cancel()
        pcmJob?.cancel()
        audio.stop()
        stt.close()
        _ui.value = DictationUi()
    }

    fun stopAndFinish() {
        if (_ui.value.phase != DictationPhase.Listening) return
        finishJob = scope.launch { finishInternal(confirmOnly = true) }
    }

    fun confirmPaste() {
        if (_ui.value.phase != DictationPhase.Confirm) return
        val token = confirmToken ?: return
        val text = _ui.value.finalText
        if (text.isNotEmpty() && pasteIntoFocusedApp && session.claimPaste(token)) {
            TextInjector.insert(context, confirmInjectionText)
        }
        _ui.value = DictationUi()
    }

    private var confirmToken: Long? = null
    private var confirmInjectionText = ""

    private suspend fun finishInternal(confirmOnly: Boolean) {
        val token = session.beginFinish() ?: return
        val releasedAt = SystemClock.elapsedRealtime()
        val durationSecs = (releasedAt - startedAt) / 1000.0
        val target = sessionApp
        val paste = pasteIntoFocusedApp
        _ui.value = _ui.value.copy(phase = DictationPhase.Processing)
        pcmJob?.cancel()
        audio.stop()
        stt.finish()
        val snap = settings.snapshot()
        delay(280)
        if (!session.isCurrent(token)) return
        stt.close()
        val raw = transcript.text()
        if (raw.isBlank()) {
            _ui.value = DictationUi()
            return
        }
        _ui.value = _ui.value.copy(originalText = raw)
        val processingStarted = SystemClock.elapsedRealtime()
        val expanded = FaithfulDictation.expandExplicit(raw, sessionDictionary, sessionSnippets)
        val multilingual = sessionLanguage == "multi" || FaithfulDictation.hasNonLatinScript(expanded)
        val local = FaithfulDictation.localCleanup(expanded, sessionTone, sessionLanguage.startsWith("en") && !multilingual)
        var out = local
        if (local.isNotBlank() && snap.aiEnhance && snap.llmKey.isNotBlank() && !multilingual &&
            durationSecs >= EnhanceClient.quickSkipSecs(snap.enhanceSpeed)
        ) {
            val remaining = (420 - (SystemClock.elapsedRealtime() - processingStarted)).coerceAtLeast(0)
            val candidate = withTimeoutOrNull(remaining) {
                enhance.enhance(local, sessionTone, snap.llmKey, snap.enhanceSpeed, multilingual, sessionDictionary, remaining)
            }
            if (candidate == null) Log.w("MaxSpeech", "Dictation enhance fallback reason=deadline")
            out = candidate ?: local
        }
        if (!session.isCurrent(token)) return
        val enhanced = out != raw
        if (out.isEmpty()) {
            _ui.value = DictationUi()
            return
        }
        val isFormattingCommand = sessionLanguage.startsWith("en") && FaithfulDictation.formattingCommand(raw) != null
        val injectionText = if (snap.trailingSpace && !isFormattingCommand) "$out " else out
        withContext(Dispatchers.IO) {
            db.historyDao().insert(HistoryEntity(text = out, appName = friendlyApp(target), enhanced = enhanced))
            db.usageDao().insert(UsageEntity(wordCount = PlanCalculator.wordCount(out)))
        }
        if (!session.isCurrent(token)) return
        val confirm = confirmOnly && (snap.overlayConfirm || !paste)
        confirmToken = token
        confirmInjectionText = injectionText
        _ui.value = _ui.value.copy(
            phase = if (confirm) DictationPhase.Confirm else DictationPhase.Idle,
            finalText = out,
            originalText = raw,
            liveText = out,
        )
        Log.i("MaxSpeech", "Dictation ready release_to_ready_ms=${SystemClock.elapsedRealtime() - releasedAt} processing_ms=${SystemClock.elapsedRealtime() - processingStarted}")
        if (!confirm && paste && session.claimPaste(token)) {
            TextInjector.insert(context, injectionText)
            delay(400)
            if (session.isCurrent(token)) _ui.value = DictationUi()
        }
    }

    private fun expandTemplate(template: String): String {
        val now = java.util.Date()
        var text = template
        if ("{clipboard}" in text) {
            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            val value = clipboard.primaryClip?.getItemAt(0)?.coerceToText(context)?.toString().orEmpty()
            text = text.replace("{clipboard}", value)
        }
        return text.replace("{date}", java.text.SimpleDateFormat("yyyy-MM-dd", java.util.Locale.ROOT).format(now))
            .replace("{time}", java.text.SimpleDateFormat("HH:mm", java.util.Locale.ROOT).format(now))
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
