//! Neko's core: the store and its first records (docs/knowledge/store.md).
//!
//! Logic here never reads the system clock or makes a random id itself: a [`Clock`] and an [`Ids`] are handed in, so the tests
//! run the same every time. The daemon hands in [`SystemClock`] and [`RandomIds`].

pub mod briefing;
pub mod cats;
pub mod checkpoint;
pub mod claude;
pub mod clock;
pub mod conductor;
pub mod find;
pub mod guard;
pub mod history;
pub mod needs;
pub mod mcp;
pub mod plugins;
pub mod policy;
pub mod queue;
pub mod records;
pub mod rounds;
pub mod runtime;
pub mod secrets;
pub mod shelf;
pub mod store;
pub mod subagents;
pub mod thread;
pub mod title;
pub mod workplace;
pub mod workspaces;

pub use cats::CatError;
pub use clock::{Clock, FixedClock, Ids, RandomIds, SequentialIds, SystemClock};
pub use records::Place;
pub use store::{DEFAULT_WORKSPACE, Store, StoreError};

use neko_protocol::{MAX_WORDS_BYTES, Task, TaskId, Workspace};

/// Why a new task was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum NewTaskError {
    Empty,
    TooLong,
    /// There is no such workspace.
    NoWorkspace,
    /// The workspace was removed: no new task is made in it.
    Removed(String),
    /// The workspace's folder is not there.
    FolderMissing(String),
    /// The cat given cannot take it (retired, not there).
    Cat(CatError),
    /// The branch to start from is not a name Neko hands to git.
    Branch(String),
    Store(String),
}

impl std::fmt::Display for NewTaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewTaskError::Empty => f.write_str("Type what you want done first."),
            NewTaskError::TooLong => write!(f, "That is longer than a thread can be ({} KB). Put the long part in a file.", MAX_WORDS_BYTES / 1000),
            NewTaskError::NoWorkspace => f.write_str("That project is not there any more. Choose another one."),
            NewTaskError::Removed(name) => write!(f, "{name} was removed from Neko, so no new thread can be made in it. Add its folder again, or choose another project."),
            NewTaskError::FolderMissing(name) => write!(f, "The folder of {name} was not found. Locate it in the sidebar, or choose another project."),
            NewTaskError::Cat(e) => write!(f, "{e}"),
            NewTaskError::Branch(b) => write!(f, "{b} is not a branch name Neko can start from."),
            NewTaskError::Store(e) => write!(f, "The thread could not be saved: {e}"),
        }
    }
}

/// Why a change to a workspace was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    NotFound,
    Folder(workspaces::FolderError),
    /// Not allowed (General, a removed workspace, a folder another workspace has); the words say why.
    Refused(String),
    Store(String),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceError::NotFound => f.write_str("There is no such project."),
            WorkspaceError::Folder(e) => write!(f, "{e}"),
            WorkspaceError::Refused(why) => f.write_str(why),
            WorkspaceError::Store(e) => write!(f, "The project could not be saved: {e}"),
        }
    }
}

impl From<StoreError> for WorkspaceError {
    fn from(e: StoreError) -> Self {
        WorkspaceError::Store(e.to_string())
    }
}

/// What adding a folder did.
#[derive(Debug, PartialEq, Eq)]
pub struct Added {
    pub workspace: Workspace,
    /// The folder was already a workspace (not removed): nothing changed.
    pub existed: bool,
    /// It had been removed and is back on the list.
    pub restored: bool,
    /// The revision of the change, when there was one (added, or brought back from removed).
    pub revision: Option<u64>,
}

/// The store with the clock and ids its writes need.
pub struct Core {
    pub store: Store,
    clock: Box<dyn Clock>,
    ids: Box<dyn Ids>,
}

impl Core {
    /// Opens (or makes) the store at `path` and makes sure the default workspace exists.
    pub fn open(path: &std::path::Path, clock: Box<dyn Clock>, ids: Box<dyn Ids>) -> Result<Core, StoreError> {
        let mut store = Store::open(path)?;
        let now = clock.now_ms();
        store.ensure_default_workspace(now)?;
        store.ensure_starter_cats(now)?;
        Ok(Core { store, clock, ids })
    }

    /// Stores a task with the person's exact words, in General, and the event that says so. Returns the task and the event's
    /// revision. It has no cat.
    pub fn create_task(&mut self, words: &str) -> Result<(Task, u64), NewTaskError> {
        self.create_task_in(words, None)
    }

    /// [`Core::create_task`] in a workspace (None: General). The task works in the workspace's folder; a removed workspace, or one
    /// whose folder is not there, takes no new task.
    pub fn create_task_in(&mut self, words: &str, workspace: Option<&str>) -> Result<(Task, u64), NewTaskError> {
        self.create_task_for(words, workspace, None)
    }

    /// [`Core::create_task_in`] given `cat` (an active cat's id) when there is one. The daemon never passes one for a thread the
    /// person starts (version 14); a cat's tasks are made by the code that owns the cat.
    pub fn create_task_for(&mut self, words: &str, workspace: Option<&str>, cat: Option<&str>) -> Result<(Task, u64), NewTaskError> {
        self.create_task_placed(words, workspace, cat, &workplace::Placement::default())
    }

    /// [`Core::create_task_for`] with what the person chose about where it works ([`workplace::Placement`]): the project's folder
    /// itself, or the branch its worktree starts from.
    pub fn create_task_placed(&mut self, words: &str, workspace: Option<&str>, cat: Option<&str>, how: &workplace::Placement) -> Result<(Task, u64), NewTaskError> {
        if let Some(b) = how.from_branch.as_deref().filter(|b| !workplace::branch_name_ok(b)) {
            return Err(NewTaskError::Branch(b.to_owned()));
        }
        let (id, w, cat, now) = self.plan_task(words, workspace, cat)?;
        self.store.create_task(&id, &w.id, words, w.folder.as_deref(), cat.as_ref().map(|c| c.id.as_str()), how, now).map_err(|e| NewTaskError::Store(e.to_string()))
    }

    /// The checks before any task is made, and its id: the workspace (there, not removed, its folder found) and the cat, when one
    /// is given.
    pub(crate) fn plan_task(&mut self, words: &str, workspace: Option<&str>, cat: Option<&str>) -> Result<(TaskId, Workspace, Option<neko_protocol::Cat>, i64), NewTaskError> {
        if words.trim().is_empty() {
            return Err(NewTaskError::Empty);
        }
        if words.len() > MAX_WORDS_BYTES {
            return Err(NewTaskError::TooLong);
        }
        let ws = workspace.filter(|w| !w.is_empty()).unwrap_or(DEFAULT_WORKSPACE);
        let w = self.store.get_workspace(ws).map_err(|e| NewTaskError::Store(e.to_string()))?.ok_or(NewTaskError::NoWorkspace)?;
        if !w.is_active() {
            return Err(NewTaskError::Removed(w.name));
        }
        if workspaces::with_missing(w.clone()).missing {
            return Err(NewTaskError::FolderMissing(w.name));
        }
        let cat = cat.filter(|c| !c.is_empty()).map(|c| self.active_cat(c)).transpose().map_err(NewTaskError::Cat)?;
        let now = self.clock.now_ms();
        let id = TaskId(format!("task_{}", self.ids.next_id(now)));
        Ok((id, w, cat, now))
    }

    /// Every workspace, with whether its folder is there now.
    pub fn workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        Ok(self.store.list_workspaces()?.into_iter().map(workspaces::with_missing).collect())
    }

    /// A workspace the person may change: not General, and there.
    fn changeable(&self, id: &str, doing: &str) -> Result<Workspace, WorkspaceError> {
        let w = self.store.get_workspace(id)?.ok_or(WorkspaceError::NotFound)?;
        if w.is_general() {
            return Err(WorkspaceError::Refused(format!("General is built in: it cannot be {doing}.")));
        }
        Ok(w)
    }

    /// Adds a folder as a workspace, named after the folder. The same folder again answers with its workspace (brought back when
    /// it was removed).
    pub fn add_workspace(&mut self, folder: &str) -> Result<Added, WorkspaceError> {
        let real = workspaces::check_folder(folder).map_err(WorkspaceError::Folder)?;
        let real_s = real.to_string_lossy().into_owned();
        let now = self.clock.now_ms();
        if let Some(w) = self.store.workspace_by_folder(&real_s)? {
            if w.is_active() {
                return Ok(Added { workspace: workspaces::with_missing(w), existed: true, restored: false, revision: None });
            }
            let (w, rev) = self.store.restore_workspace(&w.id, now)?;
            return Ok(Added { workspace: workspaces::with_missing(w), existed: false, restored: true, revision: Some(rev) });
        }
        let id = format!("ws_{}", self.ids.next_id(now));
        let (w, rev) = self.store.add_workspace(&id, &real_s, &workspaces::folder_name(&real), now)?;
        Ok(Added { workspace: workspaces::with_missing(w), existed: false, restored: false, revision: Some(rev) })
    }

    /// A new name: trimmed, one line, at most [`workspaces::MAX_NAME`] characters. An empty name is refused.
    pub fn rename_workspace(&mut self, id: &str, name: &str) -> Result<(Workspace, u64), WorkspaceError> {
        self.changeable(id, "renamed")?;
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(workspaces::MAX_NAME).collect();
        if name.is_empty() {
            return Err(WorkspaceError::Refused("A project needs a name.".into()));
        }
        let now = self.clock.now_ms();
        let (w, rev) = self.store.rename_workspace(id, &name, now)?;
        Ok((workspaces::with_missing(w), rev))
    }

    /// Off the list; nothing on disk is touched and its tasks stay readable.
    pub fn remove_workspace(&mut self, id: &str) -> Result<(Workspace, u64), WorkspaceError> {
        self.changeable(id, "removed")?;
        let now = self.clock.now_ms();
        let (w, rev) = self.store.remove_workspace(id, now)?;
        Ok((workspaces::with_missing(w), rev))
    }

    /// The folder moved: the workspace (and its tasks) follow it to `folder`, which no other workspace may have.
    pub fn locate_workspace(&mut self, id: &str, folder: &str) -> Result<(Workspace, u64), WorkspaceError> {
        let w = self.changeable(id, "moved to another folder")?;
        let real = workspaces::check_folder(folder).map_err(WorkspaceError::Folder)?.to_string_lossy().into_owned();
        if let Some(other) = self.store.workspace_by_folder(&real)?
            && other.id != w.id
        {
            return Err(WorkspaceError::Refused(format!("That folder is already the project {}.", other.name)));
        }
        let now = self.clock.now_ms();
        let (w, rev) = self.store.locate_workspace(id, &real, now)?;
        Ok((workspaces::with_missing(w), rev))
    }

    /// Reorders: the workspace goes to `to` among the ones on the list (0 is first, after General).
    pub fn move_workspace(&mut self, id: &str, to: usize) -> Result<(Vec<Workspace>, u64), WorkspaceError> {
        let w = self.changeable(id, "moved")?;
        if !w.is_active() {
            return Err(WorkspaceError::Refused(format!("{} was removed from Neko.", w.name)));
        }
        let now = self.clock.now_ms();
        let (changed, rev) = self.store.move_workspace(id, to, now)?;
        Ok((changed.into_iter().map(workspaces::with_missing).collect(), rev))
    }

    /// Now, from the clock handed in.
    pub fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    /// A new id with its prefix (`run_`, `part_`, `perm_`).
    pub fn new_id(&mut self, prefix: &str) -> String {
        let now = self.clock.now_ms();
        format!("{prefix}_{}", self.ids.next_id(now))
    }
}
