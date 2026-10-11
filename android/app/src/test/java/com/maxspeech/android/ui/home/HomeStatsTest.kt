package com.maxspeech.android.ui.home

import com.maxspeech.android.data.HistoryEntity
import org.junit.Assert.assertEquals
import org.junit.Test

class HomeStatsTest {
    private val day = 86_400_000L
    private val now = 1_800_000_000_000L

    private fun row(text: String, ago: Long, failed: Boolean = false) =
        HistoryEntity(text = text, appName = "", createdAt = now - ago, failed = failed)

    @Test
    fun countsWordsAndSkipsFailed() {
        val s = HomeStats.from(listOf(row("one two three", 0), row("ignored words", 0, failed = true)), now)
        assertEquals(3, s.totalWords)
        assertEquals(3, s.todayWords)
    }

    @Test
    fun streakRunsBackFromYesterdayWhenQuietToday() {
        val s = HomeStats.from(listOf(row("a", day), row("b", 2 * day), row("c", 4 * day)), now)
        assertEquals(2, s.streak)
        assertEquals(0, s.todayWords)
    }
}
