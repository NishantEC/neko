//! Cats (docs/knowledge/cats.md): the named agents who do the person's work.
//!
//! - A cat has a name (1 to 40 characters, one line, unique among all cats without regard to case), a face from a fixed set, an
//!   optional purpose line (200 characters), standing rules (Markdown, 16 KB) with one previous version kept, and an optional
//!   default workspace. At most 16 are active; at least one always is.
//! - Three starter cats (miso, mochi, tofu) are made on first open. They are ordinary cats.
//! - A thread has no cat (version 14). Nothing assigns one: the picked, workspace-default and least-busy rules are gone. A task made
//!   before keeps the cat it was given, and a round's task has its round's cat; their runs still carry that cat's rules.
//! - A cat's rules reach the agent as one block ([`block`]) on every run of its tasks. They are words for the agent and nothing
//!   more: they never change the permission mode, the hook or any flag Neko passes.

use neko_protocol::cats::{self, Cat, FACES, MAX_ACTIVE, MAX_NAME, MAX_PURPOSE, MAX_RULES, STARTERS};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::records::log_event;
use crate::store::{Store, StoreError};
use crate::Core;

const CAT_COLUMNS: &str = "id, name, face, purpose, rules, previous_rules, rules_saved_ms, workspace_id, created_ms, retired_ms";

/// Active cats first, oldest first; then the retired ones.
const CAT_ORDER: &str = "ORDER BY (retired_ms IS NOT NULL), created_ms, id";

fn cat_of(r: &rusqlite::Row<'_>) -> rusqlite::Result<Cat> {
    let rules: String = r.get(4)?;
    Ok(Cat {
        id: r.get(0)?,
        name: r.get(1)?,
        face: r.get(2)?,
        purpose: r.get(3)?,
        rules_hash: cats::rules_hash(&rules),
        rules,
        previous_rules: r.get(5)?,
        rules_saved_ms: r.get(6)?,
        workspace: r.get(7)?,
        created_ms: r.get(8)?,
        retired_ms: r.get(9)?,
    })
}

/// How names are compared: without case.
pub fn name_key(name: &str) -> String {
    name.to_lowercase()
}

/// Why a change to a cat was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum CatError {
    NotFound,
    /// The name is empty, too long or taken; the words say which.
    InvalidName(String),
    /// Not allowed now (the limit, the last active cat, a retired cat, rules too long, a run already started); the words say why.
    Refused(String),
    Store(String),
}

impl std::fmt::Display for CatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatError::NotFound => f.write_str("There is no such cat."),
            CatError::InvalidName(why) | CatError::Refused(why) => f.write_str(why),
            CatError::Store(e) => write!(f, "The cat could not be saved: {e}"),
        }
    }
}

impl From<StoreError> for CatError {
    fn from(e: StoreError) -> Self {
        CatError::Store(e.to_string())
    }
}

/// A name as it is kept: one line, spaces collapsed, 1 to [`MAX_NAME`] characters. Too long is refused, not cut.
pub fn clean_name(name: &str) -> Result<String, CatError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(CatError::InvalidName("A cat needs a name.".into()));
    }
    if name.chars().count() > MAX_NAME {
        return Err(CatError::InvalidName(format!("A cat's name can be at most {MAX_NAME} characters.")));
    }
    Ok(name)
}

/// A purpose as it is kept: one line, spaces collapsed; empty is none. Over [`MAX_PURPOSE`] characters is refused.
pub fn clean_purpose(purpose: &str) -> Result<Option<String>, CatError> {
    let p = purpose.split_whitespace().collect::<Vec<_>>().join(" ");
    if p.chars().count() > MAX_PURPOSE {
        return Err(CatError::Refused(format!("A purpose can be at most {MAX_PURPOSE} characters.")));
    }
    Ok((!p.is_empty()).then_some(p))
}

/// The block that carries a cat to the agent, ahead of the person's words: it names the cat, and carries its purpose and rules
/// exactly as saved. It says nothing else.
pub fn block(cat: &Cat) -> String {
    let mut b = format!("<neko-cat>\nYou are {}, one of the person's cats in Neko.\n", cat.name);
    if let Some(p) = cat.purpose.as_deref().filter(|p| !p.is_empty()) {
        b.push_str("\nYour purpose:\n");
        b.push_str(p);
        b.push('\n');
    }
    if !cat.rules.is_empty() {
        b.push_str("\nYour standing rules, from the person:\n");
        b.push_str(&cat.rules);
        if !cat.rules.ends_with('\n') {
            b.push('\n');
        }
    }
    b.push_str("</neko-cat>");
    b
}

/// One starter cat as migration 6 weighs it, when it gives the tasks that already existed a cat. Nothing else chooses a cat for a
/// task any more (threads need none, version 14): this is only that migration's history.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    id: String,
    created_ms: i64,
    /// Its tasks working right now (working, or waiting on a request).
    working: u32,
    /// When it was last given a task (None: never).
    last_used_ms: Option<i64>,
}

/// The least busy of `cats`: the fewest tasks working now, then the one given a task least recently (never comes first), then the
/// oldest, then by id.
fn least_busy(cats: &[Candidate]) -> Option<&Candidate> {
    cats.iter().min_by(|a, b| (a.working, a.last_used_ms.unwrap_or(i64::MIN), a.created_ms, &a.id).cmp(&(b.working, b.last_used_ms.unwrap_or(i64::MIN), b.created_ms, &b.id)))
}

fn insert_starters(tx: &Transaction<'_>, created_ms: i64) -> rusqlite::Result<()> {
    for (id, name, face) in STARTERS {
        tx.execute(
            "INSERT OR IGNORE INTO cats (id, name, name_key, face, rules, created_ms) VALUES (?1, ?2, ?3, ?4, '', ?5)",
            params![id, name, name_key(name), face, created_ms],
        )?;
    }
    Ok(())
}

/// Migration 6's step: when there are tasks, the starter cats are made (as old as the oldest task) and every task goes to one,
/// oldest first, by the rule a new task followed then. No workspace had a default cat yet, so it is the least busy: a task still
/// marked working counts as busy, and ties go to the cat given a task least recently. No clock and no randomness: the same store
/// gives the same answer every time.
pub(crate) fn migrate_tasks(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let tasks: Vec<(String, i64, String)> = {
        let mut st = tx.prepare("SELECT id, created_ms, status FROM tasks ORDER BY created_ms, id")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?
    };
    let Some(oldest) = tasks.first().map(|t| t.1) else { return Ok(()) };
    insert_starters(tx, oldest)?;
    let mut cats: Vec<Candidate> = STARTERS.iter().map(|(id, ..)| Candidate { id: (*id).to_owned(), created_ms: oldest, working: 0, last_used_ms: None }).collect();
    for (task, created, status) in tasks {
        let Some(pick) = least_busy(&cats).map(|c| c.id.clone()) else { break };
        tx.execute("UPDATE tasks SET cat_id = ?2 WHERE id = ?1", params![task, pick])?;
        if let Some(c) = cats.iter_mut().find(|c| c.id == pick) {
            c.last_used_ms = Some(created);
            if status == "working" || status == "needs_you" {
                c.working += 1;
            }
        }
    }
    Ok(())
}

impl Store {
    /// Every cat: active ones first, oldest first; then the retired.
    pub fn list_cats(&self) -> Result<Vec<Cat>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {CAT_COLUMNS} FROM cats {CAT_ORDER}"))?;
        let rows = st.query_map([], cat_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_cat(&self, id: &str) -> Result<Option<Cat>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {CAT_COLUMNS} FROM cats WHERE id = ?1"), [id], cat_of).optional()?)
    }

    /// The cat with this name (compared without case), retired or not.
    pub fn cat_by_name(&self, name: &str) -> Result<Option<Cat>, StoreError> {
        Ok(self.conn.query_row(&format!("SELECT {CAT_COLUMNS} FROM cats WHERE name_key = ?1"), [name_key(name)], cat_of).optional()?)
    }

    /// Makes the starter cats when there are no cats at all (a new install). Like General, they are part of a new store, not a
    /// change: no event is written.
    pub fn ensure_starter_cats(&mut self, now_ms: i64) -> Result<(), StoreError> {
        let none: bool = self.conn.query_row("SELECT COUNT(*) = 0 FROM cats", [], |r| r.get(0))?;
        if none {
            let tx = self.conn.transaction()?;
            insert_starters(&tx, now_ms)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// A new cat and its event.
    pub fn insert_cat(&mut self, c: &Cat, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            &format!("INSERT INTO cats ({CAT_COLUMNS}, name_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"),
            params![c.id, c.name, c.face, c.purpose, c.rules, c.previous_rules, c.rules_saved_ms, c.workspace, c.created_ms, c.retired_ms, name_key(&c.name)],
        )?;
        let seq = log_event(&tx, now_ms, &c.id, "cat_added", serde_json::json!({ "name": c.name }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Writes a cat whole, with an event of `kind` (`cat_renamed`, `cat_rules_saved`, …). Returns it as stored.
    pub fn write_cat(&mut self, c: &Cat, kind: &str, now_ms: i64) -> Result<(Cat, u64), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE cats SET name = ?2, name_key = ?3, face = ?4, purpose = ?5, rules = ?6, previous_rules = ?7, rules_saved_ms = ?8, workspace_id = ?9, retired_ms = ?10 WHERE id = ?1",
            params![c.id, c.name, name_key(&c.name), c.face, c.purpose, c.rules, c.previous_rules, c.rules_saved_ms, c.workspace, c.retired_ms],
        )?;
        let seq = log_event(&tx, now_ms, &c.id, kind, serde_json::json!({ "name": c.name, "rules": cats::rules_hash(&c.rules) }))?;
        let c = tx.query_row(&format!("SELECT {CAT_COLUMNS} FROM cats WHERE id = ?1"), [&c.id], cat_of)?;
        tx.commit()?;
        Ok((c, seq))
    }
}

impl Core {
    /// Every cat: active first, then retired.
    pub fn cats(&self) -> Result<Vec<Cat>, StoreError> {
        self.store.list_cats()
    }

    fn cat(&self, id: &str) -> Result<Cat, CatError> {
        self.store.get_cat(id)?.ok_or(CatError::NotFound)
    }

    fn active_count(&self) -> Result<usize, StoreError> {
        Ok(self.store.list_cats()?.iter().filter(|c| c.is_active()).count())
    }

    /// The name is free: no other cat has it, compared without case.
    fn name_free(&self, name: &str, except: Option<&str>) -> Result<(), CatError> {
        match self.store.cat_by_name(name)? {
            Some(other) if Some(other.id.as_str()) != except => Err(CatError::InvalidName(if other.is_active() {
                format!("There is already a cat called {}.", other.name)
            } else {
                format!("A retired cat is called {}. Bring it back from Settings, or choose another name.", other.name)
            })),
            _ => Ok(()),
        }
    }

    fn check_face(face: &str) -> Result<(), CatError> {
        if FACES.contains(&face) { Ok(()) } else { Err(CatError::Refused(format!("Neko has no face called {face}."))) }
    }

    fn check_workspace(&self, ws: Option<&str>) -> Result<Option<String>, CatError> {
        let Some(id) = ws.filter(|w| !w.is_empty()) else { return Ok(None) };
        let w = self.store.get_workspace(id)?.ok_or_else(|| CatError::Refused("That project is not there any more.".into()))?;
        if !w.is_active() {
            return Err(CatError::Refused(format!("{} was removed from Neko.", w.name)));
        }
        Ok(Some(w.id))
    }

    /// A new cat. Its face, when none is given, is the first no active cat has (else the first).
    pub fn create_cat(&mut self, name: &str, face: Option<&str>, purpose: Option<&str>, workspace: Option<&str>) -> Result<(Cat, u64), CatError> {
        let name = clean_name(name)?;
        self.name_free(&name, None)?;
        if self.active_count()? >= MAX_ACTIVE {
            return Err(CatError::Refused(format!("Neko keeps at most {MAX_ACTIVE} cats at work. Retire one first.")));
        }
        let all = self.store.list_cats()?;
        let face = match face {
            Some(f) => {
                Self::check_face(f)?;
                f.to_owned()
            }
            None => FACES.iter().find(|f| !all.iter().any(|c| c.is_active() && c.face == **f)).unwrap_or(&FACES[0]).to_string(),
        };
        let purpose = clean_purpose(purpose.unwrap_or(""))?;
        let workspace = self.check_workspace(workspace)?;
        let now = self.now();
        let cat = Cat { id: self.new_id("cat"), name, face, purpose, rules: String::new(), rules_hash: cats::rules_hash(""), previous_rules: None, rules_saved_ms: None, workspace, created_ms: now, retired_ms: None };
        let rev = self.store.insert_cat(&cat, now)?;
        Ok((cat, rev))
    }

    fn write(&mut self, cat: &Cat, kind: &str) -> Result<(Cat, u64), CatError> {
        let now = self.now();
        Ok(self.store.write_cat(cat, kind, now)?)
    }

    pub fn rename_cat(&mut self, id: &str, name: &str) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        let name = clean_name(name)?;
        self.name_free(&name, Some(id))?;
        cat.name = name;
        self.write(&cat, "cat_renamed")
    }

    pub fn set_cat_face(&mut self, id: &str, face: &str) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        Self::check_face(face)?;
        cat.face = face.to_owned();
        self.write(&cat, "cat_face")
    }

    pub fn set_cat_purpose(&mut self, id: &str, purpose: &str) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        cat.purpose = clean_purpose(purpose)?;
        self.write(&cat, "cat_purpose")
    }

    pub fn set_cat_workspace(&mut self, id: &str, workspace: Option<&str>) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        cat.workspace = self.check_workspace(workspace)?;
        self.write(&cat, "cat_workspace")
    }

    /// Saves the rules exactly as given (at most [`MAX_RULES`] bytes); the ones before become the previous rules. Saving the same
    /// rules again changes nothing.
    pub fn save_cat_rules(&mut self, id: &str, rules: &str) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        if rules.len() > MAX_RULES {
            return Err(CatError::Refused(format!("A cat's rules can be at most {} KB.", MAX_RULES / 1024)));
        }
        if rules == cat.rules {
            let rev = self.store.revision()?;
            return Ok((cat, rev));
        }
        cat.previous_rules = Some(std::mem::replace(&mut cat.rules, rules.to_owned()));
        cat.rules_saved_ms = Some(self.now());
        self.write(&cat, "cat_rules_saved")
    }

    /// Puts the previous rules back; the current ones become the previous, so restoring again goes back.
    pub fn restore_cat_rules(&mut self, id: &str) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        let Some(previous) = cat.previous_rules.take() else { return Err(CatError::Refused(format!("{} has no earlier rules to restore.", cat.name))) };
        cat.previous_rules = Some(std::mem::replace(&mut cat.rules, previous));
        cat.rules_saved_ms = Some(self.now());
        self.write(&cat, "cat_rules_restored")
    }

    /// Retires a cat: it takes no new tasks, and no workspace's new tasks go to it. Its tasks stay readable. The last active cat
    /// cannot be retired.
    pub fn retire_cat(&mut self, id: &str) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        if !cat.is_active() {
            return Ok((cat, self.store.revision()?));
        }
        if self.active_count()? <= 1 {
            return Err(CatError::Refused(format!("{} is your only cat at work. Make another one first.", cat.name)));
        }
        cat.retired_ms = Some(self.now());
        self.write(&cat, "cat_retired")
    }

    /// Brings a retired cat back, unless [`MAX_ACTIVE`] are at work.
    pub fn bring_back_cat(&mut self, id: &str) -> Result<(Cat, u64), CatError> {
        let mut cat = self.cat(id)?;
        if cat.is_active() {
            return Ok((cat, self.store.revision()?));
        }
        if self.active_count()? >= MAX_ACTIVE {
            return Err(CatError::Refused(format!("Neko keeps at most {MAX_ACTIVE} cats at work. Retire one first.")));
        }
        cat.retired_ms = None;
        // Its default workspace may have been removed meanwhile.
        if self.check_workspace(cat.workspace.as_deref()).is_err() {
            cat.workspace = None;
        }
        self.write(&cat, "cat_brought_back")
    }

    /// A cat a task may be given (a round's cat): it must be there and active. Threads the person starts have none.
    pub fn active_cat(&self, id: &str) -> Result<Cat, CatError> {
        let cat = self.cat(id)?;
        if !cat.is_active() {
            return Err(CatError::Refused(format!("{} is retired and takes no new threads. Bring it back from Settings, or pick another cat.", cat.name)));
        }
        Ok(cat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_names_the_cat_and_carries_its_words_verbatim_and_nothing_else() {
        let mut cat = Cat { id: "cat_miso".into(), name: "miso".into(), face: "ginger".into(), purpose: None, rules: String::new(), rules_hash: String::new(), previous_rules: None, rules_saved_ms: None, workspace: None, created_ms: 0, retired_ms: None };
        assert_eq!(block(&cat), "<neko-cat>\nYou are miso, one of the person's cats in Neko.\n</neko-cat>");
        cat.purpose = Some("Keeps the docs tidy.".into());
        cat.rules = "- Use British spelling.\n- **Never** push.".into();
        let b = block(&cat);
        assert!(b.contains("\nYour purpose:\nKeeps the docs tidy.\n"));
        assert!(b.contains("\nYour standing rules, from the person:\n- Use British spelling.\n- **Never** push.\n</neko-cat>"));
        assert_eq!(b.matches("<neko-cat").count(), 1);
    }

    #[test]
    fn names_are_one_line_and_at_most_forty_characters() {
        assert_eq!(clean_name("  Big   miso \n").unwrap(), "Big miso");
        assert!(matches!(clean_name(" \n "), Err(CatError::InvalidName(_))));
        assert!(clean_name(&"é".repeat(40)).is_ok());
        assert!(matches!(clean_name(&"é".repeat(41)), Err(CatError::InvalidName(_))));
        assert_eq!(clean_purpose("  ").unwrap(), None);
        assert!(clean_purpose(&"x".repeat(201)).is_err());
    }
}
