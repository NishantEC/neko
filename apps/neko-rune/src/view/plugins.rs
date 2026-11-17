//! More, Plugins: the web's Customisations page (`web/src/pages/customisations/Customisations.tsx`) as a view. One row per
//! plugin with its status, a switch, Details, Reload, Copy folder path and Remove (after a confirmation); a details panel with the
//! manifest, version, viewers, pages, how it last loaded and the problem text; "Change Neko…" and "Reload all" at the top.

use rune::{El, TextStyle, col, el, icon, row, text};

use super::ui::{MenuEntry, group, page, quiet_line, switch_when, with_menu};
use super::{primary_button, quiet_button};
use crate::app::App;
use crate::icons;
use crate::link::http::PluginListing;
use crate::pages::ago;
use crate::plugin_manage::{Tone, status_of};
use crate::theme::Neko;

fn small_button(t: &Neko, key: &str, label: &str, glyph: &'static rune::Icon, enabled: bool, on_press: impl Fn(&mut App) + 'static) -> El {
    let b = row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned()).focusable(enabled).size(26.0, 26.0).center().radius(t.radius.control).hover(t.surface.active).focus_ring(2.0, 2.0, t.surface.ring).child(icon(glyph, 14.0).color(if enabled { t.text.muted } else { t.text.faint }).a11y_hidden());
    if enabled { b.on_press(on_press) } else { b.disabled(true) }
}

fn copy_button(t: &Neko, key: &str, label: &str, value: String, toast: &'static str) -> El {
    small_button(t, key, label, &icons::COPY, true, |_| {}).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
        tx.copy(value.clone());
        s.show_toast(toast);
    })
}

fn path_row(s: &App, t: &Neko, key: &str, label: &str, path: Option<&str>, note: &str, toast: &'static str) -> El {
    let _ = s;
    let mut r = row().key(key.to_owned()).w_full().min_h(36.0).px(8.0).gap(12.0).center_y().radius(t.radius.row).hover(t.surface.hover).child(text(label.to_owned()).font(t.ty.row).color(t.text.base).w(64.0)).child(text(path.unwrap_or("…").to_owned()).key(format!("{key}-path")).font(t.ty.mono_meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).help(path.unwrap_or("").to_owned()));
    if !note.is_empty() {
        r = r.child(text(note.to_owned()).font(t.ty.meta).color(t.text.muted));
    }
    match path {
        Some(p) => r.child(copy_button(t, &format!("{key}-copy"), &format!("Copy the {} path", label.to_lowercase()), p.to_owned(), toast)),
        None => r,
    }
}

fn plugin_row(s: &App, t: &Neko, layer: &mut Vec<El>, p: &PluginListing) -> El {
    let health = s.pages.plugins.as_ref().and_then(|l| l.iter().find(|h| h.id == p.id));
    let problem = health.and_then(|h| h.problem.as_deref());
    let pages = s.plugin_pages_of(&p.id);
    let (tone, status) = status_of(p, problem, pages.len());
    let label = if p.title.is_empty() { p.id.clone() } else { p.title.clone() };
    let busy = s.plugin_ui.busy.as_deref() == Some(p.id.as_str());
    let key = format!("plugin-{}", p.id);
    let id = p.id.clone();
    let mut head = row().w_full().min_h(32.0).gap(10.0).center_y();
    if tone == Tone::Error {
        head = head.child(icon(&icons::INFO, 16.0).color(t.meaning.coral).a11y_hidden());
    }
    head = head.child(text(label.clone()).key(format!("{key}-title")).font(t.ty.row).color(t.text.base).help(p.id.clone()));
    if !p.manifest_version.is_empty() {
        head = head.child(text(format!("v{}", p.manifest_version)).font(t.ty.id).color(t.text.faint));
    }
    if !p.entry.is_empty() {
        head = head.child(text(p.entry.trim_start_matches(&format!("/plugins/{}/", p.id)).to_owned()).font(t.ty.mono).color(t.text.faint));
    }
    head = head.child(text(p.description.clone()).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0));
    head = head.child(text(status).key(format!("{key}-status")).font(t.ty.meta).color(match tone {
        Tone::Ok => t.text.muted,
        Tone::Off => t.text.faint,
        Tone::Error => t.meaning.coral,
    }));
    let (a, b, c) = (id.clone(), id.clone(), id.clone());
    head = head
        .child(small_button(t, &format!("{key}-details"), &format!("Details of {label}"), &icons::INFO, true, move |s: &mut App| s.plugin_details(&a)))
        .child(small_button(t, &format!("{key}-reload"), &format!("Reload {label}"), &icons::REFRESH, !p.disabled, move |s: &mut App| s.plugin_reload(Some(&b))));
    if !p.path.is_empty() {
        head = head.child(copy_button(t, &format!("{key}-copy"), &format!("Copy the folder path of {label}"), p.path.clone(), "Folder path copied"));
    }
    head = head.child(small_button(t, &format!("{key}-remove"), &format!("Remove {label}"), &icons::TRASH, !busy, move |s: &mut App| s.ask_remove_plugin(&c)));
    let on = !p.disabled;
    let toggle_id = id.clone();
    head = head.child(switch_when(t, &format!("{key}-switch"), &format!("{} {label}", if on { "Disable" } else { "Enable" }), on, !busy, move |s: &mut App| s.plugin_set(&toggle_id, !on)));
    let mut r = col().key(key.clone()).w_full().radius(t.radius.row).hover(t.surface.hover).px(8.0).child(head);
    if !pages.is_empty() {
        let mut chips = row().w_full().gap(6.0).pb(4.0).wrap();
        for page in &pages {
            let pg = page.clone();
            chips = chips.child(row().key(format!("{key}-page-{}", page.id)).role("button").label(format!("Open {}", page.title)).focusable(true).h(22.0).px(8.0).center().radius(11.0).fill(t.surface.hover).hover(t.surface.active).focus_ring(2.0, 2.0, t.surface.ring).on_press(move |s: &mut App| s.open_plugin_page(pg.clone())).child(text(page.title.clone()).font(t.ty.meta).color(t.text.base).a11y_hidden()));
        }
        r = r.child(chips);
    }
    let last_error = if !p.error.is_empty() { Some(p.error.clone()) } else { problem.map(str::to_owned) };
    if let Some(e) = last_error {
        r = r.child(col().key(format!("{key}-error")).w_full().pb(6.0).gap(2.0).role("alert").child(text("Last error").font(t.ty.meta).color(t.meaning.coral)).child(text(e).font(t.ty.mono).color(t.meaning.coral).wrap().min_w(0.0)));
    }
    let mut entries: Vec<MenuEntry> = vec![
        ("Details".to_owned(), { let id = id.clone(); std::rc::Rc::new(move |s: &mut App| s.plugin_details(&id)) }),
        ("Reload this one".to_owned(), { let id = id.clone(); std::rc::Rc::new(move |s: &mut App| s.plugin_reload(Some(&id))) }),
        (if on { "Disable" } else { "Enable" }.to_owned(), { let id = id.clone(); std::rc::Rc::new(move |s: &mut App| s.plugin_set(&id, !on)) }),
    ];
    if !p.path.is_empty() {
        let path = p.path.clone();
        entries.push(("Open folder".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_path(&path))));
    }
    entries.push(("Remove…".to_owned(), { let id = id.clone(); std::rc::Rc::new(move |s: &mut App| s.ask_remove_plugin(&id)) }));
    with_menu(s, t, layer, &key, r, entries)
}

/// The details panel: what the manifest says, what the plugin registered, and how it last loaded.
fn details(s: &App, t: &Neko, p: &PluginListing) -> El {
    let health = s.pages.plugins.as_ref().and_then(|l| l.iter().find(|h| h.id == p.id));
    let label = |k: &str, v: String, mono: bool, ink: rune::Color| {
        row().w_full().gap(12.0).py(2.0).child(text(k.to_owned()).font(t.ty.meta).color(t.text.muted).w(96.0)).child(text(v).key(format!("plugin-detail-{k}")).font(if mono { t.ty.mono } else { t.ty.meta }).color(ink).wrap().grow(1.0).min_w(0.0))
    };
    let mut c = col().key("plugin-details").w_full().gap(6.0).px(14.0).py(14.0).radius(t.radius.row).border(1.0, t.surface.field_border);
    c = c.child(row().w_full().center_y().child(text(if p.title.is_empty() { p.id.clone() } else { p.title.clone() }).font(TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.strong).grow(1.0)).child(quiet_button("Close", "plugin-details-close", t, |s: &mut App| s.plugin_details_close())));
    if !p.description.is_empty() {
        c = c.child(text(p.description.clone()).font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0));
    }
    c = c.child(label("Version", if p.manifest_version.is_empty() { "not stated".to_owned() } else { p.manifest_version.clone() }, false, t.text.base));
    if !p.viewers.is_empty() {
        let v = p.viewers.iter().map(|(id, title, kinds)| format!("{}{}", if title.is_empty() { id } else { title }, if kinds.is_empty() { String::new() } else { format!(" ({})", kinds.join(", ")) })).collect::<Vec<_>>().join(" · ");
        c = c.child(label("Viewers", v, false, t.text.base));
    }
    if !p.fences.is_empty() {
        c = c.child(label("Fences", p.fences.join(", "), true, t.text.base));
    }
    let pages = s.plugin_pages_of(&p.id);
    if !pages.is_empty() {
        c = c.child(label("Pages", pages.iter().map(|x| x.title.clone()).collect::<Vec<_>>().join(" · "), false, t.text.base));
    }
    let (words, ink) = match health {
        Some(h) if h.problem.is_some() => (format!("Failed: {}", h.problem.clone().unwrap_or_default()), t.meaning.coral),
        Some(h) if h.rolled_back => ("Rolled back to the previous version".to_owned(), t.meaning.amber),
        Some(h) if h.ok => (format!("Loaded {}", ago(s.now_ms, h.at_ms)), t.text.base),
        Some(_) => ("Not loaded".to_owned(), t.text.muted),
        None => ("Not reported yet".to_owned(), t.text.muted),
    };
    c = c.child(label("Last load", words, false, ink));
    if !p.path.is_empty() {
        c = c.child(label("Folder", p.path.clone(), true, t.text.muted));
    }
    let manifest = match &s.plugin_ui.manifest {
        Some((id, text)) if *id == p.id => text.clone(),
        _ => "Reading…".to_owned(),
    };
    c = c.child(text("manifest.json").font(t.ty.meta).color(t.text.muted).pt(6.0));
    c = c.child(text(manifest).key("plugin-manifest").font(t.ty.mono).color(t.text.base).wrap().w_full().px(10.0).py(10.0).radius(t.radius.row).fill(t.surface.hover).min_w(0.0));
    c.child(text("Edit the files in that folder (or ask Neko to); the page reloads by itself a few seconds after a file changes.").font(t.ty.meta).color(t.text.faint).wrap().min_w(0.0))
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let ui = &s.plugin_ui;
    let mut body = col().w_full().gap(14.0);
    // Where things live.
    let custom = ui.custom.as_ref();
    body = body.child(
        col()
            .key("plugin-places")
            .w_full()
            .child(path_row(s, t, "plugin-folder", "Pages", custom.map(|c| c.plugins_dir.as_str()), "", "Folder path copied"))
            .child(path_row(s, t, "css-path", "Styles", custom.map(|c| c.css_path.as_str()), custom.map_or("", |c| if c.css_exists { "In use" } else { "Not created yet" }), "File path copied")),
    );
    let mut list = col().key("plugin-list").w_full().gap(1.0);
    match &ui.listings {
        None => {
            let line = ui.failed.clone().unwrap_or_else(|| "Reading the plugins…".to_owned());
            list = list.child(quiet_line(t, "plugins-loading", &line));
            if ui.failed.is_some() {
                list = list.child(row().px(6.0).child(quiet_button("Try now", "plugins-retry", t, |s: &mut App| s.plugin_reload(None))));
            }
        }
        Some(l) if l.is_empty() => {
            list = list
                .child(quiet_line(t, "plugins-none", "Nothing customised yet"))
                .child(quiet_line(t, "plugins-none-help", "A page of yours is a folder with a manifest.json and an ES module. Give it a built-in page’s id and it replaces that page. Describe what you want and Neko writes it for you."));
        }
        Some(l) => {
            for p in l {
                list = list.child(plugin_row(s, t, layer, p));
            }
        }
    }
    body = body.child(list);
    if let Some(note) = &ui.note {
        body = body.child(text(note.clone()).key("plugins-note").font(t.ty.meta).color(t.meaning.amber).wrap().px(6.0).live_region(rune::LiveRegion::Polite));
    }
    if let Some(detail) = ui.detail.as_deref().and_then(|id| ui.listings.as_ref().and_then(|l| l.iter().find(|p| p.id == id))) {
        body = body.child(details(s, t, detail));
    }
    body = body.child(
        col()
            .w_full()
            .px(6.0)
            .gap(6.0)
            .child(text("Changes Neko makes to its own source go live after they land, and the previous web UI is kept.").font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0))
            .child(row().child(quiet_button("Roll back the web UI in Settings", "plugins-roll-back", t, |s: &mut App| {
                s.pages.section = crate::pages::Section::Updates;
                s.open_view(crate::views::FullView::Settings);
            }))),
    );
    let _ = group;
    let trailing = vec![
        small_button(t, "plugins-reload-all", "Reload all customisations", &icons::REFRESH, !ui.reloading, |s: &mut App| s.plugin_reload(None)),
        primary_button("Change Neko…", "change-neko", t, |s: &mut App| s.ask_change_neko()),
    ];
    let _ = el;
    page(t, "plugins", "Plugins", ui.listings.as_ref().filter(|l| !l.is_empty()).map(Vec::len), trailing, body)
}
