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
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

enum class DictationPhase { Idle, Starting, Listening, Processing, Confirm, Error, Limit }

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
    private var transcriptJob: Job? = null
    private val transcript = TranscriptAccumulator()
    private var sessionApp: String = ""
    var pasteIntoFocusedApp: Boolean = false

    fun start(targetApp: String = "", paste: Boolean = false) {
        if (_ui.value.phase == DictationPhase.Starting || _ui.value.phase == DictationPhase.Listening ||
            _ui.value.phase == DictationPhase.Processing
        ) return
        val token = session.next()
        confirmToken = null
        finishJob?.cancel()
        audio.stop()
        stt.close()
        startedAt = SystemClock.elapsedRealtime()
        pasteIntoFocusedApp = paste
        sessionApp = targetApp.ifBlank { TextInjector.foregroundPackage() ?: "MaxSpeech" }
        listenJob?.cancel()
        pcmJob?.cancel()
        transcriptJob?.cancel()
        transcriptJob = null
        transcript.clear()
        _ui.value = DictationUi(phase = DictationPhase.Starting, targetApp = sessionApp)
        listenJob = scope.launch {
            var failureJob: Job? = null
            try {
                val snap = settings.snapshot()
                val user = auth.current()
                val used = db.usageDao().wordsSince(PlanCalculator.weekStartUtc())
                val plan = PlanCalculator.from(user, used)
                if (!plan.canDictate) {
                    _ui.value = DictationUi(
                        phase = DictationPhase.Limit,
                        error = "Weekly word limit reached.",
                        targetApp = sessionApp,
                    )
                    return@launch
                }
                haptic()
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
                if (keys.none { it.isNotBlank() }) {
                    throw IllegalStateException("No Deepgram API key is configured.")
                }
                stt.connect(keys, lang, DeepgramClient.BUILTIN_KEYTERMS + dict)
                val streamToken = stt.sessionId
                if (withTimeoutOrNull(15_000) { stt.awaitOpen() } == null) {
                    throw IllegalStateException("Transcription connection timed out while opening.")
                }
                if (!session.isCurrent(token) || _ui.value.phase != DictationPhase.Starting) return@launch
                transcriptJob = scope.launch {
                    stt.chunks.collect { chunk ->
                        if (!session.isCurrent(token) || chunk.sessionId != streamToken) return@collect
                        transcript.add(chunk.text, chunk.isFinal)
                        _ui.value = _ui.value.copy(liveText = transcript.text())
                    }
                }
                failureJob = launch {
                    reportStreamFailure(token, stt.awaitFailure())
                }
                if (!session.isCurrent(token) || _ui.value.phase != DictationPhase.Starting) return@launch
                pcmJob = scope.launch {
                    if (!session.isCurrent(token) || _ui.value.phase != DictationPhase.Starting) return@launch
                    try {
                        audio.start(
                            onStarted = {
                                if (session.isCurrent(token) && _ui.value.phase == DictationPhase.Starting) {
                                    _ui.value = _ui.value.copy(phase = DictationPhase.Listening)
                                }
                            },
                            onPcm = {
                                if (session.isCurrent(token) && _ui.value.phase == DictationPhase.Listening) {
                                    stt.sendPcm(it)
                                }
                            },
                            onLevel = onLevel@{ lvl ->
                                if (!session.isCurrent(token) || _ui.value.phase != DictationPhase.Listening) {
                                    return@onLevel
                                }
                                val next = _ui.value.levels.toMutableList()
                                next.removeAt(0)
                                next += lvl
                                _ui.value = _ui.value.copy(levels = next)
                            },
                        )
                        if (session.isCurrent(token) && _ui.value.phase == DictationPhase.Listening) {
                            reportStreamFailure(token, IllegalStateException("Microphone capture stopped unexpectedly."))
                        }
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        reportStreamFailure(token, e)
                    }
                }
                launch {
                    delay(120_000)
                    if (session.isCurrent(token) && _ui.value.phase == DictationPhase.Listening) {
                        stopAndFinish()
                    }
                }
                pcmJob?.join()
            } catch (e: CancellationException) {
                val cause = e.cause
                if (cause != null && session.isCurrent(token) &&
                    _ui.value.phase in setOf(DictationPhase.Starting, DictationPhase.Listening)
                ) {
                    reportStreamFailure(token, cause)
                } else {
                    throw e
                }
            } catch (e: Exception) {
                reportFailure(token, failureMessage(e), e, transcript.text())
            } finally {
                failureJob?.cancel()
            }
        }
    }

    fun cancel() {
        session.next()
        confirmToken = null
        finishJob?.cancel()
        listenJob?.cancel()
        pcmJob?.cancel()
        transcriptJob?.cancel()
        transcriptJob = null
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
        if (text.isBlank()) {
            reportFailure(token, "There is no transcript to paste. Please dictate again.", null)
            return
        }
        if (!session.claimPaste(token)) return
        try {
            val inserted = TextInjector.insert(context, confirmInjectionText.ifBlank { text })
            if (inserted) {
                _ui.value = DictationUi()
            } else {
                reportFailure(
                    token,
                    "Couldn't paste directly. The transcript was copied to the clipboard; paste it manually.",
                    null,
                    text,
                )
            }
        } catch (e: Exception) {
            reportFailure(
                token,
                "Could not paste or copy the transcript. The text is still available in MaxSpeech.",
                e,
                text,
            )
        }
    }

    private var confirmToken: Long? = null
    private var confirmInjectionText = ""

    private suspend fun finishInternal(confirmOnly: Boolean, streamFailure: Throwable? = null) {
        val token = session.beginFinish() ?: return
        try {
            val releasedAt = SystemClock.elapsedRealtime()
            val durationSecs = (releasedAt - startedAt) / 1000.0
            val target = sessionApp
            val paste = pasteIntoFocusedApp
            _ui.value = _ui.value.copy(phase = DictationPhase.Processing)
            pcmJob?.cancel()
            audio.stop()
            if (streamFailure == null && !stt.finish()) {
                throw IllegalStateException("Transcription connection ended before the final audio was sent.")
            }
            val snap = settings.snapshot()
            if (streamFailure == null) {
                val serverClosed = withTimeoutOrNull(900) { stt.awaitClosed() } != null
                delay(if (serverClosed) 50 else 280)
            } else {
                delay(80)
            }
            if (!session.isCurrent(token)) return
            stt.close()
            transcriptJob?.cancelAndJoin()
            transcriptJob = null
            val raw = transcript.text()
            if (raw.isBlank()) {
                val message = streamFailure?.let(::failureMessage)
                    ?: "No speech was transcribed. Check your microphone and internet connection, then try again."
                reportFailure(token, message, streamFailure)
                return
            }
            _ui.value = _ui.value.copy(originalText = raw)
            val processingStarted = SystemClock.elapsedRealtime()
            val expanded = FaithfulDictation.expandExplicit(raw, sessionDictionary, sessionSnippets)
            val multilingual = sessionLanguage == "multi" || FaithfulDictation.hasNonLatinScript(expanded)
            val local = FaithfulDictation.localCleanup(
                expanded,
                sessionTone,
                sessionLanguage.startsWith("en") && !multilingual,
            )
            var out = local
            if (local.isNotBlank() && snap.aiEnhance && snap.llmKey.isNotBlank() && !multilingual &&
                durationSecs >= EnhanceClient.quickSkipSecs(snap.enhanceSpeed)
            ) {
                val remaining = (420 - (SystemClock.elapsedRealtime() - processingStarted)).coerceAtLeast(0)
                val candidate = withTimeoutOrNull(remaining) {
                    enhance.enhance(
                        local,
                        sessionTone,
                        snap.llmKey,
                        snap.enhanceSpeed,
                        multilingual,
                        sessionDictionary,
                        remaining,
                    )
                }
                if (candidate == null) Log.w("MaxSpeech", "Dictation enhance fallback reason=deadline")
                out = candidate ?: local
            }
            if (!session.isCurrent(token)) return
            val enhanced = out != raw
            if (out.isBlank()) {
                reportFailure(token, "No usable text came back from transcription. Please try again.", streamFailure, raw)
                return
            }
            val isFormattingCommand = sessionLanguage.startsWith("en") && FaithfulDictation.formattingCommand(raw) != null
            val injectionText = if (snap.trailingSpace && !isFormattingCommand) "$out " else out
            val historyFailure = try {
                withContext(Dispatchers.IO) {
                    db.historyDao().insert(HistoryEntity(text = out, appName = friendlyApp(target), enhanced = enhanced))
                    db.usageDao().insert(UsageEntity(wordCount = PlanCalculator.wordCount(out)))
                }
                null
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.e("MaxSpeech", "Could not save dictation history", e)
                "Dictation completed, but MaxSpeech couldn't save all session data."
            }
            if (!session.isCurrent(token)) return
            val confirm = streamFailure != null || (confirmOnly && (snap.overlayConfirm || !paste))
            confirmToken = token
            confirmInjectionText = injectionText
            val status = listOfNotNull(
                historyFailure,
                streamFailure?.let { "Transcription connection dropped. Review the recovered words before pasting." },
            ).joinToString(" ").ifBlank { null }
            _ui.value = _ui.value.copy(
                phase = if (confirm) DictationPhase.Confirm else DictationPhase.Idle,
                finalText = out,
                originalText = raw,
                liveText = out,
                error = status,
            )
            Log.i(
                "MaxSpeech",
                "Dictation ready release_to_ready_ms=${SystemClock.elapsedRealtime() - releasedAt} " +
                    "processing_ms=${SystemClock.elapsedRealtime() - processingStarted}",
            )
            if (!confirm && paste && session.claimPaste(token)) {
                val inserted = TextInjector.insert(context, injectionText)
                if (!inserted) {
                    reportFailure(
                        token,
                        "Couldn't paste directly. The transcript was copied to the clipboard; paste it manually.",
                        null,
                        out,
                    )
                } else if (historyFailure != null) {
                    reportFailure(token, historyFailure, null, out)
                } else {
                    delay(400)
                    if (session.isCurrent(token)) _ui.value = DictationUi()
                }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            val raw = transcript.text()
            transcriptJob?.cancel()
            transcriptJob = null
            val copied = raw.isNotBlank() && TextInjector.copyToClipboard(context, raw)
            val message = if (copied) {
                "Dictation couldn't finish. The recovered words were copied to the clipboard; paste them manually."
            } else {
                failureMessage(e)
            }
            reportFailure(token, message, e, raw)
        }
    }

    private fun reportStreamFailure(token: Long, error: Throwable) {
        if (!session.isCurrent(token) ||
            _ui.value.phase !in setOf(DictationPhase.Starting, DictationPhase.Listening)
        ) return
        Log.e("MaxSpeech", "Dictation stream failed", error)
        audio.stop()
        pcmJob?.cancel()
        finishJob?.cancel()
        finishJob = scope.launch { finishInternal(confirmOnly = true, streamFailure = error) }
    }

    private fun reportFailure(token: Long, message: String, error: Throwable?, transcriptText: String = "") {
        if (!session.isCurrent(token)) return
        if (error != null) Log.e("MaxSpeech", message, error) else Log.w("MaxSpeech", message)
        _ui.value = DictationUi(
            phase = DictationPhase.Error,
            finalText = transcriptText,
            originalText = transcriptText,
            error = message,
            targetApp = sessionApp,
        )
    }

    private fun failureMessage(error: Throwable): String {
        val details = generateSequence(error) { it.cause }
            .joinToString(" ") { "${it::class.simpleName.orEmpty()} ${it.message.orEmpty()}" }
            .lowercase()
        return when {
            "no deepgram api key" in details -> "No Deepgram API key is set. Add one in Settings and try again."
            "http 401" in details || "unauthorized" in details ->
                "Deepgram rejected the API key. Check your key in Settings."
            "http 403" in details || "forbidden" in details ->
                "Deepgram denied the request. Check your API key and account access."
            "clipboard" in details ->
                "Couldn't copy or paste the transcript. The recognized words remain visible in MaxSpeech."
            "unknownhost" in details || "timeout" in details || "timed out" in details ||
                "network is unreachable" in details ||
                "connection" in details || "socket" in details ->
                "Couldn't reach the transcription service. Check your internet connection and try again."
            "microphone" in details || "audiorecord" in details || "record_audio" in details ||
                "permission" in details ->
                "Couldn't access the microphone. Check MaxSpeech's microphone permission and try again."
            else -> "Dictation couldn't finish. Check your connection and microphone, then try again."
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
