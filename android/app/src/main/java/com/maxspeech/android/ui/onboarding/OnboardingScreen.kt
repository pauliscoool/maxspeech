@file:OptIn(ExperimentalComposeUiApi::class)

package com.maxspeech.android.ui.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.AutofillNode
import androidx.compose.ui.autofill.AutofillType
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalAutofill
import androidx.compose.ui.platform.LocalAutofillTree
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import android.view.autofill.AutofillManager
import com.maxspeech.android.R
import com.maxspeech.android.ui.components.MsSpinner
import com.maxspeech.android.ui.components.PageEnterSpec
import com.maxspeech.android.ui.style.StyleGroupsSetupPane
import com.maxspeech.android.ui.theme.DisplayLarge
import com.maxspeech.android.ui.theme.LocalMsColors
import com.maxspeech.android.ui.theme.Orange
import com.maxspeech.android.ui.theme.Turquoise
import kotlinx.coroutines.delay

private enum class AuthMode { SignIn, SignUp, Forgot }

private const val PasswordRevealSeconds = 20

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
    onStyleGroups: (String, String, String, String) -> Unit,
    onDone: () -> Unit,
    micGranted: Boolean,
    overlayGranted: Boolean,
    a11yGranted: Boolean,
    styleMessaging: String = "casual",
    styleEmail: String = "formal",
    styleWork: String = "default",
    styleSocial: String = "casual",
) {
    val c = LocalMsColors.current
    val context = LocalContext.current
    var step by remember { mutableIntStateOf(if (authed) 2 else 1) }
    var messaging by remember { mutableStateOf(styleMessaging) }
    var emailTone by remember { mutableStateOf(styleEmail) }
    var work by remember { mutableStateOf(styleWork) }
    var social by remember { mutableStateOf(styleSocial) }
    LaunchedEffect(authed) {
        if (authed && step < 2) {
            runCatching {
                context.getSystemService(AutofillManager::class.java)
                    ?.commit()
            }
            step = 2
        }
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
                    if (current != 3) {
                        Image(
                            painter = painterResource(R.drawable.ms_logo),
                            contentDescription = "MaxSpeech",
                            contentScale = ContentScale.Fit,
                            modifier = Modifier
                                .padding(bottom = 16.dp)
                                .size(72.dp)
                                .clip(RoundedCornerShape(18.dp)),
                        )
                    }
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
                            onProceed = { step = 3 },
                        )
                        else -> {
                            StyleGroupsSetupPane(
                                messaging = messaging,
                                email = emailTone,
                                work = work,
                                social = social,
                                onChange = { m, e, w, s ->
                                    messaging = m
                                    emailTone = e
                                    work = w
                                    social = s
                                },
                            )
                            Spacer(Modifier.height(20.dp))
                            Button(
                                onClick = {
                                    onStyleGroups(messaging, emailTone, work, social)
                                    onDone()
                                },
                                modifier = Modifier.fillMaxWidth().height(52.dp),
                                shape = RoundedCornerShape(28.dp),
                                colors = ButtonDefaults.buttonColors(containerColor = Turquoise),
                            ) {
                                Text("Continue", color = Color.White, fontSize = 16.sp)
                            }
                        }
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
        Field(
            label = "Username",
            value = username,
            autofillTypes = listOf(AutofillType.NewUsername, AutofillType.Username),
            imeAction = ImeAction.Next,
            onChange = onUsername,
        )
        Spacer(Modifier.height(8.dp))
    }
    Field(
        label = "Email",
        value = email,
        type = KeyboardType.Email,
        autofillTypes = listOf(AutofillType.EmailAddress, AutofillType.Username),
        imeAction = if (mode == AuthMode.Forgot) ImeAction.Done else ImeAction.Next,
        onChange = onEmail,
    )
    if (mode != AuthMode.Forgot) {
        Spacer(Modifier.height(8.dp))
        PasswordField(
            password = password,
            onPassword = onPassword,
            newPassword = mode == AuthMode.SignUp,
        )
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
private fun PasswordField(
    password: String,
    onPassword: (String) -> Unit,
    newPassword: Boolean = false,
) {
    val c = LocalMsColors.current
    var visible by remember { mutableStateOf(false) }
    var secondsLeft by remember { mutableIntStateOf(0) }

    LaunchedEffect(visible) {
        if (!visible) {
            secondsLeft = 0
            return@LaunchedEffect
        }
        secondsLeft = PasswordRevealSeconds
        while (secondsLeft > 0) {
            delay(1_000)
            secondsLeft -= 1
        }
        visible = false
    }

    Column(Modifier.fillMaxWidth()) {
        OutlinedTextField(
            value = password,
            onValueChange = onPassword,
            label = { Text("Password") },
            singleLine = true,
            visualTransformation = if (visible) {
                VisualTransformation.None
            } else {
                PasswordVisualTransformation()
            },
            keyboardOptions = KeyboardOptions(
                keyboardType = KeyboardType.Password,
                imeAction = ImeAction.Done,
                autoCorrectEnabled = false,
            ),
            trailingIcon = {
                IconButton(onClick = { visible = !visible }) {
                    AnimatedContent(
                        targetState = visible,
                        transitionSpec = {
                            (
                                fadeIn(tween(140)) +
                                    scaleIn(spring(dampingRatio = 0.65f), initialScale = 0.72f)
                                ) togetherWith (
                                fadeOut(tween(100)) + scaleOut(tween(100), targetScale = 0.72f)
                                )
                        },
                        label = "passwordEye",
                    ) { show ->
                        Icon(
                            imageVector = if (show) Icons.Filled.VisibilityOff else Icons.Filled.Visibility,
                            contentDescription = if (show) "Hide password" else "Show password",
                            tint = Color.White,
                            modifier = Modifier.size(22.dp),
                        )
                    }
                }
            },
            modifier = Modifier
                .fillMaxWidth()
                .autofill(
                    autofillTypes = listOf(
                        if (newPassword) AutofillType.NewPassword else AutofillType.Password,
                    ),
                    onFill = onPassword,
                ),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Turquoise,
                focusedLabelColor = Turquoise,
                unfocusedBorderColor = c.hairline,
                cursorColor = Turquoise,
                focusedTextColor = c.text,
                unfocusedTextColor = c.text,
            ),
            shape = RoundedCornerShape(16.dp),
        )
        AnimatedVisibility(
            visible = visible && secondsLeft > 0,
            enter = fadeIn(tween(160)),
            exit = fadeOut(tween(180)),
        ) {
            Text(
                text = if (secondsLeft == 1) {
                    "Your password will be hidden in 1 second."
                } else {
                    "Your password will be hidden in $secondsLeft seconds."
                },
                color = c.textDim,
                fontSize = 12.sp,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 6.dp, start = 4.dp),
            )
        }
    }
}

@Composable
private fun Field(
    label: String,
    value: String,
    type: KeyboardType = KeyboardType.Text,
    autofillTypes: List<AutofillType> = emptyList(),
    imeAction: ImeAction = ImeAction.Next,
    onChange: (String) -> Unit,
) {
    val c = LocalMsColors.current
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(
            keyboardType = type,
            imeAction = imeAction,
            autoCorrectEnabled = type != KeyboardType.Email && type != KeyboardType.Password,
        ),
        modifier = Modifier
            .fillMaxWidth()
            .then(
                if (autofillTypes.isNotEmpty()) {
                    Modifier.autofill(autofillTypes = autofillTypes, onFill = onChange)
                } else {
                    Modifier
                },
            ),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = Turquoise,
            focusedLabelColor = Turquoise,
            unfocusedBorderColor = c.hairline,
            cursorColor = Turquoise,
            focusedTextColor = c.text,
            unfocusedTextColor = c.text,
        ),
        shape = RoundedCornerShape(16.dp),
    )
}

/** Marks a field for Samsung Pass / Google Autofill password managers. */
@Composable
private fun Modifier.autofill(
    autofillTypes: List<AutofillType>,
    onFill: (String) -> Unit,
): Modifier {
    val autofill = LocalAutofill.current
    val autofillTree = LocalAutofillTree.current
    val node = remember(autofillTypes) {
        AutofillNode(autofillTypes = autofillTypes, onFill = onFill)
    }
    DisposableEffect(node) {
        autofillTree += node
        onDispose { autofillTree.children.remove(node.id) }
    }
    return this
        .onGloballyPositioned { node.boundingBox = it.boundsInWindow() }
        .onFocusChanged { focusState ->
            autofill?.run {
                if (focusState.isFocused) {
                    requestAutofillForNode(node)
                } else {
                    cancelAutofillForNode(node)
                }
            }
        }
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
