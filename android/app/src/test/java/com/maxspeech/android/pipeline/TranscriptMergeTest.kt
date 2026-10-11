package com.maxspeech.android.pipeline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TranscriptMergeTest {

    private fun fold(vararg chunks: Triple<String, Boolean, Long>): String {
        val finals = StringBuilder()
        val interim = StringBuilder()
        val seen = HashSet<Long>()
        for ((text, isFinal, start) in chunks) TranscriptMerge.apply(finals, interim, text, isFinal, start, seen)
        return TranscriptMerge.mergeTrailing(finals.toString(), interim.toString())
    }

    @Test fun lowercasesContinuationWordAtSeam() {
        val out = fold(
            Triple("My main question is", true, 0L),
            Triple("What is that mod", true, 1_500L),
        )
        assertEquals("My main question is what is that mod", out)
    }

    @Test fun keepsCapitalAfterSentenceEnd() {
        assertEquals("That works. What next", TranscriptMerge.joinSeam("That works.", "What next"))
    }

    @Test fun keepsNamesAtSeam() {
        assertEquals("send it to Sarah tomorrow", TranscriptMerge.joinSeam("send it to", "Sarah tomorrow"))
    }

    @Test fun dropsResentFinal() {
        val out = fold(
            Triple("so what do we do", true, 0L),
            Triple("so what do we do", true, 0L),
            Triple("about the pack", true, 2_000L),
        )
        assertEquals("so what do we do about the pack", out)
    }

    @Test fun neverDropsEarlierWordsWhenNextFinalStartsTheSame() {
        // The old prefix-replace turned "So" + "So what do we" into just "So what do we".
        val out = fold(
            Triple("So", true, 0L),
            Triple("So what do we", true, 800L),
        )
        assertTrue(out, out.split(" ").count { it.equals("so", true) } == 2)
    }

    @Test fun trailingInterimOverlapIgnoresPunctuationAndCase() {
        assertEquals("is it any good", TranscriptMerge.mergeTrailing("is it", "It any good"))
        assertEquals("we went home.", TranscriptMerge.mergeTrailing("we went home.", "home"))
    }

    @Test fun trimsRepeatedWindowAtSeam() {
        assertEquals("we should go to the store now", TranscriptMerge.joinSeam("we should go to the", "to the store now"))
    }
}
