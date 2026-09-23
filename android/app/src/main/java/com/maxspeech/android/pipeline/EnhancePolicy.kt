package com.maxspeech.android.pipeline

import com.maxspeech.android.data.EnhanceSpeed

/** Mirrors desktop `EnhanceSpeed` policy in tone.rs — keep in sync. */
object EnhancePolicy {
    /** Skip the LLM below this many seconds of speech (local cleanup only). */
    fun quickSkipSecs(speed: EnhanceSpeed): Double = when (speed) {
        EnhanceSpeed.Fast -> 6.25
        EnhanceSpeed.Thinking -> 5.0
        EnhanceSpeed.Ultra -> 1.5
    }

    fun longWordThreshold(speed: EnhanceSpeed): Int = when (speed) {
        EnhanceSpeed.Fast -> 64
        EnhanceSpeed.Thinking -> 40
        EnhanceSpeed.Ultra -> 24
    }

    fun tokenBudget(speed: EnhanceSpeed, base: Int): Int {
        val scaled = when (speed) {
            EnhanceSpeed.Fast -> Math.round(base * 0.8).toInt()
            EnhanceSpeed.Thinking -> base
            EnhanceSpeed.Ultra -> Math.round(base * 1.25).toInt()
        }
        return maxOf(scaled, 256)
    }
}