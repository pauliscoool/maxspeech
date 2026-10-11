package com.maxspeech.android.ui.plan

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.PlanStatus
import com.maxspeech.android.data.PlanTier
import com.maxspeech.android.ui.theme.DisplayHero
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors

private val TierPerks = mapOf(
    PlanTier.Free to "Try MaxSpeech everywhere you type",
    PlanTier.Starter to "Daily messages and email, multilingual",
    PlanTier.Pro to "Heavy daily dictation",
    PlanTier.Max to "Unlimited-feeling, for writing all day",
)

@Composable
fun PlanScreen(
    plan: PlanStatus,
    onBack: () -> Unit,
    onUpgrade: (PlanTier) -> Unit,
    onManage: () -> Unit,
    onRefresh: () -> Unit,
) {
    val c = LocalMsColors.current
    LaunchedEffect(Unit) { onRefresh() }
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(top = 8.dp, bottom = 120.dp),
    ) {
        IconButton(onClick = onBack, modifier = Modifier.size(40.dp)) {
            Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = c.text)
        }
        Text("Plan & billing", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(16.dp))

        UsageCard(plan)
        Spacer(Modifier.height(22.dp))

        Text("Plans", color = c.textDim, fontSize = 13.sp, modifier = Modifier.padding(bottom = 8.dp))
        PlanTier.entries.forEach { tier ->
            TierCard(
                tier = tier,
                current = tier == plan.tier,
                upgrade = tier.ordinal > plan.tier.ordinal,
                onClick = { onUpgrade(tier) },
            )
            Spacer(Modifier.height(10.dp))
        }

        if (plan.tier != PlanTier.Free) {
            Spacer(Modifier.height(6.dp))
            Box(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .border(1.dp, c.hairline, RoundedCornerShape(14.dp))
                    .clickable(onClick = onManage)
                    .padding(vertical = 14.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text("Manage subscription", color = c.text, fontSize = 15.sp, fontWeight = FontWeight.Medium)
            }
        }
        Text(
            "Payments are handled securely by Stripe on maxspeech.vercel.app. Your plan updates here automatically after checkout.",
            color = c.textDim,
            fontSize = 12.sp,
            modifier = Modifier.padding(top = 14.dp),
        )
    }
}

@Composable
fun UsageCard(plan: PlanStatus, modifier: Modifier = Modifier, onClick: (() -> Unit)? = null) {
    val c = LocalMsColors.current
    val frac = (plan.wordsUsed.toFloat() / plan.weeklyLimit.coerceAtLeast(1)).coerceIn(0f, 1f)
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(c.surface)
            .border(1.dp, c.hairline, RoundedCornerShape(20.dp))
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(20.dp),
    ) {
        Text("%,d words".format(plan.wordsUsed), style = DisplayHero, color = c.turquoise)
        Text("spoken this week", color = c.textDim, fontSize = 14.sp)
        Spacer(Modifier.height(14.dp))
        Box(
            Modifier
                .fillMaxWidth()
                .height(6.dp)
                .clip(RoundedCornerShape(3.dp))
                .background(c.surface2),
        ) {
            Box(
                Modifier
                    .fillMaxWidth(frac)
                    .fillMaxHeight()
                    .background(if (frac >= 0.9f) c.orange else c.turquoise),
            )
        }
        Spacer(Modifier.height(8.dp))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(
                "%,d of %,d · resets Monday".format(plan.wordsUsed, plan.weeklyLimit),
                color = c.textDim,
                fontSize = 12.sp,
                modifier = Modifier.weight(1f),
            )
            Text(
                plan.tier.label + if (onClick != null) " ›" else "",
                color = c.text,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}

@Composable
private fun TierCard(tier: PlanTier, current: Boolean, upgrade: Boolean, onClick: () -> Unit) {
    val c = LocalMsColors.current
    val shape = RoundedCornerShape(16.dp)
    Row(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.surface)
            .border(1.dp, if (current) c.turquoise else c.hairline, shape)
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(tier.label, color = c.text, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                if (current) Text("Current", color = c.turquoise, fontSize = 12.sp)
            }
            Text(
                "%,d words / week · %s".format(tier.weeklyLimit, TierPerks[tier].orEmpty()),
                color = c.textDim,
                fontSize = 12.sp,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
        Column(horizontalAlignment = Alignment.End) {
            Text(
                if (tier.priceUsd == 0) "Free" else "$${tier.priceUsd}/mo",
                color = c.text,
                fontSize = 15.sp,
                fontWeight = FontWeight.Medium,
            )
            if (upgrade) {
                Text(
                    "Upgrade",
                    color = Color.Black,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier
                        .padding(top = 6.dp)
                        .clip(RoundedCornerShape(20.dp))
                        .background(c.turquoise)
                        .clickable(onClick = onClick)
                        .padding(horizontal = 14.dp, vertical = 6.dp),
                )
            }
        }
    }
}
