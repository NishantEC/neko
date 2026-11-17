//! What the composers do beyond typing, ported from the web (`chat/ChatInputComposer.tsx`, `ChatInputSlash.ts`,
//! `ChatInputDrafts.ts`, `queue-tray.tsx`):
//!
//! * **`/` and `@` in Neko's ask box**: a popover above it. `/` lists the web's commands (`/plan` first), `@` the
//!   workspaces, the tool connections and the files the daemon finds (`/api/files`, 180 ms after the last key). Up and
//!   Down choose, Return or Tab accept, Esc closes; a command either runs, fills in its argument or switches Plan on.
//! * **Attachments**: "+" picks any file, a drop on the session region adds the files, each shows as a chip above the
//!   text (icon, name, size, ×) while it is uploaded (`POST /api/attachments`, the web's request) and its reference joins
//!   the message when it is sent.
//! * **The queue tray**: Neko's own messages that wait behind a reply, each with Edit (in place), Send now (stop the reply
//!   and send it first) and Cancel (`DiscardQueuedMessage`); ⌥↑ edits the last one and ⌘↵ stops the reply and sends.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use neko_protocol::workbench::ChatRole;
use rune::{Key, KeyEvent, TextEdit};

use crate::app::{App, Dispatch};
use crate::link::http::{Http, HttpError, Kept, MentionFile};
use crate::link::{Call, Summary};
use crate::work::PLAN_PREFIX;

// ------------------------------------------------------------------------------------------------ slash and mention

/// One command the `/` menu offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashEntry {
    pub name: &'static str,
    pub detail: &'static str,
    pub shortcut: Option<&'static str>,
}

/// `/plan` first, then the same list the native composer offers (the web's `PLAN_ENTRY` and `SLASH_ENTRIES`).
pub const PLAN_ENTRY: SlashEntry = SlashEntry { name: "/plan", detail: "Plan first, start after you approve", shortcut: None };

pub const SLASH_ENTRIES: [SlashEntry; 11] = [
    SlashEntry { name: "/stop", detail: "Stop all work (⌘⇧Esc)", shortcut: Some("⌘⇧Esc") },
    SlashEntry { name: "/remember", detail: "Remember a fact, e.g. /remember tests use pnpm", shortcut: None },
    SlashEntry { name: "/forget", detail: "Forget a memory, e.g. /forget pnpm", shortcut: None },
    SlashEntry { name: "/recall", detail: "Show what Neko remembers", shortcut: None },
    SlashEntry { name: "/memory", detail: "Open the Memory page", shortcut: None },
    SlashEntry { name: "/tickets", detail: "Open All tasks", shortcut: None },
    SlashEntry { name: "/clear", detail: "Clear finished tasks", shortcut: None },
    SlashEntry { name: "/models", detail: "Choose and check the agent model", shortcut: None },
    SlashEntry { name: "/permissions", detail: "What Neko is allowed to do on this Mac", shortcut: None },
    SlashEntry { name: "/setup", detail: "Show the first-run tour again", shortcut: None },
    SlashEntry { name: "/help", detail: "List these commands", shortcut: None },
];

/// Commands whose argument is typed after the name: choosing one only fills it in.
pub const TAKES_ARGUMENT: [&str; 2] = ["/remember", "/forget"];

/// The commands that are about Neko's own conversation (Plan first, what it remembers): the session's reply box does not offer them.
pub const NEKO_ONLY: [&str; 4] = ["/plan", "/remember", "/forget", "/recall"];

/// Commands matching a partial "/re…" draft; empty once a space is typed.
pub fn slash_suggestions(draft: &str) -> Vec<SlashEntry> {
    let text = draft.trim().to_lowercase();
    if !text.starts_with('/') || text.contains(' ') {
        return Vec::new();
    }
    std::iter::once(PLAN_ENTRY).chain(SLASH_ENTRIES).filter(|e| e.name.starts_with(&text)).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slash {
    Stop,
    Clear,
    Memory,
    Recall,
    Models,
    Setup,
    Permissions,
    Tickets,
    Help,
    Plan,
    Remember(String),
    Forget(String),
}

/// None for ordinary messages, including ones that merely contain a slash (the web's `parseSlash`).
pub fn parse_slash(draft: &str) -> Option<Slash> {
    let text = draft.trim();
    if !text.starts_with('/') {
        return None;
    }
    let name: String = text.chars().take_while(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    let argument = text[name.len()..].trim().to_owned();
    Some(match name.as_str() {
        "/stop" => Slash::Stop,
        "/clear" => Slash::Clear,
        "/memory" => Slash::Memory,
        "/remember" if !argument.is_empty() => Slash::Remember(argument),
        "/forget" if !argument.is_empty() => Slash::Forget(argument),
        "/recall" => Slash::Recall,
        "/models" | "/model" => Slash::Models,
        "/setup" => Slash::Setup,
        "/permissions" => Slash::Permissions,
        "/tickets" | "/steps" => Slash::Tickets,
        "/plan" => Slash::Plan,
        "/help" | "/" => Slash::Help,
        _ => return None,
    })
}

/// Commands that become an ordinary chat message (the daemon answers these in code, without a model).
pub fn slash_chat_text(command: &Slash) -> Option<String> {
    match command {
        Slash::Remember(a) => Some(format!("Remember that {a}")),
        Slash::Forget(a) => Some(format!("Forget {a}")),
        Slash::Recall => Some("What do you remember?".to_owned()),
        _ => None,
    }
}

/// The text that goes to the daemon (the web's `homeText`).
pub fn home_text(raw: &str, plan_only: bool) -> String {
    let command = parse_slash(raw);
    let text = command.as_ref().and_then(slash_chat_text).unwrap_or_else(|| raw.to_owned());
    if plan_only && command.is_none() { format!("{PLAN_PREFIX}{text}") } else { text }
}

/// The daemon's limit is in UTF-8 bytes, attachment references included.
pub const MESSAGE_LIMIT_BYTES: usize = 4096;

pub fn validation_error(text: &str) -> Option<String> {
    let bytes = text.len();
    (bytes > MESSAGE_LIMIT_BYTES).then(|| format!("Message is {bytes} bytes; the limit is {MESSAGE_LIMIT_BYTES}, including attachment references and context. Shorten it before sending."))
}

/// Swaps the trailing "@word" for `replacement` (the web's `replaceTrailingMention`).
pub fn replace_trailing_mention(text: &str, replacement: &str) -> String {
    let mut words: Vec<&str> = text.split(' ').collect();
    if let Some(last) = words.last_mut().filter(|w| w.starts_with('@')) {
        *last = replacement.trim();
    }
    let joined = words.join(" ");
    let joined = joined.trim();
    if joined.is_empty() { String::new() } else { format!("{joined} ") }
}

/// The "@word" being typed at the end of the draft, if any (without the "@").
pub fn mention_query(draft: &str) -> Option<String> {
    if draft.ends_with(' ') {
        return None;
    }
    let last = draft.rsplit(' ').next().unwrap_or("");
    last.strip_prefix('@').map(str::to_owned)
}

/// What a menu row does when accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Apply {
    /// Replace the trailing "@word".
    Mention(String),
    /// `/plan`: Plan on, the box emptied.
    Plan,
    /// Fill the box with this (a command that takes an argument).
    Fill(String),
    /// Put the command in the box and run it.
    Run(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Workspace,
    Tool,
    File,
    Command,
}

/// One row of the popover: 32 points tall, an icon, a title, a detail, an optional shortcut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRow {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub detail: String,
    pub shortcut: Option<String>,
    pub apply: Apply,
}

/// How many rows the popover shows without scrolling.
pub const VISIBLE_ROWS: usize = 8;
/// How many rows it holds (the web's `slice(0, 9)` for mentions).
pub const MAX_ROWS: usize = 9;

// ------------------------------------------------------------------------------------------------ attachments

/// Where an attachment goes: a composer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dest {
    /// The open session's reply box.
    Reply,
    /// Home's new-session box.
    New,
    /// Neko's ask box.
    Ask,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipState {
    Uploading,
    Ready { reference: String, is_image: bool },
    Failed(String),
}

/// A file attached to a draft: shown above the text until the message is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub state: ChipState,
}

/// "412 B", "3.2 KB", "1.4 MB".
pub fn size_words(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

/// A request that goes over HTTP, on a thread (`link/http.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpWork {
    Upload { dest: Dest, id: u64, name: String, path: PathBuf },
    Files { query: String, workspace: String },
}

/// What came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpDone {
    Uploaded { dest: Dest, id: u64, result: Result<Kept, HttpError> },
    Files { query: String, result: Result<Vec<MentionFile>, HttpError> },
}

/// Does the work on the calling thread. Reading the file happens here, not in a frame.
pub fn run_http(http: &Http, work: HttpWork) -> HttpDone {
    match work {
        HttpWork::Upload { dest, id, name, path } => {
            let result = crate::attachments::read_for_upload(&path).map_err(HttpError).and_then(|bytes| http.upload(&name, &bytes));
            HttpDone::Uploaded { dest, id, result }
        }
        HttpWork::Files { query, workspace } => {
            let result = http.files(&query, &workspace);
            HttpDone::Files { query, result }
        }
    }
}

// ------------------------------------------------------------------------------------------------ state

/// What the composers keep between frames.
pub struct ComposeUi {
    /// The row of the popover the arrow keys are on.
    pub index: usize,
    /// The draft the person pressed Esc on: the popover stays shut until it changes.
    pub dismissed: Option<String>,
    /// Files the daemon found for `files_for`.
    pub files: Vec<MentionFile>,
    pub files_for: Option<String>,
    pub files_loading: bool,
    /// The query the last frame saw, and when it changed (the 180 ms wait).
    pub seen_query: Option<String>,
    pub seen_at: f64,
    /// One calm line under the ask box.
    pub error: Option<String>,
    /// The queued message being edited in its row: its id and its words.
    pub editing: Option<(String, TextEdit)>,
    /// Files are being dragged over the session region.
    pub drop_over: bool,
    pub chips: HashMap<Dest, Vec<Chip>>,
    pub next_chip: u64,
    /// HTTP work waiting for the daemon's signed address, and (in a headless run with nowhere to send) what was asked.
    pub waiting: Vec<HttpWork>,
    pub asked: Vec<HttpWork>,
}

impl Default for ComposeUi {
    fn default() -> Self {
        ComposeUi {
            index: 0,
            dismissed: None,
            files: Vec::new(),
            files_for: None,
            files_loading: false,
            seen_query: None,
            seen_at: 0.0,
            error: None,
            editing: None,
            drop_over: false,
            chips: HashMap::new(),
            next_chip: 0,
            waiting: Vec::new(),
            asked: Vec::new(),
        }
    }
}

impl App {
    fn edit_of(&self, dest: Dest) -> &TextEdit {
        match dest {
            Dest::Reply => &self.composer,
            Dest::New => &self.new_request,
            Dest::Ask => &self.ask,
        }
    }

    /// The chips of one composer.
    pub fn chips(&self, dest: Dest) -> &[Chip] {
        self.compose.chips.get(&dest).map_or(&[], Vec::as_slice)
    }

    /// `base` and, under it, a reference for every file that is ready: what is sent (the web's `submission`).
    pub fn composed(&self, dest: Dest, base: &str) -> String {
        let refs: Vec<&str> = self.chips(dest).iter().filter_map(|c| if let ChipState::Ready { reference, .. } = &c.state { Some(reference.as_str()) } else { None }).collect();
        if refs.is_empty() {
            return base.to_owned();
        }
        if base.is_empty() { refs.join("\n") } else { format!("{base}\n{}", refs.join("\n")) }
    }

    /// Whether a file of this composer is still on its way to the daemon (it cannot be sent yet).
    pub fn uploading(&self, dest: Dest) -> bool {
        self.chips(dest).iter().any(|c| c.state == ChipState::Uploading)
    }

    /// Something to send: words, or a file that is ready.
    pub fn has_payload(&self, dest: Dest) -> bool {
        !self.edit_of(dest).text.trim().is_empty() || self.chips(dest).iter().any(|c| matches!(c.state, ChipState::Ready { .. }))
    }

    /// The files chosen with "+" or dropped: each becomes a chip and is uploaded.
    pub fn attach_files(&mut self, dest: Dest, paths: &[PathBuf]) {
        for path in paths {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "File".to_owned());
            self.compose.next_chip += 1;
            let id = self.compose.next_chip;
            let size = crate::attachments::size_of(path);
            let refusal = match size {
                Some(0) => Some(format!("{name} is empty.")),
                Some(n) if n as usize > crate::link::http::MAX_ATTACHMENT_BYTES => Some("Choose a file under 32 MB.".to_owned()),
                None => Some(format!("Couldn’t read {name}.")),
                _ => None,
            };
            let state = match refusal {
                Some(why) => ChipState::Failed(why),
                None => ChipState::Uploading,
            };
            let uploading = state == ChipState::Uploading;
            self.compose.chips.entry(dest).or_default().push(Chip { id, name: name.clone(), size: size.unwrap_or(0), state });
            if uploading {
                self.http_work(HttpWork::Upload { dest, id, name, path: path.clone() });
            }
        }
    }

    pub fn remove_chip(&mut self, dest: Dest, id: u64) {
        if let Some(list) = self.compose.chips.get_mut(&dest) {
            list.retain(|c| c.id != id);
        }
    }

    /// The message was taken: its files are done with.
    pub fn clear_chips(&mut self, dest: Dest) {
        self.compose.chips.remove(&dest);
    }

    /// Drops files on the session region: they go to the open session's reply box, or Home's new-session box.
    pub fn drop_files(&mut self, paths: &[PathBuf]) {
        self.compose.drop_over = false;
        let dest = if self.selected.is_some() && self.view.is_none() { Dest::Reply } else { Dest::New };
        self.attach_files(dest, paths);
    }

    /// Sends HTTP work: on a thread in the window, inline in a headless run against a daemon, or (with no daemon) onto the
    /// list a test reads. Waits for the daemon's signed address first.
    pub fn http_work(&mut self, work: HttpWork) {
        let Some(http) = self.http() else {
            self.compose.waiting.push(work);
            return;
        };
        match self.dispatch.clone() {
            Dispatch::None => self.compose.asked.push(work),
            Dispatch::Blocking { .. } => {
                let done = run_http(&http, work);
                self.http_done(done);
            }
            Dispatch::Async { handle, .. } => {
                let _ = std::thread::Builder::new().name("neko-http".into()).spawn(move || {
                    let done = run_http(&http, work);
                    handle.update_if(move |s, tx| {
                        tx.instant();
                        s.http_done(done)
                    });
                });
            }
        }
    }

    /// The daemon's address is known: what waited goes out.
    pub fn flush_http(&mut self) {
        for work in std::mem::take(&mut self.compose.waiting) {
            self.http_work(work);
        }
    }

    /// What came back over HTTP. True when something on screen changed.
    pub fn http_done(&mut self, done: HttpDone) -> bool {
        match done {
            HttpDone::Uploaded { dest, id, result } => {
                let Some(chip) = self.compose.chips.get_mut(&dest).and_then(|l| l.iter_mut().find(|c| c.id == id)) else { return false };
                chip.state = match result {
                    Ok(kept) => {
                        chip.name = kept.name;
                        ChipState::Ready { reference: kept.reference, is_image: kept.is_image }
                    }
                    Err(e) => ChipState::Failed(e.0),
                };
                true
            }
            HttpDone::Files { query, result } => {
                self.compose.files_loading = false;
                if self.compose.seen_query.as_deref() != Some(query.as_str()) {
                    return false;
                }
                self.compose.files_for = Some(query);
                self.compose.files = result.unwrap_or_default();
                true
            }
        }
    }

    // ---- the popover ------------------------------------------------------------------------------

    /// The rows the popover shows for the ask box right now (empty when it is shut).
    pub fn ask_rows(&self) -> Vec<MenuRow> {
        let text = self.ask.text.as_str();
        if self.compose.dismissed.as_deref() == Some(text) {
            return Vec::new();
        }
        if let Some(q) = mention_query(text) {
            let q = q.to_lowercase();
            let mut rows = Vec::new();
            let Some(m) = self.summary.as_ref() else { return rows };
            for w in &m.workspaces {
                if !q.is_empty() && !w.name.to_lowercase().contains(&q) {
                    continue;
                }
                rows.push(MenuRow { id: format!("w:{}", w.id), kind: ItemKind::Workspace, title: w.name.clone(), detail: "Workspace".into(), shortcut: None, apply: Apply::Mention(format!("Workspace: {}", w.name)) });
            }
            for (id, label, tools) in &m.connections {
                if !q.is_empty() && !label.to_lowercase().contains(&q) {
                    continue;
                }
                rows.push(MenuRow { id: format!("c:{id}"), kind: ItemKind::Tool, title: label.clone(), detail: format!("{tools} {}", if *tools == 1 { "tool" } else { "tools" }), shortcut: None, apply: Apply::Mention(format!("@{label} ")) });
            }
            for f in &self.compose.files {
                rows.push(MenuRow { id: format!("f:{}", f.path), kind: ItemKind::File, title: f.name.clone(), detail: f.location.clone(), shortcut: None, apply: Apply::Mention(format!("{} ", f.path)) });
            }
            rows.truncate(MAX_ROWS);
            return rows;
        }
        slash_suggestions(text)
            .into_iter()
            .map(|e| MenuRow {
                id: e.name.to_owned(),
                kind: ItemKind::Command,
                title: e.name.to_owned(),
                detail: e.detail.to_owned(),
                shortcut: e.shortcut.map(str::to_owned),
                apply: if e.name == "/plan" {
                    Apply::Plan
                } else if TAKES_ARGUMENT.contains(&e.name) {
                    Apply::Fill(format!("{} ", e.name))
                } else {
                    Apply::Run(e.name.to_owned())
                },
            })
            .collect()
    }

    /// The rows of the `/` popover above the session's reply box: the web's command list (`ChatInputSlash.ts`) without the ones
    /// that are Neko's own conversation. Empty when it is shut, when no session is open, or after Esc until the draft changes.
    /// (The web's task reply box has no `/` menu of its own; this is the list its Home composer has.)
    pub fn reply_rows(&self) -> Vec<MenuRow> {
        let text = self.composer.text.as_str();
        if self.selected.is_none() || self.view.is_some() || self.compose.dismissed.as_deref() == Some(text) {
            return Vec::new();
        }
        slash_suggestions(text)
            .into_iter()
            .filter(|e| !NEKO_ONLY.contains(&e.name))
            .map(|e| MenuRow { id: e.name.to_owned(), kind: ItemKind::Command, title: e.name.to_owned(), detail: e.detail.to_owned(), shortcut: e.shortcut.map(str::to_owned), apply: Apply::Run(e.name.to_owned()) })
            .collect()
    }

    /// Accepts a row of the session's popover: the command runs and the box is emptied.
    pub fn reply_apply(&mut self, row: &MenuRow) {
        if let Apply::Run(name) = &row.apply {
            self.composer.clear();
            if let Some(command) = parse_slash(name) {
                self.run_session_slash(&command);
            }
        }
        self.compose.index = 0;
    }

    /// Runs a command chosen in the session's reply box: the ask box's, except that the model is the session's own and Help
    /// writes `/` into this box.
    pub fn run_session_slash(&mut self, command: &Slash) {
        match command {
            Slash::Models => self.runtime_toggle(),
            Slash::Help => self.composer.replace_text("/"),
            other => self.run_slash(other),
        }
    }

    /// The keys of the session's reply box that belong to its `/` popover: Up and Down choose, Return and Tab accept, Esc closes.
    /// False when the key is the field's own (a Return then sends).
    pub fn reply_key(&mut self, ev: &KeyEvent) -> bool {
        if self.composer.is_composing() || ev.mods.cmd || ev.mods.ctrl || ev.mods.alt || ev.mods.shift {
            return false;
        }
        let rows = self.reply_rows();
        if rows.is_empty() {
            return false;
        }
        let active = self.ask_active(rows.len()).unwrap_or(0);
        match ev.key {
            Key::Down => self.compose.index = (active + 1) % rows.len(),
            Key::Up => self.compose.index = (active + rows.len() - 1) % rows.len(),
            Key::Enter | Key::Tab => self.reply_apply(&rows[active]),
            Key::Escape => self.compose.dismissed = Some(self.composer.text.clone()),
            _ => return false,
        }
        true
    }

    /// The row the arrow keys are on (None when the popover is shut).
    pub fn ask_active(&self, rows: usize) -> Option<usize> {
        (rows > 0).then(|| self.compose.index.min(rows - 1))
    }

    /// Accepts a row of the popover.
    pub fn ask_apply(&mut self, row: &MenuRow) {
        match &row.apply {
            Apply::Mention(replacement) => {
                let text = replace_trailing_mention(&self.ask.text, replacement);
                self.ask.replace_text(text);
            }
            Apply::Plan => {
                self.ask_plan = true;
                self.ask.replace_text("");
            }
            Apply::Fill(text) => self.ask.replace_text(text.clone()),
            Apply::Run(name) => {
                self.ask.replace_text(name.clone());
                self.ask_neko();
            }
        }
        self.compose.index = 0;
    }

    /// The keys of the ask box, before it types: the popover's arrows, Return and Tab, Esc; ⌘↵ stops the reply and sends;
    /// ⌥↑ edits the last queued message; Return sends. False when the key is the field's own.
    pub fn ask_key(&mut self, ev: &KeyEvent) -> bool {
        if self.ask.is_composing() {
            return false;
        }
        let plain = !(ev.mods.cmd || ev.mods.ctrl || ev.mods.alt || ev.mods.shift);
        let rows = self.ask_rows();
        if !rows.is_empty() && plain {
            let active = self.ask_active(rows.len()).unwrap_or(0);
            match ev.key {
                Key::Down => {
                    self.compose.index = (active + 1) % rows.len();
                    return true;
                }
                Key::Up => {
                    self.compose.index = (active + rows.len() - 1) % rows.len();
                    return true;
                }
                Key::Enter | Key::Tab => {
                    self.ask_apply(&rows[active]);
                    return true;
                }
                Key::Escape => {
                    self.compose.dismissed = Some(self.ask.text.clone());
                    return true;
                }
                _ => {}
            }
        }
        if ev.key == Key::Enter && !ev.mods.shift {
            if ev.mods.cmd || ev.mods.ctrl {
                // Interrupt and send: the reply stops and this goes first; with nothing typed, the last queued one does.
                if self.has_payload(Dest::Ask) {
                    self.ask_neko_now();
                } else if let Some((id, text)) = self.queued_chat().pop() {
                    self.steer_queued(&id, &text);
                }
            } else if !ev.mods.alt {
                self.ask_neko();
            }
            return true;
        }
        if ev.key == Key::Up
            && ev.mods.alt
            && let Some((id, text)) = self.queued_chat().pop()
        {
            self.start_edit_queued(&id, &text);
            return true;
        }
        false
    }

    /// The `@` file search, a beat after the last key: asks the daemon for files named like the query.
    pub fn tick_compose(&mut self, t: f64, tx: &mut rune::Tx) {
        let query = mention_query(&self.ask.text);
        if query != self.compose.seen_query {
            self.compose.seen_query = query.clone();
            self.compose.seen_at = t;
            if query.as_ref().is_none_or(|q| q.chars().count() < 2) {
                self.compose.files.clear();
                self.compose.files_for = None;
            }
        }
        let Some(q) = query.filter(|q| q.chars().count() >= 2) else { return };
        if self.compose.files_for.as_deref() == Some(q.as_str()) || self.compose.files_loading {
            return;
        }
        if t - self.compose.seen_at < 0.18 {
            tx.wake_after(std::time::Duration::from_millis(200));
            return;
        }
        self.compose.files_loading = true;
        let workspace = self.workspace_scope.clone().unwrap_or_default();
        self.http_work(HttpWork::Files { query: q, workspace });
    }

    // ---- sending ----------------------------------------------------------------------------------

    /// Neko's own messages still waiting behind a reply, oldest first: (id, words).
    pub fn queued_chat(&self) -> Vec<(String, String)> {
        let Some(m) = self.summary.as_ref() else { return Vec::new() };
        m.conversation.iter().filter(|c| c.queued && c.role == ChatRole::User).map(|c| (c.id.clone(), parts_text(&c.parts))).collect()
    }

    pub fn start_edit_queued(&mut self, id: &str, text: &str) {
        let mut edit = TextEdit::new(text.to_owned());
        edit.focused = true;
        self.compose.editing = Some((id.to_owned(), edit));
    }

    /// Enter on an edited queued row: the old message goes and the new words queue behind the reply.
    pub fn finish_edit_queued(&mut self) {
        let Some((id, edit)) = self.compose.editing.take() else { return };
        let text = edit.text.trim().to_owned();
        if text.is_empty() {
            self.call(Call::DiscardQueued { message_id: id });
        } else {
            let workspace_id = self.scope().map(str::to_owned);
            self.call(Call::EditQueued { message_id: id, text, workspace_id });
        }
    }

    pub fn cancel_edit_queued(&mut self) {
        self.compose.editing = None;
    }

    pub fn discard_queued(&mut self, id: &str) {
        self.call(Call::DiscardQueued { message_id: id.to_owned() });
    }

    /// "Send now": the message leaves the queue and goes first, stopping the reply.
    pub fn steer_queued(&mut self, id: &str, text: &str) {
        let workspace_id = self.scope().map(str::to_owned);
        self.call(Call::SteerQueued { message_id: id.to_owned(), text: text.to_owned(), workspace_id });
    }

    /// What the ask box sends: the words and their files, behind the Plan prefix when Plan is on (the web's `homeText`).
    pub fn ask_payload(&self) -> String {
        home_text(&self.composed(Dest::Ask, self.ask.text.trim()), self.ask_plan)
    }

    /// Runs a command typed in the ask box.
    pub fn run_slash(&mut self, command: &Slash) {
        match command {
            Slash::Stop => self.ask_stop_all(),
            Slash::Clear => {
                self.ask_clear_finished();
            }
            Slash::Memory => self.open_in_web(Some("memory")),
            Slash::Tickets => self.open_all_sessions(),
            Slash::Models => self.runtime_toggle_home(),
            Slash::Permissions => self.open_view(crate::views::FullView::Settings),
            Slash::Setup => self.open_in_web(None),
            Slash::Help => {
                self.ask.replace_text("/");
            }
            Slash::Plan => self.ask_plan = true,
            Slash::Remember(_) | Slash::Forget(_) | Slash::Recall => {}
        }
    }

    /// Return in the ask box: a command runs, anything else is sent (queued by the daemon while a reply runs).
    pub fn ask_neko(&mut self) {
        self.send_ask(false);
    }

    /// ⌘↵: stops the reply and sends this first.
    pub fn ask_neko_now(&mut self) {
        self.send_ask(true);
    }

    fn send_ask(&mut self, interrupt: bool) {
        let raw = self.ask.text.trim().to_owned();
        if raw.is_empty() && !self.has_payload(Dest::Ask) {
            return;
        }
        if self.uploading(Dest::Ask) {
            self.compose.error = Some("Wait for the files to finish uploading, then send.".to_owned());
            return;
        }
        if let Some(command) = parse_slash(&raw)
            && slash_chat_text(&command).is_none()
        {
            self.ask.clear();
            self.run_slash(&command);
            return;
        }
        let payload = self.ask_payload();
        if let Some(why) = validation_error(&payload) {
            self.compose.error = Some(why);
            return;
        }
        self.compose.error = None;
        if self.work.busy.insert("ask".to_owned()) {
            let workspace_id = self.scope().map(str::to_owned);
            self.call(if interrupt { Call::AskNow { text: payload, workspace_id } } else { Call::Ask { text: payload, workspace_id } });
        }
    }

    /// A reply to `Ask` was taken: the box and its files are done with.
    pub fn ask_taken(&mut self, text: &str) {
        if self.ask_payload() == text {
            self.ask.clear();
            self.clear_chips(Dest::Ask);
        }
    }

    /// "Ask Neko about this" (the right-click menu on selected text): Neko's column opens with the words quoted in its composer
    /// (the web's `askNeko`).
    pub fn ask_neko_about(&mut self, words: &str) {
        let quoted = format!("{}\n\n", words.trim().lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n"));
        self.layout.set_neko_open(true);
        self.ask.replace_text(quoted);
        self.ask.focused = true;
    }

    /// "Quote in reply": the text quoted into a composer, as the web's `quote`.
    pub fn quote_into_composer(&mut self, text: &str, in_neko: bool) {
        let quoted = format!("{}\n\n", text.trim().lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n"));
        let dest = if !in_neko && self.selected.is_some() && self.view.is_none() { Dest::Reply } else { Dest::Ask };
        self.into_composer(dest, &quoted);
    }

    /// "Edit and send again": the message's words go back into the composer it came from.
    pub fn edit_and_send_again(&mut self, text: &str, in_neko: bool) {
        let edit = if in_neko { &mut self.ask } else { &mut self.composer };
        edit.replace_text(text.to_owned());
        edit.focused = true;
    }

    /// Puts `text` in a composer after what is there.
    pub fn into_composer(&mut self, dest: Dest, text: &str) {
        let edit = match dest {
            Dest::Reply => &mut self.composer,
            Dest::New => &mut self.new_request,
            Dest::Ask => &mut self.ask,
        };
        let joined = if edit.text.trim().is_empty() { text.to_owned() } else { format!("{}\n{text}", edit.text.trim_end()) };
        edit.replace_text(joined);
        edit.focused = true;
    }
}

/// The words of a message's `text` parts.
pub fn parts_text(parts: &[serde_json::Value]) -> String {
    parts.iter().filter(|p| p["type"] == "text").filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("")
}

/// A path's extension as the chip's icon would pick it: images get a picture, the rest a page.
pub fn is_image_name(name: &str) -> bool {
    matches!(Path::new(name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("png" | "jpg" | "jpeg" | "gif" | "webp"))
}

impl Summary {
    /// The messages of Neko's chat that wait behind a reply.
    pub fn queued_chat(&self) -> usize {
        self.conversation.iter().filter(|c| c.queued && c.role == ChatRole::User).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(draft: &str) -> Vec<&'static str> {
        slash_suggestions(draft).into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn a_slash_lists_plan_first_then_the_webs_commands_and_narrows_as_you_type() {
        let all = names("/");
        assert_eq!(all.len(), 12);
        assert_eq!(all[0], "/plan");
        assert_eq!(&all[1..4], ["/stop", "/remember", "/forget"]);
        assert_eq!(names("/re"), ["/remember", "/recall"]);
        assert_eq!(names("/RE"), ["/remember", "/recall"], "case does not matter");
        assert!(names("/remember x").is_empty(), "a space ends the menu");
        assert!(names("hello").is_empty());
    }

    #[test]
    fn commands_parse_as_the_web_parses_them() {
        assert_eq!(parse_slash("/stop"), Some(Slash::Stop));
        assert_eq!(parse_slash("  /Models "), Some(Slash::Models));
        assert_eq!(parse_slash("/model"), Some(Slash::Models));
        assert_eq!(parse_slash("/steps"), Some(Slash::Tickets));
        assert_eq!(parse_slash("/remember tests use pnpm"), Some(Slash::Remember("tests use pnpm".into())));
        assert_eq!(parse_slash("/remember"), None, "an argument is needed");
        assert_eq!(parse_slash("/"), Some(Slash::Help));
        assert_eq!(parse_slash("/unknown"), None);
        assert_eq!(parse_slash("not /stop"), None, "a slash inside a message is a message");
    }

    #[test]
    fn some_commands_become_chat_messages_and_plan_wraps_everything_else() {
        assert_eq!(home_text("/remember tests use pnpm", false), "Remember that tests use pnpm");
        assert_eq!(home_text("/forget pnpm", false), "Forget pnpm");
        assert_eq!(home_text("/recall", true), "What do you remember?", "a command never takes the plan prefix");
        assert!(home_text("fix the bug", true).starts_with(PLAN_PREFIX));
        assert_eq!(home_text("fix the bug", false), "fix the bug");
    }

    #[test]
    fn the_message_limit_counts_utf8_bytes() {
        assert!(validation_error(&"a".repeat(4096)).is_none());
        assert!(validation_error(&"é".repeat(2049)).unwrap().contains("4098 bytes"));
    }

    #[test]
    fn a_mention_swaps_the_trailing_word() {
        assert_eq!(mention_query("look at @rea"), Some("rea".into()));
        assert_eq!(mention_query("look at @rea "), None);
        assert_eq!(mention_query("no mention"), None);
        assert_eq!(replace_trailing_mention("look at @rea", "Workspace: Neko"), "look at Workspace: Neko ");
        assert_eq!(replace_trailing_mention("@x", "@Linear "), "@Linear ");
    }

    #[test]
    fn sizes_read_as_a_person_says_them() {
        assert_eq!(size_words(412), "412 B");
        assert_eq!(size_words(3300), "3.2 KB");
        assert_eq!(size_words(1_500_000), "1.4 MB");
    }
}
