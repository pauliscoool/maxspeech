package com.maxspeech.android.ui.style

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.AppProfileEntity
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.SnippetEntity
import com.maxspeech.android.data.StyleGroups
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall
import com.maxspeech.android.ui.theme.Turquoise

val TONES = StyleGroups.tones

@Composable
fun StyleScreen(
    settings: AppSettings,
    selected: String,
    onSelect: (String) -> Unit,
    profiles: List<AppProfileEntity>,
    onToggle: (AppProfileEntity) -> Unit,
    onProfileTone: (AppProfileEntity, String) -> Unit,
    snippets: List<SnippetEntity>,
    dictionary: List<String>,
    onAddSnippet: (String, String) -> Unit,
    onDeleteSnippet: (String) -> Unit,
    onAddWord: (String) -> Unit,
    onDeleteWord: (String) -> Unit,
    onStyleGroups: (String, String, String, String) -> Unit,
) {
    val c = LocalMsColors.current
    var query by remember { mutableStateOf("") }
    val q = query.trim()
    val filteredProfiles = remember(profiles, q) {
        if (q.isEmpty()) profiles
        else profiles.filter {
            it.packagePattern.contains(q, true) ||
                it.tone.contains(q, true) ||
                friendlyAppName(it.packagePattern).contains(q, true)
        }
    }
    val filteredDict = remember(dictionary, q) {
        if (q.isEmpty()) dictionary else dictionary.filter { it.contains(q, true) }
    }
    val filteredSnippets = remember(snippets, q) {
        if (q.isEmpty()) snippets
        else snippets.filter {
            it.trigger.contains(q, true) || it.expansion.contains(q, true)
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(top = 12.dp, bottom = 120.dp),
    ) {
        Text("Style", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(8.dp))
        Text(
            "App-aware voice for MaxSpeech — search apps, pick a tone, teach words it should know.",
            color = c.textDim,
            fontSize = 14.sp,
        )
        Spacer(Modifier.height(16.dp))

        GlassSurface(Modifier.fillMaxWidth()) {
            BasicTextField(
                value = query,
                onValueChange = { query = it },
                singleLine = true,
                textStyle = TextStyle(color = c.text, fontSize = 15.sp),
                cursorBrush = SolidColor(Turquoise),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 14.dp),
                decorationBox = { inner ->
                    Box {
                        if (query.isEmpty()) Text("Search apps, tones, words…", color = c.textDim)
                        inner()
                    }
                },
            )
        }

        Spacer(Modifier.height(22.dp))
        Text("Default voice", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(10.dp))
        ToneDropdown(
            value = selected,
            onSelect = onSelect,
            modifier = Modifier.fillMaxWidth(),
        )

        Spacer(Modifier.height(24.dp))
        Text("App groups", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(10.dp))
        StyleGroupsEditor(
            messaging = settings.styleToneMessaging,
            email = settings.styleToneEmail,
            work = settings.styleToneWork,
            social = settings.styleToneSocial,
            onChange = onStyleGroups,
            compact = true,
        )

        Spacer(Modifier.height(24.dp))
        Text("Apps", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(10.dp))
        filteredProfiles.forEach { p ->
            GlassSurface(Modifier.fillMaxWidth().padding(bottom = 8.dp)) {
                Column(Modifier.padding(horizontal = 14.dp, vertical = 10.dp)) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(friendlyAppName(p.packagePattern), color = c.text, fontSize = 15.sp)
                            Text(p.packagePattern, color = c.textDim, fontSize = 11.sp)
                        }
                        Switch(
                            checked = p.enabled,
                            onCheckedChange = { onToggle(p) },
                            colors = SwitchDefaults.colors(checkedTrackColor = Turquoise),
                        )
                    }
                    Spacer(Modifier.height(8.dp))
                    ToneDropdown(
                        value = p.tone,
                        onSelect = { onProfileTone(p, it) },
                        compact = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
        if (filteredProfiles.isEmpty()) {
            Text("No apps match", color = c.textDim, fontSize = 13.sp)
        }

        Spacer(Modifier.height(24.dp))
        Text("Snippets", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(10.dp))
        var trigger by remember { mutableStateOf("") }
        var expansion by remember { mutableStateOf("") }
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                BasicTextField(
                    value = trigger,
                    onValueChange = { trigger = it },
                    singleLine = true,
                    textStyle = TextStyle(color = c.text, fontSize = 14.sp),
                    decorationBox = { inner ->
                        if (trigger.isEmpty()) Text("Trigger (e.g. /sig)", color = c.textDim)
                        inner()
                    },
                )
                BasicTextField(
                    value = expansion,
                    onValueChange = { expansion = it },
                    textStyle = TextStyle(color = c.text, fontSize = 14.sp),
                    decorationBox = { inner ->
                        if (expansion.isEmpty()) Text("Expansion", color = c.textDim)
                        inner()
                    },
                )
                TextButton(onClick = {
                    onAddSnippet(trigger, expansion)
                    trigger = ""
                    expansion = ""
                }) { Text("Add snippet", color = Turquoise) }
            }
        }
        filteredSnippets.forEach { s ->
            GlassSurface(Modifier.fillMaxWidth().padding(top = 8.dp)) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(s.trigger, color = c.turquoise, fontSize = 13.sp)
                        Text(s.expansion, color = c.text, fontSize = 14.sp)
                    }
                    TextButton(onClick = { onDeleteSnippet(s.trigger) }) { Text("Remove", color = c.error) }
                }
            }
        }

        Spacer(Modifier.height(24.dp))
        Text("Dictionary", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(10.dp))
        var word by remember { mutableStateOf("") }
        GlassSurface(Modifier.fillMaxWidth()) {
            Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                BasicTextField(
                    value = word,
                    onValueChange = { word = it },
                    singleLine = true,
                    textStyle = TextStyle(color = c.text, fontSize = 14.sp),
                    modifier = Modifier.weight(1f),
                    decorationBox = { inner ->
                        if (word.isEmpty()) {
                            Text("Add a word MaxSpeech should know", color = c.textDim)
                        }
                        inner()
                    },
                )
                TextButton(onClick = { onAddWord(word); word = "" }) { Text("Add", color = Turquoise) }
            }
        }
        filteredDict.forEach { w ->
            GlassSurface(Modifier.fillMaxWidth().padding(top = 8.dp)) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(w, color = c.text, modifier = Modifier.weight(1f))
                    TextButton(onClick = { onDeleteWord(w) }) { Text("Remove", color = c.error) }
                }
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun ToneDropdown(
    value: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val c = LocalMsColors.current
    var open by remember { mutableStateOf(false) }
    val chevron by animateFloatAsState(
        targetValue = if (open) 180f else 0f,
        animationSpec = tween(220, easing = FastOutSlowInEasing),
        label = "toneChevron",
    )
    val shape = RoundedCornerShape(if (compact) 16.dp else 20.dp)

    Column(
        modifier
            .clip(shape)
            .border(1.dp, if (open) Turquoise.copy(alpha = 0.40f) else c.hairline, shape)
            .background(c.surface)
            .animateContentSize(animationSpec = tween(240, easing = FastOutSlowInEasing)),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable { open = !open }
                .padding(
                    horizontal = if (compact) 12.dp else 16.dp,
                    vertical = if (compact) 10.dp else 14.dp,
                ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                StyleGroups.labelForTone(value),
                color = c.text,
                fontSize = if (compact) 13.sp else 15.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            Icon(
                Icons.Filled.ExpandMore,
                contentDescription = if (open) "Collapse" else "Expand",
                tint = if (open) Turquoise else c.textDim,
                modifier = Modifier
                    .size(if (compact) 18.dp else 22.dp)
                    .rotate(chevron),
            )
        }

        AnimatedVisibility(
            visible = open,
            enter = expandVertically(tween(220, easing = FastOutSlowInEasing)) + fadeIn(tween(180)),
            exit = shrinkVertically(tween(180, easing = FastOutSlowInEasing)) + fadeOut(tween(120)),
        ) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(c.bgSoft)
                    .padding(horizontal = if (compact) 6.dp else 8.dp)
                    .padding(bottom = if (compact) 6.dp else 8.dp),
            ) {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 10.dp)
                        .height(1.dp)
                        .background(c.hairline),
                )
                Spacer(Modifier.height(6.dp))
                TONES.forEach { tone ->
                    val selected = tone.equals(value, ignoreCase = true)
                    val rowShape = RoundedCornerShape(14.dp)
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .padding(vertical = 2.dp)
                            .clip(rowShape)
                            .background(if (selected) Turquoise.copy(alpha = 0.14f) else Color.Transparent)
                            .clickable {
                                onSelect(tone)
                                open = false
                            }
                            .padding(
                                horizontal = if (compact) 10.dp else 12.dp,
                                vertical = if (compact) 9.dp else 11.dp,
                            ),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Text(
                            StyleGroups.labelForTone(tone),
                            color = if (selected) Turquoise else c.text,
                            fontSize = if (compact) 13.sp else 14.sp,
                            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
                            modifier = Modifier.weight(1f),
                        )
                        if (selected) {
                            Icon(
                                Icons.Filled.Check,
                                contentDescription = null,
                                tint = Turquoise,
                                modifier = Modifier.size(16.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun friendlyAppName(pkg: String): String {
    val last = pkg.substringAfterLast('.').replace('_', ' ')
    return last.replaceFirstChar { it.uppercase() }
}
