//! Settings, the rest (parity batch D): the theme pickers, "Make me a theme", the summon shortcut, launch at login, first-run
//! setup, the exact model id and its check, Full power, Start over, the rulebook editor, the quick panel's folders, the
//! clipboard switch, diagnostics, the update source and rolling back the web UI. Behaviour is the web's
//! `pages/setup/settings/{Appearance,General,Ai,Rulebook,System}.tsx`; the state and the requests are `extra.rs`.

use rune::{El, TextStyle, col, el, icon, row, text};
use rune_kit::{TextArea, TextInput};

use super::settings::{Item, item};
use super::ui::{edit_of, editing, note, setting_row, switch, switch_when, with_menu};
use super::{primary_button, quiet_button};
use crate::app::App;
use crate::extra::{combo_words, valid_model_id};
use crate::icons;
use crate::onboarding::Step;
use crate::pages::Section;
use crate::theme::Neko;
use crate::themes::{self, Source};

fn with_note(s: &App, t: &Neko, key: &str, r: El) -> El {
    match note(s, t, key) {
        Some(n) => col().w_full().child(r).child(n),
        None => r,
    }
}

fn kit_mono(t: &Neko) -> rune_kit::Kit {
    let mut kit = t.kit.clone();
    kit.ty.ui = TextStyle { family: rune::Family::Mono, size: t.ty.mono.size, ..kit.ty.ui };
    kit
}

/// A one-line text field in the mono face (a path, an id, an address).
fn mono_input(s: &App, t: &Neko, key: &str, label: &str, placeholder: &str, width: f32, submit: impl Fn(&mut App) + 'static) -> El {
    TextInput::new(edit_of(s, key)).label(label).placeholder(placeholder).width(Some(width)).on_edit(editing(key.to_owned())).on_submit(move |s: &mut App, _| submit(s)).build(&kit_mono(t)).key(key.to_owned()).shrink().max_w(360.0)
}

fn word(t: &Neko, key: &str, words: &str, ink: rune::Color) -> El {
    text(words.to_owned()).key(key.to_owned()).font(t.ty.meta).color(ink).wrap().min_w(0.0)
}

fn marked(t: &Neko, key: &str, ok: bool, unavailable: bool, words: &str) -> El {
    let (glyph, ink) = if ok { (&icons::CHECK_LINE, t.meaning.mint) } else if unavailable { (&icons::INFO, t.meaning.amber) } else { (&icons::CROSS, t.meaning.coral) };
    row().key(key.to_owned()).w_full().gap(8.0).px(6.0).py(4.0).child(icon(glyph, 16.0).color(ink).a11y_hidden()).child(word(t, &format!("{key}-words"), words, t.text.base).shrink()).live_region(rune::LiveRegion::Polite)
}

// ---- General ------------------------------------------------------------------------------------

pub(super) fn general(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Agents, workspaces, routines and phone reminders, one calm pass.";
    out.push(item(Section::General, "first-run-setup", "First-run setup", help, setting_row(t, "set-first-run", "First-run setup", Some(help), quiet_button("Show again", "set-first-run-open", t, |s: &mut App| s.open_onboarding(Step::Welcome)))));

    let help = "Opens Neko when you log in. This changes the same login item the web UI and the native app do, so it is one setting on this Mac.";
    let control = match s.extra.login {
        Some(on) => switch(t, "set-login-switch", "Launch at login", on, |s: &mut App| s.toggle_login()),
        None => switch_when(t, "set-login-switch", "Launch at login", false, false, |_| {}),
    };
    out.push(item(Section::General, "launch-at-login", "Launch at login", help, with_note(s, t, "setting:launch-at-login", setting_row(t, "set-login", "Launch at login", Some(help), control))));

    let help = "Recording a shortcut checks it against what macOS reserves, then saves it. The app that owns the global key registers it.";
    let shown = s.extra.hotkey.as_ref().map_or_else(|| "…".to_owned(), |h| combo_words(&h.combo));
    let control = if s.extra.recording {
        row()
            .gap(8.0)
            .center_y()
            .flex_wrap()
            .child(word(t, "set-hotkey-recording", "Press the new shortcut. Esc cancels.", t.meaning.iris))
            .child(quiet_button("Cancel", "set-hotkey-cancel", t, |s: &mut App| s.hotkey_stop()))
    } else {
        row()
            .gap(8.0)
            .center_y()
            .child(text(shown).key("set-hotkey-value").font(t.ty.row).color(t.text.base).role("status"))
            .child(quiet_button("Change…", "set-hotkey-change", t, |s: &mut App| s.hotkey_record()))
    };
    let mut r = col().w_full().child(setting_row(t, "set-hotkey", "Summon shortcut", Some(help), control));
    if let Some(say) = &s.extra.hotkey_say {
        r = r.child(word(t, "set-hotkey-say", &say.words, if say.ok { t.text.muted } else { t.meaning.coral }).px(6.0).pb(4.0).live_region(rune::LiveRegion::Polite));
    }
    out.push(item(Section::General, "summon-shortcut", "Summon shortcut", help, with_note(s, t, "setting:hotkey", r)));
}

// ---- AI and agents ------------------------------------------------------------------------------

pub(super) fn ai(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let typed = s.pages.text("custom-model").trim().to_owned();
    let help = "For a model that isn’t in the list.";
    let control = row()
        .gap(6.0)
        .center_y()
        .shrink()
        .child(mono_input(s, t, "custom-model", "Exact model ID", "Or enter an exact model ID", 240.0, |s: &mut App| s.use_exact_model()))
        .child(if valid_model_id(&typed) { quiet_button("Use ID", "set-custom-use", t, |s: &mut App| s.use_exact_model()) } else { quiet_button("Use ID", "set-custom-use", t, |_| {}).disabled(true) });
    out.push(item(Section::Ai, "custom-model", "Exact model ID", help, setting_row(t, "set-custom-model", "Exact model ID", Some(help), control)));

    let help = "Sends one real prompt through the runner with the runtime shown above. It becomes the default only when it works.";
    let button = if s.extra.checking { quiet_button("Checking…", "set-check-model", t, |_| {}).disabled(true) } else { quiet_button("Check model", "set-check-model", t, |s: &mut App| s.check_model()) };
    let mut r = col().w_full().child(setting_row(t, "set-check", "Check model", Some(help), button));
    if let Some(check) = &s.extra.model_check {
        r = r.child(marked(t, "set-check-result", check.ok, check.unavailable, &check.message));
    }
    out.push(item(Section::Ai, "check-model", "Check model", help, with_note(s, t, "setting:model-check", r)));

    let help = "Home chat works like your own Claude Code session in bypass mode: it edits files, runs commands, uses your tools and browser, and manages Neko itself, without asking per step. Off makes it read-only, with an approval for each action.";
    let control = match s.extra.super_agent {
        Some(on) => switch(t, "set-super-agent-switch", "Full power for Home chat", on, move |s: &mut App| s.set_super_agent(!on)),
        None => switch_when(t, "set-super-agent-switch", "Full power for Home chat", false, false, |_| {}),
    };
    out.push(item(Section::Ai, "super-agent", "Full power", help, with_note(s, t, "setting:super-agent", setting_row(t, "set-super-agent", "Full power", Some(help), control))));

    let help = "Removes every task except those with a pull request, and lets your watches file their issues again from scratch. Memory, settings, workspaces, tools and watches stay.";
    let danger = row()
        .key("set-start-over-button")
        .role("button")
        .label("Start over…")
        .focusable(true)
        .h(26.0)
        .px(10.0)
        .radius(t.radius.control)
        .center()
        .border(1.0, t.meaning.coral.alpha(0.5))
        .hover(t.meaning.coral.alpha(0.12))
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.ask_start_over())
        .child(text("Start over…").font(t.ty.meta).color(t.meaning.coral).a11y_hidden());
    out.push(item(Section::Ai, "start-over", "Start over", help, with_note(s, t, "setting:start-over", setting_row(t, "set-start-over", "Start over", Some(help), danger))));
}

// ---- Appearance ---------------------------------------------------------------------------------

fn swatch(t: &Neko, key: &str, hex: &str, r: f32) -> El {
    el().key(key.to_owned()).size(14.0, 14.0).radius(r).fill(rune::color::hex(hex).unwrap_or(t.surface.window)).border(1.0, t.surface.field_border)
}

fn theme_rows(s: &App, t: &Neko, layer: &mut Vec<El>, dark: bool) -> El {
    let slot = if dark { "dark" } else { "light" };
    let (light_id, dark_id) = s.theme_choice();
    let chosen = if dark { dark_id } else { light_id };
    let all: Vec<themes::ThemeEntry> = s.theme_entries().into_iter().filter(|e| e.dark == dark).collect();
    // The id in use, or the first when the chosen one is not listed (the web's `selected`).
    let selected = if all.iter().any(|e| e.id == chosen) { chosen } else { all.first().map(|e| e.id.clone()).unwrap_or_default() };
    let mut list = col().key(format!("themes-{slot}")).w_full().gap(1.0).role("radiogroup").label(if dark { "Dark theme" } else { "Light theme" });
    for e in all {
        let key = format!("theme-{slot}-{}", e.id);
        let on = e.id == selected;
        let id = e.id.clone();
        let [a, b, c] = themes::swatch(&e);
        let tag = match (e.source, e.overrides) {
            (Source::User, true) => "Customised",
            (Source::User, false) => "Yours",
            _ => "",
        };
        let mut line = row()
            .key(key.clone())
            .role("radio")
            .label(e.name.clone())
            .checked(on)
            .focusable(true)
            .w_full()
            .h(32.0)
            .px(8.0)
            .gap(10.0)
            .center_y()
            .radius(t.radius.row)
            .fill(if on { t.surface.selected } else { rune::Color::TRANSPARENT })
            .hover(t.surface.hover)
            .focus_ring(2.0, 2.0, t.surface.ring)
            .on_press(move |s: &mut App| s.pick_theme(dark, &id))
            .child(row().gap(2.0).a11y_hidden().child(swatch(t, &format!("{key}-a"), &a, 4.0)).child(swatch(t, &format!("{key}-b"), &b, 4.0)).child(swatch(t, &format!("{key}-c"), &c, 7.0)))
            .child(text(e.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).a11y_hidden());
        if !e.family.is_empty() && e.family != e.name {
            line = line.child(text(e.family.clone()).font(t.ty.meta).color(t.text.faint).truncate().min_w(0.0).a11y_hidden());
        }
        if !tag.is_empty() {
            line = line.child(text(tag).font(t.ty.id).color(t.text.muted).a11y_hidden());
        }
        if e.low_contrast {
            line = line.child(text("Low contrast").font(t.ty.id).color(t.meaning.amber).a11y_hidden());
        }
        line = line.child(el().grow(1.0).min_w(0.0));
        let mut entries: Vec<super::ui::MenuEntry> = Vec::new();
        if e.file.is_some() {
            let remove_id = e.id.clone();
            line = line.child(
                row()
                    .key(format!("{key}-remove"))
                    .role("button")
                    .label(format!("Remove {}", e.name))
                    .help("Remove this theme file")
                    .focusable(true)
                    .size(24.0, 24.0)
                    .center()
                    .radius(t.radius.control)
                    .hover(t.surface.active)
                    .focus_ring(2.0, 2.0, t.surface.ring)
                    .on_press({
                        let id = remove_id.clone();
                        move |s: &mut App| s.ask_remove_theme(&id)
                    })
                    .child(icon(&icons::TRASH, 14.0).color(t.text.muted).a11y_hidden()),
            );
            entries.push(("Remove…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.ask_remove_theme(&remove_id))));
        }
        if on {
            line = line.child(icon(&icons::CHECK_LINE, 16.0).color(t.meaning.iris).a11y_hidden());
        }
        list = list.child(with_menu(s, t, layer, &key, line, entries));
    }
    list
}

pub(super) fn appearance(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    for (dark, id, title, help) in [(false, "light-theme", "Light theme", "The theme used when the window is light."), (true, "dark-theme", "Dark theme", "The theme used when the window is dark.")] {
        let heading = row().w_full().px(6.0).pt(6.0).pb(2.0).child(text(title).font(t.ty.group).color(t.text.muted));
        let body = col().w_full().child(heading).child(theme_rows(s, t, layer, dark));
        out.push(item(Section::Appearance, id, title, help, with_note(s, t, "setting:themes", body)));
    }
    let mine = s.extra.user_themes.len();
    let words = match mine {
        0 => "None yet.".to_owned(),
        1 => "1 theme of yours, marked “Yours” above. Hover it to remove it.".to_owned(),
        n => format!("{n} themes of yours, marked “Yours” above. Hover one to remove it."),
    };
    let help = "Neko writes a theme file for you and checks it.";
    let mut r = col().w_full().child(setting_row(t, "set-make-theme", "Your themes", Some(help), quiet_button("Make me a theme", "make-theme", t, |s: &mut App| s.ask_make_theme())));
    r = r.child(word(t, "your-themes", &words, t.text.muted).px(6.0).pb(4.0));
    for (i, (file, message)) in s.extra.theme_problems.iter().enumerate() {
        r = r.child(word(t, &format!("theme-problem-{i}"), &format!("{file} was skipped: {message}"), t.meaning.amber).px(6.0).pb(2.0));
    }
    out.push(item(Section::Appearance, "make-theme", "Make me a theme", help, r));
}

// ---- Rulebook -----------------------------------------------------------------------------------

pub(super) fn rulebook(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let custom = s.summary.as_ref().is_some_and(|m| !m.settings.rulebook.is_empty());
    let dirty = s.rulebook_dirty();
    let help = "Changes apply to the next agent that starts; agents already running keep the rules they started with.";
    let status = if custom { "Using your own rules" } else { "Using Neko’s default rules" };
    let area = TextArea::new(edit_of(s, "rulebook")).label("Rulebook").placeholder("The rules Neko and every agent it hands work to follow.").rows(18).on_edit(editing("rulebook".to_owned())).build(&kit_mono(t)).key("rulebook").w_full().min_w(0.0);
    let mut buttons = row().gap(8.0).center_y().flex_wrap();
    if dirty {
        buttons = buttons.child(quiet_button("Revert changes", "rulebook-revert", t, |s: &mut App| s.revert_rulebook()));
    }
    if custom {
        buttons = buttons.child(quiet_button("Restore default", "rulebook-restore", t, |s: &mut App| s.ask_restore_rulebook()));
    }
    buttons = buttons.child(if dirty { primary_button("Save rulebook", "rulebook-save", t, |s: &mut App| s.save_rulebook()) } else { primary_button("Save rulebook", "rulebook-save", t, |_| {}).disabled(true).opacity(0.4) });
    let mut r = col()
        .key("set-rulebook")
        .w_full()
        .gap(8.0)
        .child(row().w_full().px(6.0).gap(8.0).center_y().child(text(help).font(t.ty.meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0)).child(text(status).key("rulebook-status").font(t.ty.id).color(if custom { t.meaning.mint } else { t.text.muted })))
        .child(area)
        .child(row().w_full().px(6.0).gap(8.0).center_y().flex_wrap().child(text(if dirty { "Unsaved changes · ⌘S saves" } else { "Saved · ⌘S saves" }).key("rulebook-state").font(t.ty.id).color(if dirty { t.meaning.amber } else { t.text.faint })).child(el().grow(1.0)).child(buttons));
    let open = s.open("rulebook:default");
    r = r.child(row().child(quiet_button(if open { "Hide Neko’s built-in default" } else { "View Neko’s built-in default" }, "rulebook-default-toggle", t, |s: &mut App| s.toggle("rulebook:default"))));
    if open {
        let built_in = s.summary.as_ref().map_or("", |m| m.settings.rulebook_default.as_str());
        let words = if built_in.is_empty() { "The daemon didn’t send the built-in default." } else { built_in };
        r = r.child(text(words.to_owned()).key("rulebook-default").font(t.ty.mono).color(t.text.muted).wrap().w_full().px(12.0).py(10.0).radius(t.radius.row).fill(t.surface.hover));
    }
    out.push(item(Section::Rulebook, "rulebook", "Rulebook", help, with_note(s, t, "setting:rulebook", r)));
}

// ---- Quick panel --------------------------------------------------------------------------------

pub(super) fn quick_panel(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Folders searched by the quick panel.";
    let mut list = col().key("set-folders").w_full().gap(1.0);
    match &s.extra.folders {
        None => list = list.child(word(t, "folders-loading", "Reading the folders…", t.text.muted).px(6.0).py(8.0)),
        Some(f) if f.is_empty() => list = list.child(word(t, "folders-none", "No folders added", t.text.muted).px(6.0).py(8.0)),
        Some(f) => {
            for (i, folder) in f.iter().enumerate() {
                let id = folder.id.clone();
                list = list.child(
                    row()
                        .key(format!("folder-{i}"))
                        .w_full()
                        .h(32.0)
                        .px(8.0)
                        .gap(10.0)
                        .center_y()
                        .radius(t.radius.row)
                        .hover(t.surface.hover)
                        .child(icon(&icons::FOLDER, 16.0).color(t.text.muted).a11y_hidden())
                        .child(text(folder.title.clone()).font(t.ty.mono_meta).color(t.text.base).truncate().grow(1.0).min_w(0.0))
                        .child(quiet_button("Remove", &format!("folder-remove-{i}"), t, move |s: &mut App| s.remove_folder(&id)).label(format!("Remove {} from search", folder.title))),
                );
            }
        }
    }
    out.push(item(Section::QuickPanel, "folders", "Included folders", help, with_note(s, t, "setting:folders", list)));
    let typed = !s.pages.text("folder-path").trim().is_empty();
    let help = "An absolute path on this Mac. The daemon checks that it is a folder.";
    let control = row().gap(6.0).center_y().shrink().child(mono_input(s, t, "folder-path", "Folder path", "/Users/you/Documents", 260.0, |s: &mut App| s.add_folder())).child(if typed { quiet_button("Add folder", "folder-add", t, |s: &mut App| s.add_folder()) } else { quiet_button("Add folder", "folder-add", t, |_| {}).disabled(true) });
    out.push(item(Section::QuickPanel, "add-folder", "Add a folder", help, setting_row(t, "set-add-folder", "Add a folder", Some(help), control)));
}

// ---- This Mac: permissions, diagnostics, updates -------------------------------------------------

pub(super) fn clipboard(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Saves what you copy, only on this Mac. Turning it off stops new capture; existing history stays until you clear it.";
    let control = match s.extra.clipboard {
        Some(on) => switch(t, "set-clipboard-switch", "Clipboard history", on, move |s: &mut App| s.set_clipboard(!on)),
        None => switch_when(t, "set-clipboard-switch", "Clipboard history", false, false, |_| {}),
    };
    out.push(item(Section::Permissions, "clipboard-history", "Clipboard history", help, with_note(s, t, "setting:clipboard", setting_row(t, "set-clipboard", "Clipboard history", Some(help), control))));
}

pub(super) fn diagnostics(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Times each runtime and shows which installs Neko uses.";
    let button = if s.extra.diag_running { quiet_button("Running…", "diag-run", t, |_| {}).disabled(true) } else { quiet_button("Run checks", "diag-run", t, |s: &mut App| s.run_diagnostics()) };
    let mut r = col().key("diag-checks").w_full().child(setting_row(t, "set-diag-run", "Run checks", Some(help), button));
    match &s.extra.diag {
        None if !s.extra.diag_running => r = r.child(word(t, "diag-none", "Run a check to see connection and runtime details.", t.text.muted).px(6.0).py(6.0)),
        None => {}
        Some((ms, checks)) => {
            r = r.child(setting_row(t, "diag-roundtrip", "Neko window ↔ daemon", None, text(format!("{ms} ms")).key("diag-roundtrip-ms").font(t.ty.mono_meta).color(t.text.muted)));
            for (i, c) in checks.iter().enumerate() {
                let (glyph, ink) = if c.ok { (&icons::CHECK_LINE, t.meaning.mint) } else { (&icons::CROSS, t.meaning.coral) };
                let mut line = row().key(format!("diag-check-{i}")).w_full().min_h(32.0).px(6.0).py(4.0).gap(10.0).center_y().radius(t.radius.row).hover(t.surface.hover);
                line = line.child(icon(glyph, 16.0).color(ink).a11y_hidden()).child(col().grow(1.0).min_w(0.0).child(text(c.name.clone()).key(format!("diag-check-{i}-name")).font(t.ty.row).color(t.text.base)).child(text(c.detail.clone()).font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0)));
                if c.millis > 0 {
                    line = line.child(text(format!("{} ms", c.millis)).font(t.ty.mono_meta).color(t.text.muted));
                }
                r = r.child(line);
            }
        }
    }
    out.push(item(Section::Diagnostics, "diagnostics", "Run diagnostics", help, with_note(s, t, "setting:diagnostics", r)));
}

pub(super) fn updates(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let Some(u) = s.summary.as_ref().and_then(|m| m.settings.self_update.clone()) else { return };
    let dirty = s.pages.edited("update-source", false);
    let help = "owner/name or a github.com address. Empty follows Neko’s own repository.";
    let _ = &u;
    let mut control = row().gap(6.0).center_y().shrink().child(mono_input(s, t, "update-source", "Updates come from", "owner/name", 260.0, |s: &mut App| s.save_update_source()));
    if dirty {
        control = control.child(quiet_button("Save", "update-source-save", t, |s: &mut App| s.save_update_source()));
    }
    out.push(item(Section::Updates, "update-source", "Updates come from", help, with_note(s, t, "setting:update-source", setting_row(t, "set-update-source", "Updates come from", Some(help), control))));
    let help = "Back to the build it replaced, or the one the app shipped with.";
    out.push(item(Section::Updates, "roll-back-web", "Roll back the web UI", help, with_note(s, t, "setting:updates-roll", setting_row(t, "set-roll-back", "Roll back the web UI", Some(help), quiet_button("Roll back", "roll-back-web", t, |s: &mut App| s.ask_roll_back_web())))));
}

// ---- the unsaved-rulebook question --------------------------------------------------------------

/// "Discard your changes?" when a half-written rulebook is about to be left behind.
pub fn mount_guard(s: &App, t: &Neko, layer: &mut Vec<El>) {
    if s.extra.going.is_none() {
        return;
    }
    let no = quiet_button("Keep editing", "guard-keep", t, |s: &mut App| s.guard_keep());
    let yes = row()
        .key("guard-discard")
        .role("button")
        .label("Discard changes")
        .focusable(true)
        .h(32.0)
        .px(14.0)
        .center()
        .radius(t.radius.control)
        .fill(t.meaning.coral)
        .hover(rune::Color::WHITE.alpha(0.14))
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.guard_discard())
        .child(text("Discard changes").font(TextStyle { weight: 500.0, ..t.ty.row }).color(rune::Color::WHITE).a11y_hidden());
    let body = col()
        .key("guard-body")
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        .child(text("The rulebook has changes that are not saved. Leaving now discards them.").key("guard-detail").font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0))
        .child(row().w_full().gap(8.0).justify_end().child(no).child(yes));
    rune_kit::Dialog::new("guard", "Discard your changes?").open(true).width(420.0).fit().child(body).on_close(|s: &mut App| s.guard_keep()).build(&t.kit, layer);
}
