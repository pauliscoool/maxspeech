package com.maxspeech.android.ui.snippets

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.SnippetEntity
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun SnippetsScreen(
    snippets: List<SnippetEntity>,
    onAdd: (String, String) -> Unit,
    onDelete: (String) -> Unit,
) {
    val c = LocalMsColors.current
    var trigger by remember { mutableStateOf("") }
    var expansion by remember { mutableStateOf("") }
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(top = 12.dp, bottom = 120.dp),
    ) {
        Text("Snippets", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(8.dp))
        Text("Say a short trigger. MaxSpeech expands it.", color = c.textDim, fontSize = 14.sp)
        Spacer(Modifier.height(16.dp))
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                BasicTextField(
                    value = trigger,
                    onValueChange = { trigger = it },
                    singleLine = true,
                    textStyle = TextStyle(color = c.text, fontSize = 16.sp),
                    modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
                    decorationBox = { inner ->
                        if (trigger.isEmpty()) Text("Trigger", color = c.textDim, fontSize = 16.sp)
                        inner()
                    },
                )
                BasicTextField(
                    value = expansion,
                    onValueChange = { expansion = it },
                    singleLine = true,
                    textStyle = TextStyle(color = c.text, fontSize = 16.sp),
                    modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
                    decorationBox = { inner ->
                        if (expansion.isEmpty()) Text("Expansion", color = c.textDim, fontSize = 16.sp)
                        inner()
                    },
                )
                TextButton(onClick = {
                    val t = trigger.trim()
                    val e = expansion.trim()
                    if (t.isNotEmpty() && e.isNotEmpty()) {
                        onAdd(t, e)
                        trigger = ""
                        expansion = ""
                    }
                }) { Text("Add snippet", color = Turquoise) }
            }
        }
        Spacer(Modifier.height(16.dp))
        if (snippets.isEmpty()) {
            Text("No snippets yet.", color = c.textDim, fontSize = 14.sp)
        }
        snippets.forEach { s ->
            GlassSurface(Modifier.fillMaxWidth().padding(bottom = 8.dp)) {
                Row(
                    Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(s.trigger, color = c.text, fontSize = 16.sp)
                        Text(s.expansion, color = c.textDim, fontSize = 13.sp)
                    }
                    TextButton(onClick = { onDelete(s.trigger) }) { Text("Remove", color = c.textDim) }
                }
            }
        }
    }
}
