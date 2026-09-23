package com.maxspeech.android.pipeline

/**
 * Fold Deepgram Results the same way as desktop `pipeline/mod.rs`.
 * Nova-3 emits multiple `is_final` deltas; interims replace the in-progress window.
 */
internal object TranscriptMerge {
    fun apply(
        finalText: StringBuilder,
        lastInterim: StringBuilder,
        chunkText: String,
        isFinal: Boolean,
    ) {
        val t = chunkText.trim()
        if (t.isEmpty()) return
        if (!isFinal) {
            lastInterim.clear()
            lastInterim.append(t)
            return
        }

        val accumulated = finalText.toString().trim()
        // Some results repeat everything so far instead of a delta — replace.
        if (isTextPrefix(t, accumulated) && t.length >= accumulated.length) {
            finalText.clear()
            finalText.append(t).append(' ')
            lastInterim.clear()
            return
        }

        val interim = lastInterim.toString().trim()
        if (isTextPrefix(interim, t)) {
            val rest = if (interim.length > t.length) {
                interim.substring(t.length).trimStart()
            } else {
                ""
            }
            if (finalText.isNotEmpty() && !finalText.endsWith(' ')) finalText.append(' ')
            finalText.append(t).append(' ')
            lastInterim.clear()
            lastInterim.append(rest)
            return
        }

        if (finalText.isNotEmpty() && !finalText.endsWith(' ')) finalText.append(' ')
        finalText.append(t).append(' ')
        lastInterim.clear()
    }

    fun display(finalText: CharSequence, lastInterim: CharSequence): String {
        val base = finalText.toString().trim()
        val interim = lastInterim.toString().trim()
        if (interim.isEmpty()) return base
        if (base.isEmpty()) return interim
        return "$base $interim".trim()
    }

    fun mergeTrailing(finalText: String, interim: String): String {
        val text = finalText.trim()
        val tip = interim.trim()
        if (tip.isEmpty()) return text
        if (text.isEmpty()) return tip
        if (text == tip || text.endsWith(tip)) return text
        if (isTextPrefix(tip, text)) return tip

        val tWords = text.split(Regex("\\s+")).filter { it.isNotEmpty() }
        val iWords = tip.split(Regex("\\s+")).filter { it.isNotEmpty() }
        val maxOverlap = minOf(tWords.size, iWords.size)
        for (overlap in maxOverlap downTo 1) {
            if (tWords.takeLast(overlap) == iWords.take(overlap)) {
                val out = tWords.dropLast(overlap) + iWords
                return out.joinToString(" ")
            }
        }
        return "$text $tip"
    }

    /** True when [full] is [prefix], or [prefix] plus a non-alphanumeric break. */
    fun isTextPrefix(full: String, prefix: String): Boolean {
        if (prefix.isEmpty() || !full.startsWith(prefix)) return false
        if (full.length == prefix.length) return true
        val next = full[prefix.length]
        return !next.isLetterOrDigit()
    }
}
