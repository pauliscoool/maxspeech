package com.maxspeech.android.pipeline

import java.io.IOException
import com.maxspeech.android.data.Secrets
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.json.JSONObject
import java.net.URLEncoder
import java.nio.ByteBuffer
import java.nio.ByteOrder

data class TranscriptChunk(val text: String, val isFinal: Boolean, val sessionId: Long)

class DeepgramClient {
    private val http = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(20, TimeUnit.SECONDS)
        .build()

    private val _chunks = MutableSharedFlow<TranscriptChunk>(extraBufferCapacity = 32)
    val chunks = _chunks.asSharedFlow()

    private var socket: WebSocket? = null
    @Volatile private var generation = 0L
    val sessionId: Long get() = generation
    private val ready = Channel<Result<Unit>>(Channel.BUFFERED)
    private val failures = Channel<Throwable>(Channel.CONFLATED)
    private val closed = Channel<Unit>(Channel.CONFLATED)

    fun connect(keys: List<String>, language: String, keyterms: List<String>) {
        close()
        val connection = generation
        while (ready.tryReceive().isSuccess) { /* A previous socket may have closed after its session ended. */ }
        while (failures.tryReceive().isSuccess) { /* Discard failures from the previous session. */ }
        while (closed.tryReceive().isSuccess) { /* Discard close events from the previous session. */ }
        val key = keys.firstOrNull().orEmpty()
        val url = buildUrl(language, keyterms)
        val req = Request.Builder()
            .url(url)
            .header("Authorization", "Token $key")
            .build()
        socket = http.newWebSocket(req, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                if (connection != generation) return
                ready.trySend(Result.success(Unit))
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                if (connection != generation) return
                parse(text, connection)
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (connection != generation) return
                val failure = if (response != null) {
                    IOException("Transcription service returned HTTP ${response.code}.", t)
                } else {
                    t
                }
                ready.trySend(Result.failure(failure))
                failures.trySend(failure)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (connection != generation) return
                val failure = IOException(
                    reason.ifBlank { "Transcription connection closed (code $code)." },
                )
                ready.trySend(Result.failure(failure))
                failures.trySend(failure)
                closed.trySend(Unit)
            }
        })
    }

    suspend fun awaitOpen() {
        ready.receive().getOrThrow()
    }

    suspend fun awaitFailure(): Throwable = failures.receive()

    suspend fun awaitClosed() {
        closed.receive()
    }

    fun sendPcm(samples: ShortArray) {
        val current = socket ?: throw IOException("Transcription connection is not open.")
        val bytes = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        samples.forEach { bytes.putShort(it) }
        if (!current.send(bytes.array().toByteString(0, samples.size * 2))) {
            throw IOException("Transcription connection stopped accepting audio.")
        }
    }

    fun finish(): Boolean {
        val current = socket ?: return false
        return current.send("""{"type":"CloseStream"}""")
    }

    fun close() {
        generation++
        socket?.cancel()
        socket = null
    }

    private fun parse(raw: String, connection: Long) {
        runCatching {
            val json = JSONObject(raw)
            val channel = json.optJSONObject("channel") ?: return
            val alts = channel.optJSONArray("alternatives") ?: return
            if (alts.length() == 0) return
            val transcript = alts.getJSONObject(0).optString("transcript")
            if (transcript.isBlank()) return
            val isFinal = json.optBoolean("is_final", false)
            _chunks.tryEmit(TranscriptChunk(transcript, isFinal, connection))
        }
    }

    private fun buildUrl(language: String, keyterms: List<String>): String {
        val endpointing = if (language == "multi") 350 else 650
        val sb = StringBuilder(
            "wss://api.deepgram.com/v1/listen?model=nova-3&language=$language&punctuate=true&interim_results=true&smart_format=true&numerals=true&endpointing=$endpointing&encoding=linear16&sample_rate=16000&channels=1",
        )
        for (term in keyterms.take(80)) {
            val t = term.trim()
            if (t.isNotEmpty()) {
                sb.append("&keyterm=").append(URLEncoder.encode(t, "UTF-8"))
            }
        }
        return sb.toString()
    }

    companion object {
        val BUILTIN_KEYTERMS = listOf(
            "Deepgram", "Supabase", "GitHub", "Vercel", "TypeScript", "JavaScript",
            "OpenAI", "ChatGPT", "Claude", "Cursor", "Slack", "Discord", "Notion",
            "Android", "WhatsApp", "Gmail",
        )
    }
}
