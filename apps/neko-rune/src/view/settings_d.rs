//! Settings, the rest (parity batch D): the theme pickers, "Make me a theme", the summon shortcut, launch at login, first-run
//! setup, the exact model id and its check, Full power, Start over, the rulebook editor, the quick panel's folders, the
//! clipboard switch, diagnostics, the update source and rolling back the web UI. Behaviour is the web's
//! `pages/setup/settings/{Appearance,General,Ai,Rulebook,System}.tsx`; the state and the requests are `extra.rs`.

use rune::{El, TextStyle, col, el, icon, row, text};
use rune_kit::{TextArea, TextInput};

use super::settings::{Item, item};
use super::ui::{edit_of, editing, note, switch, switch_when, with_menu};
use super::webkit::{self, button, setting_row};
use super::{primary_button, quiet_button};
use crate::app::App;
use crate::extra::combo_keys;
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

// ---- General ------------------------------------------------------------------------------------

pub(super) fn general(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Agents, workspaces, routines and phone reminders, one calm pass.";
    out.push(item(Section::General, "Setup", "first-run-setup", "First-run setup", help, setting_row(t, "set-first-run", "First-run setup", Some(help), button(t, "set-first-run-open", "Show again", None, false, false, |s: &mut App| s.open_onboarding(Step::Welcome)))));

    // The keys are the button that records a new one (the web shows them and sends you to the native app).
    let help = "Recording a new shortcut needs the native app: macOS only lets that app register global keys.";
    let control = if s.extra.recording {
        row()
            .gap(8.0)
            .center_y()
            .flex_wrap()
            .child(word(t, "set-hotkey-recording", "Press the new shortcut. Esc cancels.", t.meaning.iris))
            .child(button(t, "set-hotkey-cancel", "Cancel", None, false, false, |s: &mut App| s.hotkey_stop()))
    } else if let Some(h) = &s.extra.hotkey {
        let mut keys = row().key("set-hotkey-change").role("button").label("Change summon shortcut").focusable(true).gap(4.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| s.hotkey_record());
        for (i, k) in combo_keys(&h.combo).iter().enumerate() {
            keys = keys.child(webkit::kbd(t, &format!("set-hotkey-key-{i}"), k));
        }
        keys
    } else {
        webkit::skeleton(t, "set-hotkey-loading", 80.0, 24.0)
    };
    let mut r = col().w_full().child(setting_row(t, "set-hotkey", "Summon shortcut", Some(help), control));
    if let Some(say) = &s.extra.hotkey_say {
        r = r.child(word(t, "set-hotkey-say", &say.words, if say.ok { t.text.muted } else { t.meaning.coral }).px(12.0).pb(4.0).live_region(rune::LiveRegion::Polite));
    }
    out.push(item(Section::General, "Summon & startup", "summon-shortcut", "Summon shortcut", help, with_note(s, t, "setting:hotkey", r)));

    let help = "Opens Neko when you log in. Writes a login item for this Mac.";
    let control = match s.extra.login {
        Some(on) => switch(t, "set-login-switch", "Launch at login", on, |s: &mut App| s.toggle_login()),
        None => switch_when(t, "set-login-switch", "Launch at login", false, false, |_| {}),
    };
    out.push(item(Section::General, "Summon & startup", "launch-at-login", "Launch at login", help, with_note(s, t, "setting:launch-at-login", setting_row(t, "set-login", "Launch at login", Some(help), control))));
}

// ---- Appearance ---------------------------------------------------------------------------------

fn hexc(hex: &str, fallback: rune::Color) -> rune::Color {
    rune::color::hex(hex).unwrap_or(fallback)
}

/// `ThemeSwatch`: the mock window drawn from the theme's own colours (a sidebar, a page with one card, a button and three dots), 72 high.
fn swatch_mock(t: &Neko, key: &str, e: &themes::ThemeEntry, width: f32) -> El {
    let c = &e.colors;
    let wash = hexc(&c.background, t.surface.window);
    let line = |w: f32, ink: &str| el().w(w.max(1.0)).h(3.0).radius(1.5).fill(hexc(ink, t.text.base));
    let inner = width - 2.0;
    let side_w = ((inner * 0.26) * 10.0).round() / 10.0;
    let page_w = inner - side_w;
    let box_w = page_w - 12.0;
    let sidebar = col().key(format!("{key}-a")).w(side_w).h_full().padding(6.0).gap(6.0).fill(hexc(&c.sidebar, wash)).child(line((side_w - 12.0) * 0.7, &c.sidebar_foreground)).child(line((side_w - 12.0) * 0.5, &c.muted)).child(line((side_w - 12.0) * 0.6, &c.muted));
    let card = col().key(format!("{key}-b")).w(box_w).padding(6.0).gap(4.0).radius(5.0).fill(hexc(&c.card, wash)).child(line((box_w - 12.0) * 0.8, &c.foreground)).child(line((box_w - 12.0) * 0.55, &c.muted));
    let dots = row().w(box_w).h(10.0).gap(4.0).center_y().child(el().key(format!("{key}-c")).w(28.0).h(10.0).radius(5.0).fill(hexc(&c.primary, t.meaning.iris))).child(el().grow(1.0)).child(el().size(6.0, 6.0).radius(3.0).fill(hexc(&c.success, t.meaning.mint))).child(el().size(6.0, 6.0).radius(3.0).fill(hexc(&c.warning, t.meaning.amber))).child(el().size(6.0, 6.0).radius(3.0).fill(hexc(&c.destructive, t.meaning.coral)));
    let page = col().w(page_w).h_full().padding(6.0).gap(6.0).child(card).child(el().grow(1.0)).child(dots);
    let edge = hexc(&c.foreground, t.text.base).alpha(0.18);
    row().w(width).h(72.0).radius(14.0).clip_rounded(14.0).fill(wash).border(1.0, edge).child(sidebar).child(page)
}

fn theme_grid(s: &App, t: &Neko, layer: &mut Vec<El>, dark: bool) -> El {
    let slot = if dark { "dark" } else { "light" };
    let (light_id, dark_id) = s.theme_choice();
    let chosen = if dark { dark_id } else { light_id };
    let all: Vec<themes::ThemeEntry> = s.theme_entries().into_iter().filter(|e| e.dark == dark).collect();
    let selected = if all.iter().any(|e| e.id == chosen) { chosen } else { all.first().map(|e| e.id.clone()).unwrap_or_default() };
    // `grid-cols-2 sm:grid-cols-3 lg:grid-cols-4`, gap 12, by the page's own width (the web's viewport).
    let page_w = super::webkit::page_width();
    let cols = if page_w >= 1024.0 { 4 } else if page_w >= 640.0 { 3 } else { 2 };
    let avail = if page_w >= 900.0 { page_w - 241.0 } else { page_w };
    let grid_w = avail.min(832.0) - 80.0;
    let card_w = ((grid_w - 12.0 * (cols as f32 - 1.0)) / cols as f32 * 10.0).floor() / 10.0;
    let mut grid = row().key(format!("themes-{slot}")).w(grid_w).gap(12.0).flex_wrap().role("radiogroup").label(if dark { "Dark theme" } else { "Light theme" });
    for e in all {
        let key = format!("theme-{slot}-{}", e.id);
        let on = e.id == selected;
        let id = e.id.clone();
        let mut name = row().w_full().min_h(21.0).px(2.0).gap(6.0).center_y().child(text(e.name.clone()).font(super::webkit::font(t, 13.0, 500.0, 21.125)).color(t.text.strong).truncate().min_w(0.0).a11y_hidden());
        if on {
            name = name.child(el().grow(1.0)).child(icon(&icons::CHECK_LINE, 14.0).color(t.meaning.iris).a11y_hidden());
        }
        let mut meta = row().w_full().min_h(20.0).px(2.0).gap(4.0).center_y().flex_wrap();
        if !e.family.is_empty() {
            meta = meta.child(text(e.family.clone()).font(super::webkit::font(t, 11.5, 400.0, 18.6875)).color(t.text.muted).a11y_hidden());
        }
        match (e.source, e.overrides) {
            (Source::User, true) => meta = meta.child(text("Yours · replaces built-in").font(t.ty.id).color(t.text.muted).a11y_hidden()),
            (Source::User, false) => meta = meta.child(text("Yours").font(t.ty.id).color(t.text.muted).a11y_hidden()),
            _ => {}
        }
        if e.low_contrast {
            meta = meta.child(text("Low contrast").font(t.ty.id).color(t.meaning.coral).a11y_hidden());
        }
        let mut card = col()
            .key(key.clone())
            .role("radio")
            .label(e.name.clone())
            .checked(on)
            .focusable(true)
            .w(card_w)
            .padding(9.0)
            .gap(8.0)
            .radius(20.0)
            .fill(t.surface.card)
            .border(1.0, if on { t.meaning.iris } else { t.surface.border })
            .hover(t.surface.hover)
            .focus_ring(2.0, 2.0, t.surface.ring)
            .on_press(move |s: &mut App| s.pick_theme(dark, &id))
            .child(swatch_mock(t, &key, &e, card_w - 18.0))
            .child(name)
            .child(meta);
        if on {
            card = card.box_shadow(rune::Shadow { spread: 1.0, color: t.meaning.iris, ..Default::default() });
        }
        let mut entries: Vec<super::ui::MenuEntry> = Vec::new();
        if e.file.is_some() {
            let remove_id = e.id.clone();
            entries.push(("Remove…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.ask_remove_theme(&remove_id))));
        }
        // The web's trash button on a card of the person's own: top right, there while the card is hovered or focused.
        let mut cell = col().w(card_w).hover_group().child(with_menu(s, t, layer, &key, card, entries));
        if e.file.is_some() {
            let remove_id = e.id.clone();
            cell = cell.child(
                row()
                    .key(format!("{key}-remove"))
                    .role("button")
                    .label(format!("Remove {}", e.name))
                    .help("Remove this theme")
                    .focusable(true)
                    .pin(None, Some(14.0), Some(14.0), None)
                    .size(24.0, 24.0)
                    .center()
                    .radius(6.0)
                    .fill(t.surface.window.alpha(0.8))
                    .hover(t.surface.active)
                    .focus_ring(2.0, 2.0, t.surface.ring)
                    .group_opacity(0.0, 1.0)
                    .on_press(move |s: &mut App| s.ask_remove_theme(&remove_id))
                    .child(icon(&icons::TRASH, 13.0).color(t.text.muted).a11y_hidden()),
            );
        }
        grid = grid.child(cell);
    }
    grid
}

pub(super) fn appearance(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    use super::webkit::{button, panel, section};
    let in_use = |dark: bool| {
        let mode = s.pages.appearance.as_deref().unwrap_or("system");
        let on = match mode {
            "dark" => dark,
            "light" => !dark,
            _ => t.dark == dark,
        };
        on.then_some("In use")
    };
    for (dark, id, title, help) in [(false, "light-theme", "Light theme", "The theme used when the window is light."), (true, "dark-theme", "Dark theme", "The theme used when the window is dark.")] {
        let body = with_note(s, t, "setting:themes", theme_grid(s, t, layer, dark));
        out.push(item(Section::Appearance, "", id, title, help, section(t, &format!("set-{id}"), title, in_use(dark), None, body)));
    }
    let mine = s.extra.user_themes.len();
    let words = match mine {
        0 => "None yet.".to_owned(),
        1 => "1 theme of yours, marked “Yours” above. Hover a card to remove it.".to_owned(),
        n => format!("{n} themes of yours, marked “Yours” above. Hover a card to remove it."),
    };
    let help = "Neko writes a theme file for you and checks it.";
    let mut rows = vec![col().w_full().padding(12.0).child(word(t, "your-themes", &words, t.text.muted).font(super::webkit::font(t, 14.0, 400.0, 20.0)))];
    for (i, (file, message)) in s.extra.theme_problems.iter().enumerate() {
        rows.push(word(t, &format!("theme-problem-{i}"), &format!("{file} was skipped: {message}"), t.meaning.amber).px(12.0).py(12.0));
    }
    let make = button(t, "make-theme", "Make me a theme", Some(&icons::SPARK), true, false, |s: &mut App| s.ask_make_theme());
    out.push(item(Section::Appearance, "", "make-theme", "Make me a theme", help, section(t, "set-your-themes", "Your themes", None, Some(make), with_note(s, t, "setting:make-theme", panel(rows)))));
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
    out.push(item(Section::Rulebook, "Rulebook", "rulebook", "Rulebook", help, with_note(s, t, "setting:rulebook", r)));
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
    out.push(item(Section::QuickPanel, "Included folders", "folders", "Included folders", help, with_note(s, t, "setting:folders", list)));
    let typed = !s.pages.text("folder-path").trim().is_empty();
    let help = "An absolute path on this Mac. The daemon checks that it is a folder.";
    let control = row().gap(6.0).center_y().shrink().child(mono_input(s, t, "folder-path", "Folder path", "/Users/you/Documents", 260.0, |s: &mut App| s.add_folder())).child(if typed { quiet_button("Add folder", "folder-add", t, |s: &mut App| s.add_folder()) } else { quiet_button("Add folder", "folder-add", t, |_| {}).disabled(true) });
    out.push(item(Section::QuickPanel, "Included folders", "add-folder", "Add a folder", help, setting_row(t, "set-add-folder", "Add a folder", Some(help), control)));
}

// ---- This Mac: permissions, diagnostics, updates -------------------------------------------------

pub(super) fn clipboard(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let help = "Saves what you copy, only on this Mac. Turning it off stops new capture; existing history stays until you clear it.";
    let control = match s.extra.clipboard {
        Some(on) => switch(t, "set-clipboard-switch", "Clipboard history", on, move |s: &mut App| s.set_clipboard(!on)),
        None => switch_when(t, "set-clipboard-switch", "Clipboard history", false, false, |_| {}),
    };
    out.push(item(Section::Permissions, "What Neko can access", "clipboard-history", "Clipboard history", help, with_note(s, t, "setting:clipboard", setting_row(t, "set-clipboard", "Clipboard history", Some(help), control))));
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
    out.push(item(Section::Diagnostics, "Diagnostics", "diagnostics", "Run diagnostics", help, with_note(s, t, "setting:diagnostics", r)));
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
    out.push(item(Section::Updates, "Neko updates", "update-source", "Updates come from", help, with_note(s, t, "setting:update-source", setting_row(t, "set-update-source", "Updates come from", Some(help), control))));
    let help = "Back to the build it replaced, or the one the app shipped with.";
    out.push(item(Section::Updates, "Neko updates", "roll-back-web", "Roll back the web UI", help, with_note(s, t, "setting:updates-roll", setting_row(t, "set-roll-back", "Roll back the web UI", Some(help), quiet_button("Roll back", "roll-back-web", t, |s: &mut App| s.ask_roll_back_web())))));
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
