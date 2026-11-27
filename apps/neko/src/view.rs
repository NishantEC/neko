//! The window: a sidebar and the content beside it, under one 52 point header band (docs/knowledge/app-shell.md).
//!
//! The band runs across the sidebar and the content on the traffic lights' centre line. Every part of it that is not a control drags
//! the window. The lights' room comes from Rune (`Cx::titlebar_inset`, zero in full screen). Everything drawn here comes from
//! rune-ui's copies (`crate::ui`) and the theme file; the only colours are the theme's.

use rune::{Color, Cx, El, Shadow, col, el, row, text};
use rune_ui_style::weight;

use std::rc::Rc;

use neko_protocol::TaskStatus;
use rune_ui_style::ButtonVariant;

use crate::icons;
use crate::state::{App, Confirm, Effect};
use crate::ui::agent::{PressTx, Status, icon_button};
use crate::ui::alert_dialog::AlertDialog;
use crate::ui::base::{Shape, Size, States, Theme, glyph, paint, paint_state};
use crate::ui::button::Button;
use crate::ui::dialog::Dialog;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::input::Input;
use crate::ui::tooltip::Tooltip;

/// The header band's height, shared by the sidebar and the content.
pub const BAND: f32 = 52.0;
/// The sidebar's width.
pub const SIDEBAR: f32 = 256.0;
/// The reading column of a task, and Home's composer.
pub const COLUMN: f32 = 680.0;
/// The key of the window's root (its size is the window's).
pub const ROOT: &str = "neko-root";

thread_local! {
    /// The appearance and Reduce Motion of the last view built, for the tests.
    pub static LOOK: std::cell::Cell<(bool, bool)> = const { std::cell::Cell::new((false, false)) };
}

/// The theme file (`themes/neko.json`): House's warm paper on a warm window ground, deep warm charcoal in dark, and one identity
/// colour (a deep teal) for focus and work.
pub const THEME: &str = include_str!("../themes/neko.json");

/// The theme in the system's appearance, with the person's Reduce Motion. House's type scale, its identity colour and its scrim are
/// in the theme file.
pub fn theme(dark: bool, reduced_motion: bool) -> Theme {
    Theme::from_json(THEME, dark).expect("the theme file is valid (tested)").with_reduced_motion(reduced_motion)
}

/// The fill of a raised surface: the floating surface in light; in dark, a small step above the sidebar's panel (which is itself a
/// step above the ground), so the light from above (its top edge and a faint gradient, [`raised`]) does the lifting rather than a
/// lighter fill.
pub fn raised_fill(t: &Theme) -> Color {
    if t.dark { rune_ui_style::mix(t.c.surface.bg, Color::WHITE, 0.105) } else { t.c.floating.bg }
}

/// A raised surface (a busy cat's tile, the open sidebar row, a receipt, a card that asks): the theme's raised material, lit from
/// above in dark.
pub fn raised(t: &Theme, e: El) -> El {
    let e = paint(e, &t.mat.raised.base).fill(raised_fill(t));
    if t.dark {
        e.gradient(Color::WHITE.alpha(0.035), Color::WHITE.alpha(0.0)).box_shadow(Shadow { y: 1.0, inset: true, color: Color::WHITE.alpha(0.09), ..Shadow::default() })
    } else {
        e
    }
}

/// A well: a surface pressed into the page (the person's words, a receipt's files, Home's lists).
pub fn well(t: &Theme, e: El) -> El {
    paint(e, &t.mat.well.base)
}

/// How long ago, in a few words ("12 min", "3 h", "2 days").
pub fn since(s: &App, ms: i64) -> String {
    crate::needs_view::waited(ms, s.now_ms)
}

/// The colour a task's state is drawn in, or none for a state that needs no colour (done, new, stopped). Work is the identity
/// colour; waiting on the person amber; a failure red.
pub fn state_ink(t: &Theme, st: TaskStatus) -> Option<Color> {
    match st {
        TaskStatus::Working => Some(t.c.accent.fg),
        TaskStatus::NeedsYou | TaskStatus::Asked => Some(t.c.warning.fg),
        TaskStatus::Failed => Some(t.c.danger.fg),
        // A thread that waits is not busy: the second ink, still.
        _ => None,
    }
}

/// The tasks that are live: working first, then those that need the person, newest first inside each.
pub fn live_tasks(s: &App) -> Vec<&neko_protocol::Task> {
    let mut v: Vec<&neko_protocol::Task> = s.tasks.iter().filter(|t| t.is_thread() && matches!(t.status, TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Asked | TaskStatus::Waiting)).collect();
    v.sort_by_key(|t| (t.status != TaskStatus::Working, std::cmp::Reverse(t.created_ms)));
    v
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
    // A warm window ground, the sidebar an inset panel on it, the content on paper.
    let mut root = row().key(ROOT).w_full().h_full().fill(t.c.surface.bg).color(t.ink());
    if shown {
        // Above the content beside it (while it comes in it paints over it, never under it), but under a dialog's scrim: `raise` put
        // it above the whole window, so an open dialog left the sidebar undimmed.
        root = root.child(crate::sidebar::panel(&t, crate::sidebar::sidebar(s, &t, lights, &mut layer)).paint_order(1));
    }
    let page = if s.settings_open {
        crate::settings_view::settings_view(s, &t, &mut layer)
    } else if let Some(cat) = s.page_cat() {
        crate::cat_view::cat_view(s, &t, cat, &mut layer)
    } else if s.needs_open {
        crate::needs_view::needs_view(s, &t)
    } else if s.rounds_open {
        crate::rounds_view::rounds_view(s, &t, &mut layer)
    } else if s.briefing_open {
        crate::briefing_view::briefing_view(s, &t)
    } else {
        match s.open_task() {
            Some(_) => crate::task_view::task_view(s, &t, &mut layer),
            None => crate::home_view::home(s, &t, &mut layer),
        }
    };
    let page = page_swap(page);
    let content = col().key("content").role("main").label(main_label(s)).grow(1.0).min_w(0.0).h_full().fill(t.c.page.bg).child(content_band(s, &t, if shown { 0.0 } else { lights }, &mut layer)).child(page);
    dialogs(s, &t, &mut layer);
    crate::ui::base::overlay_root(root.child(content), layer)
}

/// How a page of the content (Home, a task's conversation, Review, a cat's page, Needs you, Settings) comes and goes: it fades in, and when another page replaces it, it is gone in
/// that frame. Rune's default fades a removed element out over a quarter second, so two pages that swap in one press were both
/// drawn, one over the other, until it ended.
pub fn page_swap(page: El) -> El {
    page.transition(rune::Transition::opacity()).transition_motion(rune::Curve::tween(0.18, rune::Easing::EASE_OUT), rune::Anim::INSTANT)
}

fn main_label(s: &App) -> String {
    if s.settings_open {
        return "Settings".to_owned();
    }
    if s.needs_open {
        return "Needs you".to_owned();
    }
    if s.rounds_open {
        return "Rounds".to_owned();
    }
    if s.briefing_open {
        return "Briefing".to_owned();
    }
    if let Some(c) = s.page_cat() {
        return c.name.clone();
    }
    s.open_task().map_or_else(|| "Home".to_owned(), |t| t.title.clone())
}

/// The room the traffic lights take at the band's left, and a little air after them.
pub fn lights_room(lights: f32) -> El {
    el().w(if lights > 0.0 { lights + 8.0 } else { 12.0 }).h_full()
}

pub fn toggle_button(t: &Theme, shown: bool) -> El {
    let (key, words) = if shown { ("sidebar-hide", "Hide sidebar") } else { ("sidebar-show", "Show sidebar") };
    let b = Button::new(key, words).variant(ButtonVariant::Ghost).size(Size::Sm).icon_only(&icons::SIDEBAR).on_press(|s: &mut App| s.toggle_sidebar()).build(t);
    Tooltip::new(words).keys("⌃⌘S").wrap(b)
}

/// The mark a task's row shows for its state (rune-ui's step marks: a spinner while it works, the warning while it needs you), and
/// the words that say it.
pub fn status_mark(s: TaskStatus) -> (Status, &'static str) {
    match s {
        TaskStatus::New => (Status::Pending, "New"),
        TaskStatus::Working => (Status::Running, "Working"),
        // Drawn as the still ring (`subagents_view::status_glyph`): the kit has no mark for it.
        TaskStatus::Waiting => (Status::Pending, "Waiting"),
        TaskStatus::NeedsYou | TaskStatus::Asked => (Status::Waiting, "Needs you"),
        TaskStatus::Done => (Status::Done, "Done"),
        TaskStatus::Failed => (Status::Error, "Failed"),
        TaskStatus::Stopped => (Status::Stopped, "Stopped"),
        TaskStatus::Interrupted => (Status::Skipped, "Interrupted"),
    }
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

/// Under this content width (points) the band drops the task's folder line, so the title keeps its room.
pub const NARROW_BAND: f32 = 700.0;
/// The least room the band's title keeps before anything else does.
pub const TITLE_MIN: f32 = 140.0;

/// The content's band: the sidebar's button when it is folded; in a task, its cat (a press opens the cat's page), its title, the
/// run counter, its folder and branch with Reveal in Finder, and its menu.
fn content_band(s: &App, t: &Theme, lights: f32, layer: &mut Vec<El>) -> El {
    let mut band = row().key("content-band").w_full().h(BAND).center_y().px(12.0).gap(t.space.at(2.5)).window_drag();
    if !s.sidebar_shown() {
        band = band.child(lights_room(lights).w(if lights > 0.0 { (lights - 4.0).max(0.0) } else { 0.0 })).child(toggle_button(t, false));
    }
    let Some(task) = s.open_task().filter(|_| !s.settings_open && !s.needs_open && !s.rounds_open && !s.briefing_open && s.page_cat().is_none()) else { return band };
    let sub = task.subagent.is_some();
    if let Some(c) = s.cat(&task.cat).filter(|_| !sub) {
        let id = c.id.clone();
        let st = (task.status == TaskStatus::Working).then_some(TaskStatus::Working);
        let face = row().key("task-cat").role("button").label(format!("Cat: {}", c.name)).help(format!("Open {}'s page", c.name)).focusable(true).padding(2.0).radius(t.radius.md).round_as(t);
        band = band.child(paint_state(face, &t.mat.row, States::default()).child(crate::faces::portrait_in(t, "task-cat-face", &c.face, 24.0, st, t.c.page.bg)).on_press(move |s: &mut App| s.open_cat_page(&id)));
    }
    // The title always wins the room: in a narrow content area the folder line goes first and the workspace's name shortens.
    let room = s.width.map_or(f32::MAX, |w| w - if s.sidebar_shown() { SIDEBAR + 2.0 * crate::sidebar::INSET + 1.0 } else { 0.0 });
    let narrow = room < NARROW_BAND;
    band = if sub {
        // A subagent's band says whose conversation it is: `Fix login / Subagent 1`, the thread's title a way back.
        band.child(crate::subagents_view::crumb(s, t, task, narrow))
    } else {
        band.child(text(task.title.clone()).key("band-title").role("heading").level(1).font(weight(t.ty.lg, t.ty.semibold)).color(t.ink()).truncate().min_w(TITLE_MIN).shrink())
    };
    band = band.child(row().grow(1.0).min_w(t.space.at(4.0)).h_full());
    let wide = s.width.unwrap_or(1200.0) >= 1000.0;
    if let Some(c) = crate::task_view::run_counter(s, t).filter(|_| wide) {
        band = band.child(c);
    }
    if let Some(w) = s.workspace_of(&task.workspace) {
        band = band.child(
            text(w.name.clone())
                .key("task-workspace")
                .role("note")
                .label(format!("Project: {}", w.name))
                .font(weight(t.ty.sm, t.ty.medium))
                .color(t.ink_muted())
                .truncate()
                // With room, the folder path (cut in its middle) gives way before the workspace's name does.
                .min_w(if narrow { 0.0 } else { 72.0 })
                .shrink()
                .max_w(if narrow { 110.0 } else { 180.0 }),
        );
    }
    if !narrow
        && let Some(place) = task_folder(s, t, room)
    {
        band = band.child(place);
    }
    // A subagent is removed with its thread, never alone: it has no menu.
    if sub { band } else { band.child(task_menu(s, t, task, layer)) }
}

/// The task's menu at the band's right: Remove task…, off while its agent works.
fn task_menu(s: &App, t: &Theme, task: &neko_protocol::Task, layer: &mut Vec<El>) -> El {
    let open = s.menu_open.as_deref() == Some("task-more");
    let face: PressTx<App> = Rc::new(|s: &mut App, _: &mut rune::Tx| s.menu_open = if s.menu_open.as_deref() == Some("task-more") { None } else { Some("task-more".into()) });
    let trigger = icon_button(t, "task-more-button", "Thread options", &crate::ui::icons::MORE, None, false, Some(face)).expanded(open);
    let id = task.id.clone();
    let menu = DropdownMenu::new("task-more")
        .trigger(trigger)
        .open(open)
        .min_width(180.0)
        .on_close(|s: &mut App| s.menu_open = None)
        .item({
            let id = task.id.clone();
            let any = s.book().is_some_and(|b| b.runs.iter().any(|r| r.undo.as_ref().is_some_and(neko_protocol::RunUndo::ready)));
            DropdownMenuItem::new("All changes").disabled(!any).on_select(move |s: &mut App| s.open_review(&id, None))
        })
        .item(DropdownMenuItem::new("Remove thread…").danger(true).disabled(s.open_is_working()).on_select(move |s: &mut App| s.ask_remove_task(&id)));
    menu_named(menu, t, layer, "Thread options", open)
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
                1 => " Its thread stays readable in the sidebar under “Earlier, removed project”.".to_owned(),
                n => format!(" Its {n} threads stay readable in the sidebar under “Earlier, removed project”."),
            };
            AlertDialog::new("remove-workspace", format!("Remove {name} from Neko?"))
                .description(format!("Nothing on disk is touched: the folder and its files stay where they are.{tasks} No new thread can be made in it until you add the folder again."))
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action("Remove", |s: &mut App| s.confirmed())
                .open(true)
                .return_focus(format!("ws-{id}"))
                .build(t, layer);
        }
        Some(Confirm::RemoveTask(task)) => {
            let title = s.tasks.iter().find(|x| &x.id == task).map_or_else(String::new, |x| x.title.clone());
            AlertDialog::new("remove-task", "Remove this thread?")
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
        Some(Confirm::UndoAnyway { redo, files, .. }) => {
            let verb = if *redo { "Redo" } else { "Undo" };
            let named = match files.len() {
                0 => String::new(),
                1..=5 => files.join(", "),
                n => format!("{} and {} more", files[..4].join(", "), n - 4),
            };
            let which = if files.len() == 1 { "This file changed" } else { "These files changed" };
            AlertDialog::new("undo-anyway", format!("{verb} over files that changed since?"))
                .description(format!("{which} after the run: {named}. Neko keeps the folder as it is now as a checkpoint of its own first, so Redo can bring it back."))
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action(format!("{verb} anyway"), |s: &mut App| s.confirmed())
                .open(true)
                .build(t, layer);
        }
        Some(Confirm::RetireCat(id)) => {
            let name = s.cat(id).map_or_else(String::new, |c| c.name.clone());
            let n = s.tasks.iter().filter(|x| x.is_thread() && &x.cat == id).count();
            let tasks = match n {
                0 => String::new(),
                1 => format!(" Its thread stays readable, still {name}'s."),
                n => format!(" Its {n} threads stay readable, still {name}'s."),
            };
            AlertDialog::new("retire-cat", format!("Retire {name}?"))
                .description(format!("{name} takes no new threads after this, and no project's new threads go to it.{tasks} You can bring {name} back from Settings ▸ Cats."))
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action("Retire", |s: &mut App| s.confirmed())
                .open(true)
                .return_focus("cat-retire")
                .build(t, layer);
        }
        Some(Confirm::RemoveRound(id)) => {
            let name = s.round(id).map_or_else(String::new, |r| r.name.clone());
            AlertDialog::new("remove-round", format!("Remove the round {name}?"))
                .description("It stops starting by itself, and its history goes. The threads it made stay where they are.")
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action("Remove", |s: &mut App| s.confirmed())
                .open(true)
                .return_focus("rounds-new")
                .build(t, layer);
        }
        Some(Confirm::RemovePlugin(id)) => {
            let name = s.plugin(id).map_or_else(String::new, |p| p.name.clone());
            AlertDialog::new("remove-plugin", format!("Remove {name}?"))
                .description("It stops being offered to your cats, and its settings and secrets go, the secrets from the Keychain too. You can add it again later.")
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action("Remove", |s: &mut App| s.confirmed())
                .open(true)
                .return_focus(format!("plugin-{id}-remove"))
                .build(t, layer);
        }
        Some(Confirm::ClearUndo) => {
            AlertDialog::new("clear-undo", "Clear the undo history?")
                .description("Every checkpoint goes: no run can be reviewed, undone or redone after this. Your files and commits are not touched.")
                .destructive()
                .cancel("Cancel", |s: &mut App| s.confirm = None)
                .action("Clear", |s: &mut App| s.confirmed())
                .open(true)
                .return_focus("settings-clear-undo")
                .build(t, layer);
        }
        None => {}
    }
    crate::rounds_view::dialog(s, t, layer);
    crate::plugins_view::dialog(s, t, layer);
    if let Some((id, edit)) = &s.naming {
        let renaming = id.as_ref().and_then(|i| s.cat(i)).map(|c| c.name.clone());
        let field = Input::new("cat-name-input", edit, |s: &mut App| &mut s.naming.as_mut().expect("naming").1).name("Name").placeholder("miso").on_submit(|s: &mut App| s.finish_naming()).build(t);
        let cancel = Button::new("cat-name-cancel", "Cancel").variant(ButtonVariant::Secondary).on_press(|s: &mut App| s.naming = None).build(t);
        let save = Button::new("cat-name-save", if renaming.is_some() { "Rename" } else { "Make cat" }).disabled(edit.text.trim().is_empty()).on_press(|s: &mut App| s.finish_naming()).build(t);
        let (title, words, back) = match &renaming {
            Some(n) => (format!("Rename {n}"), "Its threads keep it; the new name shows everywhere, and reaches the agent from the next run.".to_owned(), "cat-rename".to_owned()),
            None => ("New cat".to_owned(), "A name of up to 40 characters, not another cat's. You can give it a face, a purpose and standing rules on its page.".to_owned(), "settings-new-cat".to_owned()),
        };
        let mut dialog = Dialog::new("cat-name", title).description(words);
        if renaming.is_none() {
            // The face a new cat gets: the first no active cat has (docs/knowledge/cats.md).
            let taken: Vec<&str> = s.active_cats().iter().map(|c| c.face.as_str()).collect();
            let face = neko_protocol::cats::FACES.iter().find(|f| !taken.contains(f)).copied().unwrap_or(neko_protocol::cats::FACES[0]);
            let said = format!("It gets the {} face. You can change it on its page.", crate::faces::words(face));
            dialog = dialog.child(
                row().key("cat-name-face").role("note").label(said.clone()).w_full().gap(t.space.at(3.0)).center_y().child(crate::faces::portrait(t, "cat-name-face-img", face, 40.0)).child(text(said).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).shrink().a11y_hidden()),
            );
        }
        dialog.child(field).footer(vec![cancel, save]).width(400.0).open(true).return_focus(back).on_close(|s: &mut App| s.naming = None).build(t, layer);
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
fn task_folder(s: &App, t: &Theme, room: f32) -> Option<El> {
    let (folder, branch) = working_folder(s)?;
    let shown = crate::task_view::show_path(&folder, "", s.home.as_deref());
    let label = match &branch {
        Some(b) => format!("Working folder: {shown}, on branch {b}"),
        None => format!("Working folder: {shown}"),
    };
    let mut place = row()
        .key("task-folder")
        .min_w(0.0)
        .shrink()
        .role("group")
        .label(label)
        .help(folder.clone())
        .gap(t.space.at(1.5))
        .center_y()
        .child(glyph(&crate::ui::icons::FOLDER, t.control.icon_sm, t.ink_faint()))
        // The title leads: the path is quiet (the small step, the faintest ink that reads) and short, cut in its middle.
        .child(text(shown).key("task-folder-path").font(t.ty.xs).color(t.ink_faint()).truncate_middle().min_w(0.0).shrink().max_w(if room < 900.0 { 112.0 } else if s.sidebar_shown() { 168.0 } else { 220.0 }).a11y_hidden());
    // Under 900 points the branch has no room to be read whole (the receipt names it too): the path stays, quietly.
    if let Some(b) = branch.filter(|_| room >= 900.0) {
        place = place.child(glyph(&crate::ui::icons::COMMIT, t.control.icon_sm, t.ink_faint())).child(text(b).key("task-folder-branch").font(crate::ui::agent::mono(t, t.ty.xs)).color(t.ink_faint()).truncate_middle().max_w(112.0).a11y_hidden());
    }
    let reveal: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut rune::Tx| {
        let _ = tx.reveal_path(&folder);
    });
    Some(place.child(icon_button(t, "task-folder-reveal", "Reveal in Finder", &crate::ui::icons::FOLDER_OPEN, None, false, Some(reveal))))
}

/// A rune-ui DropdownMenu, its open surface named for assistive technology (the kit's own leaves the modal surface unnamed; the
/// name is added on the built element, never in the copy).
pub fn menu_named(menu: DropdownMenu<App>, t: &Theme, layer: &mut Vec<El>, name: &str, open: bool) -> El {
    let before = layer.len();
    let trigger = menu.build(t, layer);
    if open && layer.len() > before
        && let Some(surface) = layer.pop()
    {
        layer.push(surface.label(name.to_owned()));
    }
    trigger
}
