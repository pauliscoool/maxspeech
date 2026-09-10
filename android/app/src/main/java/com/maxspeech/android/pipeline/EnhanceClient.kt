package com.maxspeech.android.pipeline

import com.maxspeech.android.data.EnhanceSpeed
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

class EnhanceClient {
    private val http = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(25, TimeUnit.SECONDS)
        .build()

    suspend fun enhance(
        text: String,
        tone: String,
        apiKey: String,
        speed: EnhanceSpeed,
        multilingual: Boolean,
    ): String = withContext(Dispatchers.IO) {
        val model = if (speed == EnhanceSpeed.Ultra) "gpt-4o" else "gpt-4o-mini"
        val temp = when (speed) {
            EnhanceSpeed.Fast -> 0.0
            EnhanceSpeed.Thinking -> 0.1
            EnhanceSpeed.Ultra -> 0.22
        }
        val timeoutMs = when (speed) {
            EnhanceSpeed.Fast -> 12_000L
            EnhanceSpeed.Thinking -> 15_000L
            EnhanceSpeed.Ultra -> 20_000L
        }
        val body = JSONObject()
            .put("model", model)
            .put("temperature", temp)
            .put("max_tokens", 1024)
            .put(
                "messages",
                JSONArray()
                    .put(JSONObject().put("role", "system").put("content", systemPrompt(tone, multilingual, speed)))
                    .put(JSONObject().put("role", "user").put("content", text)),
            )
            .toString()
        val client = http.newBuilder().readTimeout(timeoutMs, TimeUnit.MILLISECONDS).build()
        val req = Request.Builder()
            .url("https://api.openai.com/v1/chat/completions")
            .header("Authorization", "Bearer $apiKey")
            .post(body.toRequestBody(JSON))
            .build()
        client.newCall(req).execute().use { resp ->
            val raw = resp.body?.string().orEmpty()
            if (!resp.isSuccessful) {
                val err = runCatching {
                    JSONObject(raw).optJSONObject("error")?.optString("message")
                }.getOrNull() ?: "Enhance failed (${resp.code})"
                throw IllegalStateException(err)
            }
            val content = JSONObject(raw)
                .getJSONArray("choices")
                .getJSONObject(0)
                .getJSONObject("message")
                .optString("content")
                .trim()
                .trim('"')
            content.ifBlank { text }
        }
    }

    private fun systemPrompt(tone: String, multilingual: Boolean, speed: EnhanceSpeed): String {
        val base = when (tone) {
            "casual" ->
                "You are a Grammarly-like dictation assistant. Rewrite in a casual, terse chat style. Prefer lowercase; skip a trailing period. Keep it brief. Still fix grammar so it reads cleanly as a message."
            "formal" ->
                "You are a Grammarly-like dictation assistant. Rewrite in a professional, formal style suitable for email: proper capitalization, punctuation, and complete sentences."
            "code" ->
                "You are a Grammarly-like dictation assistant for a programmer. Clean up grammar and use precise technical terms."
            "prose" ->
                "You are a Grammarly-like dictation assistant. Rewrite as clean prose with proper paragraphs, punctuation, and grammar."
            else ->
                "You are a Grammarly-like dictation assistant. Clean up grammar, punctuation, and clarity while keeping the original meaning and style."
        }
        val extra = when (speed) {
            EnhanceSpeed.Fast -> " Light, fast cleanup only."
            EnhanceSpeed.Ultra -> " Thorough pass: restore sentence boundaries, fix run-ons, do not invent facts."
            EnhanceSpeed.Thinking -> ""
        }
        val multi = if (multilingual) {
            " Preserve every language and script. Do not translate or transliterate."
        } else {
            ""
        }
        return "$base$extra$multi Fix spoken self-corrections (I meant X). Return ONLY the cleaned text."
    }

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()
    }
}
