package com.maxspeech.android.pipeline

import android.content.res.AssetManager
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Kotlin port of desktop `pipeline/vocab.rs` + `learn_substitutions.rs`.
 * Keep behavior identical to the Rust version — both have mirrored tests.
 */
object Vocab {

    data class Substitution(val from: String, val to: String)

    // ---- shared data files (shared/dictation/*.txt) ----

    fun loadKeyterms(assets: AssetManager): List<String> =
        readLines(assets, "keyterms").filter { !it.startsWith("#") }

    fun loadPhraseFixes(assets: AssetManager): List<Pair<String, String>> =
        parsePhraseFixes(readLines(assets, "asr_phrase_fixes").joinToString("\n"))

    fun parsePhraseFixes(txt: String): List<Pair<String, String>> = txt.lines()
        .map { it.trim() }
        .filter { it.isNotEmpty() && !it.startsWith("#") && it.contains("=>") }
        .map { l -> l.substringBefore("=>").trim() to l.substringAfter("=>").trim() }

    private fun readLines(assets: AssetManager, name: String): List<String> =
        assets.open("dictation/$name.txt").bufferedReader().use { r ->
            r.readLines().map { it.trim() }.filter { it.isNotEmpty() }
        }

    /** Desktop `merge_keyterms`: user terms first, builtins always keep their slots. */
    fun mergeKeyterms(user: List<String>, builtin: List<String>, max: Int = MAX_KEYTERMS): List<String> {
        val out = ArrayList<String>()
        val seen = HashSet<String>()
        val userCap = (max - minOf(builtin.size, max)).coerceAtLeast(0)
        for (term in user) {
            if (out.size >= userCap) break
            val t = term.trim()
            if (t.isNotEmpty() && seen.add(t.lowercase())) out.add(t)
        }
        for (term in builtin) {
            if (out.size >= max) break
            val t = term.trim()
            if (t.isNotEmpty() && seen.add(t.lowercase())) out.add(t)
        }
        return out
    }

    const val MAX_KEYTERMS = 120

    // ---- expand_macros ----

    /** Snippets → dictionary casing → possessives → phrase fixes → learned pairs (desktop order). */
    fun expand(
        text: String,
        snippets: List<Pair<String, String>>,
        dictionary: List<String>,
        phraseFixes: List<Pair<String, String>>,
        learned: List<Substitution>,
        clipboard: () -> String = { "" },
    ): String {
        val macros = snippets.sortedByDescending { it.first.length }
        val lower = text.lowercase().trim()
        for ((trigger, expansion) in macros) {
            if (lower == trigger.lowercase()) return expandTemplateVars(expansion, clipboard)
        }
        var result = text
        for ((trigger, expansion) in macros) {
            result = replacePhraseCi(result, trigger, expandTemplateVars(expansion, clipboard))
        }
        val dict = dictionary.map { it.trim() }.filter { it.isNotEmpty() }
        for (want in dict) result = replacePhraseCi(result, want, want)
        result = applyLearnedPossessives(result, dict)
        result = fixCommonAsr(result, phraseFixes)
        return applyPairs(result, learned)
    }

    private fun expandTemplateVars(expansion: String, clipboard: () -> String): String {
        var out = expansion
        if (out.contains("{clipboard}")) out = out.replace("{clipboard}", clipboard())
        if (out.contains("{date}")) out = out.replace("{date}", SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date()))
        if (out.contains("{time}")) out = out.replace("{time}", SimpleDateFormat("HH:mm", Locale.US).format(Date()))
        return out
    }

    fun applyLearnedPossessives(text: String, names: List<String>): String {
        var result = text
        names.map { it.trim() }
            .filter { looksLikeNameTerm(it) && !it.contains('\'') && !it.endsWith('s') && !it.endsWith('S') }
            .sortedByDescending { it.length }
            .forEach { name -> result = replacePhraseCi(result, "${name}s", "$name's") }
        return result
    }

    fun fixCommonAsr(text: String, phraseFixes: List<Pair<String, String>>): String {
        var result = text
        for ((from, to) in phraseFixes) result = replacePhraseCi(result, from, to)
        return fixGetAsGit(result)
    }

    private val VC_HINTS = listOf(
        "place", "placed", "push", "pushed", "pull", "pulled", "commit", "committed", "clone",
        "cloned", "merge", "merged", "rebase", "rebased", "fork", "forked", "branch", "checkout",
        "repo", "repository", "remote", "github", "gitlab", "bitbucket", "pr ", " pull request",
    )
    private val GIT_PREP = setOf("to", "into", "from", "on", "with", "via", "onto")
    private val GIT_NOUN = setOf(
        "commit", "commits", "push", "pull", "repo", "repository", "branch", "branches", "clone",
        "merge", "rebase", "remote", "status", "diff", "log", "ignore",
    )

    private fun fixGetAsGit(text: String): String {
        val lower = text.lowercase()
        if (VC_HINTS.none { lower.contains(it) }) return text
        val out = StringBuilder()
        var i = 0
        while (i < text.length) {
            if (matchesWordAt(text, i, "get")) {
                val before = precedingToken(text, i).lowercase()
                val after = followingToken(text, i + 3).lowercase()
                if (before in GIT_PREP || after in GIT_NOUN) {
                    out.append("Git")
                    i += 3
                    continue
                }
            }
            out.append(text[i])
            i++
        }
        return out.toString()
    }

    private fun matchesWordAt(s: String, i: Int, word: String): Boolean {
        if (i + word.length > s.length) return false
        val beforeOk = i == 0 || !s[i - 1].isLetterOrDigit()
        val end = i + word.length
        val afterOk = end >= s.length || !s[end].isLetterOrDigit()
        return beforeOk && afterOk && s.regionMatches(i, word, 0, word.length, ignoreCase = true)
    }

    private fun precedingToken(s: String, at: Int): String {
        var end = at
        while (end > 0 && s[end - 1].isWhitespace()) end--
        var start = end
        while (start > 0 && s[start - 1].isLetterOrDigit()) start--
        return s.substring(start, end)
    }

    private fun followingToken(s: String, at: Int): String {
        var start = at
        while (start < s.length && s[start].isWhitespace()) start++
        var end = start
        while (end < s.length && s[end].isLetterOrDigit()) end++
        return s.substring(start, end)
    }

    /** Whole-word / phrase, case-insensitive replace of every occurrence. */
    fun replacePhraseCi(text: String, from: String, to: String): String {
        if (from.isEmpty()) return text
        val out = StringBuilder()
        var i = 0
        while (i < text.length) {
            val end = i + from.length
            if (end <= text.length && text.regionMatches(i, from, 0, from.length, ignoreCase = true)) {
                val beforeOk = i == 0 || !text[i - 1].isLetterOrDigit()
                val afterOk = end >= text.length || !text[end].isLetterOrDigit()
                if (beforeOk && afterOk) {
                    out.append(to)
                    i = end
                    continue
                }
            }
            out.append(text[i])
            i++
        }
        return out.toString()
    }

    // ---- learn_name_corrections ----

    /** Name-like words introduced by a correction — to add to the dictionary. */
    fun learnNameCorrections(before: String, after: String): List<String> {
        val b0 = before.trim()
        val a0 = after.trim()
        if (b0.isEmpty() || a0.isEmpty() || b0 == a0) return emptyList()
        val strip = { s: String ->
            s.split(Regex("\\s+")).map { it.trim(',', '.', '!', '?', ';', ':', '"', '\'') }.filter { it.isNotEmpty() }
        }
        val a = strip(b0)
        val b = strip(a0)
        if (a.isEmpty() || b.isEmpty()) return emptyList()
        var i = a.size
        var j = b.size
        while (i > 0 && j > 0 && a[i - 1].equals(b[j - 1], ignoreCase = true)) {
            i--
            j--
        }
        val nChanged = b.size - j
        val changed = b.subList(j, b.size).filter {
            if (nChanged == 1) looksLikeLearnableTerm(it) else looksLikeNameTerm(it)
        }
        if (changed.isEmpty() && a.size == b.size) {
            return a.zip(b).filter { (wa, wb) -> !wa.equals(wb, true) && looksLikeNameTerm(wb) }.map { it.second }
        }
        return changed
    }

    // ---- learn_substitutions ----

    const val LEARN_WINDOW_MS = 5_000L
    private const val MAX_CHANGED_TOKENS = 2

    fun applyPairs(text: String, pairs: List<Substitution>): String {
        var result = text
        for (sub in pairs.sortedByDescending { it.from.length }) {
            result = replacePhraseCi(result, sub.from, sub.to)
        }
        return result
    }

    /** Name-shaped tokens in learned pairs also go to the dictionary (desktop persist_pairs). */
    fun namesIn(pairs: List<Substitution>): List<String> =
        pairs.flatMap { tokenize(it.to) }.filter { looksLikeNameTerm(it) }

    fun substitutionsFromRedictate(previous: String, next: String): List<Substitution> {
        val prev = previous.trim()
        val nxt = next.trim()
        if (prev.isEmpty() || nxt.isEmpty() || prev.equals(nxt, ignoreCase = true)) return emptyList()
        if (looksLikeSecret(prev) || looksLikeSecret(nxt)) return emptyList()
        val a = tokenize(prev)
        val b = tokenize(nxt)
        if (a.isEmpty() || b.isEmpty()) return emptyList()
        if (a.size == b.size) return equalLengthSubs(a, b)
        if (b.size <= MAX_CHANGED_TOKENS && a.size > b.size) return suffixReplacementSubs(a, b)
        return emptyList()
    }

    /**
     * Desktop `redictate_corrections`. Re-dictating within seconds is often a change of mind
     * ("going home" → "going out"), not an ASR miss; learning those swaps made later dictations
     * paste words never said. Keep only pairs whose replacement is a name / term.
     */
    fun redictateCorrections(previous: String, next: String): List<Substitution> {
        val nextTokens = tokenize(next)
        return substitutionsFromRedictate(previous, next).filter { sub ->
            // Judge only the words that changed; an unchanged context word ("Mom") isn't a fix.
            val fromLc = tokenize(sub.from).map { it.lowercase() }.toSet()
            tokenize(sub.to).filter { it.lowercase() !in fromLc }.any { t ->
                val sentenceStart = nextTokens.size >= 3 && nextTokens.first() == t &&
                    t.drop(1).none { it.isUpperCase() }
                looksLikeNameTerm(t) && !sentenceStart
            }
        }
    }

    private fun equalLengthSubs(a: List<String>, b: List<String>): List<Substitution> {
        val diffs = a.indices.filter { !a[it].equals(b[it], true) }
        if (diffs.isEmpty() || diffs.size > MAX_CHANGED_TOKENS) return emptyList()
        if (!diffs.all { similarTokenLen(a[it], b[it]) && shouldLearnPair(a[it], b[it]) }) return emptyList()
        if (diffs.size == 2 && diffs[1] == diffs[0] + 1) {
            return listOf(adjacentPhraseSub(a, b, diffs[0], diffs[1]))
        }
        return diffs.mapNotNull { pairAt(a, b, it) }
    }

    private fun suffixReplacementSubs(a: List<String>, b: List<String>): List<Substitution> {
        val n = b.size
        val tail = a.subList(a.size - n, a.size)
        if (tail.zip(b).all { (x, y) -> x.equals(y, true) }) return emptyList()
        if (!tail.zip(b).all { (x, y) -> similarTokenLen(x, y) && shouldLearnPair(x, y) }) return emptyList()
        if (tail.zip(b).all { (f, t) -> looksLikeNameTerm(f) && looksLikeNameTerm(t) }) {
            return tail.zip(b).filter { (f, t) -> !f.equals(t, true) }.map { (f, t) -> Substitution(f.lowercase(), t) }
        }
        val prefixLen = a.size - n
        if (prefixLen == 0) {
            return tail.zip(b).filter { (x, y) -> !x.equals(y, true) && shouldLearnPair(x, y) }
                .map { (x, y) -> Substitution(x.lowercase(), y) }
        }
        val left = a[prefixLen - 1]
        if (!looksLikeNameTerm(left)) return emptyList()
        val from = a.subList(prefixLen - 1, a.size).joinToString(" ").lowercase()
        val to = (listOf(left) + b).joinToString(" ")
        return listOf(Substitution(from, to))
    }

    private fun pairAt(a: List<String>, b: List<String>, i: Int): Substitution? {
        val from = a[i]
        val to = b[i]
        if (!shouldLearnPair(from, to)) return null
        if (looksLikeNameTerm(from) && looksLikeNameTerm(to)) return Substitution(from.lowercase(), to)
        if (i > 0) return Substitution("${a[i - 1]} $from".lowercase(), "${b[i - 1]} $to")
        if (i + 1 < a.size && a[i + 1].equals(b[i + 1], true)) {
            return Substitution("$from ${a[i + 1]}".lowercase(), "$to ${b[i + 1]}")
        }
        return Substitution(from.lowercase(), to)
    }

    private fun adjacentPhraseSub(a: List<String>, b: List<String>, i: Int, j: Int): Substitution {
        val fromSpan = a.subList(i, j + 1).joinToString(" ")
        val toSpan = b.subList(i, j + 1).joinToString(" ")
        val bothNames = (i..j).all { looksLikeNameTerm(a[it]) && looksLikeNameTerm(b[it]) }
        if (bothNames || i == 0) return Substitution(fromSpan.lowercase(), toSpan)
        return Substitution("${a[i - 1]} $fromSpan".lowercase(), "${b[i - 1]} $toSpan")
    }

    private fun shouldLearnPair(from: String, to: String): Boolean {
        if (from.equals(to, true)) return false
        if (looksLikeSecret(from) || looksLikeSecret(to)) return false
        if (from.count { it.isLetter() } < 2 || to.count { it.isLetter() } < 2) return false
        if (from.length > 40 || to.length > 40) return false
        val bothNames = looksLikeNameShape(from) && looksLikeNameShape(to)
        if (isStopWord(from.lowercase()) || isStopWord(to.lowercase())) return bothNames
        return true
    }

    private fun looksLikeLearnableTerm(word: String): Boolean {
        val w = word.trim()
        if (w.length !in 2..40) return false
        if (w.count { it.isLetter() } < 2) return false
        return !isStopWord(w.lowercase())
    }

    private fun looksLikeNameTerm(word: String) = looksLikeLearnableTerm(word) && looksLikeNameShape(word)

    private fun looksLikeNameShape(word: String): Boolean {
        val w = word.trim()
        if (w.length !in 2..40) return false
        return w[0].isUpperCase() || w.contains('-') || (w.length <= 6 && w.all { it in 'A'..'Z' })
    }

    private val STOP = setOf(
        "a", "an", "the", "and", "or", "but", "to", "of", "in", "on", "for", "with", "at", "by",
        "from", "as", "is", "it", "this", "that", "i", "you", "he", "she", "we", "they", "my", "your",
        "me", "him", "her", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday",
        "sunday", "today", "tomorrow", "yesterday", "please", "thanks", "yes", "no", "ok", "okay",
        "um", "uh", "like", "just", "really", "hello", "hi", "hey", "thank", "sorry", "actually",
    )

    private fun isStopWord(lower: String) = lower in STOP

    private fun looksLikeSecret(s: String): Boolean {
        val t = s.trim()
        if (t.isEmpty()) return false
        return t.split(Regex("\\s+")).any(::tokenLooksLikeSecret)
    }

    private fun tokenLooksLikeSecret(t: String): Boolean {
        if (t.length < 6) return false
        val digit = t.any { it in '0'..'9' }
        val lower = t.any { it in 'a'..'z' }
        val upper = t.any { it in 'A'..'Z' }
        val sym = t.any { !it.isLetterOrDigit() }
        return (digit && sym) || (digit && lower && upper)
    }

    private fun similarTokenLen(a: String, b: String): Boolean {
        val lo = minOf(a.length, b.length)
        val hi = maxOf(a.length, b.length)
        return lo > 0 && hi <= lo * 2 + 2
    }

    private fun tokenize(s: String): List<String> = s.split(Regex("\\s+"))
        .map { it.trim(',', '.', '!', '?', ';', ':', '"', '\'', '(', ')') }
        .filter { it.isNotEmpty() }
}
