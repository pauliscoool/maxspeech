package com.maxspeech.android.pipeline

/** Port of desktop `pipeline/commands.rs` — keep phrases identical. */
object VoiceCommands {

    sealed class Result {
        object ScratchThat : Result()
        data class Rewrite(val instruction: String) : Result()
        data class InsertText(val text: String) : Result()
    }

    private val SCRATCH = setOf("scratch that", "undo that", "delete that", "never mind")
    private val INSERTS = mapOf(
        "new line" to "\n", "newline" to "\n", "new paragraph" to "\n\n",
        "period" to ".", "full stop" to ".", "comma" to ",", "question mark" to "?",
        "exclamation mark" to "!", "exclamation point" to "!",
    )
    private val REWRITE_PREFIXES = listOf("make it ", "make that ", "rewrite as ", "change to ", "make this ")

    fun check(text: String): Result? {
        val t = text.lowercase().trim().trimEnd('.')
        if (t in SCRATCH) return Result.ScratchThat
        INSERTS[t]?.let { return Result.InsertText(it) }
        for (p in REWRITE_PREFIXES) {
            if (t.startsWith(p) && t.length > p.length) return Result.Rewrite(t.removePrefix(p))
        }
        return null
    }
}
