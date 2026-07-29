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
        Ok(Self { conn: Mutex::new(conn) })
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

fn dirs_data_dir() -> String {
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        let p = std::path::Path::new(&d).join("MaxSpeech");
        return p.to_string_lossy().into_owned();
    }
    "maxspeech_data".to_string()
}
