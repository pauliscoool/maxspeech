package com.maxspeech.android.pipeline

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext

class AudioCapture {
    private var record: AudioRecord? = null
    private var frame: Long = 0
    private var agcGain = 1f

    /**
     * [onLevel] receives [BAR_COUNT] visual levels (0..1) — desktop-style global
     * energy + soft AGC + bell swell so phone mics jump as high as desktop.
     */
    suspend fun start(onPcm: suspend (ShortArray) -> Unit, onLevel: (FloatArray) -> Unit) {
        withContext(Dispatchers.IO) {
            frame = 0
            agcGain = 1f
            val min = AudioRecord.getMinBufferSize(
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
            val bufferSize = max(min, SAMPLE_RATE / 5)
            val rec = AudioRecord(
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                bufferSize * 2,
            )
            record = rec
            rec.startRecording()
            // 50ms frames — snappier bars and less speech lost before STT opens.
            val buf = ShortArray(SAMPLE_RATE / 20)
            try {
                while (isActive && rec.recordingState == AudioRecord.RECORDSTATE_RECORDING) {
                    val n = rec.read(buf, 0, buf.size)
                    if (n > 0) {
                        val slice = buf.copyOf(n)
                        onPcm(slice)
                        onLevel(visualBars(slice))
                    }
                }
            } finally {
                runCatching {
                    rec.stop()
                    rec.release()
                }
                if (record === rec) record = null
            }
        }
    }

    fun stop() {
        runCatching {
            record?.stop()
            record?.release()
        }
        record = null
    }

    /** Desktop `compute_bars` + soft AGC. */
    private fun visualBars(samples: ShortArray): FloatArray {
        val count = BAR_COUNT
        val idle = FloatArray(count) { 0.12f }
        if (samples.isEmpty()) return idle

        var sumSq = 0.0
        var peak = 0f
        for (s in samples) {
            val f = s / 32768f
            sumSq += (f * f).toDouble()
            val a = abs(f)
            if (a > peak) peak = a
        }
        val rms = sqrt(sumSq / samples.size).toFloat()
        val fromRms = if (rms > 1e-5f) {
            (AGC_TARGET_RMS / rms).coerceIn(1f, AGC_MAX_GAIN)
        } else {
            1f
        }
        val fromPeak = if (peak > 1e-5f) {
            (0.95f / peak).coerceIn(1f, AGC_MAX_GAIN)
        } else {
            AGC_MAX_GAIN
        }
        val target = minOf(fromRms, fromPeak)
        agcGain += (target - agcGain) * 0.18f

        val gRms = (rms * agcGain).coerceAtMost(1f)
        val gPeak = (peak * agcGain).coerceAtMost(1f)
        val energy = (gRms * 0.65f + gPeak * 0.35f).coerceAtLeast(0f)
        val f = frame++
        if (energy < NOISE_GATE) return idle

        val level = ((energy - NOISE_GATE * 0.35f).coerceAtLeast(0f) * LEVEL_GAIN)
            .toDouble()
            .pow(0.58)
            .toFloat()
            .coerceIn(0.12f, 0.98f)

        val t = f * 0.18f
        val out = FloatArray(count)
        val half = (count - 1) / 2f
        for (i in 0 until count) {
            val centerDist = (i - half) / half.coerceAtLeast(1f)
            val bell = 1f - centerDist * centerDist * 0.40f
            val wave = 0.82f + 0.18f * (
                sin((t * 1.6f + i * 0.5f).toDouble()).toFloat() * 0.5f + 0.5f
                )
            out[i] = (level * bell * wave).coerceIn(0.12f, 0.98f)
        }
        return out
    }

    companion object {
        const val SAMPLE_RATE = 16_000
        /** Compact overlay pill — six bars. */
        const val BAR_COUNT = 6

        private const val NOISE_GATE = 0.012f
        private const val LEVEL_GAIN = 14f
        private const val AGC_TARGET_RMS = 0.12f
        private const val AGC_MAX_GAIN = 5.2f
    }
}
