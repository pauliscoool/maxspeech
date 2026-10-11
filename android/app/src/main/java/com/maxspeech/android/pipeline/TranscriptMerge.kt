package com.maxspeech.android.pipeline

/**
 * Fold Deepgram streaming results into one transcript.
 * Only `is_final` segments are committed (Deepgram's documented method); interims just
 * preview the in-progress window. Each final is formatted on its own, so seams get fixed here.
 */
internal object TranscriptMerge {
    /**
     * [seenStarts] holds the start time (ms) of every committed final so a re-sent segment is
     * never appended twice. Pass startMs = -1 when unknown.
     */
    fun apply(
        finalText: StringBuilder,
        lastInterim: StringBuilder,
        chunkText: String,
        isFinal: Boolean,
        startMs: Long = -1L,
        seenStarts: MutableSet<Long>? = null,
    ) {
        val t = chunkText.trim()
        if (t.isEmpty()) return
        if (!isFinal) {
            lastInterim.clear()
            lastInterim.append(t)
            return
        }
        if (startMs >= 0 && seenStarts != null && !seenStarts.add(startMs)) return
        val prev = finalText.toString().trimEnd()
        finalText.clear()
        finalText.append(joinSeam(prev, t)).append(' ')
        lastInterim.clear()
    }

    /** Join two separately formatted segments: no capital mid-sentence, no doubled seam word. */
    fun joinSeam(prev: String, next: String): String {
        val a = prev.trimEnd()
        var b = next.trim()
        if (a.isEmpty()) return b
        if (b.isEmpty()) return a
        val sentenceEnded = a.last() in ".!?…\n"
        if (!sentenceEnded) {
            // smart_format capitalizes every segment start; a mid-sentence "What" goes back to "what".
            val firstWord = b.takeWhile { !it.isWhitespace() }
            val bare = firstWord.trimEnd(',', '.', '?', '!').lowercase()
            if (bare in CONTINUATION && firstWord.firstOrNull()?.isUpperCase() == true && firstWord.drop(1).none { it.isUpperCase() }) {
                b = b.replaceFirstChar { it.lowercaseChar() }
            }
        }
        // A stutter across the seam ("is" | "is it...") stays for the AI pass; exact
        // duplicated tails from a re-sent window are trimmed.
        val aWords = a.split(Regex("\\s+"))
        val bWords = b.split(Regex("\\s+"))
        val max = minOf(aWords.size, bWords.size, 6)
        for (n in max downTo 2) {
            if (aWords.takeLast(n).map(::norm) == bWords.take(n).map(::norm)) {
                return (aWords + bWords.drop(n)).joinToString(" ")
            }
        }
        return "$a $b"
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
        val tWords = text.split(Regex("\\s+")).filter { it.isNotEmpty() }
        val iWords = tip.split(Regex("\\s+")).filter { it.isNotEmpty() }
        if (tWords.takeLast(iWords.size).map(::norm) == iWords.map(::norm)) return text
        // Overlap compared without case / punctuation: smart_format differs between interim and final.
        val maxOverlap = minOf(tWords.size, iWords.size)
        for (overlap in maxOverlap downTo 1) {
            if (tWords.takeLast(overlap).map(::norm) == iWords.take(overlap).map(::norm)) {
                return (tWords + iWords.drop(overlap)).joinToString(" ")
            }
        }
        return joinSeam(text, tip)
    }

    private fun norm(w: String) = w.lowercase().trim { !it.isLetterOrDigit() && it != '\'' }

    /** Words that start a segment mid-sentence far more often than they start a name. */
    private val CONTINUATION = setOf(
        "a", "an", "the", "and", "but", "or", "so", "because", "if", "then", "when", "where", "why",
        "how", "what", "who", "which", "that", "this", "these", "those", "it", "its", "it's", "is",
        "are", "was", "were", "be", "do", "does", "did", "can", "could", "would", "should", "will",
        "to", "of", "in", "on", "at", "for", "with", "from", "about", "like", "just", "basically",
        "also", "even", "there", "their", "they", "they're", "we", "you", "he", "she", "my", "your",
        "our", "not", "all", "some", "any", "maybe", "really", "actually", "kind", "sort", "um", "uh",
    )
}
