package com.maxspeech.android.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.StyleGroupId
import com.maxspeech.android.ui.plan.UsageCard
import com.maxspeech.android.ui.theme.DisplayHero
import com.maxspeech.android.ui.theme.LocalMsColors
import java.util.Calendar

private const val SlideHeight = 188
private const val TypingWpm = 40
private const val SpeakingWpm = 150

private val TIPS = listOf(
    "Hold the mic, talk, let go.",
    "Dictate a list and it types numbered lines.",
    "Add names to your Dictionary.",
)

/** Swipeable strip at the top of Home: usage, this-week stats, writing style, tips. */
@Composable
fun HomeSlides(
    plan: PlanStatus,
    history: List<HistoryEntity>,
    settings: AppSettings,
    onOpenPlan: () -> Unit,
    onSelectStyleGroup: (StyleGroupId) -> Unit,
    onOpenStyleEdit: () -> Unit,
) {
    val c = LocalMsColors.current
    val pager = rememberPagerState { 4 }
    Column(Modifier.fillMaxWidth()) {
        HorizontalPager(
            state = pager,
            pageSpacing = 12.dp,
            modifier = Modifier.fillMaxWidth().height(SlideHeight.dp),
        ) { page ->
            val fill = Modifier.fillMaxSize()
            when (page) {
                0 -> UsageCard(plan, fill, onClick = onOpenPlan)
                1 -> StatsSlide(history, fill)
                2 -> HomeStyleHero(settings, onSelectStyleGroup, onOpenStyleEdit, fill)
                else -> TipsSlide(fill)
            }
        }
        Spacer(Modifier.height(10.dp))
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            repeat(pager.pageCount) { i ->
                Box(
                    Modifier
                        .padding(horizontal = 3.dp)
                        .size(if (i == pager.currentPage) 7.dp else 6.dp)
                        .clip(CircleShape)
                        .background(if (i == pager.currentPage) c.turquoise else c.hairline),
                )
            }
        }
    }
}

@Composable
private fun SlideShell(modifier: Modifier, content: @Composable () -> Unit) {
    val c = LocalMsColors.current
    Column(
        modifier
            .clip(RoundedCornerShape(20.dp))
            .background(c.surface)
            .border(1.dp, c.hairline, RoundedCornerShape(20.dp))
            .padding(20.dp),
    ) { content() }
}

@Composable
private fun StatsSlide(history: List<HistoryEntity>, modifier: Modifier) {
    val c = LocalMsColors.current
    val stats = remember(history) { HomeStats.from(history) }
    SlideShell(modifier) {
        Text("%,d".format(stats.totalWords), style = DisplayHero, color = c.turquoise)
        Text("words dictated in total", color = c.textDim, fontSize = 14.sp)
        Spacer(Modifier.height(12.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Stat("Day streak", stats.streak.toString(), Modifier.weight(1f))
            Stat("Today", "%,d words".format(stats.todayWords), Modifier.weight(1f))
            Stat("Time saved", formatMinutes(stats.minutesSaved), Modifier.weight(1f))
        }
    }
}

@Composable
private fun Stat(label: String, value: String, modifier: Modifier) {
    val c = LocalMsColors.current
    Column(modifier) {
        Text(value, color = c.text, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
        Text(label, color = c.textDim, fontSize = 12.sp)
    }
}

@Composable
private fun TipsSlide(modifier: Modifier) {
    val c = LocalMsColors.current
    SlideShell(modifier) {
        Text("Tips", color = c.text, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
        Spacer(Modifier.height(10.dp))
        TIPS.forEach {
            Text("•  $it", color = c.textDim, fontSize = 13.sp, modifier = Modifier.padding(bottom = 6.dp))
        }
    }
}

private fun formatMinutes(min: Int): String =
    if (min < 60) "$min min" else "%.1f h".format(min / 60f)

internal data class HomeStats(val totalWords: Int, val todayWords: Int, val streak: Int, val minutesSaved: Int) {
    companion object {
        fun from(history: List<HistoryEntity>, now: Long = System.currentTimeMillis()): HomeStats {
            val ok = history.filter { !it.failed }
            val words = ok.sumOf { wordCount(it.text) }
            val today = dayIndex(now)
            val byDay = ok.groupBy { dayIndex(it.createdAt) }
            val todayWords = byDay[today].orEmpty().sumOf { wordCount(it.text) }
            // A streak survives until the end of today, so start from yesterday when nothing is said yet.
            var d = if (byDay.containsKey(today)) today else today - 1
            var streak = 0
            while (byDay.containsKey(d)) {
                streak++
                d--
            }
            val saved = (words / TypingWpm.toFloat() - words / SpeakingWpm.toFloat()).toInt().coerceAtLeast(0)
            return HomeStats(words, todayWords, streak, saved)
        }

        private fun wordCount(s: String) = s.trim().split(Regex("\\s+")).count { it.isNotEmpty() }

        private fun dayIndex(ms: Long): Long {
            val cal = Calendar.getInstance().apply { timeInMillis = ms }
            val offset = (cal.get(Calendar.ZONE_OFFSET) + cal.get(Calendar.DST_OFFSET)).toLong()
            return Math.floorDiv(ms + offset, 86_400_000L)
        }
    }
}
