package com.maxspeech.android.a11y

import android.accessibilityservice.AccessibilityService
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Toast

class InjectAccessibilityService : AccessibilityService() {
    override fun onServiceConnected() {
        instance = this
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        val pkg = event?.packageName?.toString() ?: return
        if (pkg.isNotBlank()) lastPackage = pkg
    }

    override fun onInterrupt() = Unit

    override fun onDestroy() {
        if (instance === this) instance = null
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
    }
}

object TextInjector {
    fun isAccessibilityOn(): Boolean = InjectAccessibilityService.instance != null

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
