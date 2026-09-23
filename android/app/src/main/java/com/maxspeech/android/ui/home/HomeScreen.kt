package com.maxspeech.android.ui.home

import android.app.Activity
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.maxspeech.android.data.AppSettings
import com.maxspeech.android.data.HistoryEntity
import com.maxspeech.android.data.StyleGroupId
import com.maxspeech.android.ui.components.SurfaceCard
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale

@Composable
fun HomeScreen(
    name: String,
    history: List<HistoryEntity>,
    wordsUsed: Int,
    wordsLimit: Int,
    appCount: Int,
    overlayReady: Boolean,
    a11yReady: Boolean,
    settings: AppSettings,
    onOpenSettings: () -> Unit,
    onOpenStyleEdit: () -> Unit,
    onSelectStyleGroup: (StyleGroupId) -> Unit,
    onCopy: (String) -> Unit,
    onDelete: (Long) -> Unit,
    onRetranscribe: (Long) -> Unit,
    onRetryFailed: (Long) -> Unit,
) {
    val c = LocalMsColors.current
    val clip = LocalClipboardManager.current
    val ctx = LocalContext.current
    var query by remember { mutableStateOf("") }
    var searchOpen by remember { mutableStateOf(false) }
    var failedOnly by remember { mutableStateOf(false) }
    val searchFocus = remember { FocusRequester() }
    val filtered = remember(history, query, failedOnly) {
        history.filter { row ->
            (!failedOnly || row.failed) &&
                (
                    query.isBlank() ||
                        row.text.contains(query, true) ||
                        row.appName.contains(query, true) ||
                        row.errorMessage.contains(query, true)
                    )
        }
    }
    val grouped = remember(filtered) { groupByDay(filtered) }
    val failedCount = remember(history) { history.count { it.failed } }
    var pendingDelete by remember { mutableStateOf<HistoryEntity?>(null) }

    LaunchedEffect(searchOpen) {
        if (searchOpen) {
            runCatching { searchFocus.requestFocus() }
        }
    }

    Box(Modifier.fillMaxSize()) {
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
            Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = c.text)
        }
        Spacer(Modifier.height(8.dp))

        Text(
            buildAnnotatedString {
                append("Welcome back, ")
                withStyle(
                    SpanStyle(
                        brush = Brush.horizontalGradient(listOf(Turquoise, Orange)),
                        fontWeight = FontWeight.SemiBold,
                    ),
                ) {
                    append(name.ifBlank { "there" })
                }
            },
            style = DisplayLarge,
            color = c.text,
        )

        Spacer(Modifier.height(16.dp))

        HomeStyleHero(
            settings = settings,
            onSelectGroup = onSelectStyleGroup,
            onEdit = onOpenStyleEdit,
        )

        Spacer(Modifier.height(16.dp))
        SurfaceCard(Modifier.fillMaxWidth()) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .background(
                        Brush.radialGradient(
                            colors = listOf(
                                Turquoise.copy(alpha = 0.18f),
                                Orange.copy(alpha = 0.10f),
                                c.surface,
                            ),
                        ),
                    )
                    .padding(16.dp),
            ) {
                Row(verticalAlignment = Alignment.Top) {
                    Icon(
                        Icons.Outlined.Mic,
                        contentDescription = null,
                        tint = Turquoise,
                        modifier = Modifier.size(22.dp).padding(top = 2.dp),
                    )
                    Spacer(Modifier.width(12.dp))
                    Column {
                        Text("Dictate anywhere", color = c.text, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                        Text(
                            if (overlayReady && a11yReady) {
                                "Open a text field in another app — the floating mic appears automatically."
                            } else {
                                "Enable Overlay + Accessibility in Settings so the floating mic can show."
                            },
                            color = c.textDim,
                            fontSize = 13.sp,
                            modifier = Modifier.padding(top = 4.dp),
                        )
                        if (!overlayReady || !a11yReady) {
                            Text(
                                "Open Settings",
                                color = Turquoise,
                                fontSize = 13.sp,
                                modifier = Modifier
                                    .padding(top = 10.dp)
                                    .pointerInput(Unit) {
                                        detectTapGestures { onOpenSettings() }
                                    },
                            )
                        }
                    }
                }
            }
        }

        Spacer(Modifier.height(20.dp))
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            StatTile("Words", "$wordsUsed", Modifier.weight(1f))
            StatTile("Limit", "$wordsLimit", Modifier.weight(1f))
            StatTile("Apps", "$appCount", Modifier.weight(1f))
        }

        Spacer(Modifier.height(22.dp))
        // Compact toolbar: search expands on the left; failed filter on the right.
        Row(
            Modifier
                .fillMaxWidth()
                .animateContentSize(animationSpec = tween(220, easing = FastOutSlowInEasing)),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SurfaceCard(
                modifier = if (searchOpen) Modifier.weight(1f) else Modifier,
                shape = RoundedCornerShape(percent = 50),
            ) {
                Row(
                    Modifier.padding(horizontal = 4.dp, vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    IconButton(
                        onClick = {
                            searchOpen = !searchOpen
                            if (!searchOpen) query = ""
                        },
                        modifier = Modifier.size(40.dp),
                    ) {
                        Icon(
                            if (searchOpen) Icons.Filled.Close else Icons.Outlined.Search,
                            contentDescription = if (searchOpen) "Close search" else "Search",
                            tint = c.text,
                        )
                    }
                    if (searchOpen) {
                        BasicTextField(
                            value = query,
                            onValueChange = { query = it },
                            cursorBrush = SolidColor(Turquoise),
                            singleLine = true,
                            textStyle = TextStyle(color = c.text, fontSize = 15.sp),
                            modifier = Modifier
                                .weight(1f)
                                .padding(end = 12.dp)
                                .focusRequester(searchFocus),
                            decorationBox = { inner ->
                                Box {
                                    if (query.isEmpty()) {
                                        Text("Search…", color = c.textDim, fontSize = 15.sp)
                                    }
                                    inner()
                                }
                            },
                        )
                    }
                }
            }
            Spacer(Modifier.width(10.dp))
            SurfaceCard(shape = RoundedCornerShape(percent = 50)) {
                Text(
                    text = if (failedOnly) "Failed" else "All",
                    color = if (failedOnly) Orange else c.text,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier
                        .clip(RoundedCornerShape(percent = 50))
                        .clickable { failedOnly = !failedOnly }
                        .padding(horizontal = 14.dp, vertical = 12.dp),
                )
            }
        }
        if (failedCount > 0 && !failedOnly) {
            Text(
                "$failedCount failed — tap Failed to review & retry",
                color = c.textDim,
                fontSize = 12.sp,
                modifier = Modifier.padding(top = 8.dp),
            )
        }

        Spacer(Modifier.height(12.dp))
        if (filtered.isEmpty()) {
            SurfaceCard(Modifier.fillMaxWidth()) {
                Text(
                    when {
                        history.isEmpty() ->
                            "No dictations yet — tap a text field outside MaxSpeech to start."
                        failedOnly -> "No failed dictations"
                        else -> "No matches"
                    },
                    color = c.textDim,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(18.dp),
                )
            }
        } else {
            grouped.forEach { (day, rows) ->
                Text(
                    day,
                    color = c.textDim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(top = 14.dp, bottom = 8.dp),
                )
                rows.forEach { row ->
                    HistoryRow(
                        row = row,
                        onCopy = {
                            clip.setText(AnnotatedString(row.text))
                            onCopy(row.text)
                        },
                        onRequestDelete = { pendingDelete = row },
                        onRetranscribe = { onRetranscribe(row.id) },
                        onRetryFailed = { onRetryFailed(row.id) },
                    )
                }
            }
        }
    }

        pendingDelete?.let { target ->
            DeleteTranscriptDialog(
                preview = target.text,
                onRetranscribe = {
                    pendingDelete = null
                    if (target.failed) onRetryFailed(target.id) else onRetranscribe(target.id)
                },
                onCancel = { pendingDelete = null },
                onDelete = {
                    pendingDelete = null
                    onDelete(target.id)
                },
            )
        }
    }
}

@Composable
private fun DeleteTranscriptDialog(
    preview: String,
    onRetranscribe: () -> Unit,
    onCancel: () -> Unit,
    onDelete: () -> Unit,
) {
    val c = LocalMsColors.current
    var ready by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { ready = true }
    val scale by animateFloatAsState(
        targetValue = if (ready) 1f else 0.72f,
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioMediumBouncy,
            stiffness = 520f,
        ),
        label = "deleteJump",
    )
    val alpha by animateFloatAsState(
        targetValue = if (ready) 1f else 0f,
        animationSpec = tween(160),
        label = "deleteFade",
    )
    Dialog(
        onDismissRequest = onCancel,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.55f * alpha))
                .clickable(onClick = onCancel),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                Modifier
                    .padding(horizontal = 28.dp)
                    .fillMaxWidth()
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                        this.alpha = alpha
                    }
                    .clip(RoundedCornerShape(24.dp))
                    .background(c.surface)
                    .clickable(enabled = false) {}
                    .padding(20.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    "Delete this transcription?",
                    color = c.text,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.SemiBold,
                    textAlign = TextAlign.Center,
                )
                Text(
                    preview.take(120).ifBlank { "This can’t be undone." },
                    color = c.textDim,
                    fontSize = 13.sp,
                    lineHeight = 18.sp,
                    textAlign = TextAlign.Center,
                    maxLines = 3,
                    modifier = Modifier.padding(top = 10.dp, bottom = 22.dp),
                )
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    DialogAction(
                        label = "Retranscribe",
                        color = Turquoise,
                        onClick = onRetranscribe,
                        modifier = Modifier.weight(1f),
                    )
                    DialogAction(
                        label = "Cancel",
                        color = c.text,
                        fill = c.bgSoft,
                        onClick = onCancel,
                        modifier = Modifier.weight(1f),
                    )
                    DialogAction(
                        label = "Delete",
                        color = Color.White,
                        fill = c.error,
                        onClick = onDelete,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

@Composable
private fun DialogAction(
    label: String,
    color: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    fill: Color = color.copy(alpha = 0.14f),
    enabled: Boolean = true,
) {
    Box(
        modifier
            .clip(RoundedCornerShape(16.dp))
            .background(fill)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(vertical = 12.dp, horizontal = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            color = if (enabled) color else color.copy(alpha = 0.4f),
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
            textAlign = TextAlign.Center,
            maxLines = 1,
        )
    }
}

@Composable
private fun HistoryRow(
    row: HistoryEntity,
    onCopy: () -> Unit,
    onRequestDelete: () -> Unit,
    onRetranscribe: () -> Unit,
    onRetryFailed: () -> Unit,
) {
    val c = LocalMsColors.current
    var menuOpen by remember { mutableStateOf(false) }
    SurfaceCard(
        Modifier
            .fillMaxWidth()
            .padding(bottom = 8.dp)
            .pointerInput(row.id) {
                detectTapGestures { onCopy() }
            },
    ) {
        Column(Modifier.padding(14.dp)) {
            if (row.failed) {
                Text(
                    row.errorMessage.ifBlank { "Dictation failed" },
                    color = Orange,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier.padding(bottom = 6.dp),
                )
            }
            Text(
                row.text,
                color = if (row.failed) c.textDim else c.text,
                fontSize = 14.sp,
                maxLines = 4,
            )
            Spacer(Modifier.height(8.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    row.appName.ifBlank { "MaxSpeech" },
                    color = c.textDim,
                    fontSize = 11.sp,
                )
                if (row.enhanced && !row.failed) {
                    Text("AI", color = Turquoise, fontSize = 11.sp)
                }
                Spacer(Modifier.weight(1f))
                if (row.failed) {
                    Text(
                        "Retry",
                        color = Color.White,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier
                            .clip(RoundedCornerShape(14.dp))
                            .background(Orange)
                            .clickable(onClick = onRetryFailed)
                            .padding(horizontal = 12.dp, vertical = 6.dp),
                    )
                }
                Box {
                    IconButton(
                        onClick = { menuOpen = true },
                        modifier = Modifier.size(32.dp),
                    ) {
                        Icon(
                            Icons.Filled.MoreVert,
                            contentDescription = "More",
                            tint = c.text,
                            modifier = Modifier.size(18.dp),
                        )
                    }
                    DropdownMenu(
                        expanded = menuOpen,
                        onDismissRequest = { menuOpen = false },
                    ) {
                        if (!row.failed) {
                            DropdownMenuItem(
                                text = { Text("Retranscribe") },
                                onClick = {
                                    menuOpen = false
                                    onRetranscribe()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text("Copy") },
                                onClick = {
                                    menuOpen = false
                                    onCopy()
                                },
                            )
                        } else {
                            DropdownMenuItem(
                                text = { Text("Retry") },
                                onClick = {
                                    menuOpen = false
                                    onRetryFailed()
                                },
                            )
                        }
                        DropdownMenuItem(
                            text = { Text("Delete") },
                            onClick = {
                                menuOpen = false
                                onRequestDelete()
                            },
                        )
                    }
                }
                IconButton(
                    onClick = onRequestDelete,
                    modifier = Modifier.size(32.dp),
                ) {
                    Icon(
                        Icons.Outlined.Delete,
                        contentDescription = "Delete",
                        tint = c.error,
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun StatTile(label: String, value: String, modifier: Modifier = Modifier) {
    val c = LocalMsColors.current
    SurfaceCard(modifier) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            Text(label, color = c.textDim, fontSize = 11.sp)
            Text(value, color = c.text, fontSize = 16.sp, fontWeight = FontWeight.Medium)
        }
    }
}

private fun groupByDay(items: List<HistoryEntity>): List<Pair<String, List<HistoryEntity>>> {
    val fmt = SimpleDateFormat("EEEE, MMM d", Locale.getDefault())
    val cal = Calendar.getInstance()
    val todayStart = cal.apply {
        set(Calendar.HOUR_OF_DAY, 0)
        set(Calendar.MINUTE, 0)
        set(Calendar.SECOND, 0)
        set(Calendar.MILLISECOND, 0)
    }.timeInMillis
    val yesterdayStart = todayStart - 24 * 60 * 60 * 1000L
    return items
        .groupBy { row ->
            when {
                row.createdAt >= todayStart -> "Today"
                row.createdAt >= yesterdayStart -> "Yesterday"
                else -> fmt.format(Date(row.createdAt))
            }
        }
        .entries
        .sortedByDescending { (_, rows) -> rows.maxOfOrNull { it.createdAt } ?: 0L }
        .map { it.key to it.value }
}
