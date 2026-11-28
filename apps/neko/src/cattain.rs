//! A Cattain in the app (docs/knowledge/cats.md, "Cattains"): opening one opens its own thread, its files as the daemon last said, the
//! words the person is editing in RULES.md and CREW.md, and the changes made from its Profile. The drawing is `cattain_view.rs`.

use std::collections::{HashMap, HashSet};

use neko_protocol::cats::{self, CattainFile};
use neko_protocol::{Cat, PermissionMode, Reply, Request, Task, TaskId};
use rune::TextEdit;

use crate::pane::Tab;
use crate::state::{App, Asked, Refusal};

/// A file of a Cattain, by the cat's id and its name.
pub type FileKey = (String, String);

#[derive(Default)]
pub struct Cattains {
    /// Each file as the daemon last said.
    pub files: HashMap<FileKey, CattainFile>,
    /// RULES.md and CREW.md as the person is editing them (Save sends them; Revert puts the saved words back).
    pub drafts: HashMap<FileKey, TextEdit>,
    /// The file the Files tab shows, by cat (RULES.md at first).
    pub chosen: HashMap<String, String>,
    /// Files asked for and not answered yet.
    pub asked: HashSet<FileKey>,
    /// The Profile's purpose field, by cat.
    pub purpose: HashMap<String, TextEdit>,
    /// The Cattain whose thread is being made (asked with `open_cattain`), and the tab to show once it opens.
    pub opening: Option<(String, Tab)>,
}

/// What each file is for, in a line (the Files tab and the Overview's strip).
pub fn file_words(name: &str) -> &'static str {
    match name {
        cats::RULES => "Its standing rules, read on every run",
        cats::CREW => "Read by every thread in its crew",
        cats::NOTES => "Its own plan, kept by it",
        cats::DIGEST => "What happened, kept by it for you",
        _ => "",
    }
}

impl App {
    /// The cat whose own thread this is.
    pub fn cattain_of(&self, task: &TaskId) -> Option<&Cat> {
        let t = self.tasks.iter().find(|t| &t.id == task).filter(|t| t.cattain)?;
        t.cat.as_deref().and_then(|c| self.cat(c))
    }

    /// The Cattain whose thread is in view (the pane's thread), if it is one.
    pub fn open_cattain_cat(&self) -> Option<&Cat> {
        self.pane_root().and_then(|r| self.cattain_of(&r))
    }

    /// The Cattain's own thread, when the app has it.
    pub fn cattain_thread(&self, cat: &str) -> Option<&Task> {
        let id = self.cat(cat)?.thread.clone()?;
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Opens a Cattain: its own thread (made by the daemon the first time), with the pane on `tab`.
    pub fn open_cattain_on(&mut self, cat: &str, tab: Tab) {
        self.menu_open = None;
        if let Some(id) = self.cattain_thread(cat).map(|t| t.id.clone()) {
            self.open_task_id(&id);
            self.pane.tab.insert(id, tab.clone());
            if tab != Tab::Overview {
                self.show_pane();
            }
            return;
        }
        if let Some(id) = self.send(Request::OpenCattain { cat: cat.to_owned() }) {
            self.asked.insert(id, Asked::OpenCattain);
            self.cattains.opening = Some((cat.to_owned(), tab));
        }
    }

    /// Opens a Cattain on its Overview (the sidebar, Search, Settings ▸ Cattains).
    pub fn open_cattain(&mut self, cat: &str) {
        self.open_cattain_on(cat, Tab::Overview);
    }

    /// The Files tab on one file.
    pub fn open_cattain_file(&mut self, cat: &str, name: &str) {
        self.cattains.chosen.insert(cat.to_owned(), name.to_owned());
        self.open_cattain_on(cat, Tab::Files);
    }

    /// The file the Files tab shows for a cat.
    pub fn chosen_file(&self, cat: &str) -> String {
        self.cattains.chosen.get(cat).cloned().unwrap_or_else(|| cats::RULES.to_owned())
    }

    pub fn choose_file(&mut self, cat: &str, name: &str) {
        self.cattains.chosen.insert(cat.to_owned(), name.to_owned());
        self.ask_file(cat, name);
    }

    /// Asks for a file once, until it is answered (the pane's tabs ask for what they show).
    pub fn ask_file(&mut self, cat: &str, name: &str) {
        let key = (cat.to_owned(), name.to_owned());
        if self.cattains.files.contains_key(&key) || self.cattains.asked.contains(&key) {
            return;
        }
        if let Some(id) = self.send(Request::GetCattainFile { cat: cat.to_owned(), name: name.to_owned() }) {
            self.asked.insert(id, Asked::CattainFile);
            self.cattains.asked.insert(key);
        }
    }

    /// A file as the daemon says it now. The editor follows it, unless the person has unsaved words in it.
    pub fn take_cattain_file(&mut self, f: CattainFile) {
        let key = (f.cat.clone(), f.name.clone());
        self.cattains.asked.remove(&key);
        let unsaved = self.file_unsaved(&key.0, &key.1);
        if !unsaved {
            let focused = self.cattains.drafts.get(&key).is_some_and(|d| d.focused);
            let mut fresh = TextEdit::new(&f.text);
            fresh.focused = focused;
            self.cattains.drafts.insert(key.clone(), fresh);
        }
        self.cattains.files.insert(key, f);
    }

    /// The editor's words for RULES.md or CREW.md.
    pub fn file_draft_mut(&mut self, cat: &str, name: &str) -> &mut TextEdit {
        let key = (cat.to_owned(), name.to_owned());
        let saved = self.cattains.files.get(&key).map(|f| TextEdit::new(&f.text)).unwrap_or_default();
        self.cattains.drafts.entry(key).or_insert(saved)
    }

    /// The person has words in the editor that are not saved.
    pub fn file_unsaved(&self, cat: &str, name: &str) -> bool {
        let key = (cat.to_owned(), name.to_owned());
        match (self.cattains.drafts.get(&key), self.cattains.files.get(&key)) {
            (Some(d), Some(f)) => d.text != f.text,
            _ => false,
        }
    }

    fn cattain_request(&mut self, r: Request) {
        if let Some(id) = self.send(r) {
            self.asked.insert(id, Asked::CattainFile);
        }
    }

    /// Save in the Files tab.
    pub fn save_file(&mut self, cat: &str, name: &str) {
        let text = self.file_draft_mut(cat, name).text.clone();
        self.cattain_request(Request::SaveCattainFile { cat: cat.to_owned(), name: name.to_owned(), text });
    }

    /// Revert: the saved words back in the editor.
    pub fn revert_file(&mut self, cat: &str, name: &str) {
        let key = (cat.to_owned(), name.to_owned());
        if let Some(f) = self.cattains.files.get(&key) {
            self.cattains.drafts.insert(key, TextEdit::new(&f.text));
        }
    }

    /// Restore previous: the daemon swaps the two versions; the editor follows.
    pub fn restore_file(&mut self, cat: &str, name: &str) {
        self.cattains.drafts.remove(&(cat.to_owned(), name.to_owned()));
        self.cattain_request(Request::RestoreCattainFile { cat: cat.to_owned(), name: name.to_owned() });
    }

    /// The Profile's purpose field.
    pub fn purpose_mut(&mut self, cat: &str) -> &mut TextEdit {
        let saved = self.cat(cat).and_then(|c| c.purpose.clone()).unwrap_or_default();
        self.cattains.purpose.entry(cat.to_owned()).or_insert_with(|| TextEdit::new(&saved))
    }

    /// Return in the purpose field, or leaving it: saved when it changed.
    pub fn save_purpose(&mut self, cat: &str) {
        let words = self.purpose_mut(cat).text.split_whitespace().collect::<Vec<_>>().join(" ");
        if self.cat(cat).is_some_and(|c| c.purpose.clone().unwrap_or_default() != words) {
            self.cat_change(Request::SetCatPurpose { cat: cat.to_owned(), purpose: words });
        }
    }

    pub fn set_cat_model(&mut self, cat: &str, model: Option<String>) {
        self.menu_open = None;
        self.cat_change(Request::SetCatModel { cat: cat.to_owned(), model });
    }

    pub fn set_cat_access(&mut self, cat: &str, access: Option<PermissionMode>) {
        self.menu_open = None;
        self.cat_change(Request::SetCatAccess { cat: cat.to_owned(), access });
    }

    /// New thread, in the Cattain's crew: the screen opens with its crew chip.
    pub fn new_crew_thread(&mut self, cat: &str) {
        self.go_new_thread();
        self.new_thread.crew = Some(cat.to_owned());
        if let Some(ws) = self.cat(cat).and_then(|c| c.workspace.clone()).filter(|w| self.workspace_of(w).is_some_and(|w| w.is_active())) {
            self.choose_workspace(&ws);
        }
        self.draft.focused = true;
    }

    /// A Cattain an agent just made: the toast "Made yuzu", its purpose under it, with Undo (⌘Z too) for five seconds.
    pub fn cattain_made_toast(&mut self, cat: &Cat) {
        let mut spec = crate::ui::toast::ToastSpec::new(format!("Made {}", cat.name)).action("Undo", format!("unmake:{}", cat.id)).timeout(crate::shelf::TOAST_SECONDS);
        if let Some(p) = cat.purpose.as_ref().filter(|p| !p.is_empty()) {
            spec = spec.description(format!("A Cattain for {p}"));
        }
        self.toasts.push(spec);
    }

    /// The toast's Undo: the daemon retires the Cattain while it has no work yet, and says why not in a toast otherwise.
    pub fn undo_make_cattain(&mut self, cat: &str) {
        if let Some(id) = self.send(Request::UndoMakeCattain { cat: cat.to_owned() }) {
            self.asked.insert(id, Asked::Shelf);
        }
    }

    /// Settings ▸ Cattains: "Agents may make Cattains" (on unless the person keeps them to making by hand).
    pub fn set_agents_make_cattains(&mut self, on: bool) {
        if let Some((_, s)) = &self.runtimes {
            let settings = neko_protocol::Settings { cattains_by_hand: !on, ..s.clone() };
            self.send(Request::SetSettings { settings });
        }
    }

    /// The replies this module asked for. True when the reply was one of them.
    pub(crate) fn cattain_reply(&mut self, asked: &Option<Asked>, reply: &Reply) -> bool {
        match (asked, reply) {
            (Some(Asked::OpenCattain), Reply::Task { task }) => {
                let id = task.id.clone();
                self.upsert(task.clone());
                // The cat may not have heard of its thread yet: it says so in the event that follows.
                if let Some(c) = task.cat.as_deref().and_then(|c| self.cats.iter_mut().find(|x| x.id == c)) {
                    c.thread = Some(id.clone());
                }
                let tab = self.cattains.opening.take().map_or(Tab::Overview, |(_, tab)| tab);
                self.open_task_id(&id);
                if tab != Tab::Overview {
                    self.show_pane();
                }
                self.pane.tab.insert(id, tab);
                true
            }
            (Some(Asked::CattainFile), Reply::CattainFile { file }) => {
                self.take_cattain_file(file.clone());
                true
            }
            (Some(Asked::OpenCattain | Asked::CattainFile), Reply::Error { message, .. }) => {
                self.cattains.opening = None;
                self.cattains.asked.clear();
                self.refusal = Some(Refusal { title: "Neko could not do that".into(), words: message.clone(), reveal: None });
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_says_what_it_is_for() {
        assert!(cats::FILES.iter().all(|f| !file_words(f).is_empty()));
    }
}
