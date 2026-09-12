package com.maxspeech.android.ui.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
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
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.maxspeech.android.R
import com.maxspeech.android.ui.components.MsSpinner
import com.maxspeech.android.ui.components.PageEnterSpec
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise

private enum class AuthMode { SignIn, SignUp, Forgot }

@Composable
fun OnboardingScreen(
    busy: Boolean,
    error: String?,
    info: String?,
    authed: Boolean,
    onSignIn: (String, String) -> Unit,
    onSignUp: (String, String, String) -> Unit,
    onForgot: (String) -> Unit,
    onClearFlash: () -> Unit,
    onMic: () -> Unit,
    onOverlay: () -> Unit,
    onAppInfo: () -> Unit,
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
    var mode by remember { mutableStateOf(AuthMode.SignIn) }
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var username by remember { mutableStateOf("") }

    LaunchedEffect(info) {
        if (info != null && step == 1 && mode != AuthMode.SignIn) mode = AuthMode.SignIn
    }

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
                targetState = step to mode,
                transitionSpec = { PageEnterSpec },
                label = "onboard",
            ) { (current, authMode) ->
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Image(
                        painter = painterResource(R.drawable.ms_logo),
                        contentDescription = "MaxSpeech",
                        contentScale = ContentScale.Fit,
                        modifier = Modifier
                            .padding(bottom = 16.dp)
                            .size(72.dp)
                            .clip(RoundedCornerShape(18.dp)),
                    )
                    when (current) {
                        1 -> AuthStep(
                            mode = authMode,
                            busy = busy,
                            error = error,
                            info = info,
                            email = email,
                            password = password,
                            username = username,
                            onEmail = { email = it },
                            onPassword = { password = it },
                            onUsername = { username = it },
                            onSignIn = { onSignIn(email, password) },
                            onSignUp = { onSignUp(email, password, username) },
                            onForgot = { onForgot(email) },
                            onMode = {
                                onClearFlash()
                                mode = it
                            },
                        )
                        2 -> PermStep(
                            title = "Microphone",
                            body = "The only permission MaxSpeech needs to dictate inside the app. Audio is never stored.",
                            granted = micGranted,
                            onContinue = onMic,
                            onProceed = onDone,
                        )
                        else -> PermStep(
                            title = "Microphone",
                            body = "The only permission MaxSpeech needs to dictate inside the app. Audio is never stored.",
                            granted = micGranted,
                            onContinue = onMic,
                            onProceed = onDone,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun AuthStep(
    mode: AuthMode,
    busy: Boolean,
    error: String?,
    info: String?,
    email: String,
    password: String,
    username: String,
    onEmail: (String) -> Unit,
    onPassword: (String) -> Unit,
    onUsername: (String) -> Unit,
    onSignIn: () -> Unit,
    onSignUp: () -> Unit,
    onForgot: () -> Unit,
    onMode: (AuthMode) -> Unit,
) {
    val c = LocalMsColors.current
    val title = when (mode) {
        AuthMode.SignIn -> "Sign in"
        AuthMode.SignUp -> "Create account"
        AuthMode.Forgot -> "Reset password"
    }
    val subtitle = when (mode) {
        AuthMode.SignIn -> "Use your MaxSpeech account. Cloud sync stays with you."
        AuthMode.SignUp -> "Same account as desktop. Username, email, password."
        AuthMode.Forgot -> "We’ll email a reset link to the address on your account."
    }
    Text(title, style = DisplayLarge, color = c.text)
    Text(
        subtitle,
        color = c.textDim,
        fontSize = 14.sp,
        modifier = Modifier.padding(top = 8.dp, bottom = 20.dp),
    )
    if (mode == AuthMode.SignUp) {
        Field("Username", username) { onUsername(it) }
        Spacer(Modifier.height(8.dp))
    }
    Field("Email", email, KeyboardType.Email) { onEmail(it) }
    if (mode != AuthMode.Forgot) {
        Spacer(Modifier.height(8.dp))
        Field("Password", password, KeyboardType.Password, password = true) { onPassword(it) }
    }
    AnimatedVisibility(error != null) {
        Text(error.orEmpty(), color = c.error, fontSize = 13.sp, modifier = Modifier.padding(top = 10.dp))
    }
    AnimatedVisibility(info != null) {
        Text(info.orEmpty(), color = c.turquoise, fontSize = 13.sp, modifier = Modifier.padding(top = 10.dp))
    }
    Spacer(Modifier.height(16.dp))
    Button(
        onClick = {
            when (mode) {
                AuthMode.SignIn -> onSignIn()
                AuthMode.SignUp -> onSignUp()
                AuthMode.Forgot -> onForgot()
            }
        },
        enabled = !busy,
        colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
        modifier = Modifier.fillMaxWidth().height(52.dp),
        shape = RoundedCornerShape(28.dp),
    ) {
        if (busy) MsSpinner(color = Color.Black)
        else Text(
            when (mode) {
                AuthMode.SignIn -> "Sign in"
                AuthMode.SignUp -> "Create account"
                AuthMode.Forgot -> "Send reset link"
            },
        )
    }
    when (mode) {
        AuthMode.SignIn -> {
            TextButton(
                onClick = { onMode(AuthMode.Forgot) },
                modifier = Modifier.padding(top = 4.dp),
            ) {
                Text("Forgot password?", color = c.turquoise, fontSize = 15.sp)
            }
            OutlinedButton(
                onClick = { onMode(AuthMode.SignUp) },
                enabled = !busy,
                modifier = Modifier.fillMaxWidth().height(52.dp),
                shape = RoundedCornerShape(28.dp),
                colors = ButtonDefaults.outlinedButtonColors(contentColor = Turquoise),
                border = BorderStroke(1.dp, Turquoise),
            ) {
                Text("Sign up")
            }
        }
        AuthMode.SignUp -> {
            TextButton(onClick = { onMode(AuthMode.SignIn) }) {
                Text("Already have one? Sign in", color = c.turquoise, fontSize = 15.sp)
            }
        }
        AuthMode.Forgot -> {
            TextButton(onClick = { onMode(AuthMode.SignIn) }) {
                Text("Back to sign in", color = c.turquoise, fontSize = 15.sp)
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

@Composable
private fun A11yPermStep(
    granted: Boolean,
    onOpenAppInfo: () -> Unit,
    onOpenA11y: () -> Unit,
    onProceed: () -> Unit,
) {
    val c = LocalMsColors.current
    Text("Accessibility", style = DisplayLarge, color = c.text)
    Text(
        "Sideloaded apps hide this until you unlock it. Used only to paste into the field you’re typing in — audio never goes through Accessibility.",
        color = c.textDim,
        fontSize = 15.sp,
        modifier = Modifier.padding(top = 8.dp, bottom = 16.dp),
    )
    Text(
        "1. Open Settings, search Apps, then open MaxSpeech.\n" +
            "2. Top-right ⋮ three dots → Allow restricted settings. Don’t tap Clear cache.\n" +
            "3. Come back here, open Accessibility, find MaxSpeech, turn it on.\n" +
            "4. Return to this screen — Continue becomes Proceed.\n\n" +
            "Turn off Wispr Flow / Whisper Flow Accessibility while using MaxSpeech, or the two overlays fight and the app can close.",
        color = c.textDim,
        fontSize = 14.sp,
        modifier = Modifier.padding(bottom = 20.dp).fillMaxWidth(),
    )
    if (granted) {
        Button(
            onClick = onProceed,
            colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
            modifier = Modifier.fillMaxWidth().height(52.dp),
            shape = RoundedCornerShape(28.dp),
        ) { Text("Proceed") }
        Text("Turned on — you’re good.", color = c.turquoise, modifier = Modifier.padding(top = 12.dp))
    } else {
        Button(
            onClick = onOpenAppInfo,
            colors = ButtonDefaults.buttonColors(containerColor = Turquoise, contentColor = Color.Black),
            modifier = Modifier.fillMaxWidth().height(52.dp),
            shape = RoundedCornerShape(28.dp),
        ) { Text("1. Open app info") }
        Spacer(Modifier.height(10.dp))
        OutlinedButton(
            onClick = onOpenA11y,
            modifier = Modifier.fillMaxWidth().height(52.dp),
            shape = RoundedCornerShape(28.dp),
            colors = ButtonDefaults.outlinedButtonColors(contentColor = Turquoise),
            border = BorderStroke(1.dp, Turquoise),
        ) { Text("2. Open Accessibility") }
    }
}
