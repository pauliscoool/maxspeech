package com.maxspeech.android.pipeline

import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import com.maxspeech.android.a11y.TextInjector
import com.maxspeech.android.data.AppDatabase
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.AuthRepository
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.PlanCalculator
import com.maxspeech.android.data.SettingsRepository
import com.maxspeech.android.data.UsageEntity
import com.maxspeech.android.overlay.OverlayService
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.exp
import kotlin.math.max
import kotlin.math.sin
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

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
private const val THINKING_DIG_AFTER_S = 3f
/** How long the Retry affordance stays visible after a failure. */
const val DictationRetryWindowMs = 5_000L
private const val RETRY_WINDOW_MS = DictationRetryWindowMs

class DictationController(
    private val context: Context,
    private val db: AppDatabase,
    private val settings: SettingsRepository,
    private val auth: AuthRepository,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val audio = AudioCapture()
    private val stt = DeepgramClient()
    private val enhance = EnhanceClient()
    private val soundCue = SoundCue(context)

    private val _ui = MutableStateFlow(DictationUi())
    val ui = _ui.asStateFlow()

    private var listenJob: Job? = null
    private var pcmJob: Job? = null
    private var thinkingJob: Job? = null
    private var errorDismissJob: Job? = null
    private val finals = StringBuilder()
    private val lastInterim = StringBuilder()
    private var sessionApp: String = ""
    private val smoothed = FloatArray(BAR_COUNT) { 0.14f }
    var pasteIntoFocusedApp: Boolean = false
    private var listenStartedAtMs: Long = 0L
    private val finishing = AtomicBoolean(false)
    @Volatile private var cachedSnap: AppSettings = AppSettings()
    private var lastLevelUiAt = 0L

    init {
        scope.launch {
            settings.flow.collect { cachedSnap = it }
        }
    }

    fun start(targetApp: String = "", paste: Boolean = false) {
        if (_ui.value.phase == DictationPhase.Listening ||
            _ui.value.phase == DictationPhase.Processing
        ) {
            return
        }
        errorDismissJob?.cancel()
        finishing.set(false)
        pasteIntoFocusedApp = paste
        sessionApp = targetApp.ifBlank { TextInjector.foregroundPackage() ?: sessionApp.ifBlank { "MaxSpeech" } }
        listenJob?.cancel()
        thinkingJob?.cancel()
        pcmJob?.cancel()
        audio.stop()
        finals.clear()
        lastInterim.clear()
        for (i in smoothed.indices) smoothed[i] = 0.14f
        lastLevelUiAt = 0L
        // Always enter Listening first — never flash X/Retry in place of the mic.
        listenStartedAtMs = System.currentTimeMillis()
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
                stt.connect(keys, lang, DeepgramClient.BUILTIN_KEYTERMS)
                launch {
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
                    audio.start(
                        onPcm = { stt.sendPcm(it) },
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
                }
                // Plan / dictionary in parallel — don't block first audio.
                val planOk = async(Dispatchers.IO) {
                    val used = db.usageDao().wordsSince(PlanCalculator.weekStartUtc())
                    val fresh = settings.snapshot()
                    cachedSnap = fresh
                    PlanCalculator.from(auth.current(), used).canDictate
                }
                stt.awaitOpen()
                if (!planOk.await()) {
                    pcmJob?.cancel()
                    audio.stop()
                    stt.close()
                    presentError("Weekly word limit reached", autoClearMs = 2_400)
                    return@launch
                }
                launch {
                    delay(120_000)
                    if (_ui.value.phase == DictationPhase.Listening) {
                        stopAndFinish()
                    }
                }
                pcmJob?.join()
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Throwable) {
                thinkingJob?.cancel()
                audio.stop()
                stt.close()
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

    /** Re-run the last session (overlay or in-app Retry). */
    fun retry() {
        if (_ui.value.phase != DictationPhase.Error) return
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
        listenJob?.cancel()
        pcmJob?.cancel()
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
        // Nothing on screen — bail immediately instead of a thinking-wave wait.
        if (preview.isBlank()) {
            resetToIdle()
            return
        }
        // Kick thinking animation NOW — flush / enhance run underneath.
        _ui.value = _ui.value.copy(
            phase = DictationPhase.Processing,
            originalText = preview,
            liveText = preview,
        )
        startThinkingWave()
        scope.launch {
            try {
                finishInternal(confirmOnly = !pasteIntoFocusedApp)
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
                TextInjector.insert(context, text)
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
        val trailMs = if (heldMs < 5_000L) 60L else 120L
        delay(trailMs)
        audio.stop()
        pcmJob?.cancel()
        stt.finishAndFlush(waitMs = 380)
        val merged = TranscriptMerge.mergeTrailing(finals.toString(), lastInterim.toString())
        val live = _ui.value.liveText.trim()
        val raw = listOf(merged, live, _ui.value.originalText)
            .maxByOrNull { it.length }
            ?.trim()
            .orEmpty()
        if (raw.isBlank()) {
            // Flush still empty — stop quietly; no error chip / retry delay.
            resetToIdle()
            return
        }
        _ui.value = _ui.value.copy(originalText = raw, liveText = raw)

        var out = raw
        if (snap.aiEnhance) {
            val key = snap.llmKey.trim()
            if (key.isNotBlank()) {
                runCatching {
                    val tone = resolveTone(sessionApp, snap.toneOverride)
                    out = enhance.enhance(
                        text = raw,
                        tone = tone,
                        apiKey = key,
                        speed = snap.enhanceSpeed,
                        multilingual = snap.multilingual,
                    )
                }.onFailure {
                    out = raw
                }
            }
        }
        // Always polish numerals locally (enhance may be off / keyless).
        out = NumeralPolish.polish(out)

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
            scope.launch(Dispatchers.IO) {
                runCatching {
                    db.historyDao().insert(
                        HistoryEntity(text = historyText, appName = historyApp, enhanced = enhanced),
                    )
                    db.usageDao().insert(UsageEntity(wordCount = wordCount))
                }
            }
            return
        }

        if (pasteIntoFocusedApp && out.isNotBlank()) {
            TextInjector.insert(context, historyText)
        }
        OverlayService.notifyRecording(context, false)
        _ui.value = DictationUi()
        scope.launch(Dispatchers.IO) {
            runCatching {
                db.historyDao().insert(
                    HistoryEntity(text = historyText, appName = historyApp, enhanced = enhanced),
                )
                db.usageDao().insert(UsageEntity(wordCount = wordCount))
            }
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

    private fun presentError(message: String, autoClearMs: Long = RETRY_WINDOW_MS) {
        OverlayService.notifyRecording(context, false)
        val snippet = listOf(
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
            }
        }
        errorDismissJob?.cancel()
        errorDismissJob = scope.launch {
            delay(autoClearMs)
            if (_ui.value.phase == DictationPhase.Error) {
                _ui.value = DictationUi()
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
