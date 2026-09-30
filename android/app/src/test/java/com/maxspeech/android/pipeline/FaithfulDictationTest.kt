package com.maxspeech.android.pipeline

import com.maxspeech.android.data.EnhanceSpeed
import java.io.File
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.Call
import okhttp3.EventListener
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.SocketPolicy
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class FaithfulDictationTest {
    private fun fixture(name: String): String = File(System.getProperty("dictation.fixtures"), name).readText()

    @Test
    fun sharedGoldenCases() {
        val cases = JSONArray(fixture("golden.json"))
        for (i in 0 until cases.length()) {
            val case = cases.getJSONObject(i)
            val dictionary = case.getJSONArray("dictionary").let { values ->
                (0 until values.length()).map { values.getString(it) }
            }
            val snippets = case.getJSONArray("snippets").let { values ->
                (0 until values.length()).map { values.getJSONArray(it).let { pair -> pair.getString(0) to pair.getString(1) } }
            }
            val expanded = FaithfulDictation.expandExplicit(case.getString("input"), dictionary, snippets)
            assertEquals(case.getString("name"), case.getString("output"),
                FaithfulDictation.localCleanup(expanded, case.getString("tone"), case.getBoolean("english")))
        }
    }

    @Test
    fun guardRejectsSmallChangesAndParaphrases() {
        val local = "I do not want to change the words in this long sentence."
        for (candidate in listOf("Please preserve my phrasing.",
            "I do want to change the words in this long sentence.",
            "I do not wish to change the words in this long sentence.",
            "I do not want to change the words.", "")) {
            val decision = FaithfulDictation.guardOutput(local, candidate)
            assertNotEquals("accepted", decision.reason)
            assertEquals(local, decision.text)
        }
        assertEquals("accepted", FaithfulDictation.guardOutput(local, local.uppercase()).reason)
        assertEquals("surface_difference", FaithfulDictation.guardOutput("Yes. No.", "Yes, no.").reason)
        assertEquals("lexical_drift", FaithfulDictation.guardOutput("one two three", "three two one").reason)
        assertTrue(FaithfulDictation.guardOutput(local, local.replace(" not", "")).drift < 0.20)
        for (candidate in listOf("Send 3 files to Sarah.", "Send 2 files to Sandra.")) {
            assertEquals("lexical_drift", FaithfulDictation.guardOutput("Send 2 files to Sarah.", candidate).reason)
        }
    }

    @Test
    fun promptsPreserveWordsAndDictionaryForEveryTone() {
        val client = EnhanceClient(fixture("prompt.txt"))
        for (tone in listOf("default", "casual", "formal", "prose", "code")) {
            val prompt = client.systemPrompt(tone, true, listOf("MaxSpeech"))
            assertTrue(prompt.startsWith(fixture("prompt.txt")))
            assertTrue(prompt.contains("Keep every content word"))
            assertTrue(prompt.contains("Never paraphrase"))
            assertTrue(prompt.contains("Preferred vocabulary"))
            assertFalse(prompt.contains("Rewrite in"))
        }
        assertEquals(6.25, EnhanceClient.quickSkipSecs(EnhanceSpeed.Fast), 0.0)
        assertEquals(5.0, EnhanceClient.quickSkipSecs(EnhanceSpeed.Thinking), 0.0)
        assertEquals(1.5, EnhanceClient.quickSkipSecs(EnhanceSpeed.Ultra), 0.0)
    }

    @Test
    fun httpErrorsMalformedAndTruncatedResponsesFallBack() = runBlocking {
        MockWebServer().use { server ->
            server.start()
            val client = EnhanceClient(fixture("prompt.txt"), endpoint = server.url("/").toString())
            val responses = listOf(
                MockResponse().setResponseCode(500),
                MockResponse().setBody("not json"),
                MockResponse().setBody("""{"choices":[{"finish_reason":"length","message":{"content":"I"}}]}"""),
                MockResponse().setBody("""{"choices":[{"finish_reason":"stop","message":{"content":""}}]}"""),
            )
            for (response in responses) {
                server.enqueue(response)
                assertEquals("I need coffee.", client.enhance("I need coffee.", "default", "test-key", EnhanceSpeed.Thinking, false, emptyList(), 1000))
            }
        }
    }

    @Test
    fun allSpeedsUseSameModelAndRejectParaphrases() = runBlocking {
        MockWebServer().use { server ->
            server.start()
            val client = EnhanceClient(fixture("prompt.txt"), endpoint = server.url("/").toString())
            for (speed in EnhanceSpeed.entries) {
                server.enqueue(MockResponse().setBody("""{"choices":[{"finish_reason":"stop","message":{"content":"Please get me a beverage."}}]}"""))
                assertEquals("I need coffee.", client.enhance("I need coffee.", "default", "test-key", speed, false, listOf("MaxSpeech"), 1000))
                val body = JSONObject(server.takeRequest(2, TimeUnit.SECONDS)!!.body.readUtf8())
                assertEquals("gpt-4o-mini", body.getString("model"))
                assertEquals(0.0, body.getDouble("temperature"), 0.0)
            }
        }
    }

    @Test
    fun cancellationCancelsSocketWithinProcessingBudget() = runBlocking {
        MockWebServer().use { server ->
            server.start()
            server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.NO_RESPONSE))
            val cancelled = AtomicBoolean(false)
            val http = OkHttpClient.Builder().eventListener(object : EventListener() {
                override fun canceled(call: Call) { cancelled.set(true) }
            }).build()
            val client = EnhanceClient(fixture("prompt.txt"), http, server.url("/").toString())
            val start = System.nanoTime()
            try {
                withTimeout(150) {
                    client.enhance("I need coffee.", "default", "test-key", EnhanceSpeed.Ultra, false, emptyList(), 420)
                }
                fail("Cancellation must propagate")
            } catch (_: CancellationException) {
                assertTrue(cancelled.get())
                assertTrue((System.nanoTime() - start) / 1_000_000 < 420)
            }
        }
    }

    @Test
    fun staleSessionsAndRepeatedFinishOrPasteCannotWin() {
        val session = DictationSession()
        val first = session.next()
        assertEquals(first, session.beginFinish())
        assertNull(session.beginFinish())
        assertTrue(session.claimPaste(first))
        assertFalse(session.claimPaste(first))
        val second = session.next()
        assertFalse(session.isCurrent(first))
        assertFalse(session.claimPaste(first))
        assertEquals(second, session.beginFinish())
        assertTrue(session.claimPaste(second))
    }

    @Test
    fun finalizedTranscriptWinsWithoutDroppingInterimTail() {
        val accumulator = TranscriptAccumulator()
        accumulator.add("Daniel walked outside", false)
        accumulator.add("Daniel walked", true)
        assertEquals("Daniel walked outside", accumulator.text())
        accumulator.add("out", true)
        assertEquals("Daniel walked out", accumulator.text())
        accumulator.clear()
        accumulator.add("I need coffee", true)
        accumulator.add("I need coffee now", true)
        assertEquals("I need coffee now", accumulator.text())
        accumulator.add("now please", false)
        assertEquals("I need coffee now please", accumulator.text())
    }

    @Test
    fun legacyWordSubstitutionsCannotAffectExplicitVocabulary() {
        assertEquals("then Than", FaithfulDictation.expandExplicit("then than", listOf("Than"), emptyList()))
        assertEquals("Regards signal", FaithfulDictation.expandExplicit("sign off signal", emptyList(), listOf("sign" to "X", "sign off" to "Regards")))
    }
}
