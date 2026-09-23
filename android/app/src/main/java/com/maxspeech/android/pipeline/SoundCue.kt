package com.maxspeech.android.pipeline

import android.content.Context
import android.media.AudioAttributes
import android.media.SoundPool
import com.maxspeech.android.R

/**
 * Short bubble-click cue for dictation start/stop (Settings → Start / stop sound).
 */
class SoundCue(context: Context) {
    private val app = context.applicationContext
    private val pool: SoundPool = SoundPool.Builder()
        .setMaxStreams(2)
        .setAudioAttributes(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION)
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

    fun play(volume: Float = 0.85f) {
        if (!ready || soundId == 0) return
        val v = volume.coerceIn(0.05f, 1f)
        pool.play(soundId, v, v, 1, 0, 1f)
    }

    fun release() {
        ready = false
        pool.release()
    }
}
