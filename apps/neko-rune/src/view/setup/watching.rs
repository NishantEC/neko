//! Setup, Watching: what Neko keeps an eye on. A watch is an instruction plus the connected tools allowed to answer it. The page is
//! the web's (`pages/setup/Watching.tsx`): the workspace picker and the options menu, the "What should Neko keep an eye on?" card
//! (Set up watch, Suggest from tools, Use an idea), then Suggestions, Active and Paused. Edit, rename, delete (with a confirmation),
//! check now, turn it on or off (turning on is a decision and says what it allows).

use neko_protocol::mcp_host::Responsibility;
use rune::{El, col, el, row, text};

use super::Page;
use super::state::SetupSection;
use super::words::{self, Tone};
use crate::app::App;
use crate::icons;
use crate::icons_phosphor;
use crate::link::SetupData;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::view::webkit;

/// The fold flag of the Paused section (the page's own, kept in the setup rows).
const PAUSED_OPEN: &str = "watch-paused-open";
const IDEAS_OPEN: &str = "watch-ideas-open";

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "watch-none", "Looking for your watches…") };
    if data.workspaces.is_empty() {
        let body = webkit::empty(t, "watch-no-workspace", &icons_phosphor::EYE_REGULAR, "Add a workspace to create a watch", "Watches belong to a workspace, because they use that workspace’s connected tools.", Some(("watch-no-workspace-action", "Open Workspaces", Box::new(|s: &mut App| s.open_setup(SetupSection::Workspaces)))));
        return Page { count: None, trailing: Vec::new(), body, title: None, back: None };
    }
    let scope = s.setup.scope.as_str();
    let all: Vec<&Responsibility> = data.watches.iter().filter(|r| scope.is_empty() || r.workspace_id == scope).collect();
    let suggested: Vec<&Responsibility> = all.iter().copied().filter(|r| data.suggested.contains(&r.id)).collect();
    let active: Vec<&Responsibility> = all.iter().copied().filter(|r| r.enabled).collect();
    let paused: Vec<&Responsibility> = all.iter().copied().filter(|r| !r.enabled && !data.suggested.contains(&r.id)).collect();
    let first = data.workspaces[0].id.clone();

    // The top line: the workspace picker, then the options menu at the far end.
    let mut top = row().w_full().gap(8.0).flex_wrap().center_y();
    if data.workspaces.len() > 1 {
        let mut names = vec!["All workspaces".to_owned()];
        names.extend(data.workspaces.iter().map(|w| w.name.clone()));
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        let current = if scope.is_empty() { "All workspaces".to_owned() } else { data.workspace_name(scope) };
        top = top.child(webkit::pick(s, t, layer, "watch-scope", "Show watches for", &names, &current, move |s: &mut App, chosen: &str| {
            s.setup.scope = by.iter().find(|(n, _)| n == chosen).map(|(_, id)| id.clone()).unwrap_or_default();
        }));
    }
    top = top.child(el().grow(1.0).min_w(0.0));
    let target = if scope.is_empty() { first.clone() } else { scope.to_owned() };
    let active_ids: Vec<String> = active.iter().map(|r| r.id.clone()).collect();
    let options = webkit::more_menu(
        s,
        t,
        layer,
        "watch-options",
        "Watching options",
        true,
        vec![
            Some(("Write a watch yourself…".to_owned(), &icons::PLUS, false, true, std::rc::Rc::new(move |s: &mut App| s.open_watch_dialog(None, &target)) as std::rc::Rc<dyn Fn(&mut App)>)),
            Some(("Check all now".to_owned(), &icons::REFRESH, false, !active_ids.is_empty(), std::rc::Rc::new(move |s: &mut App| {
                for id in &active_ids {
                    s.setup_call(crate::link::SetupAction::Wake { id: id.clone() });
                }
                let n = active_ids.len();
                s.show_toast(&format!("Asked {n} {} to check now.", if n == 1 { "watch" } else { "watches" }));
            }))),
        ],
    );
    top = top.child(options);

    let mut body = col().key("watch-body").w_full().gap(32.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    body = body.child(top).child(new_watch(s, t, layer, data));
    if !suggested.is_empty() {
        let rows: Vec<El> = suggested.iter().map(|w| suggestion(s, t, layer, data, w)).collect();
        body = body.child(webkit::section(t, "watch-suggestions", "Suggestions", Some(&suggested.len().to_string()), None, webkit::panel(rows)));
    }
    let active_body = if active.is_empty() {
        text((if paused.is_empty() { "No active watches. Describe one above, or review a suggestion." } else { "No active watches. Resume a paused one or describe a new one above." }).to_owned()).key("watch-active-empty").font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.muted).wrap().min_w(0.0)
    } else {
        webkit::panel(active.iter().map(|w| watch_row(s, t, layer, data, w)).collect())
    };
    body = body.child(webkit::section(t, "watch-active", "Active", Some(&active.len().to_string()), None, active_body));
    if !paused.is_empty() {
        let show = s.row_open(PAUSED_OPEN);
        let toggle = webkit::ghost_xs_button(t, "watch-paused-toggle", if show { "Hide" } else { "Show" }, None, "Paused watches never check. Turn one on to confirm what it may do.", |s: &mut App| s.toggle_row(PAUSED_OPEN));
        let inner = if show { webkit::panel(paused.iter().map(|w| watch_row(s, t, layer, data, w)).collect()) } else { el() };
        body = body.child(webkit::section(t, "watch-paused", "Paused", Some(&paused.len().to_string()), Some(toggle), inner));
    }
    Page { count: None, trailing: Vec::new(), body, title: None, back: None }
}

/// `NewWatchPanel`: the card that asks Neko, in words, to set a watch up.
fn new_watch(s: &App, t: &Neko, layer: &mut Vec<El>, data: &SetupData) -> El {
    let scope = s.setup.scope.as_str();
    let chosen = s.pages.text("watch:target");
    let ws_id = if !scope.is_empty() { scope.to_owned() } else if data.workspaces.iter().any(|w| w.id == chosen) { chosen.to_owned() } else { data.workspaces[0].id.clone() };
    let tools: Vec<&neko_protocol::mcp_host::McpConnection> = data.connections.iter().filter(|c| c.enabled && (c.workspace_id.is_empty() || c.workspace_id == ws_id)).collect();
    let small = webkit::font(t, 12.5, 400.0, 20.3125);

    let mut head = row().w_full().gap(12.0).center_y().justify_between().child(row().px(4.0).child(text("What should Neko keep an eye on?".to_owned()).key("watch-ask-label").font(small).color(t.text.muted).a11y_hidden()));
    if !tools.is_empty() {
        let trigger = webkit::ghost_xs_button(t, "watch-ideas", "Use an idea", Some(&icons_phosphor::LIGHTBULB), "Use an idea", |s: &mut App| s.toggle_row(IDEAS_OPEN));
        let mut menu = rune_kit::Menu::new("watch-ideas-menu").trigger(trigger).open(s.row_open(IDEAS_OPEN)).placement(rune_kit::Placement::BottomEnd).min_width(240.0).on_close(|s: &mut App| {
            if s.row_open(IDEAS_OPEN) {
                s.toggle_row(IDEAS_OPEN);
            }
        });
        for c in &tools {
            let idea = words::watch_idea(&c.label);
            let said = idea.clone();
            menu = menu.item(rune_kit::MenuItem::new(idea).on_select(move |s: &mut App| {
                s.toggle_row(IDEAS_OPEN);
                s.pages.set_field("watch:ask", &said);
            }));
        }
        head = head.child(menu.build(&t.kit, layer));
    }

    let left = if scope.is_empty() && data.workspaces.len() > 1 {
        let names: Vec<String> = data.workspaces.iter().map(|w| w.name.clone()).collect();
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        webkit::pick(s, t, layer, "watch-target", "Workspace for the new watch", &names, &data.workspace_name(&ws_id), move |s: &mut App, picked: &str| {
            if let Some((_, id)) = by.iter().find(|(n, _)| n == picked) {
                s.pages.set_field("watch:target", id);
            }
        })
    } else {
        let sources = if tools.is_empty() { " · connect a source in Tools & skills to start".to_owned() } else { format!(" · {} {}", tools.len(), if tools.len() == 1 { "source" } else { "sources" }) };
        row().px(4.0).children([text(data.workspace_name(&ws_id)).font(small).color(t.text.muted).a11y_hidden(), text(sources).font(small).color(t.text.muted).a11y_hidden()])
    };
    let (suggest_ws, ask_ws) = (ws_id.clone(), ws_id.clone());
    let can_suggest = !tools.is_empty();
    let can_ask = !s.pages.text("watch:ask").trim().is_empty();
    let suggest = webkit::ghost_button(t, "watch-suggest", "Suggest from tools", Some(&icons_phosphor::SPARKLE), webkit::gated(can_suggest, move |s: &mut App| s.suggest_watches(&suggest_ws)));
    let suggest = if can_suggest { suggest } else { suggest.opacity(0.5) };
    let set_up = webkit::button(t, "watch-new", "Set up watch", None, true, false, webkit::gated(can_ask, move |s: &mut App| {
        let described = s.pages.text("watch:ask").to_owned();
        s.ask_watch(&described, &ask_ws)
    }));
    let set_up = if can_ask { set_up } else { set_up.opacity(0.5) };
    let foot = row().w_full().gaps(12.0, 12.0).flex_wrap().center_y().justify_between().child(left).child(row().gap(6.0).center_y().child(suggest).child(set_up));

    let mut inner = col().w_full().padding(12.0).gap(10.0).child(head).child(webkit::bare_text_area(s, t, "watch:ask", "What should Neko keep an eye on?", "New Linear issues assigned to me")).child(foot);
    if let Some(n) = note(s, t, "setup:page") {
        inner = inner.child(n);
    }
    let mut card = col().key("watch-card").w_full().radius(20.0).fill(t.text.base.alpha(0.03)).child(inner);
    if let Some((pending, said)) = s.suggestion_reply().filter(|_| !s.setup.dismissed_reply) {
        let mut line = row().w_full().gap(8.0).center_y().justify_between().child(text(if pending { "Looking through your tools…" } else { "Neko’s reply" }.to_owned()).key("watch-reply-title").font(webkit::font(t, 13.0, 500.0, 21.125)).color(t.text.base).a11y_hidden());
        if !pending {
            line = line.child(webkit::ghost_xs_button(t, "watch-reply-hide", "Hide reply", None, "Hide reply", |s: &mut App| s.setup.dismissed_reply = true));
        }
        let mut reply = col().key("watch-reply").w_full().px(16.0).pb(16.0).gap(6.0).child(line);
        if !pending {
            reply = reply.child(text(said).key("watch-reply-text").font(webkit::font(t, 13.0, 400.0, 21.125)).color(t.text.muted).wrap().min_w(0.0));
        }
        card = card.child(reply);
    }
    card
}

/// `WatchRow`: the status glyph, the title with its status and workspace, the row's actions, the switch; opened, the details.
fn watch_row(s: &App, t: &Neko, layer: &mut Vec<El>, data: &SetupData, w: &Responsibility) -> El {
    let id = w.id.clone();
    let open = s.row_open(&format!("watch:{id}"));
    let title = words::watch_title(w);
    let (status, tone) = words::watch_status(w, s.now_ms);
    let (toggle_id, sw_id) = (id.clone(), id.clone());
    let enabled = w.enabled;
    let (c, e, d, r) = (id.clone(), id.clone(), id.clone(), id.clone());
    let (menu_c, menu_e, menu_d, menu_r) = (id.clone(), id.clone(), id.clone(), id.clone());
    let button = row()
        .key(format!("watch-{id}"))
        .role("button")
        .label(format!("{} details for {title}", if open { "Hide" } else { "Show" }))
        .expanded(open)
        .focusable(true)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle_row(&format!("watch:{toggle_id}")))
        .child(text(title.clone()).font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.base).truncate().min_w(0.0).shrink().help(w.instruction.clone()).a11y_hidden())
        .child(el().grow(1.0).min_w(0.0))
        .child(text(status.clone()).key(format!("watch-status-{id}")).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(if tone == Tone::Warning { t.meaning.amber } else { t.text.muted }).a11y_hidden())
        .child(webkit::faint(t, &data.workspace_name(&w.workspace_id)));
    let more = webkit::more_menu(
        s,
        t,
        layer,
        &format!("watch-more-{id}"),
        &format!("Actions for {title}"),
        false,
        vec![
            Some(("Check now".to_owned(), &icons_phosphor::ARROWS_CLOCKWISE, false, enabled, std::rc::Rc::new(move |s: &mut App| s.setup_call(crate::link::SetupAction::Wake { id: menu_c.clone() })) as std::rc::Rc<dyn Fn(&mut App)>)),
            Some(("Rename…".to_owned(), &icons::PENCIL, false, true, std::rc::Rc::new(move |s: &mut App| s.open_rename_watch(&menu_r)))),
            Some(("Edit".to_owned(), &icons::PENCIL, false, true, std::rc::Rc::new(move |s: &mut App| s.open_watch_dialog(Some(&menu_e), "")))),
            None,
            Some(("Delete watch…".to_owned(), &icons::TRASH, true, true, std::rc::Rc::new(move |s: &mut App| s.confirm_delete_watch(&menu_d)))),
        ],
    );
    let check = webkit::row_action(t, &format!("watch-check-{id}"), &format!("Check {title} now"), &icons_phosphor::ARROWS_CLOCKWISE, webkit::gated(enabled, move |s: &mut App| s.setup_call(crate::link::SetupAction::Wake { id: c.clone() })));
    let check = if enabled { check } else { check.opacity(0.4) };
    let line = row()
        .w_full()
        .min_h(44.0)
        .px(12.0)
        .py(6.0)
        .gap(12.0)
        .center_y()
        .child(super::kit::glyph(t, words::watch_glyph(tone), &status))
        .child(button)
        .child(webkit::row_actions(vec![check, more]))
        .child(webkit::toggle(t, &format!("watch-switch-{id}"), &format!("Watch {title}"), enabled, move |s: &mut App| {
            let on = s.setup_data().and_then(|d| d.watch(&sw_id)).is_some_and(|w| w.enabled);
            s.watch_switch(&sw_id, !on)
        }));
    let _ = (d, e, r);
    // The right-click menu: every action of the row.
    let mut entries: Vec<MenuEntry> = Vec::new();
    if enabled {
        let i = id.clone();
        entries.push(("Check now".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(crate::link::SetupAction::Wake { id: i.clone() }))));
    }
    let i = id.clone();
    entries.push(("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_watch_dialog(Some(&i), ""))));
    let i = id.clone();
    entries.push(("Rename…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_rename_watch(&i))));
    let i = id.clone();
    entries.push((if enabled { "Pause" } else { "Turn on…" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.watch_switch(&i, !enabled))));
    let i = id.clone();
    entries.push(("Delete watch…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_delete_watch(&i))));
    let mut block = col().key(format!("watch-block-{id}")).w_full().radius(10.0).hover_group().hover(t.text.base.alpha(0.03)).child(line);
    if open {
        block = block.fill(t.text.base.alpha(0.02)).child(details(s, t, data, w));
    }
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        block = block.child(col().w_full().px(12.0).pb(8.0).child(n));
    }
    with_menu(s, t, layer, &format!("watch:{id}"), block, entries)
}

/// The open watch (`text-[13px]`): its instruction, the facts, what is wrong, the last result and the two buttons.
fn details(s: &App, t: &Neko, data: &SetupData, w: &Responsibility) -> El {
    let id = w.id.as_str();
    let body = webkit::font(t, 13.0, 400.0, 21.125);
    let sources: Vec<String> = w.connection_ids.iter().map(|c| words::connection_label(data, c)).collect();
    let missing = words::ungranted(data, &w.workspace_id, &w.connection_ids);
    let fact = |key: &str, label: &str, value: &str| row().key(key.to_owned()).w_full().gap(16.0).child(text(label.to_owned()).font(body).color(t.text.muted).w(128.0).a11y_hidden()).child(text(value.to_owned()).key(format!("{key}-value")).font(body).color(t.text.base.alpha(0.85)).wrap().grow(1.0).shrink().min_w(0.0).a11y_hidden());
    let mut c = col().key(format!("watch-details-{id}")).w_full().px(12.0).pb(16.0).pl(40.0).gap(16.0);
    c = c.child(text(w.instruction.clone()).key(format!("watch-instruction-{id}")).font(body).color(t.text.base.alpha(0.85)).wrap().min_w(0.0).max_w(70.0 * 7.6));
    let delivery = if !data.plan_first {
        "Tickets start working right away · reviewed results are delivered"
    } else if w.prepare_low_risk {
        "Low-risk fixes allowed · reviewed results are delivered"
    } else {
        "Plans require approval before local work"
    };
    c = c.child(
        col()
            .w_full()
            .gap(8.0)
            .child(fact(&format!("watch-d-status-{id}"), "Status", if w.enabled { "Watching" } else { "Paused" }))
            .child(fact(&format!("watch-d-last-{id}"), "Last checked", &w.last_attempt_ms.map_or("Not checked yet".to_owned(), |at| words::relative_time(Some(at), s.now_ms))))
            .child(fact(&format!("watch-d-sources-{id}"), "Sources", &if sources.is_empty() { "No sources selected".to_owned() } else { sources.join(", ") }))
            .child(fact(&format!("watch-d-delivery-{id}"), "Delivery", delivery)),
    );
    if w.failures > 0 {
        c = c.child(text(format!("{} consecutive failed {}", w.failures, if w.failures == 1 { "check" } else { "checks" })).key(format!("watch-d-failures-{id}")).font(body).color(t.meaning.amber).wrap().min_w(0.0));
    }
    if !missing.is_empty() {
        let ws = w.workspace_id.clone();
        c = c.child(row().w_full().gap(8.0).center_y().flex_wrap().child(text(format!("Connect or discover tools for {} to turn this on.", missing.join(", "))).key(format!("watch-d-missing-{id}")).font(body).color(t.meaning.amber).wrap().min_w(0.0).shrink()).child(webkit::ghost_xs_button(t, &format!("watch-review-{id}"), "Review connections", None, "Review connections", move |s: &mut App| {
            s.setup.grant_workspace = Some(ws.clone());
            s.open_setup(SetupSection::Tools)
        })));
    }
    if !w.last_result.trim().is_empty() {
        c = c.child(col().w_full().gap(6.0).child(text("Last check result".to_owned()).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).a11y_hidden()).child(text(w.last_result.clone()).key(format!("watch-d-result-{id}")).font(body).color(t.text.base.alpha(0.85)).wrap().min_w(0.0).padding(12.0).radius(10.0).fill(t.text.base.alpha(0.03))));
    }
    let (a, b) = (id.to_owned(), id.to_owned());
    let enabled = w.enabled;
    let check = webkit::ghost_button(t, &format!("watch-d-check-{id}"), "Check now", Some(&icons_phosphor::ARROWS_CLOCKWISE), webkit::gated(enabled, move |s: &mut App| s.setup_call(crate::link::SetupAction::Wake { id: a.clone() })));
    let check = if enabled { check } else { check.opacity(0.5) };
    c.child(row().w_full().gap(6.0).child(check).child(webkit::ghost_button(t, &format!("watch-d-edit-{id}"), "Edit watch", None, move |s: &mut App| s.open_watch_dialog(Some(&b), ""))))
}

/// `SuggestionCard`: a watch Neko proposed; nothing runs until you turn it on.
fn suggestion(s: &App, t: &Neko, layer: &mut Vec<El>, data: &SetupData, w: &Responsibility) -> El {
    let id = w.id.clone();
    let missing = words::ungranted(data, &w.workspace_id, &w.connection_ids);
    let sources: Vec<String> = w.connection_ids.iter().map(|c| words::connection_label(data, c)).collect();
    let live = w.enabled;
    let (words_, ink) = if live {
        ("Watching".to_owned(), t.text.muted)
    } else if !missing.is_empty() {
        (format!("Needs access to {}", missing.join(", ")), t.meaning.amber)
    } else if sources.is_empty() {
        ("Ready to review".to_owned(), t.text.muted)
    } else {
        (sources.join(", "), t.text.muted)
    };
    let label = if missing.is_empty() { "Turn on…" } else { "Review connections" };
    let (on, on2, e, d) = (id.clone(), id.clone(), id.clone(), id.clone());
    let lead = if live {
        super::kit::glyph(t, crate::status::StatusKind::InProgress, "Watching")
    } else {
        row().key(format!("suggestion-glyph-{id}")).role("button").label(format!("{label} {}", w.instruction)).focusable(true).size(14.0, 14.0).center().radius(7.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.watch_turn_on(&on2)).child(super::kit::glyph(t, crate::status::StatusKind::Backlog, "Suggested"))
    };
    let mut acts: Vec<El> = Vec::new();
    if !live {
        acts.push(webkit::row_action(t, &format!("suggestion-on-{id}"), label, &icons_phosphor::EYE_REGULAR, move |s: &mut App| s.watch_turn_on(&on)));
    }
    acts.push(webkit::row_action(t, &format!("suggestion-edit-{id}"), &format!("Edit {}", w.instruction), &icons::PENCIL, move |s: &mut App| s.open_watch_dialog(Some(&e), "")));
    if !live {
        acts.push(webkit::row_action(t, &format!("suggestion-dismiss-{id}"), &format!("Dismiss {}", w.instruction), &icons::TRASH, move |s: &mut App| s.confirm_delete_watch(&d)));
    }
    let line = vec![
        lead,
        text(w.instruction.clone()).key(format!("suggestion-{id}")).font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.base).truncate().grow(1.0).shrink().min_w(0.0).help(w.instruction.clone()).a11y_hidden(),
        text(words_).key(format!("suggestion-sources-{id}")).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(ink).truncate().max_w(0.4 * (webkit::page_width() - 24.0)).a11y_hidden(),
        webkit::row_actions(acts),
    ];
    let mut below: Vec<El> = Vec::new();
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        below.push(n);
    }
    let (m1, m2, m3) = (id.clone(), id.clone(), id.clone());
    let mut entries: Vec<MenuEntry> = Vec::new();
    if !live {
        entries.push((label.to_owned(), std::rc::Rc::new(move |s: &mut App| s.watch_turn_on(&m1))));
    }
    entries.push(("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_watch_dialog(Some(&m2), ""))));
    if !live {
        entries.push(("Dismiss".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_delete_watch(&m3))));
    }
    with_menu(s, t, layer, &format!("suggestion:{id}"), webkit::item_row(t, &format!("suggestion-block-{id}"), line, below), entries)
}
