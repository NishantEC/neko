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
            CREATE TABLE IF NOT EXISTS clipboard_entries (
                content           TEXT PRIMARY KEY,
                content_kind      TEXT NOT NULL,
                source_app        TEXT,
                copied_at_unix_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_clipboard_entries_copied_at
                ON clipboard_entries (copied_at_unix_ms);
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

    /// Insert or, on a repeat copy of the same content, move the existing
    /// row to the top by bumping its timestamp (`content` is the primary
    /// key, so a second copy of identical text is a dedup, not a new row —
    /// see the launch brief's "copying the same thing twice should not
    /// create two entries" requirement). Prunes down to `history_limit`
    /// rows afterward, oldest-copied first, so the table is bounded
    /// regardless of how bursty copying is.
    pub fn record_clipboard_entry(
        &self,
        content: &str,
        content_kind: &str,
        source_app: Option<&str>,
        copied_at_unix_ms: i64,
        history_limit: usize,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO clipboard_entries (content, content_kind, source_app, copied_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(content) DO UPDATE SET
                content_kind = excluded.content_kind,
                source_app = excluded.source_app,
                copied_at_unix_ms = excluded.copied_at_unix_ms",
            (content, content_kind, source_app, copied_at_unix_ms),
        )?;
        self.conn.execute(
            "DELETE FROM clipboard_entries
             WHERE content NOT IN (
                 SELECT content FROM clipboard_entries
                 ORDER BY copied_at_unix_ms DESC
                 LIMIT ?1
             )",
            (history_limit as i64,),
        )?;
        Ok(())
    }

    /// Permanently removes one stored entry by its own content (the primary
    /// key) — the clipboard actions menu's "Delete" action
    /// (`ClipboardProvider::perform_action`). A content string that isn't
    /// actually stored (already deleted, e.g. a mis-keyed double-delete) is
    /// not an error: the end state ("this content is not in history") is
    /// already what the caller wanted.
    pub fn delete_clipboard_entry(&self, content: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM clipboard_entries WHERE content = ?1", [content])?;
        Ok(())
    }

    /// `(content, content_kind, source_app, copied_at_unix_ms)` per entry,
    /// most recently copied first.
    #[allow(clippy::type_complexity)]
    pub fn clipboard_entries(&self) -> rusqlite::Result<Vec<(String, String, Option<String>, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT content, content_kind, source_app, copied_at_unix_ms
             FROM clipboard_entries
             ORDER BY copied_at_unix_ms DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
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

    #[test]
    fn copying_the_same_content_twice_moves_it_to_top_instead_of_duplicating() {
        let db = Db::open_in_memory().unwrap();
        db.record_clipboard_entry("hello", "text", Some("Terminal"), 100, 200)
            .unwrap();
        db.record_clipboard_entry("hello", "text", Some("Notes"), 200, 200)
            .unwrap();
        let entries = db.clipboard_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            ("hello".to_string(), "text".to_string(), Some("Notes".to_string()), 200)
        );
    }

    #[test]
    fn deleting_a_clipboard_entry_removes_only_that_one() {
        let db = Db::open_in_memory().unwrap();
        db.record_clipboard_entry("keep", "text", None, 100, 200).unwrap();
        db.record_clipboard_entry("drop", "text", None, 200, 200).unwrap();
        db.delete_clipboard_entry("drop").unwrap();
        let entries = db.clipboard_entries().unwrap();
        let contents: Vec<&str> = entries.iter().map(|(c, ..)| c.as_str()).collect();
        assert_eq!(contents, vec!["keep"]);
    }

    #[test]
    fn deleting_a_content_that_was_never_stored_is_not_an_error() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.delete_clipboard_entry("never-stored").is_ok());
    }

    #[test]
    fn history_is_pruned_to_the_configured_limit_oldest_first() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..5 {
            db.record_clipboard_entry(&format!("entry-{i}"), "text", None, i, 3)
                .unwrap();
        }
        let entries = db.clipboard_entries().unwrap();
        assert_eq!(entries.len(), 3);
        let contents: Vec<&str> = entries.iter().map(|(c, ..)| c.as_str()).collect();
        assert_eq!(contents, vec!["entry-4", "entry-3", "entry-2"]);
    }
}
