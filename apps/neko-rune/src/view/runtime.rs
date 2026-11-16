//! The model chip and its popover: the session's runtime, model, reasoning effort and speed, as the web's composer
//! picker (`chat/ChatInputRuntime.tsx`) offers them. The chip sits in the composer's bar and opens a 392 point popover
//! above it: a search over the models, the models by source, an Effort control, a Speed control, "Allow automatic
//! paid speed", then Reset, Discard and "Check & apply". Applying sends `SetConversationRuntime` on `task:<id>`.

use neko_protocol::agent_models::{ModelAccess, SourceStatus};
use rune::{Color, El, ScrollSpec, TextStyle, col, el, icon, row, text};
use rune_kit::{Placement, TextInput, popover, scrim};

use crate::app::App;
use crate::icons;
use crate::runtime_pick::{RuntimeUi, clean, compact_label, effort_label, options_for, speed_label, validation};
use crate::theme::Neko;

/// The popover's width (the web's `w-[392px]`).
pub const WIDTH: f32 = 392.0;

/// What the chip says: the pins in words, or the runtime the session last ran on when nothing is pinned.
pub fn chip_words(s: &App) -> String {
    let Some(detail) = s.detail() else { return String::new() };
    let pins = clean(&detail.prefs);
    if pins == Default::default() {
        return detail.runtime.clone();
    }
    compact_label(&pins, &s.runtime.catalog.clone().unwrap_or_default())
}

/// What the ask box's chip says: the pins of Neko's own conversation, or the runtime it last ran on.
pub fn ask_chip_words(s: &App) -> String {
    let Some(m) = s.summary.as_ref() else { return String::new() };
    let conversation = m.home_conversation(s.workspace_scope.as_deref());
    match m.pins.get(&conversation).map(|(p, _)| clean(p)).filter(|p| *p != Default::default()) {
        Some(pins) => compact_label(&pins, &s.runtime.catalog.clone().unwrap_or_default()),
        None => m.home_runtime.clone(),
    }
}

/// The chip in the ask box's bar: the same popover, on Neko's own conversation.
pub fn ask_chip(s: &App, t: &Neko) -> El {
    let open = s.runtime.open && s.runtime.anchor == "ask-model";
    row()
        .key("ask-model")
        .role("button")
        .label(format!("Conversation settings: {}", ask_chip_words(s)))
        .help("Model, effort and speed for this conversation. Changes apply to the next turn or worker.")
        .expanded(open)
        .focusable(true)
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(14.0)
        .fill(if open { t.surface.active } else { Color::TRANSPARENT })
        .border(1.0, t.surface.field_border)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.runtime_toggle_home())
        .child(text(ask_chip_words(s)).font(t.ty.meta).color(t.text.muted).truncate().max_w(180.0).min_w(0.0).a11y_hidden())
        .child(icon(&icons::CHEVRON_DOWN, 14.0).color(t.text.muted).a11y_hidden())
}

/// The chip in the composer's bar: the words, a chevron, a 1 px border; pressing it opens the popover.
pub fn chip(s: &App, t: &Neko) -> El {
    row()
        .key("runtime-chip")
        .role("button")
        .label(format!("Conversation settings: {}", chip_words(s)))
        .help("Model, effort and speed for this conversation. Changes apply to the next turn or worker.")
        .expanded(s.runtime.open && s.runtime.anchor == "runtime-chip")
        .focusable(true)
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(14.0)
        .fill(if s.runtime.open && s.runtime.anchor == "runtime-chip" { t.surface.active } else { Color::TRANSPARENT })
        .border(1.0, t.surface.field_border)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.runtime_toggle())
        .child(text(chip_words(s)).key("composer-model").font(t.ty.meta).color(t.text.muted).truncate().max_w(240.0).min_w(0.0).a11y_hidden())
        .child(icon(&icons::CHEVRON_DOWN, 14.0).color(t.text.muted).a11y_hidden())
}

fn small_button(t: &Neko, key: &str, label: &str, enabled: bool, primary: bool, f: impl Fn(&mut App) + 'static) -> El {
    let ink = if primary { Color::WHITE } else { t.text.base };
    let mut r = row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .disabled(!enabled)
        .focusable(enabled)
        .h(28.0)
        .px(10.0)
        .center()
        .radius(t.radius.control)
        .fill(if primary { if enabled { t.meaning.iris } else { t.meaning.iris.alpha(0.4) } } else { Color::TRANSPARENT })
        .hover(if primary { Color::WHITE.alpha(if enabled { 0.14 } else { 0.0 }) } else { t.surface.hover })
        .focus_ring(2.0, 2.0, t.surface.ring)
        .child(text(label.to_owned()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(if enabled { ink } else { ink.alpha(0.5) }).a11y_hidden());
    if enabled {
        r = r.on_press(f);
    }
    r
}

/// A row of choices that wrap, one on: the web's segmented radio group.
fn segmented(t: &Neko, key: &str, label: &str, value: &str, options: Vec<(String, String)>, pick: impl Fn(&mut App, &str) + Clone + 'static) -> El {
    let mut group = row().key(key.to_owned()).role("radiogroup").label(label.to_owned()).w_full().gap(4.0).padding(2.0).wrap().radius(8.0).fill(t.text.base.alpha(0.05));
    for (id, words) in options {
        let on = id == value;
        let f = pick.clone();
        let chosen = id.clone();
        group = group.child(
            row()
                .key(format!("{key}-{id}"))
                .role("radio")
                .label(words.clone())
                .checked(on)
                .focusable(true)
                .h(26.0)
                .px(10.0)
                .center()
                .radius(6.0)
                .fill(if on { t.surface.popover } else { Color::TRANSPARENT })
                .border(1.0, if on { t.surface.border } else { Color::TRANSPARENT })
                .hover(t.surface.hover)
                .focus_ring(2.0, 2.0, t.surface.ring)
                .on_press(move |s: &mut App| f(s, &chosen))
                .child(text(words).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(if on { t.text.base } else { t.text.muted }).a11y_hidden()),
        );
    }
    group
}

fn heading(t: &Neko, words: &str, value: &str) -> El {
    row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(words.to_owned()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).grow(1.0))
        .child(text(value.to_owned()).font(t.ty.meta).color(t.text.muted))
}

#[allow(clippy::too_many_arguments)]
fn model_row(t: &Neko, key: String, title: &str, detail: Option<String>, badge: Option<&str>, selected: bool, enabled: bool, f: impl Fn(&mut App) + 'static) -> El {
    let mut words = col().grow(1.0).min_w(0.0).child(
        row()
            .gap(8.0)
            .center_y()
            .child(text(title.to_owned()).font(TextStyle { weight: 500.0, ..t.ty.row }).color(if enabled { t.text.base } else { t.text.muted }).truncate().min_w(0.0))
            .children(badge.map(|b| row().px(6.0).h(16.0).center().radius(8.0).border(1.0, t.surface.field_border).child(text(b.to_owned()).font(TextStyle { size: 10.5, ..t.ty.id }).color(t.text.muted)))),
    );
    if let Some(d) = detail {
        words = words.child(text(d).font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted).wrap());
    }
    let mut r = row()
        .key(key)
        .role("button")
        .label(title.to_owned())
        .checked(selected)
        .disabled(!enabled)
        .focusable(enabled)
        .w_full()
        .px(10.0)
        .py(6.0)
        .gap(8.0)
        .items_start()
        .radius(8.0)
        .fill(if selected { t.surface.hover } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .child(words);
    if selected {
        r = r.child(icon(&icons::CHECK_LINE, 14.0).color(t.text.base).a11y_hidden());
    }
    if enabled { r.on_press(f) } else { r }
}

fn models(s: &App, t: &Neko) -> El {
    let ui = &s.runtime;
    let q = ui.query.text.trim().to_lowercase();
    let mut list = col().key("runtime-models").w_full().gap(0.0);
    list = list.child(model_row(t, "runtime-model-auto".into(), "Neko decides", Some("Chooses a compatible model and keeps your effort and speed pins.".into()), None, ui.draft.model.is_none(), true, |s: &mut App| s.runtime.automatic_model()));
    let Some(catalog) = ui.catalog.as_ref() else {
        return list;
    };
    let mut any = false;
    for source in &catalog.sources {
        let ready = source.status == SourceStatus::Ready;
        let matching: Vec<_> = source.models.iter().filter(|m| q.is_empty() || [m.id.as_str(), m.label.as_str(), m.description.as_deref().unwrap_or(""), source.label.as_str()].iter().any(|f| f.to_lowercase().contains(&q))).collect();
        if matching.is_empty() && (ready || !q.is_empty()) {
            continue;
        }
        any |= !matching.is_empty();
        let connection = if source.connection.is_empty() { String::new() } else { format!(" · {}", source.connection) };
        list = list.child(text(format!("{}{connection}", source.label)).font(TextStyle { size: 11.0, weight: 500.0, ..t.ty.id }).color(t.text.faint).px(10.0).pt(8.0).pb(2.0));
        if !ready {
            list = list.child(text(source.note.clone().unwrap_or_else(|| "This source is not ready.".to_owned())).font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted).wrap().px(10.0).py(2.0));
        }
        for model in matching {
            let usable = model.access != ModelAccess::Unavailable;
            let provider = source.provider.clone();
            let chosen = model.clone();
            let detail = [model.reason.clone().or_else(|| model.description.clone()), (!usable).then(|| "Unavailable".to_owned())].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            let selected = ui.draft.provider.as_deref() == Some(source.provider.as_str()) && ui.draft.model.as_deref() == Some(model.id.as_str());
            list = list.child(model_row(
                t,
                format!("runtime-model-{}-{}", source.provider, model.id),
                &model.label,
                Some(detail).filter(|d| !d.is_empty()),
                model.recommended.then_some("Recommended"),
                selected,
                ready && usable,
                move |s: &mut App| s.runtime.choose(&provider, &chosen),
            ));
        }
    }
    if !any && !ui.loading {
        list = list.child(text(if q.is_empty() { "No models available. Refresh models or connect a provider in Settings." } else { "No models match this search." }).font(t.ty.meta).color(t.text.muted).wrap().px(10.0).py(12.0));
    }
    list
}

fn panel(s: &App, t: &Neko) -> El {
    let ui: &RuntimeUi = &s.runtime;
    let catalog = ui.catalog.clone().unwrap_or_default();
    let draft = &ui.draft;
    let effort = options_for(draft, &catalog, false);
    let speed = options_for(draft, &catalog, true);

    let refresh = row()
        .key("runtime-refresh")
        .role("button")
        .label("Refresh models")
        .help("Refresh models")
        .focusable(!ui.loading && !ui.saving)
        .size(28.0, 28.0)
        .center()
        .radius(t.radius.control)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.runtime_refresh())
        .child(icon(&icons::REFRESH, 16.0).color(t.text.muted).a11y_hidden());
    let head = row().w_full().gap(8.0).center_y().child(text("Model & effort").font(TextStyle { size: 15.0, weight: 600.0, ..t.ty.row }).color(t.text.base).grow(1.0)).child(refresh);

    let search = TextInput::new(&ui.query).placeholder("Search models").label("Search available models").on_edit(|s: &mut App| &mut s.runtime.query).build(&t.kit).key("runtime-search");
    let list = col().key("runtime-model-scroll").w_full().max_h(216.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(models(s, t));

    let mut effort_options = vec![("auto".to_owned(), "Auto".to_owned())];
    effort_options.extend(effort.iter().map(|o| (o.id.clone(), o.label.clone())));
    let effort_block = if !effort.is_empty() {
        col().w_full().gap(6.0).child(heading(t, "Effort", &effort_label(draft, &catalog))).child(segmented(t, "runtime-effort", "Reasoning effort", draft.reasoning_effort.as_deref().unwrap_or("auto"), effort_options, |s: &mut App, id: &str| {
            let mut next = s.runtime.draft.clone();
            next.reasoning_effort = (id != "auto").then(|| id.to_owned());
            s.runtime.set_draft(next);
        }))
    } else {
        col().w_full().gap(6.0).child(heading(t, "Effort", &effort_label(draft, &catalog))).child(text(if ui.loaded() { "This model does not advertise effort levels." } else { "Load models to see effort levels." }).font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted).wrap())
    };

    let mut speed_options = vec![("auto".to_owned(), "Auto".to_owned()), ("default".to_owned(), "Normal".to_owned())];
    speed_options.extend(speed.iter().map(|o| (o.id.clone(), o.label.clone())));
    let speed_block = if !speed.is_empty() {
        col().w_full().gap(6.0).child(heading(t, "Speed", &speed_label(draft, &catalog))).child(segmented(t, "runtime-speed", "Speed", draft.service_tier.as_deref().unwrap_or("auto"), speed_options, |s: &mut App, id: &str| {
            let mut next = s.runtime.draft.clone();
            next.service_tier = (id != "auto").then(|| id.to_owned());
            s.runtime.set_draft(next);
        }))
        .child(text("Extra speed may use more allowance. Auto respects the paid speed preference.").font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted).wrap())
    } else {
        col().w_full().gap(6.0).child(heading(t, "Speed", &speed_label(draft, &catalog))).child(text(if ui.loaded() { "No extra speed is offered for this model." } else { "Load models to see speed options." }).font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted).wrap())
    };

    let paid = row()
        .w_full()
        .gap(12.0)
        .center_y()
        .child(super::ui::switch(t, "runtime-paid-speed", "Allow automatic paid speed", draft.allow_paid_speed, |s: &mut App| {
            let mut next = s.runtime.draft.clone();
            next.allow_paid_speed = !next.allow_paid_speed;
            s.runtime.set_draft(next);
        }))
        .child(col().grow(1.0).min_w(0.0).child(text("Allow automatic paid speed").font(t.ty.meta).color(t.text.base)).child(text("Off by default. When speed is Auto, Neko may pick a tier that uses more allowance.").font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted).wrap()));

    let mut body = col().key("runtime-panel").w_full().gap(12.0).child(head).child(search).child(list).child(effort_block).child(speed_block).child(paid);
    if let Some(error) = ui.catalog_error.as_ref() {
        body = body.child(text(format!("Models could not be loaded: {error}")).font(t.ty.meta).color(t.meaning.amber).wrap());
    }
    for note in &ui.notes {
        body = body.child(text(note.clone()).font(t.ty.meta).color(t.meaning.amber).wrap());
    }
    if let Some(why) = validation(draft, &catalog, ui.loaded()) {
        body = body.child(text(why).key("runtime-validation").font(t.ty.meta).color(t.meaning.amber).wrap());
    }
    if let Some(error) = ui.apply_error.as_ref() {
        body = body.child(text(format!("Settings were not applied. {error}")).key("runtime-error").font(t.ty.meta).color(t.meaning.coral).wrap().selectable(true));
    } else if let Some(status) = ui.status.as_ref().filter(|_| !ui.dirty()) {
        body = body.child(text(status.clone()).key("runtime-status").font(t.ty.meta).color(t.text.muted).live_region(rune::LiveRegion::Polite));
    } else if ui.dirty() && !ui.saving {
        body = body.child(text("Not applied yet.").font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted));
    }
    body = body.child(el().key("runtime-rule").w_full().h(1.0).fill(t.surface.border));
    let mut foot = row().key("runtime-foot").w_full().gap(6.0).center_y().pt(10.0).child(small_button(t, "runtime-reset", "Reset", !ui.saving, false, |s: &mut App| s.runtime.reset()));
    if ui.dirty() && !ui.saving {
        foot = foot.child(small_button(t, "runtime-discard", "Discard", true, false, |s: &mut App| s.runtime.discard()));
    }
    foot = foot.child(el().grow(1.0));
    if ui.saving {
        foot = foot.child(text("Checking…").font(TextStyle { size: 12.0, ..t.ty.meta }).color(t.text.muted));
    }
    foot = foot.child(small_button(t, "runtime-apply", "Check & apply", ui.can_apply(), true, |s: &mut App| s.runtime_apply()));
    body = body.child(foot).child(text("One short check may use allowance. Applies to the next turn or worker.").font(TextStyle { size: 11.5, ..t.ty.meta }).color(t.text.muted).wrap());
    body
}

/// The popover, above the chip.
pub fn mount(s: &App, t: &Neko, layer: &mut Vec<El>) {
    if !s.runtime.open {
        return;
    }
    layer.push(scrim("runtime-scrim", |s: &mut App| s.runtime.open = false));
    let surface = popover::raised(&t.kit, col().key("runtime-popover").role("dialog").label("Model and effort").w(WIDTH).max_w(WIDTH).padding(16.0).min_h(0.0).child(panel(s, t)), t.kit.radius.md);
    layer.push(popover::anchored(&t.kit, surface, s.runtime.anchor, Placement::TopStart, 10.0));
}

impl App {
    /// The session's chip was pressed: opens the popover on what the session has pinned, and reads the catalog once.
    pub fn runtime_toggle(&mut self) {
        let Some(detail) = self.detail() else { return };
        let (conversation, saved, revision) = (format!("task:{}", detail.task.id), clean(&detail.prefs), detail.prefs_revision);
        self.runtime_open(conversation, "runtime-chip", saved, revision);
    }

    /// The ask box's chip: the same popover on Neko's own conversation (what `/models` opens in the web).
    pub fn runtime_toggle_home(&mut self) {
        let Some(m) = self.summary.as_ref() else { return };
        let conversation = m.home_conversation(self.workspace_scope.as_deref());
        let (saved, revision) = m.pins.get(&conversation).map(|(p, r)| (clean(p), *r)).unwrap_or_default();
        self.runtime_open(conversation, "ask-model", saved, revision);
    }

    fn runtime_open(&mut self, conversation: String, anchor: &'static str, saved: neko_protocol::workbench::RuntimePreferences, revision: u64) {
        if self.runtime.open && self.runtime.conversation == conversation {
            self.runtime.open = false;
            return;
        }
        self.runtime.conversation = conversation;
        self.runtime.anchor = anchor;
        self.runtime.saved = saved.clone();
        self.runtime.draft = saved;
        self.runtime.revision = revision;
        self.runtime.open = true;
        self.runtime.apply_error = None;
        self.runtime.status = None;
        self.runtime.notes.clear();
        self.runtime.query = rune::TextEdit::new("");
        if !self.runtime.loaded() && !self.runtime.loading {
            self.runtime.loading = true;
            self.call(crate::link::Call::Models);
        }
    }

    pub fn runtime_refresh(&mut self) {
        if !self.runtime.loading {
            self.runtime.loading = true;
            self.runtime.catalog_error = None;
            self.call(crate::link::Call::ModelsRefresh);
        }
    }

    /// "Check & apply": sends every pin of the conversation, as the web does.
    pub fn runtime_apply(&mut self) {
        if !self.runtime.can_apply() {
            return;
        }
        self.runtime.saving = true;
        self.runtime.apply_error = None;
        let prefs = clean(&self.runtime.draft);
        self.call(crate::link::Call::SetRuntimePrefs { conversation_id: self.runtime.conversation.clone(), prefs });
    }
}
