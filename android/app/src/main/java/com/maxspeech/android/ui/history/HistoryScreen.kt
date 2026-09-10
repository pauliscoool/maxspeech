package com.maxspeech.android.ui.history

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.ui.components.GlassChip
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors

@Composable
fun HistoryScreen(
    items: List<HistoryEntity>,
    onDelete: (Long) -> Unit,
) {
    val c = LocalMsColors.current
    val clip = LocalClipboardManager.current
    var query by remember { mutableStateOf("") }
    val filtered = items.filter {
        query.isBlank() || it.text.contains(query, true) || it.appName.contains(query, true)
    }
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .padding(horizontal = 20.dp, vertical = 16.dp),
    ) {
        Text("History", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(16.dp))
        GlassSurface(Modifier.fillMaxWidth(), shape = RoundedCornerShape(22.dp)) {
            BasicTextField(
                value = query,
                onValueChange = { query = it },
                cursorBrush = SolidColor(c.turquoise),
                singleLine = true,
                textStyle = androidx.compose.ui.text.TextStyle(color = c.text, fontSize = 16.sp),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                decorationBox = { inner ->
                    if (query.isEmpty()) Text("Search dictations", color = c.textDim)
                    inner()
                },
            )
        }
        Spacer(Modifier.height(12.dp))
        LazyColumn(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            items(filtered, key = { it.id }) { row ->
                GlassSurface(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp)) {
                        Text(row.text, color = c.text, fontSize = 15.sp)
                        Spacer(Modifier.height(8.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            GlassChip(row.appName.ifBlank { "MaxSpeech" }) {
                                clip.setText(AnnotatedString(row.text))
                            }
                            if (row.enhanced) GlassChip("AI-enhanced") {}
                            Spacer(Modifier.weight(1f))
                            IconButton(onClick = { onDelete(row.id) }) {
                                Icon(Icons.Outlined.Delete, "Delete", tint = c.error)
                            }
                        }
                    }
                }
            }
        }
    }
}
