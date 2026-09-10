package com.maxspeech.android.ui.style

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.BasicTextField
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.AppProfileEntity
import com.maxspeech.android.data.SnippetEntity
import com.maxspeech.android.ui.components.GlassChip
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall
import com.maxspeech.android.ui.theme.Turquoise

val TONES = listOf("casual", "formal", "code", "prose", "default")

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun StyleScreen(
    selected: String,
    onSelect: (String) -> Unit,
    profiles: List<AppProfileEntity>,
    onToggle: (AppProfileEntity) -> Unit,
    snippets: List<SnippetEntity>,
    dictionary: List<String>,
    onAddSnippet: (String, String) -> Unit,
    onDeleteSnippet: (String) -> Unit,
    onAddWord: (String) -> Unit,
    onDeleteWord: (String) -> Unit,
) {
    val c = LocalMsColors.current
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
    ) {
        Text("Style", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(8.dp))
        Text("App-aware tone, same idea as desktop — package name instead of .exe.", color = c.textDim, fontSize = 14.sp)
        Spacer(Modifier.height(20.dp))
        Text("Voice", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(10.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            TONES.forEach { tone ->
                GlassChip(tone.replaceFirstChar { it.uppercase() }, selected = tone == selected) { onSelect(tone) }
            }
        }
        Spacer(Modifier.height(24.dp))
        Text("Apps", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(10.dp))
        profiles.forEach { p ->
            GlassSurface(Modifier.fillMaxWidth().padding(bottom = 8.dp)) {
                Row(
                    Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(p.packagePattern.substringAfterLast('.'), color = c.text, fontSize = 15.sp)
                        Text(p.tone.replaceFirstChar { it.uppercase() }, color = c.textDim, fontSize = 12.sp)
                    }
                    Switch(
                        checked = p.enabled,
                        onCheckedChange = { onToggle(p) },
                        colors = SwitchDefaults.colors(checkedTrackColor = Turquoise),
                    )
                }
            }
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
                    textStyle = androidx.compose.ui.text.TextStyle(color = c.text, fontSize = 14.sp),
                    decorationBox = { inner ->
                        if (trigger.isEmpty()) Text("Trigger (e.g. /sig)", color = c.textDim)
                        inner()
                    },
                )
                BasicTextField(
                    value = expansion,
                    onValueChange = { expansion = it },
                    textStyle = androidx.compose.ui.text.TextStyle(color = c.text, fontSize = 14.sp),
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
        snippets.forEach { s ->
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
                    textStyle = androidx.compose.ui.text.TextStyle(color = c.text, fontSize = 14.sp),
                    modifier = Modifier.weight(1f),
                    decorationBox = { inner ->
                        if (word.isEmpty()) Text("Add a word Deepgram should know", color = c.textDim)
                        inner()
                    },
                )
                TextButton(onClick = { onAddWord(word); word = "" }) { Text("Add", color = Turquoise) }
            }
        }
        dictionary.forEach { w ->
            GlassSurface(Modifier.fillMaxWidth().padding(top = 8.dp)) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(w, color = c.text, modifier = Modifier.weight(1f))
                    TextButton(onClick = { onDeleteWord(w) }) { Text("Remove", color = c.error) }
                }
            }
        }
        Spacer(Modifier.height(96.dp))
    }
}
