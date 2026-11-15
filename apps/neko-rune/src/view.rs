//! The app shell (DESIGN.md §1, §8): one window with a tab strip across the titlebar (back,
//! forward, tabs, `+`), the rail of six places with "More", and the page. Every page is a
//! placeholder that says what is coming and offers the web UI at the same route, so nobody is
//! stuck while a screen has no native version yet. ⌘K opens the palette stub.
//!
//! Colours, sizes and fonts come from `theme.rs`; this file names none.

use rune::{Cx, El, Key, ScrollSpec, col, el, icon, row, text};
use rune_kit::shell::{TabItem, TabOrientation, TabOverflow, TabStrip, WindowTitlebar};
use rune_kit::{ControlSize, Dialog, IconButton, IconButtonVariant, Menu, MenuItem, TextInput, root};

use crate::app::{App, Entry, entries, status_line};
use crate::lab::lab;
use crate::pages::{self, Page};
use crate::theme::{Neko, theme};

pub const MIN_TAB: f32 = 84.0;
pub const MAX_TAB: f32 = 200.0;
const TAB_GAP: f32 = 6.0;
/// The `+` after the last tab, with the gap before it.
const TRAILING: f32 = 34.0;
/// One column for list and setup pages (DESIGN.md §8 rule 8).
pub const COLUMN: f32 = 880.0;
pub const RAIL_WIDTH: f32 = 56.0;

/// The width every tab gets: wide enough for the longest title, between 84 and 200 points.
pub fn fit_tab_width<'a>(titles: impl IntoIterator<Item = &'a str>) -> f32 {
    let longest = titles.into_iter().map(|t| t.chars().count()).max().unwrap_or(0);
    // The icon, its gap, the close slot, the fade and the padding are about 84 points; Inter at
    // 13.5 medium is about 7.4 points a character.
    (longest as f32 * 7.4 + 84.0).clamp(MIN_TAB, MAX_TAB)
}

/// How wide the strip should say it is, so tabs fit their titles instead of sharing the whole
/// titlebar: what they need, but never more than there is.
pub fn strip_width(tabs: usize, tab: f32, available: f32) -> f32 {
    (tabs as f32 * tab + tabs.saturating_sub(1) as f32 * TAB_GAP + TRAILING).min(available)
}

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let t = theme(cx.system_dark());
    let mut layer = Vec::new();
    let bar = titlebar(s, cx, &t, &mut layer);
    let rail = rail(s, &t, &mut layer);
    let card = card(s, &t);
    palette(s, &t, &mut layer);
    root(
        col().key("window").w_full().h_full().fill(t.surface.sidebar).child(bar).child(row().key("body").w_full().grow(1.0).min_h(0.0).child(rail).child(card)),
        layer,
    )
}

fn titlebar(s: &App, cx: &mut Cx<App>, t: &Neko, layer: &mut Vec<El>) -> El {
    let k = &t.kit;
    let nav = |glyph, label: &str, on: bool| -> IconButton {
        IconButton::new(glyph).control_size(ControlSize::Large).variant(IconButtonVariant::GhostMuted).label(label).disabled(!on)
    };
    let back = nav(&pages::ARROW_LEFT, "Back", s.tabs.can_back()).shortcut("⌘[").on_press(|s: &mut App| s.tabs.back()).build(k).key("back");
    let forward = nav(&pages::ARROW_RIGHT, "Forward", s.tabs.can_forward()).shortcut("⌘]").on_press(|s: &mut App| s.tabs.forward()).build(k).key("forward");
    let plus = IconButton::new(&pages::PLUS)
        .control_size(ControlSize::Large)
        .variant(IconButtonVariant::GhostMuted)
        .label("New tab")
        .shortcut("⌘K")
        .on_press(|s: &mut App| s.open_palette())
        .build(k)
        .key("new-tab");

    let pages = s.tabs.open_pages();
    let active = s.tabs.active();
    let tab = fit_tab_width(pages.iter().map(|p| p.title()));
    let available = cx.layout_size("tabs").map(|z| z[0]);
    // Before the first layout there is no width to read: take what the tabs need.
    let width = Some(strip_width(pages.len(), tab, available.unwrap_or(f32::INFINITY)));
    let items = pages.iter().map(|p| TabItem::new(p.id(), p.title()).icon(p.icon()).closable(*p != Page::Home));
    let strip = TabStrip::new("tabs", TabOrientation::Horizontal)
        .tabs(items)
        .active(Some(active.id()))
        .fill(t.surface.sidebar)
        .viewport_width(width)
        .overflow(TabOverflow::ShrinkThenScroll { min_width: MIN_TAB, max_width: MAX_TAB, pinned_width: 36.0, fade_width: 24.0 })
        .drag_state(&s.tab_drag, |s: &mut App| &mut s.tab_drag)
        .close_icon(&pages::CLOSE_SMALL)
        .label("Open places")
        .trailing_action(plus)
        .on_select(|s: &mut App, id, _| {
            if let Some(page) = Page::from_id(id) {
                s.tabs.select(page);
                s.notice = None;
            }
        })
        .on_close(|s: &mut App, id, _| {
            if let Some(page) = Page::from_id(id) {
                s.tabs.close(page);
            }
        })
        .on_reorder(|s: &mut App, r, _| s.tabs.reorder(r.from, r.to))
        .build_layered(k, layer);
    WindowTitlebar::from_insets(cx.chrome_insets())
        .fill(t.surface.sidebar)
        .leading(row().key("nav").gap(2.0).center_y().child(back).child(forward))
        .tabs(strip)
        .build(k)
}

/// One place on the rail: 36 points square, the active one a soft fill, named by a tooltip.
fn rail_button(page: Page, active: bool, badge: usize, t: &Neko) -> El {
    let ink = if active { t.text.base } else { t.text.muted };
    let mut b = row()
        .key(format!("rail-{}", page.id()))
        .role("button")
        .label(page.title())
        .help(page.title())
        .checked(active)
        .focusable(true)
        .size(36.0, 36.0)
        .center()
        .radius(t.radius.row)
        .fill(if active { t.surface.active } else { rune::Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.open(page))
        .child(icon(page.icon(), 18.0).color(ink).a11y_hidden());
    if badge > 0 {
        // The one badge on the rail: what waits on you (DESIGN.md §8 rule 5's neighbour, the
        // Tasks count).
        let label = if badge > 99 { "99+".to_owned() } else { badge.to_string() };
        b = b.child(
            row().key("badge").absolute(20.0, 1.0).h(16.0).min_w(16.0).px(4.0).radius(8.0).center().fill(t.meaning.amber).a11y_hidden().child(
                text(label).font(rune::TextStyle { weight: 500.0, ..t.ty.id }).color(t.surface.window),
            ),
        );
    }
    b
}

fn rail(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let active = s.tabs.active();
    let waiting = s.summary.as_ref().map_or(0, |m| m.needs_you());
    let mut top = col().key("rail-places").gap(4.0).items_center();
    for page in Page::RAIL {
        let badge = if page == Page::Tasks { waiting } else { 0 };
        top = top.child(rail_button(page, active == page, badge, t));
    }
    let more_trigger = row()
        .key("rail-more")
        .role("button")
        .label("More")
        .help("More")
        .focusable(true)
        .size(36.0, 36.0)
        .center()
        .radius(t.radius.row)
        .fill(if s.more_open { t.surface.active } else { rune::Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(|s: &mut App| s.more_open = !s.more_open)
        .child(icon(&pages::MORE, 18.0).color(t.text.muted).a11y_hidden());
    let mut menu = Menu::new("more").trigger(more_trigger).open(s.more_open).placement(rune_kit::Placement::RightStart).on_close(|s: &mut App| s.more_open = false);
    for page in Page::MORE {
        menu = menu.item(MenuItem::new(page.title()).icon(page.icon()).on_select(move |s: &mut App| s.open(page)));
    }
    let more = menu.build(&t.kit, layer);
    top = top.child(more);
    col()
        .key("rail")
        .w(RAIL_WIDTH)
        .h_full()
        .pt(4.0)
        .pb(10.0)
        .items_center()
        .child(top)
        .child(el().grow(1.0))
        .child(rail_button(Page::FOOT, active == Page::FOOT, 0, t))
}

/// The framed page, inset from the window's edge and the rail with the window's large radius.
fn card(s: &App, t: &Neko) -> El {
    let page = s.tabs.active();
    let status = status_line(&s.link, s.summary.as_ref());
    col()
        .key("card")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .mr(8.0)
        .mb(8.0)
        .radius(t.radius.window)
        .fill(t.surface.window)
        .child(row().key("page-scroll").w_full().grow(1.0).min_h(0.0).justify_center().child(if s.lab { lab(t) } else { placeholder(s, page, t) }))
        .child(row().key("status").w_full().justify_center().pb(14.0).child(
            row().w_full().max_w(COLUMN).px(32.0).child(text(status).font(t.ty.meta).color(t.text.faint).truncate().live_region(rune::LiveRegion::Polite).key("status-text")),
        ))
}

/// The page before its native screen exists: the title, one calm line, one way out.
fn placeholder(s: &App, page: Page, t: &Neko) -> El {
    let mut c = col()
        .key(format!("page-{}", page.id()))
        .w_full()
        .max_w(COLUMN)
        .px(32.0)
        .pt(40.0)
        .gap(14.0)
        .items_start()
        .child(text(page.title()).key("title").font(t.ty.title).color(t.text.base).role("heading"))
        .child(text(format!("Coming in {}: open in the web UI", page.phase().name())).key("coming").font(t.ty.row).color(t.text.muted))
        .child(primary_button("Open in the web UI", "open-web", t, move |s: &mut App| s.open_in_web(page)));
    if let Some(notice) = &s.notice {
        c = c.child(text(notice.clone()).key("notice").font(t.ty.meta).color(t.text.muted).live_region(rune::LiveRegion::Polite));
    }
    c
}

/// The one primary action on a page: Iris, the colour of "do this".
fn primary_button(label: &str, key: &str, t: &Neko, on_press: impl Fn(&mut App) + 'static) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label)
        .focusable(true)
        .h(28.0)
        .px(12.0)
        .radius(t.radius.control)
        .center()
        .fill(t.meaning.iris)
        .hover(rune::Color::WHITE.alpha(0.14))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(on_press)
        .child(text(label.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(rune::Color::WHITE).a11y_hidden())
}

/// ⌘K: go to a page. The stub that P4 replaces with the full palette.
fn palette(s: &App, t: &Neko, layer: &mut Vec<El>) {
    if !s.palette.open {
        return;
    }
    let k = &t.kit;
    let rows = entries(&s.palette.query.text);
    let input = TextInput::new(&s.palette.query)
        .placeholder("Go to a page…")
        .label("Go to a page")
        .leading_icon(&pages::SEARCH)
        .on_edit(|s: &mut App| &mut s.palette.query)
        .on_change(|s: &mut App, _| s.query_changed())
        .on_key(|s: &mut App, ev, _| match ev.key {
            Key::Down => {
                s.move_selection(1);
                true
            }
            Key::Up => {
                s.move_selection(-1);
                true
            }
            Key::Enter => {
                if let Some(entry) = entries(&s.palette.query.text).get(s.palette.selected).copied() {
                    s.choose(entry);
                }
                true
            }
            _ => false,
        })
        .build(k)
        .key("palette-input");
    let mut list = col().key("palette-rows").w_full().gap(2.0).role("listbox").label("Pages");
    if rows.is_empty() {
        list = list.child(text("Nothing matches").font(t.ty.row).color(t.text.muted).px(10.0).py(8.0));
    }
    for (i, entry) in rows.iter().copied().enumerate() {
        let selected = i == s.palette.selected;
        let (lead, name) = match entry {
            Entry::GoTo(page) => (page.icon(), entry.title()),
            Entry::OpenInWeb(page) => (page.icon(), entry.title()),
        };
        list = list.child(
            row()
                .key(format!("palette-row-{i}"))
                .role("option")
                .label(name.clone())
                .checked(selected)
                .focusable(true)
                .w_full()
                .h(32.0)
                .px(10.0)
                .gap(10.0)
                .center_y()
                .radius(t.radius.row)
                .fill(if selected { t.surface.active } else { rune::Color::TRANSPARENT })
                .hover(t.surface.hover)
                .on_press(move |s: &mut App| s.choose(entry))
                .child(icon(lead, 16.0).color(t.text.muted).a11y_hidden())
                .child(text(name).font(t.ty.row).color(t.text.base).truncate().a11y_hidden()),
        );
    }
    let body = col()
        .key("palette-body")
        .w_full()
        .h_full()
        .min_h(0.0)
        .px(12.0)
        .pb(12.0)
        .gap(8.0)
        .child(input)
        .child(col().key("palette-scroll").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
    Dialog::new("palette", "Go to").open(true).width(560.0).child(body).on_close(|s: &mut App| s.close_palette()).build(k, layer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_fit_their_titles_between_84_and_200() {
        assert_eq!(fit_tab_width([""]), MIN_TAB, "an empty title does not make a tiny tab");
        assert_eq!(fit_tab_width(["a title far too long to ever fit in a tab strip, really"]), MAX_TAB);
        let mid = fit_tab_width(["Pull requests"]);
        assert!(mid > MIN_TAB && mid < MAX_TAB, "{mid}");
        assert!(fit_tab_width(["Home", "Usage & machine"]) > fit_tab_width(["Home"]), "the longest title decides");
    }

    #[test]
    fn the_strip_takes_what_the_tabs_need_and_no_more() {
        assert_eq!(strip_width(1, 84.0, 1000.0), 84.0 + TRAILING);
        assert_eq!(strip_width(3, 100.0, 1000.0), 300.0 + 2.0 * TAB_GAP + TRAILING);
        assert_eq!(strip_width(20, 100.0, 500.0), 500.0, "past the window it scrolls");
        assert_eq!(strip_width(0, 100.0, 500.0), TRAILING);
    }
}
