//! More, Plugins: the web's Customisations page (`web/src/pages/customisations/Customisations.tsx`) as a view. One row per
//! plugin with its status, a switch, Details, Reload, Copy folder path and Remove (after a confirmation); a details panel with the
//! manifest, version, viewers, pages, how it last loaded and the problem text; "Change Neko…" and "Reload all" at the top.

use rune::{El, TextStyle, col, el, icon, row, text};

use super::ui::{MenuEntry, page, with_menu};
use super::{quiet_button, webkit};
use crate::app::App;
use crate::icons_phosphor;
use crate::link::http::PluginListing;
use crate::pages::ago;
use crate::plugin_manage::{Tone, status_of};
use crate::theme::Neko;

const MANIFEST_EXAMPLE: &str = "{\n  \"title\": \"PR board\",\n  \"description\": \"Open pull requests by stage\",\n  \"version\": \"0.1.0\",\n  \"entry\": \"index.js\",\n  \"group\": \"work\"\n}";

/// A `RowAction` that copies `value` and says so.
fn copy_action(t: &Neko, key: &str, label: &str, value: String, toast: &'static str) -> El {
    webkit::row_action(t, key, label, &icons_phosphor::COPY_REGULAR, |_| {}).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
        tx.copy(value.clone());
        s.show_toast(toast);
    })
}

/// `PathRow`: the label (80 wide), the path in mono (12) that gives way, a note, and the copy action on hover; 36 high, 12 at the sides.
#[allow(clippy::too_many_arguments)]
fn path_row(t: &Neko, key: &str, label: &str, path: Option<&str>, note: &str, toast: &'static str) -> El {
    let mut r = row().key(key.to_owned()).w_full().min_h(36.0).px(12.0).py(4.0).gap(12.0).center_y().radius(10.0).hover_group().hover(t.text.base.alpha(0.03)).child(text(label.to_owned()).font(webkit::font(t, 13.5, 400.0, 20.25)).color(t.text.base).w(80.0)).child(text(path.unwrap_or("…").to_owned()).key(format!("{key}-path")).font(webkit::mono(webkit::font(t, 12.0, 400.0, 18.0))).color(t.text.muted).truncate().grow(1.0).shrink().min_w(0.0).help(path.unwrap_or("").to_owned()));
    if !note.is_empty() {
        r = r.child(text(note.to_owned()).font(webkit::font(t, 12.0, 400.0, 18.0)).color(t.text.muted));
    }
    let action = match path {
        Some(p) => copy_action(t, &format!("{key}-copy"), &format!("Copy the {} path", label.to_lowercase()), p.to_owned(), toast),
        None => webkit::row_action(t, &format!("{key}-copy"), &format!("Copy the {} path", label.to_lowercase()), &icons_phosphor::COPY_REGULAR, |_| {}).opacity(0.4),
    };
    r.child(webkit::row_actions(vec![action]))
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
    let small = webkit::font(t, 12.0, 400.0, 18.0);
    let mut head = row().w_full().min_h(32.0).gap(12.0).center_y();
    if tone == Tone::Error {
        head = head.child(icon(&icons_phosphor::WARNING_REGULAR, 16.0).color(t.meaning.coral).a11y_hidden());
    }
    head = head.child(text(label.clone()).key(format!("{key}-title")).font(webkit::font(t, 13.5, 400.0, 20.25)).color(t.text.base).help(p.id.clone()));
    if !p.manifest_version.is_empty() {
        head = head.child(row().children([text("v".to_owned()).font(webkit::tnum(small)).color(t.text.muted.alpha(0.7)).a11y_hidden(), text(p.manifest_version.clone()).font(webkit::tnum(small)).color(t.text.muted.alpha(0.7)).a11y_hidden()]));
    }
    if !p.entry.is_empty() {
        head = head.child(text(p.entry.trim_start_matches(&format!("/plugins/{}/", p.id)).to_owned()).font(webkit::mono(webkit::font(t, 11.5, 400.0, 17.25))).color(t.text.muted.alpha(0.7)));
    }
    head = head.child(text(p.description.clone()).font(webkit::font(t, 12.5, 400.0, 18.75)).color(t.text.muted).truncate().grow(1.0).shrink().min_w(0.0));
    head = head.child(text(status).key(format!("{key}-status")).font(webkit::font(t, 12.5, 400.0, 18.75)).color(match tone {
        Tone::Ok => t.text.muted,
        Tone::Off => t.text.muted.alpha(0.7),
        Tone::Error => t.meaning.coral,
    }));
    let (a, b, c) = (id.clone(), id.clone(), id.clone());
    let mut acts = vec![
        webkit::row_action(t, &format!("{key}-details"), &format!("Details of {label}"), &icons_phosphor::INFO_REGULAR, move |s: &mut App| s.plugin_details(&a)),
        {
            let reload = webkit::row_action(t, &format!("{key}-reload"), &format!("Reload {label}"), &icons_phosphor::ARROWS_CLOCKWISE, webkit::gated(!p.disabled && !busy, move |s: &mut App| s.plugin_reload(Some(&b))));
            if p.disabled || busy { reload.opacity(0.4) } else { reload }
        },
    ];
    if !p.path.is_empty() {
        acts.push(copy_action(t, &format!("{key}-copy"), &format!("Copy the folder path of {label}"), p.path.clone(), "Folder path copied"));
    }
    let remove = webkit::row_action(t, &format!("{key}-remove"), &format!("Remove {label}"), &icons_phosphor::TRASH_REGULAR, webkit::gated(!busy, move |s: &mut App| s.ask_remove_plugin(&c)));
    acts.push(if busy { remove.opacity(0.4) } else { remove });
    head = head.child(webkit::row_actions(acts));
    let on = !p.disabled;
    let toggle_id = id.clone();
    head = head.child(webkit::toggle(t, &format!("{key}-switch"), &format!("{} {label}", if on { "Disable" } else { "Enable" }), on, webkit::gated(!busy, move |s: &mut App| s.plugin_set(&toggle_id, !on))));
    let mut r = col().key(key.clone()).w_full().radius(10.0).hover_group().hover(t.text.base.alpha(0.03)).px(12.0).py(6.0).gap(4.0).child(head);
    if !pages.is_empty() {
        let mut chips = row().w_full().gap(6.0).wrap().center_y();
        for page in &pages {
            let pg = page.clone();
            chips = chips.child(row().key(format!("{key}-page-{}", page.id)).role("button").label(format!("Open {}", page.title)).focusable(true).px(8.0).py(2.0).center().radius(12.0).fill(t.text.base.alpha(0.06)).hover(t.text.base.alpha(0.1)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.open_plugin_page(pg.clone())).child(text(page.title.clone()).font(small).color(t.text.base).a11y_hidden()));
        }
        r = r.child(chips);
    }
    let last_error = if !p.error.is_empty() { Some(p.error.clone()) } else { problem.map(str::to_owned) };
    if let Some(e) = last_error {
        r = r.child(col().key(format!("{key}-error")).w_full().gap(2.0).role("alert").child(text("Last error".to_owned()).font(small).color(t.meaning.coral)).child(text(e).font(webkit::mono(small)).color(t.meaning.coral).wrap().min_w(0.0)));
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

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>, region_w: f32) -> El {
    webkit::set_page_width(region_w);
    let ui = &s.plugin_ui;
    let custom = ui.custom.as_ref();
    let small = webkit::font(t, 12.0, 400.0, 18.0);
    let muted80 = t.text.muted.alpha(0.8);
    // Where things live, and the line under them (`Go to Folder … custom.css …`).
    let note = row().w_full().flex_wrap().px(12.0).pt(4.0).children([
        text("Go to Folder (⇧⌘G) in Finder opens a path. ".to_owned()).font(small).color(muted80).a11y_hidden(),
        text("custom.css".to_owned()).font(webkit::mono(webkit::font(t, 11.5, 400.0, 17.25))).color(muted80).a11y_hidden(),
        text(" applies as you save.".to_owned()).font(small).color(muted80).a11y_hidden(),
    ]);
    let places = col()
        .key("plugin-places")
        .w_full()
        .child(path_row(t, "plugin-folder", "Pages", custom.map(|c| c.plugins_dir.as_str()), "", "Folder path copied"))
        .child(path_row(t, "css-path", "Styles", custom.map(|c| c.css_path.as_str()), custom.map_or("", |c| if c.css_exists { "In use" } else { "Not created yet" }), "File path copied"))
        .child(note);
    let mut body = col().key("plugins-body").w_full().gap(32.0).child(webkit::panel(t, vec![places]));
    let change = |key: &str| {
        // The default `Button` as the web's Empty draws it: 40 high, 16 at the sides, 8 between the icon and the words.
        let r = row().key(key.to_owned()).role("button").label("Change Neko").focusable(true).h(40.0).px(17.0).gap(8.0).center().radius(24.0).focus_ring(2.0, 2.0, t.kit.c.border_focus).press_scale(0.97).on_press(|s: &mut App| s.ask_change_neko());
        webkit::ink(t, r).child(icon(&icons_phosphor::SPARKLE, 16.0).color(t.mat.ink_fg.alpha(0.8)).mx(-2.0).a11y_hidden()).child(text("Change Neko".to_owned()).font(webkit::font(t, 14.0, 500.0, 20.0)).color(t.mat.ink_fg).a11y_hidden())
    };
    match &ui.listings {
        None => {
            let line = ui.failed.clone();
            body = body.child(match line {
                Some(_) => webkit::empty_rich(
                    t,
                    "plugins-error",
                    &icons_phosphor::WARNING_REGULAR,
                    "Neko’s daemon did not answer",
                    text("The list could not be read. It tries again every few seconds.".to_owned()).font(webkit::font(t, 14.0, 400.0, 22.75)).color(t.text.muted).wrap().align(rune::Align::Center),
                    Vec::new(),
                    Some(webkit::button(t, "plugins-retry", "Try now", None, false, false, |s: &mut App| s.plugin_reload(None))),
                ),
                None => {
                    let bars: Vec<El> = [(192.0, 20.0), (0.0, 16.0), (160.0, 20.0), (0.0, 16.0)].iter().enumerate().map(|(i, (w, h))| { let b = el().key(format!("plugins-loading-{i}")).h(*h).radius(6.0).fill(t.text.base.alpha(0.06)); if *w > 0.0 { b.w(*w) } else { b.w_full() } }).collect();
                    webkit::panel(t, vec![col().key("plugins-loading").role("status").label("Loading customisations").w_full().px(12.0).py(12.0).gap(16.0).children(bars)])
                }
            });
        }
        Some(l) if l.is_empty() => {
            let style = webkit::font(t, 14.0, 400.0, 22.75);
            let code_style = webkit::mono(webkit::font(t, 12.0, 400.0, 18.0));
            let description = webkit::inline_flow(t, "plugins-none-description", &[("A page of yours is a folder with a ", false), ("manifest.json", true), (" and an ES module. Give it a built-in page’s id and it replaces that page. Describe what you want and Neko writes it for you.", false)], style, code_style, 384.0);
            let example = col().key("plugins-example").px(16.0).py(12.0).radius(10.0).fill(t.text.base.alpha(0.03)).child(text(MANIFEST_EXAMPLE.to_owned()).key("plugins-example-text").font(webkit::mono(webkit::font(t, 12.0, 400.0, 18.0))).color(t.text.muted));
            body = body.child(webkit::empty_rich(t, "plugins-none", &icons_phosphor::SLIDERS_HORIZONTAL, "Nothing customised yet", description, vec![example], Some(change("plugins-none-change"))));
        }
        Some(l) => {
            let rows: Vec<El> = l.iter().map(|p| plugin_row(s, t, layer, p)).collect();
            body = body.child(webkit::panel(t, vec![col().key("plugin-list").role("list").w_full().children(rows)]));
        }
    }
    // The native plugins (Luau) that installed packs ship, with their status (Rune's own, after the web's list).
    if let Some(native) = super::luau::plugins_group(s, t) {
        body = body.child(native);
    }
    if let Some(note) = &ui.note {
        body = body.child(text(note.clone()).key("plugins-note").font(t.ty.meta).color(t.meaning.amber).wrap().px(6.0).live_region(rune::LiveRegion::Polite));
    }
    if let Some(detail) = ui.detail.as_deref().and_then(|id| ui.listings.as_ref().and_then(|l| l.iter().find(|p| p.id == id))) {
        body = body.child(details(s, t, detail));
    }
    let foot_style = webkit::font(t, 12.5, 400.0, 18.75);
    let link_text = "Roll back the web UI in Settings.";
    let link = row()
        .key("plugins-roll-back")
        .role("button")
        .label(link_text)
        .focusable(true)
        .radius(4.0)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(|s: &mut App| {
            s.pages.section = crate::pages::Section::Updates;
            s.open_view(crate::views::FullView::Settings);
        })
        .child(rune::rich(link_text, vec![rune::Span::new(0, link_text.len()).underline(t.text.muted)]).font(foot_style).color(t.text.muted).a11y_hidden());
    body = body.child(row().w_full().flex_wrap().children([text("Changes Neko makes to its own source go live after they land, and the previous web UI is kept. ".to_owned()).font(foot_style).color(t.text.muted).wrap().shrink().min_w(0.0).a11y_hidden(), link]));
    let reloading = ui.reloading;
    let reload = row().key("plugins-reload-all").role("button").label("Reload all customisations").help("Reload all").focusable(true).size(36.0, 36.0).center().radius(18.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(webkit::gated(!reloading, |s: &mut App| s.plugin_reload(None))).child(icon(&icons_phosphor::ARROWS_CLOCKWISE, 16.0).color(t.text.base).a11y_hidden());
    let reload = if reloading { reload.opacity(0.64) } else { reload };
    let trailing = vec![row().gap(6.0).center_y().child(reload).child(webkit::button(t, "change-neko", "Change Neko", Some(&icons_phosphor::SPARKLE), true, false, |s: &mut App| s.ask_change_neko()))];
    page(t, "plugins", "Customisations", None, trailing, body)
}
