package com.maxspeech.android.data

import androidx.annotation.DrawableRes
import com.maxspeech.android.R

enum class StyleGroupId(val key: String, val title: String, val subtitle: String) {
    Messaging("messaging", "Messaging", "Chats & quick replies"),
    Email("email", "Email", "Gmail, Outlook, Yahoo & Proton"),
    Work("work", "Work", "Teams, Slack & docs"),
    Social("social", "Social", "Feeds & posts"),
    ;

    companion object {
        fun parse(raw: String?): StyleGroupId =
            entries.firstOrNull { it.key.equals(raw, ignoreCase = true) } ?: Messaging
    }
}

data class BrandMark(
    val label: String,
    val packageName: String,
    /** ARGB brand plate behind the icon. */
    val color: Long,
    /**
     * Simple Icons slug — loaded as the real brand mark
     * (https://cdn.simpleicons.org/{slug}/{hex}).
     */
    val simpleIcon: String,
    /** Bundled fallback drawable. */
    @DrawableRes val logoRes: Int,
    val altPackages: List<String> = emptyList(),
)

data class StyleGroupDef(
    val id: StyleGroupId,
    val brands: List<BrandMark>,
)

/** App-group tone defaults — MaxSpeech / Maximus Dev product, not Deepgram. */
object StyleGroups {
    val tones = listOf("casual", "default", "formal", "prose")

    val all: List<StyleGroupDef> = listOf(
        StyleGroupDef(
            StyleGroupId.Messaging,
            listOf(
                BrandMark("WhatsApp", "com.whatsapp", 0xFF25D366, "whatsapp", R.drawable.logo_whatsapp),
                BrandMark(
                    "Messages",
                    "com.google.android.apps.messaging",
                    0xFF1A73E8,
                    "googlemessages",
                    R.drawable.logo_messages,
                ),
                BrandMark("Telegram", "org.telegram.messenger", 0xFF2AABEE, "telegram", R.drawable.logo_telegram),
                BrandMark("Discord", "com.discord", 0xFF5865F2, "discord", R.drawable.logo_discord),
                BrandMark("Signal", "org.thoughtcrime.securesms", 0xFF3A76F0, "signal", R.drawable.logo_signal),
            ),
        ),
        StyleGroupDef(
            StyleGroupId.Email,
            listOf(
                BrandMark("Gmail", "com.google.android.gm", 0xFFEA4335, "_bundled", R.drawable.logo_gmail),
                BrandMark(
                    "Outlook",
                    "com.microsoft.office.outlook",
                    0xFF0078D4,
                    "_bundled",
                    R.drawable.logo_outlook,
                ),
                BrandMark(
                    "Yahoo",
                    "com.yahoo.mobile.client.android.mail",
                    0xFF6001D2,
                    "_bundled",
                    R.drawable.logo_yahoo,
                ),
                BrandMark(
                    "Proton",
                    "ch.protonmail.android",
                    0xFF6D4AFF,
                    "_bundled",
                    R.drawable.logo_proton,
                ),
            ),
        ),
        StyleGroupDef(
            StyleGroupId.Work,
            listOf(
                BrandMark(
                    "Teams",
                    "com.microsoft.teams",
                    0xFF6264A7,
                    "microsoftteams",
                    R.drawable.logo_teams,
                    altPackages = listOf("com.microsoft.skype.teams"),
                ),
                BrandMark(
                    "Slack",
                    "com.Slack",
                    0xFF4A154B,
                    "slack",
                    R.drawable.logo_slack,
                    altPackages = listOf("com.slack"),
                ),
                BrandMark(
                    "Notion",
                    "notion.id",
                    0xFF37352F,
                    "notion",
                    R.drawable.logo_notion,
                    altPackages = listOf("com.notion.android", "com.notion.id"),
                ),
                BrandMark(
                    "Docs",
                    "com.google.android.apps.docs.editors.docs",
                    0xFF4285F4,
                    "_bundled",
                    R.drawable.logo_docs,
                ),
                BrandMark("LinkedIn", "com.linkedin.android", 0xFF0A66C2, "linkedin", R.drawable.logo_linkedin),
            ),
        ),
        StyleGroupDef(
            StyleGroupId.Social,
            listOf(
                BrandMark("Instagram", "com.instagram.android", 0xFFE1306C, "instagram", R.drawable.logo_instagram),
                BrandMark(
                    "X",
                    "com.twitter.android",
                    0xFF111111,
                    "x",
                    R.drawable.logo_x,
                    altPackages = listOf("com.twitter.android.lite"),
                ),
                BrandMark(
                    "TikTok",
                    "com.zhiliaoapp.musically",
                    0xFF010101,
                    "tiktok",
                    R.drawable.logo_tiktok,
                    altPackages = listOf("com.ss.android.ugc.trill"),
                ),
                BrandMark("Reddit", "com.reddit.frontpage", 0xFFFF4500, "reddit", R.drawable.logo_reddit),
                BrandMark(
                    "Facebook",
                    "com.facebook.katana",
                    0xFF1877F2,
                    "facebook",
                    R.drawable.logo_facebook,
                    altPackages = listOf("com.facebook.lite"),
                ),
            ),
        ),
    )

    fun def(id: StyleGroupId): StyleGroupDef = all.first { it.id == id }

    fun toneFromSlider(value: Float): String {
        val i = kotlin.math.round(value).toInt().coerceIn(0, tones.lastIndex)
        return tones[i]
    }

    fun sliderFromTone(tone: String): Float {
        val i = tones.indexOfFirst { it.equals(tone, ignoreCase = true) }
        return if (i >= 0) i.toFloat() else 1f
    }

    fun labelForTone(tone: String): String = tone.replaceFirstChar { it.uppercase() }

    /** Brand-colored Simple Icons SVG — the real marks, not random favicons. */
    fun logoUrl(brand: BrandMark): String? {
        if (brand.simpleIcon.startsWith("_")) return null
        val hex = (brand.color and 0xFFFFFFL).toString(16).padStart(6, '0')
        return "https://cdn.simpleicons.org/${brand.simpleIcon}/$hex"
    }

    fun logoData(brand: BrandMark): Any =
        logoUrl(brand) ?: brand.logoRes
}
