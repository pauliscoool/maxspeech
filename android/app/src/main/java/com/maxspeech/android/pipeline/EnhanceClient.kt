package com.maxspeech.android.pipeline

import android.content.res.AssetManager
import com.maxspeech.android.data.EnhanceSpeed
import com.maxspeech.android.data.PlanCalculator
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

class EnhanceClient(private val assets: AssetManager) {
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
        dictTerms: List<String> = emptyList(),
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
        complete(
            system = systemPrompt(text, tone, multilingual, speed) + dictionaryBlock(dictTerms),
            user = text,
            apiKey = apiKey,
            model = model,
            temp = temp,
            timeoutMs = timeoutMs,
        )
    }

    /** Desktop `rewrite_with_llm`: apply a spoken instruction ("formal", "shorter") to [text]. */
    suspend fun rewrite(text: String, instruction: String, apiKey: String): String =
        withContext(Dispatchers.IO) {
            val head = "You are a Grammarly-like dictation assistant. Rewrite the text per the instruction. " +
                "Instruction: $instruction. Only return the rewritten text, nothing else."
            complete(rulesBlock(head, false), text, apiKey, "gpt-4o-mini", 0.1, 15_000L)
        }

    private fun complete(
        system: String,
        user: String,
        apiKey: String,
        model: String,
        temp: Double,
        timeoutMs: Long,
    ): String {
        val text = user
        val body = JSONObject()
            .put("model", model)
            .put("temperature", temp)
            .put("max_tokens", 1024)
            .put(
                "messages",
                JSONArray()
                    .put(JSONObject().put("role", "system").put("content", system))
                    .put(JSONObject().put("role", "user").put("content", user)),
            )
            .toString()
        val client = http.newBuilder().readTimeout(timeoutMs, TimeUnit.MILLISECONDS).build()
        val req = Request.Builder()
            .url("https://api.openai.com/v1/chat/completions")
            .header("Authorization", "Bearer $apiKey")
            .post(body.toRequestBody(JSON))
            .build()
        return client.newCall(req).execute().use { resp ->
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

    /** Mirrors desktop tone.rs prompt selection (long → cleanup for default → tone). */
    private fun systemPrompt(text: String, tone: String, multilingual: Boolean, speed: EnhanceSpeed): String {
        val long = PlanCalculator.wordCount(text) >= 40
        val core = when {
            long -> rulesBlock(toneSection(tone).ifBlank { toneSection("default") }, multilingual) +
                "\n\n" + toneSection("long")
            tone == "default" -> rulesBlock(toneSection("cleanup"), multilingual) +
                "\n\nOnly return the cleaned text, nothing else."
            else -> rulesBlock(toneSection(tone).ifBlank { toneSection("default") }, multilingual)
        }
        val extra = when (speed) {
            EnhanceSpeed.Fast -> "\n\nKeep edits light, but still fix awkward phrasing and stray commas."
            EnhanceSpeed.Ultra -> "\n\nThorough pass: restore sentence boundaries, fix run-ons, do not invent facts."
            EnhanceSpeed.Thinking -> ""
        }
        return core + extra
    }

    /** Desktop `dictionary_prompt_block`. */
    private fun dictionaryBlock(terms: List<String>): String {
        val cleaned = terms.map { it.trim() }.filter { it.isNotEmpty() }.take(60)
        if (cleaned.isEmpty()) return ""
        return "\n\nPreferred vocabulary (spell and capitalize exactly when the user says these; " +
            "restore Name's possessives): ${cleaned.joinToString(", ")}."
    }

    private fun rulesBlock(head: String, multilingual: Boolean): String {
        val s = "$head\n\n${asset("grammar_rules")}\n\n${asset("natural_rules")}\n\n" +
            "${asset("asr_correction_rules")}\n\n${asset("self_correction_rules")}"
        return if (multilingual) "$s\n\n${asset("multilingual_rules")}" else s
    }

    private fun toneSection(name: String): String {
        val header = "[$name]"
        var inside = false
        val out = StringBuilder()
        for (line in asset("tones").lines()) {
            val t = line.trim()
            if (t.startsWith("[") && t.endsWith("]")) {
                inside = t == header
                continue
            }
            if (inside) out.appendLine(line)
        }
        return out.toString().trim()
    }

    private fun asset(name: String): String = cache.getOrPut(name) {
        assets.open("dictation/$name.txt").bufferedReader().use { it.readText() }
    }

    private val cache = HashMap<String, String>()

    companion object {
        private val JSON = "application/json; charset=utf-8".toMediaType()
    }
}
