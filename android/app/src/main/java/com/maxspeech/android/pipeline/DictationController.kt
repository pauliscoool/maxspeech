package com.maxspeech.android.pipeline

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppDatabase
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthRepository
import com.maxspeech.android.data.DictionaryEntity
import com.maxspeech.android.data.SubstitutionEntity
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.PlanCalculator
import com.maxspeech.android.data.SettingsRepository
import com.maxspeech.android.data.UsageEntity
import com.maxspeech.android.overlay.OverlayService
import java.io.IOException
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.sin
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

enum class DictationPhase { Idle, Listening, Processing, Confirm, Error, Limit }

data class DictationUi(
    val phase: DictationPhase = DictationPhase.Idle,
    val levels: List<Float> = List(BAR_COUNT) { 0.14f },
    val liveText: String = "",
    val finalText: String = "",
    val originalText: String = "",
    val error: String? = null,
    val targetApp: String = "",
)

private const val BAR_COUNT = AudioCapture.BAR_COUNT
private const val TAG = "MaxSpeechDictation"
private const val THINKING_DIG_AFTER_S = 3f
private const val STREAM_OPEN_TIMEOUT_MS = 6_000L
private const val SAVED_RETRY_WINDOW_MS = 60_000L
private const val AUTO_RETRY_PAUSE_MS = 10_000L
/** ~0.25s of 16 kHz 16-bit mono. */
private const val MIN_PCM_BYTES = 16_000 * 2 / 4

class DictationController(
    private val context: Context,
    private val db: AppDatabase,
    private val settings: SettingsRepository,
    private val auth: AuthRepository,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val audio = AudioCapture()
    private val stt = DeepgramClient()
    private val enhance = EnhanceClient(context.assets)
    private val soundCue = SoundCue(context)

    private val _ui = MutableStateFlow(DictationUi())
    val ui = _ui.asStateFlow()

    private var listenJob: Job? = null
    private var pcmJob: Job? = null
    private var transcriptJob: Job? = null
    private var thinkingJob: Job? = null
    private var errorDismissJob: Job? = null
    // Tracked so cancel / a new session stops a stale finish from pasting or closing the new socket.
    private var finishJob: Job? = null
    private var autoRetryJob: Job? = null
    private val finals = StringBuilder()
    private val lastInterim = StringBuilder()
    private var sessionApp: String = ""
    private val smoothed = FloatArray(BAR_COUNT) { 0.14f }
    var pasteIntoFocusedApp: Boolean = false
    private var listenStartedAtMs: Long = 0L
    private val finishing = AtomicBoolean(false)
    @Volatile private var cachedSnap: AppSettings = AppSettings()
    private var lastLevelUiAt = 0L
    @Volatile private var dictionary: List<String> = emptyList()
    @Volatile private var snippets: List<Pair<String, String>> = emptyList()
    @Volatile private var learned: List<Vocab.Substitution> = emptyList()
    private val builtinKeyterms by lazy { Vocab.loadKeyterms(context.assets) }
    private val phraseFixes by lazy { Vocab.loadPhraseFixes(context.assets) }
    private val pcmLock = Any()
    private val pcmCapture = java.io.ByteArrayOutputStream()
    private var sessionKeys: List<String> = emptyList()
    private var sessionLang: String = "en"
    private var lastPasteText: String? = null
    private var lastPasteAt = 0L
    private var pendingLearnFrom: String? = null
    private var savedPcm: ByteArray? = null
    private var savedHeldMs = 0L
    private var savedConfirmOnly = false

    init {
        scope.launch {
            settings.flow.collect { cachedSnap = it }
        }
        scope.launch { db.dictionaryDao().observe().collect { dictionary = it } }
        scope.launch {
            db.snippetDao().observe().collect { rows -> snippets = rows.map { it.trigger to it.expansion } }
        }
        scope.launch {
            db.substitutionDao().observe().collect { rows ->
                learned = rows.map { Vocab.Substitution(it.fromText, it.toText) }
            }
        }
    }

    fun start(targetApp: String = "", paste: Boolean = false) {
        if (_ui.value.phase == DictationPhase.Listening ||
            _ui.value.phase == DictationPhase.Processing
        ) {
            return
        }
        errorDismissJob?.cancel()
        finishJob?.cancel()
        finishing.set(false)
        savedPcm = null
        pasteIntoFocusedApp = paste
        sessionApp = targetApp.ifBlank { TextInjector.foregroundPackage() ?: sessionApp.ifBlank { "MaxSpeech" } }
        listenJob?.cancel()
        thinkingJob?.cancel()
        pcmJob?.cancel()
        transcriptJob?.cancel()
        transcriptJob = null
        audio.stop()
        finals.clear()
        lastInterim.clear()
        synchronized(pcmLock) { pcmCapture.reset() }
        for (i in smoothed.indices) smoothed[i] = 0.14f
        lastLevelUiAt = 0L
        // Always enter Listening first — never flash X/Retry in place of the mic.
        listenStartedAtMs = System.currentTimeMillis()
        // Re-dictating right after a paste usually means "that was wrong" — learn from it.
        pendingLearnFrom = lastPasteText.takeIf { listenStartedAtMs - lastPasteAt <= Vocab.LEARN_WINDOW_MS }
        _ui.value = DictationUi(phase = DictationPhase.Listening, targetApp = sessionApp)
        OverlayService.notifyRecording(context, true)
        haptic(HapticKind.Start)
        playSoundCue()
        listenJob = scope.launch {
            try {
                // Warm path: use cached settings so STT + mic start without waiting on disk.
                val snap = cachedSnap
                val lang = if (snap.multilingual &&
                    com.maxspeech.android.data.SttLanguages.multilingualAllowed(
                        PlanCalculator.from(auth.current(), 0).tier,
                    )
                ) {
                    "multi"
                } else {
                    snap.languages.firstOrNull() ?: "en"
                }
                val keys = com.maxspeech.android.data.Secrets.deepgramKeys(snap.deepgramKey.ifBlank { null })
                // Handshake + mic immediately — PCM buffers until the socket opens.
                sessionKeys = keys
                sessionLang = lang
                stt.connect(keys, lang, Vocab.mergeKeyterms(dictionary, builtinKeyterms))
                transcriptJob = scope.launch {
                    stt.chunks.collect { chunk ->
                        if (_ui.value.phase != DictationPhase.Listening &&
                            _ui.value.phase != DictationPhase.Processing
                        ) {
                            return@collect
                        }
                        TranscriptMerge.apply(finals, lastInterim, chunk.text, chunk.isFinal)
                        val shown = TranscriptMerge.display(finals, lastInterim)
                        _ui.value = _ui.value.copy(liveText = shown)
                    }
                }
                pcmJob = launch {
                    try {
                        audio.start(
                            onPcm = {
                                // Keep a local recovery copy before attempting the live socket.
                                recordPcm(it)
                                stt.sendPcm(it)
                            },
                            onLevel = { bands ->
                                if (_ui.value.phase != DictationPhase.Listening) return@start
                                val now = SystemClock.uptimeMillis()
                                // ~30fps — full recomposition every PCM frame was the "lag spikes".
                                if (now - lastLevelUiAt < 32L) return@start
                                lastLevelUiAt = now
                                val next = MutableList(BAR_COUNT) { 0.14f }
                                for (i in 0 until BAR_COUNT) {
                                    val src = if (bands.size == BAR_COUNT) {
                                        bands[i]
                                    } else {
                                        val t = i / (BAR_COUNT - 1).toFloat()
                                        val idx = (t * (bands.size - 1)).toInt()
                                            .coerceIn(0, bands.lastIndex)
                                        bands.getOrElse(idx) { 0.14f }
                                    }
                                    val target = src.coerceIn(0.12f, 0.98f)
                                    val prev = smoothed[i]
                                    val alpha = if (target > prev) 0.72f else 0.32f
                                    val v = prev + (target - prev) * alpha
                                    smoothed[i] = v
                                    next[i] = v
                                }
                                _ui.value = _ui.value.copy(levels = next)
                            },
                        )
                        if (isActive && _ui.value.phase == DictationPhase.Listening) {
                            handleCaptureFailure(IOException("Microphone capture stopped unexpectedly."))
                        }
                    } catch (e: kotlinx.coroutines.CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        handleCaptureFailure(e)
                    }
                }
                // Plan / dictionary in parallel — don't block first audio.
                val planOk = async(Dispatchers.IO) {
                    val used = db.usageDao().wordsSince(PlanCalculator.weekStartUtc())
                    val fresh = settings.snapshot()
                    cachedSnap = fresh
                    PlanCalculator.from(auth.current(), used).canDictate
                }
                if (!planOk.await()) {
                    pcmJob?.cancel()
                    audio.stop()
                    stt.close()
                    transcriptJob?.cancel()
                    transcriptJob = null
                    presentError("Weekly word limit reached", autoClearMs = 2_400)
                    return@launch
                }
                launch {
                    delay(120_000)
                    if (_ui.value.phase == DictationPhase.Listening) {
                        stopAndFinish()
                    }
                }
                // A slow or dead socket must not stop the recording: PCM is kept locally and
                // re-transcribed over HTTP when streaming didn't deliver.
                if (!stt.awaitOpen(STREAM_OPEN_TIMEOUT_MS)) Log.w(TAG, "Live stream unavailable, recording locally")
                pcmJob?.join()
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Throwable) {
                thinkingJob?.cancel()
                audio.stop()
                stt.close()
                transcriptJob?.cancel()
                transcriptJob = null
                val msg = e.message.orEmpty()
                if (msg.equals("done", ignoreCase = true) ||
                    msg.equals("closed", ignoreCase = true)
                ) {
                    return@launch
                }
                presentError(e.message ?: "Mic / STT failed")
            }
        }
    }

    private fun handleCaptureFailure(error: Exception) {
        if (_ui.value.phase != DictationPhase.Listening) return
        Log.e(TAG, "Audio capture failed", error)
        val preview = TranscriptMerge.display(finals, lastInterim)
            .ifBlank { _ui.value.liveText }
            .trim()
        if (preview.isNotBlank() || audio.heardVoice) {
            // Stop capture and let the batch path recover whatever the stream missed.
            stopAndFinish()
        } else {
            val pcm = synchronized(pcmLock) { pcmCapture.toByteArray() }
            if (pcm.size >= MIN_PCM_BYTES) {
                savedPcm = pcm
                savedHeldMs = (System.currentTimeMillis() - listenStartedAtMs).coerceAtLeast(0L)
                savedConfirmOnly = !pasteIntoFocusedApp
            }
            presentError(
                buildString {
                    append(
                        error.message?.takeIf { it.isNotBlank() }
                            ?: "Microphone capture failed. Check microphone permission and try again.",
                    )
                    if (savedPcm != null) append(" A recording was saved; tap Retry to recover it.")
                },
            )
        }
    }

    /** Re-run the last session (overlay or in-app Retry). */
    fun retry() {
        if (_ui.value.phase != DictationPhase.Error) return
        savedPcm?.let {
            retrySavedAudio(it)
            return
        }
        errorDismissJob?.cancel()
        OverlayService.notifyRecording(context, true)
        start(sessionApp, paste = pasteIntoFocusedApp)
    }

    fun cancel() {
        haptic(HapticKind.Cancel)
        playSoundCue()
        resetToIdle()
    }

    /** Tear down mic/STT and return to idle — no thinking wave, no error chrome. */
    private fun resetToIdle() {
        finishing.set(false)
        errorDismissJob?.cancel()
        finishJob?.cancel()
        listenJob?.cancel()
        pcmJob?.cancel()
        transcriptJob?.cancel()
        transcriptJob = null
        thinkingJob?.cancel()
        audio.stop()
        stt.close()
        OverlayService.notifyRecording(context, false)
        _ui.value = DictationUi()
    }

    fun stopAndFinish() {
        if (_ui.value.phase != DictationPhase.Listening) return
        if (!finishing.compareAndSet(false, true)) return
        haptic(HapticKind.Stop)
        playSoundCue()
        val preview = TranscriptMerge.display(finals, lastInterim)
            .ifBlank { _ui.value.liveText }
            .trim()
        // Nothing said: no voice on the mic and no words back. Vanish now instead of
        // spinning the thinking wave through the flush + batch retry.
        if (preview.isBlank() && !audio.heardVoice) {
            Log.i(TAG, "No speech detected, dismissing")
            resetToIdle()
            return
        }
        // Don't bail just because nothing has arrived yet — short dictations often
        // haven't gotten an interim/final back from Deepgram by the time the user
        // taps the checkmark. Always flush and let finishInternal decide (it already
        // bails on a still-blank transcript once the Finalize round-trip completes).
        // Kick thinking animation NOW — flush / enhance run underneath.
        _ui.value = _ui.value.copy(
            phase = DictationPhase.Processing,
            originalText = preview,
            liveText = preview,
        )
        startThinkingWave()
        finishJob = scope.launch {
            try {
                finishInternal(confirmOnly = !pasteIntoFocusedApp)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.e(TAG, "Dictation finalization failed", e)
                thinkingJob?.cancel()
                audio.stop()
                pcmJob?.cancel()
                transcriptJob?.cancel()
                transcriptJob = null
                stt.close()
                presentError(e.message?.takeIf { it.isNotBlank() } ?: "Dictation could not be completed.")
            } finally {
                finishing.set(false)
            }
        }
    }

    fun confirmPaste() {
        haptic(HapticKind.Confirm)
        scope.launch {
            val text = _ui.value.finalText
            if (text.isNotBlank() && pasteIntoFocusedApp) {
                val inserted = runCatching { TextInjector.insert(context, text) }.getOrDefault(false)
                if (inserted) {
                    onPasted(text)
                } else {
                    val copied = copyTranscriptToClipboard(text)
                    presentError(
                        if (copied) {
                            "Couldn't paste automatically. The transcript was copied to the clipboard; paste it manually."
                        } else {
                            "Couldn't paste or copy the transcript. It is still shown in MaxSpeech."
                        },
                        transcript = text,
                    )
                    return@launch
                }
            }
            OverlayService.notifyRecording(context, false)
            _ui.value = DictationUi()
        }
    }

    private suspend fun finishInternal(confirmOnly: Boolean) {
        // Already flipped to Processing in stopAndFinish for instant UI.
        if (_ui.value.phase != DictationPhase.Processing &&
            _ui.value.phase != DictationPhase.Listening
        ) {
            return
        }
        if (_ui.value.phase == DictationPhase.Listening) {
            val preview = TranscriptMerge.display(finals, lastInterim)
                .ifBlank { _ui.value.liveText }
                .trim()
            _ui.value = _ui.value.copy(
                phase = DictationPhase.Processing,
                originalText = preview,
                liveText = preview,
            )
            startThinkingWave()
        }

        val snap = cachedSnap
        val heldMs = (System.currentTimeMillis() - listenStartedAtMs).coerceAtLeast(0L)
        // Keep recording briefly so the tail of a short word isn't clipped.
        val trailMs = if (heldMs < 5_000L) 200L else 120L
        delay(trailMs)
        audio.stop()
        pcmJob?.cancel()
        val streamComplete = stt.finishAndFlush()
        // Let the chunk collector apply the last final before we merge.
        delay(80)
        transcriptJob?.cancelAndJoin()
        transcriptJob = null
        val merged = TranscriptMerge.mergeTrailing(finals.toString(), lastInterim.toString())
        // Finals are Deepgram's corrected words. The longest-string pick used to win with
        // stale interim guesses that Deepgram had already revised (words never said).
        var raw = merged.trim().ifBlank {
            listOf(_ui.value.liveText, _ui.value.originalText)
                .maxByOrNull { it.trim().length }
                ?.trim()
                .orEmpty()
        }
        Log.i(TAG, "finish: stream=${if (streamComplete) "complete" else "incomplete"} words=${raw.length} voice=${audio.heardVoice}")
        // Streaming missed audio (weak network) or heard nothing: re-transcribe the recording.
        if (raw.isBlank() || !streamComplete) {
            val pcm = synchronized(pcmLock) { pcmCapture.toByteArray() }
            val batch = batchFallback(pcm)
            Log.i(TAG, "batch: pcm=${pcm.size}B result=${batch::class.simpleName}")
            when (batch) {
                is BatchResult.Text -> {
                    if (batch.text.isNotBlank()) {
                        raw = batch.text
                    } else if (!streamComplete && raw.isNotBlank()) {
                        preservePartialTranscript(
                            raw,
                            pcm,
                            heldMs,
                            confirmOnly,
                            "Transcription was interrupted.",
                            retryRecording = true,
                        )
                        return
                    }
                }
                is BatchResult.Rejected -> if (raw.isBlank()) {
                    presentError(batch.message)
                    return
                } else {
                    preservePartialTranscript(
                        raw,
                        pcm,
                        heldMs,
                        confirmOnly,
                        "Transcription could not finish: ${batch.message}",
                        retryRecording = false,
                    )
                    return
                }
                BatchResult.Unreachable -> {
                    if (raw.isNotBlank()) {
                        preservePartialTranscript(
                            raw,
                            pcm,
                            heldMs,
                            confirmOnly,
                            "Transcription connection was lost.",
                            retryRecording = true,
                        )
                        return
                    }
                    if (pcm.size >= MIN_PCM_BYTES) {
                        savedPcm = pcm
                        savedHeldMs = heldMs
                        savedConfirmOnly = confirmOnly
                        presentError("No connection. Recording saved, tap Retry.")
                        autoRetryWhenOnline()
                        return
                    }
                    presentError("No transcript was returned. Check the microphone and connection, then try again.")
                    return
                }
            }
        }
        processTranscript(raw, snap, heldMs, confirmOnly)
    }

    private fun preservePartialTranscript(
        partial: String,
        pcm: ByteArray,
        heldMs: Long,
        confirmOnly: Boolean,
        reason: String,
        retryRecording: Boolean,
    ) {
        val retryAudio = pcm.takeIf { retryRecording && it.size >= MIN_PCM_BYTES }
        savedPcm = retryAudio
        savedHeldMs = heldMs
        savedConfirmOnly = confirmOnly
        val copied = copyTranscriptToClipboard(partial)
        val copyStatus = if (copied) {
            "Partial transcript copied to the clipboard."
        } else {
            "Partial transcript is still available in MaxSpeech."
        }
        val retryStatus = if (retryAudio != null) {
            "Tap Retry to recover the full recording."
        } else {
            "Please start a new dictation to try again."
        }
        presentError("$reason $copyStatus $retryStatus", transcript = partial)
        if (retryAudio != null) autoRetryWhenOnline()
    }

    private suspend fun processTranscript(raw: String, snap: AppSettings, heldMs: Long, confirmOnly: Boolean) {
        if (raw.isNotBlank() && handleVoiceCommand(raw, snap)) return
        if (raw.isBlank()) {
            // The mic heard a voice but no words came back: say so instead of vanishing.
            if (audio.heardVoice) {
                thinkingJob?.cancel()
                presentError("Didn't catch that. Try again.")
                return
            }
            // Flush still empty — stop quietly; no error chip / retry delay.
            resetToIdle()
            return
        }
        _ui.value = _ui.value.copy(originalText = raw, liveText = raw)

        // Same order as desktop pipeline/mod.rs:
        // snippets/dictionary/learned → local cleanup → AI enhance → punctuation.
        val expanded = Vocab.expand(
            raw,
            snippets = snippets,
            dictionary = dictionary,
            phraseFixes = phraseFixes,
            learned = learned,
            clipboard = ::clipboardText,
        )
        val multilingual = snap.multilingual || LocalCleanup.hasNonLatinScript(expanded)
        val corrected = if (multilingual) expanded else LocalCleanup.localSelfCorrect(expanded)
        // Spoken self-corrections ("Sarah I mean Sandra") teach the dictionary too.
        if (corrected.trim() != expanded.trim()) learnFrom(expanded, corrected)
        val tone = resolveTone(sessionApp, snap.toneOverride)
        var out = corrected
        // Desktop parity: short holds get local cleanup only; multilingual sessions skip the LLM.
        val heldSecs = heldMs / 1000.0
        val runEnhance = snap.aiEnhance && sessionLang != "multi" &&
            heldSecs >= EnhancePolicy.quickSkipSecs(snap.enhanceSpeed)
        if (runEnhance) {
            val key = snap.llmKey.trim()
            if (key.isNotBlank()) {
                runCatching {
                    out = enhance.enhance(
                        text = corrected,
                        tone = tone,
                        apiKey = key,
                        speed = snap.enhanceSpeed,
                        multilingual = multilingual,
                        dictTerms = dictionary,
                    )
                }.onFailure {
                    out = corrected
                }
            }
        }
        if (!multilingual) {
            out = LocalCleanup.fixCasualAddressCommas(out)
            out = LocalCleanup.normalizeTerminalPunctuation(out, tone)
        }

        if (snap.trailingSpace && !out.endsWith(" ")) out = "$out "
        val enhanced = out.trim() != raw.trim()
        val historyText = out.trim()
        val historyApp = friendlyApp(sessionApp)
        val wordCount = PlanCalculator.wordCount(out)
        thinkingJob?.cancel()
        stt.close()

        val confirm = confirmOnly && (snap.overlayConfirm || !pasteIntoFocusedApp)
        if (confirm) {
            _ui.value = _ui.value.copy(
                phase = DictationPhase.Confirm,
                finalText = historyText,
                originalText = raw,
                liveText = historyText,
                levels = List(BAR_COUNT) { 0.14f },
            )
            saveHistoryAndUsage(historyText, historyApp, enhanced, wordCount)
            return
        }

        var pasteFailure: String? = null
        if (pasteIntoFocusedApp && out.isNotBlank()) {
            val inserted = runCatching { TextInjector.insert(context, historyText) }.getOrDefault(false)
            if (inserted) {
                onPasted(historyText)
            } else {
                val copied = copyTranscriptToClipboard(historyText)
                pasteFailure = if (copied) {
                    "Couldn't paste automatically. The transcript was copied to the clipboard; paste it manually."
                } else {
                    "Couldn't paste or copy the transcript. It is still shown in MaxSpeech."
                }
            }
        }
        OverlayService.notifyRecording(context, false)
        if (pasteFailure == null) {
            _ui.value = DictationUi()
        } else {
            presentError(pasteFailure, transcript = historyText)
        }
        saveHistoryAndUsage(historyText, historyApp, enhanced, wordCount)
    }

    private fun saveHistoryAndUsage(text: String, appName: String, enhanced: Boolean, wordCount: Int) {
        scope.launch(Dispatchers.IO) {
            try {
                db.historyDao().insert(HistoryEntity(text = text, appName = appName, enhanced = enhanced))
                db.usageDao().insert(UsageEntity(wordCount = wordCount))
            } catch (e: Exception) {
                Log.e(TAG, "Could not save completed dictation", e)
                withContext(Dispatchers.Main.immediate) {
                    if (_ui.value.phase == DictationPhase.Idle || _ui.value.phase == DictationPhase.Confirm) {
                        presentError(
                            "Dictation was processed, but History or usage could not be fully saved.",
                            transcript = text,
                        )
                    }
                }
            }
        }
    }

    private fun recordPcm(samples: ShortArray) {
        synchronized(pcmLock) {
            // 16-bit LE, capped at the 2-minute session limit (~3.8 MB).
            if (pcmCapture.size() > 16_000 * 2 * 125) return
            for (s in samples) {
                pcmCapture.write(s.toInt() and 0xFF)
                pcmCapture.write((s.toInt() shr 8) and 0xFF)
            }
        }
    }

    private sealed interface BatchResult {
        data class Text(val text: String) : BatchResult
        /** Server refused (bad key, quota, bad audio): retrying the same audio can't help. */
        data class Rejected(val message: String) : BatchResult
        object Unreachable : BatchResult
    }

    /** Re-transcribe the recording over HTTP (desktop parity). */
    private suspend fun batchFallback(pcm: ByteArray): BatchResult {
        // Need at least ~0.25s of audio to be worth a request.
        if (pcm.size < MIN_PCM_BYTES) return BatchResult.Text("")
        return try {
            BatchResult.Text(
                stt.transcribeBatch(pcm, sessionKeys, sessionLang, Vocab.mergeKeyterms(dictionary, builtinKeyterms)),
            )
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: java.io.IOException) {
            Log.w(TAG, "batch transcription unreachable", e)
            BatchResult.Unreachable
        } catch (e: Exception) {
            Log.w(TAG, "batch transcription rejected", e)
            BatchResult.Rejected(e.message ?: "Speech service error")
        }
    }

    /** Saved recording on screen: send it by itself once the phone is back online. */
    private fun autoRetryWhenOnline() {
        autoRetryJob?.cancel()
        autoRetryJob = scope.launch {
            val cm = context.getSystemService(android.net.ConnectivityManager::class.java)
            val startedAt = SystemClock.elapsedRealtime()
            var wasOffline = !isOnline(cm)
            while (SystemClock.elapsedRealtime() - startedAt < SAVED_RETRY_WINDOW_MS - 5_000L) {
                delay(2_000L)
                val pcm = savedPcm ?: return@launch
                if (_ui.value.phase != DictationPhase.Error) return@launch
                val online = isOnline(cm)
                if (!online) {
                    wasOffline = true
                    continue
                }
                // Retry right when the network returns, or after a pause on a flaky link.
                if (wasOffline || SystemClock.elapsedRealtime() - startedAt >= AUTO_RETRY_PAUSE_MS) {
                    Log.i(TAG, "Network back — retrying saved recording")
                    retrySavedAudio(pcm)
                    return@launch
                }
            }
        }
    }

    private fun isOnline(cm: android.net.ConnectivityManager?): Boolean {
        cm ?: return true
        val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return false
        return caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_VALIDATED)
    }

    /** Retry a recording that couldn't be sent: transcribe the saved audio instead of re-recording. */
    private fun retrySavedAudio(pcm: ByteArray) {
        errorDismissJob?.cancel()
        finishing.set(true)
        OverlayService.notifyRecording(context, true)
        _ui.value = DictationUi(phase = DictationPhase.Processing, targetApp = sessionApp)
        startThinkingWave()
        finishJob = scope.launch {
            try {
                when (val batch = batchFallback(pcm)) {
                    is BatchResult.Text -> {
                        savedPcm = null
                        processTranscript(batch.text, cachedSnap, savedHeldMs, savedConfirmOnly)
                    }
                    is BatchResult.Rejected -> {
                        savedPcm = null
                        thinkingJob?.cancel()
                        presentError(batch.message)
                    }
                    BatchResult.Unreachable -> {
                        thinkingJob?.cancel()
                        presentError("Still no connection. Recording kept, tap Retry.")
                        autoRetryWhenOnline()
                    }
                }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.e(TAG, "Saved dictation recovery failed", e)
                thinkingJob?.cancel()
                presentError(e.message?.takeIf { it.isNotBlank() } ?: "Saved recording could not be recovered.")
            } finally {
                finishing.set(false)
            }
        }
    }

    /** Returns true when [raw] was a voice command and has been handled. */
    private suspend fun handleVoiceCommand(raw: String, snap: AppSettings): Boolean {
        if (!pasteIntoFocusedApp) return false
        val cmd = VoiceCommands.check(raw) ?: return false
        when (cmd) {
            is VoiceCommands.Result.ScratchThat -> {
                lastPasteText?.let { TextInjector.replaceLast(it, "") }
                lastPasteText = null
            }
            is VoiceCommands.Result.InsertText -> TextInjector.insert(context, cmd.text)
            is VoiceCommands.Result.Rewrite -> {
                val old = lastPasteText
                val key = snap.llmKey.trim()
                if (old != null && key.isNotBlank()) {
                    // Rewrite first, only then swap — a failed LLM call must never delete the text.
                    runCatching { enhance.rewrite(old, cmd.instruction, key) }.getOrNull()
                        ?.takeIf { it.isNotBlank() }
                        ?.let { rewritten ->
                            if (TextInjector.replaceLast(old, rewritten.trim())) lastPasteText = rewritten.trim()
                        }
                }
            }
        }
        resetToIdle()
        return true
    }

    private fun onPasted(text: String) {
        pendingLearnFrom?.let { prev ->
            val pairs = Vocab.redictateCorrections(prev, text)
            if (pairs.isNotEmpty()) persistLearned(pairs, Vocab.namesIn(pairs))
        }
        pendingLearnFrom = null
        lastPasteText = text
        lastPasteAt = System.currentTimeMillis()
    }

    private fun learnFrom(before: String, after: String) {
        persistLearned(
            Vocab.substitutionsFromRedictate(before, after),
            Vocab.learnNameCorrections(before, after),
        )
    }

    private fun persistLearned(pairs: List<Vocab.Substitution>, names: List<String>) {
        if (pairs.isEmpty() && names.isEmpty()) return
        scope.launch(Dispatchers.IO) {
            runCatching {
                for (p in pairs) {
                    db.substitutionDao().upsert(SubstitutionEntity(p.from, p.to))
                    Log.i(TAG, "Learned substitution: ${p.from} → ${p.to}")
                }
                for (n in (names + Vocab.namesIn(pairs)).distinct()) {
                    db.dictionaryDao().insert(DictionaryEntity(n))
                }
            }.onFailure { Log.w(TAG, "learn failed", it) }
        }
    }

    private fun clipboardText(): String = runCatching {
        val cm = context.getSystemService(ClipboardManager::class.java)
        cm.primaryClip?.getItemAt(0)?.coerceToText(context)?.toString()
    }.getOrNull().orEmpty()

    private fun copyTranscriptToClipboard(text: String): Boolean = runCatching {
        val cm = context.getSystemService(ClipboardManager::class.java)
        cm.setPrimaryClip(ClipData.newPlainText("MaxSpeech transcript", text))
        true
    }.getOrDefault(false)

    private suspend fun resolveTone(pkg: String, override: String): String {
        if (override.isNotBlank() && override != "default") return override
        val profiles = db.profileDao().all().filter { it.enabled }
        val hit = profiles
            .filter { pkg.contains(it.packagePattern, ignoreCase = true) }
            .maxByOrNull { it.packagePattern.length }
        return hit?.tone ?: "default"
    }

    private fun presentError(
        message: String,
        autoClearMs: Long? = null,
        transcript: String? = null,
    ) {
        OverlayService.notifyRecording(context, false)
        val snippet = transcript?.trim()?.takeIf { it.isNotBlank() } ?: listOf(
            _ui.value.liveText.trim(),
            _ui.value.finalText.trim(),
            _ui.value.originalText.trim(),
        ).firstOrNull { it.isNotBlank() }.orEmpty()
        val appLabel = friendlyApp(sessionApp)
        _ui.value = DictationUi(
            phase = DictationPhase.Error,
            error = message,
            targetApp = sessionApp,
            liveText = snippet,
            finalText = transcript.orEmpty(),
            originalText = snippet,
        )
        scope.launch(Dispatchers.IO) {
            runCatching {
                db.historyDao().insert(
                    HistoryEntity(
                        text = snippet.ifBlank { message },
                        appName = appLabel,
                        failed = true,
                        errorMessage = message,
                    ),
                )
            }.onFailure {
                Log.e(TAG, "Could not save dictation failure to history", it)
            }
        }
        errorDismissJob?.cancel()
        errorDismissJob = autoClearMs?.takeIf { it > 0L }?.let { clearAfter ->
            scope.launch {
                delay(clearAfter)
                if (_ui.value.phase == DictationPhase.Error) {
                    _ui.value = DictationUi()
                }
            }
        }
    }

    private fun startThinkingWave() {
        thinkingJob?.cancel()
        thinkingJob = scope.launch {
            val t0 = System.nanoTime()
            while (isActive && _ui.value.phase == DictationPhase.Processing) {
                val t = (System.nanoTime() - t0) / 1_000_000_000f
                val next = List(BAR_COUNT) { i -> thinkingBarLevel(t, i) }
                for (i in smoothed.indices) smoothed[i] = next[i]
                _ui.value = _ui.value.copy(levels = next)
                delay(20)
            }
        }
    }

    private fun haptic(kind: HapticKind = HapticKind.Start) {
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
            if (Build.VERSION.SDK_INT < 26) return@launch
            val (ms, amp) = when (kind) {
                HapticKind.Start -> 12 to 48
                HapticKind.Stop, HapticKind.Cancel -> 10 to 40
                HapticKind.Confirm -> 7 to 26
            }
            vib.vibrate(VibrationEffect.createOneShot(ms.toLong(), amp.coerceIn(1, 255)))
        }
    }

    /** Bubble-click for start / stop when Settings → Start / stop sound is on. */
    private fun playSoundCue() {
        if (!cachedSnap.soundCue) return
        soundCue.play()
    }

    /** Preview from Settings when the user enables the cue. */
    fun previewSoundCue() {
        soundCue.play()
    }

    private enum class HapticKind { Start, Stop, Cancel, Confirm }

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

        /** Exact desktop Overlay.tsx thinkingBarLevel. */
        fun thinkingBarLevel(t: Float, i: Int): Float {
            val n = (BAR_COUNT - 1).toFloat()
            val pulse = pingPong01(t, 1.05f)
            val a1 = 0.14f + gaussian(i - pulse * n, 1.65f) * 0.8f
            val t2 = max(0f, t - THINKING_DIG_AFTER_S)
            val dig = pingPong01(t2, 1.9f)
            val pos = dig * n
            val wide = gaussian(i - pos, 3.5f)
            val counter = gaussian(i - (n - pos), 1.5f)
            val floor = 0.13f + 0.05f * (0.5f + 0.5f * sin(t2 * 0.9f))
            val a2 = floor + wide * 0.55f + counter * 0.28f
            if (t < THINKING_DIG_AFTER_S) return minOf(0.98f, a1)
            val k = minOf(1f, (t - THINKING_DIG_AFTER_S) / 0.4f)
            val eased = k * k * (3f - 2f * k)
            return minOf(0.98f, a1 + (a2 - a1) * eased)
        }

        private fun pingPong01(t: Float, oneWayS: Float): Float {
            val cycle = oneWayS * 2f
            val x = ((t % cycle) + cycle) % cycle
            return if (x < oneWayS) x / oneWayS else 2f - x / oneWayS
        }

        private fun gaussian(dist: Float, sigma: Float): Float =
            exp(-(dist * dist) / (2f * sigma * sigma))
    }
}
