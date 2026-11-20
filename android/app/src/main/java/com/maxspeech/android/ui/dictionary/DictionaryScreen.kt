package com.maxspeech.android.ui.dictionary

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
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun DictionaryScreen(
    words: List<String>,
    onAdd: (String) -> Unit,
    onDelete: (String) -> Unit,
) {
    val c = LocalMsColors.current
    var draft by remember { mutableStateOf("") }
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(top = 12.dp, bottom = 120.dp),
    ) {
        Text("Dictionary", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(8.dp))
        Text("Names and terms to spell the way you say them.", color = c.textDim, fontSize = 14.sp)
        Spacer(Modifier.height(16.dp))
        GlassSurface(Modifier.fillMaxWidth()) {
            Row(
                Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BasicTextField(
                    value = draft,
                    onValueChange = { draft = it },
                    singleLine = true,
                    textStyle = TextStyle(color = c.text, fontSize = 16.sp),
                    modifier = Modifier.weight(1f).padding(vertical = 8.dp),
                    decorationBox = { inner ->
                        if (draft.isEmpty()) Text("Add a word", color = c.textDim, fontSize = 16.sp)
                        inner()
                    },
                )
                TextButton(onClick = {
                    val w = draft.trim()
                    if (w.isNotEmpty()) {
                        onAdd(w)
                        draft = ""
                    }
                }) { Text("Add", color = Turquoise) }
            }
        }
        Spacer(Modifier.height(16.dp))
        if (words.isEmpty()) {
            Text("No words yet.", color = c.textDim, fontSize = 14.sp)
        }
        words.forEach { word ->
            GlassSurface(Modifier.fillMaxWidth().padding(bottom = 8.dp)) {
                Row(
                    Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(word, color = c.text, fontSize = 16.sp, modifier = Modifier.weight(1f))
                    TextButton(onClick = { onDelete(word) }) { Text("Remove", color = c.textDim) }
                }
            }
        }
    }
}
