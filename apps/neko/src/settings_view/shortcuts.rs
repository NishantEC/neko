//! Settings ▸ Shortcuts: every key Neko answers, by where it works. Read-only for now: the keys cannot be changed yet
//! (docs/knowledge/app-shell.md, "Keyboard", says where each is handled).

use rune::{El, col, row, text};

use super::Found;
use crate::state::App;
use crate::ui::base::Theme;
use crate::ui::kbd::Kbd;

/// One shortcut: where it works, what it does, its keys (any one of them), and other words that find it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub group: &'static str,
    pub what: String,
    pub keys: Vec<&'static str>,
    pub keywords: &'static str,
}

fn sc(group: &'static str, what: &str, keys: &[&'static str], keywords: &'static str) -> Shortcut {
    Shortcut { group, what: what.to_owned(), keys: keys.to_vec(), keywords }
}

/// Every shortcut, in the order the page lists them. What Return and ⌥Return do while the agent works follows the choice in
/// General.
pub fn rows(s: &App) -> Vec<Shortcut> {
    let (first, second) = if s.queue_first() { ("Queue the words for after this run", "Steer: the agent reads them at its next step") } else { ("Steer: the agent reads them at its next step", "Queue the words for after this run") };
    vec![
        sc("Go to", "New thread", &["⌘ N", "⌘ 1"], "compose start"),
        sc("Go to", "Search", &["⌘ K"], "find palette"),
        sc("Go to", "Needs you", &["⌘ 2"], "waiting inbox"),
        sc("Go to", "Rounds", &["⌘ 3"], "schedule"),
        sc("Go to", "Briefing", &["⌘ 4"], "away summary"),
        sc("Go to", "Settings", &["⌘ ,"], "preferences"),
        sc("Window", "Hide or show the sidebar", &["⌃ ⌘ S"], "toggle"),
        sc("Window", "Hide or show the right pane", &["⌥ ⌘ B"], "toggle details"),
        sc("Window", "The right pane's tabs, in order", &["⌥ ⌘ 1–9"], "overview changes kittens"),
        sc("Window", "Changes", &["⇧ ⌘ G", "⌘ D"], "diff review"),
        sc("Window", "Enter or exit full screen", &["⌃ ⌘ F"], ""),
        sc("Window", "Close the window (Neko keeps running)", &["⌘ W"], "quit"),
        sc("Threads", "Open thread 1 to 9 of the sidebar", &["⌥ 1–9"], "jump"),
        sc("Threads", "The thread before or after", &["⌥ ⌘ ↑", "⌥ ⌘ ↓"], "previous next up down"),
        sc("Threads", "Pin or unpin the thread in view", &["⇧ ⌘ P"], "pinned"),
        sc("Threads", "Settle the thread in view", &["⇧ ⌘ S"], "settled put away"),
        sc("Threads", "Take back what the newest toast offers", &["⌘ Z"], "undo"),
        sc("Threads", "Stop the run, or say no to what it asks", &["Esc"], "escape deny cancel"),
        sc("Composer", "Send", &["↩"], "return enter"),
        sc("Composer", "New line", &["⇧ ↩"], "shift return"),
        sc("Composer", &format!("While the agent works: {}", lower_first(first)), &["↩"], "steer queue return"),
        sc("Composer", &format!("While the agent works: {}", lower_first(second)), &["⌥ ↩"], "steer queue option return"),
        sc("Composer", "While the agent works: interrupt and send now", &["⌘ ↩"], "interrupt command return"),
        sc("Composer", "The last message, or the newest queued one, in an empty composer", &["↑"], "recall edit"),
        sc("Composer", "Start a new thread without opening it (New thread)", &["⌘ ↩"], "background"),
        sc("Pages", "Mark the Briefing read", &["⌘ ↩"], "briefing"),
        sc("Pages", "Search settings (in Settings)", &["⌘ F"], "find"),
    ]
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// A shortcut's row: what it does at the left, its keys at the right ("or" between two).
fn line(t: &Theme, i: usize, r: &Shortcut) -> El {
    let key = format!("shortcut-{i}");
    let mut keys = row().gap(t.space.at(1.5)).center_y();
    for (n, k) in r.keys.iter().enumerate() {
        if n > 0 {
            keys = keys.child(text("or").font(t.ty.sm).color(t.ink_faint()).a11y_hidden());
        }
        keys = keys.child(Kbd::new(format!("{key}-keys-{n}"), *k).build(t));
    }
    let spoken: Vec<String> = r.keys.iter().map(|k| crate::ui::kbd::spoken(k)).collect();
    row()
        .key(key)
        .role("listitem")
        .label(format!("{}: {}", r.what, spoken.join(" or ")))
        .w_full()
        .min_h(t.metrics.row)
        .gap(t.space.at(4.0))
        .center_y()
        .child(text(r.what.clone()).font(t.ty.base).color(t.ink()).wrap().grow(1.0).shrink().min_w(0.0).a11y_hidden())
        .child(keys.a11y_hidden())
}

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let all = rows(s);
    let mut page = super::lead(t, "settings-shortcuts", "Every key Neko answers, by where it works. They cannot be changed yet.");
    let mut groups: Vec<&'static str> = Vec::new();
    for r in &all {
        if !groups.contains(&r.group) {
            groups.push(r.group);
        }
    }
    let mut body = col().w_full().gap(t.space.at(6.0)).pt(t.space.at(3.0));
    for g in groups {
        let mut list = col().key(format!("shortcuts-{}", g.to_lowercase())).role("list").label(g).w_full();
        for (i, r) in all.iter().enumerate().filter(|(_, r)| r.group == g) {
            list = list.child(f.lit(t, &format!("shortcut-{i}"), line(t, i, r)));
        }
        body = body.child(col().w_full().gap(t.space.at(1.0)).child(super::h3(t, g)).child(list));
    }
    page = page.child(body);
    page
}
