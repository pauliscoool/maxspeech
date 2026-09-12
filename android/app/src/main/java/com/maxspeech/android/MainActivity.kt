package com.maxspeech.android

import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import com.maxspeech.android.ui.AppViewModel
import com.maxspeech.android.ui.MaxSpeechRoot
import java.io.File

class MainActivity : ComponentActivity() {
    private val vm: AppViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        try {
            enableEdgeToEdge()
            WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = false
            val crashFile = File(filesDir, "last-crash.txt")
            val priorCrash = runCatching {
                crashFile.takeIf { it.exists() }?.readText()?.take(6000)
            }.getOrNull()

            setContent {
                var showCrash by remember { mutableStateOf(!priorCrash.isNullOrBlank()) }
                if (showCrash && !priorCrash.isNullOrBlank()) {
                    Column(
                        Modifier
                            .fillMaxSize()
                            .background(Color.Black)
                            .verticalScroll(rememberScrollState())
                            .padding(24.dp),
                    ) {
                        Text("Last crash — screenshot this and send it", color = Color.White, fontSize = 20.sp)
                        Text(
                            priorCrash,
                            color = Color(0xFFFFCC80),
                            fontSize = 11.sp,
                            modifier = Modifier.padding(top = 16.dp),
                        )
                        Button(
                            onClick = {
                                runCatching { crashFile.delete() }
                                showCrash = false
                            },
                            modifier = Modifier.padding(top = 20.dp),
                        ) { Text("Open MaxSpeech") }
                    }
                } else {
                    MaxSpeechRoot(vm)
                }
            }
        } catch (t: Throwable) {
            Log.e("MaxSpeech", "MainActivity.onCreate failed", t)
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
}
