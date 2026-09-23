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
    /** Only a real focused editable field in another app — never IME-alone guesses. */
    val typing: Boolean
        get() = editable && !password
}

class InjectAccessibilityService : AccessibilityService() {
    private val focusHandler = Handler(Looper.getMainLooper())
    private val publishRunnable = Runnable {
        runCatching { publishFocusNow() }
            .onFailure { Log.w(TAG, "publishFocus failed", it) }
    }
    private val pollRunnable = object : Runnable {
        override fun run() {
            runCatching { publishFocusNow() }
            if (instance === this@InjectAccessibilityService) {
                focusHandler.postDelayed(this, 280)
            }
        }
    }

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
            val pkg = event?.packageName?.toString().orEmpty()
            if (pkg == packageName) return
            if (pkg.isNotBlank()) lastPackage = pkg
            when (event?.eventType) {
                AccessibilityEvent.TYPE_VIEW_FOCUSED,
                AccessibilityEvent.TYPE_VIEW_CLICKED,
                AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED,
                AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED,
                AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED,
                AccessibilityEvent.TYPE_WINDOWS_CHANGED,
                AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED,
                -> {
                    focusHandler.removeCallbacks(publishRunnable)
                    // Hide fast when focus leaves a field; slight delay only to coalesce bursts.
                    focusHandler.postDelayed(publishRunnable, 16)
                }
            }
        }.onFailure { Log.w(TAG, "onAccessibilityEvent failed", it) }
    }

    override fun onInterrupt() = Unit

    override fun onDestroy() {
        focusHandler.removeCallbacks(publishRunnable)
        focusHandler.removeCallbacks(pollRunnable)
        if (instance === this) {
            instance = null
            _bound.value = false
        }
        _focus.value = InputFocus()
        Log.i(TAG, "onDestroy")
        super.onDestroy()
    }

    /**
     * Insert [text] into the focused field.
     * Prefer ACTION_SET_TEXT so we never touch the clipboard on success
     * (Android 13+ shows an annoying system toast on every clip write).
     * Clipboard is used only as a last resort, and cleared again after a
     * successful paste so dictation does not linger in the pasteboard.
     */
    fun insert(text: String): Boolean = runCatching {
        val focused = findBestEditable() ?: return false
        lastPackage = focused.packageName?.toString() ?: lastPackage
        focused.performAction(AccessibilityNodeInfo.ACTION_FOCUS)

        // 1) Direct set/splice — no clipboard.
        if (insertViaSetText(focused, text)) return true

        // 2) Last resort: clipboard paste, then always try to restore prior clip.
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val previous = runCatching { cm.primaryClip }.getOrNull()
        cm.setPrimaryClip(ClipData.newPlainText("MaxSpeech", text))
        val before = focused.text?.toString().orEmpty()
        val pasted = pasteInto(focused)
        val verified = likelyInserted(focused, text, before)
        if (pasted || verified) {
            restoreClip(cm, previous)
            // Some OEMs re-assert the clip after PASTE — clear again shortly.
            focusHandler.postDelayed({ restoreClip(cm, previous) }, 120)
            return true
        }

        // Still failed — leave text on clipboard for manual paste.
        false
    }.getOrDefault(false)

    /** Splice [text] at the caret (or append) via ACTION_SET_TEXT. */
    private fun insertViaSetText(node: AccessibilityNodeInfo, text: String): Boolean {
        val existing = node.text?.toString().orEmpty()
        val selStartRaw = node.textSelectionStart
        val selEndRaw = node.textSelectionEnd
        val start = when {
            selStartRaw < 0 -> existing.length
            else -> selStartRaw.coerceIn(0, existing.length)
        }
        val end = when {
            selEndRaw < 0 -> start
            else -> selEndRaw.coerceIn(0, existing.length)
        }
        val a = minOf(start, end)
        val b = maxOf(start, end)
        val next = existing.substring(0, a) + text + existing.substring(b)
        val applied = setText(node, next) || run {
            val fallback = if (existing.isEmpty()) text else existing + text
            setText(node, fallback)
        }
        if (!applied) return false
        runCatching { node.refresh() }
        val after = node.text?.toString()
        // Opaque fields (many WebViews): trust the action result — do not touch clipboard.
        if (after == null) return true
        if (text.isNotBlank() && after.contains(text)) return true
        if (after.length > existing.length) return true
        // SET_TEXT returned true but content unchanged — treat as failure.
        return false
    }

    private fun setText(node: AccessibilityNodeInfo, text: String): Boolean {
        val args = Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text)
        }
        return node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)
    }

    private fun pasteInto(node: AccessibilityNodeInfo): Boolean {
        val len = node.text?.length ?: 0
        if (len > 0) {
            val sel = Bundle().apply {
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, len)
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, len)
            }
            node.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, sel)
        }
        return node.performAction(AccessibilityNodeInfo.ACTION_PASTE)
    }

    private fun likelyInserted(
        node: AccessibilityNodeInfo,
        inserted: String,
        before: String,
    ): Boolean {
        runCatching { node.refresh() }
        val after = node.text?.toString() ?: return false
        if (inserted.isNotBlank() && after.contains(inserted)) return true
        if (before.isNotEmpty() && after.length > before.length) return true
        return false
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
        val label = clip.description?.label?.toString().orEmpty()
        if (label.equals("MaxSpeech", ignoreCase = true)) return true
        val text = runCatching { clip.getItemAt(0).coerceToText(this)?.toString() }.getOrNull()
        return text != null && label.contains("MaxSpeech", ignoreCase = true)
    }

    /** Prefer the focused editable in an application window — not the IME. */
    private fun findBestEditable(): AccessibilityNodeInfo? {
        val list = runCatching { windows }.getOrNull().orEmpty()
        for (window in list) {
            if (window.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD) continue
            if (window.type == AccessibilityWindowInfo.TYPE_SYSTEM) continue
            val root = runCatching { window.root }.getOrNull() ?: continue
            val pkg = root.packageName?.toString()
            if (pkg != null && pkg == packageName) continue
            val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            if (focused != null && focused.isEditable) return focused
        }
        val root = rootInActiveWindow ?: return null
        if (root.packageName?.toString() == packageName) return null
        return root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.takeIf { it.isEditable }
            ?: findEditable(root)
    }

    private fun publishFocusNow() {
        val imeTop = imeTopPx()
        val root = rootInActiveWindow
        val rootPkg = root?.packageName?.toString()
        // Never drive the float from MaxSpeech's own UI (login email/password, settings, …).
        if (rootPkg != null && rootPkg == packageName) {
            clearFocusIfNeeded("own-app")
            return
        }
        // Strict: only the currently focused input. Never "any visible EditText"
        // (Messages / WhatsApp keep a composer in the tree while you just read).
        val focused = root?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
        val pkg = focused?.packageName?.toString()
        if (pkg != null && pkg == packageName) {
            clearFocusIfNeeded("own-field")
            return
        }
        if (focused != null && focused.isEditable && focused.isFocused && pkg != null) {
            if (focused.isPassword) {
                clearFocusIfNeeded("password")
                return
            }
            val bounds = Rect()
            focused.getBoundsInScreen(bounds)
            lastPackage = pkg
            val next = InputFocus(
                editable = true,
                packageName = pkg,
                fieldTop = bounds.top,
                fieldBottom = bounds.bottom,
                imeTop = imeTop,
                password = false,
            )
            if (_focus.value != next) {
                _focus.value = next
                Log.i(TAG, "focus editable pkg=$pkg ime=$imeTop field=${bounds.top}-${bounds.bottom}")
            }
            return
        }
        clearFocusIfNeeded("idle")
    }

    private fun clearFocusIfNeeded(reason: String) {
        if (_focus.value.typing || _focus.value.editable || _focus.value.imeTop > 0) {
            Log.i(TAG, "focus cleared ($reason)")
        }
        if (_focus.value != InputFocus()) {
            _focus.value = InputFocus()
        }
    }

    private fun imeTopPx(): Int {
        val list = runCatching { windows }.getOrNull() ?: return -1
        for (window in list) {
            if (window.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD) {
                val bounds = Rect()
                window.getBoundsInScreen(bounds)
                if (bounds.height() > 80 && bounds.top > 0) return bounds.top
            }
        }
        return -1
    }

    private fun findEditable(node: AccessibilityNodeInfo): AccessibilityNodeInfo? {
        // Prefer a focused editable — used only as paste fallback, never to show the mic.
        if (node.isEditable && node.isFocused) return node
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val found = findEditable(child)
            if (found != null) return found
        }
        return null
    }

    companion object {
        private const val TAG = "MaxSpeechA11y"
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

    fun notificationGranted(context: Context): Boolean {
        if (android.os.Build.VERSION.SDK_INT < 33) return true
        return androidx.core.content.ContextCompat.checkSelfPermission(
            context,
            android.Manifest.permission.POST_NOTIFICATIONS,
        ) == android.content.pm.PackageManager.PERMISSION_GRANTED
    }

    fun foregroundPackage(): String? = InjectAccessibilityService.lastPackage

    fun insert(context: Context, text: String): Boolean {
        val svc = InjectAccessibilityService.instance
        if (svc != null && svc.insert(text)) return true
        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("MaxSpeech", text))
        Toast.makeText(context, "Copied — paste into the app", Toast.LENGTH_SHORT).show()
        return false
    }
}
