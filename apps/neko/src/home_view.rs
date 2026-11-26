//! Home (docs/knowledge/design.md): the day, a greeting that says what the house is doing, the composer addressed to the cat who
//! takes the task, then the house (each cat as a tile with its state and what it is on) and what needs the person.
//!
//! The composer comes first so it is in sight without scrolling at every window size. The house shows each cat at 76 points while
//! few are busy; once more than three are busy or there are more than six cats, it folds to one row of small tiles and the lists
//! under it say what each is on.

use neko_protocol::{Cat, Task, TaskStatus};
use rune::{El, Tx, col, el, row, text};
use rune_ui_style::{tabular, weight};

use crate::faces::{portrait, portrait_in};
use crate::icons;
use crate::state::{App, CatState};
use crate::ui::agent::chip;
use crate::ui::base::{Shape, States, Theme, glyph, paint_state};
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::kbd::Kbd;
use crate::ui::prompt_input::{Phase, PromptInput};
use crate::view::{SIDEBAR, live_tasks, menu_named, raised, raised_fill, since, state_ink, status_mark, well};

/// Home's column.
const PAGE: f32 = 880.0;
/// A cat's portrait on a full tile, and on a small one.
const TILE_FACE: f32 = 76.0;
const SMALL_FACE: f32 = 40.0;
/// A face in the cat picker's chip.
const FACE_CHIP: f32 = 20.0;

/// The live task a cat is on, if any: working first, then one that needs the person.
fn doing<'a>(s: &'a App, cat: &Cat) -> Option<&'a Task> {
    live_tasks(s).into_iter().find(|t| t.cat == cat.id)
}

/// Names as a sentence lists them ("miso", "miso and tofu", "miso, tofu and nori"; past three, "4 cats").
fn names(list: &[&str]) -> String {
    match list {
        [] => String::new(),
        [a] => (*a).to_owned(),
        [a, b] => format!("{a} and {b}"),
        [a, b, c] => format!("{a}, {b} and {c}"),
        more => format!("{} cats", more.len()),
    }
}

/// What the house is doing, in one line under the greeting.
pub fn house_line(s: &App) -> String {
    let cats = s.active_cats();
    let mut working = Vec::new();
    let mut waiting = Vec::new();
    for c in &cats {
        match s.cat_state(&c.id) {
            CatState::Working(_) => working.push(c.name.as_str()),
            CatState::NeedsYou => waiting.push(c.name.as_str()),
            CatState::Napping => {}
        }
    }
    let verb = |l: &[&str], one: &str, many: &str| if l.len() == 1 { one.to_owned() } else { many.to_owned() };
    match (working.len(), waiting.len()) {
        (0, 0) if cats.is_empty() => "Make a cat to give it something to do.".to_owned(),
        (0, 0) => "Everyone is napping. Give a cat something to do.".to_owned(),
        (w, 0) if w == cats.len() => format!("{} {} working.", names(&working), verb(&working, "is", "are")),
        (_, 0) => format!("{} {} working. The others are free.", names(&working), verb(&working, "is", "are")),
        (0, _) => format!("{} {} waiting for you.", names(&waiting), verb(&waiting, "is", "are")),
        _ => format!("{} {} working; {} {} waiting for you.", names(&working), verb(&working, "is", "are"), names(&waiting), verb(&waiting, "is", "are")),
    }
}

/// The state of a cat as its tile says it, the colour it says it in, and the state its portrait shows.
fn tile_state(t: &Theme, task: Option<&Task>) -> (&'static str, rune::Color, Option<TaskStatus>) {
    match task.map(|x| x.status) {
        Some(TaskStatus::Working) => ("Working", t.c.accent.fg, Some(TaskStatus::Working)),
        Some(st) => ("Needs you", state_ink(t, st).unwrap_or(t.ink_faint()), Some(st)),
        None => ("Napping", t.ink_faint(), None),
    }
}

/// One cat as a tile. A busy cat's tile is raised and opens its task; a napping cat's is flat, and a press addresses the composer to
/// it ("Ask tofu to…") with the keyboard in the composer.
fn tile(s: &App, t: &Theme, cat: &Cat, small: bool) -> El {
    let task = doing(s, cat);
    let (words, ink, st) = tile_state(t, task);
    let face = if small { SMALL_FACE } else { TILE_FACE };
    let pad = if small { t.space.at(2.0) } else { t.space.at(3.0) };
    let line = task.map_or_else(|| "Free for a task".to_owned(), |x| x.title.clone());
    let mut card = row()
        .key(format!("cat-tile-{}", cat.id))
        .role("button")
        .label(format!("{}: {}", cat.name, words.to_lowercase()))
        .description(line.clone())
        .help(if task.is_some() { "Open its task".to_owned() } else { format!("Ask {} to do the next task", cat.name) })
        .focusable(true)
        .grow(1.0)
        .min_w(0.0)
        .padding(pad)
        .gap(if small { t.space.at(2.0) } else { t.space.at(3.5) })
        .items_start()
        // Concentric with the portrait's corner: its radius plus the room between them.
        .radius(crate::faces::radius(face) + pad + (crate::faces::ringed(face) - face) / 2.0)
        .round_as(t);
    card = if task.is_some() { paint_state(raised(t, card), &t.mat.row, States::default()) } else { paint_state(card.fill(t.c.input.bg), &t.mat.row, States::default()) };
    let under = if task.is_some() { raised_fill(t) } else { t.c.input.bg };
    // The words start level with the portrait's top, so every tile in a row has its name on one line.
    let ring = (crate::faces::ringed(face) - face) / 2.0;
    let mut words_col = col()
        .grow(1.0)
        .min_w(0.0)
        .pt(if small { ring } else { ring + 2.0 })
        .gap(if small { 0.0 } else { 2.0 })
        .child(text(cat.name.clone()).font(weight(if small { t.ty.base } else { t.ty.lg }, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).a11y_hidden())
        .child(text(words).font(weight(t.ty.sm, t.ty.semibold)).color(ink).a11y_hidden());
    if !small {
        words_col = words_col.child(text(line).font(t.ty.sm).color(if task.is_some() { t.ink_muted() } else { t.ink_faint() }).max_lines(2).wrap().min_w(0.0).pt(2.0).a11y_hidden());
    }
    card = card.child(portrait_in(t, &cat.face, face, st, under)).child(words_col);
    match task {
        Some(x) => {
            let id = x.id.clone();
            card.on_press(move |s: &mut App| s.open_task_id(&id))
        }
        None => {
            let id = cat.id.clone();
            card.on_press_tx(move |s: &mut App, tx: &mut Tx| {
                s.pick_cat(Some(id.clone()));
                tx.focus("composer-input".to_owned(), true);
            })
        }
    }
}

/// The house: the cats as tiles, three a row where there is room; one row of small tiles once many are busy.
fn house(s: &App, t: &Theme, room: f32) -> (El, bool) {
    let cats = s.active_cats();
    let busy = cats.iter().filter(|c| s.cat_state(&c.id) != CatState::Napping).count();
    let small = busy > 3 || cats.len() > 6;
    let mut grid = col().key("house").role("group").label("The house").w_full().gap(t.space.at(3.0));
    if small {
        // As many as fit at 136 points each, in rows of equal tiles.
        let per_row = ((room + t.space.at(2.0)) / (136.0 + t.space.at(2.0))).floor().max(2.0) as usize;
        for chunk in cats.chunks(per_row) {
            let mut r = row().w_full().gap(t.space.at(2.0)).items_stretch();
            for c in chunk {
                r = r.child(tile(s, t, c, true));
            }
            for _ in chunk.len()..per_row.min(cats.len()) {
                r = r.child(el().grow(1.0));
            }
            grid = grid.child(r);
        }
    } else {
        let per_row = if room >= 900.0 || cats.len() <= 3 { 3 } else { 2 };
        for chunk in cats.chunks(per_row) {
            let mut r = row().w_full().gap(t.space.at(3.0)).items_stretch();
            for c in chunk {
                r = r.child(tile(s, t, c, false));
            }
            // A short last row keeps the tiles' width.
            for _ in chunk.len()..per_row {
                r = r.child(el().grow(1.0));
            }
            grid = grid.child(r);
        }
    }
    (grid, small)
}

/// Who the next task goes to: a chip in the composer's footer beside the workspace's, as raised as it. At rest it shows the picked
/// cat's face and name, or for "Any cat" the free cats' faces overlapping in one row; a press opens the menu, the current choice
/// checked.
fn cat_picker(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let picked = s.cat_pick().and_then(|id| s.cat(id));
    let name = picked.map_or_else(|| "Any cat".to_owned(), |c| c.name.clone());
    let faces = match picked {
        Some(c) => Some(portrait(t, &c.face, FACE_CHIP)),
        // Before the house is known there are no faces to show: the chip is its words alone.
        None if s.active_cats().is_empty() => None,
        None => {
            // The free cats (else any), up to three, overlapping, each cut from the next by the chip's own colour.
            let free: Vec<&Cat> = s.active_cats().into_iter().filter(|c| s.cat_state(&c.id) == CatState::Napping).take(3).collect();
            let shown: Vec<&Cat> = if free.is_empty() { s.active_cats().into_iter().take(3).collect() } else { free };
            let (step, ring) = (FACE_CHIP - 6.0, 1.5);
            let width = FACE_CHIP + step * shown.len().saturating_sub(1) as f32 + 2.0 * ring;
            let cut = FACE_CHIP + 2.0 * ring;
            let mut stack = el().size(width, cut);
            for (i, c) in shown.iter().enumerate().rev() {
                // Each face sits in a disc of the chip's own surface, so the one under it is cut where they meet.
                stack = stack.child(el().size(cut, cut).radius(cut * 0.3).fill(raised_fill(t)).child(portrait(t, &c.face, FACE_CHIP).absolute(ring, ring)).absolute(step * i as f32, 0.0));
            }
            Some(stack)
        }
    };
    let mut face = chip(t, "cat-picker", None)
        .role("button")
        .label(format!("Cat: {name}"))
        .help("Who does the next task")
        .expanded(s.cat_picker_open)
        .focusable(true);
    if let Some(faces) = faces {
        face = face.child(faces.a11y_hidden());
    }
    let face = face
        .child(text(name.clone()).font(weight(t.ty.sm, t.ty.medium)).color(t.ink()).truncate().max_w(140.0).min_w(0.0).a11y_hidden())
        .child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint()))
        .on_press(|s: &mut App| s.cat_picker_open = !s.cat_picker_open);
    let mut menu = DropdownMenu::new("cat-menu").trigger(face).open(s.cat_picker_open).min_width(220.0).on_close(|s: &mut App| s.cat_picker_open = false).label("New task for");
    menu = menu.item(DropdownMenuItem::new("Any cat").checked(picked.is_none()).on_select(|s: &mut App| s.pick_cat(None))).separator();
    for c in s.active_cats() {
        let id = c.id.clone();
        menu = menu.item(DropdownMenuItem::new(c.name.clone()).checked(picked.is_some_and(|p| p.id == c.id)).on_select(move |s: &mut App| s.pick_cat(Some(id.clone()))));
    }
    row().key("cat").child(menu_named(menu, t, layer, "Cats", s.cat_picker_open))
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
        let item = DropdownMenuItem::new(label.clone()).on_select(move |s: &mut App| s.pick(&pick));
        menu = match id {
            Some(_) => menu.item(item.checked(label == current)),
            None => menu.separator().item(item.icon(&icons::FOLDER_PLUS)),
        };
    }
    row().key("workspace").center_y().child(menu_named(menu, t, layer, "Workspaces", s.picker_open))
}

/// The keys under the composer.
fn hint(t: &Theme) -> El {
    row()
        .key("composer-hint")
        .gap(t.space.at(1.5))
        .center_y()
        .child(Kbd::new("hint-return", "Return").build(t))
        .child(text("to add it").font(t.ty.sm).color(t.ink_faint()))
        .child(el().w(t.space.at(2.0)))
        .child(Kbd::new("hint-newline", "Shift Return").build(t))
        .child(text("for a new line").font(t.ty.sm).color(t.ink_faint()))
}

/// The composer, addressed to a cat: "Ask tofu to…", generous at rest; the cat and the workspace in its footer.
fn composer(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let to = s.cat_pick().and_then(|id| s.cat(id)).map_or_else(|| "a cat".to_owned(), |c| c.name.clone());
    let field = PromptInput::new("composer", &s.draft, |s: &mut App| &mut s.draft)
        .placeholder(format!("Ask {to} to…"))
        .spellcheck(true)
        .rows(3, 8)
        .tool(cat_picker(s, t, layer))
        .tool(workspace_picker(s, t, layer))
        .phase(if s.creating.is_some() { Phase::Disabled } else { Phase::Ready })
        .on_submit(|s: &mut App| s.submit())
        .build(t);
    col().key("composer-block").w_full().gap(t.space.at(2.0)).child(field).child(row().pl(t.space.at(1.0)).child(hint(t)))
}

/// A list row led by the cat's portrait: what, the cat and where (or what it asks), the state in words, how long. Opens the task.
fn list_row(s: &App, t: &Theme, task: &Task, list: &str, i: usize, n: usize, what: Option<String>) -> El {
    let id = task.id.clone();
    let name = s.cat_name(task);
    let place = s.workspace_of(&task.workspace).map_or_else(|| "General".to_owned(), |w| w.name.clone());
    let (_, word) = status_mark(task.status);
    let mut r = row()
        .key(format!("{list}-{}", task.id.0))
        .role("option")
        .label(task.title.clone())
        .help(task.title.clone())
        .description(format!("{name}, {}, {place}", word.to_lowercase()))
        .position_in_set(i + 1, n)
        .focus_selected(false)
        .focusable(true)
        .press_on_return()
        .w_full()
        .px(t.space.at(3.0))
        .py(t.space.at(2.5))
        .gap(t.space.at(3.0))
        .center_y()
        .radius(t.radius.lg)
        .round_as(t);
    r = paint_state(r, &t.mat.row, States::default());
    let sub = what.unwrap_or_else(|| format!("{name} · {place}"));
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(task.title.clone()).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).a11y_hidden())
        .child(text(sub).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).a11y_hidden());
    let ink = state_ink(t, task.status).unwrap_or(t.ink_faint());
    r.child(portrait_in(t, s.cat_face(&task.cat), 32.0, Some(task.status), t.c.input.bg))
        .child(words)
        .child(text(word).font(weight(t.ty.sm, t.ty.semibold)).color(ink).a11y_hidden())
        .child(text(since(s, task.created_ms)).font(tabular(t.ty.sm)).color(t.ink_faint()).w(56.0).align(rune::Align::End).a11y_hidden())
        .on_press(move |s: &mut App| s.open_task_id(&id))
}

/// A section of Home: its heading and count, its rows in one well.
fn section(t: &Theme, key: &str, title: &str, count: Option<usize>, body: El) -> El {
    let mut head = row().w_full().px(t.space.at(1.0)).gap(t.space.at(2.0)).center_y().child(text(title.to_owned()).key(format!("{key}-title")).role("heading").level(2).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()));
    if let Some(n) = count {
        head = head.child(text(n.to_string()).font(tabular(weight(t.ty.sm, t.ty.medium))).color(t.ink_faint()).a11y_hidden());
    }
    col().key(key.to_owned()).w_full().gap(t.space.at(3.0)).child(head).child(body)
}

fn list(t: &Theme, key: &str, title: &str, rows: Vec<El>) -> El {
    let l = col().key(format!("{key}-list")).role("listbox").label(title.to_owned()).focus_group().w_full().padding(t.space.at(1.0)).gap(1.0).radius(t.radius.lg + t.space.at(1.0)).round_as(t);
    well(t, l).fill(t.c.input.bg).children(rows)
}

pub fn home(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let live = live_tasks(s);
    let head = col()
        .key("home-head")
        .w_full()
        .gap(t.space.at(1.5))
        .child(text(crate::clock::date_line(s.now_ms, s.utc_offset_s)).key("home-date").font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_faint()))
        .child(text(s.greeting()).key("greeting").role("heading").level(1).font(t.ty.xxl).color(t.ink()))
        .child(text(house_line(s)).key("greeting-sub").font(weight(t.ty.lg, t.ty.regular)).color(t.ink_muted()).wrap().min_w(0.0));
    // One quiet line for the next round due, and nothing when there is none.
    let head = match next_round_line(s) {
        Some(words) => head.child(text(words.clone()).key("home-next-round").role("note").label(words).font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0).pt(t.space.at(1.0))),
        None => head,
    };
    let mut page = col().key("home").w_full().max_w(PAGE).gap(t.space.at(8.0)).child(col().w_full().gap(t.space.at(6.0)).child(head).child(composer(s, t, layer)));
    if let Some(line) = crate::view::slow_line(s, t) {
        page = page.child(line);
    }
    if let Some(n) = &s.notice {
        page = page.child(text(n.clone()).key("notice").role("status").font(t.ty.sm).color(t.ink_muted()).wrap());
    }
    let room = s.width.unwrap_or(1200.0) - if s.sidebar_shown() { SIDEBAR + 2.0 * crate::sidebar::INSET } else { 0.0 } - 2.0 * t.space.at(10.0);
    let (grid, small) = house(s, t, room);
    if !s.active_cats().is_empty() {
        page = page.child(section(t, "home-house", "The house", None, grid));
    }
    let needs: Vec<&Task> = live.iter().copied().filter(|x| x.status != TaskStatus::Working).collect();
    if !needs.is_empty() {
        let n = needs.len();
        let rows = needs
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let what = s.visible_needs().into_iter().find(|it| it.task == task.id).map(crate::needs_view::what);
                list_row(s, t, task, "home-needs", i, n, what.map(|w| format!("{} asks: {}", s.cat_name(task), lower_first(&w))))
            })
            .collect();
        page = page.child(section(t, "home-needs", "Needs you", Some(n), list(t, "home-needs", "Needs you", rows)));
    }
    // The full tiles say what each cat is on; the small ones do not, so the list does.
    let running: Vec<&Task> = live.iter().copied().filter(|x| x.status == TaskStatus::Working).collect();
    if small && !running.is_empty() {
        let n = running.len();
        let rows = running.iter().enumerate().map(|(i, task)| list_row(s, t, task, "home-progress", i, n, None)).collect();
        page = page.child(section(t, "home-progress", "In progress", Some(n), list(t, "home-progress", "In progress", rows)));
    }
    col().key("home-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.space.at(10.0)).pt(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page.pb(t.space.at(14.0)))
}

/// "Next round: mochi, Docs check, tomorrow 09:00": the round due soonest that will run by itself (None: no round will).
pub fn next_round_line(s: &App) -> Option<String> {
    let r = s.next_round()?;
    let at = crate::rounds::when(r.next_due_ms?, s.now_ms, &*s.zone());
    let cat = s.cat(&r.cat).map_or_else(|| "A cat".to_owned(), |c| c.name.clone());
    Some(format!("Next round: {cat}, {}, {at}", r.name))
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_read_as_a_sentence_lists_them() {
        assert_eq!(super::names(&["miso"]), "miso");
        assert_eq!(super::names(&["miso", "tofu"]), "miso and tofu");
        assert_eq!(super::names(&["miso", "tofu", "nori"]), "miso, tofu and nori");
        assert_eq!(super::names(&["a", "b", "c", "d"]), "4 cats");
    }
}
