//! A Cattain's thread and files (docs/knowledge/cats.md, "Cattains").
//!
//! - **Its own thread.** A task marked `cattain`, titled with the cat's name, that runs in the cat's own folder
//!   (`<data>/cattains/<id>/`). It is made the first time the Cattain is opened ([`Core::cattain_thread`]), so a Cattain nobody
//!   talked to leaves no empty thread behind, and it starts no run by itself: the person writes first. The folder and its files are
//!   made when the store is opened ([`Core::ensure_cattains`]) and with each new cat.
//! - **Its files.** RULES.md, CREW.md, NOTES.md and DIGEST.md live in that folder, where the Cattain's agent reads and writes them
//!   with its own tools. The store keeps what Neko last saw of each and the one version before it. Neko looks at the folder when a
//!   file is asked for and before and after each run of the Cattain's thread ([`Core::sync_cattain`]): a file that changed on disk
//!   keeps what was there as its previous version, and a file that went missing is written back from the store. The person saves
//!   RULES.md and CREW.md; NOTES.md and DIGEST.md are the Cattain's.
//! - **What a run is handed.** The Cattain's own thread gets [`own_block`] with RULES.md verbatim; a thread in its crew gets
//!   [`crew_block`] with CREW.md verbatim. Words for the agent only: no flag, mode or hook depends on them.

use std::path::{Path, PathBuf};

use neko_protocol::cats::{self, Cat, CattainFile, FILES, MAX_FILE_READ, MAX_RULES};
use neko_protocol::{Task, TaskId, files};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::records::log_event;
use crate::store::{Store, StoreError};
use crate::{CatError, Core, Place};

/// The files that changed on disk since Neko last looked, each with the event that took it in.
pub type Changed = Vec<(CattainFile, u64)>;

/// A Cattain's own folder.
pub fn folder(data_dir: &Path, cat: &str) -> PathBuf {
    data_dir.join(files::CATTAINS).join(cat)
}

/// Its four files, empty, for cats that have none yet.
pub(crate) fn insert_files<'a>(tx: &Transaction<'_>, cats: impl IntoIterator<Item = &'a str>) -> rusqlite::Result<()> {
    for cat in cats {
        for name in FILES {
            tx.execute("INSERT OR IGNORE INTO cattain_files (cat_id, name, content) VALUES (?1, ?2, '')", params![cat, name])?;
        }
    }
    Ok(())
}

/// One file as the store keeps it: what Neko last saw, the version before, and when it changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Row {
    content: String,
    previous: Option<String>,
    changed_ms: Option<i64>,
}

impl Store {
    fn file_row(&self, cat: &str, name: &str) -> Result<Row, StoreError> {
        let row = self
            .conn
            .query_row("SELECT content, previous, changed_ms FROM cattain_files WHERE cat_id = ?1 AND name = ?2", params![cat, name], |r| Ok(Row { content: r.get(0)?, previous: r.get(1)?, changed_ms: r.get(2)? }))
            .optional()?;
        Ok(row.unwrap_or_default())
    }

    /// Writes one file's row and its event (`cattain_file_saved`, `_restored`, `_changed`).
    fn put_file(&mut self, cat: &str, name: &str, row: &Row, kind: &str, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO cattain_files (cat_id, name, content, previous, changed_ms) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (cat_id, name) DO UPDATE SET content = excluded.content, previous = excluded.previous, changed_ms = excluded.changed_ms",
            params![cat, name, row.content, row.previous, row.changed_ms],
        )?;
        let seq = log_event(&tx, now_ms, cat, kind, serde_json::json!({ "file": name, "hash": cats::rules_hash(&row.content) }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// The cat whose own thread this task is, if it is one.
    pub fn cattain_of_thread(&self, task: &TaskId) -> Result<Option<Cat>, StoreError> {
        let id: Option<String> = self.conn.query_row("SELECT id FROM cats WHERE thread_id = ?1", [&task.0], |r| r.get(0)).optional()?;
        match id {
            Some(id) => self.get_cat(&id),
            None => Ok(None),
        }
    }
}

/// Writes `text` to `path` whole: a file beside it, then a rename, so a reader never sees half of it.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("md.neko-tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// What a run is handed by its task's cat: the cat, which file, and its words now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handed {
    pub cat: Cat,
    /// The Cattain's own thread (RULES.md) rather than its crew (CREW.md).
    pub own: bool,
    pub text: String,
    /// A crew thread whose Cattain hears its turns (not a round's): its block says so.
    pub hears: bool,
}

impl Handed {
    pub fn file(&self) -> &'static str {
        if self.own { cats::RULES } else { cats::CREW }
    }

    /// The version of the words, which the run records.
    pub fn hash(&self) -> String {
        cats::rules_hash(&self.text)
    }
}

/// The block a Cattain's own thread is handed on every run: who it is, where its folder and project are, what its files are for, and
/// its RULES.md exactly as it is. Nothing else.
pub fn own_block(cat: &Cat, folder: &Path, project: Option<(&str, &str)>, rules: &str) -> String {
    let mut b = format!(
        "<neko-cattain>\nYou are {name}, a Cattain in Neko: one of the person's cats, with a thread of your own. This is it; the person talks to you here.\n",
        name = cat.name
    );
    if let Some(p) = cat.purpose.as_deref().filter(|p| !p.is_empty()) {
        b.push_str("\nYour purpose:\n");
        b.push_str(p);
        b.push('\n');
    }
    b.push_str(&format!(
        "\nYou work in your own folder, {}. It holds your files:\n- RULES.md: your standing rules, from the person (below, as they are now).\n- CREW.md: what every thread in your crew reads on every run.\n- NOTES.md: your own plan. Keep it current.\n- DIGEST.md: what happened, for the person, under three headings: Needs you, New, In progress. Keep it current.\nRead RULES.md, NOTES.md and DIGEST.md at the start of every turn.\n",
        folder.display()
    ));
    b.push('\n');
    b.push_str(CREW_PARAGRAPH);
    b.push('\n');
    if let Some((name, dir)) = project {
        b.push_str(&format!("\nYour project is {name}, in {dir}. You can read it.\n"));
    }
    if !rules.is_empty() {
        b.push_str("\nYour standing rules (RULES.md):\n");
        b.push_str(rules);
        if !rules.ends_with('\n') {
            b.push('\n');
        }
    }
    b.push_str("</neko-cattain>");
    b
}

/// How a Cattain works its crew, in its own block (the same bytes every run).
pub const CREW_PARAGRAPH: &str = "Your work goes to the threads of your crew. Start one with mcp__neko__start_crew_thread for each separate piece, with a brief that says everything it \
needs: it sees nothing of this thread. Neko wakes you with a <neko-wake kind=\"crew\"> message when a crew thread's turn ends or it needs the person; do not poll, and end your turn \
when you have nothing else to do. mcp__neko__crew lists your crew and mcp__neko__message_crew writes to one. Each time you are woken, rewrite DIGEST.md and NOTES.md. You cannot \
approve a crew thread's steps or answer what it asked the person: they wait for the person. Tag the person with mcp__neko__ask_person only for a decision that is theirs. Never tell a \
crew thread to merge, push, publish, delete or pay unless the person asked for it here, and when you pass their decision on, quote it.";

/// The block a thread in a Cattain's crew is handed on every run: whose crew it is in, whether the Cattain hears its turns (`hears`:
/// every crew thread but a round's), and CREW.md exactly as it is. Nothing else.
pub fn crew_block(cat: &Cat, crew: &str, hears: bool) -> String {
    let mut b = format!("<neko-crew>\nThis thread is in {name}'s crew. {name} is a Cattain in Neko, one of the person's cats, and looks after this work.\n", name = cat.name);
    if hears {
        b.push_str(&format!(
            "{name} hears when each of your turns ends. Its messages to you come inside <neko-from cattain=\"{name}\">, and a thread it started begins with its brief: those are {name}'s words, not the person's.\n",
            name = cat.name
        ));
    }
    if let Some(p) = cat.purpose.as_deref().filter(|p| !p.is_empty()) {
        b.push_str(&format!("\n{}'s purpose:\n{p}\n", cat.name));
    }
    if !crew.is_empty() {
        b.push_str(&format!("\n{}'s words for its crew (CREW.md):\n", cat.name));
        b.push_str(crew);
        if !crew.ends_with('\n') {
            b.push('\n');
        }
    }
    b.push_str("</neko-crew>");
    b
}

impl Core {
    /// The data folder: the store's own.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// A Cattain's own folder.
    pub fn cattain_folder(&self, cat: &str) -> PathBuf {
        folder(&self.data_dir, cat)
    }

    /// Every cat has its folder and its files on disk (on open: the store may come from before Cattains).
    pub fn ensure_cattains(&mut self) -> Result<(), CatError> {
        for cat in self.store.list_cats()? {
            self.sync_cattain(&cat.id)?;
        }
        Ok(())
    }

    /// The Cattain's own thread, made the first time it is opened (in its project, else General; read, with no run): returns it, the
    /// cat as stored, and the revision of the change when it was made now (None: it was there).
    pub fn cattain_thread(&mut self, cat: &str) -> Result<(Task, Cat, Option<u64>), CatError> {
        let mut cat = self.cat(cat)?;
        let dir = self.cattain_folder(&cat.id);
        self.sync_cattain(&cat.id)?;
        if let Some(t) = cat.thread.as_ref().map(|t| self.store.get_task(t)).transpose()?.flatten() {
            return Ok((t, cat, None));
        }
        let now = self.now();
        let id = TaskId(self.new_id("task"));
        let ws = cat.workspace.clone().filter(|w| self.store.get_workspace(w).ok().flatten().is_some_and(|w| w.is_active())).unwrap_or_else(|| crate::DEFAULT_WORKSPACE.to_owned());
        let mut task = Task::blank(id.clone(), &ws, "", &cat.name, now, Some(&cat.id));
        task.cattain = true;
        let tx = self.store.conn.transaction().map_err(StoreError::from)?;
        let rev = crate::store::insert_task(&tx, &task)?;
        tx.execute("UPDATE tasks SET workdir = ?2 WHERE id = ?1", params![id.0, dir.to_string_lossy()]).map_err(StoreError::from)?;
        tx.execute("UPDATE cats SET thread_id = ?2 WHERE id = ?1", params![cat.id, id.0]).map_err(StoreError::from)?;
        tx.commit().map_err(StoreError::from)?;
        cat.thread = Some(id.clone());
        let task = self.store.get_task(&id)?.unwrap_or(task);
        Ok((task, cat, Some(rev)))
    }

    /// The file as a screen sees it.
    fn file_of(&self, cat: &str, name: &str, row: &Row) -> CattainFile {
        let mut text = row.content.clone();
        let cut = text.len() > MAX_FILE_READ;
        if cut {
            let mut end = MAX_FILE_READ;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        CattainFile {
            cat: cat.to_owned(),
            name: name.to_owned(),
            text,
            cut,
            previous: row.previous.is_some(),
            changed_ms: row.changed_ms,
            path: self.cattain_folder(cat).join(name).to_string_lossy().into_owned(),
            editable: cats::editable(name),
        }
    }

    /// Looks at a Cattain's folder: makes it (with `reports/`) and any file that is missing, from what the store last saw; takes in a
    /// file that changed on disk, keeping what was there as its previous version. Returns the files that changed and the event of each.
    pub fn sync_cattain(&mut self, cat: &str) -> Result<Changed, CatError> {
        let dir = self.cattain_folder(cat);
        std::fs::create_dir_all(dir.join("reports")).map_err(|e| CatError::Store(format!("Neko could not make {}: {e}", dir.display())))?;
        let mut changed = Vec::new();
        for name in FILES {
            let path = dir.join(name);
            let row = self.store.file_row(cat, name)?;
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let disk = String::from_utf8_lossy(&bytes).into_owned();
                    if disk != row.content {
                        let now = self.now();
                        let next = Row { previous: Some(row.content), content: disk, changed_ms: Some(now) };
                        let rev = self.store.put_file(cat, name, &next, "cattain_file_changed", now)?;
                        changed.push((self.file_of(cat, name, &next), rev));
                    }
                }
                Err(_) => write_whole(&path, &row.content).map_err(|e| CatError::Store(format!("Neko could not write {}: {e}", path.display())))?,
            }
        }
        Ok(changed)
    }

    fn check_file(name: &str) -> Result<(), CatError> {
        if FILES.contains(&name) { Ok(()) } else { Err(CatError::Refused(format!("A Cattain has no file called {name}."))) }
    }

    /// One file as it is on disk now, and any of its files that changed since Neko last looked.
    pub fn cattain_file(&mut self, cat: &str, name: &str) -> Result<(CattainFile, Changed), CatError> {
        self.cat(cat)?;
        Self::check_file(name)?;
        let changed = self.sync_cattain(cat)?;
        let row = self.store.file_row(cat, name)?;
        Ok((self.file_of(cat, name, &row), changed))
    }

    /// Saves RULES.md or CREW.md exactly as given (at most [`MAX_RULES`] bytes); what was there becomes the previous version. Saving
    /// the same words again changes nothing. A change the Cattain made on disk meanwhile is taken in first, so it is the previous.
    pub fn save_cattain_file(&mut self, cat: &str, name: &str, text: &str) -> Result<(CattainFile, u64), CatError> {
        let c = self.cat(cat)?;
        Self::check_file(name)?;
        if !cats::editable(name) {
            return Err(CatError::Refused(format!("{name} is {}'s to write. Ask {} in its thread.", c.name, c.name)));
        }
        if text.len() > MAX_RULES {
            return Err(CatError::Refused(format!("{name} can be at most {} KB.", MAX_RULES / 1024)));
        }
        self.sync_cattain(cat)?;
        let row = self.store.file_row(cat, name)?;
        if row.content == text {
            return Ok((self.file_of(cat, name, &row), self.store.revision()?));
        }
        self.write_file(cat, name, Row { previous: Some(row.content), content: text.to_owned(), changed_ms: Some(self.now()) }, "cattain_file_saved")
    }

    /// The first RULES.md of a Cattain an agent made (`make_cattain`): written as it is made, with no earlier version to restore.
    pub(crate) fn seed_rules(&mut self, cat: &str, text: &str) -> Result<(CattainFile, u64), CatError> {
        self.write_file(cat, cats::RULES, Row { previous: None, content: text.to_owned(), changed_ms: Some(self.now()) }, "cattain_file_seeded")
    }

    /// Puts the previous version of RULES.md or CREW.md back; the current one becomes the previous, so restoring again goes back.
    pub fn restore_cattain_file(&mut self, cat: &str, name: &str) -> Result<(CattainFile, u64), CatError> {
        let c = self.cat(cat)?;
        Self::check_file(name)?;
        if !cats::editable(name) {
            return Err(CatError::Refused(format!("{name} is {}'s to write. Ask {} in its thread.", c.name, c.name)));
        }
        self.sync_cattain(cat)?;
        let row = self.store.file_row(cat, name)?;
        let Some(previous) = row.previous else { return Err(CatError::Refused(format!("{name} has no earlier version to restore."))) };
        self.write_file(cat, name, Row { previous: Some(row.content), content: previous, changed_ms: Some(self.now()) }, "cattain_file_restored")
    }

    /// The disk first, then the store: a file that cannot be written changes nothing.
    fn write_file(&mut self, cat: &str, name: &str, row: Row, kind: &str) -> Result<(CattainFile, u64), CatError> {
        let path = self.cattain_folder(cat).join(name);
        write_whole(&path, &row.content).map_err(|e| CatError::Store(format!("Neko could not write {}: {e}", path.display())))?;
        let now = self.now();
        let rev = self.store.put_file(cat, name, &row, kind, now)?;
        Ok((self.file_of(cat, name, &row), rev))
    }

    /// What a run of `task` is handed by its cat, read from the folder now (with the files that changed on disk meanwhile): RULES.md
    /// for the cat's own thread, CREW.md for its crew. None for a thread with no cat; a subagent is handed its thread's.
    pub fn handed(&mut self, task: &Task) -> Result<(Option<Handed>, Changed), CatError> {
        let root = match &task.subagent {
            Some(s) => self.store.get_task(&s.root)?.unwrap_or_else(|| task.clone()),
            None => task.clone(),
        };
        let Some(cat) = root.cat.as_deref().map(|c| self.store.get_cat(c)).transpose()?.flatten() else { return Ok((None, Vec::new())) };
        let changed = self.sync_cattain(&cat.id)?;
        // A kitten of the Cattain's own thread does work for it, as its crew does: it reads CREW.md.
        let own = root.cattain && task.subagent.is_none();
        let text = self.store.file_row(&cat.id, if own { cats::RULES } else { cats::CREW })?.content;
        let hears = !own && crate::crew::tells_its_cattain(&root);
        Ok((Some(Handed { cat, own, text, hears }), changed))
    }

    /// The block for a run that was handed `h`: [`own_block`] or [`crew_block`].
    pub fn block_for(&self, h: &Handed) -> String {
        if h.own {
            let project = h.cat.workspace.as_deref().and_then(|w| self.store.get_workspace(w).ok().flatten()).filter(|w| w.is_active()).and_then(|w| w.folder.clone().map(|f| (w.name.clone(), f)));
            own_block(&h.cat, &self.cattain_folder(&h.cat.id), project.as_ref().map(|(n, f)| (n.as_str(), f.as_str())), &h.text)
        } else {
            crew_block(&h.cat, &h.text, h.hears)
        }
    }

    /// The folders a run of the Cattain's own thread may also read: its project's (`--add-dir`), when it has one that is there.
    pub fn read_too(&self, h: &Handed) -> Vec<PathBuf> {
        if !h.own {
            return Vec::new();
        }
        h.cat.workspace.as_deref().and_then(|w| self.store.get_workspace(w).ok().flatten()).filter(|w| w.is_active()).and_then(|w| w.folder).map(PathBuf::from).filter(|p| p.is_dir()).into_iter().collect()
    }

    /// The place a Cattain's own thread works in: its folder, its own (edits there go ahead under "Ask before risky steps").
    pub fn cattain_place(&self, cat: &str) -> Place {
        Place { workdir: self.cattain_folder(cat).to_string_lossy().into_owned(), ..Place::default() }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn miso() -> Cat {
        Cat { id: "cat_miso".into(), name: "miso".into(), face: "ginger".into(), purpose: None, workspace: None, thread: None, model: None, access: None, created_ms: 0, retired_ms: None, made_in: None }
    }

    #[test]
    fn the_own_block_names_the_folder_and_files_and_carries_rules_verbatim() {
        let mut cat = miso();
        let b = own_block(&cat, Path::new("/data/cattains/cat_miso"), None, "");
        assert!(b.starts_with("<neko-cattain>\nYou are miso, a Cattain in Neko"));
        assert!(b.contains("You work in your own folder, /data/cattains/cat_miso."));
        assert!(b.contains("Read RULES.md, NOTES.md and DIGEST.md at the start of every turn."));
        assert!(b.contains(CREW_PARAGRAPH) && CREW_PARAGRAPH.contains("mcp__neko__start_crew_thread") && CREW_PARAGRAPH.contains("Never tell a crew thread to merge"), "how it works its crew");
        assert!(!b.contains("Your standing rules") && !b.contains("Your project"), "nothing that is not there");
        cat.purpose = Some("Keeps the docs tidy.".into());
        let b = own_block(&cat, Path::new("/d"), Some(("acme-api", "/Users/example/acme-api")), "- Use British spelling.\n- **Never** push.");
        assert!(b.contains("\nYour purpose:\nKeeps the docs tidy.\n"));
        assert!(b.contains("\nYour project is acme-api, in /Users/example/acme-api. You can read it.\n"));
        assert!(b.ends_with("\nYour standing rules (RULES.md):\n- Use British spelling.\n- **Never** push.\n</neko-cattain>"));
        assert_eq!(b.matches("<neko-cattain").count(), 1);
    }

    #[test]
    fn the_crew_block_carries_crew_md_verbatim_and_nothing_else() {
        let cat = miso();
        assert_eq!(crew_block(&cat, "", false), "<neko-crew>\nThis thread is in miso's crew. miso is a Cattain in Neko, one of the person's cats, and looks after this work.\n</neko-crew>");
        let b = crew_block(&cat, "Open a PR; never merge.", true);
        assert!(b.ends_with("\nmiso's words for its crew (CREW.md):\nOpen a PR; never merge.\n</neko-crew>"));
        assert!(b.contains("\nmiso hears when each of your turns ends. Its messages to you come inside <neko-from cattain=\"miso\">"), "{b}");
    }
}
