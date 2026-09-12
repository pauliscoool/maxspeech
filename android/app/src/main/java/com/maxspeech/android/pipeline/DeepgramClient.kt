package com.maxspeech.android.pipeline

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

data class TranscriptChunk(val text: String, val isFinal: Boolean)

class DeepgramClient {
    private val http = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(20, TimeUnit.SECONDS)
        .build()

    private val _chunks = MutableSharedFlow<TranscriptChunk>(extraBufferCapacity = 32)
    val chunks = _chunks.asSharedFlow()

    private var socket: WebSocket? = null
    private val ready = Channel<Result<Unit>>(Channel.BUFFERED)

    fun connect(keys: List<String>, language: String, keyterms: List<String>) {
        close()
        val key = keys.firstOrNull().orEmpty()
        val url = buildUrl(language, keyterms)
        val req = Request.Builder()
            .url(url)
            .header("Authorization", "Token $key")
            .build()
        socket = http.newWebSocket(req, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                ready.trySend(Result.success(Unit))
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                parse(text)
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                ready.trySend(Result.failure(t))
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                ready.trySend(Result.failure(IllegalStateException(reason.ifBlank { "closed" })))
            }
        })
    }

    suspend fun awaitOpen() {
        ready.receive().getOrThrow()
    }

    fun sendPcm(samples: ShortArray) {
        val bytes = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        samples.forEach { bytes.putShort(it) }
        socket?.send(bytes.array().toByteString(0, samples.size * 2))
    }

    fun finish() {
        socket?.send("""{"type":"CloseStream"}""")
        socket?.close(1000, "done")
        socket = null
    }

    fun close() {
        socket?.cancel()
        socket = null
    }

    private fun parse(raw: String) {
        runCatching {
            val json = JSONObject(raw)
            val channel = json.optJSONObject("channel") ?: return
            val alts = channel.optJSONArray("alternatives") ?: return
            if (alts.length() == 0) return
            val transcript = alts.getJSONObject(0).optString("transcript")
            if (transcript.isBlank()) return
            val isFinal = json.optBoolean("is_final", false) || json.optBoolean("speech_final", false)
            _chunks.tryEmit(TranscriptChunk(transcript, isFinal))
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
