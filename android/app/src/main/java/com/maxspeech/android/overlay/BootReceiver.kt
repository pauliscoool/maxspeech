package com.maxspeech.android.overlay

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.util.Log
import com.maxspeech.android.MaxSpeechApp
import kotlinx.coroutines.launch

/**
 * Restarts the lightweight floating-mic keep-alive after reboot / update.
 * Idle state is a small foreground notification only — no mic or STT held open.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) {
        val action = intent?.action ?: return
        if (action != Intent.ACTION_BOOT_COMPLETED &&
            action != Intent.ACTION_LOCKED_BOOT_COMPLETED &&
            action != Intent.ACTION_MY_PACKAGE_REPLACED &&
            action != "android.intent.action.QUICKBOOT_POWERON"
        ) {
            return
        }
        if (!Settings.canDrawOverlays(context)) {
            Log.i(TAG, "boot: no overlay permission")
            return
        }
        val pending = goAsync()
        MaxSpeechApp.instance.appScope.launch {
            try {
                val user = runCatching { MaxSpeechApp.instance.auth.current() }.getOrNull()
                if (user == null || user.local) {
                    Log.i(TAG, "boot: skipped — not signed in")
                    return@launch
                }
                Log.i(TAG, "boot: starting OverlayService")
                OverlayService.start(context.applicationContext)
            } finally {
                pending.finish()
            }
        }
    }

    companion object {
        private const val TAG = "BootReceiver"
    }
}
