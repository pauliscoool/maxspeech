package com.maxspeech.android.pipeline

import com.maxspeech.android.pipeline.VoiceCommands.Result
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class VoiceCommandsTest {
    @Test fun scratchThat() {
        assertEquals(Result.ScratchThat, VoiceCommands.check("Scratch that."))
        assertEquals(Result.ScratchThat, VoiceCommands.check("never mind"))
    }

    @Test fun punctuationAndLines() {
        assertEquals(Result.InsertText("\n"), VoiceCommands.check("new line"))
        assertEquals(Result.InsertText("\n\n"), VoiceCommands.check("New paragraph."))
        assertEquals(Result.InsertText("?"), VoiceCommands.check("question mark"))
    }

    @Test fun rewrite() {
        assertEquals(Result.Rewrite("formal"), VoiceCommands.check("Make it formal"))
        assertEquals(Result.Rewrite("shorter"), VoiceCommands.check("make that shorter."))
    }

    @Test fun regularDictationIsNotACommand() {
        assertNull(VoiceCommands.check("I need a comma in my essay"))
        assertNull(VoiceCommands.check("make it"))
    }
}
