package com.maxspeech.android.data

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.util.concurrent.TimeUnit

private val Context.authStore by preferencesDataStore("maxspeech_auth")

data class AuthUser(
    val id: String,
    val email: String,
    val username: String,
    val planTier: String,
    val local: Boolean = false,
    val accessToken: String? = null,
)

class AuthRepository(
    private val context: Context,
    private val settings: SettingsRepository,
) {
    private val http = OkHttpClient.Builder()
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .build()

    val user: Flow<AuthUser?> = context.authStore.data.map { prefs ->
        val id = prefs[Keys.id] ?: return@map null
        AuthUser(
            id = id,
            email = prefs[Keys.email].orEmpty(),
            username = prefs[Keys.username].orEmpty(),
            planTier = prefs[Keys.tier] ?: "free",
            local = prefs[Keys.local] == "true",
            accessToken = prefs[Keys.token],
        )
    }

    suspend fun current(): AuthUser? = user.first()

    suspend fun signIn(email: String, password: String): AuthUser = withContext(Dispatchers.IO) {
        val clean = email.trim().lowercase()
        if (!clean.contains("@") || password.isBlank()) {
            throw IllegalStateException("Enter your email and password.")
        }
        val body = JSONObject()
            .put("email", clean)
            .put("password", password)
            .toString()
        val req = authRequest("auth/v1/token?grant_type=password", body)
        val json = execute(req)
        val sessionUser = json.optJSONObject("user")
            ?: throw IllegalStateException("Sign in failed — try again.")
        val token = json.optString("access_token").ifBlank { null }
            ?: throw IllegalStateException("Sign in failed — try again.")
        val profile = ensureProfile(
            sessionUser.getString("id"),
            sessionUser.optString("email", clean),
            sessionUser.optJSONObject("user_metadata")?.optString("username"),
            token,
        )
        persist(profile.copy(accessToken = token, local = false))
        settings.setLocalMode(false)
        profile.copy(accessToken = token)
    }

    suspend fun signUp(email: String, password: String, username: String): Pair<AuthUser, Boolean> =
        withContext(Dispatchers.IO) {
            val cleanEmail = email.trim().lowercase()
            val cleanUser = username.trim().ifBlank { cleanEmail.substringBefore("@") }
            if (!cleanEmail.contains("@")) throw IllegalStateException("Enter a valid email.")
            if (password.length < 6) throw IllegalStateException("Password must be at least 6 characters.")
            if (cleanUser.length < 2) throw IllegalStateException("Pick a username.")
            val body = JSONObject()
                .put("email", cleanEmail)
                .put("password", password)
                .put("data", JSONObject().put("username", cleanUser))
                .toString()
            val json = execute(authRequest("auth/v1/signup", body))
            val needsConfirm = json.isNull("access_token") || json.optString("access_token").isBlank()
            val userObj = json.optJSONObject("user") ?: JSONObject()
            val token = json.optString("access_token").ifBlank { null }
            val user = if (!needsConfirm && token != null) {
                val profile = ensureProfile(
                    userObj.getString("id"),
                    cleanEmail,
                    cleanUser,
                    token,
                )
                persist(profile.copy(accessToken = token, local = false))
                settings.setLocalMode(false)
                profile.copy(accessToken = token)
            } else {
                AuthUser(
                    id = userObj.optString("id", "pending"),
                    email = cleanEmail,
                    username = cleanUser,
                    planTier = "free",
                )
            }
            user to needsConfirm
        }

    suspend fun requestPasswordReset(email: String) = withContext(Dispatchers.IO) {
        val clean = email.trim().lowercase()
        if (!clean.contains("@")) throw IllegalStateException("Enter the email for your account.")
        val body = JSONObject().put("email", clean).toString()
        execute(authRequest("auth/v1/recover", body))
    }

    suspend fun signOut() {
        context.authStore.edit { it.clear() }
        settings.setLocalMode(false)
    }

    suspend fun refreshProfile() {
        val cur = current() ?: return
        if (cur.local || cur.accessToken.isNullOrBlank()) return
        runCatching {
            val profile = ensureProfile(cur.id, cur.email, cur.username, cur.accessToken)
            persist(profile.copy(accessToken = cur.accessToken, local = false))
        }
    }

    private suspend fun persist(user: AuthUser) {
        context.authStore.edit {
            it[Keys.id] = user.id
            it[Keys.email] = user.email
            it[Keys.username] = user.username
            it[Keys.tier] = user.planTier
            it[Keys.local] = if (user.local) "true" else "false"
            if (user.accessToken != null) it[Keys.token] = user.accessToken else it.remove(Keys.token)
        }
    }

    private fun ensureProfile(
        id: String,
        email: String,
        usernameHint: String?,
        token: String?,
    ): AuthUser {
        val username = (usernameHint?.trim()?.ifBlank { null } ?: email.substringBefore("@")).take(64)
        val get = Request.Builder()
            .url("${Secrets.SUPABASE_URL}/rest/v1/profiles?id=eq.$id&select=*")
            .header("apikey", Secrets.SUPABASE_ANON_KEY)
            .header("Authorization", "Bearer ${token ?: Secrets.SUPABASE_ANON_KEY}")
            .build()
        http.newCall(get).execute().use { resp ->
            val text = resp.body?.string().orEmpty()
            if (resp.isSuccessful && text.startsWith("[") && text.length > 2) {
                val arr = org.json.JSONArray(text)
                if (arr.length() > 0) {
                    val row = arr.getJSONObject(0)
                    return AuthUser(
                        id = row.getString("id"),
                        email = row.optString("email", email),
                        username = row.optString("username", username),
                        planTier = row.optString("plan_tier", "free"),
                        accessToken = token,
                    )
                }
            }
        }
        val upsert = JSONObject()
            .put("id", id)
            .put("email", email)
            .put("username", username)
            .put("plan_tier", "free")
            .toString()
        val put = Request.Builder()
            .url("${Secrets.SUPABASE_URL}/rest/v1/profiles")
            .header("apikey", Secrets.SUPABASE_ANON_KEY)
            .header("Authorization", "Bearer ${token ?: Secrets.SUPABASE_ANON_KEY}")
            .header("Prefer", "resolution=merge-duplicates,return=representation")
            .post(upsert.toRequestBody(JSON))
            .build()
        http.newCall(put).execute().use { }
        return AuthUser(id, email, username, "free", accessToken = token)
    }

    private fun authRequest(path: String, json: String): Request =
        Request.Builder()
            .url("${Secrets.SUPABASE_URL}/$path")
            .header("apikey", Secrets.SUPABASE_ANON_KEY)
            .header("Authorization", "Bearer ${Secrets.SUPABASE_ANON_KEY}")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .post(json.toRequestBody(JSON))
            .build()

    private fun execute(request: Request): JSONObject {
        val resp = try {
            http.newCall(request).execute()
        } catch (_: java.io.IOException) {
            throw IllegalStateException("Can't reach MaxSpeech. Check your connection.")
        }
        resp.use { r ->
            val text = r.body?.string().orEmpty()
            if (!r.isSuccessful) throw IllegalStateException(authErrorMessage(text, r.code))
            return if (text.isBlank()) JSONObject() else JSONObject(text)
        }
    }

    private fun authErrorMessage(text: String, code: Int): String {
        val json = runCatching { JSONObject(text) }.getOrNull()
            ?: return if (code == 0) "Can't reach MaxSpeech. Check your connection." else "Auth failed ($code)"
        val desc = json.optString("error_description").ifBlank { null }
        val message = json.optString("message").ifBlank { null }
        val msg = json.optString("msg").ifBlank { null }
        val err = when (val raw = json.opt("error")) {
            is String -> raw.ifBlank { null }
            is JSONObject -> raw.optString("message").ifBlank { null }
            else -> null
        }
        return when {
            json.optString("error_code") == "invalid_credentials" -> "Wrong email or password."
            !desc.isNullOrBlank() -> desc
            !message.isNullOrBlank() -> message
            !msg.isNullOrBlank() -> msg
            !err.isNullOrBlank() -> err
            else -> "Auth failed ($code)"
        }
    }

    private object Keys {
        val id = stringPreferencesKey("id")
        val email = stringPreferencesKey("email")
        val username = stringPreferencesKey("username")
        val tier = stringPreferencesKey("tier")
        val local = stringPreferencesKey("local")
        val token = stringPreferencesKey("token")
    }

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()
        const val OWNER_EMAIL = "pauldimov5@gmail.com"
    }
}
