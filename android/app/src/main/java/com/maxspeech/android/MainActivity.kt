package com.maxspeech.android

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.core.view.WindowCompat
import com.maxspeech.android.overlay.OverlayService
import com.maxspeech.android.ui.AppViewModel
import com.maxspeech.android.ui.MaxSpeechRoot
import java.io.File

class MainActivity : ComponentActivity() {
    private val vm: AppViewModel by viewModels()
    private var uiReady = false

    private val notifPermission = registerForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { if (uiReady) startFloatingMic() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        try {
            enableEdgeToEdge()
            WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = false
            val crashFile = crashFile()
            val priorCrash = runCatching {
                crashFile.takeIf { it.exists() }?.readText()?.take(6000)
            }.getOrNull()

            // Stale crash from the fixed overlay lifecycle bug used to block every launch.
            if (isStaleFixedCrash(priorCrash)) {
                Log.i(TAG, "Clearing stale fixed crash report")
                runCatching { crashFile.delete() }
            }
            val crashToShow = priorCrash?.takeIf { !isStaleFixedCrash(it) && crashFile.exists() }

            setContent {
                var showCrash by remember { mutableStateOf(!crashToShow.isNullOrBlank()) }
                if (showCrash && !crashToShow.isNullOrBlank()) {
                    Column(
                        Modifier
                            .fillMaxSize()
                            .background(Color.Black)
                            .verticalScroll(rememberScrollState())
                            .padding(24.dp),
                    ) {
                        Text("Last crash — screenshot this and send it", color = Color.White, fontSize = 20.sp)
                        Text(
                            crashToShow,
                            color = Color(0xFFFFCC80),
                            fontSize = 11.sp,
                            modifier = Modifier.padding(top = 16.dp),
                        )
                        Button(
                            onClick = {
                                clearCrash()
                                showCrash = false
                                uiReady = true
                                startFloatingMic()
                            },
                            modifier = Modifier.padding(top = 20.dp),
                        ) { Text("Open MaxSpeech") }
                    }
                } else {
                    LaunchedEffect(Unit) {
                        uiReady = true
                        clearCrash()
                        startFloatingMic()
                    }
                    MaxSpeechRoot(vm)
                }
            }
        } catch (t: Throwable) {
            Log.e(TAG, "MainActivity.onCreate failed", t)
            setContent {
                Column(
                    Modifier
                        .fillMaxSize()
                        .background(Color.Black)
                        .verticalScroll(rememberScrollState())
                        .padding(24.dp),
                ) {
                    Text("MaxSpeech failed to start — screenshot this", color = Color.White, fontSize = 20.sp)
                    Text(t.stackTraceToString(), color = Color(0xFFFF8A80), fontSize = 11.sp, modifier = Modifier.padding(top = 16.dp))
                }
            }
        }
    }

    override fun onStart() {
        super.onStart()
        if (uiReady) startFloatingMic()
    }

    override fun onResume() {
        super.onResume()
        if (uiReady) startFloatingMic()
        if (Build.VERSION.SDK_INT >= 33) {
            val granted = ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
            if (!granted) {
                notifPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
    }

    private fun startFloatingMic() {
        if (!Settings.canDrawOverlays(this)) {
            Log.w(TAG, "Floating mic: overlay permission missing")
            return
        }
        Log.i(TAG, "Floating mic: showing + starting keep-alive service")
        runCatching {
            MaxSpeechApp.instance.floatingMic.ensureShown(this)
            OverlayService.start(this)
            clearCrash()
        }.onFailure { Log.e(TAG, "Floating mic start failed", it) }
    }

    private fun crashFile(): File = File(filesDir, "last-crash.txt")

    private fun clearCrash() {
        runCatching { crashFile().delete() }
    }

    private fun isStaleFixedCrash(text: String?): Boolean {
        if (text.isNullOrBlank()) return false
        return text.contains("ViewTreeLifecycleOwner not found")
    }

    companion object {
        private const val TAG = "MaxSpeech"
    }
}
