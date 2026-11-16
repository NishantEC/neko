//! ⌘K (`palette.rs` holds the rows): 640 wide, its top edge at 20% of the window's height, radius 16. A 44 point search field on
//! top, then the sections in the web's order (Sessions, Go to, Actions, Appearance, Plugins), each row 36 points with its
//! shortcut or state right-aligned. Up and Down move, Return runs, Esc closes.

use rune::{Color, El, Key, KeyEvent, ScrollSpec, TextStyle, col, el, icon, row, text};
use rune_kit::{TextInput, popover, scrim};

use crate::app::App;
use crate::palette::{Item, Section};
use crate::theme::Neko;

/// The palette's width, the row's height, the field's height and the radius.
pub const WIDTH: f32 = 640.0;
pub const ROW_H: f32 = 36.0;
pub const FIELD_H: f32 = 44.0;
pub const RADIUS: f32 = 16.0;
/// Where its top edge is, as a share of the window's height.
pub const TOP: f32 = 0.20;

pub fn mount(s: &App, t: &Neko, win: (f32, f32), layer: &mut Vec<El>) {
    let Some(p) = &s.palette else { return };
    if s.summary.is_none() {
        return;
    }
    let items = s.palette_items();
    let index = p.index.min(items.len().saturating_sub(1));
    let field = TextInput::new(&s.palette_query)
        .placeholder("Go to a page, find a session or run an action…")
        .label("Command palette")
        .height(FIELD_H)
        .leading_icon(&crate::icons::SEARCH)
        .on_edit(|s: &mut App| &mut s.palette_query)
        .on_change(|s: &mut App, _| {
            if let Some(p) = s.palette.as_mut() {
                p.index = 0;
            }
        })
        .on_key(|s: &mut App, ev: &KeyEvent, _| s.palette_key(ev))
        .build(&t.kit)
        .key("palette-input");

    let mut list = col().key("palette-list").role("listbox").label("Results").w_full().gap(0.0).pb(4.0);
    if items.is_empty() {
        list = list.child(text("Nothing matches. Try a page name or a session title.").key("palette-empty").font(t.ty.meta).color(t.text.muted).px(16.0).py(12.0));
    }
    let mut at: Option<Section> = None;
    for (i, item) in items.iter().enumerate() {
        if at != Some(item.section) {
            at = Some(item.section);
            list = list.child(text(item.section.title()).key(format!("palette-heading-{}", item.section.title())).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.faint).px(16.0).pt(10.0).pb(4.0));
        }
        list = list.child(item_row(t, i, item, i == index));
    }
    let max_h = (win.1 * 0.6).clamp(240.0, 520.0);
    let body = col().key("palette-scroll").w_full().max_h(max_h).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list);
    let rule = el().key("palette-rule").w_full().h(1.0).fill(t.surface.border);
    let surface = popover::raised(
        &t.kit,
        col().key("palette").role("dialog").label("Command palette").w(WIDTH).child(row().w_full().px(8.0).h(FIELD_H).center_y().child(field.grow(1.0))).child(rule).child(body).clip_rounded(RADIUS),
        RADIUS,
    );
    layer.push(scrim("palette-scrim", |s: &mut App| s.palette = None));
    let left = ((win.0 - WIDTH) / 2.0).max(8.0);
    layer.push(el().key("palette-anchor").absolute(left, win.1 * TOP).child(surface));
}

fn item_row(t: &Neko, i: usize, item: &Item, on: bool) -> El {
    let run = item.run.clone();
    let mut r = row()
        .key(format!("palette-row-{i}"))
        .role("option")
        .label(item.title.clone())
        .checked(on)
        .w_full()
        .h(ROW_H)
        .px(12.0)
        .mx(4.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if on { t.surface.active } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .on_press(move |s: &mut App| s.palette_run(run.clone()))
        .child(icon(item.icon, 16.0).color(t.text.muted).a11y_hidden());
    let mut words = row().grow(1.0).min_w(0.0).gap(8.0).center_y().child(text(item.title.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).a11y_hidden());
    if let Some(hint) = &item.hint {
        words = words.child(text(hint.clone()).font(t.ty.meta).color(t.text.muted).truncate().a11y_hidden());
    }
    r = r.child(words);
    if let Some(right) = &item.right {
        r = r.child(text(right.clone()).key(format!("palette-right-{i}")).font(t.ty.hint).color(t.text.faint).a11y_hidden());
    }
    r.w(WIDTH - 8.0)
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
        self.palette_run(crate::palette::Do::OpenSession(id.to_owned()));
    }

    /// Up and Down choose, Return runs. True when the key was the palette's.
    pub fn palette_key(&mut self, ev: &KeyEvent) -> bool {
        let Some(p) = self.palette.as_ref() else { return false };
        let items = self.palette_items();
        let current = p.index.min(items.len().saturating_sub(1));
        match ev.key {
            Key::Down => {
                if let Some(p) = self.palette.as_mut() {
                    p.index = (current + 1).min(items.len().saturating_sub(1));
                }
                true
            }
            Key::Up => {
                if let Some(p) = self.palette.as_mut() {
                    p.index = current.saturating_sub(1);
                }
                true
            }
            Key::Escape => {
                self.palette = None;
                true
            }
            Key::Enter if !self.palette_query.is_composing() => {
                if let Some(item) = items.get(current) {
                    let run = item.run.clone();
                    self.palette_run(run);
                }
                true
            }
            _ => false,
        }
    }
}
