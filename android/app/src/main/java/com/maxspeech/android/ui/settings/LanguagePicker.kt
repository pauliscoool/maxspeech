package com.maxspeech.android.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.SttLanguages
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Turquoise

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LanguagePicker(
    selectedCodes: List<String>,
    multilingual: Boolean,
    onToggle: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = LocalMsColors.current
    var open by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val selectedLabels = SttLanguages.ALL
        .filter { it.code in selectedCodes }
        .joinToString(", ") { it.label }
        .ifBlank { "English" }
    val filtered = remember(query) {
        val q = query.trim()
        if (q.isEmpty()) SttLanguages.ALL
        else SttLanguages.ALL.filter {
            it.label.contains(q, ignoreCase = true) || it.code.contains(q, ignoreCase = true)
        }
    }

    Column(modifier) {
        Box(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .background(c.bgSoft)
                .border(1.dp, c.hairline, RoundedCornerShape(16.dp))
                .clickable {
                    query = ""
                    open = true
                }
                .padding(horizontal = 14.dp, vertical = 14.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(
                        if (multilingual) "Languages" else "Language",
                        color = c.textDim,
                        fontSize = 12.sp,
                    )
                    Text(
                        selectedLabels,
                        color = c.text,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier.padding(top = 2.dp),
                    )
                }
                Icon(
                    Icons.Filled.KeyboardArrowDown,
                    contentDescription = null,
                    tint = c.textDim,
                )
            }
        }
    }

    if (open) {
        ModalBottomSheet(
            onDismissRequest = { open = false },
            sheetState = sheetState,
            containerColor = c.surface,
            tonalElevation = 0.dp,
        ) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp)
                    .padding(bottom = 28.dp),
            ) {
                Text(
                    if (multilingual) "Pick up to ${SttLanguages.MAX} languages" else "Choose a language",
                    color = c.text,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Spacer(Modifier.height(12.dp))
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    singleLine = true,
                    placeholder = { Text("Search languages…", color = c.textDim) },
                    leadingIcon = {
                        Icon(Icons.Filled.Search, contentDescription = null, tint = c.textDim)
                    },
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(16.dp),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = Turquoise,
                        unfocusedBorderColor = c.hairline,
                        cursorColor = Turquoise,
                        focusedTextColor = c.text,
                        unfocusedTextColor = c.text,
                        focusedContainerColor = c.bgSoft,
                        unfocusedContainerColor = c.bgSoft,
                    ),
                )
                Spacer(Modifier.height(10.dp))
                LazyColumn(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(max = 420.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    items(filtered, key = { it.code }) { lang ->
                        val active = selectedCodes.contains(lang.code)
                        val locked = multilingual &&
                            !active &&
                            selectedCodes.size >= SttLanguages.MAX
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(14.dp))
                                .background(
                                    if (active) Turquoise.copy(alpha = 0.14f) else Color.Transparent,
                                )
                                .clickable(enabled = !locked) {
                                    onToggle(lang.code)
                                    if (!multilingual) open = false
                                }
                                .padding(horizontal = 14.dp, vertical = 13.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Column(Modifier.weight(1f)) {
                                Text(
                                    lang.label,
                                    color = when {
                                        locked -> c.textDim.copy(alpha = 0.55f)
                                        active -> Turquoise
                                        else -> c.text
                                    },
                                    fontSize = 15.sp,
                                    fontWeight = if (active) FontWeight.SemiBold else FontWeight.Normal,
                                )
                                Text(
                                    lang.code.uppercase(),
                                    color = c.textDim,
                                    fontSize = 11.sp,
                                )
                            }
                            if (active) {
                                Icon(
                                    Icons.Filled.Check,
                                    contentDescription = null,
                                    tint = Turquoise,
                                    modifier = Modifier.size(20.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}
