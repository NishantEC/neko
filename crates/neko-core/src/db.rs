//! The single SQLite writer. Only `neko-daemon` opens this; the client
//! never does (see `AGENTS.md`).

use std::path::Path;

use rusqlite::Connection;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let db = Self {
            conn: Connection::open_in_memory()?,
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS launches (
                app_id           TEXT PRIMARY KEY,
                last_launched_at INTEGER NOT NULL,
                launch_count     INTEGER NOT NULL DEFAULT 0
            );
            ",
        )
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(e),
            })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }

    pub fn record_launch(&self, app_id: &str, at_unix_ms: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO launches (app_id, last_launched_at, launch_count) VALUES (?1, ?2, 1)
             ON CONFLICT(app_id) DO UPDATE SET
                last_launched_at = excluded.last_launched_at,
                launch_count = launch_count + 1",
            (app_id, at_unix_ms),
        )?;
        Ok(())
    }

    /// `(last_launched_at_unix_ms, launch_count)` per app id, for ranking.
    pub fn recency(&self) -> rusqlite::Result<std::collections::HashMap<String, (i64, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT app_id, last_launched_at, launch_count FROM launches")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?)))
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_setting("hotkey").unwrap(), None);
        db.set_setting("hotkey", "alt+space").unwrap();
        assert_eq!(db.get_setting("hotkey").unwrap().as_deref(), Some("alt+space"));
        db.set_setting("hotkey", "cmd+shift+space").unwrap();
        assert_eq!(
            db.get_setting("hotkey").unwrap().as_deref(),
            Some("cmd+shift+space")
        );
    }

    #[test]
    fn recording_a_launch_twice_increments_count_and_updates_recency() {
        let db = Db::open_in_memory().unwrap();
        db.record_launch("com.apple.Safari", 100).unwrap();
        db.record_launch("com.apple.Safari", 200).unwrap();
        let recency = db.recency().unwrap();
        assert_eq!(recency["com.apple.Safari"], (200, 2));
    }
}
