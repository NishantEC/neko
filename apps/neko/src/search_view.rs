//! The Search palette's rows and its mount (docs/knowledge/search.md): rune-ui's Command, centred over a scrim.
//!
//! Items are built here, in the view, from the app's own lists and from what the daemon last found (`CommandItem` is not `Send`).
//! The daemon's threads and kittens carry the typed text as hidden keywords when Command could not match them itself, so it keeps them
//! whatever the daemon matched them by (an accent, a filter); Command's own fuzzy matching marks the letters in the title and in the
//! snippet, and filters Cattains, projects and actions as the text is typed.

use neko_protocol::{FoundBy, Task};
use rune::{El, Key, KeyEvent, Tx, col, el};

use crate::search::{ACTIONS, LIMIT, Row};
use crate::state::App;
use crate::ui::base::Theme;
use crate::ui::command::{Command, CommandItem};

/// The longest a project's name is in a row's hint before it is cut: a long name must not take the title's room.
const PROJECT_CHARS: usize = 16;

/// `words` cut to `max` characters with an ellipsis.
fn short(words: &str, max: usize) -> String {
    if words.chars().count() <= max {
        return words.to_owned();
    }
    format!("{}…", words.chars().take(max - 1).collect::<String>().trim_end())
}

/// The sheet's width.
pub const WIDTH: f32 = 600.0;
/// The most rows shown.
const ROWS: usize = LIMIT as usize;

/// How far down the window the sheet's top edge sits: about where its middle would be a little above the window's, and the same
/// whatever is found, so the field never moves while results come and go. The tallest the sheet gets is about 450 points.
pub fn top(window_height: f32) -> f32 {
    (window_height * 0.5 - 235.0).clamp(28.0, 150.0)
}

/// A thread's or kitten's row: its status mark, its title, the words it was found by under the title when they are not the title's,
/// and at the right its project and age (with "settled" or "archived" when it is put away).
fn found_item(s: &App, t: &Theme, task: &Task, kitten: bool, by: FoundBy, snippet: &str, typed: &str) -> CommandItem {
    let row = if kitten { Row::Kitten(task.id.clone()) } else { Row::Thread(task.id.clone()) };
    let under = t.c.floating.bg;
    let mut item = CommandItem::new(row.key(), task.title.clone(), if kitten { "Kittens" } else { "Threads" }).lead(crate::status_mark::bare_mark(t, &format!("pal-mark-{}", task.id.0), task, under, t.control.icon_lg));
    // What was said, when that is why the row is here; a kitten says which thread it belongs to when it is found by its title.
    if by == FoundBy::Message && !snippet.is_empty() {
        item = item.subtitle(snippet.to_owned());
    } else if kitten && let Some(parent) = s.thread_of(task) {
        let number = task.subagent.as_ref().map(|o| crate::subagents::name(&o.number)).unwrap_or_default();
        item = item.subtitle(format!("{number} in {}", parent.title));
    }
    let age = crate::threads::age(s.now_ms, s.utc_offset_s, task.activity_ms.max(task.created_ms));
    let project = s.workspace_of(&task.workspace).filter(|w| !w.is_general()).map(|w| short(&w.name, PROJECT_CHARS));
    let kept = if task.archived { Some("archived") } else if task.settled { Some("settled") } else { None };
    let hint: Vec<String> = kept.map(str::to_owned).into_iter().chain(project).chain(std::iter::once(age)).collect();
    item = item.hint(hint.join(" · "));
    // The daemon matched it by something Command's own fuzzy matching may not see (a stem, an accent, a filter): only then do the typed
    // words go in as hidden keywords, so a row Command can match itself has its letters marked in the title or the snippet.
    if !typed.is_empty() && crate::ui::command::search(std::slice::from_ref(&item), typed, crate::ui::command::Match::Fuzzy).is_empty() {
        item = item.keywords(typed.to_owned());
    }
    item
}

/// Every row the palette has for the text now in its field, in the order its sections are shown, and the keys of the recent ones.
pub fn items(s: &App, t: &Theme) -> (Vec<CommandItem>, Vec<String>) {
    let typed = s.palette.query.text.trim();
    let mut threads: Vec<CommandItem> = Vec::new();
    let mut kittens: Vec<CommandItem> = Vec::new();
    // Nothing typed: the recent threads (the daemon's answer to the empty text); stale answers for other text do not show.
    let fresh = if typed.is_empty() { s.palette.answered.as_deref() == Some("") } else { s.palette.answered.is_some() };
    if fresh {
        for h in &s.palette.hits {
            let Some(task) = s.tasks.iter().find(|x| x.id == h.task) else { continue };
            let item = found_item(s, t, task, h.kitten, h.by, &h.snippet, typed);
            if h.kitten { kittens.push(item) } else { threads.push(item) }
        }
    }
    let recents: Vec<String> = if typed.is_empty() { threads.iter().map(|i| i.key.clone()).collect() } else { Vec::new() };
    let cattains: Vec<CommandItem> = s
        .active_cats()
        .into_iter()
        .map(|c| {
            let purpose = c.purpose.clone().unwrap_or_default();
            CommandItem::new(Row::Cattain(c.id.clone()).key(), c.name.clone(), "Cattains").lead(crate::faces::portrait(t, &format!("pal-cat-{}", c.id), &c.face, t.control.icon_lg)).keywords(format!("cattain cat {purpose}"))
        })
        .collect();
    let projects: Vec<CommandItem> = s
        .workspaces
        .iter()
        .filter(|w| !w.is_general() && w.is_active())
        .map(|w| CommandItem::new(Row::Project(w.id.clone()).key(), w.name.clone(), "Projects").icon(&crate::ui::icons::FOLDER).hint("New thread").keywords("project folder"))
        .collect();
    let actions: Vec<CommandItem> = ACTIONS
        .iter()
        .filter(|a| s.action_offered(a.id))
        .map(|a| {
            let mut i = CommandItem::new(Row::Action(a.id).key(), a.label, "Actions").icon(action_icon(a.id)).keywords(a.keywords);
            if let Some(k) = a.keys {
                i = i.shortcut(k);
            }
            i
        })
        .collect();
    let mut all = Vec::new();
    if typed.is_empty() {
        // The first thing a person does with nothing typed is go somewhere: the recent threads, then the verbs.
        all.extend(threads);
        all.extend(actions);
        all.extend(cattains);
        all.extend(projects);
    } else {
        all.extend(threads);
        all.extend(kittens);
        all.extend(cattains);
        all.extend(projects);
        all.extend(actions);
    }
    (all, recents)
}

fn action_icon(id: &str) -> &'static rune::Icon {
    use crate::ui::icons as i;
    match id {
        "new-thread" => &crate::icons::COMPOSE,
        "needs-you" => &i::INBOX,
        "briefing" => &crate::icons::BRIEFING,
        "rounds" | "pause-rounds" | "resume-rounds" => &crate::icons::ROUNDS,
        "pane" => &i::PANEL_RIGHT,
        "sidebar" => &crate::icons::SIDEBAR,
        "show-archived" | "hide-archived" | "show-settled" | "hide-settled" => &crate::icons::LIST,
        _ => &i::GEAR,
    }
}

/// The palette, mounted as a modal over the window while it is open.
pub fn palette(s: &App, t: &Theme, window_height: f32, layer: &mut Vec<El>) {
    if !s.palette.open {
        return;
    }
    let (items, recents) = items(s, t);
    let empty = s.palette.problem.clone().filter(|_| !s.palette_loading()).unwrap_or_else(|| "No results".to_owned());
    let panel = Command::<App>::new("palette", &s.palette.query, |s: &mut App| &mut s.palette.query)
        .items(&items)
        .placeholder("Search threads, kittens, Cattains and actions")
        .highlight(s.palette.at)
        .recents(&recents)
        .recent_label("Recent")
        .loading(s.palette_loading())
        .empty(empty)
        .limit(ROWS)
        .width(WIDTH)
        .top(top(window_height))
        .hint("↩", "Open")
        .hint("⌘ ↩", "Open beside")
        .hint("Tab", "New thread")
        .hint("Esc", "Close")
        .on_highlight(|s: &mut App, i| s.palette.at = i)
        .on_select(|s: &mut App, key: &str| s.palette_choose(key, false))
        .on_alt(|s: &mut App, key: &str| s.palette_choose(key, true))
        .build(t);
    // Tab turns the text into a new thread. The field would otherwise pass it on as a move of focus, so it is taken before it gets there;
    // the rest is Command's own `dialog` (a scrim, the sheet placed from the top, focus trapped, Escape and a press outside close it).
    let panel = panel.on_key_preview(|s: &mut App, k: &KeyEvent, tx: &mut Tx| {
        let plain = !(k.mods.cmd || k.mods.shift || k.mods.alt || k.mods.ctrl);
        if k.key == Key::Tab && plain {
            s.palette_new_thread();
            tx.instant();
            return true;
        }
        false
    });
    let panel = panel.focus_return("content").modal(true, |s: &mut App| s.close_palette());
    layer.push(el().key("palette-scrim").absolute(0.0, 0.0).w_full().h_full().fill(t.scrim_soft()).a11y_hidden());
    layer.push(col().key("palette-layer").absolute(0.0, 0.0).w_full().h_full().pt(top(window_height)).items_center().child(panel));
}
