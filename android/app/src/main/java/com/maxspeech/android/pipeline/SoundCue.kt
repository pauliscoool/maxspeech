package com.maxspeech.android.pipeline

import android.content.Context
import android.media.AudioAttributes
import android.media.SoundPool
import com.maxspeech.android.R

/**
 * Short click for dictation start/stop (Settings → Start / stop sound, with volume).
 * USAGE_ASSISTANT follows the media-style assistant volume instead of the touch-sound
 * volume that many phones keep muted.
 */
class SoundCue(context: Context) {
    private val app = context.applicationContext
    private val pool: SoundPool = SoundPool.Builder()
        .setMaxStreams(2)
        .setAudioAttributes(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_ASSISTANT)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build(),
        )
        .build()
    private var soundId: Int = 0
    @Volatile private var ready = false

    init {
        soundId = pool.load(app, R.raw.bubble_click, 1)
        pool.setOnLoadCompleteListener { _, sampleId, status ->
            if (sampleId == soundId && status == 0) ready = true
        }
    }

    fun play(volume: Float) {
        if (!ready || soundId == 0) return
        val v = volume.coerceIn(0f, 1f)
        if (v == 0f) return
        pool.play(soundId, v, v, 1, 0, 1f)
    }

    fun release() {
        ready = false
        pool.release()
    }
}
