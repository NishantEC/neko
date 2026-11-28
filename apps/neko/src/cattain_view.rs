//! A Cattain's thread (docs/knowledge/cats.md, "Cattains"; design.md, "A Cattain"): where a cat's character lives. Its band leads with
//! its portrait, its name and its purpose; its turns are led by its face (the one place a face leads turns); a thread nobody has
//! written in yet opens on who it is. Its pane has Overview, Digest, Files and Profile. The state is `cattain.rs`.

use neko_protocol::cats::{self, CattainFile, MODELS};
use neko_protocol::{Cat, NeedsItem, PermissionMode, Task, TaskId, TaskStatus};
use rune::{El, Tx, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size, tabular, weight};

use std::rc::Rc;

use crate::cattain::file_words;
use crate::pane::Tab;
use crate::pane_view::{age, press_row, section};
use crate::state::App;
use crate::ui::agent::{PressTx, chip, icon_button, mono};
use crate::ui::base::{Shape, States, Theme, glyph, paint_state};
use crate::ui::button::Button;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::icons;
use crate::ui::input::Input;
use crate::ui::response::Response;
use crate::ui::textarea::Textarea;
use crate::view::menu_named;

/// How many of its Needs you the Overview lists before "n more".
const NEEDS_SHOWN: usize = 5;

/// What the Cattain is doing now, for its portrait's ring and dot: its own thread or a crew thread needs the person, one works, or
/// nothing.
pub fn cattain_status(s: &App, c: &Cat) -> Option<TaskStatus> {
    match s.cat_state(&c.id) {
        crate::state::CatState::NeedsYou => Some(TaskStatus::NeedsYou),
        crate::state::CatState::Working(_) => Some(TaskStatus::Working),
        crate::state::CatState::Napping => None,
    }
}

// ---- The thread -------------------------------------------------------------------------------------------------------------------

/// The band's lead in a Cattain's thread: its 28 point portrait in its state, its name as the title, then its purpose as the band's
/// quiet words, all on the band's line. The purpose gives way first when the room is short.
pub fn band_lead(s: &App, t: &Theme, c: &Cat, min_title: f32) -> El {
    let face = crate::faces::portrait_in(t, "band-face", &c.face, 28.0, cattain_status(s, c), t.c.page.bg).a11y_hidden();
    let mut lead = row()
        .key("band-cattain")
        .min_w(0.0)
        .shrink()
        .gap(t.space.at(2.5))
        .center_y()
        .child(face)
        .child(text(c.name.clone()).key("band-title").role("heading").level(1).font(weight(t.ty.lg, t.ty.semibold)).color(t.ink()).truncate().min_w(min_title.min(80.0)));
    let line = match (&c.purpose, c.is_active()) {
        (_, false) => Some("Retired: takes no new crew".to_owned()),
        (Some(p), true) if !p.is_empty() => Some(p.clone()),
        _ => None,
    };
    if let Some(p) = line {
        lead = lead.child(text(p).key("band-purpose").font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink());
    }
    lead
}

/// The head of a Cattain's turn: its face and name, on the reading edge.
pub fn turn_header(t: &Theme, key: &str, c: &Cat) -> El {
    row()
        .key(format!("{key}-who"))
        .gap(t.space.at(2.0))
        .center_y()
        .child(crate::faces::portrait(t, &format!("{key}-face"), &c.face, 20.0))
        .child(text(c.name.clone()).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink()).a11y_hidden())
}

/// A Cattain's thread before anyone has written in it: who it is, and where its files are.
pub fn intro(t: &Theme, c: &Cat) -> El {
    let mut who = col().items_center().gap(t.space.at(1.5)).child(crate::faces::portrait(t, "cattain-intro-face", &c.face, 72.0)).child(
        text(c.name.clone()).key("cattain-intro-name").role("heading").level(2).font(weight(t.ty.xl, t.ty.semibold)).color(t.ink()),
    );
    if let Some(p) = c.purpose.as_ref().filter(|p| !p.is_empty()) {
        who = who.child(text(p.clone()).key("cattain-intro-purpose").font(t.ty.base).color(t.ink_muted()).wrap().max_w(440.0).align(rune::Align::Center));
    }
    let words = format!("Write to {} here. It reads its rules, notes and digest at the start of every turn, and keeps them in its own folder.", c.name);
    who = who.child(text(words).key("cattain-intro-line").font(t.ty.sm).color(t.ink_faint()).wrap().max_w(440.0).align(rune::Align::Center).pt(t.space.at(2.0)));
    row().key("cattain-intro").role("note").label(format!("{}'s thread", c.name)).w_full().justify_center().pt(t.space.at(12.0)).pb(t.space.at(6.0)).child(who)
}

// ---- Overview ---------------------------------------------------------------------------------------------------------------------------

/// Whether a Needs you item is the Cattain's: from its own thread (or a kitten of it), or from a thread in its crew.
fn is_mine(s: &App, c: &Cat, item: &NeedsItem) -> bool {
    let root = s.root_of(&item.task);
    s.tasks.iter().find(|t| t.id == root).is_some_and(|t| t.cat.as_deref() == Some(c.id.as_str()))
}

/// Needs you (n): the first five from its crew and itself, with "n more" opening the Needs you page.
fn needs(s: &App, t: &Theme, c: &Cat) -> Option<El> {
    let items: Vec<&NeedsItem> = s.visible_needs().into_iter().filter(|i| !i.task.0.is_empty() && is_mine(s, c, i)).collect();
    if items.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    for item in items.iter().take(NEEDS_SHOWN) {
        let words = crate::needs_view::what(item);
        let own = c.thread.as_ref() == Some(&s.root_of(&item.task));
        let whose = if own { c.name.clone() } else { item.title.clone() };
        let since = age(s, item.since_ms);
        let key = format!("cattain-need-{}", item.id.replace(':', "-"));
        let task = item.task.clone();
        let line = press_row(t, key, format!("{whose}: {words}, {since}"), t.control.lg + 4.0)
            .child(el().size(8.0, 8.0).radius(4.0).fill(t.c.warning.fg).a11y_hidden())
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(1.0)
                    .child(text(words).font(t.ty.sm).color(t.ink()).truncate().min_w(0.0).a11y_hidden())
                    .child(text(whose).font(t.ty.xs).color(t.ink_faint()).truncate().min_w(0.0).a11y_hidden()),
            )
            .child(text(since).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden())
            .on_press(move |s: &mut App| s.open_task_id(&task));
        rows.push(line);
    }
    if items.len() > NEEDS_SHOWN {
        let more = items.len() - NEEDS_SHOWN;
        rows.push(row().child(Button::<App>::new("cattain-needs-more", format!("{more} more")).variant(ButtonVariant::Link).size(Size::Sm).on_press(|s: &mut App| s.open_needs()).build(t)));
    }
    Some(section(t, "cattain-needs", "Needs you", Some(items.len()), rows))
}

/// One crew thread: its status mark, its title, its project and its age.
fn crew_row(s: &App, t: &Theme, task: &Task) -> El {
    let project = s.workspace_of(&task.workspace).map(|w| s.project_name(w)).unwrap_or_default();
    let when = crate::threads::age(s.now_ms, s.utc_offset_s, task.activity_ms.max(task.created_ms));
    let words = crate::threads::state_words(task);
    let id = task.id.clone();
    press_row(t, format!("cattain-crew-{}", task.id.0), format!("{}, {words}, {project}, {when}", task.title), t.control.lg + 4.0)
        .child(row().size(16.0, 16.0).center().child(crate::status_mark::bare_mark(t, &format!("cattain-crew-{}-mark", task.id.0), task, crate::sidebar::panel_bg(t), 12.0)).a11y_hidden())
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(1.0)
                .child(text(task.title.clone()).font(t.ty.sm).color(t.ink()).truncate().min_w(0.0).a11y_hidden())
                .child(text(project).font(t.ty.xs).color(t.ink_faint()).truncate().min_w(0.0).a11y_hidden()),
        )
        .child(text(when).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden())
        .on_press(move |s: &mut App| s.open_task_id(&id))
}

/// Crew (n): the threads in its crew, those at work first; finished ones fold behind "Done (n)". New thread in its crew at the end.
fn crew(s: &App, t: &Theme, c: &Cat) -> El {
    let mut mine: Vec<&Task> = s.tasks.iter().filter(|x| x.listed() && x.crew_of() == Some(c.id.as_str()) && !x.archived).collect();
    mine.sort_by_key(|x| std::cmp::Reverse(x.activity_ms.max(x.created_ms)));
    let live = |x: &&Task| x.needs_you || matches!(x.status, TaskStatus::New | TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Asked | TaskStatus::Waiting | TaskStatus::Failed);
    let (open, done): (Vec<&Task>, Vec<&Task>) = mine.iter().copied().partition(live);
    let mut rows: Vec<El> = open.iter().map(|x| crew_row(s, t, x)).collect();
    if !done.is_empty() {
        let key = format!("cattain-crew-done-{}", c.id);
        let shown = s.is_open(&key);
        let k = key.clone();
        let fold = crate::ui::agent::FoldLine::<App>::new(key.clone(), format!("Done ({})", done.len()), shown).indent(false).on_toggle(move |s: &mut App| s.flip(&k));
        let inner = if shown { done.iter().map(|x| crew_row(s, t, x)).collect() } else { Vec::new() };
        rows.push(fold.build(t, inner));
    }
    if mine.is_empty() {
        rows.push(text(format!("No threads in {}'s crew yet. A thread you start here reads its CREW.md on every run.", c.name)).key("cattain-crew-empty").font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
    }
    if c.is_active() {
        let id = c.id.clone();
        rows.push(row().pt(t.space.at(1.0)).child(Button::<App>::new("cattain-new-crew", format!("New thread in {}'s crew", c.name)).variant(ButtonVariant::Secondary).size(Size::Sm).icon(&icons::PLUS).on_press(move |s: &mut App| s.new_crew_thread(&id)).build(t)));
    }
    section(t, "cattain-crew", "Crew", Some(mine.len()), rows)
}

/// When a file last changed, in a few words ("2m", "never").
fn changed(s: &App, f: Option<&CattainFile>) -> String {
    match f.and_then(|f| f.changed_ms) {
        Some(ms) => crate::threads::age(s.now_ms, s.utc_offset_s, ms),
        None => String::new(),
    }
}

/// Files: a strip of its four files, each a press to open in the Files tab.
fn files_strip(s: &App, t: &Theme, c: &Cat) -> El {
    let mut strip = row().key("cattain-files-strip").role("list").label("Files").w_full().gap(t.space.at(1.5)).flex_wrap();
    for name in cats::FILES {
        let f = s.cattains.files.get(&(c.id.clone(), (*name).to_owned()));
        let when = changed(s, f);
        let (id, n) = (c.id.clone(), (*name).to_owned());
        let mut face = chip(t, format!("cattain-file-chip-{}", name.trim_end_matches(".md").to_lowercase()), None)
            .role("button")
            .label(if when.is_empty() { format!("Open {name}") } else { format!("Open {name}, changed {when}") })
            .focusable(true)
            .child(text((*name).to_owned()).font(mono(t, t.ty.xs)).color(t.ink()).a11y_hidden());
        if !when.is_empty() {
            face = face.child(text(when).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden());
        }
        strip = strip.child(face.on_press(move |s: &mut App| s.open_cattain_file(&id, &n)));
    }
    section(t, "cattain-files", "Files", None, vec![strip])
}

/// Rounds: its rounds, each with its next time, and New round for it.
fn rounds(s: &App, t: &Theme, c: &Cat) -> El {
    let mine = s.rounds_of(&c.id);
    let zone = s.zone();
    let mut rows: Vec<El> = mine
        .iter()
        .map(|r| {
            let next = r.next_due_ms.map_or_else(|| if r.on { "Nothing is due".to_owned() } else { "Off".to_owned() }, |ms| format!("Next: {}", crate::rounds::when(ms, s.now_ms, &*zone)));
            press_row(t, format!("cattain-round-{}", r.id), format!("{}, {next}", r.name), t.control.md + 4.0)
                .child(glyph(&crate::icons::ROUNDS, t.control.icon_sm, t.ink_faint()))
                .child(text(r.name.clone()).font(t.ty.sm).color(t.ink()).truncate().min_w(0.0).grow(1.0).a11y_hidden())
                .child(text(next).font(t.ty.xs).color(t.ink_faint()).a11y_hidden())
                .on_press(|s: &mut App| s.open_rounds())
        })
        .collect();
    if mine.is_empty() {
        rows.push(text(format!("{} has no rounds. A round starts work by itself at the times you choose.", c.name)).key("cattain-no-rounds").font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
    }
    if c.is_active() {
        let id = c.id.clone();
        rows.push(row().pt(t.space.at(1.0)).child(Button::<App>::new("cat-new-round", "New round…").variant(ButtonVariant::Ghost).size(Size::Sm).icon(&icons::PLUS).on_press(move |s: &mut App| s.start_new_round(Some(&id))).build(t)));
    }
    section(t, "cattain-rounds", "Rounds", (!mine.is_empty()).then_some(mine.len()), rows)
}

/// The Overview of a Cattain's thread.
pub fn overview(s: &App, t: &Theme, c: &Cat) -> El {
    let mut page = col().key("pane-overview").w_full().gap(t.space.at(6.0));
    for part in [needs(s, t, c), Some(crew(s, t, c)), Some(files_strip(s, t, c)), Some(rounds(s, t, c))].into_iter().flatten() {
        page = page.child(part);
    }
    scroll(t, "pane-overview-scroll", page)
}

fn scroll(t: &Theme, key: &str, page: El) -> El {
    col().key(key.to_owned()).w_full().grow(1.0).min_h(0.0).px(t.space.at(4.0)).pt(t.space.at(3.0)).pb(t.space.at(8.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}

/// Markdown from a file, its headings one step down: the file's `# Plan` is a heading inside the pane, never the page's main one.
fn demoted(text: &str) -> String {
    let mut fence = false;
    let mut out = String::with_capacity(text.len() + 16);
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = !fence;
        }
        if !fence && trimmed.starts_with('#') && !trimmed.starts_with("######") && trimmed.trim_start_matches('#').starts_with([' ', '\n']) {
            out.push('#');
        }
        out.push_str(line);
    }
    out
}

// ---- Digest -----------------------------------------------------------------------------------------------------------------------------

/// Digest: DIGEST.md as Markdown, with how long ago it changed.
pub fn digest(s: &App, t: &Theme, c: &Cat) -> El {
    let f = s.cattains.files.get(&(c.id.clone(), cats::DIGEST.to_owned()));
    let mut page = col().key("cattain-digest").w_full().gap(t.space.at(3.0));
    match f {
        None => page = page.child(text("Reading DIGEST.md…").key("cattain-digest-loading").font(t.ty.sm).color(t.ink_faint())),
        Some(f) if f.text.trim().is_empty() => {
            page = page.child(
                text(format!("{} has not written a digest yet. It keeps one in DIGEST.md as it works: what needs you, what is new and what is in progress.", c.name))
                    .key("cattain-digest-empty")
                    .font(t.ty.sm)
                    .color(t.ink_muted())
                    .wrap()
                    .min_w(0.0),
            )
        }
        Some(f) => {
            let when = f.changed_ms.map(|ms| format!("Updated {}", crate::needs_view::waited(ms, s.now_ms))).unwrap_or_else(|| "As it was made".to_owned());
            let when = if when.starts_with("Updated") { format!("{when} ago") } else { when };
            page = page.child(text(when).key("cattain-digest-age").role("note").font(tabular(t.ty.xs)).color(t.ink_faint()));
            page = page.child(Response::new("cattain-digest-words", demoted(&f.text)).selectable(true).column(crate::pane::MAX_WIDTH).build(t));
            if f.cut {
                page = page.child(text("The rest is longer than Neko shows here. Reveal the file to read it all.").font(t.ty.xs).color(t.ink_faint()));
            }
        }
    }
    scroll(t, "cattain-digest-scroll", page)
}

// ---- Files ------------------------------------------------------------------------------------------------------------------------------

fn reveal_button(t: &Theme, key: &str, path: String, words: &str) -> El {
    let reveal: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| {
        let _ = tx.reveal_path(&path);
    });
    icon_button(t, key, words, &icons::FOLDER_OPEN, None, false, Some(reveal))
}

/// Files: the four files as a list; the chosen one below, RULES.md and CREW.md in an editor, NOTES.md and DIGEST.md to read.
pub fn files(s: &App, t: &Theme, c: &Cat) -> El {
    let chosen = s.chosen_file(&c.id);
    let n = cats::FILES.len();
    let mut list = col().key("cattain-file-list").role("listbox").label("Files").w_full().gap(1.0).focus_group();
    for (i, name) in cats::FILES.iter().enumerate() {
        let f = s.cattains.files.get(&(c.id.clone(), (*name).to_owned()));
        let when = changed(s, f);
        let (id, nm) = (c.id.clone(), (*name).to_owned());
        let mut item = crate::ui::sidebar::SidebarItem::new(format!("cattain-file-{}", name.trim_end_matches(".md").to_lowercase()), (*name).to_owned())
            .icon(&icons::FILE_TEXT)
            .subtitle(file_words(name))
            .list_row(i + 1, n)
            .selected(chosen == *name)
            .on_press(move |s: &mut App| s.choose_file(&id, &nm));
        if !when.is_empty() {
            item = item.trail(text(when).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden());
        }
        list = list.child(item.build(t));
    }
    let folder = s.cattains.files.get(&(c.id.clone(), chosen.clone())).map(|f| f.path.rsplit_once('/').map_or_else(|| f.path.clone(), |(d, _)| d.to_owned()));
    let mut head = row().w_full().center_y().child(crate::pane_view::heading(t, "cattain-files-heading", "Files", None));
    if let Some(dir) = folder {
        head = head.child(el().grow(1.0)).child(reveal_button(t, "cattain-folder-reveal", dir, &format!("Reveal {}'s folder in Finder", c.name)));
    }
    let page = col().key("cattain-files").w_full().gap(t.space.at(4.0)).child(col().w_full().gap(t.space.at(1.5)).child(head).child(list)).child(file_body(s, t, c, &chosen));
    scroll(t, "cattain-files-scroll", page)
}

/// The chosen file: an editor for RULES.md and CREW.md with Save, Revert and Restore previous; NOTES.md and DIGEST.md to read.
fn file_body(s: &App, t: &Theme, c: &Cat, name: &str) -> El {
    let key = (c.id.clone(), name.to_owned());
    let Some(f) = s.cattains.files.get(&key) else { return text(format!("Reading {name}…")).key("cattain-file-loading").font(t.ty.sm).color(t.ink_faint()) };
    let mut body = col().key("cattain-file-body").role("group").label(name.to_owned()).w_full().gap(t.space.at(2.0));
    let about = match name {
        cats::RULES => format!("{} reads these on every run of its own thread, ahead of your words. Words for the agent only: they never change what Neko asks before a step.", c.name),
        cats::CREW => format!("Every thread in {}'s crew reads this on every run, exactly as it is here.", c.name),
        cats::NOTES => format!("{}'s own plan. It keeps this file; you read it here.", c.name),
        _ => format!("What happened, kept by {} for you. It shows in the Digest tab too.", c.name),
    };
    body = body.child(text(about).key("cattain-file-about").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    if !f.editable {
        if f.text.trim().is_empty() {
            return body.child(text(format!("{} has not written {name} yet.", c.name)).key("cattain-file-empty").font(t.ty.sm).color(t.ink_faint()));
        }
        return body.child(paint_state(col().key("cattain-file-read").w_full().padding(t.space.at(3.0)).radius(t.radius.lg).round_as(t), &t.mat.well, States::default()).child(Response::new("cattain-file-words", demoted(&f.text)).selectable(true).column(crate::pane::MAX_WIDTH).build(t)));
    }
    let unsaved = s.file_unsaved(&c.id, name);
    let draft = s.cattains.drafts.get(&key).cloned().unwrap_or_else(|| rune::TextEdit::new(&f.text));
    let (a, b) = (c.id.clone(), name.to_owned());
    let placeholder = if name == cats::RULES { "For example: Plan first. Hand the work to your crew. Never merge without asking." } else { "For example: Open a PR; never merge. Use British spelling." };
    body = body.child(Textarea::new("cattain-file", &draft, move |s: &mut App| s.file_draft_mut(&a, &b)).name(format!("{}'s {name}", c.name)).placeholder(placeholder).rows(8, 22).build(t));
    let mut buttons = row().key("cattain-file-actions").w_full().gap(t.space.at(2.0)).center_y();
    let (a, b, a2, b2) = (c.id.clone(), name.to_owned(), c.id.clone(), name.to_owned());
    let long = draft.text.len() > cats::MAX_RULES;
    buttons = buttons
        .child(Button::<App>::new("cattain-file-save", "Save").size(Size::Sm).disabled(!unsaved || long).on_press(move |s: &mut App| s.save_file(&a, &b)).build(t))
        .child(Button::<App>::new("cattain-file-revert", "Revert").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(!unsaved).on_press(move |s: &mut App| s.revert_file(&a2, &b2)).build(t));
    if unsaved {
        buttons = buttons.child(text("Unsaved changes").key("cattain-file-unsaved").role("status").font(t.ty.sm).color(t.ink_muted()));
    }
    buttons = buttons.child(row().grow(1.0));
    if f.previous {
        let (a, b) = (c.id.clone(), name.to_owned());
        buttons = buttons.child(Button::<App>::new("cattain-file-restore", "Restore previous").variant(ButtonVariant::Ghost).size(Size::Sm).disabled(unsaved).on_press(move |s: &mut App| s.restore_file(&a, &b)).build(t));
    }
    body = body.child(buttons);
    if long {
        body = body.child(text(format!("That is {} KB; {name} can be at most 16 KB.", draft.text.len().div_ceil(1024))).key("cattain-file-long").role("status").font(t.ty.sm).color(t.c.warning.fg));
    }
    body
}

// ---- Profile ----------------------------------------------------------------------------------------------------------------------------

/// A pop-up menu on a chip: the current choice checked.
fn picker(s: &App, t: &Theme, layer: &mut Vec<El>, key: &str, name: &str, shown: String, menu: DropdownMenu<App>) -> El {
    let open = s.menu_open.as_deref() == Some(key);
    let k = key.to_owned();
    let face = chip(t, format!("{key}-chip"), None)
        .role("button")
        .label(format!("{name}: {shown}"))
        .focusable(true)
        .expanded(open)
        .child(text(shown).font(t.ty.sm).color(t.ink()).truncate().max_w(180.0).min_w(0.0).a11y_hidden())
        .child(glyph(&icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint()))
        .on_press(move |s: &mut App| {
            s.menu_open = if s.menu_open.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) };
        });
    let menu = menu.trigger(face).open(open).on_close(|s: &mut App| s.menu_open = None);
    menu_named(menu, t, layer, name, open)
}

/// The faces as a row of portraits to choose from: one Tab stop, the arrows move, Return or Space picks; its own is ringed.
fn face_picker(t: &Theme, c: &Cat) -> El {
    let n = crate::faces::COATS.len();
    let mut swatches = row().key("cat-face").role("radiogroup").label("Face").gap(t.space.at(1.5)).flex_wrap().focus_group();
    for (i, coat) in crate::faces::COATS.iter().enumerate() {
        let on = c.face == coat.id;
        let (cat, face) = (c.id.clone(), coat.id.to_owned());
        let size = 32.0;
        let ring = 2.0;
        let mut sw = el()
            .key(format!("cat-face-{}", coat.id))
            .role("radio")
            .label(coat.words)
            .checked(on)
            .position_in_set(i + 1, n)
            .focusable(true)
            .focus_selected(on)
            .press_on_return()
            .padding(ring)
            .radius(crate::faces::radius(size) + 2.0 * ring)
            .round_as(t);
        sw = paint_state(sw, &t.mat.row, States::default());
        let mut frame = el().size(size + 2.0 * ring, size + 2.0 * ring).radius(crate::faces::radius(size) + ring).padding(ring);
        if on {
            frame = frame.border(2.0, t.c.ring);
        }
        sw = sw.child(frame.child(crate::faces::portrait(t, &format!("face-swatch-{}", coat.id), coat.id, size)));
        swatches = swatches.child(sw.on_press(move |s: &mut App| s.set_cat_face(&cat, &face)));
    }
    swatches
}

fn access_words(m: PermissionMode) -> &'static str {
    match m {
        PermissionMode::Follow => "Follow Claude Code's own rules",
        PermissionMode::Ask => "Ask before risky steps",
        PermissionMode::Full => "Full access",
    }
}

/// Profile: name, face, purpose, project, model, access, its plugins, and Retire….
pub fn profile(s: &App, t: &Theme, c: &Cat, layer: &mut Vec<El>) -> El {
    let id = c.id.clone();
    let st = cattain_status(s, c);
    let state = if c.is_active() { crate::sidebar::cat_words(s, c) } else { "Retired: takes no new crew".to_owned() };
    let rename = {
        let id = id.clone();
        Button::<App>::new("cat-rename", "Rename…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.start_rename_cat(&id)).build(t)
    };
    let head = row()
        .key("cat-head")
        .w_full()
        .gap(t.space.at(3.0))
        .center_y()
        .child(crate::faces::portrait_in(t, "cat-head-face", &c.face, 48.0, st, crate::sidebar::panel_bg(t)))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(t.space.at(0.5))
                .child(text(c.name.clone()).key("cat-title").role("heading").level(2).font(weight(t.ty.lg, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0))
                .child(text(state).key("cat-state").role("status").font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0)),
        )
        .child(rename);
    let purpose = {
        let (a, b) = (id.clone(), id.clone());
        let draft = s.cattains.purpose.get(&id).cloned().unwrap_or_else(|| rune::TextEdit::new(c.purpose.as_deref().unwrap_or("")));
        Input::new("cat-purpose", &draft, move |s: &mut App| s.purpose_mut(&a)).name("Purpose").placeholder("What this Cattain is for, in a line").on_submit(move |s: &mut App| s.save_purpose(&b)).build(t)
    };
    let purpose_changed = s.cattains.purpose.get(&id).is_some_and(|d| d.text.split_whitespace().collect::<Vec<_>>().join(" ") != c.purpose.clone().unwrap_or_default());
    let mut purpose_part = col().key("cat-purpose-part").w_full().gap(t.space.at(1.5)).child(label(t, "Purpose")).child(purpose);
    if purpose_changed {
        let a = id.clone();
        purpose_part = purpose_part.child(row().child(Button::<App>::new("cat-purpose-save", "Save purpose").size(Size::Sm).on_press(move |s: &mut App| s.save_purpose(&a)).build(t)));
    }
    // Project.
    let current = c.workspace.as_deref().and_then(|w| s.workspace_of(w)).filter(|w| w.is_active() && w.folder.is_some()).map(|w| w.name.clone());
    let mut menu = DropdownMenu::new("cat-workspace-menu").min_width(220.0).label("Project");
    {
        let a = id.clone();
        menu = menu.item(DropdownMenuItem::new("None").checked(current.is_none()).on_select(move |s: &mut App| s.set_cat_workspace(&a, None))).separator();
    }
    for w in s.workspaces.iter().filter(|w| w.is_active() && w.folder.is_some()) {
        let (a, ws) = (id.clone(), w.id.clone());
        menu = menu.item(DropdownMenuItem::new(w.name.clone()).checked(c.workspace.as_deref() == Some(w.id.as_str())).on_select(move |s: &mut App| s.set_cat_workspace(&a, Some(ws.clone()))));
    }
    let project = picker(s, t, layer, "cat-workspace-menu", "Project", current.unwrap_or_else(|| "None".to_owned()), menu);
    // Model.
    let model_shown = match c.model.as_deref() {
        None => "Default".to_owned(),
        Some(m) => MODELS.iter().find(|(id, _)| *id == m).map_or_else(|| m.to_owned(), |(_, w)| (*w).to_owned()),
    };
    let mut menu = DropdownMenu::new("cat-model-menu").min_width(200.0).label("Model");
    {
        let a = id.clone();
        menu = menu.item(DropdownMenuItem::new("Default").checked(c.model.is_none()).on_select(move |s: &mut App| s.set_cat_model(&a, None))).separator();
    }
    for (m, words) in MODELS {
        let a = id.clone();
        menu = menu.item(DropdownMenuItem::new(*words).checked(c.model.as_deref() == Some(*m)).on_select(move |s: &mut App| s.set_cat_model(&a, Some((*m).to_owned()))));
    }
    let model = picker(s, t, layer, "cat-model-menu", "Model", model_shown, menu);
    // Access.
    let setting = s.runtimes.as_ref().map(|(_, x)| x.permission_mode).unwrap_or_default();
    let access_shown = c.access.map_or_else(|| format!("As in Settings ({})", access_words(setting)), |m| access_words(m).to_owned());
    let mut menu = DropdownMenu::new("cat-access-menu").min_width(260.0).label("Access");
    {
        let a = id.clone();
        menu = menu.item(DropdownMenuItem::new(format!("As in Settings ({})", access_words(setting))).checked(c.access.is_none()).on_select(move |s: &mut App| s.set_cat_access(&a, None))).separator();
    }
    for m in [PermissionMode::Follow, PermissionMode::Ask, PermissionMode::Full] {
        let a = id.clone();
        menu = menu.item(DropdownMenuItem::new(access_words(m)).checked(c.access == Some(m)).on_select(move |s: &mut App| s.set_cat_access(&a, Some(m))));
    }
    let access = picker(s, t, layer, "cat-access-menu", "Access", access_shown, menu);
    let facts = col()
        .key("cat-facts")
        .w_full()
        .gap(t.space.at(1.0))
        .child(fact_row(t, "cat-project-row", "Project", project))
        .child(fact_row(t, "cat-model-row", "Model", model))
        .child(fact_row(t, "cat-access-row", "Access", access))
        .child(text(format!("{}'s own thread runs with these. Its crew runs as Settings say.", c.name)).font(t.ty.xs).color(t.ink_faint()).wrap().min_w(0.0).pt(t.space.at(1.0)));
    let end = if c.is_active() {
        let a = id.clone();
        let last = s.active_cats().len() <= 1;
        Button::<App>::new("cat-retire", "Retire…").variant(ButtonVariant::Ghost).size(Size::Sm).disabled(last).on_press(move |s: &mut App| s.ask_retire_cat(&a)).build(t)
    } else {
        let a = id.clone();
        Button::<App>::new("cat-bring-back", "Bring back").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.bring_back_cat(&a)).build(t)
    };
    let page = col()
        .key("cat-profile")
        .w_full()
        .gap(t.space.at(6.0))
        .child(head)
        .child(col().w_full().gap(t.space.at(2.0)).child(label(t, "Face")).child(face_picker(t, c)))
        .child(purpose_part)
        .child(facts)
        .child(crate::plugins_view::cat_section(s, t, c))
        .child(row().w_full().pt(t.space.at(2.0)).border_sides([0.0, 1.0, 0.0, 0.0], t.c.line).child(row().pt(t.space.at(4.0)).child(end)));
    scroll(t, "cat-profile-scroll", page)
}

fn label(t: &Theme, words: &str) -> El {
    text(words.to_owned()).role("heading").level(3).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted())
}

fn fact_row(t: &Theme, key: &str, words: &str, value: El) -> El {
    row().key(key.to_owned()).w_full().min_h(t.control.md + 4.0).gap(t.space.at(2.0)).center_y().child(text(words.to_owned()).font(t.ty.sm).color(t.ink_muted()).w(64.0).a11y_hidden()).child(value)
}

/// The tab's body for a Cattain's thread, or None for a tab that is not its own.
pub fn body(s: &App, t: &Theme, root: &TaskId, tab: &Tab, layer: &mut Vec<El>) -> Option<El> {
    let c = s.cattain_of(root)?;
    Some(match tab {
        Tab::Overview => overview(s, t, c),
        Tab::Digest => digest(s, t, c),
        Tab::Files => files(s, t, c),
        Tab::Profile => profile(s, t, c, layer),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::demoted;

    #[test]
    fn a_files_headings_step_down_and_code_is_left_alone() {
        assert_eq!(demoted("# Plan\n\n## Next\n#tag\n"), "## Plan\n\n### Next\n#tag\n");
        assert_eq!(demoted("```sh\n# a comment\n```\n"), "```sh\n# a comment\n```\n");
        assert_eq!(demoted("###### Six"), "###### Six");
    }
}
