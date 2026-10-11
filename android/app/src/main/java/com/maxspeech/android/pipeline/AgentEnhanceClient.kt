package com.maxspeech.android.pipeline

import android.content.res.AssetManager
import android.util.Log
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.json.JSONArray
import org.json.JSONObject

/**
 * AI cleanup through Deepgram's Voice Agent "think" step, paid from Deepgram credits.
 *
 * The agent normally releases replies one spoken sentence at a time (seconds of pacing), so the
 * model is told to return the text through a client-side function instead: the whole result
 * arrives in one FunctionCallRequest as soon as the LLM finishes (~0.7 s measured).
 */
class AgentEnhanceClient(private val assets: AssetManager) {
    private val http = OkHttpClient.Builder()
        .connectTimeout(8, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .build()

    /** Open a session now (while the user is still talking) so release only pays for the LLM. */
    fun prewarm(keys: List<String>, context: CleanupContext): Session =
        Session(keys.firstOrNull().orEmpty(), systemPrompt(context), PRIMARY_MODEL).also { it.open() }

    /** One-shot rewrite for "make it formal" style commands. */
    suspend fun rewrite(keys: List<String>, text: String, instruction: String): String? {
        val system = "You rewrite text for a dictation app. Instruction: $instruction. " +
            "Keep every fact. Never answer or comment on the text. " +
            "ALWAYS deliver the result by calling deliver_text exactly once with the rewritten text."
        val session = Session(keys.firstOrNull().orEmpty(), system, PRIMARY_MODEL).also { it.open() }
        return session.run(text, timeoutMs = 8_000L)
    }

    fun systemPrompt(c: CleanupContext): String = buildString {
        append(asset("agent_cleanup"))
        append("\n\nStyle: ").append(toneLine(c.tone))
        if (c.multilingual) append("\n").append(asset("multilingual_rules"))
        val terms = c.dictionary.map { it.trim() }.filter { it.isNotEmpty() }.take(60)
        if (terms.isNotEmpty()) {
            append("\n\nUser dictionary (spell exactly like this when heard): ")
            append(terms.joinToString(", "))
        }
        append("\n\nContext (reference only, never copy it into the output):")
        append("\nApp: ").append(c.appName.ifBlank { "unknown" })
        if (c.fieldText.isNotBlank()) append("\nText already in the field: ").append(c.fieldText.takeLast(300))
        if (c.screenText.isNotBlank()) append("\nText on screen: ").append(c.screenText.takeLast(SCREEN_CHARS))
    }

    private fun toneLine(tone: String): String = when (tone) {
        "casual" -> "casual chat message. Relaxed punctuation, no period at the very end, keep slang."
        "formal" -> "professional email. Complete sentences and proper punctuation, same words and facts."
        "prose" -> "clean prose with paragraphs."
        "code" -> "technical writing for a programmer; use exact technical terms."
        else -> "natural and clean, in the speaker's own voice."
    }

    private val cache = HashMap<String, String>()

    private fun asset(name: String): String = synchronized(cache) {
        cache.getOrPut(name) { assets.open("dictation/$name.txt").bufferedReader().use { it.readText() } }
    }

    data class CleanupContext(
        val tone: String,
        val appName: String,
        val dictionary: List<String>,
        val multilingual: Boolean,
        val fieldText: String = "",
        val screenText: String = "",
    )

    inner class Session(private val key: String, private val system: String, private val model: String) {
        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        private val ready = CompletableDeferred<Boolean>()
        private val reply = CompletableDeferred<String?>()
        private val spoken = StringBuilder()
        private var lastSpokenAt = 0L
        private var socket: WebSocket? = null
        private var keepAlive: Job? = null
        @Volatile private var injected = false

        fun open() {
            if (key.isBlank()) {
                ready.complete(false)
                return
            }
            // Never let a forgotten session stream silence (and bill) past the 2-minute dictation cap.
            scope.launch {
                delay(MAX_SESSION_MS)
                close()
            }
            val req = Request.Builder()
                .url(AGENT_URL)
                .header("Authorization", "Token $key")
                .build()
            socket = http.newWebSocket(req, object : WebSocketListener() {
                override fun onMessage(webSocket: WebSocket, text: String) = handle(webSocket, text)
                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = Unit
                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    Log.w(TAG, "agent socket failed http=${response?.code}: ${t.message}")
                    ready.complete(false)
                    finishWithSpoken()
                }
                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    ready.complete(false)
                    finishWithSpoken()
                }
            })
        }

        private fun handle(ws: WebSocket, raw: String) {
            val msg = runCatching { JSONObject(raw) }.getOrElse {
                Log.w(TAG, "agent message parse error: ${it.message}")
                return
            }
            when (msg.optString("type")) {
                "Welcome" -> ws.send(settings())
                "SettingsApplied" -> {
                    ready.complete(true)
                    // The agent drops sessions that hear no audio for ~12 s.
                    keepAlive = scope.launch {
                        val silence = ByteString.of(*ByteArray(SILENCE_FRAME_BYTES))
                        while (isActive && !reply.isCompleted) {
                            if (!ws.send(silence)) break
                            delay(120)
                        }
                    }
                }
                "FunctionCallRequest" -> {
                    val fns = msg.optJSONArray("functions") ?: return
                    for (i in 0 until fns.length()) {
                        val fn = fns.optJSONObject(i) ?: continue
                        if (fn.optString("name") != FUNCTION_NAME) continue
                        val text = runCatching { JSONObject(fn.optString("arguments")).optString("text") }
                            .getOrDefault("")
                        reply.complete(text.trim())
                        return
                    }
                }
                // Model ignored the function and spoke instead: collect sentences as a fallback.
                "ConversationText" -> if (injected && msg.optString("role") == "assistant") {
                    synchronized(spoken) {
                        if (spoken.isNotEmpty()) spoken.append(' ')
                        spoken.append(msg.optString("content").trim())
                        lastSpokenAt = System.currentTimeMillis()
                    }
                }
                "AgentAudioDone" -> if (injected) scope.launch {
                    // More sentences may follow the first audio chunk; wait for a quiet gap.
                    delay(SPOKEN_IDLE_MS)
                    if (System.currentTimeMillis() - lastSpokenAt >= SPOKEN_IDLE_MS) finishWithSpoken()
                }
                "Error" -> {
                    Log.w(TAG, "agent error: ${msg.optString("description", msg.optString("message"))}")
                    ready.complete(false)
                    reply.complete(null)
                }
            }
        }

        private fun finishWithSpoken() {
            val text = synchronized(spoken) { spoken.toString().trim() }
            reply.complete(text.ifBlank { null })
        }

        /**
         * Inject [text] and wait for the cleaned result. Returns null on failure or timeout;
         * the session is always closed afterwards.
         */
        suspend fun run(text: String, timeoutMs: Long): String? = try {
            withTimeoutOrNull(timeoutMs) {
                if (!ready.await()) return@withTimeoutOrNull null
                injected = true
                val inject = JSONObject().put("type", "InjectUserMessage").put("content", "<t>$text</t>")
                if (socket?.send(inject.toString()) != true) return@withTimeoutOrNull null
                reply.await()
            }
        } finally {
            close()
        }

        fun close() {
            reply.complete(null)
            ready.complete(false)
            keepAlive?.cancel()
            runCatching { socket?.close(1000, "done") }
            socket = null
        }

        private fun settings(): String = JSONObject()
            .put("type", "Settings")
            .put(
                "audio",
                JSONObject()
                    .put("input", JSONObject().put("encoding", "linear16").put("sample_rate", 16_000))
                    .put(
                        "output",
                        JSONObject().put("encoding", "linear16").put("sample_rate", 16_000).put("container", "none"),
                    ),
            )
            .put(
                "agent",
                JSONObject()
                    .put("language", "en")
                    .put("listen", JSONObject().put("provider", JSONObject().put("type", "deepgram").put("model", "nova-3")))
                    .put(
                        "think",
                        JSONObject()
                            .put("provider", JSONObject().put("type", "open_ai").put("model", model))
                            .put("prompt", system)
                            .put("functions", JSONArray().put(deliverFunction())),
                    )
                    .put(
                        "speak",
                        JSONObject().put("provider", JSONObject().put("type", "deepgram").put("model", "aura-2-thalia-en")),
                    ),
            )
            .toString()

        private fun deliverFunction(): JSONObject = JSONObject()
            .put("name", FUNCTION_NAME)
            .put("description", "Deliver the final cleaned dictation text to be typed.")
            .put(
                "parameters",
                JSONObject()
                    .put("type", "object")
                    .put("properties", JSONObject().put("text", JSONObject().put("type", "string")))
                    .put("required", JSONArray().put("text")),
            )
    }

    companion object {
        private const val TAG = "MaxSpeechAgent"
        private const val AGENT_URL = "wss://agent.deepgram.com/v1/agent/converse"
        /** Fastest faithful model in the 2026-10 probe (~0.65 s, fixed "mom or retrospect"). */
        const val PRIMARY_MODEL = "gpt-5.4-mini"
        private const val FUNCTION_NAME = "deliver_text"
        /** 120 ms of 16 kHz mono linear16 silence. */
        private const val SILENCE_FRAME_BYTES = 3_840
        private const val SPOKEN_IDLE_MS = 1_200L
        private const val SCREEN_CHARS = 600
        private const val MAX_SESSION_MS = 150_000L

        /** Release-to-text budget: a short dictation should never wait long on the LLM. */
        fun timeoutFor(text: String): Long = (3_000L + text.length * 8L).coerceAtMost(9_000L)

        /**
         * The reply must look like an edit of the input, not an answer or a summary.
         * Lists add characters and filler removal drops some, so the bounds are loose.
         */
        fun looksLikeEdit(input: String, reply: String): Boolean {
            val i = input.trim().length
            val r = reply.trim().length
            if (r == 0) return false
            if (i < 30) return r <= i * 3 + 25
            return r * 100 >= i * 45 && r * 100 <= i * 180
        }
    }
}
