//! Part 3: the launcher panel. The view is shared by two hosts so the latency of each can be
//! measured: Rune's `WindowOptions::launcher_panel()` (a key-taking non-activating NSPanel, the
//! `launcher` example's pattern) and a plain `App::overlay` with `KeyPolicy::None` (never key).

use std::borrow::Cow;
use std::sync::OnceLock;

use rune::tok::{self, type_};
use rune::{
    AppHandle, Color, Cx, DisplayTarget, El, GrowthSpec, Key, KeyEvent, OverlayPlacement, ScreenEdge, ScrollSpec, TextEdit, Tx,
    col, el, row, spacer, text, text_input,
};

pub const SIZE: (f32, f32) = (620.0, 372.0);
const ROW_H: f32 = 36.0;
const VISIBLE_ROWS: f32 = 8.0;

const VERBS: [&str; 8] = ["Open", "Review", "Rebase", "Archive", "Summarise", "Deploy", "Rename", "Pin"];
const NOUNS: [&str; 25] = [
    "ticket", "branch", "worktree", "digest", "routine", "skill", "memory", "inbox", "calendar", "artifact", "checkpoint",
    "goal", "watch", "source", "connection", "workspace", "preview", "release", "commit", "diff", "agent", "session",
    "prompt", "tool", "plugin",
];

/// 200 fake items: "Review worktree 017".
pub fn items() -> &'static [String] {
    static ITEMS: OnceLock<Vec<String>> = OnceLock::new();
    ITEMS.get_or_init(|| (0..200).map(|i| format!("{} {} {:03}", VERBS[i % 8], NOUNS[(i * 7 + i / 8) % 25], i)).collect())
}

pub struct Launcher {
    pub query: TextEdit,
    pub picked: usize,
    /// Indices into `items()` that match the query, in order.
    pub found: Vec<usize>,
    /// The hotkey handler needs its own app handle to show the panel.
    pub me: Option<AppHandle<Launcher>>,
}

pub fn matching(query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    items().iter().enumerate().filter(|(_, t)| t.to_lowercase().contains(&q)).map(|(i, _)| i).collect()
}

impl Launcher {
    pub fn new(me: Option<AppHandle<Launcher>>) -> Self {
        let mut query = TextEdit::new("");
        query.focused = true;
        Self { query, picked: 0, found: matching(""), me }
    }
    /// Start a fresh summon: empty query, first row selected.
    pub fn reset(&mut self) {
        self.query.clear();
        self.picked = 0;
        self.found = matching("");
    }
}

pub fn place_top_centre() -> (DisplayTarget, OverlayPlacement) {
    (DisplayTarget::Primary, OverlayPlacement::Edge { edge: ScreenEdge::Top, margin: 150.0, align: rune::Align::Center, inset: 0.0 })
}

fn row_key(id: usize) -> String {
    format!("item-{id}")
}

fn open_picked(s: &mut Launcher, tx: &mut Tx) {
    if let Some(&id) = s.found.get(s.picked) {
        // stdout is the launcher's result channel.
        println!("picked: {}", items()[id]);
    }
    s.reset();
    tx.close_window();
}

pub fn view(s: &Launcher, _: &mut Cx<Launcher>) -> El {
    let t = tok::theme(true);
    let accent = tok::accent(0, true);
    let field = text_input(&s.query, type_::BODY.lh(26.0)).key("query").single_line().grow(1.0)
        .color(t.text).placeholder("Search 200 items", t.text_secondary)
        .on_edit(|s: &mut Launcher| &mut s.query)
        .on_input_key(|s: &mut Launcher, ev: &KeyEvent, tx: &mut Tx| {
            let n = s.found.len();
            match ev.key {
                Key::Down if n > 0 => s.picked = (s.picked + 1) % n,
                Key::Up if n > 0 => s.picked = (s.picked + n - 1) % n,
                Key::Escape => { tx.close_window(); return true; }
                _ => return false,
            }
            if let Some(&id) = s.found.get(s.picked) { tx.scroll_into_view(row_key(id)); }
            true
        })
        .on_input_change(|s: &mut Launcher, _| {
            s.found = matching(&s.query.text);
            s.picked = 0;
        })
        .on_submit(open_picked);

    let list = if s.found.is_empty() {
        col().key("empty").h(ROW_H * VISIBLE_ROWS).center()
            .child(text("No matches").font(type_::BODY).color(t.text_secondary))
    } else {
        col().key("scroll").h(ROW_H * VISIBLE_ROWS + 12.0).scroll(ScrollSpec { stick_bottom: false }).child(
            col().key("rows").padding(6.0).gap(0.0).growth(GrowthSpec::default()).virtual_rows(
                s.found.len(), ROW_H,
                |s: &Launcher, i| Cow::Owned(row_key(s.found[i])),
                move |s: &Launcher, i| {
                    let id = s.found[i];
                    let on = i == s.picked;
                    row().h(ROW_H).px(12.0).radius(8.0).center_y()
                        .fill(if on { accent.alpha(0.85) } else { Color::TRANSPARENT })
                        .child(text(&items()[id]).font(type_::BODY).color(if on { Color::WHITE } else { t.text }))
                        .child(spacer())
                        .child(text(format!("#{id}")).font(type_::LABEL).color(if on { Color::WHITE.alpha(0.8) } else { t.text_secondary }))
                },
            ),
        )
    };
    let panel = col().key("panel").w_full().h_full().radius(14.0).clip_rounded(14.0)
        .fill(t.card).border(1.0, Color::WHITE.alpha(0.1))
        .child(row().h(56.0).px(18.0).gap(12.0).center_y()
            .child(text("⌕").font(type_::TITLE.lh(26.0)).color(t.text_secondary))
            .child(field))
        .child(el().h(1.0).w_full().fill(t.separator))
        .child(list);
    col().key("root").w_full().h_full().child(panel)
}
