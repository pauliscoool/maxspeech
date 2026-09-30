package com.maxspeech.android.pipeline

// Session ownership prevents late processing and repeated confirmation from inserting twice.
internal class DictationSession {
    @Volatile private var epoch = 0L
    private var finishing = false
    private var pasted = false

    fun next(): Long {
        epoch++
        finishing = false
        pasted = false
        return epoch
    }

    fun beginFinish(): Long? {
        if (finishing) return null
        finishing = true
        return epoch
    }

    fun isCurrent(token: Long): Boolean = token == epoch

    fun claimPaste(token: Long): Boolean {
        if (!isCurrent(token) || pasted) return false
        pasted = true
        return true
    }
}
