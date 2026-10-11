package com.maxspeech.android.pipeline

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentEnhanceClientTest {
    @Test fun acceptsCleanupAndLists() {
        val input = "um so for the trip I need first my passport second the charger and third some snacks"
        val list = "So for the trip I need:\n1. My passport\n2. The charger\n3. Some snacks"
        assertTrue(AgentEnhanceClient.looksLikeEdit(input, list))
        assertTrue(AgentEnhanceClient.looksLikeEdit("can you tell me what time it is", "Can you tell me what time it is?"))
    }

    @Test fun rejectsAnswersAndSummaries() {
        val input = "can you tell me what time it is"
        assertFalse(
            AgentEnhanceClient.looksLikeEdit(
                input,
                "I don't have access to a clock, but you can check the time on your phone's status bar or ask your assistant for the local time.",
            ),
        )
        val long = "so basically what happened was we went to the store and they were out of milk and eggs so we drove across town to the other one"
        assertFalse(AgentEnhanceClient.looksLikeEdit(long, "We went shopping."))
        assertFalse(AgentEnhanceClient.looksLikeEdit(long, ""))
    }
}
