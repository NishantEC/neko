//! The sidebar (the prototype's `.side`, `~/Documents/neko-design/prototype-2026-11-21`): flat on the window's own colour, 46 above its
//! first row. Top to bottom: the brand row (the mark, "Neko", the workspace at the right), Home, Search ⌘K, Calendar, Tasks, then the Cats
//! section (up to three pinned faces in a row, the other cats as rows, the open cat's tasks nested under it), then pinned to the bottom
//! Rounds and Settings and the footer ("<name>" and what today has cost). What needs you is on Home and as the amber dot on a cat's face,
//! not in a list here; the account menu (Usage, Plugins, Pull requests, Stop all work, the web UI, Quit) opens from the footer.
//!
//! A nav row is 32 high with 10 of padding, an 8 gap, radius 8 and 13.5 point words in the second grey; the open one is the selected fill.

use rune::{Color, El, LoopSpec, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};

use super::look;
use super::webkit::{font, tnum};
use crate::app::App;
use crate::icons;
use crate::theme::Neko;
use crate::views::FullView;

/// The column's outer padding at the sides, and the room above its first row (the traffic lights sit in it).
pub const PAD: f32 = 10.0;
pub const TOP: f32 = 45.0;
/// A nav row.
pub const ROW_H: f32 = 32.0;

pub fn column(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    // Flat on the window's own colour: no card, no gradient, no shadow.
    let mut c = col().w_full().min_w(0.0).pt(TOP).px(PAD).pb(PAD).fill(t.surface.sidebar);
    c = c.child(brand(s, t, layer));
    // The top nav: Home, Search, Calendar, Tasks.
    let home_on = s.home_shown();
    let needs = s.needs_you_count();
    let cal_count = s.pages.calendar.as_ref().filter(|c| c.status.enabled).map(|c| s.events_today(c)).filter(|n| *n > 0);
    let mut nav = col().key("sidebar-nav").w_full().mt(4.0).mb(12.0);
    nav = nav.child(nav_row(t, "view-home", "Home", &icons::HOME, home_on, (needs > 0).then(|| (needs.to_string(), true)), |s: &mut App| s.go_home()));
    nav = nav.child(nav_row(t, "sessions-search", "Search", &icons::SEARCH, false, Some(("⌘K".to_owned(), false)), |s: &mut App| s.palette_toggle()).help("Search (⌘K)"));
    nav = nav.child(nav_row(t, "view-calendar", "Calendar", &icons::CALENDAR, s.view == Some(FullView::Calendar), cal_count.map(|n| (n.to_string(), false)), |s: &mut App| s.open_view(FullView::Calendar)));
    let tasks_on = s.view == Some(FullView::AllSessions) || (s.selected.is_some() && s.view.is_none() && s.cats.open.is_none());
    nav = nav.child(nav_row(t, "view-all", "Tasks", &icons::ISSUES, tasks_on, s.summary.as_ref().map(|m| m.tasks.iter().filter(|t| !matches!(t.status, neko_protocol::workbench::TaskStatus::Completed | neko_protocol::workbench::TaskStatus::Cancelled)).count()).filter(|n| *n > 0).map(|n| (n.to_string(), false)), |s: &mut App| s.open_all_sessions()));
    c = c.child(nav);
    // Cats: the section scrolls inside the free space, the bottom nav and the footer stay where they are.
    c = c.child(look::scroller("sessions-scroll", super::cats::section(s, t, layer)).scroll_edge_fade(12.0));
    // Before the first answer, and while Neko is not running: say which, calmly (the Neko column's foot says it too while it is open).
    if !matches!(s.link, crate::app::LinkState::Live) {
        c = c.child(text(crate::app::status_line(&s.link, s.summary.as_ref())).key("sessions-status").font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).px(10.0).py(6.0).wrap().live_region(rune::LiveRegion::Polite));
    }
    // The bottom nav: Rounds and Settings.
    let rounds = s.pages.routines.as_ref().map(|r| r.views.iter().filter(|v| v.routine.enabled && s.scope().is_none_or(|w| v.routine.workspace_id == w)).count()).filter(|n| *n > 0);
    let settings_on = matches!(s.view, Some(FullView::Settings | FullView::Setup | FullView::Plugins | FullView::Usage));
    let bottom = col().key("sidebar-bottom").w_full().mb(10.0).child(nav_row(t, "view-inbox", "Rounds", &icons::ROUND, s.view == Some(FullView::Routines), rounds.map(|n| (n.to_string(), false)), |s: &mut App| s.open_view(FullView::Routines))).child(nav_row(t, "view-settings", "Settings", &icons::GEAR, settings_on, Some(super::setup::attention(s)).filter(|n| *n > 0).map(|n| (n.to_string(), true)), |s: &mut App| s.open_view(FullView::Settings)));
    c = c.child(bottom);
    c.child(footer(s, t, layer))
}

/// The brand row: the mark (18, radius 5, the ink), "Neko" 13.5 semibold and, at the right, the workspace in scope with a chevron (the
/// switcher). Dragging it moves the window.
fn brand(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let name = s.scope_name();
    let trigger = row()
        .key("scope")
        .role("button")
        .label(format!("Workspace: {name}"))
        .help("Show one workspace or all of them")
        .expanded(s.scope_open)
        .focusable(true)
        .h(20.0)
        .px(4.0)
        .gap(3.0)
        .center_y()
        .radius(6.0)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.scope_open = !s.scope_open)
        .child(text(name.clone()).key("scope-name").font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).truncate().max_w(110.0).a11y_hidden())
        .child(icon(&icons::CHEVRON_DOWN, 12.0).color(t.text.faint).a11y_hidden());
    let mut menu = Menu::new("scope-menu").trigger(trigger).open(s.scope_open).placement(Placement::BottomEnd).min_width(220.0).on_close(|s: &mut App| s.scope_open = false);
    menu = menu.item(MenuItem::new("All workspaces").icon(&icons::FOLDER).checked(s.scope().is_none()).on_select(|s: &mut App| s.set_scope(None)));
    if let Some(summary) = s.summary.as_ref() {
        for w in &summary.workspaces {
            let id = w.id.clone();
            menu = menu.item(MenuItem::new(w.name.clone()).icon(&icons::FOLDER).checked(s.scope() == Some(w.id.as_str())).on_select(move |s: &mut App| s.set_scope(Some(id.clone()))));
        }
    }
    row()
        .key("sessions-header")
        .w_full()
        .px(8.0)
        .pb(12.0)
        .gap(8.0)
        .center_y()
        .window_drag()
        .child(icon(&icons::NEKO_MARK, 18.0).color(t.mat.ink_bottom).a11y_hidden())
        .child(text("Neko").key("sessions-brand").font(font(t, 13.5, 600.0, 18.0)).color(t.text.base).role("heading").a11y_hidden())
        .child(el().grow(1.0).min_w(0.0))
        .child(col().key("scope-row").child(menu.build(&t.kit, layer)))
}

/// A nav row: a 16 point glyph, the words, a count (or a hint) at the right. `amber` makes the count amber (what needs you).
pub fn nav_row(t: &Neko, key: &str, label: &str, glyph: &'static rune::Icon, on: bool, count: Option<(String, bool)>, on_press: impl Fn(&mut App) + 'static) -> El {
    let mut r = row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .checked(on)
        .focusable(true)
        .w_full()
        .h(ROW_H)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if on { t.surface.selected } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(on_press)
        .child(icon(glyph, 16.0).color(if on { t.text.base } else { t.text.muted.alpha(0.75) }).a11y_hidden())
        .child(text(label.to_owned()).font(font(t, 13.5, 400.0, 18.0)).color(if on { t.text.base } else { t.text.muted }).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if let Some((n, amber)) = count {
        r = r.child(text(n).key(format!("count-{key}")).font(tnum(font(t, 12.0, 400.0, 16.0))).color(if amber { t.meaning.amber_text } else { t.text.faint }).a11y_hidden());
    }
    r
}

/// The 16 point ring of a Running row: a 2 px track at the foreground's 12% and an Iris arc, `done` of
/// `of`; with no fraction a quarter-turn arc that rotates once every 1.2 s (still under Reduce Motion).
pub fn progress_ring(progress: Option<(u32, u32)>, t: &Neko) -> El {
    let track = icon(&icons::RING_TRACK, 16.0).absolute(0.0, 0.0).color(t.text.base.alpha(0.12));
    let arc = match progress {
        Some((done, of)) if of > 0 => icon(icons::ring_arc(done as f32 / of as f32), 16.0).absolute(0.0, 0.0).color(t.meaning.iris),
        _ => icon(&icons::RING_QUARTER, 16.0).absolute(0.0, 0.0).color(t.meaning.iris).looping(LoopSpec::rotation(1.2)),
    };
    el().size(16.0, 16.0).role("progressbar").label(match progress {
        Some((done, of)) => format!("{done} of {of}"),
        None => "Working".to_owned(),
    }).child(track).child(arc)
}


/// The footer (`.foot`): the account's avatar (22, the tint, the initial), the first name in the third grey at 12 points and, at the right,
/// what today has cost. It opens the account menu.
fn footer(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let name = if s.account.name.is_empty() { "You".to_owned() } else { s.account.name.clone() };
    let first = if s.account.first_name().is_empty() { name.clone() } else { s.account.first_name().to_owned() };
    let total = s.spend_today().map_or(String::new(), |v| format!("${v:.2} today"));
    let avatar = if s.account.has_picture() {
        el().key("account-avatar").size(22.0, 22.0).radius(11.0).pixels(&s.account.picture).a11y_hidden()
    } else {
        row().key("account-avatar").size(22.0, 22.0).radius(11.0).center().fill(t.surface.tint).a11y_hidden().child(text(s.account.initials()).font(font(t, 10.5, 600.0, 14.0)).color(t.text.muted))
    };
    let mut trigger = row()
        .key("account")
        .role("button")
        .label(format!("{name}, {total}. Menu"))
        .focusable(true)
        .w_full()
        .h(30.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if s.more_open { t.surface.selected } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.more_open = !s.more_open)
        .child(avatar)
        .child(text(first).font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if !total.is_empty() {
        trigger = trigger.child(text(total).key("account-spend").font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint).a11y_hidden());
    }
    let mut menu = Menu::new("account-menu").trigger(trigger).open(s.more_open).placement(Placement::TopStart).min_width(220.0).on_close(|s: &mut App| s.more_open = false);
    for view in FullView::MENU {
        menu = menu.item(MenuItem::new(view.title()).icon(view.icon()).checked(s.view == Some(view)).on_select(move |s: &mut App| {
            s.more_open = false;
            s.open_view(view);
        }));
    }
    // The pages plugins registered (plugin slot b), under their own heading.
    // The web's pages first, then the native views (Luau) that installed packs ship.
    let mut pages: Vec<neko_protocol::PluginPage> = s.plugin_pages.clone().unwrap_or_default();
    pages.extend(s.luau.views().iter().filter_map(|p| p.page()));
    if !pages.is_empty() {
        menu = menu.separator().group_label("Plugin pages");
        for page in &pages {
            let open = page.clone();
            menu = menu.item(MenuItem::new(page.title.clone()).icon(&icons::WRENCH).checked(s.plugin_view.as_ref().is_some_and(|p| p.id == page.id)).on_select(move |s: &mut App| {
                s.more_open = false;
                s.open_plugin_page(open.clone());
            }));
        }
    }
    menu = menu.separator().item(MenuItem::new("Stop all work").icon(&icons::CROSS).shortcut("⌘⇧Esc").on_select(|s: &mut App| {
        s.more_open = false;
        s.ask_stop_all();
    }));
    menu = menu.item(MenuItem::new("Open the web UI").icon(&icons::EXTERNAL).on_select(|s: &mut App| {
        s.more_open = false;
        s.open_in_web(None);
    }));
    menu = menu.item(MenuItem::new("Quit").icon(&icons::POWER).on_select_tx(|s: &mut App, tx: &mut rune::Tx| {
        s.more_open = false;
        tx.quit();
    }));
    col().key("sessions-footer").w_full().child(menu.build(&t.kit, layer))
}
