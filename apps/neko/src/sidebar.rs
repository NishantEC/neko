//! The sidebar (docs/knowledge/app-shell.md, docs/knowledge/design.md, docs/knowledge/threads.md): an inset panel on the window's warm
//! ground. New thread, Needs you, Briefing and Rounds; then the Cattains (the cats at work now, each with what it is doing; absent
//! when none is); then Threads, grouped by project (or in one list), each thread a two-line row led by its status mark; Add project….
//!
//! Every group and row is one list for the keys: Up and Down move through groups and rows, a typed name goes to it, Tab stops
//! once (on the open thread, else the first group). A group's own buttons are Tab stops of their own.

use std::rc::Rc;

use neko_protocol::{NeedsItem, Task, TaskStatus, Workspace};
use rune::{Color, El, Key, KeyEvent, Shadow, col, el, row, text};
use rune_ui_style::{ButtonVariant, tabular, weight};

use crate::icons;
use crate::shelf::Shelf;
use crate::state::{App, CatState, Daemon, Effect, Pick, SECTION_ROWS};
use crate::ui::agent::{PressTx, icon_button};
use crate::ui::base::{Delta, Shape, Size, States, Theme, glyph, paint_state};
use crate::ui::button::Button;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::sidebar::SidebarItem;
use crate::ui::tooltip::Tooltip;
use crate::threads::{self, Grouping, Line};
use crate::view::{BAND, SIDEBAR, lights_room, menu_named, raised, toggle_button};

/// How far the panel sits in from the window's edges.
pub const INSET: f32 = 8.0;
/// The panel's corner radius: the window's own corner less the inset, so the two curves run parallel.
const PANEL_RADIUS: f32 = 14.0;
/// A cat's portrait in a row (a Cattain's).
const FACE: f32 = 28.0;

/// The sidebar's one vertical rhythm, in steps of the theme's spacing unit (docs/knowledge/design.md, "Sidebar rhythm"). Every
/// gap between stacked rows is `ROW_GAP`; a group's header is one control high and sits `ROW_GAP` over its own rows; groups
/// are `SECTION_GAP` apart, and the sections (the nav, the Cattains, Threads with Add project…) `GROUP_GAP` apart.
pub const ROW_GAP_STEPS: f32 = 0.5;
pub const SECTION_GAP_STEPS: f32 = 3.0;
pub const GROUP_GAP_STEPS: f32 = 5.0;

/// A one-line row's height (nav rows, Add project…, Show more).
pub fn row_1(t: &Theme) -> f32 {
    t.metrics.row
}

/// A two-line row's height (a thread, a Cattain): the one-line row and one more step either side.
pub fn row_2(t: &Theme) -> f32 {
    t.metrics.row + t.space.at(4.0)
}

/// A section header's height: a control high, because its buttons are.
pub fn header(t: &Theme) -> f32 {
    t.control.sm
}

/// The panel's colour: in light a step lighter than the window's ground; in dark the window's ground lifted a fixed step (the
/// page and the ground are one colour in dark, so the lift is taken from the ground, never from the page).
pub fn panel_bg(t: &Theme) -> Color {
    if t.dark { rune_ui_style::mix(t.c.surface.bg, Color::WHITE, 0.088) } else { rune_ui_style::mix(t.c.page.bg, t.c.surface.bg, 0.35) }
}

/// The sidebar as an inset panel on the window's ground: a hairline, a soft shadow, lit from above in dark.
pub fn panel(t: &Theme, inner: El) -> El {
    let edge = if t.dark { Color::WHITE.alpha(0.08) } else { Color::BLACK.alpha(0.07) };
    let mut p = inner.w_full().h_full().radius(PANEL_RADIUS).round_as(t).fill(panel_bg(t)).border(1.0, edge);
    p = p
        .box_shadow(Shadow { y: 1.0, blur: 3.0, color: Color::BLACK.alpha(if t.dark { 0.4 } else { 0.06 }), ..Shadow::default() })
        .box_shadow(Shadow { y: 8.0, blur: 24.0, spread: -8.0, color: Color::BLACK.alpha(if t.dark { 0.5 } else { 0.12 }), ..Shadow::default() });
    if t.dark {
        p = p.box_shadow(Shadow { y: 1.0, inset: true, color: Color::WHITE.alpha(0.06), ..Shadow::default() });
    }
    col().w(SIDEBAR + 2.0 * INSET).h_full().padding(INSET).child(p.clip_rounded(PANEL_RADIUS))
}


/// Search (⌘K), beside Hide sidebar in the band.
fn search_button(t: &Theme) -> El {
    let b = Button::new("search-open", "Search").variant(ButtonVariant::Ghost).size(Size::Sm).icon_only(&crate::ui::icons::SEARCH).on_press(|s: &mut App| s.toggle_palette()).build(t);
    Tooltip::new("Search").keys("⌘K").wrap(b)
}

pub fn sidebar(s: &App, t: &Theme, lights: f32, layer: &mut Vec<El>) -> El {
    // The panel starts INSET down and in: its band is twice that shorter, so the lights stay on the window's centre line.
    let band = row().key("sidebar-band").w_full().h(BAND - 2.0 * INSET).center_y().pr(INSET).window_drag().child(lights_room((lights - INSET).max(0.0))).child(row().grow(1.0).h_full()).child(search_button(t)).child(toggle_button(t, true));
    // New thread is where the person starts: the row is selected while its composer (Home, until it is its own page) is what shows.
    let at_home = s.open.is_none() && !s.settings_open && !s.needs_open && !s.rounds_open && !s.briefing_open && s.open_cat.is_none();
    let new = SidebarItem::new("new-thread", "New thread").icon(&icons::COMPOSE).selected(at_home).on_press(|s: &mut App| s.new_thread_here()).build(t);
    let new = Tooltip::new("New thread").keys("⌘N").wrap(new);
    // Everything waiting for the person, with how many (the same count as the Dock's).
    let n = s.visible_needs().len();
    let mut needs = SidebarItem::new("needs-you", "Needs you").icon(&crate::ui::icons::INBOX).selected(s.needs_open).on_press(|s: &mut App| s.open_needs());
    if n > 0 {
        // The count is the amber pill; the row says it in words ("3 waiting").
        needs = needs.description(if n == 1 { "1 waiting".to_owned() } else { format!("{n} waiting") }).trail(pill(t, "needs-you-count", n));
    }
    let needs = needs.build(t);
    let needs = Tooltip::new("Needs you").keys("⌘2").wrap(needs);
    // What happened while the person was away: when there is news the row says it in words (Needs you keeps the count).
    let news = s.briefing_news();
    let mut briefing = SidebarItem::new("briefing", "Briefing").icon(&icons::BRIEFING).selected(s.briefing_open).on_press(|s: &mut App| s.open_briefing());
    if let Some(c) = &news {
        briefing = briefing.description(c.headline());
    }
    let briefing = Tooltip::new("Briefing").keys("⌘4").wrap(briefing.build(t));
    // Work that starts by itself: with how many rounds there are, or that every one is paused.
    let rounds_n = s.rounds.len();
    let mut rounds = SidebarItem::new("rounds", "Rounds").icon(&icons::ROUNDS).selected(s.rounds_open).on_press(|s: &mut App| s.open_rounds());
    if s.rounds_paused && rounds_n > 0 {
        rounds = rounds.description("Paused");
    } else if rounds_n > 0 {
        rounds = rounds.description(if rounds_n == 1 { "1 round".to_owned() } else { format!("{rounds_n} rounds") });
    }
    let rounds = Tooltip::new("Rounds").keys("⌘3").wrap(rounds.build(t));
    let row_gap = t.space.at(ROW_GAP_STEPS);
    let nav = col().key("nav").w_full().gap(row_gap).child(new).child(needs).child(briefing).child(rounds);
    // The Cattains (the cats at work) sit under the nav, above the threads; with none at work there is no section at all.
    let mut body = col().w_full().px(t.space.at(2.0)).child(nav);
    if let Some(c) = cattains(s, t, layer) {
        body = body.child(c);
    }
    let body = body.child(threads_section(s, t, layer));
    let list = col().key("sidebar-scroll").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body.pb(t.space.at(4.0)));
    let mut column = col().key("sidebar").role("navigation").label("Sidebar").w_full().h_full().child(band).child(list);
    // No money at the foot: what runs cost is in the thread's context ring, for whoever opens it.
    if let Some(line) = connection_line(s, t) {
        column = column.child(row().w_full().px(t.space.at(4.0)).py(t.space.at(3.0)).child(line));
    }
    column
}

/// The amber count of things waiting: on Needs you, and on a group with something waiting in it.
fn pill(t: &Theme, key: &str, n: usize) -> El {
    row().h(18.0).min_w(18.0).px(t.space.at(1.5)).radius(9.0).center().fill(t.c.warning.bg).child(text(n.to_string()).key(key.to_owned()).font(tabular(weight(t.ty.xs, t.ty.semibold))).color(t.c.warning.fg).a11y_hidden())
}

/// The surface a row sits on (the portrait's dot is ringed in it).
fn under(t: &Theme, selected: bool) -> Color {
    if selected { crate::view::raised_fill(t) } else { panel_bg(t) }
}

/// A row's frame: an option in a list with its place in it, selected as a raised pebble, Return presses it, the row material gives
/// hover and the focus ring. The same words and keys as rune-ui's SidebarItem in a list. Kept here, not a `SidebarItem` with `.image`
/// and `.subtitle`: its second line is plain words, and these are parts in their own inks (the state word in its colour). Upstream ask:
/// a second line given as an element.
#[allow(clippy::too_many_arguments)]
fn row_frame(t: &Theme, key: String, title: &str, description: String, i: usize, n: usize, selected: bool, indent: f32) -> El {
    let mut r = row()
        .key(key)
        .role("option")
        .label(title.to_owned())
        .help(title.to_owned())
        .description(description)
        .focusable(true)
        .position_in_set(i + 1, n)
        .focus_selected(selected)
        .press_on_return()
        .h(row_2(t))
        .pl(t.space.at(1.0))
        .pr(t.space.at(2.0))
        .gap(t.space.at(2.0))
        .center_y()
        .radius(t.radius.lg)
        .round_as(t)
        .hover_group();
    // A nested row (a Cattain's thread under the Cattain) steps in: the same row, narrower, so its pebble and its hit area start further in.
    r = if indent > 0.0 { r.ml(indent).w(SIDEBAR - 2.0 * t.space.at(2.0) - indent) } else { r.w_full() };
    // The row material gives hover and the focus ring in both states; the open row is raised under it.
    paint_state(if selected { raised(t, r) } else { r }, &t.mat.row, States::default())
}

/// A Cattain's row: its portrait in its state, its name and a line under it, selected as a raised pebble.
#[allow(clippy::too_many_arguments)]
fn face_row(t: &Theme, key: String, title: &str, description: String, face: &str, st: Option<TaskStatus>, meta: El, i: usize, n: usize, selected: bool) -> El {
    let r = row_frame(t, key.clone(), title, description, i, n, selected, 0.0);
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(1.0)
        .child(text(title.to_owned()).font(weight(t.ty.base, if selected { t.ty.semibold } else { t.ty.medium })).color(t.ink()).truncate().min_w(0.0).a11y_hidden())
        .child(meta);
    let portrait = crate::faces::portrait_in(t, &format!("{key}-face"), face, FACE, st, under(t, selected));
    r.child(portrait).child(words)
}

/// The quiet line under a row's title, its parts apart by middle dots.
fn meta_line(t: &Theme, parts: Vec<(String, Color, bool)>) -> El {
    let mut m = row().gap(t.space.at(1.5)).min_w(0.0);
    for (i, (words, ink, strong)) in parts.into_iter().enumerate() {
        if i > 0 {
            m = m.child(text("·").font(t.ty.xs).color(t.ink_faint()).a11y_hidden());
        }
        m = m.child(text(words).font(tabular(weight(t.ty.xs, if strong { t.ty.semibold } else { t.ty.regular }))).color(ink).truncate().min_w(0.0).a11y_hidden());
    }
    m
}

/// The width of a thread row's right side: the age, and under the pointer its menu.
const RIGHT_AGE: f32 = 38.0;
/// And of "Working".
const RIGHT_WORKING: f32 = 46.0;

/// A thread's row (spec: two lines, 52 points): its status mark; the title, semibold while there is activity the person has not
/// seen, and at the right the age or "Working" (the thread's menu under the pointer); under the title the first line that applies
/// ([`threads::second_line`]). `project` is the project's name on the second line's right, when the section is not grouped.
#[allow(clippy::too_many_arguments)]
pub fn thread_row(s: &App, t: &Theme, task: &Task, i: usize, n: usize, open: bool, prefix: &str, indent: f32, project: Option<&str>, layer: &mut Vec<El>) -> El {
    let key = format!("{prefix}-{}", task.id.0);
    // A request of one of its subagents is the thread's to show: "Needs you: Run a command · Kitten 1".
    let need = s.visible_needs().into_iter().find(|x| x.task == task.id || s.tasks.iter().any(|u| u.id == x.task && u.subagent.as_ref().is_some_and(|o| o.root == task.id)));
    let line = threads::second_line(task, need, s.thread_line(task).as_deref());
    let state = threads::state_words(task);
    let age = threads::age(s.now_ms, s.utc_offset_s, task.activity_ms.max(task.created_ms));
    // The right side says the state the mark says: "Working" only while the mark is the working ring, never beside an amber dot.
    let working = crate::status_mark::mark_of(task) == crate::status_mark::Mark::Working;
    let description = format!("{}{}{}", if state.is_empty() { String::new() } else { format!("{state}. ") }, line.spoken(), if project.is_some_and(|p| !p.is_empty()) { format!(", in {}", project.unwrap_or_default()) } else { String::new() });
    let tid = task.id.clone();
    let r = row_frame(t, key.clone(), &task.title, format!("{description}, {}", if working { "working".to_owned() } else { age.clone() }), i, n, open, indent);
    let unread = task.unread();
    let title = text(task.title.clone()).font(weight(t.ty.base, if unread { t.ty.semibold } else { t.ty.regular })).color(t.ink()).truncate().grow(1.0).min_w(0.0).a11y_hidden();
    // The right side: the age (or "Working" in teal), and the menu in its place while the pointer is over the row or the menu is open.
    let menu_open = s.menu_open.as_deref() == Some(format!("thread-menu-{}", task.id.0).as_str());
    let said = if working {
        text("Working").font(weight(t.ty.xs, t.ty.medium)).color(crate::status_mark::ink(t, crate::status_mark::Mark::Working, under(t, open)).unwrap_or(t.c.accent.fg)).a11y_hidden()
    } else {
        text(age).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden()
    };
    let slot_h = row_2(t) / 2.0;
    let right_w = if working { RIGHT_WORKING } else { RIGHT_AGE };
    let mut said = row().absolute(0.0, 0.0).size(right_w, slot_h).justify_end().center_y().child(said);
    let mut menu = row().absolute(0.0, 0.0).size(right_w, slot_h).justify_end().center_y().child(thread_menu(s, t, task, layer));
    if menu_open {
        said = said.opacity(0.0);
    } else {
        said = said.group_opacity(1.0, 0.0);
        menu = menu.group_opacity(0.0, 1.0);
    }
    let right = el().size(right_w, slot_h).child(said).child(menu);
    let first = row().w_full().gap(t.space.at(1.0)).center_y().child(title).child(right);
    let mut second = row().w_full().gap(t.space.at(1.5)).min_w(0.0);
    let ink = match &line {
        Line::NeedsYou(_) => t.c.warning.fg,
        Line::Failed(_) => t.c.danger.fg,
        Line::Quiet(_) => t.ink_muted(),
    };
    let said_line = line.spoken();
    second = second.child(text(said_line).font(tabular(t.ty.xs)).color(ink).truncate().min_w(0.0).grow(1.0).a11y_hidden());
    if let Some(p) = project.filter(|p| !p.is_empty()) {
        second = second.child(text(p.to_owned()).font(t.ty.xs).color(t.ink_faint()).truncate().max_w(88.0).a11y_hidden());
    }
    let words = col().grow(1.0).min_w(0.0).gap(1.0).child(first).child(second);
    let mark = crate::status_mark::status_mark(t, &format!("{key}-mark"), task, under(t, open));
    r.child(mark).child(words).on_press(move |s: &mut App| s.open_task_id(&tid))
}

/// A thread's menu: Pin, Settle, Mark as unread (or read), Archive, Remove thread…. Shown on hover; ⌘⇧P and ⌘⇧S do the first two.
fn thread_menu(s: &App, t: &Theme, task: &Task, layer: &mut Vec<El>) -> El {
    let key = format!("thread-menu-{}", task.id.0);
    let open = s.menu_open.as_deref() == Some(key.as_str());
    let k = key.clone();
    let name = format!("Options for {}", task.title);
    // Pointer only: the list is one Tab stop and its rows are reached by the arrows; the same actions are in the band's menu.
    let mut trigger = row().key(format!("{key}-button")).role("button").label(name.clone()).help("Thread options".to_owned()).focusable(false).size(24.0, 20.0).center().radius(t.radius.md).round_as(t).child(glyph(&crate::ui::icons::MORE, t.control.icon, t.ink_muted()));
    trigger = paint_state(trigger, &t.mat.ghost, States::default()).on_press(move |s: &mut App| s.menu_open = if s.menu_open.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) }).expanded(open);
    let (a, b, c, d, e) = (task.id.clone(), task.id.clone(), task.id.clone(), task.id.clone(), task.id.clone());
    let unread = task.unread();
    let menu = DropdownMenu::new(key)
        .trigger(trigger)
        .open(open)
        .min_width(210.0)
        .on_close(|s: &mut App| s.menu_open = None)
        .item(shelf_item(s, task, Shelf::Pin, a))
        .item(shelf_item(s, task, Shelf::Settle, b))
        .item(if unread {
            DropdownMenuItem::new("Mark as read").on_select(move |s: &mut App| s.mark_read(&c))
        } else {
            // The thread in view is read as soon as anything arrives, so it cannot be left unread.
            DropdownMenuItem::new("Mark as unread").disabled(s.open.as_ref() == Some(&task.id)).on_select(move |s: &mut App| s.mark_unread(&c))
        })
        .item(shelf_item(s, task, Shelf::Archive, d))
        .danger_item(DropdownMenuItem::new("Remove thread…").disabled(task.thread_status == neko_protocol::ThreadStatus::Working).on_select(move |s: &mut App| s.ask_remove_task(&e)));
    menu_named(menu, t, layer, &name, open)
}

/// One of Pin, Settle and Archive as a menu item: it says which way it goes and is off while the thread cannot be
/// settled or archived (it works).
pub fn shelf_item(s: &App, task: &Task, shelf: Shelf, id: neko_protocol::TaskId) -> DropdownMenuItem<App> {
    match shelf {
        Shelf::Pin => {
            let on = task.pinned;
            DropdownMenuItem::new(if on { "Unpin" } else { "Pin to top" }).on_select(move |s: &mut App| s.pin_thread(&id, !on))
        }
        Shelf::Settle => {
            let on = task.settled;
            let item = DropdownMenuItem::new(if on { "Bring back" } else { "Settle" }).on_select(move |s: &mut App| s.settle_thread(&id, !on));
            if on { item } else { item.disabled(!s.can_shelve(&task.id, Shelf::Settle)) }
        }
        Shelf::Archive => {
            let on = task.archived;
            let item = DropdownMenuItem::new(if on { "Restore" } else { "Archive" }).on_select(move |s: &mut App| s.archive_thread(&id, !on));
            if on { item } else { item.disabled(!s.can_shelve(&task.id, Shelf::Archive)) }
        }
    }
}

/// The Cattains: only the cats at work now (a thread working, a request or a question waiting for the person). Each is a two-line
/// row, its portrait in its state, its name and one line of what it is doing: the working thread's title, "needs you: Run a
/// command", or "working on 2" with its threads nested under it. A napping cat is not here; cats are made and edited in
/// Settings > Cats. With no cat at work the section is absent.
fn cattains(s: &App, t: &Theme, layer: &mut Vec<El>) -> Option<El> {
    let at_work: Vec<&neko_protocol::Cat> = s.active_cats().into_iter().filter(|c| s.cat_state(&c.id) != CatState::Napping).collect();
    if at_work.is_empty() {
        return None;
    }
    let head = row().key("cattains-head").w_full().h(header(t)).pl(t.metrics.gap).center_y().child(text("Cattains").key("cattains-heading").role("heading").level(2).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted()));
    let mut list = col().key("cattains").role("list").label("Cattains").w_full().gap(t.space.at(ROW_GAP_STEPS)).focus_group();
    for c in at_work {
        let tasks = s.cat_work(&c.id);
        let needs: Vec<&NeedsItem> = s.visible_needs().into_iter().filter(|i| i.cat == c.id).collect();
        let (st, ink, said, line) = if let Some(first) = needs.first() {
            let line = if tasks.len() >= 2 { format!("needs you · {} threads", tasks.len()) } else { format!("needs you: {}", threads::need_words(first)) };
            (TaskStatus::NeedsYou, t.c.warning.fg, line.clone(), line)
        } else if tasks.len() >= 2 {
            let line = format!("working on {}", tasks.len());
            (TaskStatus::Working, t.c.accent.fg, line.clone(), line)
        } else {
            let title = tasks.first().map_or_else(String::new, |x| x.title.clone());
            (TaskStatus::Working, t.ink_muted(), format!("working on {title}"), title)
        };
        let id = c.id.clone();
        let open = s.open_cat.as_deref() == Some(c.id.as_str());
        let n = 1 + if tasks.len() >= 2 { tasks.len() } else { 0 };
        let strong = ink != t.ink_muted();
        let meta = meta_line(t, vec![(line, ink, strong)]);
        // One list per cat: the cat, then (when it has several) its threads as options of the same list, in the order they are drawn.
        let mut crew = col().key(format!("cattain-{}", c.id)).role("listbox").label(c.name.clone()).w_full().gap(t.space.at(ROW_GAP_STEPS));
        crew = crew.child(face_row(t, format!("cat-{}", c.id), &c.name, said, &c.face, Some(st), meta, 0, n, open).on_press(move |s: &mut App| s.open_cat_page(&id)));
        if tasks.len() >= 2 {
            for (i, task) in tasks.iter().enumerate() {
                crew = crew.child(thread_row(s, t, task, i + 1, n, s.row_is_open(task), "cattain-thread", t.space.at(3.0), None, layer));
            }
        }
        list = list.child(crew);
    }
    Some(col().key("cattains-section").role("group").label("Cattains").w_full().gap(t.space.at(ROW_GAP_STEPS)).pt(t.space.at(GROUP_GAP_STEPS)).child(head).child(list))
}

/// Threads: a header with View and New thread, then the threads in groups (a project's, or the one list), then Add project….
fn threads_section(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let view = view_menu(s, t, layer);
    let new: PressTx<App> = Rc::new(|s: &mut App, _: &mut rune::Tx| s.new_thread_here());
    let new = icon_button(t, "threads-new", "New thread", &crate::ui::icons::PLUS, None, false, Some(new));
    let head = row()
        .key("threads-head")
        .w_full()
        .h(header(t))
        .pl(t.metrics.gap)
        .pr(t.space.at(1.0))
        .gap(t.space.at(0.5))
        .center_y()
        .child(text("Threads").key("threads-heading").role("heading").level(2).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted()).grow(1.0))
        .child(view)
        .child(new);
    let row_gap = t.space.at(ROW_GAP_STEPS);
    let loaded = s.tasks_loaded || s.daemon == Daemon::Absent;
    let none_at_all = s.tasks.is_empty() && loaded;
    let mut groups = col().key("workspaces").role("list").label("Threads").w_full().gap(t.space.at(SECTION_GAP_STEPS)).focus_group();
    // Pinned comes first, from any project; it is not there when nothing is pinned.
    if let Some(p) = s.pinned_group() {
        groups = groups.child(pinned(s, t, &p, layer));
    }
    match s.grouping {
        Grouping::Project => {
            for g in s.thread_groups() {
                // With no thread anywhere, No project is not listed (an empty group says nothing); the projects the person added are.
                if none_at_all && g.workspace.is_none_or(|w| w.is_general()) {
                    continue;
                }
                groups = groups.child(group(s, t, &g, layer));
            }
        }
        Grouping::None => {
            let all = s.thread_groups().into_iter().next();
            if let Some(g) = all.filter(|g| !g.threads.is_empty()) {
                groups = groups.child(ungrouped(s, t, &g, layer));
            }
        }
    }
    // Archived threads are out of the lists; View ▸ Show archived brings them back as one fold at the end, the way to reach one until
    // Search finds it.
    if s.show_archived
        && let Some(a) = s.archived_group()
    {
        groups = groups.child(archived(s, t, &a, layer));
    }
    let mut section = col().key("threads-section").role("group").label("Threads").w_full().gap(row_gap).pt(t.space.at(GROUP_GAP_STEPS)).child(head);
    if none_at_all {
        section = section.child(row().key("threads-none").h(row_1(t)).pl(t.metrics.gap).center_y().child(text("Threads you start appear here").font(t.ty.sm).color(t.ink_faint())));
    } else if !loaded {
        section = section.child(row().key("threads-none").h(row_1(t)).pl(t.metrics.gap).center_y().child(text("Loading…").font(t.ty.sm).color(t.ink_faint())));
    }
    section = section.child(groups);
    let add: PressTx<App> = Rc::new(|s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, Pick::Add { select: false }, tx));
    let add = SidebarItem::<App>::new("add-project", "Add project…").icon(&icons::FOLDER_PLUS).button(true).build(t).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| add(s, tx));
    // The groups and Add project… are one run: the button sits a row gap under the list, as a row sits under a row.
    section.child(add)
}

/// View (the three lines at the Threads header's right): Group by project or no grouping.
fn view_menu(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let open = s.menu_open.as_deref() == Some("threads-view");
    let face: PressTx<App> = Rc::new(|s: &mut App, _: &mut rune::Tx| s.menu_open = if s.menu_open.as_deref() == Some("threads-view") { None } else { Some("threads-view".into()) });
    let trigger = icon_button(t, "threads-view-button", "View", &icons::LIST, None, false, Some(face)).expanded(open);
    let menu = DropdownMenu::new("threads-view")
        .trigger(trigger)
        .open(open)
        .min_width(200.0)
        .on_close(|s: &mut App| s.menu_open = None)
        .label("Group by")
        .item(DropdownMenuItem::new("Project").checked(s.grouping == Grouping::Project).on_select(|s: &mut App| s.set_grouping(Grouping::Project)))
        .item(DropdownMenuItem::new("None").checked(s.grouping == Grouping::None).on_select(|s: &mut App| s.set_grouping(Grouping::None)))
        .separator()
        .item(DropdownMenuItem::new("Show settled").checked(s.show_settled).on_select({
            let on = !s.show_settled;
            move |s: &mut App| s.set_show_settled(on)
        }))
        .item(DropdownMenuItem::new("Show archived").checked(s.show_archived).on_select({
            let on = !s.show_archived;
            move |s: &mut App| s.set_show_archived(on)
        }));
    menu_named(menu, t, layer, "View", open)
}

/// The rows of a group (or of the one list), after eight "Show more".
fn rows(s: &App, t: &Theme, g: &threads::Group<'_>, key: &str, name: &str, project: bool, layer: &mut Vec<El>) -> (El, Option<El>) {
    let all = s.show_all.contains(&g.id) || g.threads.len() <= SECTION_ROWS + 1;
    let shown = s.shown_rows(g);
    let mut list = col().key(format!("{key}-list")).role("listbox").label(name.to_owned()).w_full().gap(t.space.at(ROW_GAP_STEPS));
    let n = shown.len();
    for (i, task) in shown.into_iter().enumerate() {
        let from = if project { None } else { Some(s.workspace_of(&task.workspace).map_or_else(String::new, |w| s.project_name(w))) };
        list = list.child(thread_row(s, t, task, i, n, s.row_is_open(task), "thread", 0.0, from.as_deref(), layer));
    }
    let more = if g.threads.len() > SECTION_ROWS + 1 {
        let id = g.id.clone();
        let words = if all { "Show fewer".to_owned() } else { format!("Show {} more", g.threads.len() - SECTION_ROWS) };
        Some(
            SidebarItem::new(format!("{key}-more"), words)
                .icon(if all { &crate::ui::icons::CHEVRON_UP } else { &crate::ui::icons::CHEVRON_DOWN })
                .on_press(move |s: &mut App| {
                    if !s.show_all.remove(&id) {
                        s.show_all.insert(id.clone());
                    }
                })
                .button(true)
                .build(t),
        )
    } else {
        None
    };
    (list, more)
}

/// The one list when the threads are not grouped: by last activity, each row naming its project.
fn ungrouped(s: &App, t: &Theme, g: &threads::Group<'_>, layer: &mut Vec<El>) -> El {
    let (list, more) = rows(s, t, g, "all", "Threads", false, layer);
    let mut sec = col().key("all-section").role("group").label("Threads").w_full().gap(t.space.at(ROW_GAP_STEPS)).child(list);
    if let Some(m) = more {
        sec = sec.child(m);
    }
    if let Some(f) = settled_fold(s, t, g, "all", false, layer) {
        sec = sec.child(f);
    }
    sec
}

/// Pinned: its heading (a pin and the word, no fold: the group goes when nothing is pinned) and the pinned threads of every project,
/// each naming its project at the right of its second line.
fn pinned(s: &App, t: &Theme, g: &threads::Group<'_>, layer: &mut Vec<El>) -> El {
    let head = row()
        .key("pinned-head")
        .w_full()
        .h(header(t))
        .pl(t.metrics.gap)
        .gap(t.space.at(1.5))
        .center_y()
        .child(glyph(&icons::PIN, t.control.icon_sm, t.ink_faint()))
        .child(text("Pinned").key("pinned-heading").role("heading").level(3).font(weight(t.ty.sm, t.ty.medium)).color(t.ink_muted()).grow(1.0));
    let (list, more) = rows(s, t, g, "pinned", "Pinned threads", false, layer);
    let mut sec = col().key("pinned-section").role("group").label("Pinned").w_full().gap(t.space.at(ROW_GAP_STEPS)).child(head).child(list);
    if let Some(m) = more {
        sec = sec.child(m);
    }
    sec
}

/// A fold line: a chevron, a word and how many are inside; pressing it opens or shuts what it holds.
fn fold_line(t: &Theme, key: String, words: &str, n: usize, open: bool, press: impl Fn(&mut App) + 'static) -> El {
    let chevron = glyph(if open { &crate::ui::icons::CHEVRON_DOWN } else { &crate::ui::icons::CHEVRON_RIGHT }, t.control.icon_sm, t.ink_faint());
    let r = row()
        .key(key)
        .role("button")
        .label(words.to_owned())
        .description(if n == 1 { "1 thread".to_owned() } else { format!("{n} threads") })
        .expanded(open)
        .focusable(true)
        .w_full()
        .h(header(t))
        .pl(t.metrics.gap)
        .pr(t.space.at(2.0))
        .gap(t.space.at(1.5))
        .center_y()
        .radius(t.radius.md)
        .round_as(t);
    paint_state(r, &t.mat.row, States::default())
        .child(chevron)
        .child(text(words.to_owned()).font(weight(t.ty.sm, t.ty.medium)).color(t.ink_faint()).grow(1.0).a11y_hidden())
        .child(text(n.to_string()).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden())
        .on_press(press)
}

/// A group's Settled fold, at the end of the group: shown when the group has settled threads. Pressing it opens them in place (View
/// ▸ Show settled keeps every fold open).
fn settled_fold(s: &App, t: &Theme, g: &threads::Group<'_>, key: &str, project: bool, layer: &mut Vec<El>) -> Option<El> {
    if g.settled.is_empty() {
        return None;
    }
    let open = s.settled_shown(&g.id);
    let id = g.id.clone();
    let line = fold_line(t, format!("{key}-settled"), "Settled", g.settled.len(), open, move |s: &mut App| s.flip_settled(&id));
    let mut sec = col().key(format!("{key}-settled-section")).role("group").label("Settled").w_full().gap(t.space.at(ROW_GAP_STEPS)).child(line);
    if open {
        let mut list = col().key(format!("{key}-settled-list")).role("listbox").label(format!("Settled threads in {}", g.name)).w_full().gap(t.space.at(ROW_GAP_STEPS));
        let n = g.settled.len();
        for (i, task) in g.settled.iter().enumerate() {
            let from = if project { None } else { Some(s.workspace_of(&task.workspace).map_or_else(String::new, |w| s.project_name(w))) };
            list = list.child(thread_row(s, t, task, i, n, s.row_is_open(task), "thread", 0.0, from.as_deref(), layer));
        }
        sec = sec.child(list);
    }
    Some(sec)
}

/// The Archived fold View ▸ Show archived adds: the archived threads, each naming its project; Restore is in each row's menu.
fn archived(s: &App, t: &Theme, g: &threads::Group<'_>, layer: &mut Vec<El>) -> El {
    let open = s.settled_open.contains(threads::ARCHIVED);
    let line = fold_line(t, "archived-fold".to_owned(), "Archived", g.threads.len(), open, |s: &mut App| s.flip_settled(threads::ARCHIVED));
    let mut sec = col().key("archived-section").role("group").label("Archived").w_full().gap(t.space.at(ROW_GAP_STEPS)).child(line);
    if open {
        let mut list = col().key("archived-list").role("listbox").label("Archived threads").w_full().gap(t.space.at(ROW_GAP_STEPS));
        let n = g.threads.len();
        for (i, task) in g.threads.iter().enumerate() {
            let from = s.workspace_of(&task.workspace).map_or_else(String::new, |w| s.project_name(w));
            list = list.child(thread_row(s, t, task, i, n, s.row_is_open(task), "thread", 0.0, Some(&from), layer));
        }
        sec = sec.child(list);
    }
    sec
}

/// What a group's header says about its project, after its name.
fn group_state(s: &App, w: &Workspace) -> String {
    let busy = s.needs_in(&w.id);
    let mut parts = Vec::new();
    if !w.is_active() {
        parts.push("Earlier, removed project".to_owned());
    } else if w.missing {
        parts.push("Folder not found".to_owned());
    }
    if busy > 0 {
        parts.push(format!("{busy} need{} you", if busy == 1 { "s" } else { "" }));
    }
    parts.join(", ")
}

/// One project: its header (fold, name, an amber count when something waits; New thread and its menu on hover or focus), what is
/// wrong with it, and its threads (those that need the person first, then by last activity) with "Show more" after eight.
fn group(s: &App, t: &Theme, g: &threads::Group<'_>, layer: &mut Vec<El>) -> El {
    let fallback;
    let w = match g.workspace {
        Some(w) => w,
        None => {
            fallback = Workspace { id: g.id.clone(), name: g.name.clone(), folder: None, created_ms: 0, removed_ms: None, position: 0, missing: false };
            &fallback
        }
    };
    let id = w.id.clone();
    let folded = s.folded.contains(&w.id);
    let busy = s.needs_in(&w.id);
    let key = format!("ws-{}", w.id);
    let chevron = glyph(if folded { &crate::ui::icons::CHEVRON_RIGHT } else { &crate::ui::icons::CHEVRON_DOWN }, t.control.icon_sm, t.ink_faint());
    let mut head = row()
        .key(key.clone())
        .role("button")
        .label(g.name.clone())
        .expanded(!folded)
        .focusable(true)
        .w_full()
        .h(header(t))
        .pl(t.metrics.gap)
        .pr(t.space.at(1.0))
        .gap(t.space.at(1.5))
        .center_y()
        .radius(t.radius.md)
        .round_as(t)
        .hover_group();
    let said = group_state(s, w);
    if !said.is_empty() {
        head = head.description(said);
    }
    // A header is a label, not a row: no wash under the pointer (it read as the selected row), only the focus ring for the keys. The
    // buttons that appear on hover are its pointer feedback.
    let mut label = t.mat.row.clone();
    label.hover = Delta::new();
    label.pressed = Delta::new();
    head = paint_state(head, &label, States::default());
    head = head.child(chevron).child(text(g.name.clone()).font(weight(t.ty.sm, t.ty.medium)).color(if w.is_active() { t.ink_muted() } else { t.ink_faint() }).grow(1.0).truncate().min_w(0.0).a11y_hidden());
    if w.is_active() {
        let here = id.clone();
        let new: PressTx<App> = Rc::new(move |s: &mut App, _: &mut rune::Tx| s.new_thread_in(&here));
        let new = icon_button(t, &format!("{key}-new"), &format!("New thread in {}", g.name), &crate::ui::icons::PLUS, None, w.missing, Some(new)).group_opacity(0.0, 1.0);
        head = head.child(new);
        if w.is_general() {
            // Room where the others have their menu, so every count lines up at the right.
            head = head.child(el().w(t.control.sm).h(t.control.sm));
        } else {
            head = head.child(project_menu(s, t, w, layer));
        }
    }
    // The count sits last, at the same place in every group, and only when something waits.
    head = head.child(row().w(t.space.at(6.0)).justify_end().child(if busy > 0 { pill(t, &format!("{key}-count"), busy) } else { el().key(format!("{key}-count")) }));
    let toggle = id.clone();
    let keys = id.clone();
    head = head.on_press(move |s: &mut App| s.toggle_fold(&toggle)).on_key(move |s: &mut App, ev: &KeyEvent, _| group_key(s, &keys, ev));
    let mut sec = col().key(format!("{key}-section")).role("group").label(g.name.clone()).w_full().gap(t.space.at(ROW_GAP_STEPS)).child(head);
    let indent = t.metrics.gap + t.control.icon_sm + t.space.at(1.5);
    if w.is_active() && w.missing {
        let here = id.clone();
        let locate: PressTx<App> = Rc::new(move |s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, Pick::Locate(here.clone()), tx));
        let gone = id.clone();
        let line = col()
            .key(format!("{key}-missing"))
            .w_full()
            .pl(indent - t.space.at(2.0))
            .gap(0.0)
            .child(
                row()
                    .pl(t.space.at(2.0))
                    .h(t.control.sm)
                    .gap(t.space.at(1.5))
                    .center_y()
                    .child(glyph(&crate::ui::icons::WARNING, t.control.icon_sm, t.c.warning.fg))
                    .child(text("Folder not found").role("status").font(weight(t.ty.sm, t.ty.medium)).color(t.c.warning.fg)),
            )
            .child(
                row()
                    .gap(t.space.at(1.0))
                    .center_y()
                    .child(Button::<App>::new(format!("{key}-locate"), "Locate…").variant(ButtonVariant::Ghost).size(Size::Sm).build(t).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| locate(s, tx)))
                    .child(Button::new(format!("{key}-remove"), "Remove").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(move |s: &mut App| s.ask_remove_workspace(&gone)).build(t)),
            );
        sec = sec.child(line);
    }
    if !w.is_active() {
        sec = sec.child(row().key(format!("{key}-removed")).w_full().pl(indent).h(t.control.sm).center_y().child(text("Earlier, removed project").font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0)));
    }
    if folded {
        return sec;
    }
    if g.threads.is_empty() && g.settled.is_empty() {
        // Before the daemon's list arrives: a quiet placeholder, not "No threads yet".
        let waiting = s.daemon != Daemon::Absent && !s.tasks_loaded;
        let none = text(if waiting { "Loading…" } else { "No threads yet" }).key(format!("{key}-none")).font(t.ty.sm).color(t.ink_faint());
        return sec.child(row().h(row_1(t)).pl(indent).center_y().child(none));
    }
    if !g.threads.is_empty() {
        let (list, more) = rows(s, t, g, &key, &format!("Threads in {}", g.name), true, layer);
        sec = sec.child(list);
        if let Some(m) = more {
            sec = sec.child(m);
        }
    }
    if let Some(f) = settled_fold(s, t, g, &key, true, layer) {
        sec = sec.child(f);
    }
    sec
}

/// Keys on a group's header: Left folds it, Right opens it, ⌥⌘↑ and ⌥⌘↓ move it.
fn group_key(s: &mut App, id: &str, ev: &KeyEvent) -> bool {
    let m = ev.mods;
    match ev.key {
        Key::Up | Key::Down if m.alt && m.cmd && !m.shift && !m.ctrl => {
            s.move_workspace(id, if ev.key == Key::Up { -1 } else { 1 });
            true
        }
        Key::Left if !(m.alt || m.cmd || m.shift || m.ctrl) && !s.folded.contains(id) => {
            s.set_folded(id, true);
            true
        }
        Key::Right if !(m.alt || m.cmd || m.shift || m.ctrl) && s.folded.contains(id) => {
            s.set_folded(id, false);
            true
        }
        _ => false,
    }
}

/// A project's menu: Rename, Reveal in Finder, Locate (when its folder is not found), Move up and down, Remove project….
fn project_menu(s: &App, t: &Theme, w: &Workspace, layer: &mut Vec<El>) -> El {
    let key = format!("ws-menu-{}", w.id);
    let open = s.menu_open.as_deref() == Some(key.as_str());
    let k = key.clone();
    let face: PressTx<App> = Rc::new(move |s: &mut App, _: &mut rune::Tx| s.menu_open = if s.menu_open.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) });
    let trigger = icon_button(t, &format!("ws-{}-menu", w.id), &format!("{} options", w.name), &crate::ui::icons::MORE, None, false, Some(face)).expanded(open);
    // Shown on hover or focus, and while its menu is open.
    let trigger = if open { trigger } else { trigger.group_opacity(0.0, 1.0) };
    let (a, b, c, d, e) = (w.id.clone(), w.id.clone(), w.id.clone(), w.id.clone(), w.id.clone());
    let folder = w.folder.clone().unwrap_or_default();
    let mut menu = DropdownMenu::new(key)
        .trigger(trigger)
        .open(open)
        .min_width(200.0)
        .on_close(|s: &mut App| s.menu_open = None)
        .item(DropdownMenuItem::new("Rename…").on_select(move |s: &mut App| s.start_rename(&a)))
        .item(DropdownMenuItem::new("Reveal in Finder").disabled(w.missing).on_select(move |s: &mut App| {
            s.menu_open = None;
            s.effects.push(Effect::Reveal(folder.clone()));
        }));
    if w.missing {
        menu = menu.item(DropdownMenuItem::new("Locate…").on_select(move |s: &mut App| {
            s.menu_open = None;
            s.effects.push(Effect::PickFolder(Pick::Locate(b.clone())));
        }));
    }
    menu = menu
        .separator()
        .item(DropdownMenuItem::new("Move up").disabled(!s.can_move(&w.id, -1)).on_select(move |s: &mut App| s.move_workspace(&c, -1)))
        .item(DropdownMenuItem::new("Move down").disabled(!s.can_move(&w.id, 1)).on_select(move |s: &mut App| s.move_workspace(&d, 1)))
        .danger_item(DropdownMenuItem::new("Remove project…").on_select(move |s: &mut App| s.ask_remove_workspace(&e)));
    menu_named(menu, t, layer, &format!("{} options", w.name), open)
}

/// A quiet line at the sidebar's foot while the daemon is not there.
fn connection_line(s: &App, t: &Theme) -> Option<El> {
    let words = match &s.daemon {
        _ if s.daemon_slow() => "Neko has not started",
        Daemon::Starting => "Starting Neko…",
        Daemon::Lost(_) => "Reconnecting to Neko…",
        Daemon::Connected | Daemon::Absent => return None,
    };
    Some(text(words).key("connection").role("status").font(t.ty.sm).color(t.ink_faint()))
}
