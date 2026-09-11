package com.maxspeech.android.ui.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.ui.components.MsSpinner
import com.maxspeech.android.ui.components.PageEnterSpec
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise

@Composable
fun OnboardingScreen(
    busy: Boolean,
    error: String?,
    info: String?,
    authed: Boolean,
    onSignIn: (String, String) -> Unit,
    onSignUp: (String, String, String) -> Unit,
    onMic: () -> Unit,
    onOverlay: () -> Unit,
    onA11y: () -> Unit,
    onDone: () -> Unit,
    micGranted: Boolean,
    overlayGranted: Boolean,
    a11yGranted: Boolean,
) {
    val c = LocalMsColors.current
    var step by remember { mutableIntStateOf(if (authed) 2 else 1) }
    LaunchedEffect(authed) {
        if (authed && step < 2) step = 2
        if (!authed && step >= 2) step = 1
    }
    var modeSignUp by remember { mutableStateOf(false) }
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }

    Box(Modifier.fillMaxSize().background(c.bg)) {
        Box(
            Modifier
                .align(Alignment.BottomStart)
                .offset((-48).dp, 40.dp)
                .size(260.dp)
                .background(
                    Brush.radialGradient(listOf(Orange.copy(alpha = 0.38f), Color.Transparent)),
                    CircleShape,
                ),
        )
        Box(
            Modifier
                .align(Alignment.TopEnd)
                .offset(40.dp, (-32).dp)
                .size(280.dp)
                .background(
                    Brush.radialGradient(listOf(Turquoise.copy(alpha = 0.32f), Color.Transparent)),
                    CircleShape,
                ),
        )
        Column(
            Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            AnimatedContent(
                targetState = step to modeSignUp,
                transitionSpec = { PageEnterSpec },
                label = "onboard",
            ) { (current, signUp) ->
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(
                        Icons.Filled.Mic,
                        null,
                        tint = Turquoise,
                        modifier = Modifier.padding(bottom = 12.dp).size(36.dp),
                    )
                    when (current) {
                        0 -> {
                            Text("MaxSpeech", style = DisplayLarge, color = c.text)
                            Text(
                                "AI dictation that types anywhere. Hold, speak, release.",
                                color = c.textDim,
                                fontSize = 16.sp,
                                modifier = Modifier.padding(top = 8.dp, bottom = 28.dp),
                            )
                            Button(
                                onClick = { step = 1 },
                                colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
                                modifier = Modifier.fillMaxWidth(),
                                shape = RoundedCornerShape(28.dp),
                            ) { Text("Get Started") }
                        }
                        1 -> {
                            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                Text(if (signUp) "Create account" else "Sign in", style = DisplayLarge, color = c.text)
                                Text(
                                    "Use your MaxSpeech account. Cloud sync stays with you.",
                                    color = c.textDim,
                                    fontSize = 14.sp,
                                    modifier = Modifier.padding(top = 8.dp, bottom = 20.dp),
                                )
                                if (signUp) {
                                    Field("Username", username) { username = it }
                                    Spacer(Modifier.height(8.dp))
                                }
                                Field("Email", email, KeyboardType.Email) { email = it }
                                Spacer(Modifier.height(8.dp))
                                Field("Password", password, KeyboardType.Password, password = true) { password = it }
                                AnimatedVisibility(error != null) {
                                    Text(error.orEmpty(), color = c.error, fontSize = 13.sp, modifier = Modifier.padding(top = 10.dp))
                                }
                                AnimatedVisibility(info != null) {
                                    Text(info.orEmpty(), color = c.turquoise, fontSize = 13.sp, modifier = Modifier.padding(top = 10.dp))
                                }
                                Spacer(Modifier.height(16.dp))
                                Button(
                                    onClick = {
                                        if (signUp) onSignUp(email, password, username) else onSignIn(email, password)
                                    },
                                    enabled = !busy,
                                    colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
                                    modifier = Modifier.fillMaxWidth().height(52.dp),
                                    shape = RoundedCornerShape(28.dp),
                                ) {
                                    if (busy) MsSpinner(color = Color.Black)
                                    else Text(if (signUp) "Create account" else "Sign in")
                                }
                                TextButton(onClick = { modeSignUp = !signUp }) {
                                    Text(
                                        if (signUp) "Already have one? Sign in" else "No account? Sign up",
                                        color = c.turquoise,
                                    )
                                }
                            }
                        }
                        2 -> PermStep(
                            title = "Microphone",
                            body = "Needed to hear you. Nothing is stored as audio.",
                            granted = micGranted,
                            onContinue = onMic,
                            onProceed = { step = 3 },
                        )
                        3 -> PermStep(
                            title = "Display over other apps",
                            body = "Lets the dictation capsule pop up over WhatsApp, Gmail, Messages — above the field you’re typing in.",
                            granted = overlayGranted,
                            onContinue = onOverlay,
                            onProceed = { step = 4 },
                        )
                        else -> PermStep(
                            title = "Accessibility",
                            body = "Used only to paste into the focused field and to notice when a chat box is focused. Audio never goes through Accessibility.",
                            granted = a11yGranted,
                            onContinue = onA11y,
                            onProceed = onDone,
                        )
                    }
                }
            }
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
private fun PermStep(
    title: String,
    body: String,
    granted: Boolean,
    onContinue: () -> Unit,
    onProceed: () -> Unit,
) {
    val c = LocalMsColors.current
    Text(title, style = DisplayLarge, color = c.text)
    Text(body, color = c.textDim, fontSize = 16.sp, modifier = Modifier.padding(top = 8.dp, bottom = 24.dp))
    Button(
        onClick = if (granted) onProceed else onContinue,
        colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
        modifier = Modifier.fillMaxWidth().height(52.dp),
        shape = RoundedCornerShape(28.dp),
    ) { Text(if (granted) "Proceed" else "Continue") }
    if (granted) {
        Text("Turned on — you’re good.", color = c.turquoise, modifier = Modifier.padding(top = 12.dp))
    } else {
        Text(
            "Open the system screen, enable MaxSpeech, then come back — this button becomes Proceed.",
            color = c.textDim,
            fontSize = 13.sp,
            modifier = Modifier.padding(top = 12.dp),
        )
    }
}
