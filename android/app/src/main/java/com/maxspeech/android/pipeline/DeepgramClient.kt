package com.maxspeech.android.pipeline

import java.net.URLEncoder
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString.Companion.toByteString
import org.json.JSONObject

data class TranscriptChunk(val text: String, val isFinal: Boolean)

class DeepgramClient {
    private val http = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(20, TimeUnit.SECONDS)
        .build()

    private val _chunks = MutableSharedFlow<TranscriptChunk>(extraBufferCapacity = 256)
    val chunks = _chunks.asSharedFlow()

    private var socket: WebSocket? = null
    @Volatile private var ready: Channel<Result<Unit>> = Channel(Channel.BUFFERED)
    private val closed = AtomicBoolean(false)
    private val open = AtomicBoolean(false)
    /** Bumps every connect so stale onClosed from a prior socket can't poison awaitOpen. */
    private val generation = AtomicInteger(0)
    private val pendingLock = Any()
    private val pending = ArrayDeque<okio.ByteString>(MAX_PENDING)

    fun connect(keys: List<String>, language: String, keyterms: List<String>) {
        closeQuietly()
        closed.set(false)
        open.set(false)
        val gen = generation.incrementAndGet()
        // Fresh channel every session — old "closed" results must never reach awaitOpen.
        ready = Channel(Channel.BUFFERED)
        val key = keys.firstOrNull().orEmpty()
        val url = buildUrl(language, keyterms)
        val req = Request.Builder()
            .url(url)
            .header("Authorization", "Token $key")
            .build()
        socket = http.newWebSocket(req, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                if (generation.get() != gen) return
                // Flush audio captured while the handshake was in flight.
                synchronized(pendingLock) {
                    open.set(true)
                    while (pending.isNotEmpty()) {
                        webSocket.send(pending.removeFirst())
                    }
                }
                ready.trySend(Result.success(Unit))
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                if (generation.get() != gen) return
                parse(text)
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (generation.get() != gen) return
                open.set(false)
                ready.trySend(Result.failure(t))
                closed.set(true)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (generation.get() != gen) return
                open.set(false)
                // Only fail awaitOpen if we never opened — intentional CloseStream is fine.
                ready.trySend(Result.failure(IllegalStateException(reason.ifBlank { "closed" })))
                closed.set(true)
            }
        })
    }

    suspend fun awaitOpen() {
        ready.receive().getOrThrow()
    }

    fun sendPcm(samples: ShortArray) {
        if (closed.get()) return
        val bytes = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        samples.forEach { bytes.putShort(it) }
        val payload = bytes.array().toByteString(0, samples.size * 2)
        if (!open.get()) {
            synchronized(pendingLock) {
                if (!open.get()) {
                    if (pending.size >= MAX_PENDING) pending.removeFirst()
                    pending.addLast(payload)
                    return
                }
            }
        }
        socket?.send(payload)
    }

    /**
     * Finalize → wait for last finals → CloseStream.
     * Marks this generation done so late onClosed cannot break the next start.
     */
    suspend fun finishAndFlush(waitMs: Long = 550) {
        val ws = socket ?: return
        val gen = generation.get()
        open.set(false)
        synchronized(pendingLock) { pending.clear() }
        runCatching { ws.send("""{"type":"Finalize"}""") }
        delay(waitMs.coerceIn(350, 900))
        runCatching { ws.send("""{"type":"CloseStream"}""") }
        delay(120)
        // Invalidate before close so onClosed is ignored.
        generation.compareAndSet(gen, gen + 1)
        runCatching { ws.close(1000, "done") }
        if (socket === ws) socket = null
        closed.set(true)
    }

    fun finish() {
        val ws = socket
        val gen = generation.get()
        open.set(false)
        synchronized(pendingLock) { pending.clear() }
        runCatching { ws?.send("""{"type":"Finalize"}""") }
        runCatching { ws?.send("""{"type":"CloseStream"}""") }
        generation.compareAndSet(gen, gen + 1)
        runCatching { ws?.close(1000, "done") }
        socket = null
        closed.set(true)
    }

    fun close() {
        closeQuietly()
    }

    private fun closeQuietly() {
        val gen = generation.get()
        generation.compareAndSet(gen, gen + 1)
        open.set(false)
        synchronized(pendingLock) { pending.clear() }
        runCatching { socket?.cancel() }
        socket = null
        closed.set(true)
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
        val endpointing = if (language == "multi") 400 else 750
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
        /** ~2s of 50ms frames while the WebSocket handshake completes. */
        private const val MAX_PENDING = 40

        val BUILTIN_KEYTERMS = listOf(
            "MaxSpeech", "Maximus Dev", "Maximus", "Supabase", "GitHub", "Vercel",
            "TypeScript", "JavaScript", "OpenAI", "ChatGPT", "Claude", "Cursor",
            "Slack", "Discord", "Notion", "Android", "WhatsApp", "Gmail", "Outlook", "Teams",
        )
    }
}
