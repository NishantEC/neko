//! Cats on screen (docs/knowledge/cats.md, "Rune: slice 1"), after the approved prototype
//! (`~/Documents/neko-design/prototype-2026-11-21`, `?p=cat&c=miso`): the sidebar's Cats section (a header with a plus, up to three
//! pinned cats as faces, the rest as rows, "No cats yet" when there are none), the New cat sheet, the cat page (a header, a quiet
//! line where the conversation will be) and its panel with the Overview and Files tabs, and the file editor.
//!
//! Faces are `cat_face.rs`. Colour appears only for what needs you (amber) and what runs (the working mark); everything else is the
//! window's own greys on its 4/8 rhythm.

use neko_protocol::cats::{Cat, CatFileKind, MAX_CAT_FILE};
use neko_protocol::workbench::TaskStatus;
use rune::{Color, El, Phase, ScrollSpec, col, el, icon, row, text};
use rune_kit::{Dialog, Menu, MenuItem, Placement, context_menu};

use super::setup::kit::{field, text_area, text_field};
use super::sessions::{PAD, ROW_H};
use super::ui::{edit_of, editing, pick};
use super::webkit::{self, font, tnum};
use super::{band_px, icon_button, look, placeholder, primary_button, quiet_button};
use crate::app::App;
use crate::cat_face::{self, Marks};
use crate::cats::{self, CatTab, F_AREA, F_FILE, F_LIMIT, F_NAME, F_PURPOSE, file_purpose, file_title};
use crate::icons;
use crate::theme::Neko;

/// The sidebar's cat rows: 36 high like every row, a 20 point face.
const FACE_ROW: f32 = 20.0;
/// The faces of the pinned row.
const FACE_BIG: f32 = 44.0;

// ---- the sidebar --------------------------------------------------------------------------------

/// The Cats section of the sidebar (the prototype's `.ghead`, `.pinned`, `.caphead`, `.kids`): a header with a plus, up to three pinned cats as
/// faces in one row, the other cats as rows, and the open cat's tasks nested under it. "No cats yet" when there are none.
pub fn section(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(list) = s.cats.list.as_ref() else { return el().key("cats-section").w(0.0).h(0.0) };
    let mut c = col().key("cats-section").role("list").label("Cats").w_full();
    c = c.child(header(t));
    if list.is_empty() {
        let empty = col()
            .key("cats-empty")
            .w_full()
            .px(10.0)
            .pt(4.0)
            .pb(8.0)
            .gap(8.0)
            .child(text("No cats yet").key("cats-empty-line").font(t.ty.meta).color(t.text.faint))
            .child(row().child(quiet_button("New cat", "cats-empty-new", t, |s: &mut App| s.open_new_cat())));
        return c.child(empty);
    }
    let (faces, rows) = cats::display(list);
    let current = s.sidebar_cat().map(|c| c.id.clone());
    if !faces.is_empty() {
        // Three cells share the width, so one or two pinned cats keep the size of three.
        let mut line = row().key("cats-pinned").w_full().gap(4.0).pt(5.0).pb(8.0);
        for cat in &faces {
            line = line.child(pinned(s, t, cat, layer));
        }
        for i in faces.len()..cats::PINNED_SHOWN {
            line = line.child(el().key(format!("cats-pinned-gap-{i}")).grow(1.0).min_w(0.0).h(1.0));
        }
        c = c.child(line);
        // A pinned cat's name, area and tasks sit under the faces; an unpinned one has its own row below.
        if let Some(cat) = faces.iter().find(|f| Some(&f.id) == current.as_ref()) {
            c = c.child(caphead(s, t, cat)).child(kids(s, t, cat, layer));
        }
    }
    for cat in &rows {
        c = c.child(cat_row(s, t, cat, layer));
        if Some(&cat.id) == current.as_ref() {
            c = c.child(kids(s, t, cat, layer));
        }
    }
    c
}

/// "Cats" and the plus: a 26 point line in the third grey (11.5 medium), the plus a small quiet button at the right.
fn header(t: &Neko) -> El {
    row()
        .key("cats-header")
        .w_full()
        .h(26.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .child(text("Cats").key("cats-title").font(font(t, 11.5, 500.0, 16.0)).color(t.text.faint).role("heading").grow(1.0).min_w(0.0))
        .child(row().key("cats-new").role("button").label("New cat").help("New cat").focusable(true).size(20.0, 20.0).center().radius(5.0).hover(t.surface.hover).focus_ring(2.0, 2.0, t.surface.ring).on_press(|s: &mut App| s.open_new_cat()).child(icon(&icons::PLUS, 14.0).color(t.text.faint).a11y_hidden()))
}

fn is_open(s: &App, id: &str) -> bool {
    s.cats.open.as_deref() == Some(id)
}

/// How far a drag has carried this cat, for its look while it moves.
fn carried(s: &App, id: &str) -> bool {
    s.cats.carry.as_ref().is_some_and(|c| c.moved && c.id == id)
}

/// The cell of a pinned cat (`.pin`): its face at 44 and its name under it (12.5, the second grey), the open one with a hairline round it.
fn pinned(s: &App, t: &Neko, cat: &Cat, layer: &mut Vec<El>) -> El {
    let open = is_open(s, &cat.id);
    let marks = s.cat_marks(&cat.id);
    let id = cat.id.clone();
    let (press, drag, menu) = (id.clone(), id.clone(), id.clone());
    // One cell is a third of the column's inner width, which is what a drag along the row steps by.
    let cell = ((s.layout.sessions.width() - 2.0 * PAD - 8.0) / 3.0).max(40.0);
    let r = col()
        .key(format!("cat-{}", cat.id))
        .role("button")
        .label(row_label(cat, marks))
        .help(format!("{}{}", cat.name, if cat.area.is_empty() { String::new() } else { format!(" · {}", cat.area) }))
        .checked(open)
        .focusable(true)
        .grow(1.0)
        .min_w(0.0)
        .items_center()
        .gap(6.0)
        .pt(8.0)
        .pb(6.0)
        .radius(10.0)
        .fill(if open { t.surface.hover } else { Color::TRANSPARENT })
        .border(1.0, if open { t.surface.line_strong } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .opacity(if carried(s, &cat.id) { 0.4 } else { 1.0 })
        .on_press(move |s: &mut App| {
            if std::mem::take(&mut s.cats.suppress_press) {
                return;
            }
            s.open_cat(&press);
        })
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _| s.cat_drag(&drag, d.phase, (d.dx, d.dy), cell, true, d.phase == Phase::Up && d.cancelled()))
        .on_secondary_press(move |s: &mut App, p, _| s.cats.row_menu = Some((menu.clone(), p)))
        .child(cat_face::face(&cat.face.0, FACE_BIG, marks, t.surface.sidebar, t))
        .child(text(cat.name.clone()).key(format!("cat-name-{}", cat.id)).font(font(t, 12.5, 400.0, 18.0)).color(if open { t.text.base } else { t.text.muted }).truncate().max_w(cell - 8.0).a11y_hidden());
    with_menu(s, t, cat, r, layer)
}

/// The open cat's line under the faces (`.caphead`): its 18 point face, its name (13 medium), its area (12, the third grey).
fn caphead(s: &App, t: &Neko, cat: &Cat) -> El {
    let marks = s.cat_marks(&cat.id);
    let mut r = row().key(format!("cat-head-{}", cat.id)).w_full().h(30.0).px(10.0).gap(8.0).center_y().child(cat_face::face(&cat.face.0, 18.0, marks, t.surface.sidebar, t)).child(text(cat.name.clone()).font(font(t, 13.0, 500.0, 18.0)).color(t.text.base).a11y_hidden());
    if !cat.area.is_empty() {
        r = r.child(text(cat.area.clone()).key(format!("cat-area-{}", cat.id)).font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).truncate().min_w(0.0).a11y_hidden());
    }
    r
}

/// The open cat's tasks nested under its line (`.kids`, `.srow`): a hairline at 19 from the left, then 30 point rows of a glyph and a title,
/// waiting first, then working, ready, queued and the day's last finished one; at most six.
fn kids(s: &App, t: &Neko, cat: &Cat, layer: &mut Vec<El>) -> El {
    use neko_protocol::workbench::TaskStatus as St;
    let rank = |st: St| match st {
        St::AwaitingApproval | St::Failed => 0,
        St::Planning | St::Building | St::Reviewing => 1,
        St::ReadyForReview => 2,
        St::Queued => 3,
        _ => 4,
    };
    let mut tasks = s.cat_tasks(&cat.id, |_| true);
    let finished = tasks.iter().filter(|t| rank(t.status) == 4).count();
    // Finished work is a footnote: only the newest of it shows, and only when the cat has little else.
    let mut kept: Vec<&crate::link::TaskRow> = tasks.iter().copied().filter(|t| rank(t.status) < 4).collect();
    if finished > 0 && kept.len() < 5 {
        kept.extend(tasks.iter().copied().filter(|t| rank(t.status) == 4).take(1));
    }
    tasks = kept;
    tasks.sort_by_key(|t| rank(t.status));
    let mut c = col().key(format!("cat-kids-{}", cat.id)).ml(19.0).mb(4.0).pl(6.0).border_sides([0.0, 0.0, 0.0, 1.0], t.surface.line_strong);
    for task in tasks.into_iter().take(6) {
        let id = task.id.clone();
        let on = s.selected.as_deref() == Some(task.id.as_str()) && s.view.is_none();
        let menu_id = task.id.clone();
        let line = row()
                .key(format!("session-{}", task.id))
                .role("button")
                .label(format!("{}, {}{}", task.title, crate::groups::status_word(task.status), if s.unseen.contains(&task.id) { ", not yet seen" } else { "" }))
                .help(format!("{} · {}", crate::status::status_label(task.status), crate::groups::age(s.now_ms, task.updated_at_ms)))
                .checked(on)
                .focusable(true)
                .w_full()
                .h(30.0)
                .pl(8.0)
                .pr(10.0)
                .gap(9.0)
                .center_y()
                .radius(t.radius.row)
                .fill(if on { t.surface.selected } else { Color::TRANSPARENT })
                .hover(t.surface.hover)
                .focus_ring(2.0, 2.0, t.surface.ring)
                .on_press(move |s: &mut App| s.select(&id))
                .on_secondary_press(move |s: &mut App, p, _| s.row_menu = Some((menu_id.clone(), p)))
                .child(look::task_glyph(t, task))
                .child(text(task.title.clone()).font(font(t, 13.0, 400.0, 18.0)).color(if on { t.text.base } else { t.text.muted }).truncate().grow(1.0).min_w(0.0).a11y_hidden());
        // Iris, 6 points: what changed since you looked.
        let line = if s.unseen.contains(&task.id) { line.child(el().key(format!("unseen-{}", task.id)).size(6.0, 6.0).radius(3.0).fill(t.meaning.iris).a11y_hidden()) } else { line };
        // The same right-click menu as the task's own "..." (`task_actions.rs`): one list, decided once.
        c = c.child(match s.row_menu.as_ref().filter(|(open, _)| *open == task.id) {
            Some((_, at)) => {
                let menu = context_menu(format!("row-menu-{}", task.id), line, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
                crate::task_actions::fill_menu(menu, s, &task.id).build(&t.kit, layer)
            }
            None => line,
        });
    }
    c
}

/// One of the other cats as a row (`.srow`): a 20 point face, its name, its area quietly at the right.
fn cat_row(s: &App, t: &Neko, cat: &Cat, layer: &mut Vec<El>) -> El {
    let open = is_open(s, &cat.id);
    let marks = s.cat_marks(&cat.id);
    let id = cat.id.clone();
    let (press, drag, menu) = (id.clone(), id.clone(), id.clone());
    let mut r = row()
        .key(format!("cat-{}", cat.id))
        .role("button")
        .label(row_label(cat, marks))
        .checked(open)
        .focusable(true)
        .w_full()
        .h(30.0)
        .px(10.0)
        .gap(9.0)
        .center_y()
        .radius(t.radius.row)
        .fill(if open { t.surface.selected } else { Color::TRANSPARENT })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .opacity(if carried(s, &cat.id) { 0.4 } else { 1.0 })
        .on_press(move |s: &mut App| {
            if std::mem::take(&mut s.cats.suppress_press) {
                return;
            }
            s.open_cat(&press);
        })
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _| s.cat_drag(&drag, d.phase, (d.dx, d.dy), ROW_H, false, d.phase == Phase::Up && d.cancelled()))
        .on_secondary_press(move |s: &mut App, p, _| s.cats.row_menu = Some((menu.clone(), p)))
        .child(cat_face::face(&cat.face.0, FACE_ROW, marks, t.surface.sidebar, t))
        .child(text(cat.name.clone()).key(format!("cat-name-{}", cat.id)).font(font(t, 13.0, 500.0, 18.0)).color(if open { t.text.base } else { t.text.muted }).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if !cat.area.is_empty() {
        r = r.child(text(cat.area.clone()).key(format!("cat-area-{}", cat.id)).font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).truncate().max_w(80.0).a11y_hidden());
    }
    with_menu(s, t, cat, r, layer)
}

fn row_label(cat: &Cat, marks: Marks) -> String {
    let mut words = cat.name.clone();
    if !cat.area.is_empty() {
        words += &format!(", {}", cat.area);
    }
    if marks.needs_you {
        words += ", needs you";
    } else if marks.working {
        words += ", working";
    }
    words
}

/// The right-click menu of a sidebar cat (every drag has a menu item too): Open, Edit, Pin or Unpin, Move up, Move down, Archive.
fn with_menu(s: &App, t: &Neko, cat: &Cat, r: El, layer: &mut Vec<El>) -> El {
    let Some((_, at)) = s.cats.row_menu.as_ref().filter(|(open, _)| *open == cat.id) else { return r };
    let menu = context_menu(format!("cat-menu-{}", cat.id), r, Some(*at)).on_close(|s: &mut App| s.cats.row_menu = None);
    cat_menu_items(menu, cat).build(&t.kit, layer)
}

fn cat_menu_items(mut menu: Menu, cat: &Cat) -> Menu {
    let (open, edit, pin, up, down, archive) = (cat.id.clone(), cat.id.clone(), cat.id.clone(), cat.id.clone(), cat.id.clone(), cat.id.clone());
    let pinned = cat.pinned;
    menu = menu.item(MenuItem::new("Open").on_select(move |s: &mut App| {
        s.cats.row_menu = None;
        s.open_cat(&open);
    }));
    menu = menu.item(MenuItem::new("Edit").icon(&icons::PENCIL).on_select(move |s: &mut App| s.open_edit_cat(&edit)));
    menu = menu.item(MenuItem::new(if pinned { "Unpin" } else { "Pin" }).icon(&icons::PIN).on_select(move |s: &mut App| s.cat_pin(&pin, !pinned)));
    menu = menu.item(MenuItem::new("Move up").on_select(move |s: &mut App| s.cat_move(&up, -1)));
    menu = menu.item(MenuItem::new("Move down").on_select(move |s: &mut App| s.cat_move(&down, 1)));
    menu.separator().item(MenuItem::new("Archive").icon(&icons::ARCHIVE).danger(true).on_select(move |s: &mut App| s.ask_archive_cat(&archive)))
}

// ---- the cat page -------------------------------------------------------------------------------

/// The middle (the prototype's `.col`): the cat's bar (face, name, area, cost, the "..." menu), the conversation (until it is built, the
/// cat's Briefing as its first message) and, at the bottom, the composer with its placeholder line.
pub fn page(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(cat) = s.open_cat_row() else {
        // The list is on its way (a link from elsewhere, a window that has just opened).
        return col().key("cat-page").w_full().h_full().child(super::band(t)).child(placeholder(&["Opening the cat…"], t));
    };
    let marks = s.cat_marks(&cat.id);
    let workspace = s.workspace_name_of(&cat.workspace_id);
    let crumb = match (cat.area.is_empty(), workspace.is_empty()) {
        (false, false) => format!("{} · {workspace}", cat.area),
        (false, true) => cat.area.clone(),
        (true, false) => workspace,
        (true, true) => String::new(),
    };
    // The bar: 48 high, 20 at the sides, 10 apart, a hairline under it. The folded sidebar's "Show sidebar" button leads it.
    let mut head = band_px(t, 20.0).key("cat-head").h(48.0).pt(0.0).fill(t.surface.window).gap(10.0).border_sides([0.0, 0.0, 1.0, 0.0], t.surface.border).child(cat_face::face(&cat.face.0, 22.0, marks, t.surface.window, t)).child(text(cat.name.clone()).key("cat-title").font(font(t, 14.0, 600.0, 20.0)).color(t.text.base).truncate().min_w(0.0).role("heading").a11y_hidden());
    if !crumb.is_empty() {
        head = head.child(text(crumb).key("cat-crumb").font(font(t, 13.0, 400.0, 18.0)).color(t.text.faint).truncate().min_w(0.0));
    }
    head = head.child(el().grow(1.0).min_w(0.0));
    // The cat's cost comes from the usage ledger, which does not break a day down by cat yet: the honest number is the one it has.
    head = head.child(text("$0.00 today").key("cat-cost").font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint));
    head = head.child(cat_menu(s, t, cat, layer)).child(icon_button(&icons::PANEL_RIGHT, if s.layout.context.is_open() { "Hide the panel" } else { "Show the panel" }, "context-toggle", t, |s: &mut App| s.panel_toggle()));
    // The feed: the cat's Briefing, when it has written one, otherwise one quiet line; the conversation itself comes in the next build.
    let briefing = s.cats.files.get(&cat.id).and_then(|f| f.files.iter().find(|f| f.kind == CatFileKind::Briefing)).map(|f| (f.text.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join(" ").trim().to_owned(), f.updated_ms)).filter(|(words, _)| !words.is_empty());
    let mut turns = col().key("cat-turns").w_full().max_w(720.0).px(32.0);
    match briefing {
        Some((words, at)) => {
            let hour = crate::clock::local(at);
            let who = row().gap(7.0).center_y().mb(7.0).child(cat_face::face(&cat.face.0, 16.0, Marks::default(), t.surface.window, t)).child(text(cat.name.clone()).font(font(t, 12.0, 500.0, 16.0)).color(t.text.muted).a11y_hidden()).child(row().pl(7.0).border_sides([0.0, 0.0, 0.0, 1.0], t.surface.line_strong).child(text(format!("Briefing · {:02}:{:02}", hour.hour, hour.minute)).font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).a11y_hidden()));
            turns = turns.child(col().key("cat-briefing").w_full().child(who).child(text(words).key("cat-briefing-text").font(font(t, 14.5, 400.0, 23.5)).color(t.text.base).wrap().min_w(0.0)));
        }
        None => {
            turns = turns.child(text(format!("Talk to {} here. This comes in the next build.", cat.name)).key("cat-coming").font(t.ty.meta).color(t.text.faint).wrap().px(16.0));
        }
    }
    let feed = col().key("cat-conversation").w_full().grow(1.0).min_h(0.0).items_center().justify_end().pt(28.0).pb(8.0).child(turns);
    // The dock: the example-request hint and the composer, 720 wide at most, 32 at the sides, 20 below.
    let hint = row().key("cat-hint").gap(8.0).mb(10.0).center_y().child(row().h(26.0).px(9.0).gap(6.0).center_y().radius(7.0).border(1.0, t.surface.line_strong).child(icon(&icons::SPARKLES, 14.0).color(t.text.muted).a11y_hidden()).child(text("Try an example request").font(font(t, 12.0, 400.0, 16.0)).color(t.text.muted).a11y_hidden()));
    let model = look::chip(t, "cat-model", None, &s.cat_model_label(cat), true);
    let bar = row().w_full().gap(6.0).center_y().child(look::chip_square(t, "cat-attach", "Attach a file", &icons::CLIP)).child(model).child(el().grow(1.0)).child(look::send(t, "cat-send", "Send", false, |_: &mut App| {}));
    let composer = col().key("cat-composer").role("textbox").label(format!("Tell {} what to work on", cat.name)).w_full().pt(12.0).pb(10.0).pl(14.0).pr(12.0).radius(t.radius.window).fill(t.surface.card).border(1.0, t.surface.line_strong).child(text(format!("Tell {} what to work on…", cat.name)).key("cat-composer-hint").font(font(t, 13.5, 400.0, 20.0)).color(t.text.faint).h(40.0).a11y_hidden()).child(bar);
    let dock = col().key("cat-dock").w_full().items_center().px(32.0).pt(12.0).pb(20.0).child(col().w_full().max_w(720.0).child(hint).child(composer));
    col().key("cat-page").w_full().h_full().min_w(0.0).child(head).child(feed).child(dock)
}

/// The header's "..." menu: Edit, Pin or Unpin, Archive.
fn cat_menu(s: &App, t: &Neko, cat: &Cat, layer: &mut Vec<El>) -> El {
    let trigger = icon_button(&icons::MORE, "More actions", "cat-more", t, |s: &mut App| s.cats.menu = !s.cats.menu);
    let (edit, pin, archive) = (cat.id.clone(), cat.id.clone(), cat.id.clone());
    let pinned = cat.pinned;
    let menu = Menu::new("cat-menu").trigger(trigger).open(s.cats.menu).placement(Placement::BottomEnd).min_width(200.0).on_close(|s: &mut App| s.cats.menu = false);
    menu.item(MenuItem::new("Edit").icon(&icons::PENCIL).on_select(move |s: &mut App| s.open_edit_cat(&edit)))
        .item(MenuItem::new(if pinned { "Unpin" } else { "Pin" }).icon(&icons::PIN).on_select(move |s: &mut App| s.cat_pin(&pin, !pinned)))
        .separator()
        .item(MenuItem::new("Archive").icon(&icons::ARCHIVE).danger(true).on_select(move |s: &mut App| s.ask_archive_cat(&archive)))
        .build(&t.kit, layer)
}

// ---- the panel ----------------------------------------------------------------------------------

/// The right panel (the prototype's `.ov`): on the chrome colour with a hairline on its left, a 48 point head holding the segmented control
/// (Overview, Files, Rounds) over one scrolling body.
pub fn panel(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let tab = s.cat_tab();
    let labels: Vec<&str> = CatTab::ALL.iter().map(|x| x.title()).collect();
    let selected = CatTab::ALL.iter().position(|x| *x == tab).unwrap_or(0);
    let control = look::seg_full(t, "cat-tabs", &labels, selected, |s: &mut App, i: usize| s.cat_choose_tab(CatTab::ALL[i]));
    let head = row().key("cat-panel-head").w_full().h(super::context::HEAD).px(12.0).center_y().child(control);
    let body = match (s.open_cat_row(), tab) {
        (None, _) => placeholder(&["Nothing to show yet."], t),
        (Some(cat), CatTab::Overview) => overview(s, t, cat),
        (Some(cat), CatTab::Files) => files(s, t, cat, layer),
        (Some(cat), CatTab::Rounds) => rounds(s, t, cat),
    };
    col().key("context").min_w(0.0).fill(t.surface.sidebar).border_sides([0.0, 0.0, 0.0, 1.0], t.surface.border).child(head.border_sides([0.0, 0.0, 1.0, 0.0], t.surface.border)).child(col().key("cat-panel-body").w_full().grow(1.0).min_h(0.0).child(body))
}

/// A section's heading (`.sec`): its title and, at the right, how many (or a quiet figure).
fn heading(t: &Neko, key: &str, title: &str, count: Option<usize>) -> El {
    look::label(t, &format!("cat-sec-{key}"), title, count.map(|n| n.to_string())).mt(if key == "needs" { 0.0 } else { 6.0 })
}

/// What a section says when nothing is in it (an `.item` with only a `.sub`).
fn empty_line(t: &Neko, key: &str, words: &str) -> El {
    text(words.to_owned()).key(format!("cat-empty-{key}")).font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).wrap().px(8.0).py(8.0).min_w(0.0)
}

/// One task of the cat (`.item`): its glyph, its title (13.5), a line under it (12, the third grey), a progress bar when it works, how long ago.
fn task_item(s: &App, t: &Neko, task: &crate::link::TaskRow, sub: String, bar: Option<f32>) -> El {
    let id = task.id.clone();
    let mut tx = col().grow(1.0).min_w(0.0).child(text(task.title.clone()).font(font(t, 13.5, 400.0, 18.0)).color(t.text.base).wrap().min_w(0.0).a11y_hidden()).child(text(sub).font(font(t, 12.0, 400.0, 16.0)).color(t.text.faint).wrap().mt(2.0).min_w(0.0).a11y_hidden());
    if let Some(share) = bar {
        tx = tx.child(row().w_full().h(3.0).radius(1.5).mt(8.0).fill(t.surface.line_strong).clip_rounded(1.5).child(el().h(3.0).w(1.0).grow(share.max(0.01)).fill(t.meaning.iris)).child(el().h(3.0).grow((1.0 - share).max(0.0)).w(0.0)));
    }
    row()
        .key(format!("cat-task-{}", task.id))
        .role("button")
        .label(task.title.clone())
        .focusable(true)
        .w_full()
        .padding(8.0)
        .gap(10.0)
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.select(&id))
        .child(el().mt(2.0).child(look::task_glyph(t, task)))
        .child(tx)
        .child(text(crate::groups::age(s.now_ms, task.updated_at_ms)).font(tnum(font(t, 11.5, 400.0, 16.0))).color(t.text.faint).a11y_hidden())
}

/// Overview: Needs you, Working, Ready to merge, Rounds, Files and what is settled, each with its own honest empty line.
fn overview(s: &App, t: &Neko, cat: &Cat) -> El {
    let needs = s.cat_tasks(&cat.id, |st| st == TaskStatus::AwaitingApproval);
    let working = s.cat_tasks(&cat.id, |st| matches!(st, TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing));
    let ready = s.cat_tasks(&cat.id, |st| st == TaskStatus::ReadyForReview);
    let settled = s.cat_tasks(&cat.id, |st| matches!(st, TaskStatus::Completed | TaskStatus::Cancelled)).len();
    let mut c = col().key("cat-overview").w_full().padding(10.0).pb(16.0);
    c = c.child(heading(t, "needs", "Needs you", Some(needs.len())));
    if needs.is_empty() {
        c = c.child(empty_line(t, "needs", "Nothing is waiting for you."));
    }
    for task in needs {
        c = c.child(task_item(s, t, task, "Waiting for you".to_owned(), None));
    }
    if !working.is_empty() {
        c = c.child(heading(t, "working", "Working", None));
        for task in working {
            let (sub, bar) = match task.progress {
                Some((done, of)) if of > 0 => (format!("Step {done} of {of}"), Some(done as f32 / of as f32)),
                _ => ("Working".to_owned(), None),
            };
            c = c.child(task_item(s, t, task, sub, bar));
        }
    }
    if !ready.is_empty() {
        c = c.child(heading(t, "ready", "Ready to merge", None));
        for task in ready {
            let pr = s.pr_of(task).map_or_else(|| "Ready to merge".to_owned(), |p| format!("PR {p} · checks passed"));
            c = c.child(task_item(s, t, task, pr, None));
        }
    }
    // Rounds are not tied to a cat yet (`RoutinePolicy::cat_id` is not built).
    c = c.child(heading(t, "rounds", "Rounds", Some(0))).child(empty_line(t, "rounds", "No rounds yet."));
    c = c.child(heading(t, "files", "Files", None));
    for kind in CatFileKind::ALL {
        c = c.child(filerow(t, kind));
    }
    if settled > 0 {
        c = c.child(row().key("cat-settled").w_full().h(32.0).px(8.0).gap(8.0).center_y().mt(10.0).border_sides([1.0, 0.0, 0.0, 0.0], t.surface.border).child(text(format!("Done · {settled}")).font(font(t, 12.5, 400.0, 18.0)).color(t.text.faint).grow(1.0).a11y_hidden()).child(icon(&icons::CHEVRON_RIGHT, 14.0).color(t.text.faint).a11y_hidden()));
    }
    col().key("cat-overview-scroll").w_full().h_full().min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}

/// A file on the Overview (`.filerow`): its icon, its name, its file name in the mono face at the right; a press opens it in Files.
fn filerow(t: &Neko, kind: CatFileKind) -> El {
    row()
        .key(format!("cat-filerow-{}", kind.file_name()))
        .role("button")
        .label(file_title(kind))
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(8.0)
        .gap(9.0)
        .center_y()
        .radius(7.0)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| {
            s.cat_choose_tab(CatTab::Files);
            s.open_cat_file(kind);
        })
        .child(icon(&icons::FILE, 16.0).color(t.text.faint).a11y_hidden())
        .child(text(file_title(kind)).font(font(t, 13.0, 400.0, 18.0)).color(t.text.muted).grow(1.0).truncate().a11y_hidden())
        .child(text(kind.file_name()).font(rune::TextStyle { size: 11.5, ..t.ty.mono_meta }).color(t.text.faint).a11y_hidden())
}

/// Rounds: the cat's own, which do not exist yet (`RoutinePolicy::cat_id` is not built).
fn rounds(_s: &App, t: &Neko, _cat: &Cat) -> El {
    col().key("cat-rounds").w_full().padding(10.0).child(heading(t, "rounds", "Rounds", Some(0))).child(empty_line(t, "rounds-tab", "No rounds yet."))
}

/// Files: the four files of the cat, and the editor once one is opened.
fn files(s: &App, t: &Neko, cat: &Cat, layer: &mut Vec<El>) -> El {
    if let Some(e) = s.cats.editor.as_ref().filter(|e| e.cat == cat.id) {
        return editor(s, t, cat, e);
    }
    let _ = layer;
    let mut c = col().key("cat-files").w_full().px(4.0).pt(4.0).pb(16.0).gap(2.0);
    c = c.child(heading(t, "files", "Files", None));
    for kind in CatFileKind::ALL {
        c = c.child(file_row(t, kind));
    }
    if !cat.purpose.trim().is_empty() {
        c = c.child(heading(t, "purpose", "Purpose", None)).child(text(cat.purpose.clone()).key("cat-purpose").font(font(t, 13.0, 400.0, 20.0)).color(t.text.base.alpha(0.8)).wrap().px(8.0).pb(8.0).min_w(0.0));
    }
    col().key("cat-files-scroll").w_full().h_full().min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}

/// One file: its icon, its name and what it is for, and its file name in the mono face at the right.
fn file_row(t: &Neko, kind: CatFileKind) -> El {
    row()
        .key(format!("cat-file-{}", kind.file_name()))
        .role("button")
        .label(format!("{}: {}", file_title(kind), file_purpose(kind)))
        .focusable(true)
        .w_full()
        .min_h(48.0)
        .px(8.0)
        .py(6.0)
        .gap(10.0)
        .center_y()
        .radius(8.0)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.open_cat_file(kind))
        .child(icon(&icons::FILE, 16.0).color(t.text.muted).a11y_hidden())
        .child(col().grow(1.0).min_w(0.0).child(text(file_title(kind)).font(font(t, 13.5, 400.0, 20.0)).color(t.text.base).truncate().a11y_hidden()).child(text(file_purpose(kind)).font(font(t, 12.0, 400.0, 16.0)).color(t.text.muted).truncate().a11y_hidden()))
        .child(text(kind.file_name()).font(t.ty.mono_meta).color(t.text.faint).a11y_hidden())
}

/// The editor: a back button and the file's name, the text in the mono face, then the state, Revert and Save.
fn editor(s: &App, t: &Neko, cat: &Cat, e: &cats::FileEditor) -> El {
    let text_now = s.pages.text(F_FILE);
    let dirty = text_now != e.saved;
    let size = text_now.len();
    let over = size > MAX_CAT_FILE;
    let back = row()
        .key("cat-file-back")
        .role("button")
        .label("Back to the files")
        .focusable(true)
        .h(28.0)
        .pl(4.0)
        .pr(8.0)
        .gap(2.0)
        .center_y()
        .radius(t.radius.control)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.close_cat_file())
        .child(icon(&icons::CHEVRON_LEFT, 16.0).color(t.text.muted).a11y_hidden())
        .child(text("Files").font(t.ty.meta).color(t.text.muted).a11y_hidden());
    let head = row().key("cat-editor-head").w_full().px(8.0).pt(4.0).pb(8.0).gap(8.0).center_y().child(back).child(el().grow(1.0).min_w(0.0)).child(text(e.kind.file_name()).key("cat-editor-name").font(t.ty.mono_meta).color(t.text.faint));
    let title = text(format!("{}'s {}", cat.name, file_title(e.kind).to_lowercase())).key("cat-editor-title").font(font(t, 13.5, 500.0, 20.0)).color(t.text.base).truncate().px(12.0).pb(6.0);
    // The text: the mono face on the well, growing with what is typed; the panel body scrolls.
    let area = rune::text_input(edit_of(s, F_FILE), t.ty.mono)
        .key("input")
        .w_full()
        .color(t.text.base)
        .placeholder(format!("Nothing in {} yet.", file_title(e.kind)), t.text.faint)
        .caret(t.text.base, t.kit.selection())
        .label(format!("{} text", file_title(e.kind)))
        .on_edit(editing(F_FILE.to_owned()));
    let well = col().key(F_FILE).role("textbox").w_full().min_h(240.0).px(12.0).py(10.0).radius(12.0).fill(t.text.base.alpha(0.03)).border(1.0, t.surface.field_border).focus_within_ring(2.0, 2.0, t.kit.c.border_focus).child(rune::auto_grow(area, 14, 10_000));
    let mut body = col().key("cat-editor-body").w_full().px(8.0).pb(8.0).gap(8.0).child(well);
    if e.truncated {
        body = body.child(text("This file is longer than 16 KB on disk. Only its first 16 KB is here, and saving will cut the rest.").key("cat-editor-truncated").font(t.ty.meta).color(t.meaning.amber).wrap().min_w(0.0));
    }
    if over {
        body = body.child(text(format!("Keep it under 16 KB. This is {}.", cats::size_words(size))).key("cat-editor-over").font(t.ty.meta).color(t.meaning.amber).wrap().min_w(0.0));
    }
    if let Some(error) = &e.error {
        body = body.child(text(error.clone()).key("cat-editor-error").font(t.ty.meta).color(t.meaning.coral).wrap().min_w(0.0).live_region(rune::LiveRegion::Polite));
    }
    let state = if e.saving {
        ("Saving", t.text.muted)
    } else if dirty {
        ("Unsaved changes", t.meaning.amber)
    } else {
        ("Saved", t.text.muted)
    };
    let plain = font(t, 12.5, 400.0, 18.0);
    let left = row().gap(6.0).center_y().child(text(state.0.to_owned()).key("cat-editor-state").font(plain).color(state.1)).child(text("·".to_owned()).font(plain).color(t.text.muted)).child(webkit::kbd(t, "cat-editor-kbd", "⌘S")).child(text("saves".to_owned()).font(plain).color(t.text.muted));
    let revert = if dirty { quiet_button("Revert", "cat-editor-revert", t, |s: &mut App| s.revert_cat_file()) } else { quiet_button("Revert", "cat-editor-revert", t, |_| {}).disabled(true).opacity(0.5) };
    let can_save = dirty && !e.saving && !over;
    let save = if can_save { primary_button("Save", "cat-editor-save", t, |s: &mut App| s.save_cat_file()) } else { primary_button("Save", "cat-editor-save", t, |_| {}).disabled(true).opacity(0.4) };
    let foot = row().key("cat-editor-foot").w_full().px(12.0).py(8.0).gap(8.0).center_y().flex_wrap().child(left).child(el().grow(1.0).min_w(0.0)).child(revert).child(save);
    col().key("cat-editor").w_full().h_full().min_h(0.0).child(head).child(col().w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(title).child(body)).child(foot)
}

// ---- the New cat sheet --------------------------------------------------------------------------

/// The line under a field the daemon (or this client) refused.
fn error_line(s: &App, t: &Neko, field: &'static str) -> Option<El> {
    s.cats.sheet.as_ref()?.errors.get(field).map(|m| text(m.clone()).key(format!("cat-error-{field}")).font(t.ty.meta).color(t.meaning.coral).wrap().min_w(0.0).live_region(rune::LiveRegion::Polite))
}

/// A field with its label above, its control, and its refusal under it.
fn labelled(s: &App, t: &Neko, key: &'static str, label: &str, hint: Option<&str>, control: El) -> El {
    let mut c = col().key(format!("cat-field-{key}")).w_full().gap(6.0).child(field(t, key, label, None, control));
    if let Some(e) = error_line(s, t, key) {
        c = c.child(e);
    } else if let Some(h) = hint {
        c = c.child(text(h.to_owned()).font(t.ty.meta).color(t.text.faint).wrap().min_w(0.0));
    }
    c
}

/// A live row of the cat as it will look in the sidebar: face, name, area.
fn preview(s: &App, t: &Neko, face: &str) -> El {
    let name = s.pages.text(F_NAME).trim().to_owned();
    let area = s.pages.text(F_AREA).trim().to_owned();
    let named = if name.is_empty() { text("Name".to_owned()).font(font(t, 14.0, 400.0, 20.0)).color(t.text.faint) } else { text(name).font(font(t, 14.0, 500.0, 20.0)).color(t.text.base) };
    row()
        .key("cat-preview")
        .role("status")
        .label("How the cat will look")
        .w_full()
        .h(48.0)
        .px(12.0)
        .gap(12.0)
        .center_y()
        .radius(12.0)
        .fill(t.text.base.alpha(0.04))
        .child(cat_face::face(face, 24.0, Marks::default(), t.surface.window, t))
        .child(named.key("cat-preview-name").truncate().min_w(0.0))
        .child(text(area).key("cat-preview-area").font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0))
}

/// The six faces, one pressed.
fn face_picker(t: &Neko, current: &str) -> El {
    let mut r = row().key("cat-faces").role("radiogroup").label("Face").focus_group().w_full().gap(8.0);
    for id in cat_face::ids() {
        let on = id == current;
        let pick = id.to_owned();
        r = r.child(
            row()
                .key(format!("cat-face-{id}"))
                .role("radio")
                .label(cat_face::label(id))
                .help(cat_face::label(id))
                .checked(on)
                .focusable(true)
                .focus_selected(on)
                .size(48.0, 48.0)
                .center()
                .radius(14.0)
                .border(2.0, if on { t.meaning.iris } else { Color::TRANSPARENT })
                .hover(t.surface.hover)
                .focus_ring(2.0, 2.0, t.surface.ring)
                .on_press(move |s: &mut App| s.sheet_face(&pick))
                .child(cat_face::face(id, 36.0, Marks::default(), t.surface.window, t)),
        );
    }
    r
}

/// The sheet, when it is open: New cat, or Edit.
pub fn mount_sheet(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32) {
    let Some(sheet) = s.cats.sheet.as_ref() else { return };
    let editing = sheet.editing.is_some();
    let mut popups = Vec::new();
    let mut body = col().key("cat-sheet-body").w_full().gap(14.0);
    body = body.child(preview(s, t, &sheet.face));
    body = body.child(labelled(s, t, "face", "Face", None, face_picker(t, &sheet.face)));
    body = body.child(labelled(s, t, "name", "Name", None, text_field(s, t, F_NAME, "Name", "miso", false, false, |s: &mut App| s.sheet_submit())));
    // Looks after: the workspace (fixed once the cat exists) and the area inside it.
    let workspaces: Vec<(String, String)> = s.summary.as_ref().map(|m| m.workspaces.iter().map(|w| (w.id.clone(), w.name.clone())).collect()).unwrap_or_default();
    let current = workspaces.iter().find(|(id, _)| *id == sheet.workspace).map(|(_, n)| n.clone()).unwrap_or_default();
    let place = if editing {
        text(if current.is_empty() { "Unavailable workspace".to_owned() } else { current.clone() }).key("cat-workspace-fixed").font(font(t, 14.0, 400.0, 20.0)).color(t.text.muted).truncate().min_w(0.0).h(32.0).center_y()
    } else {
        let names: Vec<String> = workspaces.iter().map(|(_, n)| n.clone()).collect();
        let by_name = workspaces.clone();
        pick(s, t, &mut popups, "cat-workspace", "Workspace", &names, &current, 216.0, move |s: &mut App, chosen: &str| {
            if let Some((id, _)) = by_name.iter().find(|(_, n)| n == chosen) {
                s.sheet_workspace(id);
            }
        })
    };
    let area = text_field(s, t, F_AREA, "Area", "Web", false, false, |s: &mut App| s.sheet_submit()).grow(1.0).min_w(0.0);
    let mut looks = col().key("cat-field-looks").w_full().gap(6.0).child(field(t, "looks", "Looks after", None, row().w_full().gap(8.0).center_y().child(place).child(area)));
    for f in ["workspace", "area"] {
        if let Some(e) = error_line(s, t, f) {
            looks = looks.child(e);
        }
    }
    if editing {
        looks = looks.child(text("A cat looks after one workspace for as long as it exists.").font(t.ty.meta).color(t.text.faint).wrap().min_w(0.0));
    }
    body = body.child(looks);
    // Model: Neko's default, or any runtime and model the daemon lists.
    let default = "Neko's default".to_owned();
    let mut options = vec![default.clone()];
    options.extend(s.cats.choices.iter().map(|c| c.label.clone()));
    let chosen = sheet.model.as_ref().and_then(|(p, m)| s.cats.choices.iter().find(|c| c.provider == *p && (c.model == *m || m.is_empty()))).map_or(default.clone(), |c| c.label.clone());
    let choices = s.cats.choices.clone();
    let model = pick(s, t, &mut popups, "cat-model", "Model", &options, &chosen, 360.0, move |s: &mut App, label: &str| {
        s.sheet_model(choices.iter().find(|c| c.label == label).map(|c| (c.provider.clone(), c.model.clone())));
    });
    body = body.child(labelled(s, t, "model", "Model", Some("What the cat thinks with and what its tasks start on."), model));
    let limit = row().w_full().gap(8.0).center_y().child(text("$").font(font(t, 14.0, 400.0, 20.0)).color(t.text.muted)).child(text_field(s, t, F_LIMIT, "Daily limit", "No limit", false, false, |s: &mut App| s.sheet_submit()).w(160.0));
    body = body.child(labelled(s, t, "limit", "Daily limit · optional", Some("The most this cat may spend in a day."), limit));
    body = body.child(labelled(s, t, "purpose", "Purpose · optional", None, text_area(s, t, F_PURPOSE, "Purpose", "What this cat looks after and how it should work.", 3)));
    if let Some(e) = error_line(s, t, "form") {
        body = body.child(e);
    }
    let blocked = s.sheet_blocker();
    let cancel = quiet_button("Cancel", "cat-sheet-cancel", t, |s: &mut App| s.close_cat_sheet());
    let label = if editing { "Save" } else { "Create cat" };
    let go = if blocked.is_none() { primary_button(label, "cat-sheet-save", t, |s: &mut App| s.sheet_submit()) } else { primary_button(label, "cat-sheet-save", t, |_| {}).disabled(true).opacity(0.4) };
    let buttons = row().key("cat-sheet-buttons").w_full().gap(8.0).justify_end().center_y().pt(4.0).child(cancel).child(go);
    let scroll = col().key("cat-sheet-scroll").w_full().max_h((win_h - 240.0).max(200.0)).scroll(ScrollSpec { stick_bottom: false }).child(body);
    let intro = text(if editing { "Change what this cat is called, looks like and thinks with." } else { "A cat looks after one area of a workspace." }.to_owned()).key("cat-sheet-intro").font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0);
    let title = if editing { format!("Edit {}", s.pages.text(F_NAME).trim()).trim().to_owned() } else { "New cat".to_owned() };
    let title = if editing && s.pages.text(F_NAME).trim().is_empty() { "Edit cat".to_owned() } else { title };
    Dialog::new("cat-sheet", title).open(true).width(520.0).fit().child(col().w_full().px(16.0).pb(16.0).gap(12.0).child(intro).child(scroll).child(buttons)).on_close(|s: &mut App| s.close_cat_sheet()).build(&t.kit, layer);
    layer.extend(popups);
}
