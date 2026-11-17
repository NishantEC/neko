//! Luau plugins on screen (`src/luau/`): the Cards section of the Neko column, a view's full page, and the status rows the pack detail
//! and the Plugins page show.
//!
//! **Cards** sit in the Neko column below the Digest as a "Cards" section: open by default, collapsible, at most three and "and N more"
//! to open the rest. A card is a 32 point header (icon, title, a right-click menu: Reload, Hide, Open pack) and a body up to 160 points
//! tall that scrolls. Calm rules (DESIGN.md §8): no card fill, no border; the separation is spacing.
//! **A view** replaces the session like a plugin page does: the page header (the title), then the plugin's content in the 880 point column.
//! **A plugin that is stopped** shows one quiet line, "<title> stopped: <reason>", and Reload; nothing else of the app is touched.

use rune::{El, ScrollSpec, col, icon, row, text};

use super::{quiet_button, ui};
use crate::app::App;
use crate::icons;
use crate::luau::{CARD_BODY_MAX, CARD_HEAD, FileRow, FileStatus, MAX_CARDS, Plugin, SLOW_RENDER_MS, Slot};
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;

/// The icon a plugin named, or the dot.
pub fn icon_named(name: Option<&str>) -> &'static rune::Icon {
    name.and_then(icons::by_name).unwrap_or(&icons::DOT)
}

/// The one quiet line of a plugin that stopped, with Reload.
fn stopped_line(t: &Neko, p: &Plugin, why: &str) -> El {
    let key = p.key();
    let words = format!("{} stopped: {why}", p.title());
    let k = key.clone();
    row()
        .key(format!("luau-stopped-{key}"))
        .w_full()
        .min_h(28.0)
        .gap(8.0)
        .center_y()
        .role("status")
        .child(text(words.clone()).key(format!("luau-stopped-text-{key}")).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).help(words))
        .child(quiet_button("Reload", &format!("luau-reload-{key}"), t, move |s: &mut App| s.luau_reload(&k)))
}

/// What a plugin draws now. A plugin that is stopped, errors while drawing, or takes too long to draw is one quiet line instead.
pub fn draw(s: &App, t: &Neko, p: &Plugin) -> El {
    if let Some(why) = p.stopped() {
        return stopped_line(t, p, &why);
    }
    let started = std::time::Instant::now();
    let drawn = p.script.render_uncached::<App>("render", t.dark);
    let took = started.elapsed().as_millis();
    if took > SLOW_RENDER_MS && p.failure.borrow().is_none() {
        *p.failure.borrow_mut() = Some(format!("it takes too long to draw ({took} ms)"));
    }
    // A render error comes back as rune-luau's own error card and is logged: show the quiet line, not that.
    match p.stopped() {
        Some(why) => stopped_line(t, p, &why),
        None => {
            let _ = s;
            drawn
        }
    }
}

fn card(s: &App, t: &Neko, layer: &mut Vec<El>, p: &Plugin) -> El {
    let key = p.key();
    let Some(meta) = p.meta() else { return col() };
    let menu_key = key.clone();
    let head = row()
        .key(format!("luau-card-head-{key}"))
        .w_full()
        .h(CARD_HEAD)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .role("heading")
        .label(meta.title.clone())
        .on_secondary_press(move |s: &mut App, at, _| s.luau.menu = Some((menu_key.clone(), at)))
        .child(icon(icon_named(meta.icon.as_deref()), 16.0).color(t.text.muted).a11y_hidden())
        .child(text(meta.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    let head = match s.luau.menu.as_ref().filter(|(open, _)| *open == key) {
        Some((_, at)) => {
            let (reload, hide, pack) = (key.clone(), (p.pack.clone(), meta.id.clone()), p.pack.clone());
            rune_kit::context_menu(format!("luau-card-menu-{key}"), head, Some(*at))
                .on_close(|s: &mut App| s.luau.menu = None)
                .item(rune_kit::MenuItem::new("Reload").icon(&icons::REFRESH).on_select(move |s: &mut App| {
                    s.luau.menu = None;
                    s.luau_reload(&reload);
                }))
                .item(rune_kit::MenuItem::new("Hide").icon(&icons::EYE).on_select(move |s: &mut App| s.luau_hide(&hide.0, &hide.1)))
                .item(rune_kit::MenuItem::new("Open pack").icon(&icons::EXTERNAL).on_select(move |s: &mut App| {
                    s.luau.menu = None;
                    s.open_pack_named(&pack);
                }))
                .build(&t.kit, layer)
        }
        None => head,
    };
    let body = col()
        .key(format!("luau-card-body-{key}"))
        .w_full()
        .max_h(CARD_BODY_MAX)
        .min_h(0.0)
        .px(8.0)
        .clip_rounded(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(draw(s, t, p));
    col().key(format!("luau-card-{key}")).w_full().gap(2.0).pb(8.0).child(head).child(body)
}

/// The "Cards" section of the Neko column: none when no pack has a card.
pub fn cards(s: &App, t: &Neko, layer: &mut Vec<El>) -> Option<El> {
    let cards = s.luau.cards();
    if cards.is_empty() {
        return None;
    }
    let open = !s.open("cards:closed");
    let mut c = col().key("cards").w_full().gap(2.0).child(super::neko::section_head(t, "cards-toggle", "Cards", Some(cards.len()), open, "cards:closed"));
    if !open {
        return Some(c);
    }
    let all = s.open("cards:all");
    let shown = if all { cards.len() } else { cards.len().min(MAX_CARDS) };
    for p in cards.iter().take(shown) {
        c = c.child(card(s, t, layer, p));
    }
    if cards.len() > MAX_CARDS {
        let label = if all { "Show fewer".to_owned() } else { format!("and {} more", cards.len() - MAX_CARDS) };
        c = c.child(
            row()
                .key("cards-more")
                .role("button")
                .label(label.clone())
                .focusable(true)
                .h(32.0)
                .px(8.0)
                .gap(8.0)
                .center_y()
                .radius(t.radius.row)
                .hover(t.surface.hover)
                .focus_ring(2.0, 2.0, t.surface.ring)
                .on_press(|s: &mut App| s.toggle("cards:all"))
                .child(rune::el().size(16.0, 16.0).a11y_hidden())
                .child(text(label).font(t.ty.meta).color(t.text.muted).a11y_hidden()),
        );
    }
    Some(c)
}

/// A view's page: the page header (the title), then the plugin's content in the 880 point column.
pub fn page(s: &App, t: &Neko, page: &neko_protocol::PluginPage) -> El {
    let body = match s.luau.view_for(page) {
        Some(p) => col().key("luau-view-body").w_full().child(draw(s, t, p)),
        None => ui::quiet_line(t, "luau-view-gone", "This view isn’t running. Its pack may have been removed, or it changed on disk and waits for your review in Setup, Packs."),
    };
    ui::page(t, "luau-view", &page.title, None, Vec::new(), body)
}

/// One file's row: mint check or coral cross, its title and path, and why it stopped.
pub fn file_row(t: &Neko, row_data: &FileRow, with_pack: bool) -> El {
    let key = format!("luau-file-{}-{}", row_data.pack, row_data.path.replace('/', "-"));
    let (kind, word) = match &row_data.status {
        FileStatus::Loaded => (StatusKind::Done, "Loaded".to_owned()),
        FileStatus::Stopped(why) => (StatusKind::Failed, format!("Stopped: {why}")),
    };
    let slot = match row_data.slot {
        Some(Slot::Card) => "card",
        Some(Slot::View) => "view",
        None => "",
    };
    let mut line = row().key(key.clone()).w_full().min_h(28.0).px(8.0).gap(10.0).center_y().child(status_glyph(kind, t, 14.0, Some(&word)));
    if with_pack {
        line = line.child(text(row_data.pack.clone()).font(t.ty.meta).color(t.text.faint));
    }
    line = line.child(text(row_data.title.clone()).key(format!("{key}-title")).font(t.ty.row).color(t.text.base).truncate().min_w(0.0));
    if !slot.is_empty() {
        line = line.child(text(slot.to_owned()).font(t.ty.meta).color(t.text.faint));
    }
    line = line.child(text(row_data.path.clone()).font(t.ty.mono_meta).color(t.text.faint).truncate().min_w(0.0));
    let tone = if matches!(row_data.status, FileStatus::Loaded) { t.text.muted } else { t.meaning.coral };
    line.child(text(word).key(format!("{key}-status")).font(t.ty.meta).color(tone).truncate().grow(1.0).min_w(0.0))
}

/// The pack detail's "Native panels": every Luau file with its status, and Reload.
pub fn pack_files(s: &App, t: &Neko, pack: &str) -> Option<El> {
    let rows: Vec<FileRow> = s.luau.rows().into_iter().filter(|r| r.pack == pack).collect();
    let note = s.luau.packs.get(pack);
    if rows.is_empty() && note.is_none() {
        return None;
    }
    let mut c = col().key("pack-luau").w_full().gap(2.0);
    for r in &rows {
        c = c.child(file_row(t, r, false));
    }
    let pack_name = pack.to_owned();
    Some(c.child(row().w_full().px(6.0).pt(4.0).child(quiet_button("Reload native panels", "pack-luau-reload", t, move |s: &mut App| s.luau_reload_pack(&pack_name)))))
}

/// The Plugins page's "Native plugins" group: every pack's Luau files, each view with Open, and the hidden cards.
pub fn plugins_group(s: &App, t: &Neko) -> Option<El> {
    let rows = s.luau.rows();
    if rows.is_empty() && s.luau.hidden.is_empty() {
        return None;
    }
    let mut c = col().key("luau-plugins").w_full().gap(2.0).child(ui::group(t, "Native plugins (Luau)"));
    for r in &rows {
        let mut line = file_row(t, r, true);
        if let Some(page) = s.luau.plugin(&crate::luau::plugin_key(&r.pack, &r.path)).and_then(Plugin::page) {
            let key = format!("luau-open-{}-{}", r.pack, r.path.replace('/', "-"));
            line = line.child(quiet_button("Open", &key, t, move |s: &mut App| s.open_luau_view(page.clone())));
        }
        c = c.child(line);
    }
    if !s.luau.hidden.is_empty() {
        c = c.child(row().key("luau-hidden").w_full().px(8.0).pt(4.0).gap(8.0).center_y().child(text(format!("{} hidden card{}", s.luau.hidden.len(), if s.luau.hidden.len() == 1 { "" } else { "s" })).font(t.ty.meta).color(t.text.muted)).child(quiet_button("Show all", "luau-show-all", t, |s: &mut App| s.luau_show_all())));
    }
    Some(c)
}
