package com.maxspeech.android.ui.home

import android.app.Activity
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.GraphicEq
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.outlined.EmojiEmotions
import androidx.compose.material.icons.outlined.FilterNone
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.pipeline.DictationPhase
import com.maxspeech.android.pipeline.DictationUi
import com.maxspeech.android.ui.components.GlassChip
import com.maxspeech.android.ui.components.GlassIconButton
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.components.RibbonWaveform
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.TitleSmall

@Composable
fun HomeScreen(
    name: String,
    history: List<HistoryEntity>,
    wordsUsed: Int,
    wordsLimit: Int,
    appCount: Int,
    enhancedCount: Int,
    editedCount: Int,
    chips: List<String>,
    selectedChip: String,
    onChip: (String) -> Unit,
    ui: DictationUi,
    onHoldStart: () -> Unit,
    onHoldEnd: () -> Unit,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    onOpenHistory: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenStyle: () -> Unit,
    onCopy: (String) -> Unit,
) {
    val c = LocalMsColors.current
    val clip = LocalClipboardManager.current
    val ctx = LocalContext.current
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(top = 8.dp, bottom = 120.dp),
    ) {
        IconButton(
            onClick = { (ctx as? Activity)?.moveTaskToBack(true) },
            modifier = Modifier.size(40.dp),
        ) {
            Icon(
                Icons.AutoMirrored.Filled.ArrowBack,
                contentDescription = "Back",
                tint = c.text,
            )
        }
        Spacer(Modifier.height(10.dp))
        Text("You might want to say", style = DisplayLarge, color = c.text)
        Spacer(Modifier.height(16.dp))
        Row(
            Modifier.horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            chips.forEach { chip ->
                GlassChip(chip, selected = chip.equals(selectedChip, ignoreCase = true)) { onChip(chip) }
            }
        }
        Spacer(Modifier.height(28.dp))
        Text("Activity", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(8.dp))
        GlassSurface(Modifier.fillMaxWidth(), shape = RoundedCornerShape(26.dp)) {
            Row(
                Modifier.padding(18.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ActivityCell(
                    icon = Icons.Filled.AutoAwesome,
                    label = if (enhancedCount > 0) "AI-enhanced · $enhancedCount" else "AI-enhanced",
                    modifier = Modifier.weight(1f),
                )
                ActivityCell(
                    icon = Icons.Filled.Edit,
                    label = if (editedCount > 0) "Edited · $editedCount" else "Edited",
                    modifier = Modifier.weight(1f),
                )
                GlassIconButton(
                    onClick = onOpenHistory,
                    modifier = Modifier.size(44.dp),
                ) {
                    Icon(Icons.Filled.GraphicEq, contentDescription = "History", tint = c.textDim)
                }
            }
        }
        Spacer(Modifier.height(22.dp))
        Text("Insights", style = TitleSmall, color = c.textDim)
        Spacer(Modifier.height(8.dp))
        Box {
            GlassSurface(Modifier.fillMaxWidth().padding(bottom = 28.dp), shape = RoundedCornerShape(26.dp)) {
                Column(Modifier.padding(18.dp)) {
                    Row {
                        StatCell("Words", "$wordsUsed", Modifier.weight(1f))
                        StatCell("Limit", "$wordsLimit", Modifier.weight(1f))
                    }
                    Spacer(Modifier.height(14.dp))
                    Box(Modifier.fillMaxWidth().height(1.dp).background(c.hairline))
                    Spacer(Modifier.height(14.dp))
                    Row {
                        StatCell("Apps", "$appCount", Modifier.weight(1f), dim = true)
                        StatCell("Hi, ${name.ifBlank { "there" }}", "", Modifier.weight(1f), dim = true)
                    }
                    Spacer(Modifier.height(56.dp))
                }
            }
            Box(Modifier.align(Alignment.BottomCenter).padding(horizontal = 8.dp)) {
                when (ui.phase) {
                    DictationPhase.Listening, DictationPhase.Processing, DictationPhase.Confirm -> {
                        ConfirmRow(ui, onCancel, onConfirm, onHoldEnd)
                    }
                    else -> {
                        DictateCapsule(
                            text = ui.liveText.ifBlank { ui.error ?: "Tap to speak" },
                            onHoldStart = onHoldStart,
                        )
                    }
                }
            }
        }
        Spacer(Modifier.height(18.dp))
        GlassSurface(Modifier.fillMaxWidth(), shape = RoundedCornerShape(28.dp)) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 10.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ToolbarIcon(Icons.Filled.AutoAwesome, "Enhance", onOpenStyle)
                ToolbarIcon(Icons.Outlined.EmojiEmotions, "Snippets", onOpenStyle)
                ToolbarIcon(Icons.Filled.ContentCopy, "Copy") {
                    val t = ui.finalText.ifBlank { history.firstOrNull()?.text.orEmpty() }
                    if (t.isNotBlank()) {
                        clip.setText(AnnotatedString(t))
                        onCopy(t)
                    }
                }
                ToolbarIcon(Icons.Outlined.FilterNone, "History", onOpenHistory)
                ToolbarIcon(Icons.Filled.Settings, "Settings", onOpenSettings)
                ToolbarIcon(Icons.Filled.MoreHoriz, "More", onOpenSettings)
            }
        }
        Spacer(Modifier.height(24.dp))
        history.take(3).forEach { row ->
            GlassSurface(
                Modifier
                    .fillMaxWidth()
                    .padding(bottom = 8.dp)
                    .pointerInput(row.id) {
                        detectTapGestures {
                            clip.setText(AnnotatedString(row.text))
                            onCopy(row.text)
                        }
                    },
            ) {
                Column(Modifier.padding(14.dp)) {
                    Text(row.text, color = c.text, fontSize = 14.sp, maxLines = 3)
                    Spacer(Modifier.height(4.dp))
                    Text(row.appName, color = c.textDim, fontSize = 11.sp)
                }
            }
        }
    }
}

@Composable
private fun ActivityCell(icon: ImageVector, label: String, modifier: Modifier = Modifier) {
    val c = LocalMsColors.current
    Row(modifier, verticalAlignment = Alignment.CenterVertically) {
        Icon(icon, contentDescription = null, tint = c.text, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(8.dp))
        Text(label, color = c.text, fontSize = 15.sp, maxLines = 1)
    }
}

@Composable
private fun StatCell(label: String, value: String, modifier: Modifier = Modifier, dim: Boolean = false) {
    val c = LocalMsColors.current
    Column(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (value.isNotBlank()) {
                Text(label, color = c.text, fontSize = 16.sp)
                Spacer(Modifier.width(8.dp))
                Text(value, color = if (dim) c.textDim else c.text, fontSize = 16.sp)
            } else {
                Text(label, color = if (dim) c.textDim else c.text, fontSize = 14.sp)
            }
        }
    }
}

@Composable
private fun DictateCapsule(text: String, onHoldStart: () -> Unit) {
    val c = LocalMsColors.current
    val shape = RoundedCornerShape(28.dp)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .height(52.dp)
            .clip(shape)
            .background(c.capsule, shape)
            .pointerInput(Unit) {
                detectTapGestures(
                    onPress = {
                        onHoldStart()
                        tryAwaitRelease()
                    },
                )
            }
            .padding(horizontal = 18.dp),
    ) {
        Text(text, color = Color.White, fontSize = 16.sp, modifier = Modifier.weight(1f), maxLines = 1)
        Icon(Icons.Filled.Mic, contentDescription = "Tap to speak", tint = Color.White)
        Spacer(Modifier.width(8.dp))
        Icon(Icons.Filled.MoreHoriz, contentDescription = null, tint = Color.White.copy(alpha = 0.8f))
    }
}

@Composable
private fun ConfirmRow(
    ui: DictationUi,
    onCancel: () -> Unit,
    onConfirm: () -> Unit,
    onHoldEnd: () -> Unit,
) {
    val c = LocalMsColors.current
    Row(
        Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        GlassIconButton(onClick = onCancel, modifier = Modifier.size(48.dp)) {
            Icon(Icons.Filled.Close, null, tint = c.text)
        }
        GlassSurface(
            Modifier.weight(1f).height(48.dp),
            shape = RoundedCornerShape(24.dp),
            contentAlignment = Alignment.Center,
        ) {
            Box(Modifier.padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
                RibbonWaveform(ui.levels)
            }
        }
        Box(
            Modifier
                .size(48.dp)
                .clip(CircleShape)
                .background(Orange)
                .pointerInput(ui.phase) {
                    detectTapGestures {
                        if (ui.phase == DictationPhase.Confirm) onConfirm() else onHoldEnd()
                    }
                },
            contentAlignment = Alignment.Center,
        ) { Icon(Icons.Filled.Check, "Done", tint = Color.White) }
    }
}

@Composable
private fun ToolbarIcon(icon: ImageVector, label: String, onClick: () -> Unit) {
    val c = LocalMsColors.current
    IconButton(onClick = onClick, modifier = Modifier.size(48.dp)) {
        Icon(icon, label, tint = c.text, modifier = Modifier.size(22.dp))
    }
}
