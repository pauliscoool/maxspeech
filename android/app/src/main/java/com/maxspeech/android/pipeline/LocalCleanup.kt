package com.maxspeech.android.pipeline

/**
 * Kotlin port of desktop `src-tauri/src/pipeline/tone.rs` local cleanup.
 * Keep behavior identical to the Rust version — both have mirrored tests.
 */
object LocalCleanup {

    private val WEEKDAYS = setOf(
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
        "mon", "tue", "tues", "wed", "thu", "thur", "thurs", "fri", "sat", "sun",
    )

    fun hasNonLatinScript(text: String): Boolean = text.any { c ->
        val n = c.code
        n in 0x0400..0x04FF || n in 0x0500..0x052F || n in 0x0370..0x03FF ||
            n in 0x0590..0x05FF || n in 0x0600..0x06FF || n in 0x0900..0x097F ||
            n in 0x0E00..0x0E7F || n in 0x3040..0x30FF || n in 0x3400..0x9FFF ||
            n in 0xAC00..0xD7AF
    }

    fun localSelfCorrect(text: String): String = localAsrCleanup(selfCorrectMarkers(text))

    fun localAsrCleanup(text: String): String {
        var t = fixSpokenContractions(text)
        t = fixNumeralHomophones(t)
        t = fixCommonHomophones(t)
        t = fixSpokenOkay(t)
        t = fixSpokenPercentWord(t)
        t = fixPercentHeardAsTimes(t)
        return fixCasualAddressCommas(t)
    }

    fun normalizeTerminalPunctuation(text: String, tone: String): String {
        val s = text.trimEnd()
        if (s.isEmpty()) return ""
        val last = s.last()
        if (last in ".!?…" || s.endsWith("...")) return s
        if (last in "\"')]}") return s
        if (last == ':') return s
        if (last == ',' || last == ';') {
            val stem = s.dropLast(1).trimEnd()
            return if (stem.isEmpty()) s else "$stem."
        }
        if (tone == "casual" || !last.isLetterOrDigit()) return s
        return if (looksLikeFinishedSentence(s)) "$s." else s
    }

    private fun looksLikeFinishedSentence(s: String): Boolean {
        val words = words(s)
        if (words.size < 3) return false
        val first = s.firstOrNull { !it.isWhitespace() } ?: return false
        return first.isUpperCase() || words.size >= 5
    }

    // ---- tokenizing helpers ----

    private fun words(s: String): List<String> = s.split(Regex("\\s+")).filter { it.isNotEmpty() }

    private data class Parts(val lead: String, val bare: String, val trail: String)

    private fun splitWordPunct(w: String): Parts {
        var l = 0
        while (l < w.length && w[l] in ",.;:!?\"([") l++
        var r = w.length
        while (r > l && w[r - 1] in ",.;:!?\")]") r--
        return Parts(w.substring(0, l), w.substring(l, r), w.substring(r))
    }

    private fun bareAlpha(word: String): String = word.filter { it.isLetter() }.lowercase()

    private fun copyCasing(src: String, dest: String): String {
        val alpha = src.filter { it.isLetter() }
        if (alpha.isNotEmpty() && alpha.all { it.isUpperCase() }) return dest.uppercase()
        val first = src.firstOrNull { it.isLetter() } ?: return dest
        if (first.isUpperCase() && dest.isNotEmpty()) {
            return dest[0].uppercase() + dest.substring(1)
        }
        return dest
    }

    private fun nextBareLower(words: List<String>, i: Int): String =
        words.getOrNull(i + 1)?.let { splitWordPunct(it).bare.lowercase() }.orEmpty()

    private fun prevBareLower(out: List<String>): String =
        out.lastOrNull()?.let { splitWordPunct(it).bare.lowercase() }.orEmpty()

    // ---- contractions ----

    private val LETS_NEXT = setOf(
        "go", "see", "try", "make", "get", "do", "start", "talk", "look", "check", "wait", "say",
        "take", "put", "add", "keep", "move", "open", "use", "run", "build", "fix", "test", "ship",
        "meet", "eat", "play", "watch", "read", "write", "call", "ask", "stop", "begin",
    )
    private val ID_NEXT = setOf(
        "like", "love", "rather", "be", "have", "want", "prefer", "say", "go", "do", "suggest", "recommend",
    )
    private val CONTRACTIONS = mapOf(
        "dont" to "don't", "doesnt" to "doesn't", "didnt" to "didn't", "wont" to "won't",
        "cant" to "can't", "isnt" to "isn't", "arent" to "aren't", "wasnt" to "wasn't",
        "werent" to "weren't", "havent" to "haven't", "hasnt" to "hasn't", "hadnt" to "hadn't",
        "wouldnt" to "wouldn't", "couldnt" to "couldn't", "shouldnt" to "shouldn't",
        "mustnt" to "mustn't", "im" to "I'm", "ive" to "I've", "youre" to "you're",
        "theyre" to "they're", "weve" to "we've", "youve" to "you've", "theyve" to "they've",
        "thats" to "that's", "whats" to "what's", "whos" to "who's", "wheres" to "where's",
        "heres" to "here's", "theres" to "there's", "shes" to "she's", "hes" to "he's",
        "aint" to "ain't", "hows" to "how's", "whens" to "when's", "oclock" to "o'clock",
    )

    private val ILL_NEXT = setOf(
        "go", "be", "have", "get", "do", "see", "take", "make", "come", "send", "call", "ask", "try",
        "start", "stop", "let", "put", "use", "need", "just", "also", "still", "probably", "maybe",
        "check", "wait", "add", "fix",
    )

    private fun contractionFor(lower: String, next: String): String? = when (lower) {
        "lets" -> if (next in LETS_NEXT) "let's" else null
        "id" -> if (next in ID_NEXT) "I'd" else null
        "ill" -> if (next in ILL_NEXT) "I'll" else null
        else -> CONTRACTIONS[lower]
    }

    private fun fixSpokenContractions(text: String): String {
        val words = words(text)
        if (words.isEmpty()) return text
        val out = ArrayList<String>(words.size)
        for ((i, w) in words.withIndex()) {
            val (lead, bare, trail) = splitWordPunct(w)
            if (bare in setOf("IM", "ID", "OK")) {
                out.add(w)
                continue
            }
            val repl = contractionFor(bare.lowercase(), nextBareLower(words, i))
            out.add(if (repl != null) "$lead${copyCasing(bare, repl)}$trail" else w)
        }
        return out.joinToString(" ")
    }

    // ---- numerals ----

    // Digits stay only where numbers are labels, clock times, percents, ordinals or tech units.
    private val QUANTITY_PREV = setOf(
        "at", "around", "room", "page", "version", "v", "chapter", "item", "number", "line", "port",
        "issue", "age", "aged", "volume", "size", "count", "versus", "vs", "episode", "season",
        "track", "level", "floor", "apartment", "apt", "suite", "gate", "build", "revision", "model",
        "step", "part",
    )
    private val UNIT_NEXT = setOf(
        "times", "time", "percent", "percentage", "pm", "am", "st", "nd", "rd", "th", "km", "kg",
        "lbs", "gb", "mb", "kb", "tb", "ghz", "mhz", "px", "bit", "bits", "bytes", "k",
    )
    private val RANGE_LINK = setOf("to", "and", "or", "through", "thru", "versus", "vs")

    private fun isNumericToken(bare: String) = bare.isNotEmpty() && bare.all { it in '0'..'9' }

    private fun isRangeKeep(prev: String, prev2: String, next: String, next2: String): Boolean {
        if (isNumericToken(prev) || isNumericToken(next)) return true
        return (next in RANGE_LINK && isNumericToken(next2)) || (prev in RANGE_LINK && isNumericToken(prev2))
    }
    private val FOR_NEXT = setOf(
        "the", "a", "an", "you", "me", "us", "them", "him", "her", "it", "this", "that", "those",
        "these", "my", "your", "our", "their", "his", "now", "later", "today", "tomorrow", "tonight",
        "example", "instance", "sure", "real", "once", "all", "each", "every", "some", "any", "more",
        "less", "good", "better", "worse", "work", "school", "dinner", "lunch", "breakfast", "meeting",
        "everyone", "somebody", "someone", "anyone", "anybody", "both", "either", "neither", "free",
        "sale", "what", "which", "whom", "whose", "why", "how", "reference", "context", "review",
        "approval", "testing", "production", "monday", "tuesday", "wednesday", "thursday", "friday",
        "saturday", "sunday",
    )
    private val TOO_NEXT = setOf(
        "much", "many", "late", "bad", "far", "soon", "long", "short", "early", "hard", "easy", "close",
        "big", "small", "often", "fast", "slow", "high", "low", "old", "young", "tired", "busy", "good",
        "well", "loud", "quiet", "hot", "cold", "expensive", "cheap", "heavy", "light", "few", "little",
    )
    private val TO_NEXT = setOf(
        "the", "a", "an", "be", "do", "go", "get", "make", "see", "say", "have", "know", "think", "try",
        "find", "take", "come", "give", "keep", "let", "put", "use", "work", "me", "you", "us", "them",
        "him", "her", "it", "this", "that", "my", "your", "our", "their", "his", "who", "whom", "which",
        "what", "where", "when", "why", "how", "everyone", "someone", "anyone", "anybody", "somebody",
        "everybody", "check", "confirm", "ask", "tell", "call", "send", "write", "read", "open",
        "close", "start", "stop", "run", "build", "deploy", "test", "fix", "add", "remove", "update",
        "install", "launch", "join", "leave", "meet", "eat", "drink", "sleep", "wait", "talk",
        "listen", "look", "watch", "play", "help", "show", "pick", "choose", "decide", "finish",
        "complete",
    )
    /** Spoken 0–20 → words. 21+ and decimals stay numeric. */
    private val PROSE_NUMBER = mapOf(
        "0" to "zero", "1" to "one", "2" to "two", "3" to "three", "4" to "four", "5" to "five",
        "6" to "six", "7" to "seven", "8" to "eight", "9" to "nine", "10" to "ten", "11" to "eleven",
        "12" to "twelve", "13" to "thirteen", "14" to "fourteen", "15" to "fifteen", "16" to "sixteen",
        "17" to "seventeen", "18" to "eighteen", "19" to "nineteen", "20" to "twenty",
    )
    private val SPELLED_SMALL = PROSE_NUMBER.values.toSet()

    private fun isUnitOrQuantityNext(next: String): Boolean =
        (next.isNotEmpty() && next.all { it in '0'..'9' }) || next in UNIT_NEXT

    private fun capitalizeIfNeeded(prevOrig: String?, word: String): String {
        val start = prevOrig == null || prevOrig.trimEnd('"', '\'', ')', ']').let {
            it.endsWith('.') || it.endsWith('!') || it.endsWith('?')
        }
        if (!start || word.isEmpty()) return word
        return word[0].uppercase() + word.substring(1)
    }

    private fun fixNumeralHomophones(text: String): String {
        val words = words(text)
        if (words.isEmpty()) return text
        val out = ArrayList<String>(words.size)
        for ((i, w) in words.withIndex()) {
            val (lead, bare, trail) = splitWordPunct(w)
            val next = nextBareLower(words, i)
            val next2 = words.getOrNull(i + 2)?.let { splitWordPunct(it).bare.lowercase() }.orEmpty()
            val prev = prevBareLower(out)
            // Mirrors Rust: out.get(len.saturating_sub(2)).
            val prev2 = out.getOrNull(maxOf(out.size - 2, 0))?.let { splitWordPunct(it).bare.lowercase() }.orEmpty()
            val keepDigit = prev in QUANTITY_PREV || isUnitOrQuantityNext(next) ||
                isRangeKeep(prev, prev2, next, next2)
            val mapped = when {
                prev == "no" && bare == "1" -> "one"
                keepDigit -> null
                bare == "4" && next in FOR_NEXT -> "for"
                bare == "2" && next in TOO_NEXT -> "too"
                bare == "2" && next in TO_NEXT -> "to"
                bare == "1" && next == "of" -> "one"
                else -> PROSE_NUMBER[bare]
            }
            if (mapped != null) {
                out.add("$lead${capitalizeIfNeeded(out.lastOrNull(), mapped)}$trail")
            } else {
                out.add(w)
            }
        }
        return out.joinToString(" ")
    }

    // ---- homophones ----

    private val ITS_NEXT = setOf(
        "a", "an", "the", "not", "been", "going", "gonna", "ok", "okay", "k", "kay", "just", "really",
        "already", "always", "never", "still", "also", "only", "actually", "currently", "probably",
        "fine", "ready", "done", "time", "working", "broken",
    )
    private val YOURE_NEXT = setOf(
        "going", "gonna", "not", "welcome", "being", "doing", "getting", "looking", "trying",
        "having", "making", "coming", "k", "ok", "okay", "kay", "right", "sure", "fine", "ready",
    )
    private val WHOSE_NEXT = setOf("going", "gonna", "not", "been", "doing", "coming", "got", "here", "there", "that", "this")
    private val FUSED = mapOf(
        "alot" to "a lot", "aswell" to "as well", "atleast" to "at least", "incase" to "in case",
        "eachother" to "each other", "nevermind" to "never mind", "noone" to "no one",
        "everytime" to "every time", "infront" to "in front", "woulda" to "would've",
        "coulda" to "could've", "shoulda" to "should've", "cuz" to "because", "tho" to "though",
        "dunno" to "don't know", "lemme" to "let me", "gimme" to "give me", "outta" to "out of",
        "supposably" to "supposedly", "expresso" to "espresso", "yea" to "yeah",
    )
    private val THEYRE_NEXT = setOf(
        "going", "gonna", "not", "being", "doing", "getting", "looking", "trying", "here", "there",
    )
    private val OF_BLOCK = setOf(
        "course", "the", "a", "an", "this", "that", "it", "his", "her", "our", "my", "your", "their",
    )

    private fun fixCommonHomophones(text: String): String {
        val words = words(text)
        if (words.isEmpty()) return text
        val out = ArrayList<String>(words.size)
        for ((i, w) in words.withIndex()) {
            val (lead, bare, trail) = splitWordPunct(w)
            val lower = bare.lowercase()
            val next = nextBareLower(words, i)
            val prev = prevBareLower(out)

            if (prev in setOf("could", "would", "should", "must", "might") && lower == "of" &&
                next.isNotEmpty() && next !in OF_BLOCK
            ) {
                val modal = out.removeAt(out.lastIndex)
                val m = splitWordPunct(modal)
                val repl = when (prev) {
                    "could" -> "could've"
                    "would" -> "would've"
                    "should" -> "should've"
                    "might" -> "might've"
                    else -> "must've"
                }
                out.add("${m.lead}${copyCasing(m.bare, repl)}${m.trail}")
                continue
            }
            val repl = when {
                lower == "then" && prev in setOf("better", "worse", "rather", "less", "more", "other") -> "than"
                lower == "to" && next in TOO_NEXT -> "too"
                lower == "its" && next in ITS_NEXT -> "it's"
                lower == "your" && next in YOURE_NEXT -> "you're"
                (lower == "their" || lower == "there") && next in THEYRE_NEXT -> "they're"
                lower == "whose" && next in WHOSE_NEXT -> "who's"
                else -> FUSED[lower]
            }
            out.add(if (repl != null) "$lead${copyCasing(bare, repl)}$trail" else w)
        }
        return out.joinToString(" ")
    }

    // ---- spoken "k" / "ok" / "kay" → "okay" ----

    private val OKAY_BLOCK_PREV = setOf(
        "vitamin", "letter", "grade", "key", "press", "hit", "type", "factor", "model", "alt", "ctrl",
        "control", "shift",
    )
    private val KAY_NAME_PREV = setOf("hi", "hey", "dear", "ask", "tell", "call", "from", "with", "thanks", "thank", "meet")
    private val OKAY_NEXT = setOf(
        "thanks", "thank", "cool", "sounds", "got", "great", "sure", "yeah", "yes", "no", "i", "we",
        "you", "lets", "good", "perfect", "fine", "bet",
    )
    private val OKAY_PREV = setOf("thats", "that's", "its", "it's", "im", "i'm", "yeah", "so", "but", "alright", "yes", "no", "ok", "okay")

    private fun fixSpokenOkay(text: String): String {
        val words = words(text)
        if (words.isEmpty()) return text
        val only = words.size == 1
        val out = ArrayList<String>(words.size)
        for ((i, w) in words.withIndex()) {
            val (lead, bare, trail) = splitWordPunct(w)
            val lower = bare.lowercase()
            val next = nextBareLower(words, i)
            val prev = prevBareLower(out)
            if (shouldExpandOkay(lower, prev, next, only)) {
                val alpha = bare.filter { it.isLetter() }
                val cased = if (alpha.isNotEmpty() && alpha.all { it.isLowerCase() }) "okay"
                else capitalizeIfNeeded(out.lastOrNull(), "okay")
                out.add("$lead$cased$trail")
            } else {
                out.add(w)
            }
        }
        return out.joinToString(" ")
    }

    private fun shouldExpandOkay(lower: String, prev: String, next: String, onlyWord: Boolean): Boolean {
        if (lower != "k" && lower != "ok" && lower != "kay") return false
        if (isNumericToken(prev) || isNumericToken(next) || prev in SPELLED_SMALL) return false
        if (prev in OKAY_BLOCK_PREV) return false
        if (lower == "ok" || lower == "k") return true
        if (prev in KAY_NAME_PREV) return false
        if (onlyWord) return true
        return next in OKAY_NEXT || prev in OKAY_PREV
    }

    // ---- percents ----

    private const val TRIM_CHARS = ",.;:!?\"'"

    private fun trimPunct(s: String) = s.trim { it in TRIM_CHARS }

    private fun trailingPunct(s: String) = s.takeLastWhile { it in TRIM_CHARS }

    private fun isNum(s: String) = s.isNotEmpty() && s.all { it in '0'..'9' }

    private fun fixSpokenPercentWord(text: String): String {
        val words = words(text)
        if (words.size < 2) return text
        val out = ArrayList<String>(words.size)
        var i = 0
        while (i < words.size) {
            val bare = trimPunct(words[i])
            val nextPct = words.getOrNull(i + 1)?.let {
                trimPunct(it).lowercase() in setOf("percent", "percentage", "percents")
            } ?: false
            if (isNum(bare) && nextPct) {
                out.add("$bare%${trailingPunct(words[i + 1])}")
                i += 2
                continue
            }
            out.add(words[i])
            i++
        }
        return out.joinToString(" ")
    }

    private val MULT_NEXT = setOf(
        "as", "more", "faster", "slower", "larger", "bigger", "harder", "louder", "cheaper", "better",
        "worse", "higher", "lower", "greater", "smaller", "again", "over", "before", "after", "until",
        "the", "a", "an",
    )
    private val PERCENT_PREV = setOf(
        "at", "by", "of", "about", "around", "roughly", "exactly", "only", "to", "from", "discount",
        "off", "plus", "minus", "rate", "tax", "tip", "interest", "fee", "up", "down", "nearly",
        "almost", "over", "under", "above", "below", "was", "is", "be", "been",
    )

    private fun fixPercentHeardAsTimes(text: String): String {
        val words = words(text)
        if (words.size < 2) return text
        val out = ArrayList<String>(words.size)
        var i = 0
        while (i < words.size) {
            val bare = trimPunct(words[i])
            val nextTimes = words.getOrNull(i + 1)?.let {
                trimPunct(it).lowercase() in setOf("times", "time")
            } ?: false
            if (isNum(bare) && nextTimes) {
                val prev = out.lastOrNull()?.let { trimPunct(it).lowercase() }.orEmpty()
                val afterIsMult = words.getOrNull(i + 2)?.let { trimPunct(it).lowercase() in MULT_NEXT } ?: false
                val bareUtterance = words.size <= 2
                if (!afterIsMult && (prev in PERCENT_PREV || bareUtterance || prev.isEmpty())) {
                    out.add("$bare%${trailingPunct(words[i + 1])}")
                    i += 2
                    continue
                }
            }
            out.add(words[i])
            i++
        }
        return out.joinToString(" ")
    }

    // ---- casual address words ----

    private val CASUAL_AFTER = setOf("bro", "bruh", "dude", "fam", "man", "bestie")
    private val CASUAL_BEFORE = setOf("bro", "bruh", "dude", "fam", "bestie")

    fun fixCasualAddressCommas(text: String): String {
        val words = words(text)
        val out = ArrayList<String>(words.size)
        for ((i, w) in words.withIndex()) {
            val (lead, bare, trail) = splitWordPunct(w)
            val lower = bare.lowercase()
            if (lower in CASUAL_BEFORE && lead.isEmpty() && out.isNotEmpty() && out.last().endsWith(',')) {
                out[out.lastIndex] = out.last().dropLast(1)
            }
            if (lower in CASUAL_AFTER && trail == "," && i + 1 < words.size) {
                out.add("$lead$bare")
            } else {
                out.add(w)
            }
        }
        return out.joinToString(" ")
    }

    // ---- spoken self-corrections ----

    private enum class Kind { Generic, IMean, IMet }

    private data class Marker(val idx: Int, val contentStart: Int, val kind: Kind)

    private val MARKERS = listOf(
        "oh no i meant" to Kind.Generic, "oh no, i meant" to Kind.Generic,
        "oh wait i meant" to Kind.Generic, "oh wait, i meant" to Kind.Generic,
        "no wait i meant" to Kind.Generic, "no, i meant" to Kind.Generic,
        "no i meant" to Kind.Generic, "wait i meant" to Kind.Generic,
        "wait, i meant" to Kind.Generic, "actually i meant" to Kind.Generic,
        "actually, i meant" to Kind.Generic, "sorry i meant" to Kind.Generic,
        "sorry, i meant" to Kind.Generic, "scratch that i meant" to Kind.Generic,
        "correction:" to Kind.Generic, "correction" to Kind.Generic,
        "i meant" to Kind.Generic, "i mean" to Kind.IMean, "i met" to Kind.IMet,
        "or rather" to Kind.Generic,
    )

    private fun selfCorrectMarkers(text: String): String {
        val lower = text.lowercase()
        // Lowercasing must not shift indices, or slicing the original would be wrong.
        if (lower.length != text.length) return text
        val m = findLastCorrectionMarker(lower) ?: return text

        val before = text.substring(0, m.idx).trimEnd()
        var correction = text.substring(m.contentStart).trim()
        correction = correction.trimStart(',', ':', ';', '.').trim()
        correction = correction.trimEnd('.', '!', '?', ',').trim()
        if (before.isEmpty() || correction.isEmpty()) return text
        if (m.kind == Kind.IMean && looksLikeDiscourseFiller(correction)) return text
        if (m.kind == Kind.IMet && !looksLikeRestatement(before, correction)) return text

        val stemEnd = before.trimEnd().let { s ->
            var k = s.length
            while (k > 0 && s[k - 1] in ".!?,;:") k--
            k
        }
        val stem = before.substring(0, stemEnd)
        val trailing = before.substring(stemEnd)
        val words = words(stem).toMutableList()
        if (words.isEmpty()) return "$correction$trailing"
        val corr = words(correction)

        if (!applyWeekdayCorrection(words, corr) &&
            !applySingleNameCorrection(words, corr) &&
            !applySuffixRestatement(words, corr)
        ) {
            val n = minOf(corr.size, words.size)
            repeat(n) { words.removeAt(words.lastIndex) }
            words.addAll(corr)
        }
        return words.joinToString(" ") + trailing
    }

    private fun findLastCorrectionMarker(lower: String): Marker? {
        var best: Marker? = null
        var bestLen = 0
        for ((phrase, kind) in MARKERS) {
            var from = 0
            while (true) {
                val idx = lower.indexOf(phrase, from)
                if (idx < 0) break
                from = idx + 1
                if (idx > 0 && lower[idx - 1].isLetterOrDigit()) continue
                val cs = markerContentStart(lower, idx + phrase.length, phrase) ?: continue
                val b = best
                if (b == null || cs > b.contentStart ||
                    (cs == b.contentStart && phrase.length > bestLen) ||
                    (cs == b.contentStart && phrase.length == bestLen && idx < b.idx)
                ) {
                    best = Marker(idx, cs, kind)
                    bestLen = phrase.length
                }
            }
        }
        for (alt in listOf(" no wait ", " wait no ", " wait actually ")) {
            val idx = lower.lastIndexOf(alt)
            if (idx < 0) continue
            val cs = idx + alt.length
            if (cs >= lower.length) continue
            val b = best
            if (b == null || cs > b.contentStart || (cs == b.contentStart && alt.length > bestLen)) {
                best = Marker(idx, cs, Kind.Generic)
                bestLen = alt.length
            }
        }
        return best
    }

    private fun markerContentStart(lower: String, afterPhrase: Int, phrase: String): Int? {
        if (afterPhrase >= lower.length) return null
        val rest = lower.substring(afterPhrase)
        if (phrase.endsWith(':')) {
            val trimmed = rest.trimStart()
            if (trimmed.isEmpty()) return null
            return afterPhrase + (rest.length - trimmed.length)
        }
        if (rest[0].isLetterOrDigit()) return null
        var i = 0
        while (i < rest.length && (rest[i] in ",:;.!?\"'" || rest[i].isWhitespace())) i++
        if (i == 0 || i >= rest.length) return null
        if (rest.substring(0, i).none { it.isWhitespace() }) return null
        return afterPhrase + i
    }

    private fun looksLikeDiscourseFiller(correction: String): Boolean {
        val w = words(correction)
        if (w.isEmpty() || w.size > 3) return true
        return bareAlpha(w[0]) in setOf(
            "it", "that", "because", "we", "you", "i", "the", "this", "there",
            "he", "she", "they", "weve", "im", "its", "thats",
        )
    }

    private fun looksLikeNameToken(word: String): Boolean {
        val bare = word.filter { it.isLetter() || it == '-' }
        if (bare.length !in 2..40) return false
        return bare[0].isUpperCase() && bare.all { it.isLetter() || it == '-' }
    }

    private fun looksLikeRestatement(before: String, correction: String): Boolean {
        val a = words(before).map(::bareAlpha).filter { it.isNotEmpty() }
        val b = words(correction).map(::bareAlpha).filter { it.isNotEmpty() }
        if (a.isEmpty() || b.isEmpty()) return false
        var overlap = 0
        while (overlap < a.size && overlap < b.size && a[a.size - 1 - overlap] == b[b.size - 1 - overlap]) overlap++
        if (overlap >= 1 && (a.size > overlap || b.size > overlap)) return true
        if (b.size == 1) {
            val rep = b[0]
            val bw = words(before)
            if (bw.size >= 2) {
                val nonInitial = bw.drop(1).any { looksLikeNameToken(it) && !bareAlpha(it).equals(rep, true) }
                val leading = looksLikeNameToken(bw[0]) && bw.size >= 3 && !bareAlpha(bw[0]).equals(rep, true)
                if (nonInitial || leading) return true
            }
        }
        if (a.size == b.size && a.size >= 2) {
            val diffs = a.zip(b).count { (x, y) -> x != y }
            return diffs in 1..2
        }
        return false
    }

    private fun applyWeekdayCorrection(words: MutableList<String>, corr: List<String>): Boolean {
        val c = corr.indexOfLast { bareAlpha(it) in WEEKDAYS }
        if (c < 0) return false
        val m = words.indexOfLast { bareAlpha(it) in WEEKDAYS }
        if (m < 0) return false
        words[m] = corr[c]
        return true
    }

    private fun applySingleNameCorrection(words: MutableList<String>, corr: List<String>): Boolean {
        if (corr.size != 1) return false
        val rep = corr[0]
        val repBare = bareAlpha(rep)
        if (repBare.isEmpty()) return false
        val i = words.indexOfLast { looksLikeNameToken(it) && bareAlpha(it) != repBare }
        if (i < 0) return false
        words[i] = rep
        return true
    }

    private fun applySuffixRestatement(words: MutableList<String>, corr: List<String>): Boolean {
        if (corr.isEmpty()) return false
        var overlap = 0
        while (overlap < words.size && overlap < corr.size &&
            bareAlpha(words[words.size - 1 - overlap]) == bareAlpha(corr[corr.size - 1 - overlap])
        ) overlap++
        if (overlap == 0) return false
        val aHead = words.size - overlap
        val bHead = corr.size - overlap
        if (aHead == 0 && bHead == 0) return false
        if (overlap >= 2 || (aHead <= 2 && bHead <= 2)) {
            words.clear()
            words.addAll(corr)
            return true
        }
        return false
    }
}
