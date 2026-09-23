package com.maxspeech.android.pipeline

import com.maxspeech.android.pipeline.Vocab.Substitution
import com.maxspeech.android.pipeline.Vocab.applyLearnedPossessives
import com.maxspeech.android.pipeline.Vocab.applyPairs
import com.maxspeech.android.pipeline.Vocab.substitutionsFromRedictate
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Mirrors tests in src-tauri/src/pipeline/vocab.rs and learn_substitutions.rs. */
class VocabTest {

    // Unit tests run from android/app — read the same shared file desktop compiles in.
    private val fixes = Vocab.parsePhraseFixes(File("../../shared/dictation/asr_phrase_fixes.txt").readText())
    private fun asr(s: String) = Vocab.fixCommonAsr(s, fixes)

    @Test fun replacesWholeWordCasing() {
        assertEquals("use cloud storage", Vocab.replacePhraseCi("use Cloud storage", "cloud", "cloud"))
        assertEquals("cloudy day", Vocab.replacePhraseCi("cloudy day", "cloud", "cloud"))
    }

    @Test fun fixesGitVsGet() {
        assertEquals("Did you place it to Git?", asr("Did you place it to get?"))
        assertEquals("Did you push it to Git", asr("Did you push it to get"))
        assertEquals("open GitHub", asr("open get hub"))
        assertEquals("I want to get coffee", asr("I want to get coffee"))
    }

    @Test fun fixesSplitProductNames() {
        assertEquals("write it in TypeScript", asr("write it in type script"))
        assertEquals("deploy on Vercel", asr("deploy on verse cell"))
        assertEquals("open Supabase", asr("open super base"))
        assertEquals("ask ChatGPT", asr("ask chat gpt"))
        assertEquals("install CurseForge", asr("install curse forge"))
        assertEquals("edit in VS Code", asr("edit in vs code"))
        assertEquals("on the cloud", asr("on the clout"))
    }

    @Test fun fixesCovenantCoreMishears() {
        assertEquals("open Covenant Core", asr("open Covenant court"))
        assertEquals("Covenant Core", asr("Covenant Corner"))
        assertEquals("Covenant Core", asr("CovenantCore"))
        assertEquals("see you in court", asr("see you in court"))
        assertEquals("around the corner", asr("around the corner"))
    }

    @Test fun fixesTailscaleMishears() {
        assertEquals("open Tailscale", asr("open tail scale"))
        assertEquals("Tailscale VPN", asr("Tale Scale VPN"))
        assertEquals("Tailscale funnel", asr("tail-scale funnel"))
        assertEquals("the dog wagged its tail", asr("the dog wagged its tail"))
    }

    @Test fun restoresPossessiveFromLearnedName() {
        val names = listOf("Sandra", "Paul")
        assertEquals("Send it to Sandra's desk", applyLearnedPossessives("Send it to Sandras desk", names))
        assertEquals("Paul's laptop is here", applyLearnedPossessives("Pauls laptop is here", names))
        assertEquals("the reports are ready", applyLearnedPossessives("the reports are ready", names))
    }

    @Test fun expandsSnippetsAndDictionary() {
        val out = Vocab.expand(
            "my email please and ping sandra",
            snippets = listOf("my email" to "paul@example.com"),
            dictionary = listOf("Sandra"),
            phraseFixes = fixes,
            learned = emptyList(),
        )
        assertEquals("paul@example.com please and ping Sandra", out)
        assertEquals("hello", Vocab.expand("Sign off", listOf("sign off" to "hello"), emptyList(), fixes, emptyList()))
    }

    @Test fun learnsCourtToCoreInBigram() {
        val pairs = substitutionsFromRedictate("Covenant court", "Covenant Core")
        assertEquals(listOf(Substitution("covenant court", "Covenant Core")), pairs)
        assertEquals("open Covenant Core please", applyPairs("open Covenant court please", pairs))
        assertEquals("see you in court", applyPairs("see you in court", pairs))
    }

    @Test fun learnsDanielToSamuel() {
        val pairs = substitutionsFromRedictate("Daniel", "Samuel")
        assertEquals(listOf(Substitution("daniel", "Samuel")), pairs)
        assertEquals("Samuel walked out", applyPairs("Daniel walked out", pairs))
    }

    @Test fun learnsMidSentenceNameSwap() {
        assertEquals(
            listOf(Substitution("daniel", "Samuel")),
            substitutionsFromRedictate("Send it to Daniel please", "Send it to Samuel please"),
        )
    }

    @Test fun learnsSuffixAfterEraseThenRedictate() {
        assertEquals(listOf(Substitution("covenant court", "Covenant Core")), substitutionsFromRedictate("Covenant court", "Core"))
        assertEquals(listOf(Substitution("daniel", "Samuel")), substitutionsFromRedictate("Meet Daniel", "Samuel"))
    }

    @Test fun skipsUnsafeOrUnrelatedEdits() {
        assertTrue(substitutionsFromRedictate("see you in court", "Core").isEmpty())
        assertTrue(substitutionsFromRedictate("the", "a").isEmpty())
        assertTrue(substitutionsFromRedictate("to", "for").isEmpty())
        assertTrue(substitutionsFromRedictate("I", "we").isEmpty())
        assertTrue(substitutionsFromRedictate("a", "b").isEmpty())
        assertTrue(substitutionsFromRedictate("MyP@ssw0rd", "N3wSecret!").isEmpty())
        assertTrue(substitutionsFromRedictate("Abcdef1", "Xyzabc2").isEmpty())
        assertTrue(substitutionsFromRedictate("hello there friend", "goodbye now pal").isEmpty())
        assertTrue(substitutionsFromRedictate("Let's schedule a meeting tomorrow", "Ship the installer tonight").isEmpty())
    }

    @Test fun mergeKeytermsKeepsUserSlots() {
        val builtin = (0 until 81).map { "B$it" }
        val merged = Vocab.mergeKeyterms(listOf("Sandra", "Maximus"), builtin)
        assertEquals("Sandra", merged[0])
        assertEquals("Maximus", merged[1])
        assertTrue(merged.size <= Vocab.MAX_KEYTERMS)
        assertTrue(merged.contains("B0"))
    }
}
