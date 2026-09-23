package com.maxspeech.android.data

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Syncs dictionary + snippets with Supabase `user_settings.settings` using the same
 * `dictionary_json` / `macros_json` keys as desktop `src/lib/cloudSync.ts`.
 *
 * Pull is additive (never deletes local rows); push writes the full local set, but only
 * after a successful pull this session so a fresh install can't wipe the account's data.
 */
class CloudSync(
    private val db: AppDatabase,
    private val auth: AuthRepository,
) {
    private val http = OkHttpClient.Builder()
        .connectTimeout(12, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .build()

    @Volatile private var pulled = false

    @OptIn(FlowPreview::class)
    fun start(scope: CoroutineScope) {
        // Pull whenever a real (non-local) account becomes active — app start or a later sign-in.
        scope.launch(Dispatchers.IO) {
            auth.user.map { it?.takeIf { u -> !u.local }?.id }.distinctUntilChanged().collect { id ->
                pulled = false
                if (id != null) runCatching { pull() }.onFailure { Log.w(TAG, "pull failed", it) }
            }
        }
        scope.launch(Dispatchers.IO) {
            combine(db.dictionaryDao().observe(), db.snippetDao().observe()) { d, s -> d to s }
                .debounce(PUSH_DEBOUNCE_MS)
                .collect { runCatching { push() }.onFailure { Log.w(TAG, "push failed", it) } }
        }
    }

    suspend fun pull(): Boolean = withContext(Dispatchers.IO) {
        val (uid, token) = session() ?: return@withContext false
        val settings = fetchSettings(uid, token) ?: return@withContext false
        val words = parseWords(settings.optString(DICT_KEY))
        val macros = parseMacros(settings.optString(MACROS_KEY))
        val haveWords = db.dictionaryDao().all().toSet()
        for (w in words) if (w !in haveWords) db.dictionaryDao().insert(DictionaryEntity(w))
        val haveTriggers = db.snippetDao().observe().first().map { it.trigger }.toSet()
        for ((t, e) in macros) if (t !in haveTriggers) db.snippetDao().upsert(SnippetEntity(t, e))
        pulled = true
        true
    }

    suspend fun push(): Boolean = withContext(Dispatchers.IO) {
        if (!pulled && !pull()) return@withContext false
        val (uid, token) = session() ?: return@withContext false
        // Merge into the existing blob so we never drop desktop-owned keys (hotkey, theme, …).
        val settings = fetchSettings(uid, token) ?: return@withContext false
        val words = db.dictionaryDao().all()
        val macros = db.snippetDao().observe().first()
        settings.put(DICT_KEY, JSONArray(words).toString())
        settings.put(
            MACROS_KEY,
            JSONArray(macros.map { JSONObject().put("trigger", it.trigger).put("expansion", it.expansion) }).toString(),
        )
        val body = JSONObject()
            .put("user_id", uid)
            .put("settings", settings)
            .put("updated_at", java.time.Instant.now().toString())
            .toString()
        val req = Request.Builder()
            .url("${Secrets.SUPABASE_URL}/rest/v1/user_settings?on_conflict=user_id")
            .header("apikey", Secrets.SUPABASE_ANON_KEY)
            .header("Authorization", "Bearer $token")
            .header("Prefer", "resolution=merge-duplicates")
            .post(body.toRequestBody(JSON))
            .build()
        http.newCall(req).execute().use { it.isSuccessful }
    }

    private suspend fun session(): Pair<String, String>? {
        val user = auth.current() ?: return null
        if (user.local) return null
        val token = auth.freshToken() ?: return null
        return user.id to token
    }

    private fun fetchSettings(uid: String, token: String): JSONObject? {
        val req = Request.Builder()
            .url("${Secrets.SUPABASE_URL}/rest/v1/user_settings?user_id=eq.$uid&select=settings")
            .header("apikey", Secrets.SUPABASE_ANON_KEY)
            .header("Authorization", "Bearer $token")
            .build()
        http.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) return null
            val arr = runCatching { JSONArray(resp.body?.string().orEmpty()) }.getOrNull() ?: return null
            return if (arr.length() == 0) JSONObject() else arr.getJSONObject(0).optJSONObject("settings") ?: JSONObject()
        }
    }

    private fun parseWords(raw: String): List<String> = runCatching {
        val arr = JSONArray(raw)
        (0 until arr.length()).map { arr.getString(it).trim() }.filter { it.isNotEmpty() }
    }.getOrDefault(emptyList())

    private fun parseMacros(raw: String): List<Pair<String, String>> = runCatching {
        val arr = JSONArray(raw)
        (0 until arr.length()).map {
            val o = arr.getJSONObject(it)
            o.getString("trigger") to o.getString("expansion")
        }
    }.getOrDefault(emptyList())

    companion object {
        private const val TAG = "MaxSpeechSync"
        private const val DICT_KEY = "dictionary_json"
        private const val MACROS_KEY = "macros_json"
        private const val PUSH_DEBOUNCE_MS = 5_000L
        private val JSON = "application/json; charset=utf-8".toMediaType()
    }
}
