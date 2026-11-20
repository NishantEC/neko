//! Settings, "AI & agents": a port of `web/src/pages/setup/settings/Ai.tsx`, group by group and row by row (the runtime and its
//! connection, what Home chat may do, its model pins, the budget and limits, the Inbox clean-up, suggestions and how work starts),
//! with the web's words, order and controls. Budget is a group of this page in the web, so its rows are here too.
//! Rows the web does not have (Cancel on a picked runtime) sit last in the group they belong to.

use neko_protocol::agent_models::{CatalogModel, ModelAccess, ModelSource, RuntimeOption, SourceStatus};
use neko_protocol::workbench::RuntimePreferences;
use rune::{El, col, icon, row, text};

use super::settings::{Item, cap, item, limit_of, limit_options, usable, with_note};
use super::ui::switch;
use super::webkit::{self, Tone, button, danger_button, gated, ghost_button, panel_footer, setting_row, setting_row_stacked, skeleton};
use crate::app::App;
use crate::icons;
use crate::link::Call;
use crate::pages::Section;
use crate::theme::Neko;

const SEP: char = '\u{1f}';

pub(super) fn all(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    runtime(s, t, layer, out);
    home_chat(s, t, out);
    pins(s, t, layer, out);
    budget(s, t, layer, out);
    inbox(s, t, layer, out);
    suggestions(s, t, out);
    work_style(s, t, out);
}

fn model_title(m: &CatalogModel) -> String {
    format!("{}{}{}{}", if m.label.is_empty() { &m.id } else { &m.label }, if m.recommended { " · Recommended" } else { "" }, if m.access == ModelAccess::Checked { " · Checked" } else { "" }, if usable(m) { "" } else { " · Unavailable" })
}

/// A model's effort levels as `(id, label)`: what it advertises (`effortOptions`).
fn efforts_of(model: Option<&CatalogModel>) -> Vec<(String, String)> {
    let Some(m) = model else { return Vec::new() };
    match &m.effort_options {
        Some(list) => list.iter().map(|o| (o.id.clone(), if o.label.is_empty() { cap(&o.id) } else { o.label.clone() })).collect(),
        None => m.reasoning_efforts.iter().map(|e| (e.clone(), e.clone())).collect(),
    }
}

fn speeds_of(model: Option<&CatalogModel>) -> Vec<(String, String)> {
    model.and_then(|m| m.speed_options.as_ref()).map(|list: &Vec<RuntimeOption>| list.iter().map(|o| (o.id.clone(), if o.label.is_empty() { cap(&o.id) } else { o.label.clone() })).collect()).unwrap_or_default()
}

// ---- Agent runtime ------------------------------------------------------------------------------

fn runtime(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    const G: &str = "Agent runtime";
    let runtime = s.runtime_shown();
    let provider_now = if runtime.provider.is_empty() { "codex".to_owned() } else { runtime.provider.clone() };
    let catalog = s.pages.catalog.as_ref();
    let sources: Vec<ModelSource> = catalog.map(|c| c.sources.clone()).unwrap_or_default();
    let source = sources.iter().find(|x| x.provider == provider_now);
    let selected = source.and_then(|x| x.models.iter().find(|m| m.id == runtime.model));
    let ready = source.is_none_or(|x| x.status == SourceStatus::Ready);
    let blocked = !ready || selected.is_some_and(|m| !usable(m));
    let dirty = s.runtime_dirty();

    // Runs with.
    let help = "The runtime that does the work for new conversations, tasks and background checks. Work already running isn’t interrupted.";
    let provider_labels: Vec<String> = if sources.is_empty() { vec!["Codex".to_owned()] } else { sources.iter().map(|x| x.label.clone()).collect() };
    let provider_label = source.map_or_else(|| if sources.is_empty() { "Codex".to_owned() } else { cap(&provider_now) }, |x| x.label.clone());
    let by: Vec<(String, String)> = sources.iter().map(|x| (x.provider.clone(), x.label.clone())).collect();
    let control = if catalog.is_none() {
        skeleton(t, "set-runtime-loading", 176.0, 32.0)
    } else {
        webkit::select(s, t, layer, "set-runtime-pick", "Agent runtime", &provider_labels, &provider_label, 176.0, move |s: &mut App, chosen: &str| {
            let provider = by.iter().find(|(_, l)| l == chosen).map_or_else(|| "codex".to_owned(), |(p, _)| p.clone());
            s.pick_runtime(|r| {
                r.provider = provider;
                r.model.clear();
                r.reasoning_effort = None;
                r.service_tier = None;
            });
            s.extra.model_check = None;
        })
    };
    out.push(item(Section::Ai, G, "runtime", "Runs with", help, setting_row(t, "set-runtime", "Runs with", Some(help), control)));

    // Connection.
    if let Some(src) = source {
        let words = if src.connection.is_empty() { if ready { "Ready" } else { "Not ready" } } else { src.connection.as_str() };
        let mut right = row().gap(8.0).center_y().child(webkit::status(t, "set-connection-status", if ready { Tone::Success } else { Tone::Warning }, words));
        if let Some(note) = src.note.as_ref().filter(|n| !n.is_empty()) {
            right = right.child(text(note.clone()).key("set-connection-note").font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted).wrap().min_w(0.0).max_w(339.0));
        }
        let line = row().key("set-connection").w_full().px(12.0).py(14.0).gaps(12.0, 12.0).flex_wrap().center_y().justify_between().child(text("Connection".to_owned()).font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted)).child(right);
        out.push(item(Section::Ai, G, "runtime-connection", "Connection", "", line));
    }

    // Model.
    let help = selected.and_then(|m| m.reason.clone().filter(|r| !r.is_empty()).or_else(|| m.description.clone().filter(|d| !d.is_empty()))).unwrap_or_else(|| if selected.is_some() { String::new() } else { "Leave on Default to let the runtime choose.".to_owned() });
    let mut model_labels = vec![source.and_then(|x| x.default_model.clone()).map_or_else(|| "Default model".to_owned(), |d| format!("Default · {d}"))];
    let mut model_ids = vec![String::new()];
    if let Some(src) = source {
        for m in &src.models {
            model_labels.push(model_title(m));
            model_ids.push(m.id.clone());
        }
    }
    if !runtime.model.is_empty() && !model_ids.contains(&runtime.model) {
        model_labels.push(runtime.model.clone());
        model_ids.push(runtime.model.clone());
    }
    let model_now = model_ids.iter().position(|m| *m == runtime.model).map_or_else(|| model_labels[0].clone(), |i| model_labels[i].clone());
    let (labels, ids) = (model_labels.clone(), model_ids.clone());
    let control = webkit::select(s, t, layer, "set-model-pick", "Model", &model_labels, &model_now, 224.0, move |s: &mut App, chosen: &str| {
        if let Some(i) = labels.iter().position(|l| l == chosen) {
            let id = ids[i].clone();
            s.pick_runtime(|r| r.model = id);
            s.extra.model_check = None;
        }
    });
    out.push(item(Section::Ai, G, "model", "Model", &help, setting_row(t, "set-model", "Model", if help.is_empty() { None } else { Some(&help) }, control)));

    // Reasoning effort and speed: only what the model advertises.
    let efforts = efforts_of(selected);
    let help = if efforts.is_empty() { "This model doesn’t advertise effort levels." } else { "Only the levels this model advertises." };
    let mut effort_labels = vec!["Auto".to_owned()];
    effort_labels.extend(efforts.iter().map(|(_, l)| l.clone()));
    let effort_now = match runtime.reasoning_effort.as_ref().filter(|e| !e.is_empty()) {
        None => "Auto".to_owned(),
        Some(e) => efforts.iter().find(|(id, _)| id == e).map_or_else(|| format!("{e} (unavailable)"), |(_, l)| l.clone()),
    };
    if effort_now.ends_with("(unavailable)") {
        effort_labels.push(effort_now.clone());
    }
    let e2 = efforts.clone();
    let control = webkit::select(s, t, layer, "set-effort-pick", "Reasoning effort", &effort_labels, &effort_now, 176.0, move |s: &mut App, chosen: &str| {
        let id = e2.iter().find(|(_, l)| l == chosen).map(|(id, _)| id.clone());
        s.pick_runtime(|r| r.reasoning_effort = id);
    });
    out.push(item(Section::Ai, G, "effort", "Reasoning effort", help, setting_row(t, "set-effort", "Reasoning effort", Some(help), control)));
    let speeds = speeds_of(selected);
    let help = if speeds.is_empty() { "This model doesn’t advertise a faster tier." } else { "Extra speed may use more allowance." };
    let mut speed_labels = vec!["Auto".to_owned(), "Normal".to_owned()];
    speed_labels.extend(speeds.iter().map(|(_, l)| l.clone()));
    let speed_now = match runtime.service_tier.as_deref().filter(|e| !e.is_empty()) {
        None => "Auto".to_owned(),
        Some("default") => "Normal".to_owned(),
        Some(id) => speeds.iter().find(|(i, _)| i == id).map_or_else(|| id.to_owned(), |(_, l)| l.clone()),
    };
    let s2 = speeds.clone();
    let control = webkit::select(s, t, layer, "set-speed-pick", "Speed", &speed_labels, &speed_now, 176.0, move |s: &mut App, chosen: &str| {
        let tier = match chosen {
            "Auto" => None,
            "Normal" => Some("default".to_owned()),
            other => s2.iter().find(|(_, l)| l == other).map(|(id, _)| id.clone()),
        };
        s.pick_runtime(|r| r.service_tier = tier);
    });
    out.push(item(Section::Ai, G, "speed", "Speed", help, setting_row(t, "set-speed", "Speed", Some(help), control)));

    // Exact model ID: a collapsed row; the button opens a field and "Use ID".
    let help = "For a model that isn’t in the list.";
    let open = s.extra.custom_model_open;
    let toggle = button(t, "set-custom-toggle", "Use an exact model ID", None, false, false, |s: &mut App| s.extra.custom_model_open = !s.extra.custom_model_open);
    let mut block = col().w_full().child(setting_row_stacked(t, "set-custom-model", "Exact model ID", Some(help), row().gap(8.0).child(toggle)));
    if open {
        let typed = s.pages.text("custom-model").trim().to_owned();
        let valid = crate::extra::valid_model_id(&typed);
        let use_id = button(t, "set-custom-use", "Use ID", None, false, false, gated(valid, |s: &mut App| s.use_exact_model())).h(40.0);
        let use_id = if valid { use_id } else { use_id.disabled(true).opacity(0.5) };
        block = block.child(row().w_full().px(12.0).pb(16.0).gap(8.0).center_y().child(webkit::input(s, t, "custom-model", "Exact model ID", "Or enter an exact model ID", 120.0, true, |s: &mut App| s.use_exact_model()).grow(1.0)).child(use_id));
    }
    out.push(item(Section::Ai, G, "custom-model", "Exact model ID", help, block));

    // What the last check said.
    if let Some(check) = &s.extra.model_check {
        let (glyph, ink) = if check.ok { (&icons::CHECK_LINE, t.meaning.mint) } else if check.unavailable { (&icons::INFO, t.meaning.amber) } else { (&icons::CROSS, t.meaning.coral) };
        let line = row().key("set-check-result").w_full().px(12.0).py(14.0).gap(8.0).items_start().live_region(rune::LiveRegion::Polite).child(icon(glyph, 16.0).color(ink).mt(2.0).a11y_hidden()).child(text(check.message.clone()).key("set-check-result-words").font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.base).wrap().grow(1.0).min_w(0.0));
        out.push(item(Section::Ai, G, "check-result", "Check result", &check.message, with_note(s, t, "setting:model-check", line)));
    } else if let Some(n) = super::ui::note(s, t, "setting:model-check") {
        out.push(item(Section::Ai, G, "check-result", "Check result", "", n));
    }

    // The footer: what was read, and the three actions.
    let read = catalog.map_or(0, |c| c.read_at_ms);
    let notes = [if read > 0 { format!("Catalog read {}.", webkit::relative_time(s.now_ms, read)) } else { "Reading models from your runtimes…".to_owned() }, "Checking sends one short reply, so it may use quota; a failed check keeps your current model.".to_owned()];
    let note = notes.join(" ");
    let mut buttons = Vec::new();
    if dirty {
        buttons.push(ghost_button(t, "runtime-cancel", "Cancel", None, |s: &mut App| s.pages.runtime_draft = None));
    }
    buttons.push(ghost_button(t, "set-models-refresh", "Refresh models", Some(&icons::REFRESH), |s: &mut App| s.call(Call::ModelsRefresh)));
    let can_use = dirty && !blocked;
    let use_now = button(t, "runtime-use", "Use without checking", None, false, false, gated(can_use, |s: &mut App| s.use_runtime()));
    buttons.push(if can_use { use_now } else { use_now.disabled(true).opacity(0.5) });
    let can_check = !blocked && !s.extra.checking;
    let check = button(t, "set-check-model", if s.extra.checking { "Checking…" } else { "Check & use model" }, None, true, false, gated(can_check, |s: &mut App| s.check_model()));
    buttons.push(if can_check { check } else { check.disabled(true).opacity(0.5) });
    let footer = panel_footer(t, "set-runtime-footer", &notes, buttons);
    out.push(item(Section::Ai, G, "runtime-footer", "Refresh models Check & use model", &note, with_note(s, t, "setting:runtime", footer)));
}

// ---- What Home chat may do ----------------------------------------------------------------------

fn home_chat(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Home chat works like your own Claude Code session in bypass mode: it edits files, runs commands, uses your tools and browser, and manages Neko itself, without asking per step. Off makes it read-only, with an approval for each action.";
    let control = match s.extra.super_agent {
        Some(on) => switch(t, "set-super-agent-switch", "Full power for Home chat", on, move |s: &mut App| s.set_super_agent(!on)),
        None => skeleton(t, "set-super-agent-loading", 36.0, 20.0),
    };
    out.push(item(Section::Ai, "What Home chat may do", "super-agent", "Full power", help, with_note(s, t, "setting:super-agent", setting_row(t, "set-super-agent", "Full power", Some(help), control))));
}

// ---- Model pins ---------------------------------------------------------------------------------

fn pins(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    const G: &str = "Model pins";
    let shown = s.pins_shown();
    let (_, saved) = s.home_pins();
    let sources: Vec<&ModelSource> = s.pages.catalog.as_ref().map(|c| c.sources.iter().filter(|x| x.status == SourceStatus::Ready).collect()).unwrap_or_default();
    let all: Vec<(&ModelSource, &CatalogModel)> = sources.iter().flat_map(|src| src.models.iter().filter(|m| usable(m)).map(move |m| (*src, m))).collect();
    let pinned = match (&shown.provider, &shown.model) {
        (Some(p), Some(m)) => all.iter().find(|(src, model)| &src.provider == p && &model.id == m).map(|(_, m)| *m),
        _ => None,
    };
    let union = |f: fn(Option<&CatalogModel>) -> Vec<(String, String)>| -> Vec<(String, String)> {
        let mut seen: Vec<(String, String)> = Vec::new();
        for (_, m) in &all {
            for o in f(Some(m)) {
                if !seen.iter().any(|(id, _)| *id == o.0) {
                    seen.push(o);
                }
            }
        }
        seen
    };
    let efforts = if pinned.is_some() { efforts_of(pinned) } else { union(efforts_of) };
    let speeds = if pinned.is_some() { speeds_of(pinned) } else { union(speeds_of) };

    // Model.
    let help = "Neko decides picks a compatible model for each message, keeping any pins you set. Pinning a model pins its provider too.";
    let mut labels = vec!["Neko decides".to_owned()];
    let mut ids: Vec<Option<(String, String)>> = vec![None];
    for (src, m) in &all {
        labels.push(format!("{} · {}", src.label, if m.label.is_empty() { &m.id } else { &m.label }));
        ids.push(Some((src.provider.clone(), m.id.clone())));
    }
    let now = ids.iter().position(|i| i.as_ref().is_some_and(|(p, m)| Some(p) == shown.provider.as_ref() && Some(m) == shown.model.as_ref())).map_or_else(|| "Neko decides".to_owned(), |i| labels[i].clone());
    let (l2, i2) = (labels.clone(), ids.clone());
    let honoured: Vec<(String, Vec<String>, Vec<String>)> = all.iter().map(|(src, m)| (format!("{}{SEP}{}", src.provider, m.id), efforts_of(Some(m)).into_iter().map(|e| e.0).collect(), speeds_of(Some(m)).into_iter().map(|e| e.0).collect())).collect();
    let control = webkit::select(s, t, layer, "set-pin-model-pick", "Home chat model", &labels, &now, 240.0, move |s: &mut App, chosen: &str| {
        let pin = l2.iter().position(|l| l == chosen).and_then(|i| i2[i].clone());
        let key = pin.as_ref().map(|(p, m)| format!("{p}{SEP}{m}"));
        let can = key.as_ref().and_then(|k| honoured.iter().find(|(id, _, _)| id == k));
        s.pick_pins(|p| {
            p.provider = pin.as_ref().map(|(p, _)| p.clone());
            p.model = pin.as_ref().map(|(_, m)| m.clone());
            // A model change drops only the pins it can’t honour.
            if let Some((_, e, sp)) = can {
                if p.reasoning_effort.as_ref().is_some_and(|x| !e.contains(x)) {
                    p.reasoning_effort = None;
                }
                if p.service_tier.as_ref().is_some_and(|x| x != "default" && !sp.contains(x)) {
                    p.service_tier = None;
                }
            }
        });
    });
    out.push(item(Section::Ai, G, "pins", "Model", help, setting_row(t, "set-pin-model", "Model", Some(help), control)));

    // Effort.
    let mut effort_labels = vec!["Auto".to_owned()];
    effort_labels.extend(efforts.iter().map(|(_, l)| l.clone()));
    let effort_now = shown.reasoning_effort.as_ref().map_or_else(|| "Auto".to_owned(), |e| efforts.iter().find(|(id, _)| id == e).map_or_else(|| e.clone(), |(_, l)| l.clone()));
    if !effort_labels.contains(&effort_now) {
        effort_labels.push(effort_now.clone());
    }
    let e2 = efforts.clone();
    let control = webkit::select(s, t, layer, "set-pin-effort-pick", "Home chat reasoning effort", &effort_labels, &effort_now, 176.0, move |s: &mut App, chosen: &str| {
        let id = e2.iter().find(|(_, l)| l == chosen).map(|(id, _)| id.clone());
        s.pick_pins(|p| p.reasoning_effort = id);
    });
    let help = if efforts.is_empty() { Some("No connected model advertises effort levels.") } else { None };
    out.push(item(Section::Ai, G, "pin-effort", "Effort", help.unwrap_or(""), setting_row(t, "set-pin-effort", "Effort", help, control)));

    // Speed.
    let mut speed_labels = vec!["Auto".to_owned(), "Normal".to_owned()];
    speed_labels.extend(speeds.iter().map(|(_, l)| l.clone()));
    let speed_now = match shown.service_tier.as_deref() {
        None => "Auto".to_owned(),
        Some("default") => "Normal".to_owned(),
        Some(id) => speeds.iter().find(|(i, _)| i == id).map_or_else(|| id.to_owned(), |(_, l)| l.clone()),
    };
    let s2 = speeds.clone();
    let control = webkit::select(s, t, layer, "set-pin-speed-pick", "Home chat speed", &speed_labels, &speed_now, 176.0, move |s: &mut App, chosen: &str| {
        let tier = match chosen {
            "Auto" => None,
            "Normal" => Some("default".to_owned()),
            other => s2.iter().find(|(_, l)| l == other).map(|(id, _)| id.clone()),
        };
        s.pick_pins(|p| p.service_tier = tier);
    });
    let help = "Extra speed may use more allowance.";
    out.push(item(Section::Ai, G, "pin-speed", "Speed", help, setting_row(t, "set-pin-speed", "Speed", Some(help), control)));

    // Paid speed.
    let paid = shown.allow_paid_speed;
    let help = "Off by default. When speed is Auto, lets Neko choose a tier that may use more allowance.";
    out.push(item(Section::Ai, G, "pin-paid", "Allow automatic paid speed", help, setting_row(t, "set-pin-paid", "Allow automatic paid speed", Some(help), switch(t, "set-pin-paid-switch", "Allow automatic paid speed", paid, move |s: &mut App| s.pick_pins(|p| p.allow_paid_speed = !paid)))));

    // Last run: one line of separate words that flow and wrap, as the web's inline text does.
    if let Some(last) = s.summary.as_ref().and_then(|m| m.settings.home_last.clone()) {
        let plain = webkit::font(t, 13.0, 400.0, 19.5);
        let word = |w: String| text(w).font(plain).color(t.text.muted).wrap().min_w(0.0);
        let mut line = row().key("set-pin-last").w_full().px(12.0).py(14.0).gaps(0.0, 4.0).flex_wrap().child(text("Last run".to_owned()).font(webkit::font(t, 12.5, 500.0, 18.75)).color(t.text.muted)).child(word(if last.model.is_empty() { "default model".to_owned() } else { last.model.clone() }));
        if let Some(e) = last.effort.as_ref().filter(|e| !e.is_empty()) {
            line = line.child(word(format!("· {e} effort")));
        }
        line = line.child(word(if last.automatic { "· chosen by Neko" } else { "· pinned" }.to_owned())).child(word("—".to_owned())).child(word(last.reason.clone()));
        let words = format!("Last run {} {}", last.model, last.reason);
        out.push(item(Section::Ai, G, "pin-last", "Last run", &words, line));
    }

    // Footer.
    let automatic = shown.provider.is_none() && shown.reasoning_effort.is_none() && shown.service_tier.is_none();
    let notes = [(if automatic && !paid { "Neko decides everything." } else { "Some choices are pinned." }).to_owned(), "Applies to your next message.".to_owned()];
    let note = notes.join(" ");
    let can_reset = !(automatic && !paid && saved.model.is_none());
    let reset = ghost_button(t, "set-pin-reset", "Reset to Neko decides", None, gated(can_reset, |s: &mut App| s.apply_pins(Some(RuntimePreferences::default()))));
    let reset = if can_reset { reset } else { reset.disabled(true).opacity(0.5) };
    let dirty = s.pins_dirty();
    let apply = button(t, "set-pin-apply", "Apply", None, true, false, gated(dirty, |s: &mut App| s.apply_pins(None)));
    let apply = if dirty { apply } else { apply.disabled(true).opacity(0.5) };
    out.push(item(Section::Ai, G, "pin-footer", "Reset to Neko decides Apply", &note, panel_footer(t, "set-pin-footer", &notes, vec![reset, apply])));
}

// ---- Budget & limits ----------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn money_row(s: &App, t: &Neko, id: &'static str, label: &str, help: &str, placeholder: &str, button_label: &str, max: f64, make: fn(Option<u32>) -> Call) -> Item {
    let dirty = s.pages.edited(id, true);
    let save = button(t, &format!("{id}-save"), button_label, None, false, false, gated(dirty, move |s: &mut App| s.save_money(id, max, make)));
    let save = if dirty { save } else { save.disabled(true).opacity(0.5) };
    let control = row().gap(8.0).center_y().shrink().child(text("US$".to_owned()).font(rune::TextStyle { size: 13.0, line_height: 19.5, ..t.ty.mono }).color(t.text.muted)).child(webkit::input(s, t, id, label, placeholder, 112.0, true, move |s: &mut App| s.save_money(id, max, make))).child(save);
    item(Section::Ai, "Budget & limits", id, label, help, with_note(s, t, &format!("setting:{id}"), setting_row(t, &format!("set-{id}"), label, Some(help), control)))
}

fn budget(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    const G: &str = "Budget & limits";
    let st = s.summary.as_ref().map(|m| m.settings.clone()).unwrap_or_default();
    out.push(money_row(
        s,
        t,
        "task-budget",
        "Stop a task after",
        "Counts the cost Claude Code and OpenCode report for each task and stops it when it passes this amount. Codex subscriptions don’t report a price, so they aren’t limited here. Leave empty for no limit.",
        "No limit",
        "Save budget",
        1000.0,
        |cents| Call::SetTaskBudget { cents },
    ));
    out.push(money_row(
        s,
        t,
        "daily-budget",
        "Pause new work for the day at",
        "When today’s reported cost (tickets and routine runs, local day) reaches this, new goal turns and routine runs wait until tomorrow and one Inbox item asks whether to raise it. Work already running finishes. Off when empty.",
        "Off",
        "Save ceiling",
        10000.0,
        |cents| Call::SetDailyBudget { cents },
    ));
    let running = limit_options(&[1, 2, 3, 5], "No limit", st.max_running);
    let running_now = st.max_running.map_or("No limit".to_owned(), |n| n.to_string());
    let help = "Other tasks wait their turn and start as running ones finish. Work already running is not stopped.";
    out.push(item(Section::Ai, G, "tasks-at-once", "Tasks at once", help, with_note(s, t, "setting:max-running", setting_row(t, "set-max-running", "Tasks at once", Some(help), webkit::select(s, t, layer, "set-max-running-pick", "Tasks at once", &running, &running_now, 176.0, |s: &mut App, chosen: &str| s.call(Call::SetMaxRunning { limit: limit_of(chosen) }))))));
    let writable = limit_options(&[1, 2, 3, 6, 8], "2 (default)", st.max_writable);
    let writable_now = st.max_writable.map_or("2 (default)".to_owned(), |n| n.to_string());
    let help = "Each writing task has its own copy of the repository with its own installs and builds. Extra tasks in the same repository wait; planning and questions are not limited.";
    out.push(item(Section::Ai, G, "writing-per-repository", "Writing per repository", help, with_note(s, t, "setting:max-writable", setting_row(t, "set-max-writable", "Writing per repository", Some(help), webkit::select(s, t, layer, "set-max-writable-pick", "Writing per repository", &writable, &writable_now, 176.0, |s: &mut App, chosen: &str| s.call(Call::SetMaxWritable { limit: limit_of(chosen) }))))));
}

// ---- Inbox clean-up, suggestions, how work starts -----------------------------------------------

fn inbox(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    let days = s.pages.routines.as_ref().map_or(7, |d| d.age_out_days);
    let mut day_labels = vec!["Never".to_owned()];
    let mut choices = vec![3u32, 7, 14, 30];
    if days > 0 && !choices.contains(&days) {
        choices.push(days);
        choices.sort_unstable();
    }
    day_labels.extend(choices.iter().map(|d| format!("{d} days")));
    let days_now = if days == 0 { "Never".to_owned() } else { format!("{days} days") };
    let help = "An item in your Inbox that nothing has touched for this long closes itself and moves to Closed recently, where you can reopen it. A routine that still needs you can report it again. Items also close when their pull request is merged or closed, or when the time they were tied to has passed.";
    let control = webkit::select(s, t, layer, "set-age-out-pick", "Inbox items age out after", &day_labels, &days_now, 176.0, |s: &mut App, chosen: &str| {
        let days = chosen.split(' ').next().and_then(|d| d.parse().ok()).unwrap_or(0);
        s.call(Call::SetInboxAgeOut { days });
    });
    out.push(item(Section::Ai, "Inbox clean-up", "inbox-age-out", "Inbox items age out after", help, with_note(s, t, "setting:age-out", setting_row(t, "set-age-out", "Inbox items age out after", Some(help), control))));
}

fn suggestions(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Neko looks for trouble that repeats and suggests a fix. Off means it gathers nothing.";
    let control = match s.extra.suggestions {
        Some(on) => switch(t, "set-suggestions-switch", "Suggestions", on, move |s: &mut App| s.set_suggestions(!on)),
        None => skeleton(t, "set-suggestions-loading", 36.0, 20.0),
    };
    out.push(item(Section::Ai, "Suggestions", "suggestions", "Suggestions", help, with_note(s, t, "setting:suggestions", setting_row(t, "set-suggestions", "Suggestions", Some(help), control))));
}

fn work_style(s: &App, t: &Neko, out: &mut Vec<Item>) {
    const G: &str = "How work starts";
    let st = s.summary.as_ref().map(|m| m.settings.clone()).unwrap_or_default();
    let plan = st.plan_first;
    let help = "Off: a ticket is a normal session that writes code right away, and the agent stops for approval only on critical work. On: every ticket first gets a read-only plan to approve.";
    out.push(item(Section::Ai, G, "plan-first", "Plan before working", help, with_note(s, t, "setting:plan-first", setting_row(t, "set-plan-first", "Plan before working", Some(help), switch(t, "set-plan-first-switch", "Plan before working", plan, move |s: &mut App| s.call(Call::SetPlanFirst { enabled: !plan }))))));
    let swa = st.start_without_approval;
    let help = "When plans are required, a planned chat or manual ticket starts building without waiting. Nothing is ever published.";
    out.push(item(Section::Ai, G, "start-without-approval", "Start without approval", help, with_note(s, t, "setting:start-without-approval", setting_row(t, "set-swa", "Start without approval", Some(help), switch(t, "set-swa-switch", "Start without approval", swa, move |s: &mut App| s.call(Call::SetStartWithoutApproval { enabled: !swa }))))));
    let help = "Removes every task except those with a pull request, and lets your watches file their issues again from scratch. Memory, settings, workspaces, tools and watches stay.";
    out.push(item(Section::Ai, G, "start-over", "Start over", help, with_note(s, t, "setting:start-over", setting_row(t, "set-start-over", "Start over", Some(help), danger_button(t, "set-start-over-button", "Start over…", |s: &mut App| s.ask_start_over())))));
}
