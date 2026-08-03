use rusqlite::{Connection, params};
use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Serialize, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub app_name: String,
    pub created_at: String,
    /// Absolute path to local Remake WAV, if still retained (max 10 recent).
    pub recording_path: Option<String>,
    /// True when a Remake audio file exists on disk.
    pub can_remake: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct DictWord {
    pub id: i64,
    pub word: String,
    pub boost: f32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Macro {
    pub id: i64,
    pub trigger: String,
    pub expansion: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppProfile {
    pub id: i64,
    pub exe_pattern: String,
    pub title_pattern: String,
    pub tone: String,
    pub enabled: bool,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn new() -> Result<Self, rusqlite::Error> {
        let data_dir = dirs_data_dir();
        std::fs::create_dir_all(&data_dir).ok();
        let db_path = std::path::Path::new(&data_dir).join("maxspeech.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE IF NOT EXISTS history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 text TEXT NOT NULL,
                 app_name TEXT NOT NULL DEFAULT '',
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 recording_path TEXT
             );
             CREATE TABLE IF NOT EXISTS dictionary (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 word TEXT NOT NULL UNIQUE,
                 boost REAL NOT NULL DEFAULT 1.0
             );
             CREATE TABLE IF NOT EXISTS macros (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 trigger TEXT NOT NULL UNIQUE,
                 expansion TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS app_profiles (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 exe_pattern TEXT NOT NULL,
                 title_pattern TEXT NOT NULL DEFAULT '',
                 tone TEXT NOT NULL DEFAULT 'default',
                 enabled INTEGER NOT NULL DEFAULT 1
             );
             CREATE TABLE IF NOT EXISTS usage_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 word_count INTEGER NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 history_id INTEGER
             );
             CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at);
             CREATE INDEX IF NOT EXISTS idx_history_text ON history(text);
             CREATE INDEX IF NOT EXISTS idx_usage_created ON usage_events(created_at);",
        )?;
        let store = Self { conn: Mutex::new(conn) };
        let _ = store.migrate();
        let _ = store.seed_default_profiles();
        Ok(store)
    }

    fn migrate(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        // recording_path for local Remake cache (nullable)
        let _ = conn.execute(
            "ALTER TABLE history ADD COLUMN recording_path TEXT",
            [],
        );
        // Append-only usage ledger — deletes must never refund quota.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS usage_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 word_count INTEGER NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 history_id INTEGER
             );
             CREATE INDEX IF NOT EXISTS idx_usage_created ON usage_events(created_at);",
        )?;

        // One-time backfill from existing history so this week isn't under-counted.
        let already: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage_events",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if already == 0 {
            let rows: Vec<(i64, String, String)> = {
                let mut stmt =
                    conn.prepare("SELECT id, text, created_at FROM history")?;
                let mapped = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                mapped.filter_map(|r| r.ok()).collect()
            };
            for (id, text, created_at) in rows {
                let word_count = text.split_whitespace().count() as i64;
                conn.execute(
                    "INSERT INTO usage_events (word_count, created_at, history_id) VALUES (?1, ?2, ?3)",
                    params![word_count, created_at, id],
                )?;
            }
        }
        Ok(())
    }

    pub fn is_onboarded(&self) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM meta WHERE key = 'onboarded'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map(|v| v == "true")
        .unwrap_or(false)
    }

    pub fn set_onboarded(&self, val: bool) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('onboarded', ?1)",
            params![if val { "true" } else { "false" }],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        match conn.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_plan_tier(&self) -> crate::plan::PlanTier {
        self.get_setting("plan_tier")
            .ok()
            .flatten()
            .map(|v| crate::plan::PlanTier::parse(&v))
            .unwrap_or(crate::plan::PlanTier::Free)
    }

    pub fn set_plan_tier(&self, tier: crate::plan::PlanTier) -> Result<(), rusqlite::Error> {
        self.set_setting("plan_tier", tier.as_str())
    }

    /// Sum of billed words since Monday 00:00 UTC from the append-only usage ledger.
    /// Deleting history never reduces this total.
    pub fn words_this_week(&self) -> Result<u64, rusqlite::Error> {
        let week_start = crate::plan::week_starts_at_sql();
        let conn = self.conn.lock().unwrap();
        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM usage_events WHERE created_at >= ?1",
            params![week_start],
            |row| row.get(0),
        )?;
        Ok(total.max(0) as u64)
    }

    pub fn get_plan_status(&self) -> Result<crate::plan::PlanStatus, rusqlite::Error> {
        let tier = self.get_plan_tier();
        let words_used = self.words_this_week()?;
        let week_starts_at = crate::plan::week_starts_at_sql();
        Ok(crate::plan::PlanStatus::from_usage(
            tier,
            words_used,
            week_starts_at,
        ))
    }

    pub fn add_history(&self, text: &str, app_name: &str) -> Result<i64, rusqlite::Error> {
        let word_count = text.split_whitespace().count() as i64;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO history (text, app_name) VALUES (?1, ?2)",
            params![text, app_name],
        )?;
        let id = conn.last_insert_rowid();
        // Bill usage at insert time. History deletes must never remove these rows.
        conn.execute(
            "INSERT INTO usage_events (word_count, history_id) VALUES (?1, ?2)",
            params![word_count, id],
        )?;
        Ok(id)
    }

    pub fn set_history_recording_path(
        &self,
        id: i64,
        path: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE history SET recording_path = ?1 WHERE id = ?2",
            params![path, id],
        )?;
        Ok(())
    }

    /// Keep recordings only for the newest `keep` history rows that have paths; delete the rest.
    pub fn prune_recordings(&self, keep: usize) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, recording_path FROM history WHERE recording_path IS NOT NULL AND recording_path != '' ORDER BY id DESC",
        )?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        for (i, (id, path)) in rows.into_iter().enumerate() {
            if i < keep {
                // Drop stale DB pointers if file missing
                if !std::path::Path::new(&path).is_file() {
                    conn.execute(
                        "UPDATE history SET recording_path = NULL WHERE id = ?1",
                        params![id],
                    )?;
                }
                continue;
            }
            crate::recording::delete_recording_file(&path);
            conn.execute(
                "UPDATE history SET recording_path = NULL WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }

    fn map_history_row(
        id: i64,
        text: String,
        app_name: String,
        created_at: String,
        recording_path: Option<String>,
    ) -> HistoryEntry {
        let path = recording_path.filter(|p| !p.is_empty());
        let can_remake = path
            .as_ref()
            .map(|p| std::path::Path::new(p).is_file())
            .unwrap_or(false);
        HistoryEntry {
            id,
            text,
            app_name,
            created_at,
            recording_path: if can_remake { path } else { None },
            can_remake,
        }
    }

    pub fn get_history(
        &self,
        search: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
        let limit = limit.clamp(1, 200);
        let offset = offset.max(0);
        let conn = self.conn.lock().unwrap();
        if search.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT id, text, app_name, created_at, recording_path FROM history ORDER BY id DESC LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt.query_map(params![limit, offset], |row| {
                Ok(Self::map_history_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?;
            rows.collect()
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, text, app_name, created_at, recording_path FROM history WHERE text LIKE '%' || ?1 || '%' ORDER BY id DESC LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt.query_map(params![search, limit, offset], |row| {
                Ok(Self::map_history_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?;
            rows.collect()
        }
    }

    pub fn get_dictionary(&self) -> Result<Vec<DictWord>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, word, boost FROM dictionary ORDER BY word")?;
        let rows = stmt.query_map([], |row| {
            Ok(DictWord {
                id: row.get(0)?,
                word: row.get(1)?,
                boost: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn add_dict_word(&self, word: &str, boost: f32) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO dictionary (word, boost) VALUES (?1, ?2)",
            params![word, boost],
        )?;
        Ok(())
    }

    pub fn delete_dict_word(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM dictionary WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_macros(&self) -> Result<Vec<Macro>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, trigger, expansion FROM macros ORDER BY trigger")?;
        let rows = stmt.query_map([], |row| {
            Ok(Macro {
                id: row.get(0)?,
                trigger: row.get(1)?,
                expansion: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn add_macro(&self, trigger: &str, expansion: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO macros (trigger, expansion) VALUES (?1, ?2)",
            params![trigger, expansion],
        )?;
        Ok(())
    }

    pub fn delete_macro(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM macros WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_app_profiles(&self) -> Result<Vec<AppProfile>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, exe_pattern, title_pattern, tone, enabled FROM app_profiles ORDER BY exe_pattern",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AppProfile {
                id: row.get(0)?,
                exe_pattern: row.get(1)?,
                title_pattern: row.get(2)?,
                tone: row.get(3)?,
                enabled: row.get::<_, i32>(4)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn update_app_profile(
        &self,
        id: i64,
        tone: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        if let Some(t) = tone {
            conn.execute(
                "UPDATE app_profiles SET tone = ?1 WHERE id = ?2",
                params![t, id],
            )?;
        }
        if let Some(e) = enabled {
            conn.execute(
                "UPDATE app_profiles SET enabled = ?1 WHERE id = ?2",
                params![if e { 1 } else { 0 }, id],
            )?;
        }
        Ok(())
    }

    pub fn get_stats(&self) -> Result<Stats, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let total_entries: i64 = conn.query_row(
            "SELECT COUNT(*) FROM history",
            [],
            |row| row.get(0),
        )?;
        let texts: Vec<String> = {
            let mut stmt = conn.prepare("SELECT text FROM history")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        let total_words: i64 = texts
            .iter()
            .map(|t| t.split_whitespace().count() as i64)
            .sum();
        let days_active: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT date(created_at)) FROM history",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(Stats {
            total_words,
            total_entries,
            days_active,
            avg_wpm: if total_entries > 0 {
                // rough estimate assuming ~3s per dictation average
                ((total_words as f64) / (total_entries as f64 * 0.05)).round() as i64
            } else {
                0
            },
        })
    }

    pub fn get_history_by_id(&self, id: i64) -> Result<Option<HistoryEntry>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        match conn.query_row(
            "SELECT id, text, app_name, created_at, recording_path FROM history WHERE id = ?1",
            params![id],
            |row| {
                Ok(Self::map_history_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        ) {
            Ok(e) => Ok(Some(e)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn update_history_text(&self, id: i64, text: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE history SET text = ?1 WHERE id = ?2",
            params![text, id],
        )?;
        Ok(())
    }

    pub fn delete_history(&self, id: i64) -> Result<(), rusqlite::Error> {
        let path: Option<String> = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT recording_path FROM history WHERE id = ?1",
                params![id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
            .filter(|p| !p.is_empty())
        };
        if let Some(path) = path {
            crate::recording::delete_recording_file(&path);
        }
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_history_many(&self, ids: &[i64]) -> Result<usize, rusqlite::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut deleted = 0usize;
        for id in ids {
            self.delete_history(*id)?;
            deleted += 1;
        }
        Ok(deleted)
    }

    pub fn clear_all_history(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT recording_path FROM history WHERE recording_path IS NOT NULL AND recording_path != ''",
        )?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        for path in paths {
            crate::recording::delete_recording_file(&path);
        }
        conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Reset local "signed in" / session meta so the app returns to a clean state.
    pub fn clear_session_meta(&self) -> Result<(), rusqlite::Error> {
        self.set_onboarded(false)?;
        let conn = self.conn.lock().unwrap();
        // Drop preference keys that imply a configured local account/session.
        for key in [
            "user_name",
            "plan_tier",
            "deepgram_api_key",
            "llm_api_key",
            "api_key",
        ] {
            let _ = conn.execute("DELETE FROM meta WHERE key = ?1", params![key]);
        }
        Ok(())
    }

    pub fn seed_default_profiles(&self) -> Result<(), rusqlite::Error> {
        let defaults: &[(&str, &str, &str)] = &[
            ("slack.exe", "", "casual"),
            ("discord.exe", "", "casual"),
            ("outlook.exe", "", "formal"),
            ("chrome.exe", "Gmail", "formal"),
            ("code.exe", "", "code"),
            ("cursor.exe", "", "code"),
            ("winword.exe", "", "prose"),
            ("notion.exe", "", "prose"),
        ];
        let conn = self.conn.lock().unwrap();
        for &(exe, title, tone) in defaults {
            conn.execute(
                "INSERT OR IGNORE INTO app_profiles (exe_pattern, title_pattern, tone) VALUES (?1, ?2, ?3)",
                params![exe, title, tone],
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct Stats {
    pub total_words: i64,
    pub total_entries: i64,
    pub days_active: i64,
    pub avg_wpm: i64,
}

fn dirs_data_dir() -> String {
    data_dir()
}

pub fn data_dir() -> String {
    // Prefer the OS app-data location:
    // Windows: %LOCALAPPDATA%\MaxSpeech
    // macOS: ~/Library/Application Support/MaxSpeech
    // Linux: $XDG_DATA_HOME/MaxSpeech or ~/.local/share/MaxSpeech
    if let Some(base) = dirs::data_dir() {
        return base.join("MaxSpeech").to_string_lossy().into_owned();
    }
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        return std::path::Path::new(&d)
            .join("MaxSpeech")
            .to_string_lossy()
            .into_owned();
    }
    "maxspeech_data".to_string()
}
