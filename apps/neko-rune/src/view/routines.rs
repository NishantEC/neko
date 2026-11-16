//! The Routines page. One line per routine: its glyph (on, running, held, failed, paused), name,
//! scope, cadence, when it is next due, and its switch. Pressing a line opens its details under it,
//! one at a time: the editor (instruction, cadence, model, Chrome, Mac apps, catch-up), "Run now",
//! the recent runs with what each did and what needs you. Standing orders, one box per workspace,
//! sit under the list. Ported from the web's Inbox page (`pages/Inbox.tsx`), same requests.

use neko_protocol::routines::{RoutineRun, RoutineView};
use rune::{El, col, el, row, text};

use super::ui::{MenuEntry, area, group, note, page, pick, quiet_line, setting_row, switch, with_menu};
use super::{primary_button, quiet_button};
use crate::app::App;
use crate::pages::{RoutineGlyph, ago, cadence_options, cadence_text, is_goal_routine, model_options, next_due_text, routine_glyph};
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(data) = &s.pages.routines else {
        let line = match s.pages.notes.get("page:routines") {
            Some(why) => why.clone(),
            None => "Looking for your routines…".to_owned(),
        };
        return page(t, "routines", "Routines", None, Vec::new(), quiet_line(t, "routines-loading", &line));
    };
    let views: Vec<&RoutineView> = data.views.iter().filter(|v| !is_goal_routine(v)).collect();
    let mut body = col().w_full().gap(4.0);
    if !data.routines_on {
        body = body.child(
            row()
                .key("routines-off")
                .w_full()
                .px(6.0)
                .py(4.0)
                .gap(10.0)
                .center_y()
                .child(text("Routines are off").font(t.ty.meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0))
                .child(quiet_button("Turn on", "routines-turn-on", t, |s: &mut App| s.call(crate::link::Call::SetRoutinesOn { enabled: true }))),
        );
    }
    if views.is_empty() {
        body = body.child(quiet_line(t, "routines-empty", "No routines yet"));
    }
    for v in &views {
        body = body.child(routine_row(s, t, layer, data, v));
        if s.pages.open_routine.as_deref() == Some(v.routine.id.as_str()) {
            body = body.child(details(s, t, layer, v));
        }
    }
    let orders = standing_orders(s, t, data);
    let mut all = col().w_full().gap(24.0).child(body);
    if let Some(o) = orders {
        all = all.child(o);
    }
    page(t, "routines", "Routines", (!views.is_empty()).then_some(views.len()), Vec::new(), all)
}

/// The glyph and the words of the routine's state, in the one status language.
fn glyph_of(g: &RoutineGlyph) -> (StatusKind, &'static str) {
    match g {
        RoutineGlyph::On => (StatusKind::Todo, "On"),
        RoutineGlyph::Running => (StatusKind::InProgress, "Running"),
        RoutineGlyph::Held(_) => (StatusKind::InProgress, "Held"),
        RoutineGlyph::Failed => (StatusKind::Failed, "Failed"),
        RoutineGlyph::Paused => (StatusKind::Backlog, "Paused"),
    }
}

fn routine_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &crate::link::RoutinesData, v: &RoutineView) -> El {
    let routine = &v.routine;
    let id = routine.id.clone();
    let toggle_id = id.clone();
    let open = s.pages.open_routine.as_deref() == Some(routine.id.as_str());
    let glyph = routine_glyph(v, s.now_ms);
    let (kind, word) = glyph_of(&glyph);
    let scope = data.scopes.iter().find(|sc| sc.workspace_id.as_deref() == Some(routine.workspace_id.as_str())).map(|sc| sc.name.clone()).unwrap_or_default();
    // A held routine says until when in the place the next run would be.
    let due = match &glyph {
        RoutineGlyph::Held(words) => words.clone(),
        _ => next_due_text(v, s.now_ms),
    };
    let due_color = if matches!(glyph, RoutineGlyph::Held(_)) { t.meaning.amber } else { t.text.muted };
    let line = row()
        .key(format!("routine-{id}"))
        .role("button")
        .label(format!("{}, {word}, {}, {due}", routine.name, cadence_text(&routine.cadence)))
        .expanded(open)
        .focusable(true)
        .w_full()
        .min_h(36.0)
        .px(6.0)
        .gap(12.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if open { t.surface.hover } else { rune::Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press({
            let id = id.clone();
            move |s: &mut App| s.open_routine(&id)
        })
        .child(status_glyph(kind, t, 14.0, Some(word)))
        .child(text(routine.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(150.0).grow(1.0).a11y_hidden())
        .child(text(scope).key(format!("routine-scope-{id}")).font(t.ty.meta).color(t.text.muted).truncate().min_w(0.0).max_w(110.0).a11y_hidden())
        .child(text(cadence_text(&routine.cadence)).key(format!("routine-cadence-{id}")).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text(due).key(format!("routine-due-{id}")).font(t.ty.meta).color(due_color).truncate().max_w(170.0).a11y_hidden())
        .child(switch(t, &format!("routine-switch-{id}"), &format!("{} {}", if routine.enabled { "Pause" } else { "Resume" }, routine.name), routine.enabled, move |s: &mut App| {
            let on = s.pages.routine(&toggle_id).is_some_and(|v| v.routine.enabled);
            s.set_routine_enabled(&toggle_id, !on)
        }));
    // Everything the row does is also in its right-click menu.
    let busy = v.state.running || v.state.run_requested;
    let mut entries: Vec<MenuEntry> = Vec::new();
    if !busy {
        let rid = id.clone();
        entries.push(("Run now".to_owned(), std::rc::Rc::new(move |s: &mut App| s.run_routine(&rid))));
    }
    let (eid, on) = (id.clone(), routine.enabled);
    entries.push((if on { "Pause" } else { "Resume" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.set_routine_enabled(&eid, !on))));
    let oid = id.clone();
    entries.push(("Edit".to_owned(), std::rc::Rc::new(move |s: &mut App| {
        if s.pages.open_routine.as_deref() != Some(oid.as_str()) {
            s.open_routine(&oid);
        }
    })));
    with_menu(s, t, layer, &format!("routine:{id}"), line, entries)
}

/// The open routine: its editor, Run now, and its recent runs.
fn details(s: &App, t: &Neko, layer: &mut Vec<El>, v: &RoutineView) -> El {
    let id = v.routine.id.clone();
    let Some(draft) = s.pages.draft.clone() else { return el() };
    let dirty = s.routine_dirty();
    let mut c = col().key(format!("routine-details-{id}")).w_full().pl(36.0).pr(6.0).pt(4.0).pb(14.0).gap(6.0);
    c = c.child(group(t, "Instruction")).child(area(s, t, "rt:instruction", "What this routine does each run", "", 6));

    let options = cadence_options(&draft.cadence);
    let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
    let current = cadence_text(&draft.cadence);
    let opts = options.clone();
    c = c.child(setting_row(
        t,
        "rt-cadence",
        "Cadence",
        Some("Repeats through the day, less often while nothing is happening, or once a day."),
        pick(s, t, layer, "rt-cadence-pick", "Cadence", &labels, &current, 240.0, move |s: &mut App, chosen: &str| {
            if let Some((_, cadence)) = opts.iter().find(|(l, _)| l == chosen)
                && let Some(d) = s.pages.draft.as_mut()
            {
                d.cadence = cadence.clone();
            }
        }),
    ));

    let models = model_options(&draft.model);
    let model_labels: Vec<String> = models.iter().map(|(l, _)| l.clone()).collect();
    let model_now = models.iter().find(|(_, val)| *val == draft.model).map(|(l, _)| l.clone()).unwrap_or_default();
    let by = models.clone();
    c = c.child(setting_row(
        t,
        "rt-model",
        "Model",
        Some("A Claude Code model alias. Sonnet unless you choose another."),
        pick(s, t, layer, "rt-model-pick", "Model", &model_labels, &model_now, 200.0, move |s: &mut App, chosen: &str| {
            if let Some((_, value)) = by.iter().find(|(l, _)| l == chosen)
                && let Some(d) = s.pages.draft.as_mut()
            {
                d.model = value.clone();
            }
        }),
    ));
    c = c.child(setting_row(
        t,
        "rt-browser",
        "Use my Chrome",
        Some("Lets this routine work in your own Chrome, where you are signed in. Chrome has to be open."),
        switch(t, "rt-browser-switch", "Use my Chrome", draft.browser, |s: &mut App| {
            if let Some(d) = s.pages.draft.as_mut() {
                d.browser = !d.browser;
            }
        }),
    ));
    c = c.child(setting_row(
        t,
        "rt-computer",
        "Use my Mac’s apps",
        Some("Lets this routine see and operate Mac apps, in the background where it can."),
        switch(t, "rt-computer-switch", "Use my Mac’s apps", draft.computer, |s: &mut App| {
            if let Some(d) = s.pages.draft.as_mut() {
                d.computer = !d.computer;
            }
        }),
    ));
    c = c.child(setting_row(
        t,
        "rt-catch-up",
        "Catch up after downtime",
        Some("If Neko or the Mac was off across this routine’s times, run it once when it is back and say how many it missed."),
        switch(t, "rt-catch-up-switch", "Catch up after downtime", draft.catch_up, |s: &mut App| {
            if let Some(d) = s.pages.draft.as_mut() {
                d.catch_up = !d.catch_up;
            }
        }),
    ));

    // Run now is quiet; Save is the page's one primary button, and only when something changed.
    let run_id = id.clone();
    let busy = v.state.running || v.state.run_requested;
    let mut actions = row().gap(8.0).center_y().pt(4.0);
    if dirty {
        actions = actions.child(primary_button("Save", "rt-save", t, |s: &mut App| s.save_routine()));
    }
    actions = actions.child(if busy {
        quiet_button(if v.state.running { "Running now" } else { "Starts within 30 seconds" }, "rt-run", t, |_| {}).disabled(true)
    } else {
        quiet_button("Run now", "rt-run", t, move |s: &mut App| s.run_routine(&run_id))
    });
    c = c.child(actions);
    if let Some(n) = note(s, t, &format!("routine:{id}")) {
        c = c.child(n);
    }
    if let Some(n) = v.state.note.as_ref().filter(|n| !n.is_empty()) {
        c = c.child(text(n.clone()).font(t.ty.meta).color(t.text.muted).wrap().px(6.0));
    }

    c = c.child(el().h(8.0)).child(group(t, "Recent runs"));
    if v.runs.is_empty() {
        c = c.child(quiet_line(t, "rt-no-runs", "Not run yet"));
    }
    for run in v.runs.iter().take(8) {
        c = c.child(run_row(s, t, run));
    }
    c
}

/// One run: when, what it did (or why it failed), and how many things need you.
fn run_row(s: &App, t: &Neko, run: &RoutineRun) -> El {
    let (kind, what) = if !run.ok {
        (StatusKind::Failed, run.error.clone().unwrap_or_else(|| "Failed".to_owned()))
    } else if run.did.is_empty() {
        (StatusKind::Done, "Quiet: nothing to do".to_owned())
    } else {
        let more = if run.did.len() > 1 { format!(" (+{} more)", run.did.len() - 1) } else { String::new() };
        (StatusKind::Done, format!("{}{more}", run.did[0]))
    };
    let open = run.needs_you.iter().filter(|i| !i.done).count();
    let mut r = row()
        .key(format!("run-{}", run.id))
        .w_full()
        .min_h(28.0)
        .px(6.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .help(what.clone())
        .child(status_glyph(kind, t, 12.0, Some(if run.ok { "Done" } else { "Failed" })))
        .child(text(ago(s.now_ms, run.finished_ms.unwrap_or(run.started_ms))).font(t.ty.id).color(t.text.faint).w(64.0))
        .child(text(what).font(t.ty.meta).color(if run.ok { t.text.base } else { t.meaning.coral }).truncate().grow(1.0).min_w(0.0));
    if open > 0 {
        r = r.child(text(format!("{open} {}", if open == 1 { "needs you" } else { "need you" })).key(format!("run-needs-{}", run.id)).font(t.ty.meta).color(t.meaning.amber));
    } else if !run.needs_you.is_empty() {
        r = r.child(text("all handled").font(t.ty.meta).color(t.text.faint));
    }
    r
}

/// Standing orders: one box per workspace that has routines, under "Standing orders".
fn standing_orders(s: &App, t: &Neko, data: &crate::link::RoutinesData) -> Option<El> {
    let scopes: Vec<_> = data
        .scopes
        .iter()
        .filter(|sc| sc.workspace_id.as_ref().is_some_and(|w| data.views.iter().any(|v| &v.routine.workspace_id == w && !is_goal_routine(v))))
        .collect();
    if scopes.is_empty() {
        return None;
    }
    let mut c = col().key("standing-orders").w_full().gap(10.0).child(group(t, "Standing orders"));
    for scope in scopes {
        let Some(ws) = scope.workspace_id.clone() else { continue };
        let key = format!("orders:{ws}");
        let mut block = col().key(format!("orders-block-{ws}")).w_full().gap(4.0).px(6.0);
        block = block.child(text(scope.name.clone()).font(t.ty.meta).color(t.text.muted)).child(area(s, t, &key, &format!("Standing orders for {}", scope.name), "Rules every routine here follows", 3));
        if s.orders_dirty(&ws) {
            let save = ws.clone();
            block = block.child(row().child(quiet_button("Save standing orders", &format!("orders-save-{ws}"), t, move |s: &mut App| s.save_orders(&save))));
        }
        if let Some(n) = note(s, t, &key) {
            block = block.child(n);
        }
        c = c.child(block);
    }
    Some(c)
}
