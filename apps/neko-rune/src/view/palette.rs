//! ⌘K: jump to a session by a few letters of its title (`frizbee`). A dialog over the window:
//! a field, the matches below, Up and Down to choose, Return to open, Escape to close.

use rune::{El, Key, KeyEvent, col, row, text};
use rune_kit::{Dialog, TextInput};

use crate::app::App;
use crate::groups::{age, fuzzy_sessions, status_word};
use crate::status::{status_glyph, status_of};
use crate::theme::Neko;

/// The most matches the palette lists.
pub const SHOWN: usize = 8;

pub fn mount(s: &App, t: &Neko, layer: &mut Vec<El>) {
    let Some(p) = &s.palette else { return };
    let Some(summary) = &s.summary else { return };
    let hits = fuzzy_sessions(summary, &s.palette_query.text, s.now_ms, SHOWN);
    let index = p.index.min(hits.len().saturating_sub(1));
    let field = TextInput::new(&s.palette_query)
        .placeholder("Go to a session…")
        .label("Go to a session")
        .on_edit(|s: &mut App| &mut s.palette_query)
        .on_change(|s: &mut App, _| {
            if let Some(p) = s.palette.as_mut() {
                p.index = 0;
            }
        })
        .on_key(|s: &mut App, ev: &KeyEvent, _| s.palette_key(ev))
        .build(&t.kit)
        .key("palette-input");
    let mut list = col().key("palette-list").role("listbox").label("Sessions").w_full().gap(2.0).pt(8.0);
    if hits.is_empty() {
        list = list.child(text("No session matches").key("palette-empty").font(t.ty.meta).color(t.text.muted).px(10.0).py(6.0));
    }
    for (i, task) in hits.iter().enumerate() {
        let id = task.id.clone();
        list = list.child(
            row()
                .key(format!("palette-row-{}", task.id))
                .role("option")
                .label(task.title.clone())
                .checked(i == index)
                .w_full()
                .h(32.0)
                .px(10.0)
                .gap(8.0)
                .center_y()
                .radius(t.radius.row)
                .fill(if i == index { t.surface.active } else { rune::Color::TRANSPARENT })
                .hover(t.surface.hover)
                .on_press(move |s: &mut App| s.palette_open(&id))
                .child(status_glyph(status_of(task.status), t, 14.0, Some("")))
                .child(text(task.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
                .child(text(format!("{} · {}", status_word(task.status), age(s.now_ms, task.updated_at_ms))).font(t.ty.id).color(t.text.faint).a11y_hidden()),
        );
    }
    Dialog::new("palette", "Go to a session").open(true).width(520.0).fit().child(field).child(list).on_close(|s: &mut App| s.palette = None).build(&t.kit, layer);
}

impl App {
    pub fn palette_toggle(&mut self) {
        if self.palette.is_some() {
            self.palette = None;
        } else if self.summary.is_some() {
            self.palette_query = rune::TextEdit::new("");
            self.palette_query.focused = true;
            self.palette = Some(crate::app::PaletteState { index: 0 });
        }
    }

    /// Opens the session a palette row names and closes the palette.
    pub fn palette_open(&mut self, id: &str) {
        self.palette = None;
        self.select(id);
    }

    /// Up and Down choose, Return opens. True when the key was the palette's.
    pub fn palette_key(&mut self, ev: &KeyEvent) -> bool {
        let Some(summary) = self.summary.as_ref() else { return false };
        let Some(p) = self.palette.as_ref() else { return false };
        let hits: Vec<String> = fuzzy_sessions(summary, &self.palette_query.text, self.now_ms, SHOWN).iter().map(|t| t.id.clone()).collect();
        let current = p.index.min(hits.len().saturating_sub(1));
        match ev.key {
            Key::Down => {
                if let Some(p) = self.palette.as_mut() {
                    p.index = (current + 1).min(hits.len().saturating_sub(1));
                }
                true
            }
            Key::Up => {
                if let Some(p) = self.palette.as_mut() {
                    p.index = current.saturating_sub(1);
                }
                true
            }
            Key::Enter if !self.palette_query.is_composing() => {
                if let Some(id) = hits.get(current) {
                    let id = id.clone();
                    self.palette_open(&id);
                }
                true
            }
            _ => false,
        }
    }
}
