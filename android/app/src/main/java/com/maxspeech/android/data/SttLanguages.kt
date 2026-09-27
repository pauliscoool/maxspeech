package com.maxspeech.android.data

data class SttLanguage(
    val code: String,
    val label: String,
    val multi: Boolean,
)

object SttLanguages {
    const val MAX = 5
    val DEFAULT = listOf("en")

    val ALL = listOf(
        SttLanguage("en", "English", true),
        SttLanguage("es", "Spanish", true),
        SttLanguage("zh", "Chinese (Mandarin)", false),
        SttLanguage("hi", "Hindi", true),
        SttLanguage("ar", "Arabic", false),
        SttLanguage("fr", "French", true),
        SttLanguage("pt", "Portuguese", true),
        SttLanguage("ru", "Russian", true),
        SttLanguage("de", "German", true),
        SttLanguage("ja", "Japanese", true),
        SttLanguage("ko", "Korean", false),
        SttLanguage("it", "Italian", true),
        SttLanguage("tr", "Turkish", false),
        SttLanguage("vi", "Vietnamese", false),
        SttLanguage("pl", "Polish", false),
        SttLanguage("uk", "Ukrainian", false),
        SttLanguage("nl", "Dutch", true),
        SttLanguage("id", "Indonesian", false),
        SttLanguage("th", "Thai", false),
        SttLanguage("bg", "Bulgarian", false),
    )

    fun allowed(): Set<String> = ALL.map { it.code }.toSet()

    fun multilingualAllowed(tier: PlanTier): Boolean = tier != PlanTier.Free
}
