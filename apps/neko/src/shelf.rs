//! Pin, settle and archive in the window (docs/knowledge/threads.md): the requests, the toasts with Undo that follow them, ⌘Z, the
//! keys on a thread (⌘⇧P, ⌘⇧S), and moving between threads by number and by step (⌥1 to ⌥9, ⌘⌥↑ and ⌘⌥↓). The daemon decides and
//! pushes the task whole; nothing here changes a thread in the window before the daemon has.

use neko_protocol::{Request, Task, TaskId, ThreadStatus};

use crate::state::{Asked, App};
use crate::ui::toast::ToastSpec;

/// How long a toast with Undo stays: the spec's five seconds.
const TOAST_SECONDS: f64 = 5.0;

/// Which of the three the person asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shelf {
    Pin,
    Settle,
    Archive,
}

impl App {
    fn task_by(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.iter().find(|t| &t.id == id)
    }

    fn shelf_request(&mut self, id: &TaskId, shelf: Shelf, on: bool) {
        self.menu_open = None;
        let request = match shelf {
            Shelf::Pin => Request::PinTask { task: id.clone(), pinned: on },
            Shelf::Settle => Request::SettleTask { task: id.clone(), settled: on },
            Shelf::Archive => Request::ArchiveTask { task: id.clone(), archived: on },
        };
        if let Some(req) = self.send(request) {
            self.asked.insert(req, Asked::Shelf);
        }
    }

    /// Whether the thread can be settled or archived now (the daemon says no to a thread that works): the menus offer what can be done.
    pub fn can_shelve(&self, id: &TaskId, shelf: Shelf) -> bool {
        let Some(t) = self.task_by(id) else { return false };
        match shelf {
            Shelf::Pin => true,
            Shelf::Settle => t.thread_status != ThreadStatus::Working,
            Shelf::Archive => !matches!(t.thread_status, ThreadStatus::Working | ThreadStatus::Waiting),
        }
    }

    /// Pin it, or take it off the top (a toast with Undo says so).
    pub fn pin_thread(&mut self, id: &TaskId, pinned: bool) {
        let Some(t) = self.task_by(id) else { return };
        let title = t.title.clone();
        self.shelf_request(id, Shelf::Pin, pinned);
        if !pinned {
            self.toasts.push(ToastSpec::new("Unpinned").description(title).action("Undo", format!("repin:{}", id.0)).timeout(TOAST_SECONDS));
        }
    }

    /// Put it in its group's Settled fold, or bring it back. Settling a pinned thread unpins it; Undo puts the pin back too.
    pub fn settle_thread(&mut self, id: &TaskId, settled: bool) {
        let Some(t) = self.task_by(id) else { return };
        let (title, was_pinned) = (t.title.clone(), t.pinned);
        if settled && !self.can_shelve(id, Shelf::Settle) {
            return self.shelf_refused("It is working. Settle it when it stops.".to_owned());
        }
        self.shelf_request(id, Shelf::Settle, settled);
        if settled {
            self.toasts.push(ToastSpec::new("Settled").description(title).action("Undo", format!("unsettle:{}:{}", id.0, u8::from(was_pinned))).timeout(TOAST_SECONDS));
        }
    }

    /// Archive it (out of the lists until a message wakes it), or restore it without a run.
    pub fn archive_thread(&mut self, id: &TaskId, archived: bool) {
        let Some(t) = self.task_by(id) else { return };
        let title = t.title.clone();
        if archived && !self.can_shelve(id, Shelf::Archive) {
            return self.shelf_refused("It is still working. Stop it before archiving it.".to_owned());
        }
        self.shelf_request(id, Shelf::Archive, archived);
        if archived {
            self.toasts.push(ToastSpec::new("Archived").description(title).action("Undo", format!("unarchive:{}", id.0)).timeout(TOAST_SECONDS));
        }
    }

    /// A toast's action was pressed (or ⌘Z): the tag says what to take back.
    pub fn toast_action(&mut self, tag: &str) {
        let mut parts = tag.split(':');
        let (what, id) = (parts.next().unwrap_or(""), TaskId(parts.next().unwrap_or("").to_owned()));
        match what {
            "unsettle" => {
                self.shelf_request(&id, Shelf::Settle, false);
                if parts.next() == Some("1") {
                    self.shelf_request(&id, Shelf::Pin, true);
                }
            }
            "unarchive" => self.shelf_request(&id, Shelf::Archive, false),
            "repin" => self.shelf_request(&id, Shelf::Pin, true),
            _ => {}
        }
    }

    /// ⌘Z with a toast showing: its Undo, when the newest toast has one. False when there is nothing to take back.
    pub fn undo_toast(&mut self) -> bool {
        let Some((id, tag)) = self.toasts.items().filter_map(|(id, spec)| spec.action.as_ref().map(|(_, tag)| (id, tag.clone()))).last() else { return false };
        self.toasts.dismiss(id);
        self.toast_action(&tag);
        true
    }

    /// The daemon refused: said in a quiet toast, in its words.
    pub fn shelf_refused(&mut self, words: String) {
        self.announce = Some((words.clone(), false));
        self.toasts.push(ToastSpec::new(words).tone(crate::ui::base::Tone::Warning).timeout(TOAST_SECONDS));
    }

    /// The thread ⌘⇧P and ⌘⇧S act on: the one in view.
    pub fn thread_in_view(&self) -> Option<TaskId> {
        if self.settings_open || self.needs_open || self.rounds_open || self.briefing_open {
            return None;
        }
        self.open_task().filter(|t| t.is_thread()).map(|t| t.id.clone())
    }

    /// ⌘⇧P: pin the thread, or unpin it if it is pinned.
    pub fn toggle_pin(&mut self, id: &TaskId) {
        if let Some(pinned) = self.task_by(id).map(|t| t.pinned) {
            self.pin_thread(id, !pinned);
        }
    }

    /// ⌘⇧S: settle the thread, or bring it back if it is settled.
    pub fn toggle_settle(&mut self, id: &TaskId) {
        if let Some(settled) = self.task_by(id).map(|t| t.settled) {
            self.settle_thread(id, !settled);
        }
    }

    /// ⌥1 to ⌥9: the nth thread of the sidebar, in the order it is drawn.
    pub fn open_nth_thread(&mut self, n: usize) -> bool {
        let Some(id) = self.thread_order().get(n).cloned() else { return false };
        self.open_task_id(&id);
        true
    }

    /// ⌘⌥↑ and ⌘⌥↓: the thread before or after the one in view, in the order the sidebar draws them. With none in view, the first
    /// (or the last, going up).
    pub fn step_thread(&mut self, delta: i32) -> bool {
        let order = self.thread_order();
        if order.is_empty() {
            return false;
        }
        let at = self.thread_in_view().and_then(|id| order.iter().position(|x| *x == id));
        let next = match at {
            Some(i) => (i as i32 + delta).rem_euclid(order.len() as i32) as usize,
            None if delta < 0 => order.len() - 1,
            None => 0,
        };
        let id = order[next].clone();
        self.open_task_id(&id);
        true
    }

    /// View ▸ Show settled / Show archived: remembered for the next launch.
    pub fn set_show_settled(&mut self, on: bool) {
        self.menu_open = None;
        self.show_settled = on;
        self.save_bool(crate::SHOW_SETTLED_KEY, on);
    }

    pub fn set_show_archived(&mut self, on: bool) {
        self.menu_open = None;
        self.show_archived = on;
        self.save_bool(crate::SHOW_ARCHIVED_KEY, on);
    }

    fn save_bool(&self, key: &'static str, on: bool) {
        if self.persist {
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let _ = rune::prefs::Prefs::open(crate::PREFS_ID).set(key, &on);
            });
        }
    }

    /// A Settled (or Archived) fold pressed: open it, or fold it again.
    pub fn flip_settled(&mut self, group: &str) {
        if !self.settled_open.remove(group) {
            self.settled_open.insert(group.to_owned());
        }
    }
}
