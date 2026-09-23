package com.maxspeech.android.ui.about

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.BuildConfig
import com.maxspeech.android.ui.components.GlassSurface
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.TitleSmall

@Composable
fun AboutScreen() {
    val c = LocalMsColors.current
    val ctx = LocalContext.current
    fun open(url: String) {
        ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
    }
    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 20.dp)
            .padding(top = 12.dp, bottom = 120.dp),
    ) {
        Text("About", style = DisplayLarge, color = c.text)
        Text(
            "MaxSpeech for Android — same account as desktop.",
            color = c.textDim,
            fontSize = 14.sp,
            modifier = Modifier.padding(top = 6.dp, bottom = 18.dp),
        )

        Text("About & updates", style = TitleSmall, color = c.textDim, modifier = Modifier.padding(bottom = 8.dp))
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(8.dp)) {
                AboutRow("Version", BuildConfig.VERSION_NAME)
                AboutRow("Website", "maxspeech.vercel.app") {
                    open("https://maxspeech.vercel.app")
                }
                AboutRow("Android install", "Sideload guide") {
                    open("https://maxspeech.vercel.app/android")
                }
                AboutRow("Desktop app", "Windows download") {
                    open("https://maxspeech.vercel.app")
                }
                AboutRow("Terms", "Terms of Service") {
                    open("https://maxspeech.vercel.app/terms.html")
                }
                AboutRow("Privacy", "Privacy Policy") {
                    open("https://maxspeech.vercel.app/privacy.html")
                }
                AboutRow("GitHub", "pauliscoool/maxspeech") {
                    open("https://github.com/pauliscoool/maxspeech")
                }
            }
        }

        Spacer(Modifier.height(22.dp))
        Text("Notes", style = TitleSmall, color = c.textDim, modifier = Modifier.padding(bottom = 8.dp))
        GlassSurface(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text(
                    "Speech and AI run on Maximus Dev company API keys for our speech models. " +
                        "We do not use your content for training. " +
                        "Cloud sync stays with your MaxSpeech account across desktop and Android.",
                    color = c.textDim,
                    fontSize = 13.sp,
                    lineHeight = 18.sp,
                )
            }
        }
    }
}

@Composable
private fun AboutRow(title: String, subtitle: String, onClick: (() -> Unit)? = null) {
    val c = LocalMsColors.current
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, color = c.text, fontSize = 15.sp)
            Text(subtitle, color = c.textDim, fontSize = 12.sp)
        }
        if (onClick != null) Text("Open", color = c.turquoise, fontSize = 13.sp)
    }
}
