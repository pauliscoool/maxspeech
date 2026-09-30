package com.maxspeech.android.pipeline

import android.util.Log
import com.maxspeech.android.data.EnhanceSpeed
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import org.json.JSONArray
import org.json.JSONObject
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

class EnhanceClient(
    private val faithfulPrompt: String,
    private val http: OkHttpClient = OkHttpClient(),
    private val endpoint: String = "https://api.openai.com/v1/chat/completions",
) {
    suspend fun enhance(
        text: String,
        tone: String,
        apiKey: String,
        speed: EnhanceSpeed,
        multilingual: Boolean,
        dictionary: List<String>,
        deadlineMs: Long,
    ): String {
        if (apiKey.isBlank() || deadlineMs <= 0) return text
        val started = System.nanoTime()
        return try {
            val body = JSONObject()
                .put("model", "gpt-4o-mini")
                .put("temperature", 0.0)
                .put("max_tokens", (text.toByteArray(Charsets.UTF_8).size + 128).coerceIn(256, 16_384))
                .put("messages", JSONArray()
                    .put(JSONObject().put("role", "system").put("content", systemPrompt(tone, multilingual, dictionary)))
                    .put(JSONObject().put("role", "user").put("content", text)))
            val request = Request.Builder().url(endpoint)
                .header("Authorization", "Bearer $apiKey")
                .post(body.toString().toRequestBody(JSON)).build()
            val call = http.newCall(request)
            call.timeout().timeout(minOf(timeoutMs(speed), deadlineMs), TimeUnit.MILLISECONDS)
            val candidate = awaitContent(call)
            val decision = FaithfulDictation.guardOutput(text, candidate)
            Log.i("MaxSpeech", "Dictation guard reason=${decision.reason} drift=${decision.drift} over_limit=${decision.drift > 0.20} elapsed_ms=${(System.nanoTime() - started) / 1_000_000}")
            decision.text
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            Log.w("MaxSpeech", "Dictation enhance fallback reason=request_error elapsed_ms=${(System.nanoTime() - started) / 1_000_000}")
            text
        }
    }

    private suspend fun awaitContent(call: Call): String = suspendCancellableCoroutine { continuation ->
        // Cancelling the coroutine must cancel the socket, not merely stop waiting for it.
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                if (continuation.isActive) continuation.resumeWithException(e)
            }

            override fun onResponse(call: Call, response: Response) {
                response.use {
                    try {
                        if (!response.isSuccessful) throw IOException("Enhance HTTP failure")
                        val choice = JSONObject(response.body?.string().orEmpty()).getJSONArray("choices").getJSONObject(0)
                        if (choice.optString("finish_reason") != "stop") throw IOException("Incomplete enhance response")
                        val content = choice.getJSONObject("message").getString("content").trim()
                        if (content.isBlank()) throw IOException("Empty enhance response")
                        if (continuation.isActive) continuation.resume(content)
                    } catch (e: Exception) {
                        if (continuation.isActive) continuation.resumeWithException(e)
                    }
                }
            }
        })
    }

    internal fun systemPrompt(tone: String, multilingual: Boolean, dictionary: List<String>): String {
        val surface = when (tone) {
            "casual" -> "Casual surface formatting only; preserve proper names and use lighter terminal punctuation."
            "prose" -> "Keep paragraph breaks; add paragraphs only for explicit spoken commands."
            "code" -> "Preserve technical words and symbols; never invent code or comment syntax."
            else -> "Use normal sentence capitalization and punctuation; preserve incomplete sentences."
        }
        val language = if (multilingual) "Preserve all languages and scripts. Do not translate or transliterate." else "Preserve the spoken language."
        val terms = dictionary.map { it.trim() }.filter { it.isNotEmpty() }.take(60)
        val block = if (terms.isEmpty()) "" else "\n\nPreferred vocabulary (spell and capitalize exactly when the user says these; restore Name's possessives): ${terms.joinToString(", ")}."
        return "$faithfulPrompt\n$surface\n$language$block"
    }

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()

        fun quickSkipSecs(speed: EnhanceSpeed): Double = when (speed) {
            EnhanceSpeed.Fast -> 6.25
            EnhanceSpeed.Thinking -> 5.0
            EnhanceSpeed.Ultra -> 1.5
        }

        fun timeoutMs(speed: EnhanceSpeed): Long = when (speed) {
            EnhanceSpeed.Fast -> 12_000L
            EnhanceSpeed.Thinking -> 15_000L
            EnhanceSpeed.Ultra -> 20_000L
        }
    }
}
