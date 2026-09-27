use chrono::{Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Free: 2 minutes of dictation per rolling 24 hours (wall-clock recording time).
pub const FREE_DAILY_SECONDS: u64 = 120;
/// Weekly word caps for paid tiers (UTC week: Mon 00:00 → next Mon).
pub const STARTER_WEEKLY_LIMIT: u64 = 4_500;
pub const PRO_WEEKLY_LIMIT: u64 = 10_000;
pub const MAX_WEEKLY_LIMIT: u64 = 25_000;

pub const OWNER_EMAIL: &str = "pauldimov5@gmail.com";

pub fn is_owner_email(email: &str) -> bool {
    email.trim().eq_ignore_ascii_case(OWNER_EMAIL)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanTier {
    Free,
    Starter,
    Pro,
    Max,
}

impl PlanTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Starter => "starter",
            Self::Pro => "pro",
            Self::Max => "max",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "starter" => Self::Starter,
            "pro" => Self::Pro,
            "max" => Self::Max,
            _ => Self::Free,
        }
    }

    pub fn price_usd(self) -> u32 {
        match self {
            Self::Free => 0,
            Self::Starter => 3,
            Self::Pro => 5,
            Self::Max => 10,
        }
    }

    /// Weekly word limit. Free is time-capped instead (`daily_seconds_limit`).
    pub fn weekly_limit(self) -> Option<u64> {
        match self {
            Self::Free => None,
            Self::Starter => Some(STARTER_WEEKLY_LIMIT),
            Self::Pro => Some(PRO_WEEKLY_LIMIT),
            Self::Max => Some(MAX_WEEKLY_LIMIT),
        }
    }

    /// Rolling 24-hour recording cap in seconds. Paid tiers are uncapped by time.
    pub fn daily_seconds_limit(self) -> Option<u64> {
        match self {
            Self::Free => Some(FREE_DAILY_SECONDS),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct PlanStatus {
    pub tier: PlanTier,
    pub price_usd: u32,
    pub weekly_limit: Option<u64>,
    pub words_used: u64,
    pub words_remaining: Option<u64>,
    pub week_starts_at: String,
    pub can_dictate: bool,
    pub bonus_words: u64,
    pub daily_seconds_limit: Option<u64>,
    pub daily_seconds_used: u64,
    pub seconds_remaining: Option<u64>,
}

impl PlanStatus {
    pub fn from_usage(tier: PlanTier, words_used: u64, week_starts_at: String) -> Self {
        Self::from_usage_with_bonus(tier, words_used, week_starts_at, 0, 0, false)
    }

    pub fn from_usage_with_bonus(
        tier: PlanTier,
        words_used: u64,
        week_starts_at: String,
        bonus_words: u64,
        daily_seconds_used: u64,
        unlimited: bool,
    ) -> Self {
        let weekly_limit = tier.weekly_limit().map(|lim| lim.saturating_add(bonus_words));
        let words_remaining = weekly_limit.map(|lim| lim.saturating_sub(words_used));
        // On Free, owner-granted bonus_words are extra seconds for the 24h window.
        // Owner account is not time-gated (seconds_remaining stays None).
        let daily_seconds_limit = if unlimited {
            None
        } else {
            tier
                .daily_seconds_limit()
                .map(|lim| lim.saturating_add(bonus_words))
        };
        let seconds_remaining =
            daily_seconds_limit.map(|lim| lim.saturating_sub(daily_seconds_used));
        let can_dictate = if unlimited {
            true
        } else if let Some(lim) = daily_seconds_limit {
            daily_seconds_used < lim
        } else if let Some(lim) = weekly_limit {
            words_used < lim
        } else {
            true
        };
        Self {
            tier,
            price_usd: tier.price_usd(),
            weekly_limit,
            words_used,
            words_remaining,
            week_starts_at,
            can_dictate,
            bonus_words,
            daily_seconds_limit,
            daily_seconds_used,
            seconds_remaining,
        }
    }
}

/// Monday 00:00:00 UTC of the current week, formatted for SQLite `datetime('now')` comparisons.
pub fn week_starts_at_sql() -> String {
    week_start_utc(Utc::now()).format("%Y-%m-%d %H:%M:%S").to_string()
}

fn week_start_utc(now: chrono::DateTime<Utc>) -> chrono::NaiveDateTime {
    let days_since_monday = now.weekday().num_days_from_monday() as i64;
    let monday = now.date_naive() - Duration::days(days_since_monday);
    monday.and_hms_opt(0, 0, 0).expect("midnight is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parse_tiers() {
        assert_eq!(PlanTier::parse("free"), PlanTier::Free);
        assert_eq!(PlanTier::parse("STARTER"), PlanTier::Starter);
        assert_eq!(PlanTier::parse("pro"), PlanTier::Pro);
        assert_eq!(PlanTier::parse("max"), PlanTier::Max);
        assert_eq!(PlanTier::parse("unknown"), PlanTier::Free);
    }

    #[test]
    fn free_is_time_capped_not_weekly_words() {
        assert_eq!(PlanTier::Free.weekly_limit(), None);
        assert_eq!(PlanTier::Free.daily_seconds_limit(), Some(FREE_DAILY_SECONDS));
        let under = PlanStatus::from_usage_with_bonus(
            PlanTier::Free,
            9_999,
            "2026-07-28 00:00:00".into(),
            0,
            60,
            false,
        );
        assert!(under.can_dictate);
        assert_eq!(under.weekly_limit, None);
        assert_eq!(under.seconds_remaining, Some(60));
    }

    #[test]
    fn free_status_at_limit_blocks() {
        let s = PlanStatus::from_usage_with_bonus(
            PlanTier::Free,
            0,
            "2026-07-28 00:00:00".into(),
            0,
            FREE_DAILY_SECONDS,
            false,
        );
        assert!(!s.can_dictate);
        assert_eq!(s.seconds_remaining, Some(0));
        assert_eq!(s.daily_seconds_limit, Some(FREE_DAILY_SECONDS));
    }

    #[test]
    fn paid_tiers_have_caps() {
        assert_eq!(PlanTier::Starter.weekly_limit(), Some(STARTER_WEEKLY_LIMIT));
        assert_eq!(PlanTier::Pro.weekly_limit(), Some(PRO_WEEKLY_LIMIT));
        assert_eq!(PlanTier::Max.weekly_limit(), Some(MAX_WEEKLY_LIMIT));
        assert_eq!(PlanTier::Starter.price_usd(), 3);
        assert_eq!(PlanTier::Pro.price_usd(), 5);
        assert_eq!(PlanTier::Max.price_usd(), 10);
        assert_eq!(PlanTier::Starter.daily_seconds_limit(), None);
    }

    #[test]
    fn paid_status_at_limit_blocks() {
        let s = PlanStatus::from_usage(PlanTier::Pro, PRO_WEEKLY_LIMIT, "2026-07-28 00:00:00".into());
        assert!(!s.can_dictate);
        assert_eq!(s.words_remaining, Some(0));
        assert_eq!(s.weekly_limit, Some(PRO_WEEKLY_LIMIT));
        assert_eq!(s.price_usd, 5);
    }

    #[test]
    fn paid_status_under_limit_allows() {
        let s = PlanStatus::from_usage(PlanTier::Starter, 1_000, "2026-07-28 00:00:00".into());
        assert!(s.can_dictate);
        assert_eq!(s.words_remaining, Some(STARTER_WEEKLY_LIMIT - 1_000));
        assert_eq!(s.weekly_limit, Some(STARTER_WEEKLY_LIMIT));
        assert_eq!(s.bonus_words, 0);
    }

    #[test]
    fn bonus_words_raise_paid_cap() {
        let s = PlanStatus::from_usage_with_bonus(
            PlanTier::Starter,
            STARTER_WEEKLY_LIMIT,
            "2026-07-28 00:00:00".into(),
            500,
            0,
            false,
        );
        assert!(s.can_dictate);
        assert_eq!(s.weekly_limit, Some(STARTER_WEEKLY_LIMIT + 500));
        assert_eq!(s.words_remaining, Some(500));
        assert_eq!(s.bonus_words, 500);
    }

    #[test]
    fn bonus_seconds_raise_free_cap() {
        let s = PlanStatus::from_usage_with_bonus(
            PlanTier::Free,
            0,
            "2026-07-28 00:00:00".into(),
            30,
            FREE_DAILY_SECONDS,
            false,
        );
        assert!(s.can_dictate);
        assert_eq!(s.daily_seconds_limit, Some(FREE_DAILY_SECONDS + 30));
        assert_eq!(s.seconds_remaining, Some(30));
        assert_eq!(s.bonus_words, 30);
    }

    #[test]
    fn owner_unlimited_ignores_free_cap() {
        let s = PlanStatus::from_usage_with_bonus(
            PlanTier::Free,
            0,
            "2026-07-28 00:00:00".into(),
            0,
            10_000,
            true,
        );
        assert!(s.can_dictate);
        assert_eq!(s.daily_seconds_limit, None);
        assert_eq!(s.seconds_remaining, None);
        assert!(is_owner_email("  PaulDimov5@gmail.com "));
        assert!(!is_owner_email("other@example.com"));
    }

    #[test]
    fn week_start_is_monday_utc() {
        // Wednesday 2026-07-29 16:00 UTC → Monday 2026-07-27
        let wed = Utc.with_ymd_and_hms(2026, 7, 29, 16, 0, 0).unwrap();
        let start = week_start_utc(wed);
        assert_eq!(start.to_string(), "2026-07-27 00:00:00");

        // Monday itself stays that Monday
        let mon = Utc.with_ymd_and_hms(2026, 7, 27, 0, 0, 1).unwrap();
        assert_eq!(week_start_utc(mon).to_string(), "2026-07-27 00:00:00");
    }
}
