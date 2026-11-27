//! The right pane's state and words (docs/knowledge/app-shell.md, "The right pane"): which tab is shown for the thread in view, the
//! kittens opened as tabs, whether the pane is open (docked, or a sheet in a narrow window), its width, and what the daemon last
//! said in a thread's overview. The drawing is `pane_view.rs`.
//!
//! The pane belongs to the thread in view (a kitten opened in the main view shows its thread's pane). Its open state and width are
//! the person's and are kept in the prefs; the tab and the kittens' tabs are kept while the app is open.

use std::collections::{HashMap, HashSet};

use neko_protocol::{DiffSource, Overview, Request, Run, RunStatus, Task, TaskId};
use crate::ui::tabs::TabsState;

use crate::state::{App, Asked};

/// The pane's width at first, and the least and most (a share of the window) it may be dragged to.
pub const DEFAULT_WIDTH: f32 = 380.0;
pub const MIN_WIDTH: f32 = 320.0;
pub const MAX_WIDTH: f32 = 560.0;
pub const MAX_FRACTION: f32 = 0.5;
/// The conversation keeps at least this much when the pane is dragged wide.
pub const CENTER_MIN: f32 = 440.0;
/// From this window width the pane is open until the person closes it.
pub const OPEN_FROM: f32 = 1240.0;
/// Under this window width the pane is a sheet over the conversation's right edge.
pub const SHEET_BELOW: f32 = 1100.0;
/// The prefs keys of the person's choice and the width they dragged it to.
pub const OPEN_KEY: &str = "pane-open";
pub const WIDTH_KEY: &str = "pane-width";

/// One tab of the pane. Overview first, Changes second, then a kitten's tab for each one opened, in the order they were opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Changes,
    Kitten(TaskId),
}

pub struct Pane {
    /// The person's choice (⌘⌥B, the pane button); `None` follows the window's width.
    pub choice: Option<bool>,
    /// Its width when docked.
    pub width: f32,
    /// A narrow window's sheet is open.
    pub sheet: bool,
    /// The tab shown for each thread, and the kittens opened as tabs (in the order they were opened).
    pub tab: HashMap<TaskId, Tab>,
    pub kittens: HashMap<TaskId, Vec<TaskId>>,
    /// Threads the pane already opened itself for (once each, when the first kitten started or the first changes landed).
    pub proactive: HashSet<TaskId>,
    /// What the tab strip measured (for the More tabs menu).
    pub tabs: TabsState,
    /// Each thread's overview as the daemon last said, those that must be asked again, and those asked and not yet answered.
    pub overview: HashMap<TaskId, Overview>,
    pub stale: HashSet<TaskId>,
    pub asked: HashSet<TaskId>,
    /// The kitten tab's conversation is pinned to the newest words.
    pub at_bottom: bool,
    /// A kitten tab's brief is open.
    pub brief_open: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Pane {
            choice: None,
            width: DEFAULT_WIDTH,
            sheet: false,
            tab: HashMap::new(),
            kittens: HashMap::new(),
            proactive: HashSet::new(),
            tabs: TabsState::default(),
            overview: HashMap::new(),
            stale: HashSet::new(),
            asked: HashSet::new(),
            at_bottom: true,
            brief_open: false,
        }
    }
}

/// "+19 −7 · 3 files": a change's figures in words, with the real minus sign.
pub fn changes_words(files: u32, added: u32, removed: u32) -> String {
    format!("+{added} \u{2212}{removed} \u{b7} {files} file{}", if files == 1 { "" } else { "s" })
}

/// "3 files", "1 file".
pub fn files_words(n: u32) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

impl App {
    /// The thread the pane is about: the open thread, or the thread a kitten opened in the main view belongs to. None on any other page.
    pub fn pane_root(&self) -> Option<TaskId> {
        if self.settings_open || self.needs_open || self.rounds_open || self.briefing_open || self.page_cat().is_some() {
            return None;
        }
        let t = self.open_task()?;
        Some(t.subagent.as_ref().map_or_else(|| t.id.clone(), |o| o.root.clone()))
    }

    /// The root thread of any task (itself when it is one).
    pub fn root_of(&self, id: &TaskId) -> TaskId {
        self.tasks.iter().find(|t| &t.id == id).and_then(|t| t.subagent.as_ref().map(|o| o.root.clone())).unwrap_or_else(|| id.clone())
    }

    /// The window is too narrow for a docked pane: it is a sheet.
    pub fn pane_is_sheet(&self) -> bool {
        self.width.is_some_and(|w| w < SHEET_BELOW)
    }

    /// The docked pane is open: the person's choice, else wide windows only.
    pub fn pane_docked_open(&self) -> bool {
        !self.pane_is_sheet() && self.pane.choice.unwrap_or(self.width.is_some_and(|w| w >= OPEN_FROM))
    }

    /// Whether the pane (docked, or the sheet) shows now.
    pub fn pane_visible(&self) -> bool {
        self.pane_root().is_some() && if self.pane_is_sheet() { self.pane.sheet } else { self.pane_docked_open() }
    }

    /// The pane button and ⌘⌥B: opens or closes the pane (the sheet, in a narrow window). The choice is remembered.
    pub fn toggle_pane(&mut self) {
        if self.pane_is_sheet() {
            self.pane.sheet = !self.pane.sheet;
            return;
        }
        let open = !self.pane_docked_open();
        self.pane.choice = Some(open);
        self.save_pane_choice();
    }

    /// A drag past the least width collapses the pane, and a drag from the edge opens it: the same choice as the button.
    pub fn set_pane_open(&mut self, open: bool) {
        self.pane.choice = Some(open);
        self.save_pane_choice();
    }

    /// Shows the pane for something the person asked for (a tab, Review): docked, or the sheet. Not remembered.
    pub fn show_pane(&mut self) {
        if self.pane_is_sheet() {
            self.pane.sheet = true;
        } else if !self.pane_docked_open() {
            self.pane.choice = Some(true);
        }
    }

    /// Escape on the sheet, its close button, a press outside it.
    pub fn close_sheet(&mut self) {
        self.pane.sheet = false;
    }

    /// The width the pane was dragged to.
    pub fn set_pane_width(&mut self, w: f32) {
        self.pane.width = w;
        self.save_pane_width();
    }

    fn save_pane_choice(&self) {
        if self.persist && let Some(open) = self.pane.choice {
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let _ = rune::prefs::Prefs::open(crate::PREFS_ID).set(OPEN_KEY, &open);
            });
        }
    }

    fn save_pane_width(&self) {
        if self.persist {
            let w = self.pane.width;
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let _ = rune::prefs::Prefs::open(crate::PREFS_ID).set(WIDTH_KEY, &w);
            });
        }
    }

    /// The tabs of a thread, in order: Overview, Changes, then the kittens opened (the ones that still exist).
    pub fn tabs_of(&self, root: &TaskId) -> Vec<Tab> {
        let mut v = vec![Tab::Overview, Tab::Changes];
        if let Some(list) = self.pane.kittens.get(root) {
            v.extend(list.iter().filter(|k| self.tasks.iter().any(|t| &t.id == *k)).map(|k| Tab::Kitten(k.clone())));
        }
        v
    }

    /// The tab shown for a thread (Overview unless the person chose another that still exists).
    pub fn tab_of(&self, root: &TaskId) -> Tab {
        let want = self.pane.tab.get(root).cloned().unwrap_or(Tab::Overview);
        if self.tabs_of(root).contains(&want) { want } else { Tab::Overview }
    }

    /// The tab at an index of the strip.
    pub fn select_tab(&mut self, i: usize) {
        let Some(root) = self.pane_root() else { return };
        if let Some(tab) = self.tabs_of(&root).get(i).cloned() {
            self.pane.tab.insert(root, tab);
            self.pane.at_bottom = true;
        }
    }

    /// ⌘⌥1 … ⌘⌥9: a tab by its place in the strip, opening the pane.
    pub fn select_tab_shown(&mut self, i: usize) {
        if self.pane_root().is_none() {
            return;
        }
        self.show_pane();
        self.select_tab(i);
    }

    /// Closes a tab: a kitten's (Overview and Changes stay).
    pub fn close_tab(&mut self, i: usize) {
        let Some(root) = self.pane_root() else { return };
        let Some(Tab::Kitten(k)) = self.tabs_of(&root).get(i).cloned() else { return };
        if let Some(list) = self.pane.kittens.get_mut(&root) {
            list.retain(|x| x != &k);
        }
        if self.pane.tab.get(&root) == Some(&Tab::Kitten(k)) {
            self.pane.tab.insert(root, Tab::Overview);
        }
    }

    /// The Overview tab, opening the pane.
    pub fn open_overview(&mut self) {
        if let Some(root) = self.pane_root() {
            self.pane.tab.insert(root, Tab::Overview);
            self.show_pane();
        }
    }

    /// A kitten's own conversation as a tab of the pane (its card's Open, a link in the waiting line, a row of Overview).
    pub fn open_kitten_tab(&mut self, kitten: &TaskId) {
        let Some(task) = self.tasks.iter().find(|t| &t.id == kitten) else { return };
        let Some(root) = task.subagent.as_ref().map(|o| o.root.clone()) else { return };
        // The thread's own page is the place for it: a kitten opened in the main view first goes back to its thread.
        if self.open.as_ref() != Some(&root) && self.open_task().is_none_or(|t| t.subagent.as_ref().is_none_or(|o| o.root != root)) {
            self.open_task_id(&root);
        }
        let list = self.pane.kittens.entry(root.clone()).or_default();
        if !list.contains(kitten) {
            list.push(kitten.clone());
        }
        self.pane.tab.insert(root, Tab::Kitten(kitten.clone()));
        self.pane.at_bottom = true;
        self.show_pane();
        self.ask_kitten_book(kitten);
    }

    /// Asks once for a kitten's transcript when the window has not read it (a tab opened after a restart).
    pub fn ask_kitten_book(&mut self, kitten: &TaskId) {
        if self.sub_asked.contains(kitten) || self.books.get(kitten).is_some_and(|b| b.loaded) {
            return;
        }
        if self.send(Request::GetTranscript { task: kitten.clone() }).is_some() {
            self.sub_asked.insert(kitten.clone());
        }
    }

    /// ⌘-click on a kitten's Open: its conversation in the main view, in place of the thread's.
    pub fn open_kitten_main(&mut self, kitten: &TaskId) {
        let root = self.root_of(kitten);
        if let Some(list) = self.pane.kittens.get_mut(&root) {
            list.retain(|x| x != kitten);
        }
        if self.pane.tab.get(&root) == Some(&Tab::Kitten(kitten.clone())) {
            self.pane.tab.insert(root, Tab::Overview);
        }
        self.open_task_id(kitten);
    }

    /// The Changes tab (⌘⇧G, ⌘D, Review on a run's change line, All changes in the thread's menu), opening the pane. `run`: that run's
    /// changes; none: what the pane last showed there, else the newest run's.
    pub fn open_changes(&mut self, run: Option<neko_protocol::RunId>) {
        let Some(root) = self.pane_root() else { return };
        self.menu_open = None;
        self.pane.tab.insert(root.clone(), Tab::Changes);
        self.show_pane();
        match run {
            Some(r) => self.load_changes(DiffSource::Run, Some(r)),
            None if self.review.as_ref().is_none_or(|r| r.task != root) => self.load_default_changes(),
            None => {}
        }
    }

    /// What the Changes tab opens on: the newest run that changed something, else everything the thread changed.
    fn load_default_changes(&mut self) {
        let Some(root) = self.pane_root() else { return };
        match self.latest_changed_run(&root) {
            Some(run) => self.load_changes(DiffSource::Run, Some(run)),
            None => self.load_changes(DiffSource::Thread, None),
        }
    }

    /// The thread's newest run (its own, not a kitten's) whose checkpoints say it changed something.
    pub fn latest_changed_run(&self, root: &TaskId) -> Option<neko_protocol::RunId> {
        self.books.get(root)?.runs.iter().rev().find(|r| r.undo.as_ref().is_some_and(neko_protocol::RunUndo::changed)).map(|r| r.id.clone())
    }

    /// The Changes tab on a source (Show all in Overview).
    pub fn open_changes_source(&mut self, source: DiffSource) {
        self.open_changes(None);
        self.set_changes_source(source);
    }

    /// Stops a task's agent (a kitten's Stop, wherever it is shown).
    pub fn stop_task(&mut self, id: &TaskId) {
        self.send(Request::StopTask { task: id.clone() });
    }

    /// Copy diff under the files: every file's lines, as one unified diff. Files not opened yet are asked for first and the copy is made
    /// when the last arrives.
    pub fn copy_all_diff(&mut self) {
        let Some(r) = &mut self.review else { return };
        let Some(d) = r.summary.clone() else { return };
        r.copy_wanted = true;
        let (task, run, source) = (r.task.clone(), r.run.clone(), r.source);
        let missing: Vec<String> = d.files.iter().filter(|f| !f.binary && !r.patches.contains_key(&f.path)).map(|f| f.path.clone()).collect();
        for p in missing {
            if let Some(id) = self.send(Request::GetDiffFile { task: task.clone(), run: run.clone(), path: p, full: false, source }) {
                self.asked.insert(id, Asked::Diff);
            }
        }
        self.copy_diff_if_ready();
    }

    /// The copy Copy diff waits for, once every file's lines are here.
    pub fn copy_diff_if_ready(&mut self) {
        let Some(r) = &mut self.review else { return };
        let Some(d) = &r.summary else { return };
        if !r.copy_wanted || d.files.iter().any(|f| !f.binary && !r.patches.contains_key(&f.path)) {
            return;
        }
        let mut out = String::new();
        for f in &d.files {
            out.push_str(&format!("--- {}\n+++ {}\n", f.old_path.as_deref().unwrap_or(&f.path), f.path));
            match r.patches.get(&f.path) {
                Some(p) => {
                    for l in &p.lines {
                        out.push_str(l);
                        out.push('\n');
                    }
                }
                None => out.push_str(&format!("{}\n", crate::review_view::file_note(f).unwrap_or_else(|| "Binary file".to_owned()))),
            }
        }
        r.copy_wanted = false;
        self.effects.push(crate::state::Effect::Copy(out));
    }

    /// The source switch: This run, All runs, Uncommitted.
    pub fn set_changes_source(&mut self, source: DiffSource) {
        let Some(root) = self.pane_root() else { return };
        match source {
            DiffSource::Run => match self.review.as_ref().filter(|r| r.task == root).and_then(|r| r.from.clone()).or_else(|| self.latest_changed_run(&root)) {
                Some(run) => self.load_changes(DiffSource::Run, Some(run)),
                None => self.load_changes(DiffSource::Thread, None),
            },
            other => self.load_changes(other, None),
        }
    }

    /// Asks for a source's files (its lines come as each file opens).
    pub fn load_changes(&mut self, source: DiffSource, run: Option<neko_protocol::RunId>) {
        let Some(root) = self.pane_root() else { return };
        let before = self.review.as_ref().filter(|r| r.task == root);
        let from = run.clone().or_else(|| before.and_then(|r| r.from.clone()));
        self.review = Some(crate::state::Review::new(root.clone(), source, run.clone(), from));
        if let Some(id) = self.send(Request::GetDiff { task: root, run, source }) {
            self.asked.insert(id, Asked::Diff);
        }
    }

    /// Each frame: what the visible tab needs from the daemon (the Overview, the Changes files, a kitten's transcript), asked once.
    pub fn pane_sync(&mut self) {
        if !self.pane_visible() {
            return;
        }
        let Some(root) = self.pane_root() else { return };
        match self.tab_of(&root) {
            Tab::Overview => {
                // The kittens' rows say which model each one ran on: that is in their runs, so their transcripts are read once.
                let kittens: Vec<TaskId> = self.tasks.iter().filter(|t| t.subagent.as_ref().is_some_and(|o| o.root == root)).map(|t| t.id.clone()).collect();
                for k in kittens {
                    self.ask_kitten_book(&k);
                }
                if (!self.pane.overview.contains_key(&root) || self.pane.stale.contains(&root)) && !self.pane.asked.contains(&root) && self.send(Request::GetOverview { task: root.clone() }).is_some() {
                    self.pane.stale.remove(&root);
                    self.pane.asked.insert(root);
                }
            }
            Tab::Changes => {
                if self.review.as_ref().is_none_or(|r| r.task != root) {
                    self.load_default_changes();
                } else if let Some(r) = self.review.as_ref().filter(|r| r.stale) {
                    let (source, run, open) = (r.source, r.run.clone(), r.open.clone());
                    self.load_changes(source, run);
                    if let Some(n) = &mut self.review {
                        n.open = open;
                    }
                }
            }
            Tab::Kitten(k) => self.ask_kitten_book(&k),
        }
    }

    /// The daemon's overview of a thread.
    pub fn take_overview(&mut self, o: Overview) {
        self.pane.asked.remove(&o.task);
        self.pane.overview.insert(o.task.clone(), o);
    }

    /// A run of a thread (or of a kitten of it) changed: the overview and the Changes tab are asked again once the run is not working.
    pub fn pane_run_changed(&mut self, run: &Run, had_changes: bool) {
        let root = self.root_of(&run.task);
        if run.status == RunStatus::Running {
            return;
        }
        self.pane.stale.insert(root.clone());
        self.pane.asked.remove(&root);
        if let Some(r) = &mut self.review
            && r.task == root
            && (r.source != DiffSource::Run || r.run.as_ref() == Some(&run.id))
        {
            r.stale = true;
        }
        let changed = run.undo.as_ref().is_some_and(neko_protocol::RunUndo::changed);
        if changed && !had_changes {
            self.pane_proactive(&root);
        }
    }

    /// The first kitten or the first changes of a thread open the pane by themselves, once, when the person is looking at that thread
    /// and has not closed the pane. A narrow window's sheet never opens unasked.
    pub fn pane_proactive(&mut self, root: &TaskId) {
        if self.pane_root().as_ref() != Some(root) || !self.pane.proactive.insert(root.clone()) {
            return;
        }
        if !self.pane_is_sheet() && self.pane.choice != Some(false) {
            self.pane.choice = Some(true);
        }
    }

    /// A task arrived or changed: a kitten of the thread in view opens the pane the first time.
    pub fn pane_task_seen(&mut self, task: &Task, known: bool) {
        if !known && let Some(o) = &task.subagent {
            let root = o.root.clone();
            self.pane_proactive(&root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figures_in_words() {
        assert_eq!(changes_words(3, 19, 7), "+19 \u{2212}7 \u{b7} 3 files");
        assert_eq!(changes_words(1, 1, 0), "+1 \u{2212}0 \u{b7} 1 file");
        assert_eq!(files_words(2), "2 files");
    }

    #[test]
    fn the_pane_follows_the_width_until_the_person_chooses() {
        let mut a = App::default();
        a.width = Some(1280.0);
        assert!(a.pane_docked_open(), "open from 1240");
        a.width = Some(1200.0);
        assert!(!a.pane_docked_open(), "closed between 1100 and 1240 until opened");
        a.pane.choice = Some(true);
        assert!(a.pane_docked_open());
        a.width = Some(1000.0);
        assert!(a.pane_is_sheet() && !a.pane_docked_open(), "under 1100 it is a sheet, never docked");
    }
}
