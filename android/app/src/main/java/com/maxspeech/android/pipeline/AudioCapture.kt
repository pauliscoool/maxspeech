package com.maxspeech.android.pipeline

import java.io.IOException
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import kotlin.math.abs
import kotlin.math.log10
import kotlin.math.max
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext

class AudioCapture {
    private var record: AudioRecord? = null

    suspend fun start(onStarted: () -> Unit, onPcm: suspend (ShortArray) -> Unit, onLevel: (Float) -> Unit) {
        withContext(Dispatchers.IO) {
            val min = AudioRecord.getMinBufferSize(
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
            if (min <= 0) throw IOException("Android could not allocate a microphone buffer ($min).")
            val bufferSize = max(min, SAMPLE_RATE / 5) // ~200ms
            val rec = AudioRecord(
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                bufferSize * 2,
            )
            record = rec
            try {
                if (rec.state != AudioRecord.STATE_INITIALIZED) {
                    throw IOException("Android could not initialize microphone recording.")
                }
                rec.startRecording()
                if (rec.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
                    throw IOException("Android could not start microphone recording.")
                }
                onStarted()
                val buf = ShortArray(SAMPLE_RATE / 10) // 100ms
                while (isActive && rec.recordingState == AudioRecord.RECORDSTATE_RECORDING) {
                    val n = rec.read(buf, 0, buf.size)
                    if (n > 0) {
                        val slice = buf.copyOf(n)
                        onPcm(slice)
                        onLevel(rms(slice))
                    } else if (n < 0 && isActive) {
                        throw IOException("Microphone stopped unexpectedly (error $n).")
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

    private fun rms(samples: ShortArray): Float {
        if (samples.isEmpty()) return 0f
        var sum = 0.0
        for (s in samples) {
            val v = s.toDouble()
            sum += v * v
        }
        val rms = kotlin.math.sqrt(sum / samples.size) / 32768.0
        val db = (20.0 * log10(max(rms, 1e-6))).toFloat()
        return ((db + 50f) / 50f).coerceIn(0.08f, 1f)
    }

    companion object {
        const val SAMPLE_RATE = 16_000
    }
}
