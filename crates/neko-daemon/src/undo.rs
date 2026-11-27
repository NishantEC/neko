//! Undo in the daemon (docs/knowledge/undo.md): a checkpoint at each run's start and end, review, undo and redo, housekeeping and
//! the storage figure. Git's work runs with the core unlocked; only the rows are written with it locked, and every change is
//! pushed as the run it changed.

use std::path::PathBuf;
use std::sync::Arc;

use neko_core::checkpoint::{self, Folder, Kind, Refusal};
use neko_core::history::{self, Checkpoint};
use neko_core::{Core, Place};
use neko_protocol::undo::{self, Action};
use neko_protocol::{Change, DiffPatch, DiffSource, DiffSummary, ErrorCode, Overview, Reply, Run, RunId, RunUndo, ServerMessage, TaskId, Tone, UndoState, UndoStorage};

use crate::server::{Shared, lock};

/// Checkpoints older than this go when their task finishes a run (never its newest run's pair). Tests shorten it with
/// `NEKO_UNDO_KEEP_MS`.
pub const KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;

fn keep_ms() -> i64 {
    std::env::var("NEKO_UNDO_KEEP_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(KEEP_MS)
}

fn refname(task: &TaskId, run: &RunId, kind: &str) -> String {
    format!("{}/{}/{}/{kind}", checkpoint::REF_ROOT, task.0, run.0)
}

fn task_prefix(task: &TaskId) -> String {
    format!("{}/{}/", checkpoint::REF_ROOT, task.0)
}

/// Stores where a run's undo stands and tells every screen. Called with the core locked.
fn set(shared: &Shared, core: &mut Core, run: &RunId, u: RunUndo) {
    let now = core.now();
    if let Ok((run, revision)) = core.store.set_run_undo(run, &u, now) {
        shared.broadcast(&ServerMessage::Event { revision, change: Change::RunChanged { run } });
    }
}

fn unavailable(why: &Refusal) -> RunUndo {
    RunUndo { state: UndoState::Unavailable, why: Some(why.to_string()), changes: None, committed: false }
}

/// Takes a snapshot and keeps it as the run's checkpoint of `kind`: its ref, then its row.
fn save(shared: &Shared, f: &Folder, task: &TaskId, run: &RunId, kind: &str, refname: String) -> Result<Checkpoint, Refusal> {
    let tmp = checkpoint::tmp_dir(&shared.runs.data_dir);
    let snap = checkpoint::take(f, &tmp, &format!("Neko checkpoint: {kind} of {}", run.0), &shared.runs.undo_limits)?;
    checkpoint::keep(f, &refname, &snap.commit)?;
    let mut core = lock(&shared.core);
    let now = core.now();
    let c = Checkpoint {
        id: core.new_id("cp"),
        task: task.clone(),
        run: run.clone(),
        kind: kind.to_owned(),
        place: f.kind,
        work: f.work.to_string_lossy().into_owned(),
        shadow: f.shadow.as_ref().map(|s| s.to_string_lossy().into_owned()),
        refname,
        commit: snap.commit,
        tree: snap.tree,
        head: snap.head,
        files: snap.files,
        bytes: snap.bytes,
        took_ms: snap.took.as_millis() as u64,
        made_ms: now,
        state: "kept".into(),
    };
    core.store.insert_checkpoint(&c, now).map_err(|e| Refusal::Failed(format!("Neko could not store the checkpoint: {e}")))?;
    Ok(c)
}

/// Before the agent starts: runs the person undid can no longer be redone (this run starts from the undone files), which a notice
/// says; the words the agent is told first, so it does not assume its edits are still there.
pub fn before_run(shared: &Shared, task: &TaskId) -> Option<String> {
    let mut core = lock(&shared.core);
    let runs = core.store.runs_of(task).ok()?;
    let undone: Vec<Run> = runs.into_iter().filter(|r| r.undo.as_ref().is_some_and(|u| u.state == UndoState::Undone)).collect();
    if undone.is_empty() {
        return None;
    }
    for r in &undone {
        if let Some(mut u) = r.undo.clone() {
            u.state = UndoState::Dropped;
            set(shared, &mut core, &r.id, u);
        }
    }
    if let Ok(Some(t)) = core.store.get_task(task) {
        let text = if undone.len() == 1 { "This message starts from the undone files, so the undone run can no longer be redone." } else { "This message starts from the undone files, so the undone runs can no longer be redone." };
        if let Ok(outs) = neko_core::conductor::standalone_notice(&mut core, &t, Tone::Info, text.to_owned(), None, None) {
            crate::runs::push(shared, task, outs, None);
        }
    }
    Some(undo::told(undone.len()))
}

/// The start checkpoint, taken before the agent starts (so it is of the folder before the run), within the limits' time. When it
/// cannot be taken the run goes ahead and its card says why undo is not available.
pub fn at_start(shared: &Shared, task: &TaskId, run: &RunId, place: &Place) {
    let done = checkpoint::folder_for(&shared.runs.data_dir, task, place).and_then(|f| save(shared, &f, task, run, history::START, refname(task, run, history::START)));
    let u = match done {
        Ok(_) => RunUndo { state: UndoState::Taking, why: None, changes: None, committed: false },
        Err(e) => unavailable(&e),
    };
    let mut core = lock(&shared.core);
    set(shared, &mut core, run, u);
}

/// The end checkpoint, taken when the agent has ended and before the folder may be tidied: what the run changed, and whether it
/// committed. Nothing when there is no start checkpoint.
pub fn at_end(shared: &Shared, task: &TaskId, run: &RunId) {
    let start = lock(&shared.core).store.checkpoint_of_run(run, history::START).ok().flatten();
    let Some(start) = start else { return };
    let f = folder_of(&start);
    let u = match save(shared, &f, task, run, history::END, refname(task, run, history::END)) {
        Ok(end) => match checkpoint::changes(&f, &start.tree, &end.tree) {
            Ok(c) => RunUndo { state: UndoState::Applied, why: None, changes: Some(c), committed: start.head != end.head },
            Err(e) => unavailable(&e),
        },
        Err(e) => unavailable(&e),
    };
    let mut core = lock(&shared.core);
    set(shared, &mut core, run, u);
}

fn folder_of(c: &Checkpoint) -> Folder {
    Folder { kind: c.place, work: PathBuf::from(&c.work), shadow: c.shadow.as_ref().map(PathBuf::from) }
}

/// A folder git can read a checkpoint's objects from, also when its work tree is gone (a tidied worktree: its repository).
fn reader(c: &Checkpoint, place: Option<&Place>) -> Folder {
    let mut f = folder_of(c);
    if !f.work.is_dir() {
        match (c.place, place.and_then(|p| p.repo.as_ref())) {
            (Kind::Repo, Some(repo)) => f.work = PathBuf::from(repo),
            (Kind::Shadow, _) => f.work = f.shadow.clone().unwrap_or_default(),
            _ => {}
        }
    }
    f
}

/// Removes checkpoints: their refs, then marks the rows; runs that lose one can no longer be reviewed or undone. Called with the
/// core unlocked.
fn remove(shared: &Shared, list: &[Checkpoint], place: Option<&Place>) {
    let mut gone = Vec::new();
    let mut shadows = Vec::new();
    for c in list.iter().filter(|c| c.state == "kept") {
        let f = reader(c, place);
        if c.place == Kind::Shadow && !shadows.contains(&f) {
            shadows.push(f.clone());
        }
        if checkpoint::drop_ref(&f, &c.refname) || checkpoint::refs_under(&f, &c.refname).is_empty() {
            gone.push(c.clone());
        }
    }
    for f in shadows {
        checkpoint::shadow_gc(&f);
    }
    let mut core = lock(&shared.core);
    let now = core.now();
    let ids: Vec<String> = gone.iter().map(|c| c.id.clone()).collect();
    let _ = core.store.remove_checkpoints(&ids, now);
    let mut runs: Vec<RunId> = gone.iter().filter(|c| c.kind == history::START || c.kind == history::END).map(|c| c.run.clone()).collect();
    runs.dedup();
    for run in runs {
        if let Ok(Some(r)) = core.store.get_run(&run)
            && let Some(mut u) = r.undo
            && u.state != UndoState::Removed
        {
            u.state = UndoState::Removed;
            set(shared, &mut core, &run, u);
        }
    }
}

/// After a run: when its worktree was tidied away its checkpoints go with it; checkpoints older than 30 days go (never the newest
/// run's pair).
pub fn after_run(shared: &Shared, task: &TaskId, tidied: bool) {
    let (list, place, newest) = {
        let core = lock(&shared.core);
        let list = core.store.checkpoints_of(task).unwrap_or_default();
        let newest = core.store.runs_of(task).ok().and_then(|r| r.last().map(|r| r.id.clone()));
        (list, core.store.place(task).ok().flatten(), newest)
    };
    let now = lock(&shared.core).now();
    let old: Vec<Checkpoint> = list
        .into_iter()
        .filter(|c| c.state == "kept")
        .filter(|c| (tidied && c.place == Kind::Repo) || (now - c.made_ms >= keep_ms() && Some(&c.run) != newest.as_ref()))
        .collect();
    if !old.is_empty() {
        remove(shared, &old, place.as_ref());
    }
}

/// A task is being removed: its refs in the repository and its shadow repository go.
pub fn forget_task(shared: &Shared, task: &TaskId, place: Option<&Place>) {
    let list = lock(&shared.core).store.checkpoints_of(task).unwrap_or_default();
    if let Some(c) = list.iter().find(|c| c.place == Kind::Repo) {
        checkpoint::drop_refs(&reader(c, place), &task_prefix(task));
    }
    let shadow = checkpoint::shadow_dir(&shared.runs.data_dir, task);
    if shadow.starts_with(shared.runs.data_dir.join(neko_protocol::files::UNDO)) {
        let _ = std::fs::remove_dir_all(shadow);
    }
}

fn refused(words: impl Into<String>) -> Reply {
    Reply::error(ErrorCode::Refused, words)
}

/// The two trees a diff is between: a run's start and end, or for the whole task its first run's start and where its newest run
/// left the folder. With the folder to read them from.
fn ends(shared: &Shared, task: &TaskId, run: Option<&RunId>) -> Result<(String, String, Folder, String), Reply> {
    let core = lock(&shared.core);
    let place = core.store.place(task).ok().flatten();
    let list = core.store.checkpoints_of(task).map_err(|e| Reply::error(ErrorCode::Internal, e.to_string()))?;
    let kept = |run: &RunId, kind: &str| list.iter().rev().find(|c| &c.run == run && c.kind == kind && c.state == "kept").cloned();
    let none = || refused("There are no checkpoints for this to show.");
    match run {
        Some(r) => {
            let (Some(a), Some(b)) = (kept(r, history::START), kept(r, history::END)) else { return Err(none()) };
            Ok((a.tree.clone(), b.tree.clone(), reader(&b, place.as_ref()), b.work.clone()))
        }
        None => {
            let runs = core.store.runs_of(task).map_err(|e| Reply::error(ErrorCode::Internal, e.to_string()))?;
            let first = runs.iter().find_map(|r| r.undo.as_ref().filter(|u| u.ready()).and_then(|_| kept(&r.id, history::START))).ok_or_else(none)?;
            let last = runs.iter().rev().find(|r| r.undo.as_ref().is_some_and(RunUndo::ready)).ok_or_else(none)?;
            let state = last.undo.as_ref().map(|u| u.state);
            let at = if state == Some(UndoState::Applied) { kept(&last.id, history::BEFORE_UNDO).or_else(|| kept(&last.id, history::END)) } else { kept(&last.id, history::START) };
            let at = at.ok_or_else(none)?;
            Ok((first.tree.clone(), at.tree.clone(), reader(&at, place.as_ref()), at.work.clone()))
        }
    }
}

/// What is uncommitted in the task's folder now: the folder as it is against the newest commit of its branch. Only a folder in a git
/// repository has that (a task's own checkout, or a chosen folder that is a repository with commits); with the folder (the trees are
/// read from it) and where the paths are.
fn uncommitted_ends(shared: &Shared, task: &TaskId) -> Result<(String, String, Folder, String), Reply> {
    let place = lock(&shared.core).store.place(task).ok().flatten();
    let Some(place) = place else { return Err(refused("There is no folder for this thread yet.")) };
    let f = if place.branch.is_some() && place.repo.is_some() {
        Folder { kind: Kind::Repo, work: neko_core::workplace::worktree_root(&shared.runs.data_dir, task), shadow: None }
    } else {
        Folder { kind: Kind::Repo, work: PathBuf::from(&place.workdir), shadow: None }
    };
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    if !f.work.is_dir() {
        return Err(refused("The thread's folder is gone, so nothing is waiting to be committed."));
    }
    match checkpoint::head(&f, until) {
        Ok(Some(_)) => {}
        _ => return Err(refused("This folder is not a git repository with commits, so there is nothing to compare with.")),
    }
    let tmp = checkpoint::tmp_dir(&shared.runs.data_dir);
    match checkpoint::tree_now(&f, &tmp, &shared.runs.undo_limits) {
        Ok(now) => {
            let root = f.work.to_string_lossy().into_owned();
            Ok(("HEAD^{tree}".to_owned(), now, f, root))
        }
        Err(e) => Err(refused(format!("Neko could not read the folder: {e}."))),
    }
}

/// The two trees a diff is between, for any source.
fn trees(shared: &Shared, task: &TaskId, run: Option<&RunId>, source: DiffSource) -> Result<(String, String, Folder, String), Reply> {
    match (source, run) {
        (DiffSource::Uncommitted, _) => uncommitted_ends(shared, task),
        (DiffSource::Run, Some(r)) => ends(shared, task, Some(r)),
        (DiffSource::Run | DiffSource::Thread, _) => ends(shared, task, None),
    }
}

fn summary(shared: &Shared, task: &TaskId, run: Option<RunId>, source: DiffSource) -> Result<DiffSummary, Reply> {
    let (a, b, f, root) = trees(shared, task, run.as_ref(), source)?;
    match checkpoint::files(&f, &a, &b) {
        Ok(files) => {
            let (added, removed) = (files.iter().map(|x| x.added).sum(), files.iter().map(|x| x.removed).sum());
            let run = if source == DiffSource::Run { run } else { None };
            Ok(DiffSummary { task: task.clone(), run, source, root, files, added, removed })
        }
        Err(e) => Err(refused(format!("Neko could not read the changes: {e}."))),
    }
}

/// A run's, the thread's, or the folder's uncommitted changes, file by file.
pub fn diff(shared: &Shared, task: &TaskId, run: Option<RunId>, source: DiffSource) -> Reply {
    match summary(shared, task, run, source) {
        Ok(diff) => Reply::Diff { diff },
        Err(r) => r,
    }
}

/// One file's lines, cut at [`undo::CUT_LINES`] unless `full` (then at [`undo::ALL_LINES`]).
pub fn diff_file(shared: &Shared, task: &TaskId, run: Option<RunId>, source: DiffSource, path: &str, full: bool) -> Reply {
    let (a, b, f, _) = match trees(shared, task, run.as_ref(), source) {
        Ok(e) => e,
        Err(r) => return r,
    };
    let files = match checkpoint::files(&f, &a, &b) {
        Ok(files) => files,
        Err(e) => return refused(format!("Neko could not read the changes: {e}.")),
    };
    let Some(file) = files.iter().find(|x| x.path == path) else { return Reply::error(ErrorCode::NotFound, "That file did not change here.") };
    let mut paths = vec![file.path.as_str()];
    if let Some(old) = &file.old_path {
        paths.push(old.as_str());
    }
    match checkpoint::patch(&f, &a, &b, &paths) {
        Ok(mut lines) => {
            let total = lines.len() as u32;
            let limit = if full { undo::ALL_LINES } else { undo::CUT_LINES };
            let cut = lines.len() > limit;
            lines.truncate(limit);
            Reply::DiffFile { file: DiffPatch { path: path.to_owned(), lines, total, cut } }
        }
        Err(e) => refused(format!("Neko could not read the changes: {e}.")),
    }
}

/// A thread's right pane in one reply: its folder and branch (the newest run's, else the one chosen), everything its runs changed,
/// and what is uncommitted now.
pub fn overview(shared: &Shared, task: &TaskId) -> Reply {
    let (folder, branch) = {
        let core = lock(&shared.core);
        let runs = core.store.runs_of(task).unwrap_or_default();
        match runs.iter().rev().find(|r| !r.workdir.is_empty()) {
            Some(r) => (Some(r.workdir.clone()), r.branch.clone()),
            None => match core.store.place(task).ok().flatten() {
                Some(p) => ((!p.workdir.is_empty()).then(|| p.workdir.clone()), p.branch.clone()),
                None => (None, None),
            },
        }
    };
    let changes = summary(shared, task, None, DiffSource::Thread).ok();
    let uncommitted = uncommitted_ends(shared, task).ok().and_then(|(a, b, f, _)| checkpoint::changes(&f, &a, &b).ok());
    Reply::Overview { overview: Overview { task: task.clone(), folder, branch, changes, uncommitted } }
}

/// Undo (or redo, `redo`) a run: only the one [`undo::offers`] offers. Never while the task works, one at a time per task.
pub fn undo_run(shared: &Arc<Shared>, task: &TaskId, run: &RunId, anyway: bool, redo: bool) -> Reply {
    {
        let active = lock(&shared.runs.active);
        if active.contains_key(task) {
            return Reply::error(ErrorCode::Busy, "Neko is working on this thread. Undo waits until the run ends.");
        }
        if !lock(&shared.runs.undoing).insert(task.clone()) {
            return Reply::error(ErrorCode::Busy, "Neko is already undoing a run of this thread.");
        }
    }
    let reply = undo_inner(shared, task, run, anyway, redo);
    lock(&shared.runs.undoing).remove(task);
    reply
}

fn undo_inner(shared: &Shared, task: &TaskId, run: &RunId, anyway: bool, redo: bool) -> Reply {
    let verb = if redo { "redo" } else { "undo" };
    let (runs, start, end, saved) = {
        let core = lock(&shared.core);
        let runs = match core.store.runs_of(task) {
            Ok(r) => r,
            Err(e) => return Reply::error(ErrorCode::Internal, e.to_string()),
        };
        let get = |k| core.store.checkpoint_of_run(run, k).ok().flatten();
        (runs, get(history::START), get(history::END), get(history::BEFORE_UNDO))
    };
    let Some(i) = runs.iter().position(|r| &r.id == run) else { return Reply::error(ErrorCode::NotFound, "There is no such run.") };
    let offer = &undo::offers(&runs)[i];
    let want = if redo { Action::Redo } else { Action::Undo };
    if offer.action != Some((want, true)) {
        return refused(offer.line.clone().unwrap_or_else(|| format!("Neko cannot {verb} this run now.")));
    }
    let (Some(start), Some(end)) = (start, end) else { return refused("The checkpoints of this run are gone.") };
    let f = folder_of(&end);
    let tmp = checkpoint::tmp_dir(&shared.runs.data_dir);
    let limits = shared.runs.undo_limits;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let not_now = |e: Refusal| refused(format!("Neko did not {verb} this run: {e}. Nothing was changed."));
    if !f.work.is_dir() {
        return not_now(Refusal::Gone);
    }
    match (checkpoint::midway(&f, until), checkpoint::index_locked(&f, until)) {
        (Ok(Some(what)), _) => return not_now(Refusal::Midway(what)),
        (_, Ok(true)) => return not_now(Refusal::IndexLocked),
        (Err(e), _) | (_, Err(e)) => return not_now(e),
        _ => {}
    }
    if f.kind == Kind::Repo {
        let head = checkpoint::head(&f, until).ok().flatten();
        if head != end.head {
            return refused("The thread's branch has new commits since this run, so Neko did not touch it. Revert the commits instead.");
        }
    }
    let applied = saved.as_ref().map_or(end.tree.clone(), |s| s.tree.clone());
    let (expect, target) = if redo { (start.tree.clone(), applied) } else { (applied, start.tree.clone()) };
    let now = match checkpoint::tree_now(&f, &tmp, &limits) {
        Ok(t) => t,
        Err(e) => return not_now(e),
    };
    let from = if now == expect {
        expect
    } else {
        let files = match checkpoint::differing(&f, &expect, &now) {
            Ok(v) => v,
            Err(e) => return not_now(e),
        };
        if !anyway {
            return Reply::ChangedSince { files };
        }
        // Even this can be taken back: the folder as it is now is kept first.
        let kind = if redo { history::BEFORE_REDO } else { history::BEFORE_UNDO };
        let n = lock(&shared.core).store.checkpoints_of(task).map_or(0, |l| l.iter().filter(|c| &c.run == run && c.kind == kind).count());
        match save(shared, &f, task, run, kind, refname(task, run, &format!("{kind}-{}", n + 1))) {
            Ok(c) => c.tree,
            Err(e) => return not_now(e),
        }
    };
    if let Err(e) = checkpoint::restore(&f, &tmp, &from, &target, &limits) {
        return not_now(e);
    }
    let mut core = lock(&shared.core);
    if let Ok(Some(r)) = core.store.get_run(run)
        && let Some(mut u) = r.undo
    {
        u.state = if redo { UndoState::Applied } else { UndoState::Undone };
        set(shared, &mut core, run, u);
    }
    Reply::Ok
}

/// What undo keeps: the bytes of Neko's undo folder, and the checkpoints kept.
pub fn storage(shared: &Shared) -> Reply {
    let kept = lock(&shared.core).store.kept_checkpoints().unwrap_or_default();
    let mut tasks: Vec<&TaskId> = kept.iter().map(|c| &c.task).collect();
    tasks.sort();
    tasks.dedup();
    let bytes = checkpoint::disk_bytes(&shared.runs.data_dir.join(neko_protocol::files::UNDO));
    Reply::UndoStorage { storage: UndoStorage { bytes, checkpoints: kept.len() as u32, tasks: tasks.len() as u32 } }
}

/// Clears every checkpoint: refs in repositories, the shadow repositories, and the rows. Refused while any task works.
pub fn clear(shared: &Shared) -> Reply {
    if !lock(&shared.runs.active).is_empty() {
        return Reply::error(ErrorCode::Busy, "Neko is working on a thread. Clear the undo history once it ends.");
    }
    let (kept, places) = {
        let core = lock(&shared.core);
        let kept = core.store.kept_checkpoints().unwrap_or_default();
        let places: Vec<(TaskId, Option<Place>)> = kept.iter().map(|c| (c.task.clone(), core.store.place(&c.task).ok().flatten())).collect();
        (kept, places)
    };
    for (task, place) in &places {
        if let Some(c) = kept.iter().find(|c| &c.task == task && c.place == Kind::Repo) {
            checkpoint::drop_refs(&reader(c, place.as_ref()), &task_prefix(task));
        }
    }
    let undo_dir = shared.runs.data_dir.join(neko_protocol::files::UNDO);
    if let Ok(rd) = std::fs::read_dir(&undo_dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().ends_with(".git") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    remove(shared, &kept, None);
    Reply::Ok
}
