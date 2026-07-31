use rusqlite::{Connection, params};
use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Serialize, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub app_name: String,
    pub created_at: String,
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
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
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
             CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at);
             CREATE INDEX IF NOT EXISTS idx_history_text ON history(text);",
        )?;
        let store = Self { conn: Mutex::new(conn) };
        let _ = store.seed_default_profiles();
        Ok(store)
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

    /// Sum of whitespace-separated words in history since Monday 00:00 UTC.
    pub fn words_this_week(&self) -> Result<u64, rusqlite::Error> {
        let week_start = crate::plan::week_starts_at_sql();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT text FROM history WHERE created_at >= ?1")?;
        let texts = stmt.query_map(params![week_start], |row| row.get::<_, String>(0))?;
        let mut total = 0u64;
        for text in texts.filter_map(|r| r.ok()) {
            total += text.split_whitespace().count() as u64;
        }
        Ok(total)
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO history (text, app_name) VALUES (?1, ?2)",
            params![text, app_name],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_history(&self, search: &str) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        if search.is_empty() {
            let mut stmt = conn.prepare("SELECT id, text, app_name, created_at FROM history ORDER BY id DESC LIMIT 100")?;
            let rows = stmt.query_map([], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    app_name: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            rows.collect()
        } else {
            let mut stmt = conn.prepare("SELECT id, text, app_name, created_at FROM history WHERE text LIKE '%' || ?1 || '%' ORDER BY id DESC LIMIT 100")?;
            let rows = stmt.query_map(params![search], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    app_name: row.get(2)?,
                    created_at: row.get(3)?,
                })
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
            "SELECT id, text, app_name, created_at FROM history WHERE id = ?1",
            params![id],
            |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    app_name: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        ) {
            Ok(e) => Ok(Some(e)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete_history(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_history_many(&self, ids: &[i64]) -> Result<usize, rusqlite::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().unwrap();
        let mut deleted = 0usize;
        for id in ids {
            deleted += conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
        }
        Ok(deleted)
    }

    pub fn clear_all_history(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
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
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        let p = std::path::Path::new(&d).join("MaxSpeech");
        return p.to_string_lossy().into_owned();
    }
    "maxspeech_data".to_string()
}
