//! The launcher (P4, docs/knowledge/rune-overlays.md): `Palette.swift`'s summon panel with Rune's ⌘K palette as its content. A 760 by 500
//! `launcher_panel()` window (a non-activating NSPanel on every Space), centred on the screen under the pointer, made once at startup and
//! shown and hidden after that. The daemon's summon shortcut toggles it; Escape and a click outside hide it; it takes keys only while it
//! is shown. Choosing a row does what the in-window palette does (`App::palette_run`, in the main window), then hides the panel.
//!
//! The panel is a window of its own with its own state type, so it holds a copy of the palette's rows (`App::palette_all`, taken when it
//! is shown) and filters them itself (`palette::narrow`). What a row does happens in the main window: the panel sends it there.
//!
//! **A generation counter** (Swift's `PaletteGeneration`): every show and every hide advances it, and a hide that was queued carries the
//! number it was queued under. A hide queued by a focus change in the previous presentation and delivered after the next show finds a
//! newer number and does nothing, so it cannot close the panel the person just summoned.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rune::{AppHandle, Cx, El, Key, KeyEvent, TextEdit, Tx, col};

use crate::app::App;
use crate::palette::{self, Do, Item};
use crate::theme::{self, Hex, Neko};
use crate::view::palette as palette_view;

/// The panel's window, in points (Swift's `NSRect(0, 0, 760, 500)`).
pub const WINDOW: (f32, f32) = (760.0, 500.0);
/// The card inside it: 8 points of clear window on every side, so the card's shadow is not cut at the window's edge.
pub const CARD_WIDTH: f32 = WINDOW.0 - 16.0;
/// The card's top edge is this share of the screen's visible height below the visible area's top (Swift: `maxY - height - 0.15 * h`).
pub const TOP_SHARE: f32 = 0.15;

/// Tokens for each show and hide (Swift's `PaletteGeneration`), shared by the window that summons (the main one) and the panel.
#[derive(Debug, Default)]
pub struct Presentation {
    generation: AtomicU64,
    shown: AtomicBool,
}

impl Presentation {
    /// Starts a new show or hide and returns its token.
    pub fn advance(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// True while nothing newer has started since `token` was issued.
    pub fn is_current(&self, token: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == token
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// True from a show until a hide starts.
    pub fn shown(&self) -> bool {
        self.shown.load(Ordering::SeqCst)
    }

    pub fn set_shown(&self, on: bool) {
        self.shown.store(on, Ordering::SeqCst);
    }

    /// The summon shortcut: a show when hidden (the new token), a hide when shown (None).
    pub fn toggle(&self) -> Option<u64> {
        if self.shown() {
            self.advance();
            self.set_shown(false);
            None
        } else {
            let token = self.advance();
            self.set_shown(true);
            Some(token)
        }
    }
}

/// What the panel asked of the outside; a test reads them (`Launcher::record`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Hide the panel.
    Hide,
    /// Run this row in the main window; bring that window forward when `front`.
    Run { run: Do, front: bool },
}

pub struct Launcher {
    /// A copy of `App::palette_all`, taken at the show.
    pub items: Vec<Item>,
    pub query: TextEdit,
    pub index: usize,
    pub theme: Rc<Neko>,
    pub presentation: Arc<Presentation>,
    /// The show this is: a hide queued under another number does nothing.
    pub token: u64,
    /// The panel has held the keys since this show: a focus loss before that is the system settling, not the person leaving.
    pub focused_once: bool,
    pub main: Option<AppHandle<App>>,
    pub me: Option<AppHandle<Launcher>>,
    /// Keep every effect in `log` (tests); a running panel keeps none.
    pub record: bool,
    pub log: Vec<Effect>,
}

impl Launcher {
    pub fn new(presentation: Arc<Presentation>, main: Option<AppHandle<App>>) -> Launcher {
        Launcher { items: Vec::new(), query: TextEdit::new(""), index: 0, theme: theme::theme(true), presentation, token: 0, focused_once: false, main, me: None, record: false, log: Vec::new() }
    }

    /// A show: the rows, the main window's theme and this show's token. The query starts empty with the keyboard in it.
    pub fn present(&mut self, items: Vec<Item>, theme: Option<(bool, Hex)>, token: u64) {
        self.items = items;
        if let Some((dark, hex)) = theme {
            self.theme = Rc::new(Neko::from_hex(dark, hex));
        }
        self.query = TextEdit::new("");
        self.query.focused = true;
        self.index = 0;
        self.token = token;
        self.focused_once = false;
    }

    /// The rows now: what the words leave of the copy.
    pub fn visible(&self) -> Vec<Item> {
        palette::narrow(&self.items, &self.query.text)
    }

    fn emit(&mut self, effect: Effect) {
        match &effect {
            Effect::Hide => {
                if let Some(me) = &self.me {
                    me.hide();
                }
            }
            Effect::Run { run, front } => {
                if let Some(main) = &self.main {
                    let run = run.clone();
                    main.update(move |app, tx| {
                        app.palette_run(run);
                        tx.instant();
                    });
                    if *front {
                        main.open();
                    }
                }
            }
        }
        if self.record {
            self.log.push(effect);
        }
    }

    /// Hides the panel now (Escape, a choice, the summon shortcut again). Advances the generation, so anything queued before is stale.
    pub fn hide(&mut self) {
        if !self.presentation.shown() {
            return;
        }
        self.presentation.advance();
        self.presentation.set_shown(false);
        self.emit(Effect::Hide);
    }

    /// A hide that was queued under `token` (a focus change): only if nothing newer has started and the panel is still up.
    pub fn hide_if_current(&mut self, token: u64) {
        if self.presentation.is_current(token) && self.presentation.shown() {
            self.hide();
        }
    }

    /// The panel gained or lost the keys. Losing them after having them (a click on another window) hides it, through the generation
    /// check: the hide is queued behind whatever else is waiting, under the token of now.
    pub fn focus_changed(&mut self, focused: bool) {
        if focused {
            self.focused_once = true;
            return;
        }
        if !self.focused_once || !self.presentation.shown() {
            return;
        }
        let token = self.presentation.generation();
        match self.me.clone() {
            Some(me) => me.update(move |l, _| l.hide_if_current(token)),
            None => self.hide_if_current(token),
        }
    }

    /// Runs a row in the main window and hides the panel.
    pub fn pick(&mut self, run: Do) {
        let front = run.wants_window();
        self.emit(Effect::Run { run, front });
        self.hide();
    }

    /// Up and Down choose, Return runs, Escape hides. True when the key was the panel's.
    pub fn key(&mut self, ev: &KeyEvent) -> bool {
        let items = self.visible();
        let current = self.index.min(items.len().saturating_sub(1));
        match ev.key {
            Key::Down => {
                self.index = (current + 1).min(items.len().saturating_sub(1));
                true
            }
            Key::Up => {
                self.index = current.saturating_sub(1);
                true
            }
            Key::Escape => {
                self.hide();
                true
            }
            Key::Enter if !self.query.is_composing() => {
                if let Some(item) = items.get(current) {
                    let run = item.run.clone();
                    self.pick(run);
                }
                true
            }
            _ => false,
        }
    }
}

/// The panel's content: the palette's card, centred at the top of a clear window.
pub fn view(l: &Launcher, _cx: &mut Cx<Launcher>) -> El {
    let t = l.theme.clone();
    let items = l.visible();
    let index = l.index.min(items.len().saturating_sub(1));
    let field = palette_view::search_field(
        &t,
        &l.query,
        |l: &mut Launcher| &mut l.query,
        |l: &mut Launcher, _| l.index = 0,
        |l: &mut Launcher, ev: &KeyEvent, _: &mut Tx| l.key(ev),
    );
    let card = palette_view::surface(&t, field, &items, index, CARD_WIDTH, WINDOW.1 - 16.0 - palette_view::FIELD_H - 1.0, |l: &mut Launcher, run| l.pick(run));
    col().key("launcher").w_full().h_full().padding(8.0).child(card)
}

/// The launcher's top margin on a screen: its visible area's top inset (the menu bar) plus [`TOP_SHARE`] of the visible height.
pub fn top_margin(menu_bar: f32, visible_height: f32) -> f32 {
    menu_bar + TOP_SHARE * visible_height
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::Mods;

    fn launcher() -> Launcher {
        let mut l = Launcher::new(Arc::new(Presentation::default()), None);
        l.record = true;
        l
    }

    fn rows() -> Vec<Item> {
        let mut app = App::headless();
        let tasks = (0..3).map(|i| crate::link::TaskRow { id: format!("t{i}"), key: None, title: format!("Task {i} about billing"), status: neko_protocol::workbench::TaskStatus::Building, updated_at_ms: i, goal: String::new(), wait: crate::link::Wait::None, progress: None, line: Default::default() }).collect();
        app.summary = Some(crate::link::Summary { tasks, ..Default::default() });
        app.palette_all()
    }

    fn press(l: &mut Launcher, key: Key) -> bool {
        l.key(&KeyEvent { key, mods: Mods::default() })
    }

    #[test]
    fn the_summon_shortcut_toggles_and_each_toggle_is_a_new_generation() {
        let p = Presentation::default();
        let first = p.toggle().expect("a show");
        assert!(p.shown() && p.is_current(first));
        assert_eq!(p.toggle(), None, "the same shortcut again hides");
        assert!(!p.shown() && !p.is_current(first), "the hide advanced the generation");
        let second = p.toggle().expect("a show");
        assert!(second > first && p.is_current(second));
    }

    #[test]
    fn a_stale_hide_after_a_new_show_does_nothing() {
        let mut l = launcher();
        // The first show, and a focus loss queued under its token.
        l.presentation.set_shown(true);
        let stale = l.presentation.advance();
        l.present(rows(), None, stale);
        // The panel is hidden and shown again before the queued hide is delivered.
        l.hide();
        let fresh = l.presentation.toggle().expect("shown again");
        l.present(rows(), None, fresh);
        l.log.clear();
        l.hide_if_current(stale);
        assert!(l.presentation.shown(), "the panel the person just summoned is still up");
        assert!(l.log.is_empty(), "and nothing was asked of the window: {:?}", l.log);
        // A hide queued under the current token does hide.
        l.hide_if_current(fresh);
        assert!(!l.presentation.shown());
        assert_eq!(l.log, vec![Effect::Hide]);
    }

    #[test]
    fn losing_the_keys_hides_only_after_the_panel_had_them() {
        let mut l = launcher();
        let token = l.presentation.toggle().unwrap();
        l.present(rows(), None, token);
        l.focus_changed(false);
        assert!(l.presentation.shown(), "a focus loss while the show is settling is not the person leaving");
        l.focus_changed(true);
        l.focus_changed(false);
        assert!(!l.presentation.shown() && l.log == vec![Effect::Hide]);
        // Hiding twice is one hide.
        l.hide();
        assert_eq!(l.log.len(), 1);
    }

    #[test]
    fn escape_hides_and_a_hidden_panel_is_not_hidden_again() {
        let mut l = launcher();
        let token = l.presentation.toggle().unwrap();
        l.present(rows(), None, token);
        assert!(press(&mut l, Key::Escape));
        assert!(!l.presentation.shown());
        assert_eq!(l.log, vec![Effect::Hide]);
        assert!(press(&mut l, Key::Escape));
        assert_eq!(l.log.len(), 1);
    }

    #[test]
    fn arrows_move_return_runs_the_row_and_hides() {
        let mut l = launcher();
        let token = l.presentation.toggle().unwrap();
        l.present(rows(), None, token);
        assert!(l.visible().len() > 3);
        press(&mut l, Key::Down);
        press(&mut l, Key::Down);
        assert_eq!(l.index, 2);
        press(&mut l, Key::Up);
        assert_eq!(l.index, 1);
        let expected = l.visible()[1].run.clone();
        press(&mut l, Key::Enter);
        assert_eq!(l.log, vec![Effect::Run { run: expected.clone(), front: expected.wants_window() }, Effect::Hide]);
        assert!(!l.presentation.shown());
    }

    #[test]
    fn words_narrow_the_copy_and_a_new_show_starts_empty() {
        let mut l = launcher();
        let token = l.presentation.toggle().unwrap();
        l.present(rows(), None, token);
        l.query.replace_text("calendr");
        assert!(l.visible().iter().any(|i| i.title == "Go to Calendar"));
        l.index = 3;
        l.hide();
        let token = l.presentation.toggle().unwrap();
        l.present(rows(), None, token);
        assert!(l.query.text.is_empty() && l.query.focused && l.index == 0);
    }

    #[test]
    fn only_rows_that_open_something_bring_the_window_forward() {
        assert!(Do::OpenSession("a".into()).wants_window());
        assert!(Do::GoTo(crate::views::FullView::Settings).wants_window());
        assert!(Do::StopAll.wants_window(), "its confirmation is a dialog in the main window");
        assert!(!Do::Mode("dark").wants_window());
        assert!(!Do::ToggleSidebar.wants_window());
        assert!(!Do::Approve("a".into()).wants_window());
    }

    #[test]
    fn the_top_edge_is_the_menu_bar_plus_fifteen_percent_of_what_is_visible() {
        assert_eq!(top_margin(0.0, 1000.0), 150.0);
        assert_eq!(top_margin(38.0, 800.0), 158.0);
    }
}
