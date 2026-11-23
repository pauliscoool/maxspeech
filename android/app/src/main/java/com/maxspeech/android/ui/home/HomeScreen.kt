package com.maxspeech.android.ui.home

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import android.app.Activity
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.Canvas
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
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
import kotlinx.coroutines.launch

@OptIn(ExperimentalLayoutApi::class, ExperimentalFoundationApi::class)
@Composable
fun HomeScreen(
    name: String,
    history: List<HistoryEntity>,
    wordsUsed: Int,
    wordsLimit: Int,
    plan: com.maxspeech.android.data.PlanStatus,
    onOpenPlan: () -> Unit,
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

    val scroll = rememberScrollState()
    val focusManager = LocalFocusManager.current
    val searchBringer = remember { BringIntoViewRequester() }
    var searchRowHeight by remember { mutableStateOf(0) }
    val imeVisible = WindowInsets.isImeVisible
    val gapPx = with(LocalDensity.current) { 14.dp.toPx() }
    // Where the page sat before search opened, so closing slides it back.
    var scrollBeforeSearch by remember { mutableStateOf(0) }
    LaunchedEffect(searchOpen) {
        if (searchOpen) {
            scrollBeforeSearch = scroll.value
            runCatching { searchFocus.requestFocus() }
        } else {
            focusManager.clearFocus()
            scroll.animateScrollTo(scrollBeforeSearch, spring(dampingRatio = 0.85f, stiffness = 380f))
        }
    }
    // Keyboard up: lift the search bar so it rests just above the keyboard with a small gap.
    LaunchedEffect(searchOpen, imeVisible) {
        if (searchOpen && imeVisible) {
            searchBringer.bringIntoView(Rect(0f, 0f, 1f, searchRowHeight + gapPx))
        }
    }

    Box(Modifier.fillMaxSize()) {
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .imePadding()
            .verticalScroll(scroll)
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

        Text("Welcome back, ${name.ifBlank { "there" }}", style = DisplayLarge, color = c.text)

        Spacer(Modifier.height(16.dp))
        HomeSlides(
            plan = plan,
            history = history,
            settings = settings,
            onOpenPlan = onOpenPlan,
            onSelectStyleGroup = onSelectStyleGroup,
            onOpenStyleEdit = onOpenStyleEdit,
        )

        Spacer(Modifier.height(22.dp))
        // Compact toolbar: search expands on the left with a jump-slide; filter on the right.
        val searchJump = remember { Animatable(0f) }
        val searchScale = remember { Animatable(1f) }
        LaunchedEffect(searchOpen) {
            searchJump.snapTo(0f)
            searchScale.snapTo(1f)
            launch {
                searchJump.animateTo(
                    -10f,
                    spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = 900f),
                )
                searchJump.animateTo(
                    0f,
                    spring(dampingRatio = 0.38f, stiffness = 480f),
                )
            }
            launch {
                searchScale.animateTo(
                    1.08f,
                    spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = 800f),
                )
                searchScale.animateTo(
                    1f,
                    spring(dampingRatio = 0.45f, stiffness = 520f),
                )
            }
        }
        Row(
            Modifier
                .fillMaxWidth()
                .bringIntoViewRequester(searchBringer)
                .onSizeChanged { searchRowHeight = it.height }
                .animateContentSize(
                    animationSpec = spring(
                        dampingRatio = Spring.DampingRatioMediumBouncy,
                        stiffness = 380f,
                    ),
                ),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SurfaceCard(
                modifier = Modifier
                    .then(if (searchOpen) Modifier.weight(1f) else Modifier)
                    .graphicsLayer {
                        translationY = searchJump.value
                        scaleX = searchScale.value
                        scaleY = searchScale.value
                    },
                shape = RoundedCornerShape(percent = 50),
            ) {
                Row(
                    Modifier.padding(2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    IconButton(
                        onClick = {
                            searchOpen = !searchOpen
                            if (!searchOpen) query = ""
                        },
                        modifier = Modifier.size(44.dp),
                    ) {
                        AnimatedContent(
                            targetState = searchOpen,
                            transitionSpec = {
                                (
                                    fadeIn(tween(160)) +
                                        expandHorizontally(
                                            animationSpec = spring(
                                                dampingRatio = Spring.DampingRatioMediumBouncy,
                                                stiffness = 500f,
                                            ),
                                            expandFrom = Alignment.Start,
                                        )
                                    ) togetherWith (
                                    fadeOut(tween(120)) +
                                        shrinkHorizontally(
                                            animationSpec = tween(160, easing = FastOutSlowInEasing),
                                            shrinkTowards = Alignment.Start,
                                        )
                                    )
                            },
                            label = "searchIcon",
                        ) { open ->
                            Icon(
                                if (open) Icons.Filled.Close else Icons.Outlined.Search,
                                contentDescription = if (open) "Close search" else "Search",
                                tint = c.text,
                            )
                        }
                    }
                    androidx.compose.animation.AnimatedVisibility(
                        visible = searchOpen,
                        modifier = Modifier.weight(1f),
                        enter = expandHorizontally(
                            animationSpec = spring(
                                dampingRatio = Spring.DampingRatioMediumBouncy,
                                stiffness = 420f,
                            ),
                            expandFrom = Alignment.Start,
                        ) + fadeIn(tween(180)),
                        exit = shrinkHorizontally(
                            animationSpec = tween(160, easing = FastOutSlowInEasing),
                            shrinkTowards = Alignment.Start,
                        ) + fadeOut(tween(120)),
                    ) {
                        BasicTextField(
                            value = query,
                            onValueChange = { query = it },
                            cursorBrush = SolidColor(Turquoise),
                            singleLine = true,
                            textStyle = TextStyle(color = c.text, fontSize = 15.sp),
                            modifier = Modifier
                                .fillMaxWidth()
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
            val filterSlide by animateFloatAsState(
                targetValue = if (searchOpen) 6f else 0f,
                animationSpec = spring(
                    dampingRatio = Spring.DampingRatioMediumBouncy,
                    stiffness = 500f,
                ),
                label = "filterSlide",
            )
            SurfaceCard(
                modifier = Modifier.graphicsLayer { translationX = filterSlide },
                shape = RoundedCornerShape(percent = 50),
            ) {
                // Same 48dp height as the search circle; "All" stays a circle, "Failed" a pill.
                Box(
                    Modifier
                        .height(48.dp)
                        .widthIn(min = 48.dp)
                        .clip(RoundedCornerShape(percent = 50))
                        .clickable { failedOnly = !failedOnly }
                        .padding(horizontal = 12.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = if (failedOnly) "Failed" else "All",
                        color = if (failedOnly) Orange else c.text,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                    )
                }
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
            .padding(bottom = 8.dp),
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
                fontSize = 15.sp,
                lineHeight = 21.sp,
                maxLines = 8,
            )
            Spacer(Modifier.height(4.dp))
            Text(
                TIME_FMT.format(Date(row.createdAt)) + " · " + row.appName.ifBlank { "MaxSpeech" },
                color = c.textDim,
                fontSize = 12.sp,
            )
            Spacer(Modifier.height(10.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (!row.failed) {
                    Text(
                        "Copy",
                        color = c.text,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier
                            .clip(RoundedCornerShape(18.dp))
                            .border(1.dp, c.hairline, RoundedCornerShape(18.dp))
                            .clickable(onClick = onCopy)
                            .padding(horizontal = 16.dp, vertical = 7.dp),
                    )
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
                        MoreToClose(open = menuOpen, color = c.text)
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
            }
        }
    }
}

/** Three dots that morph into an X while the menu is open. */
@Composable
private fun MoreToClose(open: Boolean, color: Color) {
    val p by animateFloatAsState(
        targetValue = if (open) 1f else 0f,
        animationSpec = spring(dampingRatio = 0.62f, stiffness = 600f),
        label = "moreToClose",
    )
    Canvas(Modifier.size(20.dp)) {
        val dot = 2.6.dp.toPx()
        val gap = 6.dp.toPx()
        val half = (7.dp.toPx() * p).coerceAtLeast(0.01f)
        val cx = size.width / 2
        val cy = size.height / 2
        drawCircle(color.copy(alpha = (1f - p * 1.6f).coerceIn(0f, 1f)), dot / 2, Offset(cx, cy))
        for ((dir, sign) in listOf(-1f to 1f, 1f to -1f)) {
            val y = cy + dir * gap * (1f - p)
            val a = 45f * sign * p * (Math.PI / 180f).toFloat()
            val dx = kotlin.math.cos(a) * half
            val dy = kotlin.math.sin(a) * half
            drawLine(color, Offset(cx - dx, y - dy), Offset(cx + dx, y + dy), dot, StrokeCap.Round)
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

private val TIME_FMT = SimpleDateFormat("h:mm a", Locale.getDefault())

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
