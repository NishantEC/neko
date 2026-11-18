//! The Browser tab: a live view of one page of a Chrome on this Mac (docs/knowledge/rune-live-browser.md). A 32 point top bar (the
//! page's title and host, a picker, "Open in your browser"), then the latest frame fitted to the panel's width, centred, in the
//! panel's own colour. Watching only: nothing here sends a key or a click to the page.

use rune::{El, SurfaceFit, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};

use super::{quiet_button, webkit};
use crate::app::App;
use crate::browser::tab::Seen;
use crate::browser::{CLOSED_NOTE_MS, Follow, PageInfo, Status};
use crate::icons;
use crate::theme::Neko;

/// A picker row's words, cut when long.
fn shorten(words: &str, max: usize) -> String {
    if words.chars().count() <= max {
        return words.to_owned();
    }
    let mut cut: String = words.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// A 32 point square quiet button with an icon.
fn square(t: &Neko, key: &str, label: &str, glyph: &'static rune::Icon) -> El {
    row().key(key.to_owned()).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(32.0, 32.0).center().radius(t.radius.control).hover(t.surface.hover).focus_ring(2.0, 2.0, t.kit.c.border_focus).child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

fn top_bar(s: &App, t: &Neko, layer: &mut Vec<El>, seen: &Seen) -> El {
    let (title, host) = match &seen.shown {
        Some(p) => (if p.title.trim().is_empty() { p.host() } else { p.title.trim().to_owned() }, p.host()),
        None => ("No page yet".to_owned(), String::new()),
    };
    let mut bar = row().key("browser-bar").w_full().h(32.0).px(12.0).center_y().child(icon(&icons::GLOBE, 16.0).color(t.text.muted).a11y_hidden());
    bar = bar.child(text(title).key("browser-title").font(webkit::font(t, 13.5, 400.0, 20.0)).color(t.text.base).truncate().min_w(0.0).ml(8.0));
    if !host.is_empty() {
        bar = bar.child(text(host).key("browser-host").font(webkit::font(t, 12.5, 400.0, 18.0)).color(t.text.muted).truncate().min_w(0.0).ml(8.0));
    }
    bar = bar.child(el().grow(1.0).min_w(0.0));
    // The picker: the newest first, then every page.
    let trigger = square(t, "browser-pick", "Choose a page", &icons::CHEVRON_DOWN).on_press(|s: &mut App| s.browser.menu_open = !s.browser.menu_open);
    let mut menu = Menu::new("browser-menu").trigger(trigger).open(s.browser.menu_open).placement(Placement::BottomEnd).min_width(260.0).on_close(|s: &mut App| s.browser.menu_open = false);
    menu = menu.item(MenuItem::new("Follow the newest page").checked(seen.follow == Follow::Newest).on_select(|s: &mut App| s.browser_follow(Follow::Newest)));
    if !seen.pages.is_empty() {
        menu = menu.separator();
    }
    for page in &seen.pages {
        let id = page.target_id.clone();
        menu = menu.item(MenuItem::new(shorten(&page.label(), 56)).checked(seen.follow == Follow::Page(page.target_id.clone())).on_select(move |s: &mut App| s.browser_follow(Follow::Page(id.clone()))));
    }
    bar = bar.child(menu.build(&t.kit, layer));
    let openable = seen.shown.as_ref().is_some_and(|p| p.openable().is_some());
    let mut open = square(t, "browser-open", "Open in your browser", &icons::EXTERNAL);
    if openable {
        open = open.on_press(|s: &mut App| s.browser_open_current());
    } else {
        open = open.disabled(true);
    }
    bar.child(open)
}

/// One thing in the middle of the panel's body.
fn centred(child: El) -> El {
    col().key("browser").w_full().grow(1.0).min_h(0.0).center().child(child)
}

fn line(t: &Neko, key: &str, words: &str, size: f32, faint: bool) -> El {
    text(words.to_owned()).key(key.to_owned()).font(webkit::font(t, size, 400.0, 18.0)).color(if faint { t.text.faint } else { t.text.muted })
}

/// The panel's body for the Browser tab.
pub fn tab(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let seen = s.browser.snapshot();
    match &seen.status {
        Status::Looking => return centred(line(t, "browser-looking", "Looking for a browser…", 12.5, false)),
        Status::Lost { .. } => return centred(line(t, "browser-lost", "Lost the browser. Trying again…", 12.5, false)),
        Status::None => {
            let again = quiet_button("Look again", "browser-again", t, |s: &mut App| s.browser.look_again());
            let body = col()
                .key("browser-none")
                .max_w(320.0)
                .px(16.0)
                .gap(8.0)
                .items_center()
                .child(text("No browser to watch").key("browser-none-title").font(webkit::font(t, 13.5, 500.0, 20.0)).color(t.text.base))
                .child(text("Neko watches a Chrome on this Mac that was started with --remote-debugging-port (it looks at ports 9333 and 9222).").key("browser-none-body").font(webkit::font(t, 12.5, 400.0, 18.0)).color(t.text.muted).wrap().align(rune::Align::Center))
                .child(el().h(4.0))
                .child(again);
            return centred(body);
        }
        Status::Connected => {}
    }
    let mut c = col().key("browser").w_full().grow(1.0).min_h(0.0).child(top_bar(s, t, layer, &seen));
    let closed = seen.closed_at_ms.is_some_and(|at| (s.clock)() - at < CLOSED_NOTE_MS);
    if closed {
        c = c.child(line(t, "browser-closed", "That page closed.", 12.0, true).px(12.0).h(20.0));
    } else if seen.follow == Follow::Newest {
        c = c.child(line(t, "browser-following", "Following the newest page", 12.0, true).px(12.0).h(20.0));
    }
    let area = col().key("browser-area").w_full().grow(1.0).min_h(0.0).px(12.0).pb(12.0).pt(4.0);
    let inner = match (&s.browser.size, &seen.shown) {
        (Some(_), Some(page)) => el().key("browser-frame").w_full().grow(1.0).min_h(0.0).radius(8.0).clip_rounded(8.0).pixels_fit(&s.browser.pixels, SurfaceFit::Fit).role("img").label(frame_label(page)),
        (None, Some(_)) => col().key("browser-waiting").w_full().grow(1.0).min_h(0.0).center().child(line(t, "browser-waiting-line", "Waiting for the page to draw…", 12.5, false)),
        _ => col().key("browser-empty").w_full().grow(1.0).min_h(0.0).center().child(line(t, "browser-empty-line", "No page is open in that browser.", 12.5, false)),
    };
    c.child(area.child(inner))
}

/// What a screen reader hears for the picture.
fn frame_label(page: &PageInfo) -> String {
    format!("Live view of {}", if page.title.trim().is_empty() { page.host() } else { page.title.trim().to_owned() })
}
