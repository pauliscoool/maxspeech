package com.maxspeech.android.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun OnboardingScreen(
    busy: Boolean,
    error: String?,
    info: String?,
    authed: Boolean,
    onSignIn: (String, String) -> Unit,
    onSignUp: (String, String, String) -> Unit,
    onLocal: () -> Unit,
    onMic: () -> Unit,
    onOverlay: () -> Unit,
    onA11y: () -> Unit,
    onDone: () -> Unit,
    micGranted: Boolean,
    overlayGranted: Boolean,
    a11yGranted: Boolean,
) {
    val c = LocalMsColors.current
    var step by remember { mutableIntStateOf(if (authed) 1 else 0) }
    androidx.compose.runtime.LaunchedEffect(authed) {
        if (authed && step == 0) step = 1
    }
    var modeSignUp by remember { mutableStateOf(false) }
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }

    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(Icons.Filled.Mic, null, tint = Turquoise, modifier = Modifier.padding(bottom = 12.dp))
        when (step) {
            0 -> {
                Text("MaxSpeech", style = DisplayLarge, color = c.text)
                Text("Hold the capsule. Speak. It types into the app you’re in — cleaned up.", color = c.textDim, fontSize = 16.sp, modifier = Modifier.padding(top = 8.dp, bottom = 24.dp))
                if (modeSignUp) {
                    Field("Username", username) { username = it }
                    Spacer(Modifier.height(8.dp))
                }
                Field("Email", email, KeyboardType.Email) { email = it }
                Spacer(Modifier.height(8.dp))
                Field("Password", password, KeyboardType.Password, password = true) { password = it }
                if (error != null) Text(error, color = c.error, fontSize = 13.sp, modifier = Modifier.padding(top = 8.dp))
                if (info != null) Text(info, color = c.turquoise, fontSize = 13.sp, modifier = Modifier.padding(top = 8.dp))
                Spacer(Modifier.height(16.dp))
                Button(
                    onClick = {
                        if (modeSignUp) onSignUp(email, password, username) else onSignIn(email, password)
                    },
                    enabled = !busy,
                    colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(28.dp),
                ) { Text(if (modeSignUp) "Create account" else "Sign in") }
                Spacer(Modifier.height(10.dp))
                OutlinedButton(
                    onClick = onLocal,
                    enabled = !busy,
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(28.dp),
                ) { Text("Continue locally", color = c.text) }
                TextButton(onClick = { modeSignUp = !modeSignUp }) {
                    Text(if (modeSignUp) "Already have one? Sign in" else "No account? Sign up", color = c.turquoise)
                }
                TextButton(onClick = { step = 1 }) { Text("Skip to permissions", color = c.textDim) }
            }
            1 -> PermStep("Microphone", "MaxSpeech needs the mic to hear you.", micGranted, onMic) { step = 2 }
            2 -> PermStep("Display over other apps", "The floating capsule sits on top of Gmail, WhatsApp, Chrome…", overlayGranted, onOverlay) { step = 3 }
            3 -> PermStep("Accessibility", "This is how text is pasted into the field you were typing in. Audio never goes through Accessibility.", a11yGranted, onA11y, onDone)
        }
    }
}

@Composable
private fun Field(
    label: String,
    value: String,
    type: KeyboardType = KeyboardType.Text,
    password: Boolean = false,
    onChange: (String) -> Unit,
) {
    val c = LocalMsColors.current
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        visualTransformation = if (password) PasswordVisualTransformation() else androidx.compose.ui.text.input.VisualTransformation.None,
        keyboardOptions = KeyboardOptions(keyboardType = type),
        modifier = Modifier.fillMaxWidth(),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = Turquoise,
            focusedLabelColor = Turquoise,
            unfocusedBorderColor = c.hairline,
            cursorColor = Turquoise,
        ),
        shape = RoundedCornerShape(16.dp),
    )
}

@Composable
private fun PermStep(title: String, body: String, granted: Boolean, onGrant: () -> Unit, onNext: () -> Unit) {
    val c = LocalMsColors.current
    Text(title, style = DisplayLarge, color = c.text)
    Text(body, color = c.textDim, fontSize = 16.sp, modifier = Modifier.padding(top = 8.dp, bottom = 24.dp))
    Button(
        onClick = if (granted) onNext else onGrant,
        colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(28.dp),
    ) { Text(if (granted) "Continue" else "Allow") }
    if (granted) {
        Text("Allowed", color = c.turquoise, modifier = Modifier.padding(top = 12.dp))
    } else {
        TextButton(onClick = onNext) { Text("Later", color = c.textDim) }
    }
}
