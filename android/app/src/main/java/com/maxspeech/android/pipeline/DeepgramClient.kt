package com.maxspeech.android.pipeline

import java.net.URLEncoder
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.io.IOException
import android.os.SystemClock
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody.Companion.toRequestBody
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
        .connectTimeout(8, TimeUnit.SECONDS)
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
    private val finalCount = AtomicInteger(0)
    private val finalizeAcks = AtomicInteger(0)
    /** Socket died mid-session or audio was dropped: the live transcript can't be trusted. */
    @Volatile private var degraded = false
    /** Last chunk heard was an interim, i.e. words exist that Deepgram hasn't finalized yet. */
    @Volatile private var interimPending = false
    private val pendingLock = Any()
    private val pending = ArrayDeque<okio.ByteString>(MAX_PENDING)

    fun connect(keys: List<String>, language: String, keyterms: List<String>) {
        closeQuietly()
        closed.set(false)
        open.set(false)
        degraded = false
        interimPending = false
        finalCount.set(0)
        finalizeAcks.set(0)
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
                        if (!webSocket.send(pending.removeFirst())) degraded = true
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
                android.util.Log.w("MaxSpeechStt", "stream failed http=${response?.code}: ${t.message}")
                open.set(false)
                degraded = true
                ready.trySend(Result.failure(t))
                closed.set(true)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (generation.get() != gen) return
                open.set(false)
                degraded = true
                ready.trySend(Result.failure(IllegalStateException(reason.ifBlank { "closed" })))
                closed.set(true)
            }
        })
    }

    /**
     * Transcribe the saved 16 kHz mono PCM in one request, retrying flaky-network failures
     * within [BATCH_BUDGET_MS]. Returns "" when the service heard no speech; throws
     * [IOException] when it could not be reached so callers can keep the audio for a retry.
     */
    suspend fun transcribeBatch(
        pcm: ByteArray,
        keys: List<String>,
        language: String,
        keyterms: List<String>,
    ): String = withContext(Dispatchers.IO) {
        val key = keys.firstOrNull().orEmpty()
        if (key.isBlank() || pcm.isEmpty()) return@withContext ""
        val sb = StringBuilder(
            "https://api.deepgram.com/v1/listen?model=nova-3&language=$language&punctuate=true&smart_format=true&numerals=true&encoding=linear16&sample_rate=16000&channels=1",
        )
        for (term in keyterms.take(Vocab.MAX_KEYTERMS)) {
            val t = term.trim()
            if (t.isNotEmpty()) sb.append("&keyterm=").append(URLEncoder.encode(t, "UTF-8"))
        }
        val url = sb.toString()
        val audioSecs = pcm.size / 32_000L
        val perTryMs = (20_000L + audioSecs * 500L).coerceAtMost(40_000L)
        val startedAt = SystemClock.elapsedRealtime()
        var lastError: Exception = IOException("No connection")
        var attempt = 0
        while (attempt < BATCH_ATTEMPTS) {
            val remaining = BATCH_BUDGET_MS - (SystemClock.elapsedRealtime() - startedAt)
            if (remaining < 4_000L) break
            val req = Request.Builder()
                .url(url)
                .header("Authorization", "Token $key")
                .post(pcm.toRequestBody("application/octet-stream".toMediaType()))
                .build()
            val client = http.newBuilder()
                .connectTimeout(10, TimeUnit.SECONDS)
                .writeTimeout(20, TimeUnit.SECONDS)
                .readTimeout(perTryMs, TimeUnit.MILLISECONDS)
                .callTimeout(minOf(perTryMs + 15_000L, remaining), TimeUnit.MILLISECONDS)
                .build()
            try {
                client.newCall(req).execute().use { resp ->
                    val body = resp.body?.string().orEmpty()
                    if (resp.isSuccessful) {
                        return@withContext runCatching {
                            JSONObject(body).getJSONObject("results").getJSONArray("channels").getJSONObject(0)
                                .getJSONArray("alternatives").getJSONObject(0).optString("transcript").trim()
                        }.getOrDefault("")
                    }
                    val retryable = resp.code == 408 || resp.code == 429 || resp.code >= 500
                    if (!retryable) throw IllegalStateException("Speech service rejected the audio (${resp.code})")
                    lastError = IOException("Speech service busy (${resp.code})")
                }
            } catch (e: IOException) {
                lastError = e
            }
            attempt++
            if (attempt < BATCH_ATTEMPTS) delay(1_000L * attempt)
        }
        throw lastError
    }

    /** True once the socket is open; false on failure or timeout. Never throws: recording must not depend on it. */
    suspend fun awaitOpen(timeoutMs: Long): Boolean =
        withTimeoutOrNull(timeoutMs) { ready.receive().isSuccess } ?: false

    fun sendPcm(samples: ShortArray) {
        if (closed.get()) {
            degraded = true
            return
        }
        val bytes = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        samples.forEach { bytes.putShort(it) }
        val payload = bytes.array().toByteString(0, samples.size * 2)
        if (!open.get()) {
            synchronized(pendingLock) {
                if (!open.get()) {
                    if (pending.size >= MAX_PENDING) {
                        pending.removeFirst()
                        degraded = true
                    }
                    pending.addLast(payload)
                    return
                }
            }
        }
        if (socket?.send(payload) != true) degraded = true
    }

    /**
     * Finalize, wait for last finals, CloseStream.
     * Marks this generation done so late onClosed cannot break the next start.
     * Returns true only when the live transcript is known complete; false means the caller
     * should re-transcribe the recording (socket never opened, dropped, or the tail never arrived).
     */
    suspend fun finishAndFlush(): Boolean {
        val ws = socket ?: return false
        val gen = generation.get()
        // A one-word dictation can end before the handshake finishes; wait for onOpen
        // to flush the buffered audio instead of discarding it.
        var waited = 0L
        while (!open.get() && !closed.get() && waited < OPEN_WAIT_MS) {
            delay(25)
            waited += 25
        }
        val opened = open.get()
        val finalsBefore = finalCount.get()
        val acksBefore = finalizeAcks.get()
        open.set(false)
        synchronized(pendingLock) { pending.clear() }
        var acked = false
        if (opened && !degraded) {
            runCatching { ws.send("""{"type":"Finalize"}""") }
            // Give slow links longer only when unfinalized words are known to be in flight.
            val waitMs = if (interimPending) FLUSH_WAIT_SLOW_MS else FLUSH_WAIT_MS
            var t = 0L
            while (t < waitMs && !closed.get()) {
                if (finalizeAcks.get() != acksBefore || finalCount.get() != finalsBefore) {
                    acked = true
                    break
                }
                delay(25)
                t += 25
            }
            runCatching { ws.send("""{"type":"CloseStream"}""") }
            delay(120)
        }
        val complete = opened && !degraded && (acked || !interimPending)
        // Invalidate before close so onClosed is ignored.
        generation.compareAndSet(gen, gen + 1)
        runCatching { ws.close(1000, "done") }
        if (socket === ws) socket = null
        closed.set(true)
        return complete
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
            if (json.optBoolean("from_finalize", false)) finalizeAcks.incrementAndGet()
            val channel = json.optJSONObject("channel") ?: return
            val alts = channel.optJSONArray("alternatives") ?: return
            if (alts.length() == 0) return
            val transcript = alts.getJSONObject(0).optString("transcript")
            if (transcript.isBlank()) return
            val isFinal = json.optBoolean("is_final", false) || json.optBoolean("speech_final", false)
            if (isFinal) finalCount.incrementAndGet()
            interimPending = !isFinal
            _chunks.tryEmit(TranscriptChunk(transcript, isFinal))
        }
    }

    private fun buildUrl(language: String, keyterms: List<String>): String {
        // Same as desktop stt/deepgram.rs build_url.
        val endpointing = if (language == "multi") 700 else 1100
        val sb = StringBuilder(
            "wss://api.deepgram.com/v1/listen?model=nova-3&language=$language&punctuate=true&interim_results=true&smart_format=true&numerals=true&endpointing=$endpointing&encoding=linear16&sample_rate=16000&channels=1",
        )
        for (term in keyterms.take(Vocab.MAX_KEYTERMS)) {
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
        private const val OPEN_WAIT_MS = 3_000L
        private const val FLUSH_WAIT_MS = 1_500L
        private const val FLUSH_WAIT_SLOW_MS = 4_000L
        private const val BATCH_ATTEMPTS = 3
        private const val BATCH_BUDGET_MS = 45_000L
    }
}
