package com.maxspeech.android.a11y

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Rect
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import android.widget.Toast
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

data class InputFocus(
    val editable: Boolean = false,
    val packageName: String? = null,
    val fieldTop: Int = 0,
    val fieldBottom: Int = 0,
    val imeTop: Int = -1,
    /** True when the focused field is a password / PIN entry. */
    val password: Boolean = false,
) {
    /** A focused editable field in another app (or a keyboard open over one we can't see yet). */
    val typing: Boolean
        get() = editable && !password
}

/** What the AI cleanup may read for context. Empty for password fields and money apps. */
data class ScreenContext(val fieldText: String = "", val screenText: String = "")

class InjectAccessibilityService : AccessibilityService() {
    private val focusHandler = Handler(Looper.getMainLooper())
    @Volatile private var publishPending = false
    private val publishRunnable = Runnable {
        publishPending = false
        runCatching { publishFocusNow() }
            .onFailure { Log.w(TAG, "publishFocus failed", it) }
    }
    private val pollRunnable = object : Runnable {
        override fun run() {
            runCatching { publishFocusNow() }
            if (instance === this@InjectAccessibilityService) {
                focusHandler.postDelayed(this, POLL_MS)
            }
        }
    }

    /** Last editable node reported by a focus / text event — Chrome often only exposes it this way. */
    @Volatile private var eventField: AccessibilityNodeInfo? = null
    private var lastFieldSeenAt = 0L
    private var imeUpSince = 0L
    private var lastImeTop = -1

    override fun onServiceConnected() {
        instance = this
        _bound.value = true
        // Broaden event mask at runtime — some OEMs ignore XML bits.
        serviceInfo = serviceInfo?.apply {
            eventTypes = AccessibilityEvent.TYPES_ALL_MASK
            feedbackType = AccessibilityServiceInfo.FEEDBACK_GENERIC
            flags = flags or
                AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS or
                AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or
                AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS
            notificationTimeout = 50
        }
        Log.i(TAG, "onServiceConnected pkg=$packageName")
        focusHandler.post(publishRunnable)
        focusHandler.removeCallbacks(pollRunnable)
        focusHandler.post(pollRunnable)
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        runCatching {
            event ?: return
            val pkg = event.packageName?.toString().orEmpty()
            if (pkg == packageName) return
            if (pkg.isNotBlank() && !isKeyboardOrSystem(pkg)) lastPackage = pkg
            when (event.eventType) {
                AccessibilityEvent.TYPE_VIEW_FOCUSED,
                AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED,
                AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED,
                -> {
                    event.source?.takeIf { it.isEditable && !isKeyboardOrSystem(pkg) }?.let { eventField = it }
                    schedulePublish()
                }
                AccessibilityEvent.TYPE_WINDOWS_CHANGED -> {
                    schedulePublish()
                    // Web fields are built lazily after the keyboard opens: look again shortly.
                    for (ms in RECHECK_MS) focusHandler.postDelayed({ runCatching { publishFocusNow() } }, ms)
                }
                AccessibilityEvent.TYPE_VIEW_CLICKED,
                AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED,
                AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED,
                -> schedulePublish()
            }
        }.onFailure { Log.w(TAG, "onAccessibilityEvent failed", it) }
    }

    /** Throttle, not debounce: a flood of Chrome content events must not starve the check. */
    private fun schedulePublish() {
        if (publishPending) return
        publishPending = true
        focusHandler.postDelayed(publishRunnable, THROTTLE_MS)
    }

    override fun onInterrupt() = Unit

    override fun onDestroy() {
        focusHandler.removeCallbacksAndMessages(null)
        if (instance === this) {
            instance = null
            _bound.value = false
        }
        _focus.value = InputFocus()
        Log.i(TAG, "onDestroy")
        super.onDestroy()
    }

    /**
     * Insert [text] into the focused field, exactly once.
     * Web / browser fields: clipboard paste first (SET_TEXT applies late or is ignored by rich
     * editors). Native fields: ACTION_SET_TEXT splice first, so the clipboard is untouched.
     */
    fun insert(text: String): Boolean = runCatching {
        val focused = findBestEditable() ?: return false
        lastPackage = focused.packageName?.toString() ?: lastPackage
        val web = isWebField(focused)
        Log.d(
            TAG,
            "insert: pkg=${focused.packageName} class=${focused.className} web=$web " +
                "rawTextLen=${focused.text?.length ?: -1} realTextLen=${realText(focused).length}",
        )
        if (web) {
            if (insertViaPaste(focused, text)) return true
            // SET_TEXT on web fields lands asynchronously: trust the action, never paste again.
            return insertViaSetText(focused, text, trustResult = true)
        }
        if (insertViaSetText(focused, text, trustResult = false)) return true
        insertViaPaste(focused, text)
    }.getOrDefault(false)

    /**
     * Replace the last occurrence of [old] (a previous paste) with [new] in the focused
     * field. Used by "scratch that" / "make it formal". Never touches the clipboard.
     */
    fun replaceLast(old: String, new: String): Boolean = runCatching {
        val focused = findBestEditable() ?: return false
        val existing = realText(focused)
        val needle = old.trim()
        if (needle.isEmpty()) return false
        val at = existing.lastIndexOf(needle)
        if (at < 0) return false
        var end = at + needle.length
        // Swallow the trailing space we added after the paste so undo leaves no gap.
        if (new.isEmpty() && end < existing.length && existing[end] == ' ') end++
        val next = existing.substring(0, at) + new + existing.substring(end)
        setText(focused, next)
    }.getOrDefault(false)

    /** Field text before the caret plus visible text on screen, for the AI cleanup. */
    fun screenContext(): ScreenContext = runCatching {
        val root = rootInActiveWindow ?: return ScreenContext()
        val pkg = root.packageName?.toString().orEmpty()
        if (pkg == packageName || isSensitiveApp(pkg)) return ScreenContext()
        val field = findBestEditable()
        if (field?.isPassword == true) return ScreenContext()
        val fieldText = field?.let { f ->
            val t = realText(f)
            val caret = f.textSelectionStart.takeIf { it in 0..t.length } ?: t.length
            t.substring(0, caret)
        }.orEmpty()
        val parts = ArrayList<String>()
        collectVisibleText(root, parts, intArrayOf(0))
        val screen = parts.joinToString(" | ").replace(Regex("\\s+"), " ")
        ScreenContext(fieldText.takeLast(400), screen.takeLast(1_200))
    }.getOrDefault(ScreenContext())

    private fun collectVisibleText(node: AccessibilityNodeInfo, out: MutableList<String>, budget: IntArray) {
        if (budget[0] > MAX_CONTEXT_NODES) return
        budget[0]++
        if (!node.isVisibleToUser) return
        if (!node.isEditable && !node.isPassword) {
            val t = node.text?.toString()?.trim().orEmpty()
            if (t.length >= 2 && (out.isEmpty() || out.last() != t)) out.add(t.take(300))
        }
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            collectVisibleText(child, out, budget)
        }
    }

    /**
     * Many custom composers (Instagram's DM box included) report the placeholder
     * through getText() instead of a real hint, so treat text as empty whenever
     * the node says it's only showing hint text — otherwise dictation gets
     * spliced in right after the placeholder instead of replacing it.
     */
    private fun realText(node: AccessibilityNodeInfo): String {
        val showingHint = runCatching { node.isShowingHintText }.getOrDefault(false)
        return if (showingHint) "" else node.text?.toString().orEmpty()
    }

    /** Splice [text] at the caret (or append) via ACTION_SET_TEXT. */
    private fun insertViaSetText(node: AccessibilityNodeInfo, text: String, trustResult: Boolean): Boolean {
        val existing = realText(node)
        val selStartRaw = node.textSelectionStart
        val selEndRaw = node.textSelectionEnd
        val start = if (selStartRaw < 0) existing.length else selStartRaw.coerceIn(0, existing.length)
        val end = if (selEndRaw < 0) start else selEndRaw.coerceIn(0, existing.length)
        val a = minOf(start, end)
        val b = maxOf(start, end)
        val next = existing.substring(0, a) + text + existing.substring(b)
        if (!setText(node, next)) return false
        if (trustResult) return true
        runCatching { node.refresh() }
        val after = node.text?.toString() ?: return true
        // Native fields apply SET_TEXT synchronously, so an unchanged field really failed.
        return (text.isNotBlank() && after.contains(text.trim())) || after.length > existing.length
    }

    private fun insertViaPaste(node: AccessibilityNodeInfo, text: String): Boolean {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val previous = runCatching { cm.primaryClip }.getOrNull()
        cm.setPrimaryClip(ClipData.newPlainText("MaxSpeech", text))
        val caretKnown = node.textSelectionStart >= 0
        val len = realText(node).length
        if (!caretKnown && len > 0) {
            val sel = Bundle().apply {
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, len)
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, len)
            }
            node.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, sel)
        }
        val pasted = node.performAction(AccessibilityNodeInfo.ACTION_PASTE)
        if (pasted) {
            // Restore after the target app has read the clip.
            focusHandler.postDelayed({ restoreClip(cm, previous) }, CLIP_RESTORE_MS)
        }
        return pasted
    }

    private fun setText(node: AccessibilityNodeInfo, text: String): Boolean {
        val args = Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text)
        }
        return node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)
    }

    private fun restoreClip(cm: ClipboardManager, previous: ClipData?) {
        runCatching {
            when {
                previous != null && !isOurs(previous) -> cm.setPrimaryClip(previous)
                android.os.Build.VERSION.SDK_INT >= 28 -> cm.clearPrimaryClip()
                else -> cm.setPrimaryClip(ClipData.newPlainText("", ""))
            }
        }
    }

    private fun isOurs(clip: ClipData): Boolean {
        if (clip.itemCount <= 0) return false
        return clip.description?.label?.toString().orEmpty().contains("MaxSpeech", ignoreCase = true)
    }

    private fun isWebField(node: AccessibilityNodeInfo): Boolean {
        if (node.packageName?.toString() in BROWSERS) return true
        var cur: AccessibilityNodeInfo? = node
        var depth = 0
        while (cur != null && depth < 40) {
            if (cur.className?.toString()?.contains("WebView") == true) return true
            cur = cur.parent
            depth++
        }
        return false
    }

    /** Prefer the focused editable in an application window — not the IME. */
    private fun findBestEditable(): AccessibilityNodeInfo? = findFocusedField(deep = true)?.first

    /**
     * The focused editable field and how it was found. Checked in order: the active window's
     * input focus, every app window's input focus, the last focus/text event source, then a
     * bounded search for a focused editable node (Chrome's lazily built web tree) when [deep].
     */
    private fun findFocusedField(deep: Boolean): Pair<AccessibilityNodeInfo, String>? {
        val active = rootInActiveWindow
        if (active?.packageName?.toString() == packageName) return null
        active?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.takeIf { it.isEditable }?.let { return it to "active" }
        val list = runCatching { windows }.getOrNull().orEmpty()
        val roots = list.filter {
            it.type != AccessibilityWindowInfo.TYPE_INPUT_METHOD && it.type != AccessibilityWindowInfo.TYPE_SYSTEM
        }.mapNotNull { runCatching { it.root }.getOrNull() }
            .filter { it.packageName?.toString() != packageName }
        for (root in roots) {
            root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.takeIf { it.isEditable }?.let { return it to "window" }
        }
        eventField?.let { node ->
            if (runCatching { node.refresh() }.getOrDefault(false) && node.isEditable && node.isFocused &&
                node.packageName?.toString() != packageName
            ) {
                return node to "event"
            }
        }
        if (!deep) return null
        for (root in listOfNotNull(active) + roots) {
            findFocusedEditable(root, intArrayOf(0))?.let { return it to "search" }
        }
        return null
    }

    private fun findFocusedEditable(node: AccessibilityNodeInfo, budget: IntArray): AccessibilityNodeInfo? {
        if (budget[0]++ > MAX_SEARCH_NODES) return null
        if (node.isEditable && node.isFocused) return node
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            findFocusedEditable(child, budget)?.let { return it }
        }
        return null
    }

    private fun publishFocusNow() {
        val now = SystemClock.uptimeMillis()
        val imeTop = imeTopPx()
        if (imeTop > 0) {
            if (lastImeTop <= 0) imeUpSince = now
        } else {
            imeUpSince = 0L
        }
        lastImeTop = imeTop
        val active = rootInActiveWindow
        // Never drive the float from MaxSpeech's own UI (login email/password, settings, …).
        if (active?.packageName?.toString() == packageName) {
            clearFocusIfNeeded("own-app")
            return
        }
        // The tree walk is only worth it while a keyboard is open.
        val found = findFocusedField(deep = imeTop > 0)
        val field = found?.first
        val pkg = field?.packageName?.toString()
        if (field != null && pkg != null) {
            if (field.isPassword) {
                clearFocusIfNeeded("password")
                return
            }
            val bounds = Rect()
            field.getBoundsInScreen(bounds)
            lastPackage = pkg
            lastFieldSeenAt = now
            publish(InputFocus(true, pkg, bounds.top, bounds.bottom, imeTop, false), found.second)
            return
        }
        // Keyboard is open but the field isn't in the tree yet (web pages build it late):
        // keep showing what we had, or after a moment trust the keyboard like Wispr does.
        if (imeTop > 0) {
            val current = _focus.value
            if (current.editable) {
                publish(current.copy(imeTop = imeTop), "keep")
                return
            }
            val appPkg = active?.packageName?.toString() ?: lastPackage
            if (now - imeUpSince >= IME_ONLY_AFTER_MS && appPkg != null && !isKeyboardOrSystem(appPkg)) {
                publish(InputFocus(true, appPkg, 0, 0, imeTop, false), "ime-only")
                return
            }
            focusHandler.postDelayed({ runCatching { publishFocusNow() } }, IME_ONLY_AFTER_MS)
            return
        }
        // Ride out keyboard animations and tree rebuilds instead of flickering.
        if (now - lastFieldSeenAt < HIDE_GRACE_MS && _focus.value.editable) {
            focusHandler.postDelayed({ runCatching { publishFocusNow() } }, HIDE_GRACE_MS)
            return
        }
        clearFocusIfNeeded("idle")
    }

    private fun publish(next: InputFocus, via: String) {
        if (_focus.value != next) {
            _focus.value = next
            Log.i(TAG, "focus editable via=$via pkg=${next.packageName} ime=${next.imeTop} field=${next.fieldTop}-${next.fieldBottom}")
        }
    }

    private fun clearFocusIfNeeded(reason: String) {
        if (_focus.value != InputFocus()) {
            Log.i(TAG, "focus cleared ($reason)")
            _focus.value = InputFocus()
        }
    }

    /** Top of the keyboard in screen px, or -1 when no keyboard is showing. */
    private fun imeTopPx(): Int {
        val list = runCatching { windows }.getOrNull() ?: return -1
        val screenH = resources.displayMetrics.heightPixels
        for (window in list) {
            if (window.type != AccessibilityWindowInfo.TYPE_INPUT_METHOD) continue
            val bounds = Rect()
            window.getBoundsInScreen(bounds)
            if (bounds.height() < 40) continue
            // Floating / one-handed / full-height keyboards report odd bounds: anchor low.
            if (bounds.top <= 0 || bounds.top >= screenH) return (screenH * 0.62f).toInt()
            return bounds.top
        }
        return -1
    }

    private fun isKeyboardOrSystem(pkg: String): Boolean =
        pkg == "com.android.systemui" ||
            pkg.contains("inputmethod") ||
            pkg.contains("keyboard") ||
            pkg.contains("honeyboard") ||
            pkg.contains("swiftkey")

    private fun isSensitiveApp(pkg: String): Boolean = SENSITIVE.any { pkg.contains(it, ignoreCase = true) }

    companion object {
        private const val TAG = "MaxSpeechA11y"
        private const val POLL_MS = 280L
        private const val THROTTLE_MS = 60L
        private val RECHECK_MS = longArrayOf(150L, 400L, 800L, 1_200L)
        private const val HIDE_GRACE_MS = 350L
        private const val IME_ONLY_AFTER_MS = 900L
        private const val CLIP_RESTORE_MS = 400L
        private const val MAX_SEARCH_NODES = 600
        private const val MAX_CONTEXT_NODES = 400
        private val BROWSERS = setOf(
            "com.android.chrome", "com.chrome.beta", "com.chrome.dev",
            "com.sec.android.app.sbrowser", "com.sec.android.app.sbrowser.beta",
            "org.mozilla.firefox", "com.brave.browser", "com.microsoft.emmx",
            "com.opera.browser", "com.duckduckgo.mobile.android", "com.vivaldi.browser",
        )
        private val SENSITIVE = listOf(
            "bank", "wallet", "paypal", "venmo", "squareup.cash", "chase", "wellsfargo",
            "coinbase", "robinhood", "authenticator", "1password", "bitwarden", "lastpass",
        )
        @Volatile var instance: InjectAccessibilityService? = null
        @Volatile var lastPackage: String? = null
        private val _focus = MutableStateFlow(InputFocus())
        val focus: StateFlow<InputFocus> = _focus.asStateFlow()
        private val _bound = MutableStateFlow(false)
        val bound: StateFlow<Boolean> = _bound.asStateFlow()
    }
}

object TextInjector {
    val inputFocus: StateFlow<InputFocus> = InjectAccessibilityService.focus
    val a11yBound: StateFlow<Boolean> = InjectAccessibilityService.bound

    fun isAccessibilityOn(context: Context? = null): Boolean {
        if (InjectAccessibilityService.instance != null) return true
        if (context == null) return false
        return runCatching {
            val enabled = Settings.Secure.getString(
                context.contentResolver,
                Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
            ) ?: return false
            val needle = "${context.packageName}/${InjectAccessibilityService::class.java.name}"
            enabled.split(':').any { entry ->
                entry.equals(needle, ignoreCase = true) ||
                    entry.contains(InjectAccessibilityService::class.java.name, ignoreCase = true)
            }
        }.getOrDefault(false)
    }

    fun overlayGranted(context: Context): Boolean =
        runCatching { Settings.canDrawOverlays(context) }.getOrDefault(false)

    fun micGranted(context: Context): Boolean =
        androidx.core.content.ContextCompat.checkSelfPermission(
            context,
            android.Manifest.permission.RECORD_AUDIO,
        ) == android.content.pm.PackageManager.PERMISSION_GRANTED

    fun foregroundPackage(): String? = InjectAccessibilityService.lastPackage

    fun screenContext(): ScreenContext = InjectAccessibilityService.instance?.screenContext() ?: ScreenContext()

    fun replaceLast(old: String, new: String): Boolean =
        InjectAccessibilityService.instance?.replaceLast(old, new) ?: false

    fun insert(context: Context, text: String): Boolean {
        val svc = InjectAccessibilityService.instance
        if (svc != null && svc.insert(text)) return true
        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("MaxSpeech", text))
        Toast.makeText(context, "Copied — paste into the app", Toast.LENGTH_SHORT).show()
        return false
    }
}
