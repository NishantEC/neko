//! Plugins on screen (docs/knowledge/plugins.md): Settings ▸ Plugins (a card per plugin with its state in words, its declared settings
//! as fields, which cats may use it, Test and Remove), the Add sheet that reads a paste while it is typed, the block that says exactly
//! what an approval lets run (the Needs you card and Settings share it), and a cat's list of plugins.
//!
//! A secret is a masked field and a Set button; once set it shows "Set" with Change and Remove. No secret is ever drawn: the daemon
//! sends only whether one is set.

use neko_protocol::Cat;
use neko_protocol::plugins::{AccessMode, FieldKind, Plugin, PluginAsk, PluginState, SettingField};
use rune::{El, el, col, row, text};
use rune_ui_style::{ButtonVariant, Size, tabular, weight};

use crate::plugins::{self, AddSheet, Reading, access_words, ask_of, check_words, found_words, kind_line, list_words, passes_words, shown_value, state_tone, state_words};
use crate::state::App;
use crate::ui::agent::mono;
use crate::ui::badge::Badge;
use crate::ui::base::{Shape, Theme, glyph, paint};
use crate::ui::button::Button;
use crate::ui::dialog::Dialog;
use crate::ui::empty::Empty;
use crate::ui::input::Input;
use crate::ui::portrait::{Activity, Portrait};
use crate::ui::segmented_control::SegmentedControl;
use crate::ui::select::Select;
use crate::ui::switch::Switch;
use crate::ui::textarea::Textarea;
use crate::view::well;
use crate::{icons, ui};

fn h2(t: &Theme, words: &str) -> El {
    text(words.to_owned()).role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())
}

fn h3(t: &Theme, words: &str) -> El {
    text(words.to_owned()).role("heading").level(3).font(weight(t.ty.base, t.ty.medium)).color(t.ink())
}

fn rule(t: &Theme) -> El {
    el().w_full().h(1.0).fill(t.c.line)
}

/// The plug on a quiet ground: what leads a plugin where a cat's portrait leads a task. With `ringed` it takes the room a cat's
/// portrait takes (the Needs you card), and `activity` puts the amber dot on its corner.
pub fn tile(t: &Theme, key: &str, size: f32, activity: Activity, ringed: bool, under: rune::Color) -> El {
    let ground = rune_ui_style::mix(t.c.page.bg, t.ink(), if t.dark { 0.16 } else { 0.09 });
    let face = row().size(size, size).center().child(glyph(&icons::PLUGIN, size * 0.46, t.ink_muted()));
    Portrait::new(key, "Plugin", size).face(face).ground(ground).state(activity).ringed(ringed).under(under).decorative(true).build(t)
}

// ---- What an approval lets run ---------------------------------------------------------------------------------------------------

/// Exactly what approving lets run: the command line (or the address) whole and in mono, what it is handed by name, and, when the
/// manifest changed since it was approved, what differs. `meta` adds the version, the folder and the description (Settings'
/// card says those in its own head).
pub fn ask_details(s: &App, t: &Theme, key: &str, a: &PluginAsk, meta: bool) -> El {
    let mut body = col().key(format!("{key}-ask")).w_full().gap(t.space.at(2.5));
    if meta {
        if let Some(d) = a.description.as_ref().filter(|d| !d.is_empty()) {
            body = body.child(text(d.clone()).key(format!("{key}-description")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
        }
        let mut facts = Vec::new();
        if a.version != "0" {
            facts.push(format!("Version {}", a.version));
        }
        if let Some(f) = &a.folder {
            facts.push(format!("from {}", crate::task_view::show_path(f, "", s.home.as_deref())));
        }
        if !facts.is_empty() {
            body = body.child(text(facts.join(" · ")).key(format!("{key}-facts")).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
        }
    }
    if a.changed && !a.changes.is_empty() {
        let mut lines = col().key(format!("{key}-changes")).role("list").label("What changed").w_full().gap(t.space.at(1.0));
        for (i, c) in a.changes.iter().enumerate() {
            lines = lines.child(
                row()
                    .role("listitem")
                    .label(c.clone())
                    .position_in_set(i + 1, a.changes.len())
                    .gap(t.space.at(2.0))
                    .items_start()
                    .child(row().size(t.control.icon_sm, t.ty.sm.line_height).center().child(glyph(&crate::ui::icons::MINUS, t.control.icon_sm, t.ink_faint())))
                    .child(text(c.clone()).font(t.ty.sm).color(t.ink()).wrap().min_w(0.0).shrink().a11y_hidden()),
            );
        }
        body = body.child(text("What changed").role("heading").level(3).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted())).child(lines);
    }
    let verb = match (a.changed, a.remote) {
        (true, false) => "It will now run",
        (true, true) => "It will now connect to",
        (false, false) => "It will run",
        (false, true) => "It will connect to",
    };
    let runs = well(t, row().key(format!("{key}-runs")).role("note").label(format!("{verb} {}", a.runs)).w_full().padding(t.space.at(2.5)).radius(t.radius.md).round_as(t))
        .child(text(a.runs.clone()).font(mono(t, t.ty.sm)).color(t.ink()).selectable(true).wrap().min_w(0.0).shrink().a11y_hidden());
    body = body.child(
        col()
            .w_full()
            .gap(t.space.at(1.5))
            .child(text(verb).role("heading").level(3).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted()))
            .child(runs),
    );
    if let Some(w) = passes_words(a.remote, &a.passes) {
        body = body.child(text(w).key(format!("{key}-passes")).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    body
}

// ---- Settings ▸ Plugins ----------------------------------------------------------------------------------------------------------

/// The two ways to add one. In Settings they sit under the page's words; in the empty state they are the page's action.
fn add_buttons(t: &Theme) -> El {
    let json = Button::<App>::new("plugins-add-json", "Add MCP server…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(|s: &mut App| s.start_add_json()).build(t);
    let folder = Button::<App>::new("plugins-add-folder", "Add plugin folder…")
        .variant(ButtonVariant::Secondary)
        .size(Size::Sm)
        .build(t)
        .on_press_tx(|s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, crate::state::Pick::PluginFolder, tx));
    row().key("plugins-add").gap(t.space.at(2.0)).center_y().flex_wrap().child(json).child(folder)
}

pub fn page(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let mut part = col().key("settings-plugins").w_full().gap(t.space.at(3.0)).child(h2(t, "Plugins"));
    if s.plugins.is_empty() {
        let empty = Empty::new("plugins-empty", &icons::PLUGIN, "No plugins yet")
            .description("A plugin gives your cats more tools, like searching your docs or filing a ticket. None come with Neko: you add each one, and it runs only after you approve exactly what it will run.")
            .action(add_buttons(t))
            .build(t);
        return part.child(empty);
    }
    part = part
        .child(text("Plugins give your cats more tools. Each one runs only after you approve exactly what it will run.").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0))
        .child(add_buttons(t));
    let n = s.plugins.len();
    let mut list = col().key("plugins-list").role("list").label("Plugins").w_full().gap(t.space.at(3.0));
    for (i, p) in s.plugins.iter().enumerate() {
        list = list.child(card(s, t, p, i + 1, n, layer));
    }
    part.child(list).child(
        text("A plugin is a program that runs on this Mac with your rights, started by Claude Code for a task. Neko keeps what you type into a secret field in the Keychain, never in its own files.")
            .key("plugins-note")
            .font(t.ty.sm)
            .color(t.ink_faint())
            .wrap()
            .min_w(0.0),
    )
}

/// One plugin: who it is and where it stands, what an approval lets run while it waits, its settings, who may use it, and its test.
fn card(s: &App, t: &Theme, p: &Plugin, pos: usize, n: usize, layer: &mut Vec<El>) -> El {
    let state = p.state();
    let key = format!("plugin-{}", p.id);
    let busy = s.plugin_ui.busy.contains(&p.id);
    let words = state_words(state);
    let ok = plugins::approved(p);
    let id = p.id.clone();

    let mut title = row().w_full().gap(t.space.at(2.0)).items_end().child(text(p.name.clone()).key(format!("{key}-name")).role("heading").level(3).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).shrink());
    // A pasted server has no version of its own ("0"): nothing is shown rather than a made-up number.
    if p.version != "0" {
        title = title.child(text(p.version.clone()).key(format!("{key}-version")).font(tabular(t.ty.xs)).color(t.ink_faint()).truncate().min_w(0.0));
    }
    let who = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(t.space.at(0.5))
        .child(title)
        .child(text(kind_line(p, s.home.as_deref())).key(format!("{key}-kind")).font(t.ty.sm).color(t.ink_faint()).truncate_middle().min_w(0.0));
    let mut head = row().w_full().gap(t.space.at(3.0)).center_y().child(tile(t, &format!("{key}-tile"), 40.0, Activity::None, false, t.c.surface.bg)).child(who).child(Badge::new(format!("{key}-state"), words).tone(state_tone(state)).dot().build(t));
    if ok {
        let on = p.enabled;
        let id = id.clone();
        head = head.child(Switch::<App>::new(format!("{key}-switch"), format!("{} is {}", p.name, if on { "on" } else { "off" }), on).disabled(busy).on_toggle(move |s: &mut App| s.set_plugin_on(&id, !on)).build(t));
    }
    let mut card = col()
        .key(key.clone())
        .role("listitem")
        .label(format!("{}: {words}", p.name))
        .position_in_set(pos, n)
        .w_full()
        .padding(t.metrics.inset)
        .gap(t.space.at(3.0))
        .radius(t.radius.xl)
        .round_as(t);
    card = paint(card, &t.mat.slab.base).child(head);
    if let Some(d) = p.description.as_ref().filter(|d| !d.is_empty()) {
        card = card.child(text(d.clone()).key(format!("{key}-description")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    }
    if let Some(why) = &p.problem {
        card = card.child(note_line(t, &format!("{key}-problem"), &icons_warn(), t.c.danger.fg, &format!("Neko cannot read this plugin. {why}")));
    }
    if state == PluginState::NeedsSetup {
        card = card.child(note_line(t, &format!("{key}-missing"), &icons_warn(), t.c.warning.fg, &format!("Still to set: {}. It does not run until you do.", list_words(&p.missing))));
    }
    if let Some(why) = s.plugin_ui.notes.get(&p.id).filter(|_| !s.plugin_ui.testing.contains(&p.id)) {
        card = card.child(note_line(t, &format!("{key}-note"), &icons_warn(), t.c.warning.fg, why));
    }
    if !ok {
        card = card.child(approval(s, t, p, busy));
    }
    if !p.manifest.settings.is_empty() {
        card = card.child(rule(t));
        let mut fields = col().key(format!("{key}-settings")).role("group").label(format!("Settings of {}", p.name)).w_full().gap(t.space.at(4.0)).child(h3(t, "Settings"));
        for f in &p.manifest.settings {
            fields = fields.child(field(s, t, p, f, layer));
        }
        card = card.child(fields);
    }
    card = card.child(rule(t)).child(access(s, t, p, busy)).child(rule(t));
    card.child(test_row(s, t, p, ok))
}

fn icons_warn() -> &'static rune::Icon {
    &ui::icons::WARNING
}

fn note_line(t: &Theme, key: &str, icon: &'static rune::Icon, ink: rune::Color, words: &str) -> El {
    row()
        .key(key.to_owned())
        .role("status")
        .label(words.to_owned())
        .w_full()
        .gap(t.metrics.gap)
        .items_start()
        .child(row().size(t.control.icon, t.ty.sm.line_height.max(t.control.icon)).center().child(glyph(icon, t.control.icon, ink)))
        .child(text(words.to_owned()).font(t.ty.sm).color(ink).wrap().min_w(0.0).shrink().a11y_hidden())
}

/// The block a plugin that is not approved shows in Settings: what it lets run, and Approve and Decline.
fn approval(s: &App, t: &Theme, p: &Plugin, busy: bool) -> El {
    let key = format!("plugin-{}", p.id);
    let lead = match p.state() {
        PluginState::Changed => "It changed since you approved it, so it stays off until you approve it again.",
        PluginState::Declined => "You declined this. It stays off, and Neko does not ask again unless it changes.",
        _ => "Neko runs nothing until you approve it.",
    };
    let ask = ask_of(p);
    let (id, digest) = (p.id.clone(), p.digest.clone());
    let approve = Button::<App>::new(format!("{key}-approve"), "Approve").size(Size::Sm).disabled(busy).on_press(move |s: &mut App| s.approve_plugin(&id, &digest)).build(t);
    let mut buttons = row().gap(t.space.at(2.0)).center_y().child(approve);
    if p.approval != neko_protocol::Approval::Declined {
        let id = p.id.clone();
        buttons = buttons.child(Button::<App>::new(format!("{key}-decline"), "Decline").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(busy).on_press(move |s: &mut App| s.decline_plugin(&id)).build(t));
    }
    well(t, col().key(format!("{key}-approval")).role("group").label(format!("Approval of {}", p.name)).w_full().padding(t.space.at(3.0)).gap(t.space.at(3.0)).radius(t.radius.lg).round_as(t))
        .child(text(lead).font(weight(t.ty.sm, t.ty.medium)).color(t.ink()).wrap().min_w(0.0))
        .child(ask_details(s, t, &key, &ask, false))
        .child(buttons)
}

// ---- A plugin's own settings ------------------------------------------------------------------------------------------------------

fn field_head(t: &Theme, f: &SettingField, p: &Plugin) -> El {
    let unset = !p.value(&f.key).is_some_and(|v| v.set) && f.default.is_none();
    let mut line = row().w_full().gap(t.space.at(2.0)).items_end().child(text(f.label.clone()).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink());
    if f.required && unset {
        line = line.child(text("Required").font(t.ty.xs).color(t.c.warning.fg));
    }
    let mut head = col().w_full().gap(t.space.at(0.5)).child(line);
    if let Some(h) = f.help.as_ref().filter(|h| !h.is_empty()) {
        head = head.child(text(h.clone()).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    head
}

fn field(s: &App, t: &Theme, p: &Plugin, f: &SettingField, layer: &mut Vec<El>) -> El {
    let key = format!("plugin-{}-{}", p.id, f.key);
    let (id, fkey) = (p.id.clone(), f.key.clone());
    let current = shown_value(p, f);
    let busy = s.plugin_ui.busy.contains(&p.id);
    let control = match f.kind {
        FieldKind::Secret => secret(s, t, p, f, &key),
        FieldKind::Text => {
            let saved = current.clone().unwrap_or_default();
            let edit = s.plugin_ui.edits.get(&(id.clone(), fkey.clone())).cloned().unwrap_or_else(|| rune::TextEdit::new(&saved));
            let dirty = edit.text.trim() != saved;
            let (a, b, c) = (id.clone(), id.clone(), fkey.clone());
            let fk = fkey.clone();
            let input = Input::new(key.clone(), &edit, move |s: &mut App| s.plugin_edit_mut(&a, &fk)).name(f.label.clone()).width(340.0).on_submit({
                let fk = c.clone();
                move |s: &mut App| s.save_plugin_text(&b, &fk)
            });
            let input = match &f.default {
                Some(d) => input.placeholder(d.clone()),
                None => input,
            };
            let (b2, c2) = (id.clone(), fkey.clone());
            let save = Button::<App>::new(format!("{key}-save"), "Save").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(!dirty || busy).on_press(move |s: &mut App| s.save_plugin_text(&b2, &c2)).build(t);
            row().gap(t.space.at(2.0)).center_y().flex_wrap().child(input.build(t)).child(save)
        }
        FieldKind::Folder => {
            let shown = current.clone().map(|f| crate::task_view::show_path(&f, "", s.home.as_deref()));
            let (a, b) = (id.clone(), fkey.clone());
            let choose = Button::<App>::new(format!("{key}-choose"), if shown.is_some() { "Change…" } else { "Choose…" })
                .variant(ButtonVariant::Secondary)
                .size(Size::Sm)
                .build(t)
                .on_press_tx(move |s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, crate::state::Pick::PluginSetting(a.clone(), b.clone()), tx));
            let mut line = row().gap(t.space.at(2.5)).center_y().child(glyph(&ui::icons::FOLDER, t.control.icon_sm, t.ink_faint()));
            line = match &shown {
                Some(path) => line.child(text(path.clone()).key(format!("{key}-path")).font(t.ty.sm).color(t.ink()).truncate_middle().min_w(0.0).shrink().max_w(320.0).help(current.clone().unwrap_or_default())),
                None => line.child(text("No folder chosen").key(format!("{key}-path")).font(t.ty.sm).color(t.ink_faint())),
            };
            line = line.child(choose);
            if shown.is_some() && !f.required && p.value(&f.key).is_some_and(|v| v.set) {
                let (a, b) = (id.clone(), fkey.clone());
                line = line.child(Button::<App>::new(format!("{key}-clear"), "Clear").variant(ButtonVariant::Ghost).size(Size::Sm).disabled(busy).on_press(move |s: &mut App| s.set_plugin_value(&a, &b, None)).build(t));
            }
            line
        }
        FieldKind::YesNo => {
            let on = current.as_deref() == Some("yes");
            let (a, b) = (id.clone(), fkey.clone());
            let sw = Switch::<App>::new(key.clone(), f.label.clone(), on).disabled(busy).on_toggle(move |s: &mut App| s.set_plugin_value(&a, &b, Some(if on { "no" } else { "yes" }.to_owned())));
            let sw = match f.help.as_ref().filter(|h| !h.is_empty()) {
                Some(h) => sw.help(h.clone()),
                None => sw.show_label(),
            };
            return sw.build(t);
        }
        FieldKind::Choice => choice(s, t, p, f, &key, layer),
    };
    col().key(format!("{key}-field")).w_full().gap(t.space.at(2.0)).child(field_head(t, f, p)).child(control)
}

fn choice(s: &App, t: &Theme, p: &Plugin, f: &SettingField, key: &str, layer: &mut Vec<El>) -> El {
    let current = shown_value(p, f);
    let busy = s.plugin_ui.busy.contains(&p.id);
    let (id, fkey) = (p.id.clone(), f.key.clone());
    let at = current.as_ref().and_then(|c| f.choices.iter().position(|x| x == c));
    if f.choices.len() <= 3
        && let Some(at) = at
    {
        let items: Vec<&str> = f.choices.iter().map(String::as_str).collect();
        let choices = f.choices.clone();
        return row().child(
            SegmentedControl::<App>::new(key.to_owned(), f.label.clone(), &items, at)
                .size(Size::Sm)
                .disabled(busy)
                .on_change(move |s: &mut App, i| {
                    if let Some(c) = choices.get(i) {
                        s.set_plugin_value(&id, &fkey, Some(c.clone()));
                    }
                })
                .build(t),
        );
    }
    let open = s.menu_open.as_deref() == Some(key);
    let (k1, k2) = (key.to_owned(), key.to_owned());
    let mut sel = Select::<App>::new(key.to_owned(), f.label.clone(), &f.choices).width(240.0).size(Size::Sm).disabled(busy).open(open);
    if let Some(c) = &current {
        sel = sel.current(c);
    }
    let sel = sel
        .on_toggle(move |s: &mut App| s.menu_open = if s.menu_open.as_deref() == Some(k1.as_str()) { None } else { Some(k1.clone()) })
        .on_close(move |s: &mut App| {
            if s.menu_open.as_deref() == Some(k2.as_str()) {
                s.menu_open = None;
            }
        })
        .on_choose(move |s: &mut App, v: &str| {
            s.menu_open = None;
            s.set_plugin_value(&id, &fkey, Some(v.to_owned()));
        })
        .build(t, layer);
    row().child(sel)
}

/// A secret: masked and never shown. Unset or being changed, a masked field and Set; set, the word "Set" with Change and Remove.
fn secret(s: &App, t: &Theme, p: &Plugin, f: &SettingField, key: &str) -> El {
    let (id, fkey) = (p.id.clone(), f.key.clone());
    let busy = s.plugin_ui.busy.contains(&p.id);
    let is_set = p.value(&f.key).is_some_and(|v| v.set);
    let typing = s.plugin_ui.secrets.get(&(id.clone(), fkey.clone()));
    if is_set && typing.is_none() {
        let (a, b, c, d) = (id.clone(), fkey.clone(), id.clone(), fkey.clone());
        return row()
            .key(format!("{key}-isset"))
            .gap(t.space.at(2.5))
            .center_y()
            .child(row().key(format!("{key}-status")).role("status").label(format!("{} is set", f.label)).gap(t.space.at(1.5)).center_y().child(glyph(&ui::icons::CHECK, t.control.icon_sm, t.ink_muted())).child(text("Set").font(weight(t.ty.sm, t.ty.medium)).color(t.ink()).a11y_hidden()))
            .child(Button::<App>::new(format!("{key}-change"), "Change").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(busy).on_press(move |s: &mut App| s.change_plugin_secret(&a, &b)).build(t))
            .child(Button::<App>::new(format!("{key}-remove"), "Remove").variant(ButtonVariant::Ghost).size(Size::Sm).disabled(busy).on_press(move |s: &mut App| s.set_plugin_value(&c, &d, None)).build(t));
    }
    let edit = typing.cloned().unwrap_or_default();
    let empty = edit.text.is_empty();
    let (a, b, c) = (id.clone(), fkey.clone(), id.clone());
    let (fk, fk2) = (fkey.clone(), fkey.clone());
    let input = Input::new(key.to_owned(), &edit, move |s: &mut App| s.plugin_secret_mut(&a, &fk))
        .name(f.label.clone())
        .description("Kept in the Keychain. Neko never shows it again.")
        .placeholder("Paste the value")
        .secure()
        .width(340.0)
        .on_submit(move |s: &mut App| s.save_plugin_secret(&c, &fk2))
        .build(t);
    let set = Button::<App>::new(format!("{key}-set"), "Set").size(Size::Sm).disabled(empty || busy).on_press(move |s: &mut App| s.save_plugin_secret(&id, &b)).build(t);
    let mut line = row().gap(t.space.at(2.0)).center_y().flex_wrap().child(input).child(set);
    if is_set {
        let (a, b) = (p.id.clone(), f.key.clone());
        line = line.child(Button::<App>::new(format!("{key}-cancel"), "Cancel").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(move |s: &mut App| s.cancel_plugin_secret(&a, &b)).build(t));
    }
    line
}

// ---- Which cats ---------------------------------------------------------------------------------------------------------------------

fn access(s: &App, t: &Theme, p: &Plugin, busy: bool) -> El {
    let key = format!("plugin-{}", p.id);
    let only = p.access.mode != AccessMode::All || s.plugin_ui.only.contains(&p.id);
    let id = p.id.clone();
    let seg = SegmentedControl::<App>::new(format!("{key}-access"), format!("Which cats can use {}", p.name), &["Every cat", "Only some cats"], usize::from(only))
        .size(Size::Sm)
        .disabled(busy)
        .on_change(move |s: &mut App, i| if i == 0 { s.choose_every_cat(&id) } else { s.choose_some_cats(&id) })
        .build(t);
    let mut part = col().key(format!("{key}-cats")).role("group").label(format!("Cats that can use {}", p.name)).w_full().gap(t.space.at(2.5)).child(
        row().w_full().gap(t.space.at(3.0)).center_y().child(h3(t, "Who can use it").grow(1.0)).child(seg),
    );
    if only {
        let chosen = if p.access.mode == AccessMode::All { Vec::new() } else { s.plugin_cats(p) };
        let cats = s.active_cats();
        let mut list = col().key(format!("{key}-cat-list")).role("list").label("Cats").w_full().gap(t.space.at(0.5));
        for (i, c) in cats.iter().enumerate() {
            let on = chosen.contains(&c.id);
            let last = on && chosen.len() == 1;
            let (a, b) = (p.id.clone(), c.id.clone());
            let sw = Switch::<App>::new(format!("{key}-cat-{}", c.id), c.name.clone(), on)
                .help(if last { "At least one cat has to be able to use it. Choose Every cat to open it to all." } else { "" })
                .disabled(busy || last)
                .on_toggle(move |s: &mut App| s.toggle_plugin_cat(&a, &b));
            let sw = if last { sw } else { rune_cat_switch(sw) };
            list = list.child(row().role("listitem").position_in_set(i + 1, cats.len()).w_full().child(sw.build(t)));
        }
        part = part.child(list);
        if chosen.is_empty() {
            part = part.child(text("Choose at least one cat. Until you do, every cat can use it.").key(format!("{key}-cats-hint")).role("status").font(t.ty.sm).color(t.c.warning.fg).wrap().min_w(0.0));
        }
    } else {
        part = part.child(text("Every cat at work can use it while it is on.").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    part
}

/// A cat's row in the list is a plain label and a switch; the help line is only for the one that cannot be turned off.
fn rune_cat_switch(sw: Switch<App>) -> Switch<App> {
    sw
}

// ---- Test and Remove ----------------------------------------------------------------------------------------------------------------

fn test_row(s: &App, t: &Theme, p: &Plugin, ok: bool) -> El {
    let key = format!("plugin-{}", p.id);
    let testing = s.plugin_ui.testing.contains(&p.id);
    let remote = p.manifest.mcp.is_remote();
    let id = p.id.clone();
    let can = ok && !remote && p.problem.is_none() && !testing;
    let test = Button::<App>::new(format!("{key}-test"), if testing { "Testing…" } else { "Test" }).variant(ButtonVariant::Secondary).size(Size::Sm).disabled(!can).on_press(move |s: &mut App| s.test_plugin(&id)).build(t);
    let mut result = col().key(format!("{key}-result")).grow(1.0).min_w(0.0).gap(t.space.at(0.5)).pt(t.space.at(1.0));
    if testing {
        result = result.child(text("Testing… this takes up to ten seconds.").key(format!("{key}-testing")).role("status").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    } else if remote {
        result = result.child(text("Neko cannot test web servers yet. Claude Code connects to it when a task runs.").key(format!("{key}-untestable")).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    } else if !ok {
        result = result.child(text("Approve it to test it.").key(format!("{key}-untestable")).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
    } else if let Some(c) = &p.last_check {
        let zone = s.zone();
        let (head, tools, when) = check_words(c, s.now_ms, &*zone);
        let (icon, ink) = if c.ok { (&ui::icons::CIRCLE_CHECK, t.ink_muted()) } else { (&ui::icons::CIRCLE_X, t.c.danger.fg) };
        result = result.child(
            row()
                .key(format!("{key}-check"))
                .role("status")
                .label(format!("{head}. Tested {when}"))
                .w_full()
                .gap(t.space.at(2.0))
                .items_start()
                .child(row().size(t.control.icon_sm, t.ty.sm.line_height).center().child(glyph(icon, t.control.icon_sm, ink)))
                .child(text(head).font(weight(t.ty.sm, t.ty.medium)).color(if c.ok { t.ink() } else { t.c.danger.fg }).wrap().min_w(0.0).shrink().a11y_hidden()),
        );
        if let Some(tools) = tools {
            result = result.child(text(tools).key(format!("{key}-tools")).font(mono(t, t.ty.sm)).color(t.ink_muted()).wrap().min_w(0.0).pl(t.control.icon_sm + t.space.at(2.0)));
        }
        result = result.child(text(format!("Tested {when}")).key(format!("{key}-checked")).font(t.ty.xs).color(t.ink_faint()).pl(t.control.icon_sm + t.space.at(2.0)));
    } else {
        result = result.child(text("Not tested yet.").key(format!("{key}-untested")).font(t.ty.sm).color(t.ink_faint()));
    }
    let id = p.id.clone();
    let remove = Button::<App>::new(format!("{key}-remove"), "Remove…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.ask_remove_plugin(&id)).build(t);
    row().w_full().gap(t.space.at(3.0)).items_start().child(test).child(result).child(remove)
}

// ---- The Add sheet --------------------------------------------------------------------------------------------------------------------

fn labelled(t: &Theme, label: &str, body: El) -> El {
    col().w_full().gap(t.space.at(1.5)).child(text(label.to_owned()).role("heading").level(3).font(weight(t.ty.sm, t.ty.medium)).color(t.ink())).child(body)
}

/// What the paste says it will add, one block per server: its name, the command whole and what it is handed, by name only.
fn found(t: &Theme, list: &[neko_protocol::plugins::Parsed]) -> El {
    const SHOWN: usize = 4;
    let mut rows = col().key("plugin-found").role("list").label("Servers this will add").w_full().gap(t.space.at(2.0));
    for (i, p) in list.iter().take(SHOWN).enumerate() {
        let m = &p.manifest;
        let mut lines = col().grow(1.0).min_w(0.0).gap(t.space.at(1.0)).child(text(m.name.clone()).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).a11y_hidden());
        let runs = m.command_line(None);
        lines = lines.child(
            row().w_full().gap(t.space.at(2.0)).items_start().child(text(if m.mcp.is_remote() { "Connects to" } else { "Runs" }).font(t.ty.sm).color(t.ink_faint()).a11y_hidden()).child(
                text(runs).font(mono(t, t.ty.sm)).color(t.ink()).wrap().min_w(0.0).shrink().selectable(true).a11y_hidden(),
            ),
        );
        if let Some(w) = passes_words(m.mcp.is_remote(), &m.passes()) {
            lines = lines.child(text(w).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).a11y_hidden());
        }
        rows = rows.child(
            well(
                t,
                row()
                    .key(format!("plugin-found-{}", m.name))
                    .role("listitem")
                    .label(found_words(p))
                    .position_in_set(i + 1, list.len().min(SHOWN))
                    .w_full()
                    .padding(t.space.at(3.0))
                    .gap(t.space.at(3.0))
                    .items_start()
                    .radius(t.radius.lg)
                    .round_as(t),
            )
            .child(row().size(t.control.icon, t.ty.base.line_height).center().child(glyph(&icons::PLUGIN, t.control.icon, t.ink_muted())))
            .child(lines),
        );
    }
    if list.len() > SHOWN {
        rows = rows.child(text(format!("and {} more", list.len() - SHOWN)).font(t.ty.sm).color(t.ink_muted()));
    }
    rows
}

pub fn dialog(s: &App, t: &Theme, layer: &mut Vec<El>) {
    let Some(sheet) = &s.plugin_ui.add else { return };
    let reading = sheet.reading();
    let bad = matches!(reading, Reading::Bad(_));
    let area = Textarea::new("plugin-json", &sheet.json, |s: &mut App| &mut s.add_sheet_mut().json)
        .name("Server JSON")
        .placeholder("{\"mcpServers\": {\"acme-docs\": {\"command\": \"npx\", \"args\": [\"-y\", \"@acme/docs-server\"]}}}")
        .rows(4, 5)
        .invalid(bad)
        .build(t);
    let mut body = col().key("plugin-add-form").w_full().gap(t.space.at(3.0)).child(area);
    if sheet.wants_name() {
        let derived = match &reading {
            Reading::Found(f) if sheet.given().is_none() => f.first().map(|p| p.manifest.name.clone()),
            _ => None,
        };
        let name = Input::new("plugin-name", &sheet.name, |s: &mut App| &mut s.add_sheet_mut().name).name("Name").placeholder(derived.unwrap_or_else(|| "acme-docs".to_owned())).description("Optional. Letters, digits, - and _").build(t);
        body = body.child(labelled(t, "Name (optional)", name));
    }
    match &reading {
        Reading::Empty => {
            body = body.child(text("One server, or a whole mcpServers block.").key("plugin-hint").font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
        }
        Reading::Bad(why) => {
            body = body.child(note_line(t, "plugin-error", &ui::icons::CIRCLE_X, t.c.danger.fg, why));
        }
        Reading::Found(list) => {
            // Many servers scroll inside the sheet, so Add never leaves the window.
            body = body.child(found(t, list).max_h(150.0).scroll(rune::ScrollSpec { stick_bottom: false }));
            let notes: Vec<&String> = list.iter().flat_map(|p| p.notes.iter()).take(4).collect();
            for (i, n) in notes.into_iter().enumerate() {
                body = body.child(note_line(t, &format!("plugin-note-{i}"), &ui::icons::INFO, t.ink_muted(), n));
            }
        }
    }
    if let Some(e) = &sheet.error {
        body = body.child(note_line(t, "plugin-add-error", &ui::icons::CIRCLE_X, t.c.danger.fg, e));
    }
    body = body.child(text("Nothing runs until you approve it. Secret values in the paste go to the Keychain, and Neko shows only their names.").key("plugin-add-note").font(t.ty.xs).color(t.ink_faint()).wrap().min_w(0.0));
    let cancel = Button::<App>::new("plugin-add-cancel", "Cancel").variant(ButtonVariant::Secondary).on_press(|s: &mut App| s.close_add_json()).build(t);
    let add = Button::<App>::new("plugin-add-submit", "Add").disabled(!sheet.ready()).loading(sheet.sending).on_press(|s: &mut App| s.submit_add_json()).build(t);
    Dialog::new("plugin-add", "Add an MCP server")
        .description("Paste a server's JSON, as Claude Code or Cursor keep it.")
        .child(body)
        .footer(vec![cancel, add])
        .width(560.0)
        .open(true)
        .return_focus("plugins-add-json")
        .on_close(|s: &mut App| s.close_add_json())
        .build(t, layer);
}

// ---- A cat's plugins ----------------------------------------------------------------------------------------------------------------------

/// A cat's page: the plugins its tasks can use now (approved, on, set up, and open to it), each opening Settings ▸ Plugins at it.
pub fn cat_section(s: &App, t: &Theme, c: &Cat) -> El {
    let mut part = col().key("cat-plugins").w_full().gap(t.space.at(2.5)).child(text("Plugins").role("heading").level(2).font(weight(t.ty.base, t.ty.medium)).color(t.ink()));
    let settings = Button::<App>::new("cat-plugins-settings", "Plugins in Settings").variant(ButtonVariant::Link).size(Size::Sm).on_press(|s: &mut App| s.open_plugins(None)).build(t);
    if s.plugins.is_empty() {
        return part
            .child(text("No plugins. A plugin gives every cat more tools; add one in Settings.").key("cat-no-plugins").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0))
            .child(row().child(settings));
    }
    let mine: Vec<&Plugin> = s.plugins.iter().filter(|p| p.runs_for(&c.id)).collect();
    let on: Vec<&Plugin> = s.plugins.iter().filter(|p| p.state() == PluginState::On).collect();
    if mine.is_empty() {
        let why = if on.is_empty() { "No plugin is on. Turn one on, or approve one, in Settings.".to_owned() } else { format!("No plugin that is on is open to {}. You choose which cats can use each one in Settings.", c.name) };
        return part.child(text(why).key("cat-no-plugins").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0)).child(row().child(settings));
    }
    let every = mine.len() == on.len() && on.iter().all(|p| p.access.mode == AccessMode::All);
    let line = if every { format!("{} can use every plugin that is on.", c.name) } else { format!("{} can use these plugins.", c.name) };
    part = part.child(text(line).key("cat-plugins-line").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    let mut list = col().key("cat-plugins-list").role("list").label(format!("Plugins {} can use", c.name)).w_full().gap(t.space.at(1.0));
    for (i, p) in mine.iter().enumerate() {
        let id = p.id.clone();
        let face = row().key(format!("cat-plugin-{}", p.id)).role("button").label(format!("{}: open in Settings", p.name)).position_in_set(i + 1, mine.len()).focusable(true).gap(t.space.at(2.5)).center_y().h(t.control.md).px(t.space.at(1.0)).radius(t.radius.md).round_as(t);
        let face = rune_ui_style::paint_state(face, &t.mat.row, ui::base::States::default())
            .child(glyph(&icons::PLUGIN, t.control.icon, t.ink_muted()))
            .child(text(p.name.clone()).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink().a11y_hidden())
            .child(text(if p.version == "0" { String::new() } else { p.version.clone() }).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden())
            .on_press(move |s: &mut App| s.open_plugins(Some(&id)));
        list = list.child(row().role("listitem").w_full().child(face));
    }
    let _ = access_words;
    part.child(list).child(row().child(settings))
}

#[allow(dead_code)]
fn unused(_: &AddSheet) {}
