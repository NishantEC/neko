//! New thread (docs/knowledge/threads.md, docs/knowledge/design.md): the screen the app opens on. One question, the composer under it,
//! one row of chips under that (the project, where in it the thread works, the branch a worktree starts from) and, when there is news,
//! the Briefing's line. No greeting, no house, no list of what is going on: the sidebar holds those.
//!
//! The question sits a little above the middle of the content, so the eye lands on it. The composer is four lines high at rest and
//! grows to twelve. Return starts the thread and opens it; ⌘Return starts it and leaves this screen open with an empty box.

use rune::{El, KeyEvent, Tx, col, el, row, text};
use rune_ui_style::weight;

use crate::icons;
use crate::state::App;
use crate::ui::agent::chip;
use crate::ui::base::{Shape, Theme, glyph};
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::prompt_input::{Phase, PromptInput};
use crate::view::{menu_named, well};

/// The composer's width, and so the screen's column.
pub const WIDTH: f32 = 640.0;

/// "What should we work on?", or "…in acme-web?" once a project with a folder is chosen.
pub fn headline(s: &App) -> String {
    let project = s.chosen().filter(|w| w.folder.is_some()).map(|w| w.name.clone());
    match (s.adding_folder(), project) {
        (true, _) => format!("What should we work on in {}?", s.chip_project_name()),
        (false, Some(name)) => format!("What should we work on in {name}?"),
        _ => "What should we work on?".to_owned(),
    }
}

/// Where a new thread goes: a pop-up menu (the current choice checked) with No project, the projects and "Add folder…". The chip
/// names a folder that is still being added at once.
fn project_chip(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let options = s.picker_options();
    let current = options.iter().find(|o| o.1.as_deref() == Some(s.workspace.as_str())).map_or_else(|| "No project".to_owned(), |o| o.0.clone());
    let name = s.chip_project_name();
    let adding = s.adding_folder();
    let help = if adding {
        "Adding this folder as a project".to_owned()
    } else {
        match s.chosen().and_then(|w| w.folder.clone()) {
            Some(f) => crate::task_view::show_path(&f, "", s.home.as_deref()),
            None => "No project: each thread gets a scratch folder of its own".to_owned(),
        }
    };
    let missing = !adding && s.chosen().is_some_and(|w| w.missing);
    let has_folder = adding || s.chosen().is_some_and(|w| w.folder.is_some());
    let face = chip(t, "workspace-picker", None)
        .role("button")
        .label(if adding { format!("Project: {name}, adding") } else { format!("Project: {name}") })
        .help(help)
        .expanded(s.picker_open)
        .focusable(true)
        .child(glyph(if has_folder { &crate::ui::icons::FOLDER } else { &icons::COMPOSE }, t.control.icon_sm, if missing { t.c.warning.fg } else { t.ink_muted() }))
        .child(text(name).font(t.ty.sm).color(t.ink()).truncate().max_w(200.0).min_w(0.0).a11y_hidden())
        .child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.picker_open = !s.picker_open);
    let mut menu = DropdownMenu::new("workspace-menu").trigger(face).open(s.picker_open).min_width(260.0).on_close(|s: &mut App| s.picker_open = false).label("New thread in");
    for (label, id) in options {
        let pick = label.clone();
        let item = DropdownMenuItem::new(label.clone()).on_select(move |s: &mut App| s.pick(&pick));
        menu = match id {
            Some(_) => menu.item(item.checked(!adding && label == current)),
            None => menu.separator().item(item.icon(&icons::FOLDER_PLUS)),
        };
    }
    row().key("workspace").center_y().child(menu_named(menu, t, layer, "Projects", s.picker_open))
}

/// A repository only: "New worktree" (the default: the thread works in a copy of its own, on its own branch) or "In the folder".
fn place_chip(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let in_folder = s.new_thread.in_folder;
    let name = if in_folder { "In the folder" } else { "New worktree" };
    let help = if in_folder { "The thread works in your own checkout. Its first change asks." } else { "The thread works in a copy of its own on its own branch. Your checkout is not touched." };
    let face = chip(t, "place-picker", None)
        .role("button")
        .label(format!("Where it works: {name}"))
        .help(help)
        .expanded(s.new_thread.place_open)
        .focusable(true)
        .child(text(name).font(t.ty.sm).color(t.ink()).a11y_hidden())
        .child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.new_thread.place_open = !s.new_thread.place_open);
    let menu = DropdownMenu::new("place-menu")
        .trigger(face)
        .open(s.new_thread.place_open)
        .min_width(240.0)
        .on_close(|s: &mut App| s.new_thread.place_open = false)
        .label("Where the thread works")
        .item(DropdownMenuItem::new("New worktree").checked(!in_folder).on_select(|s: &mut App| s.choose_place(false)))
        .item(DropdownMenuItem::new("In the folder").checked(in_folder).on_select(|s: &mut App| s.choose_place(true)));
    row().key("place").center_y().child(menu_named(menu, t, layer, "Where the thread works", s.new_thread.place_open))
}

/// The branch a worktree starts from, named as git names it; the checkout's own is the default.
fn branch_chip(s: &App, t: &Theme, layer: &mut Vec<El>) -> Option<El> {
    let list = s.branch_list()?;
    let shown = s.start_branch().unwrap_or_else(|| "HEAD".to_owned());
    let face = chip(t, "branch-picker", None)
        .role("button")
        .label(format!("Starts from the branch {shown}"))
        .help("The branch this thread's copy starts from")
        .expanded(s.new_thread.branch_open)
        .focusable(true)
        .child(glyph(&icons::BRANCH, t.control.icon_sm, t.ink_muted()))
        .child(text(shown.clone()).font(t.ty.sm).color(t.ink()).truncate().max_w(200.0).min_w(0.0).a11y_hidden())
        .child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.new_thread.branch_open = !s.new_thread.branch_open);
    let mut menu = DropdownMenu::new("branch-menu").trigger(face).open(s.new_thread.branch_open).min_width(220.0).on_close(|s: &mut App| s.new_thread.branch_open = false).label("Start from");
    for b in &list.names {
        let (label, pick) = (b.clone(), if Some(b) == list.current.as_ref() { None } else { Some(b.clone()) });
        menu = menu.item(DropdownMenuItem::new(label).checked(*b == shown).on_select(move |s: &mut App| s.choose_branch(pick.clone())));
    }
    Some(row().key("branch").center_y().child(menu_named(menu, t, layer, "Branches", s.new_thread.branch_open)))
}

/// The composer: four lines high at rest, twelve at most. ⌘Return is caught before the field sees it, so the field's own Return
/// (start the thread and open it) is unchanged.
fn composer(s: &App, t: &Theme) -> El {
    let busy = s.creating.is_some() || s.new_thread.held.is_some();
    let field = PromptInput::new("composer", &s.draft, |s: &mut App| &mut s.draft)
        .placeholder("Describe the work")
        .spellcheck(true)
        .rows(4, 12)
        .phase(if busy { Phase::Disabled } else { Phase::Ready })
        .on_submit(|s: &mut App| s.submit())
        .build(t);
    col().key("composer-block").w_full().on_key_preview(|s: &mut App, k: &KeyEvent, _: &mut Tx| {
        if k.key == rune::Key::Enter && k.mods.cmd && !(k.mods.shift || k.mods.alt || k.mods.ctrl) {
            s.submit_in_background();
            return true;
        }
        false
    })
    .child(field)
}

/// New thread, the whole page.
pub fn new_thread(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let mut chips = row().key("new-thread-chips").w_full().gap(t.space.at(2.0)).center_y().flex_wrap().pl(t.space.at(1.0)).child(project_chip(s, t, layer));
    if s.branch_list().is_some() {
        chips = chips.child(place_chip(s, t, layer));
        if !s.new_thread.in_folder
            && let Some(b) = branch_chip(s, t, layer)
        {
            chips = chips.child(b);
        }
    }
    let mut page = col()
        .key("new-thread")
        .w_full()
        .max_w(WIDTH)
        .gap(t.space.at(6.0))
        .child(text(headline(s)).key("new-thread-headline").role("heading").level(1).font(weight(t.ty.xl, t.ty.semibold)).color(t.ink()).wrap().min_w(0.0).align(rune::Align::Center))
        .child(col().w_full().gap(t.space.at(3.0)).child(composer(s, t)).child(chips));
    if let Some(line) = crate::view::slow_line(s, t) {
        page = page.child(line);
    }
    if let Some(n) = &s.notice {
        page = page.child(text(n.clone()).key("notice").role("status").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    if let Some(line) = briefing_line(s, t) {
        page = page.child(el().h(t.space.at(2.0))).child(line);
    }
    // The page is centred in the room there is, a little above the middle, and scrolls when the window is too small for it.
    col().key("new-thread-scroll").w_full().grow(1.0).min_h(0.0).items_center().justify_center().px(t.space.at(10.0)).pb(t.space.at(14.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}

/// One quiet card under the chips when the briefing has news: "While you were away: 3 finished, 1 needs you", and a way in. Nothing
/// when there is nothing new or it was read.
pub fn briefing_line(s: &App, t: &Theme) -> Option<El> {
    let c = s.briefing_news()?;
    let words = format!("While you were away: {}", c.headline());
    let open = crate::ui::button::Button::<App>::new("home-briefing-open", "Open briefing").variant(rune_ui_style::ButtonVariant::Secondary).size(rune_ui_style::Size::Sm).on_press(|s: &mut App| s.open_briefing()).build(t);
    let r = row()
        .key("home-briefing")
        .role("group")
        .label(words.clone())
        .w_full()
        .px(t.space.at(4.0))
        .py(t.space.at(2.5))
        .gap(t.space.at(3.0))
        .center_y()
        .radius(t.radius.xl)
        .round_as(t);
    Some(
        well(t, r)
            .fill(t.c.input.bg)
            .child(glyph(&icons::BRIEFING, t.control.icon, t.ink_muted()))
            .child(text(words).key("home-briefing-words").font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink().grow(1.0).a11y_hidden())
            .child(open),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headline_names_the_project_once_there_is_a_folder() {
        let a = App::default();
        assert_eq!(headline(&a), "What should we work on?");
    }
}
