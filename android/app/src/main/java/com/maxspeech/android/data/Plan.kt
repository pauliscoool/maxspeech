package com.maxspeech.android.data

import java.util.Calendar
import java.util.TimeZone

enum class PlanTier(val key: String, val label: String, val priceUsd: Int, val weeklyLimit: Int) {
    Free("free", "Free", 0, 1_500),
    Starter("starter", "Starter", 3, 4_500),
    Pro("pro", "Pro", 5, 10_000),
    Max("max", "Max", 10, 25_000);

    companion object {
        fun parse(raw: String?): PlanTier = entries.firstOrNull { it.key == raw?.lowercase() } ?: Free
    }
}

data class PlanStatus(
    val tier: PlanTier,
    val wordsUsed: Int,
    val weekStartsAt: Long,
) {
    val weeklyLimit: Int get() = tier.weeklyLimit
    val wordsRemaining: Int get() = (weeklyLimit - wordsUsed).coerceAtLeast(0)
    val canDictate: Boolean get() = wordsUsed < weeklyLimit
    val priceUsd: Int get() = tier.priceUsd
}

object PlanCalculator {
    fun weekStartUtc(now: Long = System.currentTimeMillis()): Long {
        val cal = Calendar.getInstance(TimeZone.getTimeZone("UTC"))
        cal.timeInMillis = now
        cal.firstDayOfWeek = Calendar.MONDAY
        val day = cal.get(Calendar.DAY_OF_WEEK)
        val diff = if (day == Calendar.SUNDAY) 6 else day - Calendar.MONDAY
        cal.add(Calendar.DAY_OF_YEAR, -diff)
        cal.set(Calendar.HOUR_OF_DAY, 0)
        cal.set(Calendar.MINUTE, 0)
        cal.set(Calendar.SECOND, 0)
        cal.set(Calendar.MILLISECOND, 0)
        return cal.timeInMillis
    }

    fun wordCount(text: String): Int =
        text.trim().split(Regex("\\s+")).count { it.isNotBlank() }

    fun from(user: AuthUser?, wordsUsed: Int): PlanStatus {
        val tier = if (user?.local == true) PlanTier.Pro else PlanTier.parse(user?.planTier)
        return PlanStatus(tier, wordsUsed, weekStartUtc())
    }
}
