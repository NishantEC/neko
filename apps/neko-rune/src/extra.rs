//! What parity batch D keeps and does: the rest of Settings (themes, the summon shortcut, the exact model check, Full
//! power, the clipboard switch, diagnostics, the update source, the quick panel's folders, launch at login, the
//! rulebook), Usage's machine reading, and the guard that stops a half-written rulebook being lost. The wire is
//! `link/extra.rs`; the views are `view/settings_d.rs` and `view/more.rs`. Behaviour is the web's
//! `pages/setup/settings/*.tsx`, `theme.ts` and `themes/*`.

use std::rc::Rc;

use neko_protocol::agent_models::{DiagnosticCheck, ModelCheck};
use neko_protocol::workbench::MachineHealth;
use neko_protocol::{HotkeyCombo, HotkeyConfig, Modifier};

use crate::app::App;
use crate::link::{Call, ExtraAnswer, ExtraCall, ProviderRow};
use crate::pages::Section;
use crate::themes::{self, ThemeEntry};
use crate::views::FullView;

/// Where the person was going when a half-written rulebook stopped them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Going {
    Section(Section),
    View(FullView),
    Session(String),
    Home,
}

/// A line under a control, with whether it is good news.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Say {
    pub ok: bool,
    pub words: String,
}

#[derive(Default)]
pub struct ExtraState {
    pub started: bool,
    /// The person's theme files as `(id, file, theme)`, the folder they live in, and the files skipped with why.
    pub themes_dir: Option<String>,
    pub user_themes: Vec<(String, String, serde_json::Value)>,
    pub theme_problems: Vec<(String, String)>,
    pub hotkey: Option<HotkeyConfig>,
    pub recording: bool,
    pub hotkey_say: Option<Say>,
    pub model_check: Option<ModelCheck>,
    pub checking: bool,
    pub custom_model_open: bool,
    pub super_agent: Option<bool>,
    pub clipboard: Option<bool>,
    pub diag: Option<(u64, Vec<DiagnosticCheck>)>,
    pub diag_running: bool,
    /// The machine's last reading and when it was taken, by the app's clock.
    pub machine: Option<(MachineHealth, i64)>,
    pub folders: Option<Vec<ProviderRow>>,
    pub login: Option<bool>,
    /// Where the rulebook guard is sending the person if they discard.
    pub going: Option<Going>,
    /// Routes whose signed address was asked for to open in the browser (Open in browser on a Canvas row).
    pub browser_routes: Vec<String>,
    /// When the embedded page last spoke, by the app's clock (a page that floods is ignored).
    pub last_said_ms: i64,
}

// ---- pure rules --------------------------------------------------------------------------------

/// The web's `validId` in Settings, AI: an exact model id is 1 to 120 characters of letters, digits and `. - _ : / @ [ ]`, never leading with `-`.
pub fn valid_model_id(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= 120 && !id.starts_with('-') && id.chars().all(|c| c.is_ascii_alphanumeric() || ".-_:/@[]".contains(c))
}

/// `owner/name` from `owner/name`, a github.com address (with or without `.git`) or an ssh remote; None for anything else.
/// The daemon's own rule (its self-update module's `normalize_repository`), so a refusal is caught before it is sent.
pub fn normalize_repository(text: &str) -> Option<String> {
    let mut value = text.trim();
    for prefix in ["https://github.com/", "http://github.com/", "git@github.com:", "github.com/"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest;
            break;
        }
    }
    let value = value.trim_end_matches('/');
    let value = value.strip_suffix(".git").unwrap_or(value);
    let parts: Vec<&str> = value.split('/').collect();
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    (parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(allowed))).then(|| value.to_owned())
}

/// The shortcut a key press makes, named as the daemon's `HotkeyCombo` names it (`Space`, `KeyA`, `Digit1`). None for a press that
/// is not a shortcut: no modifier, or a key with no name here.
pub fn combo_of(ev: &rune::KeyEvent) -> Option<HotkeyCombo> {
    use rune::Key;
    let name = match &ev.key {
        Key::Text(t) if t == " " => "Space".to_owned(),
        Key::Text(t) => key_name(t.chars().next()?)?,
        Key::Command(c) => key_name(*c)?,
        Key::Enter => "Enter".to_owned(),
        Key::Tab => "Tab".to_owned(),
        Key::Up => "ArrowUp".to_owned(),
        Key::Down => "ArrowDown".to_owned(),
        Key::Left => "ArrowLeft".to_owned(),
        Key::Right => "ArrowRight".to_owned(),
        _ => return None,
    };
    let mut modifiers = Vec::new();
    if ev.mods.cmd {
        modifiers.push(Modifier::Cmd);
    }
    if ev.mods.alt {
        modifiers.push(Modifier::Alt);
    }
    if ev.mods.ctrl {
        modifiers.push(Modifier::Ctrl);
    }
    if ev.mods.shift {
        modifiers.push(Modifier::Shift);
    }
    (!modifiers.is_empty() && !(modifiers == [Modifier::Shift])).then(|| HotkeyCombo::new(modifiers, name))
}

fn key_name(c: char) -> Option<String> {
    match c {
        'a'..='z' | 'A'..='Z' => Some(format!("Key{}", c.to_ascii_uppercase())),
        '0'..='9' => Some(format!("Digit{c}")),
        ' ' => Some("Space".to_owned()),
        _ => None,
    }
}

/// A shortcut as the person reads it: `⌥ Space`, `⌘⇧ K`.
pub fn combo_words(combo: &HotkeyCombo) -> String {
    let mut mods = String::new();
    for m in [Modifier::Cmd, Modifier::Alt, Modifier::Ctrl, Modifier::Shift] {
        if combo.modifiers.contains(&m) {
            mods.push(match m {
                Modifier::Cmd => '⌘',
                Modifier::Alt => '⌥',
                Modifier::Ctrl => '⌃',
                Modifier::Shift => '⇧',
            });
        }
    }
    let key = combo.key.strip_prefix("Key").or_else(|| combo.key.strip_prefix("Digit")).unwrap_or(&combo.key);
    let key = match key {
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        other => other,
    };
    if mods.is_empty() { key.to_owned() } else { format!("{mods} {key}") }
}

/// What "Make me a theme" tells Neko (`web/src/themes/prompt.ts`, `THEME_PROMPT`): one constant, filled in at send time. A test reads the
/// web's file and requires this to be the same text.
pub const THEME_PROMPT: &str = include_str!("theme_prompt.txt");

const LIMIT_BYTES: usize = 4096;
const FALLBACK_DIR: &str = "<data dir>/themes";

fn fill(request: &str, dir: Option<&str>) -> String {
    THEME_PROMPT.replace("{{THEMES_DIR}}", dir.unwrap_or(FALLBACK_DIR)).replace("{{REQUEST}}", request)
}

/// How many bytes the request may use once the rest of the prompt is in (`themeRequestBudget`).
pub fn theme_budget(dir: Option<&str>) -> usize {
    LIMIT_BYTES.saturating_sub(fill("", dir).len())
}

pub fn theme_prompt(wish: &str, dir: Option<&str>) -> String {
    fill(wish.trim(), dir)
}

/// The web's rulebook rule: the text shown is the person's own, or Neko's default when they have none.
pub fn rulebook_saved(custom: &str, built_in: &str) -> String {
    if custom.is_empty() { built_in.to_owned() } else { custom.to_owned() }
}

// ---- the app ----------------------------------------------------------------------------------

impl App {
    pub fn extra_call(&mut self, call: ExtraCall) {
        self.call(Call::Extra(Box::new(call)));
    }

    /// The first time the link is live: the appearance and the themes (so the window paints the person's choice at once) and whether
    /// the first-run setup is due.
    pub fn extra_started(&mut self) {
        if std::mem::replace(&mut self.extra.started, true) {
            return;
        }
        self.call(Call::Appearance);
        self.extra_call(ExtraCall::Themes);
        self.extra_call(ExtraCall::Onboarding);
    }

    /// Settings opens or polls: everything its extra rows show.
    pub fn extra_ask_settings(&mut self) {
        for call in [ExtraCall::Themes, ExtraCall::Hotkey, ExtraCall::SuperAgent(None), ExtraCall::Clipboard(None), ExtraCall::Folders, ExtraCall::Preferences] {
            self.extra_call(call);
        }
    }

    // ---- themes

    /// Built-ins and the person's own, in the web's order.
    pub fn theme_entries(&self) -> Vec<ThemeEntry> {
        themes::merged(&self.extra.user_themes)
    }

    /// The ids chosen for light and for dark.
    pub fn theme_choice(&self) -> (String, String) {
        self.pages.theme_ids.clone().unwrap_or_else(|| (themes::DEFAULT_LIGHT.to_owned(), themes::DEFAULT_DARK.to_owned()))
    }

    /// The theme the window paints: the chosen one for the appearance on screen, or the defaults.
    pub fn theme_now(&self, system_dark: bool) -> Rc<crate::theme::Neko> {
        let dark = self.appearance_dark(system_dark);
        let (light_id, dark_id) = self.theme_choice();
        let id = if dark { dark_id } else { light_id };
        if id == themes::DEFAULT_LIGHT && !dark || id == themes::DEFAULT_DARK && dark {
            return crate::theme::theme(dark);
        }
        match self.theme_entries().into_iter().find(|t| t.id == id && t.dark == dark) {
            Some(entry) => themes::painted(&entry.colors),
            None => crate::theme::theme(dark),
        }
    }

    /// Picks the theme for light or for dark: the window follows at once, and the daemon keeps it for every client.
    pub fn pick_theme(&mut self, dark: bool, id: &str) {
        let (light_id, dark_id) = self.theme_choice();
        self.pages.theme_ids = Some(if dark { (light_id, id.to_owned()) } else { (id.to_owned(), dark_id) });
        self.extra_call(ExtraCall::PickTheme { light: (!dark).then(|| id.to_owned()), dark: dark.then(|| id.to_owned()) });
    }

    // ---- the summon shortcut

    pub fn hotkey_record(&mut self) {
        self.extra.recording = true;
        self.extra.hotkey_say = None;
    }

    pub fn hotkey_stop(&mut self) {
        self.extra.recording = false;
    }

    /// A key press while the recorder is open. True when it was taken (everything is, while recording: Esc cancels).
    pub fn hotkey_key(&mut self, ev: &rune::KeyEvent) -> bool {
        if !self.extra.recording {
            return false;
        }
        if ev.key == rune::Key::Escape && !(ev.mods.cmd || ev.mods.alt || ev.mods.ctrl) {
            self.extra.recording = false;
            return true;
        }
        match combo_of(ev) {
            Some(combo) => {
                self.extra.recording = false;
                self.extra.hotkey_say = Some(Say { ok: true, words: format!("Checking {}…", combo_words(&combo)) });
                self.extra_call(ExtraCall::HotkeyCheck(combo));
            }
            None => self.extra.hotkey_say = Some(Say { ok: false, words: "Hold ⌘, ⌥ or ⌃ with a letter, a digit or Space.".to_owned() }),
        }
        true
    }

    // ---- AI

    /// "Use ID": the exact id becomes the model of the runtime being chosen (nothing is sent until it is checked).
    pub fn use_exact_model(&mut self) {
        let id = self.pages.text("custom-model").trim().to_owned();
        if !valid_model_id(&id) {
            return;
        }
        self.pick_runtime(|r| r.model = id);
        self.extra.model_check = None;
    }

    /// "Check model": one real prompt through the runner with the runtime shown; it becomes the default only when it works.
    pub fn check_model(&mut self) {
        if self.extra.checking {
            return;
        }
        self.extra.checking = true;
        self.extra.model_check = None;
        let runtime = self.runtime_shown();
        self.extra_call(ExtraCall::ModelCheck(runtime));
    }

    pub fn set_super_agent(&mut self, on: bool) {
        self.extra_call(ExtraCall::SuperAgent(Some(on)));
    }

    // ---- This Mac

    pub fn set_clipboard(&mut self, on: bool) {
        self.extra_call(ExtraCall::Clipboard(Some(on)));
    }

    pub fn run_diagnostics(&mut self) {
        if self.extra.diag_running {
            return;
        }
        self.extra.diag_running = true;
        self.pages.notes.remove("setting:diagnostics");
        self.extra_call(ExtraCall::Diagnostics);
    }

    pub fn toggle_login(&mut self) {
        self.extra_call(ExtraCall::PreferenceToggle { id: "launch-at-login".to_owned() });
    }

    pub fn add_folder(&mut self) {
        let path = self.pages.text("folder-path").trim().to_owned();
        if path.is_empty() {
            return;
        }
        self.pages.set_field("folder-path", "");
        self.extra_call(ExtraCall::FolderChange { id: path, action: Some("add".to_owned()) });
    }

    pub fn remove_folder(&mut self, id: &str) {
        self.extra_call(ExtraCall::FolderChange { id: id.to_owned(), action: None });
    }

    /// The update source from its field: `owner/name` or a GitHub address as the daemon reads them; empty is the project's own.
    pub fn save_update_source(&mut self) {
        let typed = self.pages.text("update-source").trim().to_owned();
        let repository = if typed.is_empty() {
            String::new()
        } else {
            match normalize_repository(&typed) {
                Some(r) => r,
                None => {
                    self.pages.notes.insert("setting:update-source".to_owned(), "Use owner/name or a github.com address.".to_owned());
                    return;
                }
            }
        };
        self.pages.notes.remove("setting:update-source");
        self.extra_call(ExtraCall::UpdateSource(repository));
    }

    // ---- the rulebook

    pub fn rulebook_saved(&self) -> String {
        let st = self.summary.as_ref().map(|m| &m.settings);
        rulebook_saved(st.map_or("", |s| s.rulebook.as_str()), st.map_or("", |s| s.rulebook_default.as_str()))
    }

    /// The editor's text differs from what is saved (compared as typed: a trailing line counts).
    pub fn rulebook_dirty(&self) -> bool {
        // Compared with what the field was last seeded with (the daemon trims the text it keeps, so edges do not count), so a saved text that changed under an untouched editor is not a draft.
        self.pages.field_saved.get("rulebook").is_some_and(|seeded| self.pages.text("rulebook").trim() != seeded.trim())
    }

    pub fn save_rulebook(&mut self) {
        if !self.rulebook_dirty() {
            return;
        }
        let text = self.pages.text("rulebook").to_owned();
        self.call(Call::SetRulebook { text });
    }

    pub fn revert_rulebook(&mut self) {
        let saved = self.rulebook_saved();
        self.pages.set_field("rulebook", &saved);
    }

    /// The person is leaving; true when they may go. A half-written rulebook stops them with a question (Discard, or Keep editing).
    pub fn guard(&mut self, going: Going) -> bool {
        let leaving_rulebook = self.view == Some(FullView::Settings) && self.pages.section == Section::Rulebook && self.plugin_view.is_none() && !matches!(going, Going::Section(Section::Rulebook));
        if leaving_rulebook && self.rulebook_dirty() {
            self.extra.going = Some(going);
            return false;
        }
        true
    }

    /// "Discard": the draft goes back to what is saved and the person goes where they were going.
    pub fn guard_discard(&mut self) {
        self.revert_rulebook();
        let Some(going) = self.extra.going.take() else { return };
        match going {
            Going::Section(section) => {
                self.pages.section = section;
                self.pages.set_field("settings:search", "");
                self.pages.open_select = None;
            }
            Going::View(view) => self.open_view(view),
            Going::Session(id) => self.select(&id),
            Going::Home => self.open_new(),
        }
    }

    pub fn guard_keep(&mut self) {
        self.extra.going = None;
    }

    /// A Settings section chosen in the rail.
    pub fn go_section(&mut self, section: Section) {
        if self.pages.section == section || self.guard(Going::Section(section)) {
            self.pages.section = section;
            self.pages.set_field("settings:search", "");
            self.pages.open_select = None;
        }
    }

    // ---- what came back

    /// A daemon answer to an [`ExtraCall`]. True when something on screen changed.
    pub fn extra_answer(&mut self, call: &Call, answer: ExtraAnswer) -> bool {
        if let Some(key) = crate::pages::control_key(call) {
            self.pages.notes.remove(&key);
        }
        match answer {
            ExtraAnswer::Themes { dir, themes, problems } => {
                let changed = self.extra.themes_dir.as_deref() != Some(dir.as_str()) || self.extra.user_themes != themes || self.extra.theme_problems != problems;
                self.extra.themes_dir = Some(dir);
                self.extra.user_themes = themes;
                self.extra.theme_problems = problems;
                changed
            }
            ExtraAnswer::Appearance { mode, light, dark } => {
                self.pages.appearance = Some(mode);
                self.pages.theme_ids = Some((light, dark));
                true
            }
            ExtraAnswer::Hotkey(config) => {
                if matches!(call, Call::Extra(c) if matches!(**c, ExtraCall::HotkeyCommit(_))) {
                    self.extra.hotkey_say = Some(Say { ok: true, words: format!("Saved. The summon shortcut is now {}.", combo_words(&config.combo)) });
                } else if self.extra.hotkey_say.as_ref().is_some_and(|s| s.ok && s.words.starts_with("Checking")) {
                    self.extra.hotkey_say = None;
                }
                let changed = self.extra.hotkey.as_ref() != Some(&config);
                self.extra.hotkey = Some(config);
                changed || self.extra.hotkey_say.is_some()
            }
            ExtraAnswer::Conflict(reason) => {
                let Call::Extra(extra) = call else { return false };
                match (&**extra, reason) {
                    (ExtraCall::HotkeyCheck(combo), None) => {
                        let combo = combo.clone();
                        self.extra_call(ExtraCall::HotkeyCommit(combo));
                    }
                    (ExtraCall::HotkeyCheck(combo), Some(why)) => self.extra.hotkey_say = Some(Say { ok: false, words: format!("{} can’t be used: {why}", combo_words(combo)) }),
                    _ => {}
                }
                true
            }
            ExtraAnswer::ModelCheck(check) => {
                self.extra.checking = false;
                // The web reads the catalog again after a check; a working runtime was saved, so the draft is what is saved now.
                if check.ok {
                    self.pages.runtime_draft = None;
                }
                self.extra.model_check = Some(check);
                self.call(Call::Models);
                self.call(Call::Refresh);
                true
            }
            ExtraAnswer::SuperAgent(on) => {
                let changed = self.extra.super_agent != Some(on);
                self.extra.super_agent = Some(on);
                changed
            }
            ExtraAnswer::Clipboard(on) => {
                let changed = self.extra.clipboard != Some(on);
                self.extra.clipboard = Some(on);
                changed
            }
            ExtraAnswer::Diagnostics { round_trip_ms, checks } => {
                self.extra.diag_running = false;
                self.extra.diag = Some((round_trip_ms, checks));
                true
            }
            ExtraAnswer::Machine(health) => {
                let at = self.now_ms;
                // Held: an unchanged reading does not replace the one on screen (the web compares `at_ms`).
                let same = matches!((&self.extra.machine, &health), (Some((old, _)), Some(new)) if old.at_ms == new.at_ms);
                if let (false, Some(h)) = (same, health) {
                    self.extra.machine = Some((h, at));
                    return true;
                }
                if let Some((_, read)) = self.extra.machine.as_mut() {
                    *read = at;
                }
                false
            }
            ExtraAnswer::Rows { kind, rows } => match kind {
                "folders" => {
                    let changed = self.extra.folders.as_ref() != Some(&rows);
                    self.extra.folders = Some(rows);
                    changed
                }
                _ => {
                    let login = rows.iter().find(|r| r.id == "launch-at-login").map(|r| r.accessory == "On");
                    let changed = self.extra.login != login;
                    self.extra.login = login;
                    changed
                }
            },
            ExtraAnswer::Onboarding { completed } => self.onboarding_state(completed),
            ExtraAnswer::Noted => {
                let Call::Extra(extra) = call else { return false };
                match &**extra {
                    // A folder or the login item changed: the list is read again.
                    ExtraCall::FolderChange { .. } => self.extra_call(ExtraCall::Folders),
                    ExtraCall::PreferenceToggle { .. } => self.extra_call(ExtraCall::Preferences),
                    _ => {}
                }
                false
            }
        }
    }

    /// A refusal or failure of an [`ExtraCall`]: said beside the control that asked. A report or the machine reading says nothing.
    /// Some(changed) when the call was an extra's.
    pub fn extra_refused(&mut self, call: &Call, message: &str) -> Option<bool> {
        let Call::Extra(extra) = call else { return None };
        match &**extra {
            ExtraCall::ModelCheck(_) => {
                self.extra.checking = false;
                self.extra.model_check = Some(ModelCheck { runtime: self.runtime_shown(), ok: false, unavailable: false, message: message.to_owned() });
            }
            ExtraCall::Diagnostics => {
                self.extra.diag_running = false;
                self.pages.notes.insert(extra.key(), message.to_owned());
            }
            ExtraCall::HotkeyCheck(_) | ExtraCall::HotkeyCommit(_) => self.extra.hotkey_say = Some(Say { ok: false, words: message.to_owned() }),
            ExtraCall::OnboardingDone | ExtraCall::Onboarding => return Some(self.onboarding_failed(extra.key(), message)),
            quiet if quiet.quiet() => return Some(false),
            other => {
                // A read that fails while older data is on screen stays quiet; a write says why.
                let write = matches!(other, ExtraCall::PickTheme { .. } | ExtraCall::SuperAgent(Some(_)) | ExtraCall::Clipboard(Some(_)) | ExtraCall::UpdateSource(_) | ExtraCall::FolderChange { .. } | ExtraCall::PreferenceToggle { .. });
                if write {
                    self.pages.notes.insert(other.key(), message.to_owned());
                }
            }
        }
        Some(true)
    }

    /// The machine's reading is asked for while Usage is open (the web polls every few seconds).
    /// The window paints dark: the daemon's mode when it is Light or Dark, else the system's as last built.
    pub fn theme_now_dark(&self) -> bool {
        self.appearance_dark(self.panel.dark.get())
    }

    pub fn extra_ask_machine(&mut self) {
        self.extra_call(ExtraCall::Machine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Key, KeyEvent, Mods};

    fn press(key: Key, cmd: bool, alt: bool, ctrl: bool, shift: bool) -> KeyEvent {
        KeyEvent { key, mods: Mods { cmd, alt, ctrl, shift, ..Mods::default() } }
    }

    #[test]
    fn an_exact_model_id_follows_the_webs_rule() {
        for ok in ["gpt-5", "claude-sonnet-4.5", "org/model:tag", "a@b", "m[1m]", "x_y"] {
            assert!(valid_model_id(ok), "{ok}");
        }
        for bad in ["", "-flag", "has space", "semi;colon", &"x".repeat(121), "quo\"te"] {
            assert!(!valid_model_id(bad), "{bad}");
        }
    }

    #[test]
    fn the_update_source_is_owner_name_or_a_github_address() {
        assert_eq!(normalize_repository("acme/neko").as_deref(), Some("acme/neko"));
        assert_eq!(normalize_repository(" https://github.com/acme/neko-fork.git ").as_deref(), Some("acme/neko-fork"));
        assert_eq!(normalize_repository("git@github.com:acme/neko.git").as_deref(), Some("acme/neko"));
        assert_eq!(normalize_repository("github.com/acme/neko/").as_deref(), Some("acme/neko"));
        for bad in ["not a repo", "acme", "a/b/c", "https://example.com/a/b", "/b", "a/"] {
            assert_eq!(normalize_repository(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_recorded_shortcut_needs_a_modifier_and_is_named_as_the_daemon_names_it() {
        let alt_space = combo_of(&press(Key::Text(" ".into()), false, true, false, false)).unwrap();
        assert_eq!((alt_space.key.as_str(), alt_space.modifiers.clone()), ("Space", vec![Modifier::Alt]));
        assert_eq!(combo_words(&alt_space), "⌥ Space");
        let cmd_shift_k = combo_of(&press(Key::Command('k'), true, false, false, true)).unwrap();
        assert_eq!((cmd_shift_k.key.as_str(), combo_words(&cmd_shift_k).as_str()), ("KeyK", "⌘⇧ K"));
        assert_eq!(combo_of(&press(Key::Command('1'), true, false, false, false)).unwrap().key, "Digit1");
        assert!(combo_of(&press(Key::Text("a".into()), false, false, false, false)).is_none(), "a bare key is not a shortcut");
        assert!(combo_of(&press(Key::Text("a".into()), false, false, false, true)).is_none(), "shift alone is not enough");
        assert!(combo_of(&press(Key::Backspace, true, false, false, false)).is_none());
    }

    #[test]
    fn the_theme_prompt_is_the_webs_and_fits_the_daemons_limit() {
        let web = include_str!("../../../web/src/themes/prompt.ts");
        let start = web.find("export const THEME_PROMPT = `").unwrap() + "export const THEME_PROMPT = `".len();
        let end = start + web[start..].find("`;\n").unwrap();
        assert_eq!(THEME_PROMPT, &web[start..end], "src/theme_prompt.txt must be web/src/themes/prompt.ts's THEME_PROMPT");
        let text = theme_prompt("  a calm sage light theme ", Some("/data/themes"));
        assert!(text.starts_with("Make me a Neko theme: a calm sage light theme\n") && text.contains("/data/themes/<id>.json") && !text.contains("{{"));
        assert!(text.len() <= LIMIT_BYTES);
        assert!(theme_budget(Some("/data/themes")) > 1000);
    }

    #[test]
    fn the_rulebook_shows_the_persons_own_or_the_default() {
        assert_eq!(rulebook_saved("", "default"), "default");
        assert_eq!(rulebook_saved("mine", "default"), "mine");
    }
}
