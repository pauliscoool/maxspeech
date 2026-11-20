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
    /** Test-only MockWebServer base URL; production leaves this null. */
    private val endpointOverride: String? = null,
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
            val (endpoint, model) = if (endpointOverride != null) {
                endpointOverride to MODEL
            } else {
                resolveEndpoint(apiKey)
            }
            val body = JSONObject()
                .put("model", model)
                .put("reasoning_effort", "none")
                .put(
                    "messages",
                    JSONArray()
                        .put(JSONObject().put("role", "system").put("content", systemPrompt(tone, multilingual, dictionary)))
                        .put(JSONObject().put("role", "user").put("content", text)),
                )
            val request = Request.Builder().url(endpoint)
                .header("Authorization", "Bearer $apiKey")
                .post(body.toString().toRequestBody(JSON)).build()
            val call = http.newCall(request)
            call.timeout().timeout(minOf(timeoutMs(speed), deadlineMs), TimeUnit.MILLISECONDS)
            val candidate = awaitContent(call)
            val decision = FaithfulDictation.guardOutput(text, candidate)
            Log.i(
                "MaxSpeech",
                "Dictation guard model=$model reason=${decision.reason} drift=${decision.drift} " +
                    "elapsed_ms=${(System.nanoTime() - started) / 1_000_000}",
            )
            decision.text
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            Log.w(
                "MaxSpeech",
                "Dictation enhance fallback reason=request_error elapsed_ms=${(System.nanoTime() - started) / 1_000_000}",
            )
            text
        }
    }

    private suspend fun awaitContent(call: Call): String = suspendCancellableCoroutine { continuation ->
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                if (continuation.isActive) continuation.resumeWithException(e)
            }

            override fun onResponse(call: Call, response: Response) {
                response.use {
                    try {
                        if (!response.isSuccessful) {
                            val err = response.body?.string().orEmpty().take(200)
                            throw IOException("Enhance HTTP ${response.code}: $err")
                        }
                        val choice = JSONObject(response.body?.string().orEmpty())
                            .getJSONArray("choices").getJSONObject(0)
                        val finish = choice.optString("finish_reason")
                        if (finish.isNotEmpty() && finish != "stop") {
                            throw IOException("Incomplete enhance response ($finish)")
                        }
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
        val language = if (multilingual) {
            "Preserve all languages and scripts. Do not translate or transliterate."
        } else {
            "Preserve the spoken language."
        }
        val terms = dictionary.map { it.trim() }.filter { it.isNotEmpty() }.take(60)
        val block = if (terms.isEmpty()) {
            ""
        } else {
            "\n\nPreferred vocabulary (spell and capitalize exactly when the user says these; restore Name's possessives): ${terms.joinToString(", ")}."
        }
        return "$faithfulPrompt\n$surface\n$language$block\n\nOutput ONLY the final edited transcript. No quotes, labels, or commentary."
    }

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()
        private const val OPENAI_URL = "https://api.openai.com/v1/chat/completions"
        private const val OPENROUTER_URL = "https://openrouter.ai/api/v1/chat/completions"
        private const val MODEL = "gpt-6-luna"

        fun resolveEndpoint(apiKey: String): Pair<String, String> =
            if (apiKey.trim().startsWith("sk-or-")) {
                OPENROUTER_URL to "openai/gpt-6-luna"
            } else {
                OPENAI_URL to MODEL
            }

        fun looksLikeChatKey(key: String): Boolean = key.trim().startsWith("sk-")

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
