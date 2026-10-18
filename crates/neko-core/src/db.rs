//! The single SQLite writer. Only `neko-daemon` opens this; the client
//! never does (see `AGENTS.md`).

use std::path::Path;

use rusqlite::Connection;

pub struct Db {
    conn: Connection,
    clipboard_revision: std::cell::Cell<u64>,
    /// Settings writes cost a whole value: the workbench snapshot is one
    /// multi-megabyte JSON row, and SQLite rewrites every page of it (plus
    /// the WAL copy) even when the new value is identical. Measured
    /// 2026-10-18: neko-daemon wrote 43 GB in 12 hours, almost all of it a
    /// 2-second scheduler tick re-saving an unchanged 4.7 MB snapshot.
    /// `written` remembers what each key holds on disk so an identical
    /// write is skipped; `pending` holds coalesced writes (agent progress
    /// events) that reach disk at most once per `DEFERRED_WRITE_DELAY`.
    written: std::cell::RefCell<std::collections::HashMap<String, String>>,
    pending: std::cell::RefCell<std::collections::HashMap<String, (String, std::time::Instant)>>,
    in_transaction: std::cell::Cell<bool>,
    setting_writes: std::cell::Cell<u64>,
}

/// How long a deferred settings write may wait before it reaches disk.
/// Only progress that a crash may lose goes through this path (agent tool
/// events); user actions and status changes write immediately.
pub const DEFERRED_WRITE_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

impl Drop for Db {
    fn drop(&mut self) {
        if let Err(error) = self.flush_deferred() {
            eprintln!("neko: could not save deferred settings on close: {error}");
        }
    }
}

impl Db {
    /// Commit related settings together. Dropping the transaction rolls back
    /// every write when the operation or commit fails.
    pub fn atomic<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let transaction = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        // A write in this transaction supersedes deferred progress; if it
        // rolls back, that progress must still be waiting to be written.
        let deferred = self.pending.borrow().clone();
        self.in_transaction.set(true);
        let result = operation().and_then(|value| {
            transaction.commit().map_err(|e| e.to_string())?;
            Ok(value)
        });
        self.in_transaction.set(false);
        if result.is_err() {
            // Rolled back: what was remembered as written may not be on disk.
            self.written.borrow_mut().clear();
            *self.pending.borrow_mut() = deferred;
        }
        result
    }

    fn with_conn(conn: Connection) -> Self {
        Self {
            conn,
            clipboard_revision: std::cell::Cell::new(0),
            written: Default::default(),
            pending: Default::default(),
            in_transaction: std::cell::Cell::new(false),
            setting_writes: std::cell::Cell::new(0),
        }
    }

    /// Settings rows actually written to SQLite (identical and coalesced
    /// writes are not counted). For tests and diagnostics.
    pub fn setting_writes(&self) -> u64 {
        self.setting_writes.get()
    }

    /// Store `value` for `key`, reaching disk within `DEFERRED_WRITE_DELAY`
    /// (or sooner, with the next immediate write of a changed value for the
    /// same key, `flush_deferred` or closing the database). Reads see it at
    /// once. A crash may lose it, so use this only for progress.
    pub fn set_setting_deferred(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        if self.in_transaction.get() {
            return self.set_setting(key, value);
        }
        if self.written.borrow().get(key).is_some_and(|v| v == value) {
            self.pending.borrow_mut().remove(key);
            return Ok(());
        }
        let since = self
            .pending
            .borrow()
            .get(key)
            .map_or_else(std::time::Instant::now, |(_, since)| *since);
        self.pending
            .borrow_mut()
            .insert(key.to_owned(), (value.to_owned(), since));
        self.flush_due(DEFERRED_WRITE_DELAY)
    }

    /// Write deferred values that have waited at least `delay`.
    pub fn flush_due(&self, delay: std::time::Duration) -> rusqlite::Result<()> {
        let due: Vec<String> = self
            .pending
            .borrow()
            .iter()
            .filter(|(_, (_, since))| since.elapsed() >= delay)
            .map(|(key, _)| key.clone())
            .collect();
        for key in due {
            let pending = self.pending.borrow_mut().remove(&key);
            if let Some((value, since)) = pending
                && let Err(error) = self.set_setting(&key, &value)
            {
                self.pending.borrow_mut().insert(key, (value, since));
                return Err(error);
            }
        }
        Ok(())
    }

    /// Write every deferred value now.
    pub fn flush_deferred(&self) -> rusqlite::Result<()> {
        self.flush_due(std::time::Duration::ZERO)
    }
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Self::with_conn(conn);
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let db = Self::with_conn(Connection::open_in_memory()?);
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
        if let Some((value, _)) = self.pending.borrow().get(key) {
            return Ok(Some(value.clone()));
        }
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(e),
            })
    }

    /// Writes now, unless the stored value is already identical. A deferred
    /// value for the same key is superseded by this one; if it equals this
    /// one, the caller changed nothing on top of it and it stays deferred.
    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        if self.pending.borrow().get(key).is_some_and(|(v, _)| v == value) {
            return Ok(());
        }
        let superseded = self.pending.borrow_mut().remove(key);
        if self.written.borrow().get(key).is_some_and(|v| v == value) {
            return Ok(());
        }
        if let Err(error) = self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        ) {
            if let Some(pending) = superseded {
                self.pending.borrow_mut().insert(key.to_owned(), pending);
            }
            self.written.borrow_mut().remove(key);
            return Err(error);
        }
        self.setting_writes.set(self.setting_writes.get() + 1);
        self.written
            .borrow_mut()
            .insert(key.to_owned(), value.to_owned());
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
        // Invalidate even if pruning fails after the successful insert.
        self.clipboard_revision
            .set(self.clipboard_revision.get().wrapping_add(1));
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
        self.conn.execute(
            "DELETE FROM clipboard_entries WHERE content = ?1",
            [content],
        )?;
        self.clipboard_revision
            .set(self.clipboard_revision.get().wrapping_add(1));
        Ok(())
    }

    /// This daemon owns the only writer; unrelated settings do not invalidate search.
    pub fn clipboard_revision(&self) -> u64 {
        self.clipboard_revision.get()
    }

    /// `(content, content_kind, source_app, copied_at_unix_ms)` per entry,
    /// most recently copied first.
    /// **`content` is truncated to `max_content_bytes` by SQLite itself.**
    ///
    /// Measured on this machine: 200 entries totalling 16.9 MB, with a
    /// single entry of **17 MB** — and every keystroke read all of it and
    /// ran `fuzzy_score` over the lot. A burst of thirty one-character
    /// searches took the daemon's RSS from 14 MB to 1.6 GB. It is transient
    /// rather than a leak (the allocator gives it back), but a launcher that
    /// allocates tens of megabytes per keypress is a launcher that stutters.
    ///
    /// `substr` happens in SQLite so the bytes never enter this process at
    /// all. A match beyond the bound cannot change an outcome:
    /// `clipboard::CLIPBOARD_TITLE_LIKE_CHARS` already scales a long entry's
    /// score down toward nothing, and the row only ever shows
    /// `clipboard::preview`'s first line.
    #[allow(clippy::type_complexity)]
    pub fn clipboard_entries(
        &self,
        max_content_bytes: usize,
    ) -> rusqlite::Result<Vec<(String, String, Option<String>, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT substr(content, 1, ?1), content_kind, source_app, copied_at_unix_ms
             FROM clipboard_entries
             ORDER BY copied_at_unix_ms DESC",
        )?;
        let rows = stmt.query_map([max_content_bytes as i64], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_reload_failure_never_returns_a_deleted_cached_entry() {
        use crate::provider::Provider;
        use std::sync::{Arc, Mutex};
        let db = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
        db.lock()
            .unwrap()
            .record_clipboard_entry("private", "text", None, 1, 200)
            .unwrap();
        let provider = crate::clipboard::ClipboardProvider::new(Arc::clone(&db));
        assert_eq!(provider.search("private", 2).len(), 1);
        {
            let db = db.lock().unwrap();
            db.delete_clipboard_entry("private").unwrap();
            db.conn
                .execute_batch("DROP TABLE clipboard_entries")
                .unwrap();
        }
        assert!(provider.search("private", 3).is_empty());
        assert!(provider.search("private", 4).is_empty());
        db.lock().unwrap().migrate().unwrap();
        db.lock()
            .unwrap()
            .record_clipboard_entry("new", "text", None, 5, 200)
            .unwrap();
        assert_eq!(provider.search("new", 6).len(), 1);
    }

    #[test]
    fn settings_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_setting("hotkey").unwrap(), None);
        db.set_setting("hotkey", "alt+space").unwrap();
        assert_eq!(
            db.get_setting("hotkey").unwrap().as_deref(),
            Some("alt+space")
        );
        db.set_setting("hotkey", "cmd+shift+space").unwrap();
        assert_eq!(
            db.get_setting("hotkey").unwrap().as_deref(),
            Some("cmd+shift+space")
        );
    }

    #[test]
    fn identical_settings_are_not_rewritten_and_deferred_ones_coalesce() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("blob", "a").unwrap();
        for _ in 0..100 {
            db.set_setting("blob", "a").unwrap();
        }
        assert_eq!(db.setting_writes(), 1, "an unchanged value is never rewritten");
        for n in 0..100 {
            db.set_setting_deferred("blob", &format!("progress {n}")).unwrap();
            // A no-op save on top of deferred progress keeps it deferred.
            db.set_setting("blob", &format!("progress {n}")).unwrap();
        }
        assert_eq!(db.setting_writes(), 1, "progress waits for its delay");
        assert_eq!(db.get_setting("blob").unwrap().as_deref(), Some("progress 99"));
        db.flush_due(std::time::Duration::ZERO).unwrap();
        assert_eq!(db.setting_writes(), 2);
        // An immediate change writes now and supersedes pending progress.
        db.set_setting_deferred("blob", "later").unwrap();
        db.set_setting("blob", "decision").unwrap();
        db.flush_deferred().unwrap();
        assert_eq!(db.setting_writes(), 3);
        assert_eq!(db.get_setting("blob").unwrap().as_deref(), Some("decision"));
    }

    #[test]
    fn a_rolled_back_write_is_not_remembered_and_progress_survives_it() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("blob", "before").unwrap();
        db.set_setting_deferred("blob", "progress").unwrap();
        let failed: Result<(), String> = db.atomic(|| {
            db.set_setting("blob", "rolled back").unwrap();
            Err("boom".into())
        });
        assert!(failed.is_err());
        // The deferred progress is still waiting, and still reaches disk.
        assert_eq!(db.get_setting("blob").unwrap().as_deref(), Some("progress"));
        db.flush_deferred().unwrap();
        assert_eq!(db.get_setting("blob").unwrap().as_deref(), Some("progress"));
        db.set_setting("blob", "rolled back").unwrap();
        assert_eq!(db.get_setting("blob").unwrap().as_deref(), Some("rolled back"));
    }

    #[test]
    fn deferred_settings_reach_disk_when_the_database_closes() {
        let dir = std::env::temp_dir().join(format!("neko-db-defer-{}", std::process::id()));
        let path = dir.join("neko.db");
        let _ = std::fs::remove_dir_all(&dir);
        {
            let db = Db::open(&path).unwrap();
            db.set_setting_deferred("blob", "progress").unwrap();
        }
        assert_eq!(Db::open(&path).unwrap().get_setting("blob").unwrap().as_deref(), Some("progress"));
        let _ = std::fs::remove_dir_all(&dir);
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
        let entries = db
            .clipboard_entries(crate::clipboard::MAX_MATCHED_BYTES)
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            (
                "hello".to_string(),
                "text".to_string(),
                Some("Notes".to_string()),
                200
            )
        );
    }

    #[test]
    fn deleting_a_clipboard_entry_removes_only_that_one() {
        let db = Db::open_in_memory().unwrap();
        db.record_clipboard_entry("keep", "text", None, 100, 200)
            .unwrap();
        db.record_clipboard_entry("drop", "text", None, 200, 200)
            .unwrap();
        db.delete_clipboard_entry("drop").unwrap();
        let entries = db
            .clipboard_entries(crate::clipboard::MAX_MATCHED_BYTES)
            .unwrap();
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
        let entries = db
            .clipboard_entries(crate::clipboard::MAX_MATCHED_BYTES)
            .unwrap();
        assert_eq!(entries.len(), 3);
        let contents: Vec<&str> = entries.iter().map(|(c, ..)| c.as_str()).collect();
        assert_eq!(contents, vec!["entry-4", "entry-3", "entry-2"]);
    }

    #[test]
    fn a_huge_entry_is_truncated_by_sqlite_before_it_reaches_this_process() {
        // Measured on a real machine: one 17 MB clipboard entry, read in
        // full on every keystroke, taking the daemon's RSS from 14 MB to
        // 1.6 GB across thirty one-character searches.
        let db = Db::open_in_memory().unwrap();
        let huge = "x".repeat(200_000);
        db.record_clipboard_entry(&huge, "text", None, 100, 200)
            .unwrap();
        let entries = db.clipboard_entries(1024).unwrap();
        assert_eq!(
            entries[0].0.len(),
            1024,
            "SQLite truncates, not this process"
        );
        // And a short entry is untouched by the bound.
        db.record_clipboard_entry("short", "text", None, 200, 200)
            .unwrap();
        let entries = db.clipboard_entries(1024).unwrap();
        assert!(entries.iter().any(|e| e.0 == "short"));
    }
}
