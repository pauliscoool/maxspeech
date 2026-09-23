package com.maxspeech.android.pipeline

/**
 * Light local polish for ASR numeral quirks.
 * Deepgram `numerals=true` emits digits; keep versions/codes as digits, spell
 * out single digits in plain prose, and fix digit-homophones.
 */
object NumeralPolish {
    private val DIGIT_WORD = mapOf(
        "0" to "zero", "1" to "one", "2" to "two", "3" to "three", "4" to "four",
        "5" to "five", "6" to "six", "7" to "seven", "8" to "eight", "9" to "nine",
    )

    fun polish(raw: String): String {
        if (raw.isBlank()) return raw
        var t = raw
        t = Regex("""\bneed 2 (go|get|do|be|see|know)\b""", RegexOption.IGNORE_CASE)
            .replace(t) { m -> m.value.replace(Regex("""\b2\b"""), "to") }
        t = Regex("""\b2 (much|many|late|bad|good)\b""", RegexOption.IGNORE_CASE)
            .replace(t) { m -> m.value.replaceFirst(Regex("""\b2\b"""), "too") }
        t = Regex("""\b4 (the|a|an|you|me|us|them|him|her)\b""", RegexOption.IGNORE_CASE)
            .replace(t) { m -> m.value.replaceFirst(Regex("""\b4\b"""), "for") }
        t = Regex("""\bthanks 4 the\b""", RegexOption.IGNORE_CASE).replace(t, "thanks for the")
        t = Regex("""\b1 of\b""", RegexOption.IGNORE_CASE).replace(t, "one of")
        t = Regex("""\bno 1\b""", RegexOption.IGNORE_CASE).replace(t, "no one")

        val source = t
        t = Regex("""(?<![.\d])\b([0-9])\b(?![.\d%])""")
            .replace(t) { m ->
                val digit = m.groupValues[1]
                val around = contextAround(source, m.range.first, m.range.last)
                if (keepAsDigit(around, digit)) digit else (DIGIT_WORD[digit] ?: digit)
            }
        return t
    }

    private fun contextAround(full: String, start: Int, end: Int): String {
        val from = (start - 28).coerceAtLeast(0)
        val to = (end + 28).coerceAtMost(full.length)
        return full.substring(from, to).lowercase()
    }

    private fun keepAsDigit(around: String, digit: String): Boolean {
        val keepCtx = Regex(
            """\b(opus|version|ver|gpt|claude|sonnet|haiku|gemini|model|v|room|page|apt|suite|chapter|issue|episode|season)\s*$digit\b""",
        )
        if (keepCtx.containsMatchIn(around)) return true
        if (around.contains(":$digit") || around.contains("$digit:")) return true
        if (around.contains("$digit%") || around.contains("$digit %")) return true
        if (around.contains(".$digit") || around.contains("$digit.")) return true
        if (Regex("""\b\d+\s?(pm|am)\b""").containsMatchIn(around)) return true
        return false
    }
}
