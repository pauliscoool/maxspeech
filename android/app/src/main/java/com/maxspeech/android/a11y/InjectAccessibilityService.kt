package com.maxspeech.android.a11y

import android.accessibilityservice.AccessibilityService
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Rect
import android.os.Bundle
import android.provider.Settings
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
)

class InjectAccessibilityService : AccessibilityService() {
    override fun onServiceConnected() {
        instance = this
        _bound.value = true
        publishFocus()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        val pkg = event?.packageName?.toString().orEmpty()
        if (pkg.isNotBlank() && pkg != packageName) lastPackage = pkg
        when (event?.eventType) {
            AccessibilityEvent.TYPE_VIEW_FOCUSED,
            AccessibilityEvent.TYPE_VIEW_CLICKED,
            AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED,
            AccessibilityEvent.TYPE_WINDOWS_CHANGED,
            AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED,
            -> publishFocus()
        }
    }

    override fun onInterrupt() = Unit

    override fun onDestroy() {
        if (instance === this) {
            instance = null
            _bound.value = false
        }
        _focus.value = InputFocus()
        super.onDestroy()
    }

    fun insert(text: String): Boolean {
        val root = rootInActiveWindow ?: return false
        val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: findEditable(root)
        if (focused == null) return false
        lastPackage = focused.packageName?.toString() ?: lastPackage
        val args = Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text)
        }
        if (focused.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) return true
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("MaxSpeech", text))
        return focused.performAction(AccessibilityNodeInfo.ACTION_PASTE)
    }

    private fun publishFocus() {
        val imeTop = imeTopPx()
        val focused = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
        if (focused != null && focused.isEditable) {
            val bounds = Rect()
            focused.getBoundsInScreen(bounds)
            val pkg = focused.packageName?.toString() ?: lastPackage
            if (pkg != packageName) lastPackage = pkg
            _focus.value = InputFocus(
                editable = true,
                packageName = pkg,
                fieldTop = bounds.top,
                fieldBottom = bounds.bottom,
                imeTop = imeTop,
            )
            return
        }
        if (imeTop > 0) {
            _focus.value = _focus.value.copy(editable = _focus.value.editable, imeTop = imeTop)
            return
        }
        _focus.value = InputFocus()
    }

    private fun imeTopPx(): Int {
        val list = windows ?: return -1
        for (window in list) {
            if (window.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD) {
                val bounds = Rect()
                window.getBoundsInScreen(bounds)
                if (bounds.top > 0) return bounds.top
            }
        }
        return -1
    }

    private fun findEditable(node: AccessibilityNodeInfo): AccessibilityNodeInfo? {
        if (node.isEditable && node.isFocused) return node
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val found = findEditable(child)
            if (found != null) return found
        }
        return if (node.isEditable) node else null
    }

    companion object {
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
        val enabled = Settings.Secure.getString(
            context.contentResolver,
            Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
        ) ?: return false
        val needle = "${context.packageName}/${InjectAccessibilityService::class.java.name}"
        return enabled.split(':').any { entry ->
            entry.equals(needle, ignoreCase = true) ||
                entry.contains(InjectAccessibilityService::class.java.name, ignoreCase = true)
        }
    }

    fun overlayGranted(context: Context): Boolean =
        android.provider.Settings.canDrawOverlays(context)

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
