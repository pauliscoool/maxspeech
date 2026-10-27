package com.maxspeech.android.bench

import android.os.SystemClock
import android.util.Log
import androidx.test.platform.app.InstrumentationRegistry
import com.maxspeech.android.data.Secrets
import com.maxspeech.android.pipeline.AgentEnhanceClient
import com.maxspeech.android.pipeline.DeepgramClient
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Release→text timing on the real phone/network, driven by a synthetic voice clip (no sign-in needed). */
class LatencyBenchmarkTest {
    private val tag = "MSBench"
    private val ioScope = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.Dispatchers.IO)

    private fun pcm(): ShortArray {
        val bytes = InstrumentationRegistry.getInstrumentation().context.assets
            .open("sample_short.wav").readBytes().drop(44).toByteArray()
        val sb = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        return ShortArray(sb.remaining()).also { sb.get(it) }
    }

    @Test
    fun releaseToText() = runBlocking {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val keys = Secrets.deepgramKeys(null)
        val samples = pcm()
        repeat(3) { run ->
            val stt = DeepgramClient()
            val finals = StringBuilder()
            stt.chunks.onEach { if (it.isFinal) finals.append(it.text).append(' ') }
                .launchIn(kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.Dispatchers.Default))
            val agent = AgentEnhanceClient(app.assets)
            val openStart = SystemClock.elapsedRealtime()
            stt.connect(keys, "en", emptyList())
            val session = agent.prewarm(keys, AgentEnhanceClient.CleanupContext("casual", "Messages", emptyList(), false))
            assertTrue("deepgram open", stt.awaitOpen(5_000))
            val openMs = SystemClock.elapsedRealtime() - openStart
            val chunk = 1_600 // 100 ms @16 kHz, sent in real time like the mic
            var i = 0
            while (i < samples.size) {
                val end = minOf(i + chunk, samples.size)
                stt.sendPcm(samples.copyOfRange(i, end))
                delay(100)
                i = end
            }
            val released = SystemClock.elapsedRealtime()
            val pcmBytes = ByteBuffer.allocate(samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
                .also { b -> samples.forEach { b.putShort(it) } }.array()
            val batch = ioScope.async {
                val t0 = SystemClock.elapsedRealtime()
                val text = stt.transcribeBatch(pcmBytes, keys, "en", emptyList())
                text to (SystemClock.elapsedRealtime() - t0)
            }
            val complete = stt.finishAndFlush()
            val flushMs = SystemClock.elapsedRealtime() - released
            delay(80)
            val raw = finals.toString().trim()
            val aiStart = SystemClock.elapsedRealtime()
            val reply = session.run(raw, AgentEnhanceClient.timeoutFor(raw))
            val aiMs = SystemClock.elapsedRealtime() - aiStart
            val (batchText, batchMs) = batch.await()
            Log.i(tag, "run=$run open=${openMs}ms audio=${samples.size / 16}ms flush=${flushMs}ms(complete=$complete) " +
                "ai=${aiMs}ms batch=${batchMs}ms")
            Log.i(tag, "run=$run stream='$raw'")
            Log.i(tag, "run=$run batch ='$batchText'")
            Log.i(tag, "run=$run ai    ='$reply'")
        }
    }
}
