//! Setup, Memory: what Neko has learned and been told to remember, by scope; the suggestions waiting for a
//! yes or no; the two options; and working style, contextual guidance that shapes choices and never grants a
//! permission. Search, add, edit and forget (with a confirmation). Ported from `pages/setup/Memory.tsx`.

use std::collections::HashMap;

use neko_protocol::decision_context::{DecisionRecord, PreferenceState, WorkingPreference};
use neko_protocol::workbench::{MemoryEntry, MemoryProposal};
use rune::{El, col, row, text};

use super::Page;
use super::kit::{self, action, details_block, empty, glyph, list_row};
use super::words;
use crate::app::App;
use crate::icons;
use crate::link::SetupAction;
use crate::status::StatusKind;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, pick, switch, with_menu};
use crate::view::{primary_button, quiet_button};

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

    let mut trailing = Vec::new();
    if data.workspaces.len() > 1 {
        let mut names = vec!["All workspaces".to_owned()];
        names.extend(data.workspaces.iter().map(|w| w.name.clone()));
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        let current = if scope.is_empty() { "All workspaces".to_owned() } else { data.workspace_name(scope) };
        trailing.push(pick(s, t, layer, "mem-scope", "Show memory for", &names, &current, 180.0, move |s: &mut App, chosen: &str| {
            s.setup.scope = by.iter().find(|(n, _)| n == chosen).map(|(_, id)| id.clone()).unwrap_or_default();
        }));
    }
    trailing.push(primary_button("Add memory", "mem-add", t, |s: &mut App| s.open_memory_dialog(None)));

    let mut body = col().w_full().gap(2.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    if !data.memory.is_empty() {
        body = body.child(kit::text_field(s, t, "mem:search", "Search memories", "Search memories", false, false, |_| {}));
    }
    if !proposals.is_empty() {
        body = body.child(kit::heading(t, "mem-suggested-head", "Suggested", Some(proposals.len()), true));
        for p in &proposals {
            body = body.child(proposal_row(s, t, layer, data, p));
        }
    }
    if memories.is_empty() {
        let line = if query.is_empty() { "Nothing remembered yet. Tell Neko “remember that…” in Home, or add one here. Forgetting is permanent.".to_owned() } else { format!("No memory matches “{}”", s.pages.text("mem:search").trim()) };
        body = body.child(empty(t, "mem-empty", &line, query.is_empty().then(|| ("Add memory", Box::new(|s: &mut App| s.open_memory_dialog(None)) as Box<dyn Fn(&mut App)>))));
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
            body = body.child(kit::heading(t, &format!("mem-group-{i}"), name, Some(list.len()), i == 0 && proposals.is_empty() && data.memory.is_empty()));
            for m in list {
                body = body.child(memory_row(s, t, layer, data, m));
            }
        }
    }
    body = body.child(working_style(s, t, layer, data, &profile));
    body = body.child(options(s, t));
    Page { count: (!memories.is_empty()).then_some(memories.len()), trailing, body, title: None, back: None }
}

fn memory_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, m: &MemoryEntry) -> El {
    let id = m.id.clone();
    let key = format!("memory:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let source = if m.source.is_empty() { String::new() } else { format!(" · {}", words::memory_source(&m.source)) };
    let (e, f) = (id.clone(), id.clone());
    let line = list_row(t, format!("memory-{id}"), m.text.clone(), Some(open))
        .on_press(move |s: &mut App| s.toggle_row(&toggle))
        .child(text(m.text.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(m.text.clone()).a11y_hidden())
        .child(text(format!("{}{source}", words::kind_label(m.kind))).font(t.ty.meta).color(t.text.faint).truncate().min_w(0.0).a11y_hidden())
        .child(action(t, format!("memory-edit-{id}"), "Edit memory", &icons::PENCIL, move |s: &mut App| s.open_memory_dialog(Some(&e))))
        .child(action(t, format!("memory-forget-{id}"), "Forget memory…", &icons::TRASH, move |s: &mut App| s.confirm_forget_memory(&f)));
    let (me, mf) = (id.clone(), id.clone());
    let entries: Vec<MenuEntry> = vec![("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_memory_dialog(Some(&me)))), ("Forget memory…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_forget_memory(&mf)))];
    let mut out = col().key(format!("memory-block-{id}")).w_full().child(with_menu(s, t, layer, &key, line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    if open {
        let mut d = details_block(format!("memory-details-{id}"));
        d = d.child(text(m.text.clone()).key(format!("memory-full-{id}")).font(t.ty.row).color(t.text.base).wrap().px(8.0).min_w(0.0));
        if !m.source.is_empty() {
            d = d.child(text(format!("{} · {}", m.source, m.workspace_id.as_deref().map_or("Everywhere".to_owned(), |w| data.workspace_name(w)))).font(t.ty.mono_meta).color(t.text.faint).wrap().px(8.0).min_w(0.0));
        }
        out = out.child(d);
    }
    out
}

fn proposal_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, p: &MemoryProposal) -> El {
    let id = p.id.clone();
    let key = format!("memory-proposal:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let (keep, drop, g) = (id.clone(), id.clone(), id.clone());
    let place = p.workspace_id.as_deref().map_or("Everywhere".to_owned(), |w| data.workspace_name(w));
    let line = list_row(t, format!("memory-proposal-{id}"), format!("Suggested: {}", p.text), Some(open))
        .on_press(move |s: &mut App| s.toggle_row(&toggle))
        .child(glyph(t, StatusKind::Todo, "Suggested"))
        .child(text(p.text.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(p.text.clone()).a11y_hidden())
        .child(text(format!("{place} · {}", words::memory_source(&p.source))).font(t.ty.meta).color(t.text.faint).truncate().min_w(0.0).a11y_hidden())
        .child(action(t, format!("memory-keep-{id}"), "Keep memory", &icons::CHECK_LINE, move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: keep.clone(), accept: true })))
        .child(action(t, format!("memory-dismiss-{id}"), "Dismiss", &icons::CLOSE_SMALL, move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: drop.clone(), accept: false })));
    let _ = g;
    let (ma, mb) = (id.clone(), id.clone());
    let entries: Vec<MenuEntry> = vec![("Keep memory".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: ma.clone(), accept: true }))), ("Dismiss".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::DecideMemory { id: mb.clone(), accept: false })))];
    let mut out = col().key(format!("memory-proposal-block-{id}")).w_full().child(with_menu(s, t, layer, &key, line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    if open {
        out = out.child(details_block(format!("memory-proposal-details-{id}")).child(text(p.text.clone()).font(t.ty.row).color(t.text.base).wrap().px(8.0).min_w(0.0)));
    }
    out
}

/// Working style: guidance by state, and the recent decisions that can be corrected.
fn working_style(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::SetupData, profile: &str) -> El {
    let scope = s.setup.scope.as_str();
    let prefs: Vec<&WorkingPreference> = data.preferences.iter().filter(|p| (scope.is_empty() || p.workspace_id == scope) && p.agent_profile_id == profile).collect();
    let records: Vec<&DecisionRecord> = latest_decisions(&data.decisions).into_iter().filter(|r| scope.is_empty() || r.workspace_id == scope).take(12).collect();
    let mut c = col().key("mem-style").w_full().gap(2.0);
    c = c.child(row().w_full().gap(8.0).center_y().mt(16.0).child(kit::heading(t, "mem-style-head", "Working style", (!prefs.is_empty()).then_some(prefs.len()), true).mt(0.0).grow(1.0)).child(quiet_button("Add guidance", "mem-add-guidance", t, |s: &mut App| s.open_preference_dialog(None))));
    if prefs.is_empty() {
        c = c.child(kit::words(t, "mem-style-empty", "No guidance yet. Add some for a recurring situation, or correct a decision below."));
    }
    for (state, title) in [(PreferenceState::Proposed, "Needs your confirmation"), (PreferenceState::Confirmed, "Confirmed"), (PreferenceState::Dismissed, "Dismissed")] {
        let list: Vec<&&WorkingPreference> = prefs.iter().filter(|p| p.state == state).collect();
        if list.is_empty() {
            continue;
        }
        let key = format!("pref-group:{title}");
        // Dismissed guidance starts folded.
        let open = if state == PreferenceState::Dismissed { s.row_open(&key) } else { !s.row_open(&key) };
        let toggle = key.clone();
        c = c.child(list_row(t, format!("pref-group-{}", title.to_lowercase().replace(' ', "-")), format!("{title}, {}", list.len()), Some(open)).on_press(move |s: &mut App| s.toggle_row(&toggle)).child(text(title.to_owned()).font(t.ty.group).color(t.text.muted).a11y_hidden()).child(text(list.len().to_string()).font(t.ty.meta).color(t.text.faint).a11y_hidden()));
        if open {
            for p in list {
                c = c.child(preference_row(s, t, layer, data, p));
            }
        }
    }
    if !records.is_empty() {
        c = c.child(kit::heading(t, "mem-decisions-head", "Recent decisions", Some(records.len()), false));
        for r in records {
            c = c.child(decision_row(t, r));
        }
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
    let mut line = list_row(t, format!("preference-{id}"), p.instruction.clone(), Some(open)).on_press(move |s: &mut App| s.toggle_row(&toggle));
    if !confirmed {
        line = line.child(glyph(t, StatusKind::Todo, "Needs confirmation"));
    }
    line = line.child(text(p.instruction.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(p.instruction.clone()).a11y_hidden()).child(text(applies.clone()).font(t.ty.meta).color(t.text.faint).truncate().min_w(0.0).max_w(220.0).help(format!("Applies to: {applies}")).a11y_hidden());
    let (k, e, d, f) = (id.clone(), id.clone(), id.clone(), id.clone());
    if !confirmed {
        let (w2, k2) = (ws.clone(), k.clone());
        line = line.child(action(t, format!("preference-keep-{id}"), "Keep guidance", &icons::CHECK_LINE, move |s: &mut App| s.setup_call(SetupAction::KeepPreference { workspace_id: w2.clone(), id: k2.clone(), version })));
    }
    line = line.child(action(t, format!("preference-edit-{id}"), "Edit guidance", &icons::PENCIL, move |s: &mut App| s.open_preference_dialog(Some(&e))));
    if !dismissed {
        let (w2, d2) = (ws.clone(), d.clone());
        line = line.child(action(t, format!("preference-dismiss-{id}"), "Dismiss guidance", &icons::CLOSE_SMALL, move |s: &mut App| s.setup_call(SetupAction::DismissPreference { workspace_id: w2.clone(), id: d2.clone(), version })));
    }
    line = line.child(action(t, format!("preference-forget-{id}"), "Forget guidance…", &icons::TRASH, move |s: &mut App| s.confirm_forget_preference(&f)));
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
    let mut out = col().key(format!("preference-block-{id}")).w_full().child(with_menu(s, t, layer, &key, line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    if open {
        let except = if p.exceptions.is_empty() { String::new() } else { format!(" · Except: {}", p.exceptions.join(", ")) };
        out = out.child(details_block(format!("preference-details-{id}")).child(kit::words(t, &format!("preference-d-{id}"), &format!("Applies to: {applies}{except} · {} · version {}", data.workspace_name(&p.workspace_id), p.version))));
    }
    out
}

fn decision_row(t: &Neko, r: &DecisionRecord) -> El {
    let id = r.id.clone();
    let said = match &r.correction {
        Some(c) if !c.text.is_empty() => format!("Your correction: {}", c.text),
        _ => r.observed_outcome.clone().filter(|o| !o.is_empty()).unwrap_or_else(|| "Outcome not yet observed".to_owned()),
    };
    let rid = id.clone();
    let mut line = list_row(t, format!("decision-{id}"), format!("{}, {said}", words::decision_title(r.action)), None)
        .child(text(words::decision_title(r.action).to_owned()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(260.0).a11y_hidden())
        .child(text(said).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if r.delivery_stage == neko_protocol::decision_context::DeliveryStage::LocalAccepted {
        line = line.child(text("Accepted locally".to_owned()).font(t.ty.meta).color(t.text.faint).a11y_hidden());
    }
    line.child(action(t, format!("decision-correct-{id}"), "Correct this decision", &icons::PENCIL, move |s: &mut App| s.open_correct_dialog(&rid)))
}

fn options(s: &App, t: &Neko) -> El {
    let opts = s.setup_data().and_then(|d| d.memory_options).unwrap_or_default();
    let mk = |key: &str, label: &str, help: &str, detail: &str, on: bool, learning: bool| {
        row()
            .key(format!("mem-option-{key}"))
            .w_full()
            .min_h(kit::ROW_H)
            .px(8.0)
            .gap(10.0)
            .center_y()
            .radius(t.radius.row)
            .hover(t.surface.hover)
            .help(help.to_owned())
            .child(text(label.to_owned()).font(t.ty.row).color(t.text.base).a11y_hidden())
            .child(text(detail.to_owned()).font(t.ty.meta).color(t.text.muted).grow(1.0).min_w(0.0).a11y_hidden())
            .child(switch(t, &format!("mem-switch-{key}"), label, on, move |s: &mut App| if learning { s.set_memory_options(Some(!on), None) } else { s.set_memory_options(None, Some(!on)) }))
    };
    col()
        .key("mem-options")
        .w_full()
        .gap(2.0)
        .child(kit::heading(t, "mem-options-head", "Settings", None, false))
        .child(mk("learning", "Suggest new memories", "Propose context from chats and agents. Direct requests to remember something still work when this is off.", "from chats and agents", opts.learning, true))
        .child(mk("use", "Use saved memory", "Include memory in replies and tasks. Turning this off keeps saved memories here.", "in replies and tasks", opts.use_memory, false))
}
