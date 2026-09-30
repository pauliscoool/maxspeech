package com.maxspeech.android.pipeline

// Final words win, while a missing final response must not silently truncate the last interim tail.
internal class TranscriptAccumulator {
    private var finalText = ""
    private var interim = ""

    fun clear() {
        finalText = ""
        interim = ""
    }

    private fun prefix(full: String, part: String): Boolean = part.isNotEmpty() && full.startsWith(part) &&
        (full.length == part.length || !full[part.length].isLetterOrDigit())

    fun add(text: String, isFinal: Boolean) {
        val t = text.trim()
        if (t.isEmpty()) return
        if (!isFinal) {
            interim = t
        } else if (prefix(t, finalText.trim()) && t.length >= finalText.trim().length) {
            finalText = "$t "
            interim = ""
        } else if (prefix(interim.trim(), t)) {
            val rest = interim.trim().drop(t.length).trimStart()
            finalText = finalText.trimEnd() + (if (finalText.isEmpty()) "" else " ") + "$t "
            interim = rest
        } else {
            finalText = finalText.trimEnd() + (if (finalText.isEmpty()) "" else " ") + "$t "
            interim = ""
        }
    }

    fun text(): String {
        val final = finalText.trim()
        val tail = interim.trim()
        if (tail.isEmpty()) return final
        if (final.isEmpty()) return tail
        if (final == tail || final.endsWith(tail)) return final
        if (prefix(tail, final)) return tail
        val left = final.split(Regex("\\s+"))
        val right = tail.split(Regex("\\s+"))
        for (overlap in minOf(left.size, right.size) downTo 1) {
            if (left.takeLast(overlap) == right.take(overlap)) return (left.dropLast(overlap) + right).joinToString(" ")
        }
        return "$final $tail"
    }
}
