//! Settings, the rest (parity batch D): the theme pickers, "Make me a theme", the summon shortcut, launch at login, first-run
//! setup, the exact model id and its check, Full power, Start over, the rulebook editor, the quick panel's folders, the
//! clipboard switch, diagnostics, the update source and rolling back the web UI. Behaviour is the web's
//! `pages/setup/settings/{Appearance,General,Ai,Rulebook,System}.tsx`; the state and the requests are `extra.rs`.

use rune::{El, TextStyle, col, el, icon, row, text};

use super::settings::{Item, item};
use super::ui::{note, switch, switch_when, with_menu};
use super::webkit::{self, button, setting_row, setting_row_stacked};
use super::quiet_button;
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

    // The activity pill (P4, `--overlays`): Swift's "Show activity while Neko works" preference, kept by this client. Only a client that runs the pill shows it.
    if s.overlays.plan.pill {
        let help = "A small status in the corner. Click it to return to Neko.";
        let on = s.overlays.presence.enabled;
        out.push(item(Section::General, "Summon & startup", "activity-pill", "Show activity while Neko works", help, setting_row(t, "set-pill", "Show activity while Neko works", Some(help), switch(t, "set-pill-switch", "Show activity while Neko works", on, move |s: &mut App| s.set_presence(!on)))));
    }
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
    use super::webkit::{self, Tone, button, ghost_button, panel, panel_footer_with};
    let custom = s.summary.as_ref().is_some_and(|m| !m.settings.rulebook.is_empty());
    let dirty = s.rulebook_dirty();
    let help = "Changes apply to the next agent that starts; agents already running keep the rules they started with.";
    // The page is `Rulebook.tsx`: a "Rulebook" section whose action is the status, one stacked row holding the editor, the footer
    // ("Saved · ⌘S saves" and the buttons) and the built-in default under a collapsed button.
    let area = webkit::text_area(s, t, "rulebook", "Rulebook", "The rules Neko and every agent it hands work to follow.", 384.0);
    let editor = setting_row_stacked(t, "set-rulebook", "Rulebook", Some(help), area);
    let plain = webkit::font(t, 12.0, 400.0, 16.0);
    // "Saved · " is one run of text in the web (the spaces are real ones), then the key, then " saves".
    let run = row().child(text((if dirty { "Unsaved changes" } else { "Saved" }).to_owned()).key("rulebook-state").font(plain).color(if dirty { t.meaning.amber } else { t.text.muted })).child(text("·".to_owned()).font(plain).color(t.text.muted).mr(3.3));
    let left = row().gap(8.0).center_y().child(run).child(webkit::kbd(t, "rulebook-kbd", "⌘S")).child(text("saves".to_owned()).font(plain).color(t.text.muted));
    let mut buttons = Vec::new();
    if dirty {
        buttons.push(ghost_button(t, "rulebook-revert", "Revert changes", Some(&icons::UNDO), |s: &mut App| s.revert_rulebook()));
    }
    if custom {
        buttons.push(button(t, "rulebook-restore", "Restore default", None, false, false, |s: &mut App| s.ask_restore_rulebook()));
    }
    let save = button(t, "rulebook-save", "Save rulebook", None, true, false, webkit::gated(dirty, |s: &mut App| s.save_rulebook()));
    buttons.push(if dirty { save } else { save.disabled(true).opacity(0.5) });
    let body_panel = panel(vec![with_note(s, t, "setting:rulebook", editor), panel_footer_with("rulebook-footer", left, buttons)]);
    let open = s.open("rulebook:default");
    let toggle = webkit::ghost_button_after(t, "rulebook-default-toggle", "View Neko’s built-in default", &icons::CHEVRON_DOWN, |s: &mut App| s.toggle("rulebook:default")).ml(-8.0);
    let mut body = col().w_full().gap(6.0).child(body_panel).child(row().child(toggle));
    if open {
        let built_in = s.summary.as_ref().map_or("", |m| m.settings.rulebook_default.as_str());
        let words = if built_in.is_empty() { "The daemon didn’t send the built-in default." } else { built_in };
        let pre = text(words.to_owned()).key("rulebook-default").font(webkit::font(t, 13.0, 400.0, 21.125)).color(t.text.base.alpha(0.85)).wrap().min_w(0.0);
        body = body.child(col().w_full().max_h(384.0).mt(2.0).padding(16.0).radius(12.0).border(1.0, t.surface.border).fill(t.text.base.alpha(0.03)).scroll(rune::ScrollSpec { stick_bottom: false }).child(pre));
    }
    let badge = webkit::status(t, "rulebook-status", if custom { Tone::Success } else { Tone::Neutral }, if custom { "Using your own rules" } else { "Using Neko’s default rules" });
    out.push(item(Section::Rulebook, "", "rulebook", "Rulebook", help, webkit::section(t, "group-rulebook", "Rulebook", None, Some(badge), body)));
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
