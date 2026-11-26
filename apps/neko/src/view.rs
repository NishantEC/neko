//! The window: a sidebar and the content beside it, under one 52 point header band (docs/knowledge/app-shell.md).
//!
//! The band runs across the sidebar and the content on the traffic lights' centre line. Every part of it that is not a control drags
//! the window. The lights' room comes from Rune (`Cx::titlebar_inset`, zero in full screen). Everything drawn here comes from
//! rune-ui's copies (`crate::ui`) and the theme file; the only colours are the theme's.

use rune::{Cx, El, col, el, row, text};
use rune_ui_style::weight;

use std::rc::Rc;

use neko_protocol::TaskStatus;
use rune_ui_style::ButtonVariant;

use crate::icons;
use crate::state::{App, Daemon};
use crate::ui::agent::{PressTx, Status, chip, icon_button, mark};
use crate::ui::base::{Size, Theme, glyph, paint};
use crate::ui::button::Button;
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
    let mut root = row().key(ROOT).w_full().h_full().fill(t.c.page.bg).color(t.ink());
    if shown {
        root = root.child(sidebar(s, &t, lights)).child(el().w(1.0).h_full().fill(t.c.line));
    }
    let mut layer = Vec::new();
    let page = if s.settings_open {
        crate::settings_view::settings_view(s, &t)
    } else {
        match s.open_task() {
            Some(_) => crate::task_view::task_view(s, &t, &mut layer),
            None => home(s, &t),
        }
    };
    let content = col().key("content").role("main").label(main_label(s)).grow(1.0).min_w(0.0).h_full().child(content_band(s, &t, if shown { 0.0 } else { lights })).child(page);
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

fn sidebar(s: &App, t: &Theme, lights: f32) -> El {
    let band = row().key("sidebar-band").w_full().h(BAND).center_y().pr(8.0).window_drag().child(lights_room(lights)).child(row().grow(1.0).h_full()).child(toggle_button(t, true));
    // A sidebar row that acts: the same alignment as Home and the tasks, as Mail's and Notes' sidebars have.
    let new = SidebarItem::new("new-task", "New task").icon(&icons::COMPOSE).on_press(|s: &mut App| s.go_home_and_focus()).build(t).role("button");
    let new = Tooltip::new("New task").keys("⌘N").wrap(new);
    let home = SidebarItem::new("home", "Home").icon(&crate::ui::icons::HOME).selected(s.open.is_none() && !s.settings_open).on_press(|s: &mut App| s.go_home()).build(t);
    let (today, earlier) = s.grouped();
    let mut body = col().w_full().px(t.space.at(2.0)).gap(t.space.at(1.0)).child(new).child(home);
    body = body.child(task_list(s, t, "today", "Today", &today));
    if !earlier.is_empty() {
        body = body.child(task_list(s, t, "earlier", "Earlier", &earlier));
    }
    let list = col().key("sidebar-scroll").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body.pb(t.space.at(4.0)));
    let mut column = paint(col().key("sidebar").role("navigation").label("Sidebar").w(SIDEBAR).h_full(), &t.mat.slab.base).child(band).child(list);
    // No money at the foot: what runs cost is in the task's context ring, for whoever opens it.
    if let Some(line) = connection_line(s, t) {
        column = column.child(row().w_full().px(t.space.at(4.0)).py(t.space.at(3.0)).child(line));
    }
    column
}

/// The mark a task's row shows for its state (rune-ui's step marks: a spinner while it works, the warning while it needs you), and
/// the words that say it.
pub fn status_mark(s: TaskStatus) -> (Status, &'static str) {
    match s {
        TaskStatus::New => (Status::Pending, "New"),
        TaskStatus::Working => (Status::Running, "Working"),
        TaskStatus::NeedsYou => (Status::Waiting, "Needs you"),
        TaskStatus::Done => (Status::Done, "Done"),
        TaskStatus::Failed => (Status::Error, "Failed"),
        TaskStatus::Stopped => (Status::Skipped, "Stopped"),
        TaskStatus::Interrupted => (Status::Skipped, "Interrupted"),
    }
}

/// A quiet line at the sidebar's foot while the daemon is not there.
fn connection_line(s: &App, t: &Theme) -> Option<El> {
    let words = match &s.daemon {
        Daemon::Starting => "Starting Neko…",
        Daemon::Lost(_) => "Reconnecting to Neko…",
        Daemon::Connected | Daemon::Absent => return None,
    };
    Some(text(words).key("connection").role("status").font(t.ty.sm).color(t.ink_faint()))
}

/// One labelled group of tasks: a list the arrow keys move through (one Tab stop), Return opening the row with focus.
fn task_list(s: &App, t: &Theme, key: &str, label: &str, tasks: &[&neko_protocol::Task]) -> El {
    let heading = row().h(28.0).px(t.space.at(2.0)).pt(t.space.at(3.0)).center_y().child(text(label.to_owned()).role("heading").level(2).font(weight(t.ty.xs, t.ty.medium)).color(t.ink_faint()));
    let mut list = col().key(format!("{key}-list")).role("listbox").label(if key == "today" { "Tasks from today".to_owned() } else { "Earlier tasks".to_owned() }).w_full().gap(1.0).focus_group();
    if tasks.is_empty() {
        let none = text("No tasks yet").key(format!("{key}-none")).font(t.ty.sm).color(t.ink_faint());
        return col().w_full().child(heading).child(row().h(t.metrics.row).px(t.space.at(2.0)).center_y().child(none));
    }
    let n = tasks.len();
    for (i, task) in tasks.iter().enumerate() {
        let open = s.open.as_ref() == Some(&task.id);
        let id = task.id.clone();
        let (status, word) = status_mark(task.status);
        // The open task is selected, and the list's Tab stop (with none open, focus enters at the first row); Return opens a row.
        let row_el = SidebarItem::new(format!("task-{}", task.id.0), task.title.clone())
            .lead(mark(t, status, t.control.icon))
            .list_row(i + 1, n)
            .description(word)
            .selected(open)
            .on_press(move |s: &mut App| s.open_task_id(&id))
            .build(t);
        list = list.child(row_el);
    }
    col().w_full().child(heading).child(list)
}

/// The content's band: the sidebar's button when it is folded, and the open task's title.
fn content_band(s: &App, t: &Theme, lights: f32) -> El {
    let mut band = row().key("content-band").w_full().h(BAND).center_y().px(12.0).gap(t.space.at(2.0)).window_drag();
    if !s.sidebar_shown() {
        band = band.child(lights_room(lights).w(if lights > 0.0 { (lights - 4.0).max(0.0) } else { 0.0 })).child(toggle_button(t, false));
    }
    if let Some(task) = s.open_task().filter(|_| !s.settings_open) {
        band = band.child(text(task.title.clone()).key("band-title").role("heading").level(1).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink());
    }
    band
}

fn home(s: &App, t: &Theme) -> El {
    let greeting = col()
        .w_full()
        .gap(t.space.at(2.0))
        .items_center()
        .child(text(s.greeting()).key("greeting").role("heading").level(1).font(weight(t.ty.xxl, t.ty.medium)).color(t.ink()))
        .child(text("What should Neko do?").key("greeting-sub").font(t.ty.md).color(t.ink_muted()));
    let composer = PromptInput::new("composer", &s.draft, |s: &mut App| &mut s.draft)
        .placeholder("Describe a task")
        .spellcheck(true)
        .tool(folder_chip(s, t))
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
    if let Some(n) = &s.notice {
        stack = stack.child(text(n.clone()).key("notice").role("status").font(t.ty.sm).color(t.ink_muted()).wrap().align(rune::Align::Center));
    }
    // A little above the middle, where the eye lands.
    col().w_full().grow(1.0).min_h(0.0).items_center().justify_center().px(t.space.at(8.0)).pb(BAND * 1.5).child(stack)
}

/// The folder a new task works in: one quiet chip that opens the Mac's open panel, and a way back to no folder.
fn folder_chip(s: &App, t: &Theme) -> El {
    let name = s.folder.as_deref().map(|f| std::path::Path::new(f).file_name().map_or_else(|| f.to_owned(), |n| n.to_string_lossy().into_owned()));
    let (words, label) = match &name {
        Some(n) => (n.clone(), format!("Folder: {n}. Choose another folder")),
        None => ("Choose folder".to_owned(), "Choose a folder for this task. Without one it gets a scratch folder of its own".to_owned()),
    };
    let start = s.folder.clone();
    let face = chip(t, "folder-chip", None)
        .role("button")
        .label(label)
        .help(s.folder.clone().unwrap_or_else(|| "Without a folder, the task gets a scratch folder of its own".to_owned()))
        .focusable(true)
        .child(glyph(&crate::ui::icons::FOLDER, t.control.icon_sm, t.ink_muted()))
        .child(text(words).font(t.ty.sm).color(if name.is_some() { t.ink() } else { t.ink_muted() }).truncate().max_w(220.0).min_w(0.0).a11y_hidden())
        .on_press_tx(move |_: &mut App, tx: &mut rune::Tx| {
            let options = rune::FileDialogOptions { title: "Choose a folder for this task".into(), directory: start.as_ref().map(std::path::PathBuf::from), ..Default::default() };
            tx.open_folder(options, |s: &mut App, answer: rune::FileDialogResult, _: &mut rune::Tx| {
                if let Ok(Some(path)) = answer {
                    s.choose_folder(Some(path.to_string_lossy().into_owned()));
                }
            });
        });
    let mut r = row().key("folder").gap(t.space.at(1.0)).center_y().child(face);
    if s.folder.is_some() {
        let clear: PressTx<App> = Rc::new(|s: &mut App, _: &mut rune::Tx| s.choose_folder(None));
        r = r.child(icon_button(t, "folder-clear", "Work without a folder", &crate::ui::icons::CLOSE, None, false, Some(clear)));
    }
    r
}
