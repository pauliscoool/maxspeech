package com.maxspeech.android.pipeline

import com.maxspeech.android.pipeline.LocalCleanup.localAsrCleanup
import com.maxspeech.android.pipeline.LocalCleanup.localSelfCorrect
import com.maxspeech.android.pipeline.LocalCleanup.normalizeTerminalPunctuation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Mirrors the tests in src-tauri/src/pipeline/tone.rs — keep them in sync. */
class LocalCleanupTest {

    private fun has(out: String, s: String) = assertTrue(out, out.lowercase().contains(s))
    private fun lacks(out: String, s: String) = assertFalse(out, out.lowercase().contains(s))

    @Test fun tenTimesBecomesPercent() {
        assertEquals("10%", localAsrCleanup("10 times"))
        assertEquals("at 10%.", localAsrCleanup("at 10 times."))
        assertEquals("10%", localAsrCleanup("10 percent"))
        assertEquals("10 times faster", localAsrCleanup("10 times faster"))
        assertEquals("do it 10 times", localAsrCleanup("do it 10 times"))
        assertEquals("do it 10 times again", localAsrCleanup("do it 10 times again"))
    }

    @Test fun restoresMissingContractions() {
        assertEquals("don't worry", localAsrCleanup("dont worry"))
        assertEquals("that's fine", localAsrCleanup("thats fine"))
        assertEquals("I'm ready now", localAsrCleanup("im ready now"))
        assertEquals("let's go", localAsrCleanup("lets go"))
        assertEquals("lets the user in", localAsrCleanup("lets the user in"))
        assertEquals("I'd like coffee", localAsrCleanup("id like coffee"))
        assertEquals("user id is seven", localAsrCleanup("user id is 7"))
        assertEquals("Doesn't work", localAsrCleanup("Doesnt work"))
        assertEquals("I'll go later", localAsrCleanup("ill go later"))
        assertEquals("feel ill today", localAsrCleanup("feel ill today"))
        assertEquals("ain't ready", localAsrCleanup("aint ready"))
    }

    @Test fun reversesNumeralHomophones() {
        assertEquals("thanks for the update", localAsrCleanup("thanks 4 the update"))
        assertEquals("I need to go", localAsrCleanup("I need 2 go"))
        assertEquals("Too much work", localAsrCleanup("2 much work"))
        assertEquals("One of us", localAsrCleanup("1 of us"))
        assertEquals("no one else", localAsrCleanup("no 1 else"))
        assertEquals("For the meeting", localAsrCleanup("4 the meeting"))
        assertEquals("meet at 4pm", localAsrCleanup("meet at 4pm"))
        assertEquals("room 2", localAsrCleanup("room 2"))
        assertEquals("version 2", localAsrCleanup("version 2"))
        assertEquals("Covenant Core one", localAsrCleanup("Covenant Core 1"))
        assertEquals("I have two apples", localAsrCleanup("I have 2 apples"))
        assertEquals("chapter 3 is ready", localAsrCleanup("chapter 3 is ready"))
        assertEquals("issue 1042", localAsrCleanup("issue 1042"))
        assertEquals("call me at 5551212", localAsrCleanup("call me at 5551212"))
        assertEquals("built in 2024", localAsrCleanup("built in 2024"))
        assertEquals("I have ten apples", localAsrCleanup("I have 10 apples"))
        assertEquals("wait fifteen minutes", localAsrCleanup("wait 15 minutes"))
        assertEquals("I counted 21 people", localAsrCleanup("I counted 21 people"))
        assertEquals("from 2 to 5", localAsrCleanup("from 2 to 5"))
        assertEquals("needs 16 gb", localAsrCleanup("needs 16 gb"))
    }

    @Test fun fixesCommonEnglishHomophones() {
        assertEquals("it's a bug", localAsrCleanup("its a bug"))
        assertEquals("its own place", localAsrCleanup("its own place"))
        assertEquals("you're going to love this", localAsrCleanup("your going to love this"))
        assertEquals("your laptop", localAsrCleanup("your laptop"))
        assertEquals("too much work", localAsrCleanup("to much work"))
        assertEquals("better than that", localAsrCleanup("better then that"))
        assertEquals("could've been worse", localAsrCleanup("could of been worse"))
        assertEquals("could of course", localAsrCleanup("could of course"))
        assertEquals("they're going home", localAsrCleanup("their going home"))
        assertEquals("and then we left", localAsrCleanup("and then we left"))
        assertEquals("might've been worse", localAsrCleanup("might of been worse"))
        assertEquals("I should've known", localAsrCleanup("I shoulda known"))
        assertEquals("who's going later", localAsrCleanup("whose going later"))
        assertEquals("whose car is that", localAsrCleanup("whose car is that"))
        assertEquals("I have a lot to do", localAsrCleanup("I have alot to do"))
        assertEquals("at least try", localAsrCleanup("atleast try"))
        assertEquals("because I said so", localAsrCleanup("cuz I said so"))
        assertEquals("oh yeah", localAsrCleanup("oh yea"))
    }

    @Test fun expandsSpokenKToOkay() {
        assertEquals("okay", localAsrCleanup("k"))
        assertEquals("Okay", localAsrCleanup("K"))
        assertEquals("okay thanks", localAsrCleanup("k thanks"))
        assertEquals("okay", localAsrCleanup("ok"))
        assertEquals("Okay", localAsrCleanup("OK"))
        assertEquals("okay", localAsrCleanup("kay"))
        assertEquals("that's okay", localAsrCleanup("that's k"))
        assertEquals("it's okay", localAsrCleanup("its k"))
        assertEquals("you're okay", localAsrCleanup("your k"))
        assertEquals("Okay thanks", localAsrCleanup("Kay thanks"))
        assertEquals("vitamin k", localAsrCleanup("vitamin k"))
        assertEquals("press k", localAsrCleanup("press k"))
        assertEquals("Hi Kay", localAsrCleanup("Hi Kay"))
        assertEquals("costs 10 k", localAsrCleanup("costs 10 k"))
    }
    @Test fun dropsCommasAroundCasualAddressWords() {
        assertEquals("bro that's crazy", localAsrCleanup("bro, that's crazy"))
        assertEquals("what's up bro", localAsrCleanup("what's up, bro"))
        assertEquals("Dude no way dude.", localAsrCleanup("Dude, no way, dude."))
        assertEquals("apples, pears, and bananas", localAsrCleanup("apples, pears, and bananas"))
    }

    @Test fun trailingCommaBecomesPeriod() {
        assertEquals("This is a finished sentence.", normalizeTerminalPunctuation("This is a finished sentence,", "default"))
        assertEquals("Also done.", normalizeTerminalPunctuation("Also done;", "default"))
    }

    @Test fun keepsQuestionAndExclamation() {
        assertEquals("Are you free tomorrow?", normalizeTerminalPunctuation("Are you free tomorrow?", "default"))
        assertEquals("That was amazing!", normalizeTerminalPunctuation("That was amazing!", "default"))
    }

    @Test fun addsPeriodToCompleteStatement() {
        assertEquals("Please send the report today.", normalizeTerminalPunctuation("Please send the report today", "default"))
    }

    @Test fun casualToneDoesNotForcePeriod() {
        assertEquals("hey can you check this later", normalizeTerminalPunctuation("hey can you check this later", "casual"))
        assertEquals("hey can you check this later.", normalizeTerminalPunctuation("hey can you check this later,", "casual"))
    }

    @Test fun shortFragmentStaysUnpunctuated() {
        assertEquals("ok", normalizeTerminalPunctuation("ok", "default"))
        assertEquals("got it", normalizeTerminalPunctuation("got it", "default"))
    }

    @Test fun correctsTuesdayToMonday() {
        val out = localSelfCorrect("Would you like to go on a trip on Tuesday? Oh no I meant Monday")
        has(out, "monday"); lacks(out, "tuesday"); lacks(out, "meant")
    }

    @Test fun correctsEarlyTuesdayToMonday() {
        val out = localSelfCorrect("for Tuesday would you like to go on a trip? Oh no I meant Monday")
        has(out, "monday"); lacks(out, "tuesday"); lacks(out, "meant")
    }

    @Test fun correctsThursdayToFriday() {
        val out = localSelfCorrect("Let's meet through Thursday I meant Friday")
        has(out, "friday"); lacks(out, "thursday"); has(out, "through")
    }

    @Test fun keepsThroughTuesdayWhenIMeanIsDiscourse() {
        val out = localSelfCorrect("The deadline is through Tuesday I mean it can slip")
        has(out, "through tuesday"); has(out, "i mean")
    }

    @Test fun correctsIMean() {
        val out = localSelfCorrect("Send it to Sarah I mean Sandra")
        has(out, "sandra"); lacks(out, "sarah")
    }

    @Test fun correctsFullSentenceRestatementAfterPeriod() {
        val out = localSelfCorrect("Daniel walked out. I meant Samuel walked out")
        has(out, "samuel"); has(out, "walked out"); lacks(out, "daniel"); lacks(out, "meant")
    }

    @Test fun correctsFullSentenceRestatementWithoutPeriod() {
        val out = localSelfCorrect("Daniel walked out I meant Samuel walked out")
        has(out, "samuel"); lacks(out, "daniel"); lacks(out, "meant")
    }

    @Test fun correctsIMeantWithCommaAfterMarker() {
        val out = localSelfCorrect("Daniel walked out. I meant, Samuel walked out")
        has(out, "samuel"); lacks(out, "daniel"); lacks(out, "meant")
    }

    @Test fun correctsPrefixedRestatement() {
        val out = localSelfCorrect("So Daniel walked out. I meant Samuel walked out")
        has(out, "samuel"); has(out, "walked out"); lacks(out, "daniel"); lacks(out, "meant")
    }

    @Test fun correctsSingleNameAfterIMeant() {
        val out = localSelfCorrect("Daniel walked out. I meant Samuel")
        has(out, "samuel"); has(out, "walked out"); lacks(out, "daniel"); lacks(out, "meant")
    }

    @Test fun correctsAsrIMetWhenRestating() {
        val out = localSelfCorrect("Daniel walked out. I met Samuel walked out")
        has(out, "samuel"); lacks(out, "daniel"); lacks(out, "i met")
    }

    @Test fun keepsGenuineIMetMeeting() {
        val out = localSelfCorrect("Yesterday I met Samuel at noon")
        has(out, "yesterday"); has(out, "i met"); has(out, "samuel")
    }
}
