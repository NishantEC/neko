//! The window: a sidebar and the content beside it, under one 52 point header band (docs/knowledge/app-shell.md).
//!
//! The band runs across the sidebar and the content on the traffic lights' centre line. Every part of it that is not a control drags
//! the window. The lights' room comes from Rune (`Cx::titlebar_inset`, zero in full screen). Everything drawn here comes from
//! rune-ui's copies (`crate::ui`) and the theme file; the only colours are the theme's.

use rune::{Cx, El, col, el, row, text};
use rune_ui_style::weight;

use std::rc::Rc;

use neko_protocol::{TaskStatus, Workspace};
use rune::{Key, KeyEvent};
use rune_ui_style::ButtonVariant;

use crate::icons;
use crate::state::{App, Confirm, Daemon, Effect, Pick, SECTION_ROWS};
use crate::ui::agent::{PressTx, Status, chip, icon_button, mark};
use crate::ui::alert_dialog::AlertDialog;
use crate::ui::base::{Shape, Size, States, Theme, glyph, paint, paint_state};
use crate::ui::button::Button;
use crate::ui::dialog::Dialog;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::input::Input;
use crate::ui::kbd::Kbd;
use crate::ui::prompt_input::{Phase, PromptInput};
use crate::ui::sidebar::SidebarItem;
use crate::ui::tooltip::Tooltip;

/// The header band's height, shared by the sidebar and the content.
pub const BAND: f32 = 52.0;
/// The sidebar's width.
pub const SIDEBAR: f32 = 248.0;
/// The reading column of a task, and Home's composer.
pub const COLUMN: f32 = 680.0;
/// The key of the window's root (its size is the window's).
pub const ROOT: &str = "neko-root";

thread_local! {
    /// The appearance and Reduce Motion of the last view built, for the tests.
    pub static LOOK: std::cell::Cell<(bool, bool)> = const { std::cell::Cell::new((false, false)) };
}

/// The theme file (`themes/neko.json`): rune-ui's default with Neko's knobs.
pub const THEME: &str = include_str!("../themes/neko.json");

/// The theme in the system's appearance, with the person's Reduce Motion.
pub fn theme(dark: bool, reduced_motion: bool) -> Theme {
    Theme::from_json(THEME, dark).expect("themes/neko.json is valid (tested)").with_reduced_motion(reduced_motion)
}

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let t = theme(cx.system_dark(), cx.is_reduced_motion());
    LOOK.with(|l| l.set((t.dark, t.reduced_motion)));
    cx.help_look(crate::ui::tooltip::look(&t));
    let full_screen = cx.is_full_screen();
    crate::FULL_SCREEN.with(|c| c.set(full_screen));
    let lights = if full_screen { 0.0 } else { cx.titlebar_inset() };
    if let Some([w, _]) = cx.layout_size(ROOT) {
        // The width is the window's; the state follows it in `poll` (`lib.rs`), never from the view.
        crate::WIDTH.with(|c| c.set(Some(w)));
    }
    let shown = s.sidebar_shown();
    let mut layer = Vec::new();
    let mut root = row().key(ROOT).w_full().h_full().fill(t.c.page.bg).color(t.ink());
    if shown {
        root = root.child(sidebar(s, &t, lights, &mut layer)).child(el().w(1.0).h_full().fill(t.c.line));
    }
    let page = if s.settings_open {
        crate::settings_view::settings_view(s, &t)
    } else {
        match s.open_task() {
            Some(_) => crate::task_view::task_view(s, &t, &mut layer),
            None => home(s, &t, &mut layer),
        }
    };
    let content = col().key("content").role("main").label(main_label(s)).grow(1.0).min_w(0.0).h_full().child(content_band(s, &t, if shown { 0.0 } else { lights }, &mut layer)).child(page);
    dialogs(s, &t, &mut layer);
    crate::ui::base::overlay_root(root.child(content), layer)
}

fn main_label(s: &App) -> String {
    if s.settings_open {
        return "Settings".to_owned();
    }
    s.open_task().map_or_else(|| "Home".to_owned(), |t| t.title.clone())
}

/// The room the traffic lights take at the band's left, and a little air after them.
fn lights_room(lights: f32) -> El {
    el().w(if lights > 0.0 { lights + 8.0 } else { 12.0 }).h_full()
}

fn toggle_button(t: &Theme, shown: bool) -> El {
    let (key, words) = if shown { ("sidebar-hide", "Hide sidebar") } else { ("sidebar-show", "Show sidebar") };
    let b = Button::new(key, words).variant(ButtonVariant::Ghost).size(Size::Sm).icon_only(&icons::SIDEBAR).on_press(|s: &mut App| s.toggle_sidebar()).build(t);
    Tooltip::new(words).keys("⌃⌘S").wrap(b)
}

fn sidebar(s: &App, t: &Theme, lights: f32, layer: &mut Vec<El>) -> El {
    let band = row().key("sidebar-band").w_full().h(BAND).center_y().pr(8.0).window_drag().child(lights_room(lights)).child(row().grow(1.0).h_full()).child(toggle_button(t, true));
    // A sidebar row that acts: the same alignment as Home and the tasks, as Mail's and Notes' sidebars have.
    let new = SidebarItem::new("new-task", "New task").icon(&icons::COMPOSE).on_press(|s: &mut App| s.new_task_here()).build(t).role("button");
    let new = Tooltip::new("New task").keys("⌘N").wrap(new);
    let home = SidebarItem::new("home", "Home").icon(&crate::ui::icons::HOME).selected(s.open.is_none() && !s.settings_open).on_press(|s: &mut App| s.go_home()).build(t);
    let mut body = col().w_full().px(t.space.at(2.0)).gap(t.space.at(1.0)).child(new).child(home);
    // Every section and row is one list for the keys: Up and Down move through sections and rows, a typed name goes to it, Tab
    // stops once (on the open task, else the first section). A section's own buttons are Tab stops of their own.
    let mut sections = col().key("workspaces").role("list").label("Workspaces").w_full().gap(t.space.at(1.0)).pt(t.space.at(2.0)).focus_group();
    for w in s.sections() {
        sections = sections.child(section(s, t, &w, layer));
    }
    body = body.child(sections);
    let add: PressTx<App> = Rc::new(|s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, Pick::Add { select: false }, tx));
    let add = SidebarItem::<App>::new("add-workspace", "Add workspace…").icon(&icons::FOLDER_PLUS).build(t).role("button").on_press_tx(move |s: &mut App, tx: &mut rune::Tx| add(s, tx));
    body = body.child(add);
    let list = col().key("sidebar-scroll").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body.pb(t.space.at(4.0)));
    let mut column = paint(col().key("sidebar").role("navigation").label("Sidebar").w(SIDEBAR).h_full(), &t.mat.slab.base).child(band).child(list);
    // No money at the foot: what runs cost is in the task's context ring, for whoever opens it.
    if let Some(line) = connection_line(s, t) {
        column = column.child(row().w_full().px(t.space.at(4.0)).py(t.space.at(3.0)).child(line));
    }
    column
}

/// What a section's header says about its workspace, after its name.
fn section_state(s: &App, w: &Workspace) -> String {
    let busy = s.busy_in(&w.id);
    let mut parts = Vec::new();
    if !w.is_active() {
        parts.push("Earlier, removed workspace".to_owned());
    } else if w.missing {
        parts.push("Folder not found".to_owned());
    }
    if busy > 0 {
        parts.push(format!("{busy} need{} you or working", if busy == 1 { "s" } else { "" }));
    }
    parts.join(", ")
}

/// One workspace in the sidebar: its header (fold, name, a quiet count; New task and its menu on hover or focus), what is wrong
/// with it, and its tasks, newest first, with "Show more" after eight.
fn section(s: &App, t: &Theme, w: &Workspace, layer: &mut Vec<El>) -> El {
    let id = w.id.clone();
    let folded = s.folded.contains(&w.id);
    let busy = s.busy_in(&w.id);
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
    head = head.child(chevron).child(text(w.name.clone()).font(weight(t.ty.sm, t.ty.medium)).color(if w.is_active() { t.ink_muted() } else { t.ink_faint() }).grow(1.0).truncate().min_w(0.0).a11y_hidden());
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
    // The quiet count sits last, at the same place in every section.
    head = head.child(
        row()
            .w(t.space.at(5.0))
            .justify_end()
            .child(text(if busy > 0 { busy.to_string() } else { String::new() }).key(format!("{key}-count")).font(rune_ui_style::tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden()),
    );
    let toggle = id.clone();
    let keys = id.clone();
    head = head.on_press(move |s: &mut App| s.toggle_fold(&toggle)).on_key(move |s: &mut App, ev: &KeyEvent, _| section_key(s, &keys, ev));
    let mut sec = col().key(format!("{key}-section")).role("group").label(w.name.clone()).w_full().gap(1.0).child(head);
    if w.is_active() && w.missing {
        let here = id.clone();
        let locate: PressTx<App> = Rc::new(move |s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, Pick::Locate(here.clone()), tx));
        let gone = id.clone();
        let indent = t.metrics.gap + t.control.icon_sm + t.space.at(1.5);
        let line = col()
            .key(format!("{key}-missing"))
            .w_full()
            .pl(indent - t.space.at(2.0))
            .gap(0.0)
            .child(row().pl(t.space.at(2.0)).h(t.control.sm).center_y().child(text("Folder not found").role("status").font(t.ty.sm).color(t.ink_faint())))
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
        let indent = t.metrics.gap + t.control.icon_sm + t.space.at(1.5);
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
        return sec.child(row().h(t.metrics.row).pl(t.metrics.gap + t.control.icon_sm + t.space.at(1.5)).center_y().child(none));
    }
    let all = s.show_all.contains(&w.id) || tasks.len() <= SECTION_ROWS + 1;
    let shown: Vec<&&neko_protocol::Task> = if all { tasks.iter().collect() } else { tasks.iter().take(SECTION_ROWS).collect() };
    let mut list = col().key(format!("{key}-list")).role("listbox").label(format!("Tasks in {}", w.name)).w_full().gap(1.0);
    let n = shown.len();
    for (i, task) in shown.into_iter().enumerate() {
        let open = s.open.as_ref() == Some(&task.id);
        let tid = task.id.clone();
        let (status, word) = status_mark(task.status);
        // The open task is selected, and the list's Tab stop; Return opens a row.
        let row_el = SidebarItem::new(format!("task-{}", task.id.0), task.title.clone())
            .lead(mark(t, status, t.control.icon))
            .list_row(i + 1, n)
            .description(word)
            .selected(open)
            .on_press(move |s: &mut App| s.open_task_id(&tid))
            .build(t);
        list = list.child(row_el);
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
    let (a, b, c, d, e, f) = (w.id.clone(), w.id.clone(), w.id.clone(), w.id.clone(), w.id.clone(), w.id.clone());
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
    let _ = f;
    menu_named(menu, t, layer, &format!("{} options", w.name), open)
}

/// The mark a task's row shows for its state (rune-ui's step marks: a spinner while it works, the warning while it needs you), and
/// the words that say it.
pub fn status_mark(s: TaskStatus) -> (Status, &'static str) {
    match s {
        TaskStatus::New => (Status::Pending, "New"),
        TaskStatus::Working => (Status::Running, "Working"),
        TaskStatus::NeedsYou | TaskStatus::Asked => (Status::Waiting, "Needs you"),
        TaskStatus::Done => (Status::Done, "Done"),
        TaskStatus::Failed => (Status::Error, "Failed"),
        TaskStatus::Stopped => (Status::Skipped, "Stopped"),
        TaskStatus::Interrupted => (Status::Skipped, "Interrupted"),
    }
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

/// The calm line when the daemon has not answered for a few seconds, with Retry (under the composer, Home's or the task's).
pub fn slow_line(s: &App, t: &Theme) -> Option<El> {
    if !s.daemon_slow() {
        return None;
    }
    let words = "Neko's background service has not started yet.";
    let retry = Button::new("daemon-retry", "Retry").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(|s: &mut App| s.retry_link()).build(t);
    Some(
        row()
            .key("daemon-slow")
            .gap(t.space.at(3.0))
            .center_y()
            .child(text(words).key("daemon-slow-words").role("status").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).shrink())
            .child(retry),
    )
}

/// The content's band: the sidebar's button when it is folded, and the open task's title.
fn content_band(s: &App, t: &Theme, lights: f32, layer: &mut Vec<El>) -> El {
    let mut band = row().key("content-band").w_full().h(BAND).center_y().px(12.0).gap(t.space.at(2.0)).window_drag();
    if !s.sidebar_shown() {
        band = band.child(lights_room(lights).w(if lights > 0.0 { (lights - 4.0).max(0.0) } else { 0.0 })).child(toggle_button(t, false));
    }
    if let Some(task) = s.open_task().filter(|_| !s.settings_open) {
        band = band.child(text(task.title.clone()).key("band-title").role("heading").level(1).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink());
        band = band.child(row().grow(1.0).min_w(t.space.at(4.0)).h_full());
        if let Some(w) = s.workspace_of(&task.workspace) {
            band = band.child(
                text(w.name.clone())
                    .key("task-workspace")
                    .role("note")
                    .label(format!("Workspace: {}", w.name))
                    .font(weight(t.ty.sm, t.ty.medium))
                    .color(t.ink_muted())
                    .truncate()
                    .min_w(72.0)
                    .max_w(if s.sidebar_shown() { 180.0 } else { 110.0 }),
            );
        }
        if let Some(place) = task_folder(s, t) {
            band = band.child(place);
        }
        band = band.child(task_menu(s, t, task, layer));
    }
    band
}

/// The task's menu at the band's right: Remove task…, off while its agent works.
fn task_menu(s: &App, t: &Theme, task: &neko_protocol::Task, layer: &mut Vec<El>) -> El {
    let open = s.menu_open.as_deref() == Some("task-more");
    let face: PressTx<App> = Rc::new(|s: &mut App, _: &mut rune::Tx| s.menu_open = if s.menu_open.as_deref() == Some("task-more") { None } else { Some("task-more".into()) });
    let trigger = icon_button(t, "task-more-button", "Task options", &crate::ui::icons::MORE, None, false, Some(face)).expanded(open);
    let id = task.id.clone();
    let menu = DropdownMenu::new("task-more")
        .trigger(trigger)
        .open(open)
        .min_width(180.0)
        .on_close(|s: &mut App| s.menu_open = None)
        .item(DropdownMenuItem::new("Remove task…").danger(true).disabled(s.open_is_working()).on_select(move |s: &mut App| s.ask_remove_task(&id)));
    menu_named(menu, t, layer, "Task options", open)
}

/// The panels that ask or say something: rename a workspace, remove one, remove a task, and why something was not done.
fn dialogs(s: &App, t: &Theme, layer: &mut Vec<El>) {
    if let Some((id, edit)) = &s.renaming {
        let name = s.workspace_of(id).map_or_else(String::new, |w| w.name.clone());
        let field = Input::new("rename-input", edit, |s: &mut App| &mut s.renaming.as_mut().expect("renaming").1).name("Name").on_submit(|s: &mut App| s.finish_rename()).build(t);
        let cancel = Button::new("rename-cancel", "Cancel").variant(ButtonVariant::Secondary).on_press(|s: &mut App| s.renaming = None).build(t);
        let save = Button::new("rename-save", "Rename").disabled(edit.text.trim().is_empty()).on_press(|s: &mut App| s.finish_rename()).build(t);
        Dialog::new("rename", format!("Rename {name}"))
            .description("Only the name in Neko changes. The folder keeps its own name.")
            .child(field)
            .footer(vec![cancel, save])
            .width(400.0)
            .open(true)
            .return_focus(format!("ws-{id}"))
            .on_close(|s: &mut App| s.renaming = None)
            .build(t, layer);
    }
    match &s.confirm {
        Some(Confirm::RemoveWorkspace(id)) => {
            let w = s.workspace_of(id);
            let name = w.map_or_else(String::new, |w| w.name.clone());
            let n = s.tasks_in(id).len();
            let tasks = match n {
                0 => String::new(),
                1 => " Its task stays readable in the sidebar under “Earlier, removed workspace”.".to_owned(),
                n => format!(" Its {n} tasks stay readable in the sidebar under “Earlier, removed workspace”."),
            };
            AlertDialog::new("remove-workspace", format!("Remove {name} from Neko?"))
                .description(format!("Nothing on disk is touched: the folder and its files stay where they are.{tasks} No new task can be made in it until you add the folder again."))
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action("Remove", |s: &mut App| s.confirmed())
                .open(true)
                .return_focus(format!("ws-{id}"))
                .build(t, layer);
        }
        Some(Confirm::RemoveTask(task)) => {
            let title = s.tasks.iter().find(|x| &x.id == task).map_or_else(String::new, |x| x.title.clone());
            AlertDialog::new("remove-task", "Remove this task?")
                .description(format!(
                    "“{title}” and its conversation leave Neko. Its worktree or scratch folder goes too when nothing in it is unsaved; a branch with commits keeps them. Neko deletes nothing with unsaved work."
                ))
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action("Remove", |s: &mut App| s.confirmed())
                .open(true)
                .return_focus("task-more-button")
                .build(t, layer);
        }
        None => {}
    }
    if let Some(r) = &s.refusal {
        let mut buttons = Vec::new();
        if let Some(path) = r.reveal.clone() {
            buttons.push(
                Button::new("refusal-reveal", "Reveal in Finder")
                    .variant(ButtonVariant::Secondary)
                    .on_press(move |s: &mut App| {
                        s.effects.push(Effect::Reveal(path.clone()));
                        s.refusal = None;
                    })
                    .build(t),
            );
        }
        buttons.push(Button::new("refusal-ok", "OK").on_press(|s: &mut App| s.refusal = None).build(t));
        Dialog::new("refusal", r.title.clone()).description(r.words.clone()).footer(buttons).width(400.0).open(true).on_close(|s: &mut App| s.refusal = None).build(t, layer);
    }
}

/// Where the open task works: its newest run's folder (else the folder chosen for it), and the run's branch.
pub fn working_folder(s: &App) -> Option<(String, Option<String>)> {
    let task = s.open_task()?;
    let run = s.book().and_then(|b| b.runs.iter().rev().find(|r| !r.workdir.is_empty()));
    match run {
        Some(r) => Some((r.workdir.clone(), r.branch.clone())),
        None => task.folder.clone().map(|f| (f, None)),
    }
}

/// The task's folder, once and quietly, at the band's right: the path shortened with `~` (cut in its middle when long), the branch
/// beside it, and Reveal in Finder.
fn task_folder(s: &App, t: &Theme) -> Option<El> {
    let (folder, branch) = working_folder(s)?;
    let shown = crate::task_view::show_path(&folder, "", s.home.as_deref());
    let label = match &branch {
        Some(b) => format!("Working folder: {shown}, on branch {b}"),
        None => format!("Working folder: {shown}"),
    };
    let mut place = row()
        .key("task-folder")
        .role("group")
        .label(label)
        .help(folder.clone())
        .gap(t.space.at(1.5))
        .center_y()
        .child(glyph(&crate::ui::icons::FOLDER, t.control.icon_sm, t.ink_faint()))
        // The title gives way first; the path is cut in its middle to a width that leaves the branch whole.
        .child(text(shown).key("task-folder-path").font(t.ty.sm).color(t.ink_muted()).truncate_middle().max_w(if s.sidebar_shown() { 320.0 } else { 150.0 }).a11y_hidden());
    if let Some(b) = branch {
        place = place.child(glyph(&crate::ui::icons::COMMIT, t.control.icon_sm, t.ink_faint())).child(text(b).key("task-folder-branch").font(crate::ui::agent::mono(t, t.ty.sm)).color(t.ink_muted()).truncate_middle().max_w(160.0).a11y_hidden());
    }
    let reveal: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut rune::Tx| {
        let _ = tx.reveal_path(&folder);
    });
    Some(place.child(icon_button(t, "task-folder-reveal", "Reveal in Finder", &crate::ui::icons::FOLDER_OPEN, None, false, Some(reveal))))
}

fn home(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let greeting = col()
        .w_full()
        .gap(t.space.at(2.0))
        .items_center()
        .child(text(s.greeting()).key("greeting").role("heading").level(1).font(weight(t.ty.xxl, t.ty.medium)).color(t.ink()))
        .child(text("What should Neko do?").key("greeting-sub").font(t.ty.md).color(t.ink_muted()));
    let composer = PromptInput::new("composer", &s.draft, |s: &mut App| &mut s.draft)
        .placeholder("Describe a task")
        .spellcheck(true)
        .tool(workspace_picker(s, t, layer))
        .phase(if s.creating.is_some() { Phase::Disabled } else { Phase::Ready })
        .on_submit(|s: &mut App| s.submit())
        .build(t);
    let hint = row()
        .key("composer-hint")
        .gap(t.space.at(1.5))
        .center_y()
        .child(Kbd::new("hint-return", "Return").build(t))
        .child(text("to add it").font(t.ty.sm).color(t.ink_faint()))
        .child(el().w(t.space.at(2.0)))
        .child(Kbd::new("hint-newline", "Shift Return").build(t))
        .child(text("for a new line").font(t.ty.sm).color(t.ink_faint()));
    let mut stack = col().key("home").w_full().max_w(COLUMN).gap(t.space.at(6.0)).items_center().child(greeting).child(col().w_full().gap(t.space.at(3.0)).items_center().child(composer).child(hint));
    if let Some(line) = slow_line(s, t) {
        stack = stack.child(line);
    }
    if let Some(n) = &s.notice {
        stack = stack.child(text(n.clone()).key("notice").role("status").font(t.ty.sm).color(t.ink_muted()).wrap().align(rune::Align::Center));
    }
    // A little above the middle, where the eye lands.
    col().w_full().grow(1.0).min_h(0.0).items_center().justify_center().px(t.space.at(8.0)).pb(BAND * 1.5).child(stack)
}

/// Where a new task goes: a pop-up menu in the composer's footer (as a Mac pop-up button is: the current choice checked), with
/// General, the workspaces and "Add folder…". (rune-ui's Select is not used: its options do not say their place in the list.)
fn workspace_picker(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let options = s.picker_options();
    let current = options.iter().find(|o| o.1.as_deref() == Some(s.workspace.as_str())).map_or_else(|| "General".to_owned(), |o| o.0.clone());
    let name = s.chosen().map_or_else(|| "General".to_owned(), |w| w.name.clone());
    let help = match s.chosen().and_then(|w| w.folder.clone()) {
        Some(f) => crate::task_view::show_path(&f, "", s.home.as_deref()),
        None => "General: each task gets a scratch folder of its own".to_owned(),
    };
    let missing = s.chosen().is_some_and(|w| w.missing);
    let face = chip(t, "workspace-picker", None)
        .role("button")
        .label(format!("Workspace: {name}"))
        .help(help)
        .expanded(s.picker_open)
        .focusable(true)
        .child(glyph(if s.chosen().is_some_and(|w| w.folder.is_some()) { &crate::ui::icons::FOLDER } else { &icons::COMPOSE }, t.control.icon_sm, if missing { t.c.warning.fg } else { t.ink_muted() }))
        .child(text(name).font(t.ty.sm).color(t.ink()).truncate().max_w(200.0).min_w(0.0).a11y_hidden())
        .child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.picker_open = !s.picker_open);
    let mut menu = DropdownMenu::new("workspace-menu").trigger(face).open(s.picker_open).min_width(260.0).on_close(|s: &mut App| s.picker_open = false).label("New task in");
    for (label, id) in options {
        let pick = label.clone();
        let row = DropdownMenuItem::new(label.clone()).on_select(move |s: &mut App| s.pick(&pick));
        menu = match id {
            Some(_) => menu.item(row.checked(label == current)),
            None => menu.separator().item(row.icon(&icons::FOLDER_PLUS)),
        };
    }
    row().key("workspace").center_y().child(menu_named(menu, t, layer, "Workspaces", s.picker_open))
}

/// A rune-ui DropdownMenu, its open surface named for assistive technology (the kit's own leaves the modal surface unnamed; the
/// name is added on the built element, never in the copy).
fn menu_named(menu: DropdownMenu<App>, t: &Theme, layer: &mut Vec<El>, name: &str, open: bool) -> El {
    let before = layer.len();
    let trigger = menu.build(t, layer);
    if open && layer.len() > before
        && let Some(surface) = layer.pop()
    {
        layer.push(surface.label(name.to_owned()));
    }
    trigger
}
