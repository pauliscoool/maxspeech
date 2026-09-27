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
        val long = PlanCalculator.wordCount(text) >= EnhancePolicy.longWordThreshold(speed)
        val (prompt, baseTokens) = when {
            long -> (systemPromptForSpeed(tone, multilingual, speed) + "\n\n" + toneSection(
                when (speed) {
                    EnhanceSpeed.Fast -> "long_fast"
                    EnhanceSpeed.Thinking -> "long_thinking"
                    EnhanceSpeed.Ultra -> "long_ultra"
                },
            )) to 4096
            tone == "default" -> cleanupPrompt(multilingual, speed) to 2048
            else -> systemPromptForSpeed(tone, multilingual, speed) to 1024
        }
        complete(
            // Fidelity first: the model must not reword, guess, or answer the dictation.
            system = asset("fidelity_rules") + "\n\n" + prompt + dictionaryBlock(dictTerms),
            user = text,
            apiKey = apiKey,
            model = if (speed == EnhanceSpeed.Ultra) "gpt-4o" else "gpt-4o-mini",
            temp = when (speed) {
                EnhanceSpeed.Fast -> 0.0
                EnhanceSpeed.Thinking -> 0.1
                // Higher temperatures paraphrase and invent words; dictation wants fidelity.
                EnhanceSpeed.Ultra -> 0.1
            },
            timeoutMs = when (speed) {
                EnhanceSpeed.Fast -> 12_000L
                EnhanceSpeed.Thinking -> 15_000L
                EnhanceSpeed.Ultra -> 20_000L
            },
            maxTokens = EnhancePolicy.tokenBudget(speed, baseTokens),
        )
    }

    /** Desktop `rewrite_with_llm`: apply a spoken instruction ("formal", "shorter") to [text]. */
    suspend fun rewrite(text: String, instruction: String, apiKey: String): String =
        withContext(Dispatchers.IO) {
            val head = "You are a Grammarly-like dictation assistant. Rewrite the text per the instruction. " +
                "Instruction: $instruction. Only return the rewritten text, nothing else."
            complete(rulesBlock(head, false), text, apiKey, "gpt-4o-mini", 0.1, 15_000L, 1024)
        }

    private fun complete(
        system: String,
        user: String,
        apiKey: String,
        model: String,
        temp: Double,
        timeoutMs: Long,
        maxTokens: Int,
    ): String {
        val text = user
        val body = JSONObject()
            .put("model", model)
            .put("temperature", temp)
            .put("max_tokens", maxTokens)
            .put(
                "messages",
                JSONArray()
                    .put(JSONObject().put("role", "system").put("content", system))
                    .put(JSONObject().put("role", "user").put("content", user)),
            )
            .toString()
        val client = http.newBuilder().readTimeout(timeoutMs, TimeUnit.MILLISECONDS).callTimeout(timeoutMs + 2_000L, TimeUnit.MILLISECONDS).build()
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

    private fun toneHead(tone: String): String = toneSection(tone).ifBlank { toneSection("default") }

    /** Desktop `system_prompt_for_speed`. */
    private fun systemPromptForSpeed(tone: String, multilingual: Boolean, speed: EnhanceSpeed): String = when (speed) {
        EnhanceSpeed.Fast -> {
            val flavor = toneSection("flavor_$tone").ifBlank { toneSection("flavor_default") }
            val multi = if (multilingual) " ${asset("multilingual_rules")}" else ""
            "${toneSection("fast_rules")} $flavor$multi"
        }
        EnhanceSpeed.Thinking -> rulesBlock(toneHead(tone), multilingual)
        EnhanceSpeed.Ultra -> rulesBlock(toneHead(tone), multilingual) + "\n\n" + toneSection("ultra_rules")
    }

    /** Desktop `cleanup_self_corrections_ex` (default tone, short dictation). */
    private fun cleanupPrompt(multilingual: Boolean, speed: EnhanceSpeed): String = when (speed) {
        EnhanceSpeed.Fast -> {
            val multi = if (multilingual) " ${asset("multilingual_rules")}" else ""
            "${toneSection("cleanup_fast")} ${toneSection("fast_rules")}$multi"
        }
        EnhanceSpeed.Thinking ->
            rulesBlock(toneSection("cleanup"), multilingual) + "\n\nOnly return the cleaned text, nothing else."
        EnhanceSpeed.Ultra ->
            rulesBlock(toneSection("cleanup"), multilingual) + "\n\n" + toneSection("ultra_rules") +
                "\n\nOnly return the cleaned text, nothing else."
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
