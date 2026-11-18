//! Setup, Memory: what Neko has learned and been told to remember, by scope; the suggestions waiting for a
//! yes or no; the two options; and working style, contextual guidance that shapes choices and never grants a
//! permission. Search, add, edit and forget (with a confirmation). Ported from `pages/setup/Memory.tsx`.

use std::collections::HashMap;

use neko_protocol::decision_context::{DecisionRecord, PreferenceState, WorkingPreference};
use neko_protocol::workbench::{MemoryEntry, MemoryProposal};
use rune::{El, col, row, text};

use super::Page;
use super::kit::{self, glyph};
use super::words;
use crate::app::App;
use crate::icons;
use crate::icons_phosphor;
use crate::link::SetupAction;
use crate::status::StatusKind;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::view::webkit;

/// The newest version of each decision episode, newest first.
pub fn latest_decisions(records: &[DecisionRecord]) -> Vec<&DecisionRecord> {
    let mut episodes: HashMap<&str, &DecisionRecord> = HashMap::new();
    for r in records {
        match episodes.get(r.episode_id.as_str()) {
            Some(prior) if prior.version >= r.version => {}
            _ => {
                episodes.insert(r.episode_id.as_str(), r);
            }
        }
    }
    let mut out: Vec<&DecisionRecord> = episodes.into_values().collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.created_at_ms));
    out
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "mem-none", "Looking for your memories…") };
    let scope = s.setup.scope.as_str();
    let profile = s.memory_profile();
    let query = s.pages.text("mem:search").trim().to_lowercase();
    let in_scope = |ws: Option<&str>| ws.is_none() || scope.is_empty() || ws == Some(scope);
    let memories: Vec<&MemoryEntry> = data.memory.iter().filter(|m| m.agent_profile_id == profile && in_scope(m.workspace_id.as_deref()) && (query.is_empty() || m.text.to_lowercase().contains(&query))).collect();
    let proposals: Vec<&MemoryProposal> = data.memory_proposals.iter().filter(|m| m.agent_profile_id == profile && in_scope(m.workspace_id.as_deref())).collect();
    let style = s.row_open(STYLE_TAB);

    // `Tabs` (`Memory.tsx`): the tab list at the left; at the right the workspace picker and "Add memory" (only on Saved memories),
    // wrapping under the tabs when they do not fit.
    let tabs = webkit::tabs(
        t,
        vec![
            ("mem-tab-memories".to_owned(), "Saved memories".to_owned(), !style, Box::new(|s: &mut App| {
                s.setup.open.remove(STYLE_TAB);
            })),
            ("mem-tab-style".to_owned(), "Working style".to_owned(), style, Box::new(|s: &mut App| {
                s.setup.open.insert(STYLE_TAB.to_owned());
            })),
        ],
    );
    let mut right = row().gap(8.0).center_y();
    if data.workspaces.len() > 1 {
        let mut names = vec!["All workspaces".to_owned()];
        names.extend(data.workspaces.iter().map(|w| w.name.clone()));
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        let current = if scope.is_empty() { "All workspaces".to_owned() } else { data.workspace_name(scope) };
        right = right.child(webkit::pick(s, t, layer, "mem-scope", "Show memory for", &names, &current, move |s: &mut App, chosen: &str| {
            s.setup.scope = by.iter().find(|(n, _)| n == chosen).map(|(_, id)| id.clone()).unwrap_or_default();
        }));
    }
    if !style {
        right = right.child(webkit::button(t, "mem-add", "Add memory", Some(&icons::PLUS), true, false, |s: &mut App| s.open_memory_dialog(None)));
        // Rune-only: the search over the saved memories, last in the group (it opens a field under the tabs).
        if !data.memory.is_empty() {
            let on = s.row_open(SEARCH_OPEN);
            right = right.child(row().key("mem-search-toggle").role("button").label("Search memories").help("Search memories").checked(on).focusable(true).size(36.0, 36.0).center().radius(18.0).fill(if on { t.text.base.alpha(0.1) } else { rune::Color::TRANSPARENT }).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| s.toggle_row(SEARCH_OPEN)).child(rune::icon(&crate::icons_phosphor::MAGNIFYING_GLASS, 16.0).color(t.text.base).a11y_hidden()));
        }
    }
    let top = row().w_full().gaps(12.0, 12.0).flex_wrap().center_y().justify_between().child(tabs).child(right);

    let content = if style { working_style(s, t, layer, data, &profile) } else { saved(s, t, layer, data, &memories, &proposals, &query) };
    let body = col().key("mem-body").w_full().gap(8.0).child(top).child(col().w_full().pt(24.0).child(content));
    Page { count: None, trailing: Vec::new(), body, title: None, back: None }
}

/// What the Memory tab holds that is Rune's own: whether the Working style tab is open, and the search field.
const STYLE_TAB: &str = "mem-tab:style";
const SEARCH_OPEN: &str = "mem-search";

/// Whether a collapsible `Section` is open: its fold flag flips the default.
fn is_open(s: &App, flag: &str, default_open: bool) -> bool {
    s.pages.inbox_flags.contains(flag) != default_open
}

/// "Saved memories": the suggestions, the memories by scope, then the two options.
fn saved(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, memories: &[&MemoryEntry], proposals: &[&MemoryProposal], query: &str) -> El {
    let mut body = col().w_full().gap(32.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    if s.row_open(SEARCH_OPEN) || !query.is_empty() {
        body = body.child(kit::text_field(s, t, "mem:search", "Search memories", "Search memories", false, false, |_| {}));
    }
    if !proposals.is_empty() {
        let rows: Vec<El> = proposals.iter().map(|p| proposal_row(s, t, layer, data, p)).collect();
        body = body.child(webkit::section(t, "mem-suggested", "Suggested", Some(&proposals.len().to_string()), None, webkit::panel(rows)));
    }
    if memories.is_empty() {
        if query.is_empty() {
            body = body.child(webkit::empty(t, "mem-empty", &icons_phosphor::BRAIN, "Nothing remembered yet", "Tell Neko “remember that…” in Home, or add one here. Forgetting is permanent.", Some(("mem-empty-action", "Add memory", Box::new(|s: &mut App| s.open_memory_dialog(None))))));
        } else {
            body = body.child(kit::empty(t, "mem-empty", &format!("No memory matches “{}”", s.pages.text("mem:search").trim()), None));
        }
    } else {
        // Everywhere first, then each workspace in the order it is listed.
        let mut scopes: Vec<(String, Vec<&MemoryEntry>)> = Vec::new();
        for name in std::iter::once("Everywhere".to_owned()).chain(data.workspaces.iter().map(|w| w.name.clone())) {
            let list: Vec<&MemoryEntry> = memories.iter().copied().filter(|m| match &m.workspace_id { None => name == "Everywhere", Some(id) => data.workspace_name(id) == name }).collect();
            if !list.is_empty() {
                scopes.push((name, list));
            }
        }
        let known: Vec<&MemoryEntry> = scopes.iter().flat_map(|(_, l)| l.iter().copied()).collect();
        let other: Vec<&MemoryEntry> = memories.iter().copied().filter(|m| !known.iter().any(|k| k.id == m.id)).collect();
        if !other.is_empty() {
            scopes.push(("Workspace".to_owned(), other));
        }
        for (i, (name, list)) in scopes.iter().enumerate() {
            let rows: Vec<El> = list.iter().map(|m| memory_row(s, t, layer, data, m)).collect();
            let flag = format!("mem-fold:{name}");
            body = body.child(webkit::section_collapsible(t, &format!("mem-group-{i}"), name, Some(&list.len().to_string()), is_open(s, &flag, true), &flag, webkit::panel(rows)));
        }
    }
    body.child(options(s, t))
}

fn memory_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, m: &MemoryEntry) -> El {
    let id = m.id.clone();
    let key = format!("memory:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let source = if m.source.is_empty() { String::new() } else { format!(" · {}", words::memory_source(&m.source)) };
    let (e, f, g) = (id.clone(), id.clone(), id.clone());
    let more = webkit::more_menu(
        s,
        t,
        layer,
        &format!("memory-more-{id}"),
        "Memory actions",
        false,
        vec![
            Some(("Edit".to_owned(), &icons::PENCIL, false, true, std::rc::Rc::new(move |s: &mut App| s.open_memory_dialog(Some(&g))) as std::rc::Rc<dyn Fn(&mut App)>)),
            None,
            Some(("Forget memory…".to_owned(), &icons::TRASH, true, true, std::rc::Rc::new(move |s: &mut App| s.confirm_forget_memory(&f)))),
        ],
    );
    let line = vec![
        webkit::one_line(t, &format!("memory-{id}"), &m.text, open, move |s: &mut App| s.toggle_row(&toggle)),
        webkit::faint_nodes(t, &if source.is_empty() { vec![words::kind_label(m.kind).to_owned()] } else { vec![words::kind_label(m.kind).to_owned(), source.clone()] }),
        webkit::row_actions(vec![webkit::row_action(t, &format!("memory-edit-{id}"), "Edit memory", &icons::PENCIL, move |s: &mut App| s.open_memory_dialog(Some(&e))), more]),
    ];
    let mut below: Vec<El> = Vec::new();
    if open && !m.source.is_empty() {
        below.push(text(format!("{} · {}", m.source, m.workspace_id.as_deref().map_or("Everywhere".to_owned(), |w| data.workspace_name(w)))).key(format!("memory-source-{id}")).font(rune::TextStyle { size: 11.0, line_height: 16.5, ..t.ty.mono_meta }).color(t.text.muted).wrap().min_w(0.0));
    }
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        below.push(n);
    }
    let (me, mf) = (id.clone(), id.clone());
    let entries: Vec<MenuEntry> = vec![("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_memory_dialog(Some(&me)))), ("Forget memory…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_forget_memory(&mf)))];
    with_menu(s, t, layer, &key, webkit::item_row(t, &format!("memory-block-{id}"), line, below), entries)
}

fn proposal_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, p: &MemoryProposal) -> El {
    let id = p.id.clone();
    let key = format!("memory-proposal:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let (keep, drop, keep2, drop2, keep3) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
    let place = p.workspace_id.as_deref().map_or("Everywhere".to_owned(), |w| data.workspace_name(w));
    let line = vec![
        glyph_button(t, &format!("memory-proposal-keep-{id}"), "Keep memory", move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: keep3.clone(), accept: true })),
        webkit::one_line(t, &format!("memory-proposal-{id}"), &p.text, open, move |s: &mut App| s.toggle_row(&toggle)),
        webkit::faint_nodes(t, &[place, " · ".to_owned(), words::memory_source(&p.source).to_owned()]),
        webkit::row_actions(vec![
            webkit::row_action(t, &format!("memory-keep-{id}"), "Keep memory", &icons::CHECK_LINE, move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: keep.clone(), accept: true })),
            webkit::row_action(t, &format!("memory-dismiss-{id}"), "Dismiss", &icons::CLOSE_SMALL, move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: drop.clone(), accept: false })),
        ]),
    ];
    let _ = (keep2, drop2);
    let mut below: Vec<El> = Vec::new();
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        below.push(n);
    }
    let (ma, mb) = (id.clone(), id.clone());
    let entries: Vec<MenuEntry> = vec![("Keep memory".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: ma.clone(), accept: true }))), ("Dismiss".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: mb.clone(), accept: false })))];
    with_menu(s, t, layer, &key, webkit::item_row(t, &format!("memory-proposal-block-{id}"), line, below), entries)
}

/// `GlyphButton` (`ui/status-glyph.tsx`): the to-do circle that keeps the suggestion when pressed.
fn glyph_button(t: &Neko, key: &str, label: &str, on_press: impl Fn(&mut App) + 'static) -> El {
    row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(14.0, 14.0).center().radius(7.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press).child(glyph(t, StatusKind::Todo, label))
}

/// "Working style": the guidance row, guidance by state, and the recent decisions that can be corrected.
fn working_style(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, profile: &str) -> El {
    let scope = s.setup.scope.as_str();
    let prefs: Vec<&WorkingPreference> = data.preferences.iter().filter(|p| (scope.is_empty() || p.workspace_id == scope) && p.agent_profile_id == profile).collect();
    let records: Vec<&DecisionRecord> = latest_decisions(&data.decisions).into_iter().filter(|r| scope.is_empty() || r.workspace_id == scope).take(12).collect();
    let mut c = col().key("mem-style").w_full().gap(32.0);
    if let Some(n) = note(s, t, "setup:page") {
        c = c.child(n);
    }
    let intro = if prefs.is_empty() { "No guidance yet. Add some for a recurring situation, or correct a decision below." } else { "Guidance shapes choices within the permissions you already gave." };
    c = c.child(row().w_full().gap(12.0).center_y().justify_between().child(text(intro.to_owned()).key("mem-style-empty").font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.muted).wrap().min_w(0.0).shrink()).child(webkit::ghost_button(t, "mem-add-guidance", "Add guidance", Some(&icons::PLUS), |s: &mut App| s.open_preference_dialog(None))));
    for (state, title) in [(PreferenceState::Proposed, "Needs your confirmation"), (PreferenceState::Confirmed, "Confirmed"), (PreferenceState::Dismissed, "Dismissed")] {
        let list: Vec<&WorkingPreference> = prefs.iter().copied().filter(|p| p.state == state).collect();
        if list.is_empty() {
            continue;
        }
        let flag = format!("pref-fold:{title}");
        let open = is_open(s, &flag, state != PreferenceState::Dismissed);
        let rows: Vec<El> = if open { list.iter().map(|p| preference_row(s, t, layer, data, p)).collect() } else { Vec::new() };
        c = c.child(webkit::section_collapsible(t, &format!("pref-group-{}", title.to_lowercase().replace(' ', "-")), title, Some(&list.len().to_string()), open, &flag, webkit::panel(rows)));
    }
    if !records.is_empty() {
        let n = records.len().to_string();
        let rows: Vec<El> = records.into_iter().map(|r| decision_row(t, r)).collect();
        c = c.child(webkit::section(t, "mem-decisions", "Recent decisions", Some(&n), None, webkit::panel(rows)));
    }
    c
}

fn preference_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, p: &WorkingPreference) -> El {
    let id = p.id.clone();
    let key = format!("preference:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let mut applies: Vec<String> = p.applicability.terms.clone();
    applies.extend(p.applicability.task_ids.iter().map(|t| format!("NEK-{}", t.chars().take(4).collect::<String>().to_uppercase())));
    let applies = if applies.is_empty() { "—".to_owned() } else { applies.join(", ") };
    let (ws, version) = (p.workspace_id.clone(), p.version);
    let confirmed = p.state == PreferenceState::Confirmed;
    let dismissed = p.state == PreferenceState::Dismissed;
    let (k, e, d, f) = (id.clone(), id.clone(), id.clone(), id.clone());
    let mut line: Vec<El> = Vec::new();
    if !confirmed {
        let (w2, k2) = (ws.clone(), k.clone());
        line.push(glyph_button(t, &format!("preference-glyph-{id}"), "Keep guidance", move |s: &mut App| s.setup_call(SetupAction::KeepPreference { workspace_id: w2.clone(), id: k2.clone(), version })));
    }
    line.push(webkit::one_line(t, &format!("preference-{id}"), &p.instruction, open, move |s: &mut App| s.toggle_row(&toggle)));
    line.push(text(applies.clone()).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted.alpha(0.7)).truncate().max_w(0.35 * (webkit::page_width() - 24.0)).help(format!("Applies to: {applies}")).a11y_hidden());
    let mut acts: Vec<El> = Vec::new();
    if !confirmed {
        let (w2, k2) = (ws.clone(), k.clone());
        acts.push(webkit::row_action(t, &format!("preference-keep-{id}"), "Keep guidance", &icons::CHECK_LINE, move |s: &mut App| s.setup_call(SetupAction::KeepPreference { workspace_id: w2.clone(), id: k2.clone(), version })));
    }
    acts.push(webkit::row_action(t, &format!("preference-edit-{id}"), "Edit guidance", &icons::PENCIL, move |s: &mut App| s.open_preference_dialog(Some(&e))));
    if !dismissed {
        let (w2, d2) = (ws.clone(), d.clone());
        acts.push(webkit::row_action(t, &format!("preference-dismiss-{id}"), "Dismiss guidance", &icons::CLOSE_SMALL, move |s: &mut App| s.setup_call(SetupAction::DismissPreference { workspace_id: w2.clone(), id: d2.clone(), version })));
    }
    acts.push(webkit::row_action(t, &format!("preference-forget-{id}"), "Forget guidance…", &icons::TRASH, move |s: &mut App| s.confirm_forget_preference(&f)));
    line.push(webkit::row_actions(acts));
    let mut entries: Vec<MenuEntry> = Vec::new();
    if !confirmed {
        let (w2, k2) = (ws.clone(), k.clone());
        entries.push(("Keep guidance".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::KeepPreference { workspace_id: w2.clone(), id: k2.clone(), version }))));
    }
    let me = id.clone();
    entries.push(("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_preference_dialog(Some(&me)))));
    if !dismissed {
        let (w2, d2) = (ws.clone(), d.clone());
        entries.push(("Dismiss".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::DismissPreference { workspace_id: w2.clone(), id: d2.clone(), version }))));
    }
    let mf = id.clone();
    entries.push(("Forget…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_forget_preference(&mf))));
    let mut below: Vec<El> = Vec::new();
    if open {
        let except = if p.exceptions.is_empty() { String::new() } else { format!(" · Except: {}", p.exceptions.join(", ")) };
        below.push(text(format!("Applies to: {applies}{except} · {} · version {}", data.workspace_name(&p.workspace_id), p.version)).key(format!("preference-d-{id}")).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).wrap().min_w(0.0));
    }
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        below.push(n);
    }
    with_menu(s, t, layer, &key, webkit::item_row(t, &format!("preference-block-{id}"), line, below), entries)
}

fn decision_row(t: &Neko, r: &DecisionRecord) -> El {
    let id = r.id.clone();
    let said = match &r.correction {
        Some(c) if !c.text.is_empty() => format!("Your correction: {}", c.text),
        _ => r.observed_outcome.clone().filter(|o| !o.is_empty()).unwrap_or_else(|| "Outcome not yet observed".to_owned()),
    };
    let rid = id.clone();
    let mut line = vec![
        text(words::decision_title(r.action).to_owned()).key(format!("decision-{id}")).font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.base).a11y_hidden(),
        text(said).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden(),
    ];
    if r.delivery_stage == neko_protocol::decision_context::DeliveryStage::LocalAccepted {
        line.push(webkit::faint(t, "Accepted locally"));
    }
    line.push(webkit::row_actions(vec![webkit::row_action(t, &format!("decision-correct-{id}"), "Correct this decision", &icons::PENCIL, move |s: &mut App| s.open_correct_dialog(&rid))]));
    webkit::item_row(t, &format!("decision-row-{id}"), line, Vec::new())
}

/// "Settings": the two options, each a `Row` with a switch.
fn options(s: &App, t: &Neko) -> El {
    let opts = s.setup_data().and_then(|d| d.memory_options).unwrap_or_default();
    let mk = |key: &str, label: &str, help: &str, detail: &str, on: bool, learning: bool| {
        let title = text(label.to_owned()).font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.base).truncate().help(help.to_owned()).a11y_hidden();
        let description = text(detail.to_owned()).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden();
        webkit::info_row(t, &format!("mem-option-{key}"), title, Some(description), None, vec![webkit::toggle(t, &format!("mem-switch-{key}"), label, on, move |s: &mut App| if learning { s.set_memory_options(Some(!on), None) } else { s.set_memory_options(None, Some(!on)) })], None)
    };
    let rows = vec![
        mk("learning", "Suggest new memories", "Propose context from chats and agents. Direct requests to remember something still work when this is off.", "from chats and agents", opts.learning, true),
        mk("use", "Use saved memory", "Include memory in replies and tasks. Turning this off keeps saved memories here.", "in replies and tasks", opts.use_memory, false),
    ];
    webkit::section(t, "mem-options", "Settings", None, None, webkit::panel(rows))
}
