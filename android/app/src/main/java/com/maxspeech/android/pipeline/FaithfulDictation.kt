package com.maxspeech.android.pipeline

import java.util.Locale

data class GuardDecision(val text: String, val drift: Double, val reason: String)

object FaithfulDictation {
    fun formattingCommand(text: String): String? = when (text.trim().trimEnd('.', '?', '!').lowercase(Locale.ROOT)) {
        "new line", "newline" -> "\n"
        "new paragraph" -> "\n\n"
        "insert comma", "comma" -> ","
        "insert period", "period", "full stop" -> "."
        "question mark" -> "?"
        "exclamation mark", "exclamation point" -> "!"
        else -> null
    }
    private fun bare(word: String): String = word.codePoints().toArray()
        .filter { Character.isLetterOrDigit(it) }
        .joinToString("") { String(Character.toChars(it)) }.lowercase(Locale.ROOT)

    fun hasNonLatinScript(text: String): Boolean = text.codePoints().anyMatch {
        Character.isLetter(it) && it > 0x024f
    }

    fun replacePhrase(text: String, from: String, to: String): String {
        if (from.isEmpty()) return text
        val chars = text.codePoints().toArray()
        val needle = from.codePoints().toArray()
        val out = StringBuilder()
        var i = 0
        while (i < chars.size) {
            val end = i + needle.size
            if (end <= chars.size && (i == 0 || !Character.isLetterOrDigit(chars[i - 1])) &&
                (end == chars.size || !Character.isLetterOrDigit(chars[end])) &&
                String(chars, i, needle.size).lowercase(Locale.ROOT) == from.lowercase(Locale.ROOT)
            ) {
                out.append(to)
                i = end
            } else {
                out.appendCodePoint(chars[i])
                i++
            }
        }
        return out.toString()
    }

    fun expandExplicit(text: String, dictionary: List<String>, snippets: List<Pair<String, String>>): String {
        var result = text
        for ((trigger, expansion) in snippets.sortedByDescending { it.first.codePointCount(0, it.first.length) }) {
            result = replacePhrase(result, trigger, expansion)
        }
        for (term in dictionary.map { it.trim() }.filter { it.isNotEmpty() }) {
            result = replacePhrase(result, term, term)
        }
        return result
    }

    private fun words(text: String): List<String> = text.trim().split(Regex("\\s+")).filter { it.isNotEmpty() }

    private fun weekday(word: String): Boolean = bare(word) in listOf(
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
    )

    private fun name(word: String): Boolean {
        val clean = word.trim { !it.isLetterOrDigit() }
        return clean.length >= 2 && clean.first().isUpperCase() && clean.all { it.isLetter() || it == '-' } &&
            bare(clean) !in listOf("i", "the", "so", "send", "meet", "lets", "please", "we", "you", "it", "this", "that")
    }

    private fun selfCorrect(text: String): String {
        val tokens = words(text)
        val normalized = tokens.map(::bare)
        val markers = listOf(
            listOf("oh", "no", "i", "meant"), listOf("sorry", "i", "meant"),
            listOf("wait", "i", "meant"), listOf("i", "meant"), listOf("i", "mean"),
            listOf("no", "wait"), listOf("wait", "no"), listOf("wait", "actually"),
            listOf("actually"),
        )
        for (i in tokens.indices.reversed()) {
            val marker = markers.firstOrNull {
                i + it.size < tokens.size && normalized.subList(i, i + it.size) == it
            } ?: continue
            if (markers.any { longer -> longer.size > marker.size && i >= longer.size - marker.size &&
                    normalized.subList(i - (longer.size - marker.size), i + marker.size) == longer }) continue
            if (i == 0) continue
            val replacement = tokens.drop(i + marker.size)
            val before = words(selfCorrect(tokens.take(i).joinToString(" "))).toMutableList()
            if (marker.size == 1 && before.lastOrNull()?.let { bare(it).any { c -> c in '0'..'9' } || bare(it) in listOf("am", "pm") } != true) return text
            var target = -1
            if (replacement.size == 1) {
                val rep = replacement[0]
                if (weekday(rep)) {
                    val candidates = before.indices.filter { weekday(before[it]) }
                    if (candidates.size == 1) target = candidates[0]
                } else if (name(rep)) {
                    val candidates = before.indices.filter { name(before[it]) && !weekday(before[it]) }
                    if (candidates.size == 1) target = candidates[0]
                } else if (bare(rep).any { it in '0'..'9' }) {
                    val candidates = before.indices.filter { bare(before[it]).any { c -> c in '0'..'9' } }
                    if (candidates.size == 1) target = candidates[0]
                }
                if (target >= 0) {
                    val punctuation = before[target].takeLastWhile { it in ".?!" }
                    before[target] = rep.trimEnd('.', '?', '!', ',') + punctuation
                    return before.joinToString(" ")
                }
            }
            if (replacement.size == 2 && bare(replacement[1]) in listOf("am", "pm") && bare(replacement[0]).all { it in '0'..'9' }) {
                val candidates = (0 until before.size - 1).filter {
                    bare(before[it + 1]) in listOf("am", "pm") && bare(before[it]).all { c -> c in '0'..'9' }
                }
                if (candidates.size == 1) {
                    val targetIndex = candidates[0]
                    before[targetIndex] = replacement[0]
                    before[targetIndex + 1] = replacement[1]
                    return before.joinToString(" ")
                }
            }
            // Shared clause words locate the correction without guessing how many words to erase.
            if (replacement.size >= 3 && before.size >= replacement.size) {
                val start = before.size - replacement.size
                val old = before.drop(start)
                if (old.drop(1).map(::bare) == replacement.drop(1).map(::bare) && name(old[0]) && name(replacement[0])) {
                    return (before.take(start) + replacement).joinToString(" ")
                }
            }
            return text
        }
        return text
    }

    private fun removeHesitations(text: String): String {
        val tokens = words(text)
        return tokens.filterIndexed { i, word ->
            val prefix = bare(word)
            val literal = word.any { it in "\"'`" } || word == word.uppercase(Locale.ROOT) ||
                (i > 0 && bare(tokens[i - 1]) in listOf("a", "the", "word", "words", "say", "said", "says", "spell", "typed", "literal", "named", "called"))
            (literal || prefix !in listOf("um", "uh", "umm", "uhh")) &&
                !(word.endsWith('-') && i + 1 < tokens.size && prefix.isNotEmpty() &&
                    tokens[i + 1].trimEnd('.', ',', '?', '!').all { it.isLetter() } && bare(tokens[i + 1]).length > prefix.length + 1 &&
                    prefix.length <= 2 && bare(tokens[i + 1]).startsWith(prefix))
        }.joinToString(" ")
    }

    private fun spokenFormat(text: String): String {
        val tokens = words(text)
        var out = ""
        var i = 0
        while (i < tokens.size) {
            val first = bare(tokens[i])
            val second = tokens.getOrNull(i + 1)?.let(::bare).orEmpty()
            val literal = tokens[i].any { it in "\"'`" } ||
                (i > 0 && bare(tokens[i - 1]) in listOf("a", "the", "about", "word", "words", "phrase", "says", "said", "say", "named", "not", "dont", "never", "avoid", "without", "explain", "discuss", "discussed")) ||
                tokens.getOrNull(i + 2)?.let { bare(it) in listOf("command", "commands", "syntax", "means", "symbol", "character") } == true
            val mark = if (literal) null else when (first to second) {
                "new" to "paragraph" -> "\n\n"
                "new" to "line" -> "\n"
                "question" to "mark" -> if (i + 2 == tokens.size) "?" else null
                "exclamation" to "mark" -> if (i + 2 == tokens.size) "!" else null
                "insert" to "comma" -> ","
                "insert" to "period" -> "."
                else -> null
            }
            if (mark != null) {
                out = out.trimEnd() + mark
                i += 2
            } else {
                if (out.isNotEmpty() && !out.endsWith('\n')) out += " "
                out += tokens[i]
                i++
            }
        }
        return out
    }

    private fun numberedList(text: String): String {
        val tokens = words(text)
        val ordinals = listOf("one", "two", "three", "four", "five", "six", "seven", "eight", "nine")
        val markers = mutableListOf<Pair<Int, Int>>()
        for (i in tokens.indices) {
            val number = (markers.size + 1).toString()
            if (i + 2 < tokens.size && bare(tokens[i]) == "number" &&
                (bare(tokens[i + 1]) == number || bare(tokens[i + 1]) == ordinals.getOrNull(markers.size))) {
                markers += i to i + 2
            } else if (i + 1 < tokens.size && tokens[i] == "$number.") {
                markers += i to i + 1
            }
        }
        if (markers.size < 2 || markers[0].first != 0) return text
        val lines = mutableListOf<String>()
        for ((n, marker) in markers.withIndex()) {
            val end = markers.getOrNull(n + 1)?.first ?: tokens.size
            if (marker.second >= end) return text
            lines += "${n + 1}. ${tokens.subList(marker.second, end).joinToString(" ")}"
        }
        return lines.joinToString("\n")
    }

    private fun renderSurface(text: String, tone: String): String {
        val out = StringBuilder()
        var sentenceStart = true
        for (c in text.codePoints().toArray()) {
            if (Character.isLetter(c)) {
                val letter = String(Character.toChars(c))
                out.append(if (sentenceStart && tone != "casual") letter.uppercase(Locale.ROOT) else letter)
                sentenceStart = false
            } else {
                out.appendCodePoint(c)
                if (c in intArrayOf('.'.code, '!'.code, '?'.code, '\n'.code)) sentenceStart = true
                else if (Character.isDigit(c)) sentenceStart = false
            }
        }
        val s = out.toString().trimEnd()
        if (s.isEmpty()) return ""
        val last = s.last()
        if (last in ",;") {
            val stem = s.dropLast(1).trimEnd()
            return if (tone == "casual") stem else "$stem."
        }
        if (tone != "casual" && '\n' !in s && last.isLetterOrDigit() && words(s).size >= 3) return "$s."
        return s
    }

    fun localCleanup(text: String, tone: String, english: Boolean): String {
        if (!english || hasNonLatinScript(text)) return text.trim()
        formattingCommand(text)?.let { return it }
        val lines = text.lineSequence().map { removeHesitations(selfCorrect(it)) }
        val formatted = lines.map { numberedList(spokenFormat(it)) }.joinToString("\n")
        return renderSurface(formatted, tone)
    }

    private fun tokens(text: String): List<String> = words(text).map(::bare).filter {
        it.isNotEmpty() && it !in listOf("um", "uh", "umm", "uhh")
    }

    fun guardOutput(local: String, candidate: String): GuardDecision {
        val a = tokens(local)
        val b = tokens(candidate)
        if (a == b) {
            val reason = if (candidate.trim().lowercase(Locale.ROOT) == local.trim().lowercase(Locale.ROOT)) "accepted" else "surface_difference"
            return GuardDecision(local, 0.0, reason)
        }
        val row = IntArray(b.size + 1) { it }
        for ((i, left) in a.withIndex()) {
            var diagonal = row[0]
            row[0] = i + 1
            for ((j, right) in b.withIndex()) {
                val previous = row[j + 1]
                row[j + 1] = minOf(row[j] + 1, previous + 1, diagonal + if (left == right) 0 else 1)
                diagonal = previous
            }
        }
        val drift = row[b.size].toDouble() / a.size.coerceAtLeast(1)
        // Local rendering keeps formatting independent of remote completion or deadline timing.
        val reason = when {
            candidate.isBlank() && local.isNotBlank() -> "empty"
            a != b -> "lexical_drift"
            candidate.trim().lowercase(Locale.ROOT) != local.trim().lowercase(Locale.ROOT) -> "surface_difference"
            else -> "accepted"
        }
        return GuardDecision(local, drift, reason)
    }
}
