package com.maxspeech.android.data

/**
 * App-managed Deepgram fallback, XOR-obfuscated the same way as the desktop
 * `secrets.rs`. Never log the decoded key.
 */
object Secrets {
    private val MASK = intArrayOf(
        0xA7, 0x3C, 0x91, 0x5E, 0xE2, 0x19, 0xB4, 0x6D,
        0x42, 0xF8, 0x0C, 0x77, 0xD1, 0x2A, 0x9E, 0x53,
    )
    private val BYTES = intArrayOf(
        0x9E, 0x0D, 0xA9, 0x3C, 0x83, 0x29, 0x8D, 0x0F, 0x75, 0xCD,
        0x39, 0x11, 0xE9, 0x49, 0xA7, 0x6B, 0x96, 0x0A, 0xF3, 0x3B,
        0xD2, 0x2A, 0x80, 0x5D, 0x72, 0xCD, 0x6A, 0x11, 0xB4, 0x49,
        0xA8, 0x63, 0x90, 0x5A, 0xF7, 0x6C, 0xD0, 0x2C, 0xD2, 0x5E,
    )

    const val SUPABASE_URL = "https://eqvmjmejcwkrylqyglfm.supabase.co"
    const val AUTH_REDIRECT = "https://maxspeech.vercel.app/reset"
    const val SUPABASE_ANON_KEY =
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImVxdm1qbWVqY3drcnlscXlnbGZtIiwicm9sZSI6ImFub24iLCJpYXQiOjE3ODU1MTA1ODMsImV4cCI6MjEwMTA4NjU4M30.9VauCDSWaAEpSuIkKnPsg9Ap7MLWtXqQSQ8I4xC7_fY"

    fun deepgramFallbackKey(): String {
        val out = ByteArray(BYTES.size)
        for (i in BYTES.indices) {
            out[i] = (BYTES[i] xor MASK[i % MASK.size]).toByte()
        }
        return String(out, Charsets.UTF_8)
    }

    fun deepgramKeys(userKey: String?): List<String> {
        val fallback = deepgramFallbackKey()
        val keys = ArrayList<String>(2)
        val user = userKey?.trim().orEmpty()
        if (user.isNotEmpty() && user != fallback) keys += user
        keys += fallback
        return keys
    }
}
