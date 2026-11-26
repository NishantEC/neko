//! The sidebar (docs/knowledge/app-shell.md, docs/knowledge/design.md): an inset panel on the window's warm ground. New task, Home and
//! Needs you; each workspace as a section of its tasks, each task a two-line row led by its cat's portrait in its state; then the
//! house's cats, each with what it is doing; New cat….
//!
//! Every section and row is one list for the keys: Up and Down move through sections and rows, a typed name goes to it, Tab stops
//! once (on the open task, else the first section). A section's own buttons are Tab stops of their own.

use std::rc::Rc;

use neko_protocol::{Task, TaskStatus, Workspace};
use rune::{Color, El, Key, KeyEvent, Shadow, col, el, row, text};
use rune_ui_style::{ButtonVariant, tabular, weight};

use crate::icons;
use crate::state::{App, CatState, Daemon, Effect, Pick, SECTION_ROWS};
use crate::ui::agent::{PressTx, icon_button};
use crate::ui::base::{Shape, Size, States, Theme, glyph, paint_state};
use crate::ui::button::Button;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::sidebar::SidebarItem;
use crate::ui::tooltip::Tooltip;
use crate::view::{BAND, SIDEBAR, lights_room, menu_named, raised, since, state_ink, status_mark, toggle_button};

/// How far the panel sits in from the window's edges.
pub const INSET: f32 = 8.0;
/// The panel's corner radius: the window's own corner less the inset, so the two curves run parallel.
const PANEL_RADIUS: f32 = 14.0;
/// A task's or a cat's portrait in a row.
const FACE: f32 = 28.0;
/// A two-line row's height.
const ROW: f32 = 52.0;

/// The panel's colour: a step lighter than the window's ground in light, a step above the page in dark.
pub fn panel_bg(t: &Theme) -> Color {
    if t.dark { rune_ui_style::mix(t.c.page.bg, Color::WHITE, 0.035) } else { rune_ui_style::mix(t.c.page.bg, t.c.surface.bg, 0.35) }
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

pub fn sidebar(s: &App, t: &Theme, lights: f32, layer: &mut Vec<El>) -> El {
    // The panel starts INSET down and in: its band is twice that shorter, so the lights stay on the window's centre line.
    let band = row().key("sidebar-band").w_full().h(BAND - 2.0 * INSET).center_y().pr(INSET).window_drag().child(lights_room((lights - INSET).max(0.0))).child(row().grow(1.0).h_full()).child(toggle_button(t, true));
    // A sidebar row that acts: the same alignment as Home and the tasks, as Mail's and Notes' sidebars have.
    let new = SidebarItem::new("new-task", "New task").icon(&icons::COMPOSE).on_press(|s: &mut App| s.new_task_here()).build(t).role("button");
    let new = Tooltip::new("New task").keys("⌘N").wrap(new);
    let home = SidebarItem::new("home", "Home").icon(&crate::ui::icons::HOME).selected(s.open.is_none() && !s.settings_open && !s.needs_open && !s.rounds_open && s.open_cat.is_none()).on_press(|s: &mut App| s.go_home()).build(t);
    let home = Tooltip::new("Home").keys("⌘1").wrap(home);
    // Everything waiting for the person, with how many (the same count as the Dock's).
    let n = s.visible_needs().len();
    let mut needs = SidebarItem::new("needs-you", "Needs you").icon(&crate::ui::icons::INBOX).selected(s.needs_open).on_press(|s: &mut App| s.open_needs());
    if n > 0 {
        needs = needs.description(if n == 1 { "1 waiting".to_owned() } else { format!("{n} waiting") });
    }
    let mut needs = needs.build(t);
    if n > 0 {
        needs = needs.child(pill(t, "needs-you-count", n));
    }
    let needs = Tooltip::new("Needs you").keys("⌘2").wrap(needs);
    // Work that starts by itself: with how many rounds there are, or that every one is paused.
    let rounds_n = s.rounds.len();
    let mut rounds = SidebarItem::new("rounds", "Rounds").icon(&icons::ROUNDS).selected(s.rounds_open).on_press(|s: &mut App| s.open_rounds());
    if s.rounds_paused && rounds_n > 0 {
        rounds = rounds.description("Paused");
    } else if rounds_n > 0 {
        rounds = rounds.description(if rounds_n == 1 { "1 round".to_owned() } else { format!("{rounds_n} rounds") });
    }
    let rounds = Tooltip::new("Rounds").keys("⌘3").wrap(rounds.build(t));
    let mut body = col().w_full().px(t.space.at(2.0)).gap(t.space.at(1.0)).child(new).child(home).child(needs).child(rounds);
    let mut sections = col().key("workspaces").role("list").label("Workspaces").w_full().gap(t.space.at(1.0)).pt(t.space.at(3.0)).focus_group();
    for w in s.sections() {
        sections = sections.child(section(s, t, &w, layer));
    }
    body = body.child(sections);
    let add: PressTx<App> = Rc::new(|s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, Pick::Add { select: false }, tx));
    let add = SidebarItem::<App>::new("add-workspace", "Add workspace…").icon(&icons::FOLDER_PLUS).build(t).role("button").on_press_tx(move |s: &mut App, tx: &mut rune::Tx| add(s, tx));
    body = body.child(add).child(cats_section(s, t));
    let list = col().key("sidebar-scroll").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body.pb(t.space.at(4.0)));
    let mut column = col().key("sidebar").role("navigation").label("Sidebar").w_full().h_full().child(band).child(list);
    // No money at the foot: what runs cost is in the task's context ring, for whoever opens it.
    if let Some(line) = connection_line(s, t) {
        column = column.child(row().w_full().px(t.space.at(4.0)).py(t.space.at(3.0)).child(line));
    }
    column
}

/// The amber count of things waiting: on Needs you, and on a section with something waiting in it.
fn pill(t: &Theme, key: &str, n: usize) -> El {
    row().h(18.0).min_w(18.0).px(t.space.at(1.5)).radius(9.0).center().fill(t.c.warning.bg).child(text(n.to_string()).key(key.to_owned()).font(tabular(weight(t.ty.xs, t.ty.semibold))).color(t.c.warning.fg).a11y_hidden())
}

/// The surface a row sits on (the portrait's dot is ringed in it).
fn under(t: &Theme, selected: bool) -> Color {
    if selected { crate::view::raised_fill(t) } else { panel_bg(t) }
}

/// A two-line sidebar row: a portrait in its state, a title and a line under it, selected as a raised pebble. The same words and
/// keys as rune-ui's SidebarItem in a list: an option, its place in the list, its state as the description, Return presses it.
#[allow(clippy::too_many_arguments)]
fn face_row(t: &Theme, key: String, title: &str, description: String, face: &str, st: Option<TaskStatus>, meta: El, i: usize, n: usize, selected: bool) -> El {
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
        .w_full()
        .h(ROW)
        .pl(t.space.at(1.0))
        .pr(t.space.at(2.0))
        .gap(t.space.at(2.0))
        .center_y()
        .radius(t.radius.lg)
        .round_as(t);
    // The row material gives hover and the focus ring in both states; the open row is raised under it.
    r = paint_state(if selected { raised(t, r) } else { r }, &t.mat.row, States::default());
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(1.0)
        .child(text(title.to_owned()).font(weight(t.ty.base, if selected { t.ty.semibold } else { t.ty.medium })).color(t.ink()).truncate().min_w(0.0).a11y_hidden())
        .child(meta);
    r.child(crate::faces::portrait_in(t, face, FACE, st, under(t, selected))).child(words)
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

/// A task's row: its cat's portrait in its state, the title, and "miso · working · 12 min".
pub fn task_row(s: &App, t: &Theme, task: &Task, i: usize, n: usize, open: bool) -> El {
    let (_, word) = status_mark(task.status);
    let name = s.cat_name(task);
    let ink = state_ink(t, task.status);
    let meta = meta_line(
        t,
        vec![(name.clone(), t.ink_muted(), true), (word.to_lowercase(), ink.unwrap_or(t.ink_faint()), ink.is_some()), (since(s, task.created_ms), t.ink_faint(), false)],
    );
    let tid = task.id.clone();
    // A task a round made carries a small round mark before its line.
    let (meta, from) = if task.round.is_some() {
        (row().gap(t.space.at(1.0)).center_y().min_w(0.0).child(glyph(&icons::ROUNDS, 11.0, t.ink_faint()).a11y_hidden()).child(meta), ", from a round")
    } else {
        (meta, "")
    };
    face_row(t, format!("task-{}", task.id.0), &task.title, format!("{name}, {}{from}", word.to_lowercase()), s.cat_face(&task.cat), Some(task.status), meta, i, n, open).on_press(move |s: &mut App| s.open_task_id(&tid))
}

/// The house's cats: each active cat with its portrait in its state and what it is doing (napping, working on 2, needs you); its
/// page on a press. Then New cat….
fn cats_section(s: &App, t: &Theme) -> El {
    let head = row().h(t.control.sm).pl(t.metrics.gap).center_y().child(text("Cats").key("cats-heading").role("heading").level(2).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted()));
    let cats = s.active_cats();
    let n = cats.len();
    let mut list = col().key("cats").role("listbox").label("Cats").w_full().gap(1.0).focus_group();
    for (i, c) in cats.into_iter().enumerate() {
        let id = c.id.clone();
        let state = s.cat_state(&c.id);
        let (st, ink) = match state {
            CatState::Napping => (None, t.ink_faint()),
            CatState::Working(_) => (Some(TaskStatus::Working), t.c.accent.fg),
            CatState::NeedsYou => (Some(TaskStatus::NeedsYou), t.c.warning.fg),
        };
        let mut parts = vec![(state.words(), ink, st.is_some())];
        if let Some(p) = c.purpose.as_ref().filter(|p| !p.is_empty() && st.is_none()) {
            parts = vec![(p.clone(), t.ink_faint(), false)];
        }
        let selected = s.open_cat.as_deref() == Some(c.id.as_str());
        list = list.child(face_row(t, format!("cat-{}", c.id), &c.name, state.words(), &c.face, st, meta_line(t, parts), i, n, selected).on_press(move |s: &mut App| s.open_cat_page(&id)));
    }
    let new = SidebarItem::new("new-cat", "New cat…").icon(&crate::ui::icons::PLUS).on_press(|s: &mut App| s.start_new_cat()).build(t).role("button");
    col().key("cats-section").role("group").label("Cats").w_full().gap(1.0).pt(t.space.at(4.0)).child(head).child(list).child(new)
}

/// What a section's header says about its workspace, after its name.
fn section_state(s: &App, w: &Workspace) -> String {
    let busy = s.needs_in(&w.id);
    let mut parts = Vec::new();
    if !w.is_active() {
        parts.push("Earlier, removed workspace".to_owned());
    } else if w.missing {
        parts.push("Folder not found".to_owned());
    }
    if busy > 0 {
        parts.push(format!("{busy} need{} you", if busy == 1 { "s" } else { "" }));
    }
    parts.join(", ")
}

/// One workspace: its header (fold, name, an amber count when something waits; New task and its menu on hover or focus), what is
/// wrong with it, and its tasks, newest first, with "Show more" after eight.
fn section(s: &App, t: &Theme, w: &Workspace, layer: &mut Vec<El>) -> El {
    let id = w.id.clone();
    let folded = s.folded.contains(&w.id);
    let busy = s.needs_in(&w.id);
    let key = format!("ws-{}", w.id);
    let chevron = glyph(if folded { &crate::ui::icons::CHEVRON_RIGHT } else { &crate::ui::icons::CHEVRON_DOWN }, t.control.icon_sm, t.ink_faint());
    let mut head = row()
        .key(key.clone())
        .role("button")
        .label(w.name.clone())
        .expanded(!folded)
        .focusable(true)
        .w_full()
        .h(t.metrics.row)
        .pl(t.metrics.gap)
        .pr(t.space.at(1.0))
        .gap(t.space.at(1.5))
        .center_y()
        .radius(t.radius.md)
        .round_as(t)
        .hover_group();
    let said = section_state(s, w);
    if !said.is_empty() {
        head = head.description(said);
    }
    head = paint_state(head, &t.mat.row, States::default());
    head = head.child(chevron).child(text(w.name.clone()).font(weight(t.ty.sm, t.ty.semibold)).color(if w.is_active() { t.ink_muted() } else { t.ink_faint() }).grow(1.0).truncate().min_w(0.0).a11y_hidden());
    if w.is_active() {
        let here = id.clone();
        let new: PressTx<App> = Rc::new(move |s: &mut App, _: &mut rune::Tx| s.new_task_in(&here));
        let new = icon_button(t, &format!("{key}-new"), &format!("New task in {}", w.name), &crate::ui::icons::PLUS, None, w.missing, Some(new)).group_opacity(0.0, 1.0);
        head = head.child(new);
        if w.is_general() {
            // Room where the others have their menu, so every count lines up at the right.
            head = head.child(el().w(t.control.sm).h(t.control.sm));
        } else {
            head = head.child(workspace_menu(s, t, w, layer));
        }
    }
    // The count sits last, at the same place in every section, and only when something waits.
    head = head.child(row().w(t.space.at(6.0)).justify_end().child(if busy > 0 { pill(t, &format!("{key}-count"), busy) } else { el().key(format!("{key}-count")) }));
    let toggle = id.clone();
    let keys = id.clone();
    head = head.on_press(move |s: &mut App| s.toggle_fold(&toggle)).on_key(move |s: &mut App, ev: &KeyEvent, _| section_key(s, &keys, ev));
    let mut sec = col().key(format!("{key}-section")).role("group").label(w.name.clone()).w_full().gap(1.0).child(head);
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
        sec = sec.child(row().key(format!("{key}-removed")).w_full().pl(indent).h(t.control.sm).center_y().child(text("Earlier, removed workspace").font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0)));
    }
    if folded {
        return sec;
    }
    let tasks = s.tasks_in(&w.id);
    if tasks.is_empty() {
        // Before the daemon's list arrives: a quiet placeholder, not "No tasks yet".
        let waiting = s.daemon != Daemon::Absent && !s.tasks_loaded;
        let none = text(if waiting { "Loading…" } else { "No tasks yet" }).key(format!("{key}-none")).font(t.ty.sm).color(t.ink_faint());
        return sec.child(row().h(t.metrics.row).pl(indent).center_y().child(none));
    }
    let all = s.show_all.contains(&w.id) || tasks.len() <= SECTION_ROWS + 1;
    let shown: Vec<&&Task> = if all { tasks.iter().collect() } else { tasks.iter().take(SECTION_ROWS).collect() };
    let mut list = col().key(format!("{key}-list")).role("listbox").label(format!("Tasks in {}", w.name)).w_full().gap(1.0);
    let n = shown.len();
    for (i, task) in shown.into_iter().enumerate() {
        list = list.child(task_row(s, t, task, i, n, s.open.as_ref() == Some(&task.id)));
    }
    sec = sec.child(list);
    if tasks.len() > SECTION_ROWS + 1 {
        let more = id.clone();
        let words = if all { "Show fewer".to_owned() } else { format!("Show {} more", tasks.len() - SECTION_ROWS) };
        let toggle_more = SidebarItem::new(format!("{key}-more"), words)
            .icon(if all { &crate::ui::icons::CHEVRON_UP } else { &crate::ui::icons::CHEVRON_DOWN })
            .on_press(move |s: &mut App| {
                if !s.show_all.remove(&more) {
                    s.show_all.insert(more.clone());
                }
            })
            .build(t)
            .role("button");
        sec = sec.child(toggle_more);
    }
    sec
}

/// Keys on a section's header: Left folds it, Right opens it, ⌥⌘↑ and ⌥⌘↓ move it.
fn section_key(s: &mut App, id: &str, ev: &KeyEvent) -> bool {
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

/// A workspace's menu: Rename, Reveal in Finder, Locate (when its folder is not found), Move up and down, Remove.
fn workspace_menu(s: &App, t: &Theme, w: &Workspace, layer: &mut Vec<El>) -> El {
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
        .danger_item(DropdownMenuItem::new("Remove…").on_select(move |s: &mut App| s.ask_remove_workspace(&e)));
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
