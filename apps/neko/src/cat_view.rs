//! A cat's page (docs/knowledge/cats.md): its face and name, Rename and Retire, its face, purpose and default workspace, its
//! standing rules with Save, Revert and Restore previous, and its tasks (working, needs you, recent). Where a cat's character lives:
//! its portrait large on a wash of its own ground, its name at display size, the faces to choose from as portraits.

use neko_protocol::{Cat, Task};
use rune::{El, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size, weight};

use crate::state::App;
use crate::ui::agent::chip;
use crate::ui::base::{Shape, States, Theme, glyph, paint_state};
use crate::ui::button::Button;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::input::Input;
use crate::ui::sidebar::SidebarItem;
use crate::ui::textarea::Textarea;
use crate::view::{COLUMN, menu_named, status_mark};

/// How many finished tasks "Recent" lists.
const RECENT: usize = 8;

fn label(t: &Theme, words: &str) -> El {
    text(words.to_owned()).role("heading").level(2).font(weight(t.ty.base, t.ty.medium)).color(t.ink())
}

/// A pop-up menu on a chip: the current choice checked.
fn picker(s: &App, t: &Theme, layer: &mut Vec<El>, key: &str, name: &str, face: El, menu: DropdownMenu<App>) -> El {
    let open = s.menu_open.as_deref() == Some(key);
    let k = key.to_owned();
    let face = face.expanded(open).focusable(true).child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint())).on_press(move |s: &mut App| {
        s.menu_open = if s.menu_open.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) };
    });
    let menu = menu.trigger(face).open(open).on_close(|s: &mut App| s.menu_open = None);
    menu_named(menu, t, layer, name, open)
}

/// The faces as a row of portraits to choose from: one Tab stop, the arrows move, Return or Space picks; the cat's own is ringed.
fn face_picker(t: &Theme, c: &Cat) -> El {
    let n = crate::faces::COATS.len();
    let mut swatches = row().key("cat-face").role("radiogroup").label("Face").gap(t.space.at(2.0)).flex_wrap().focus_group();
    for (i, coat) in crate::faces::COATS.iter().enumerate() {
        let on = c.face == coat.id;
        let (cat, face) = (c.id.clone(), coat.id.to_owned());
        let size = 44.0;
        let ring = 3.0;
        let mut sw = col()
            .key(format!("cat-face-{}", coat.id))
            .role("radio")
            .label(coat.words)
            .checked(on)
            .position_in_set(i + 1, n)
            .focusable(true)
            .focus_selected(on)
            .press_on_return()
            .gap(t.space.at(1.0))
            .items_center()
            .padding(ring)
            .radius(crate::faces::radius(size) + ring)
            .round_as(t);
        sw = paint_state(sw, &t.mat.row, States::default());
        let mut frame = el().size(size + 2.0 * ring, size + 2.0 * ring).radius(crate::faces::radius(size) + ring).padding(ring);
        if on {
            frame = frame.border(2.0, t.c.ring);
        }
        sw = sw.child(frame.child(crate::faces::portrait(t, coat.id, size))).child(text(coat.words).font(weight(t.ty.xs, if on { t.ty.semibold } else { t.ty.regular })).color(if on { t.ink() } else { t.ink_muted() }).a11y_hidden());
        swatches = swatches.child(sw.on_press(move |s: &mut App| s.set_cat_face(&cat, &face)));
    }
    swatches
}

fn workspace_picker(s: &App, t: &Theme, c: &Cat, layer: &mut Vec<El>) -> El {
    let current = c.workspace.as_deref().and_then(|w| s.workspace_of(w)).map(|w| w.name.clone());
    let shown = current.clone().unwrap_or_else(|| "None".to_owned());
    let chip_el = chip(t, "cat-workspace", None)
        .role("button")
        .label(format!("Default workspace: {shown}"))
        .child(glyph(&crate::ui::icons::FOLDER, t.control.icon_sm, t.ink_muted()))
        .child(text(shown).font(t.ty.sm).color(t.ink()).truncate().max_w(200.0).min_w(0.0).a11y_hidden());
    let cat = c.id.clone();
    let mut menu = DropdownMenu::new("cat-workspace-menu").min_width(220.0).label("Default workspace");
    menu = menu.item(DropdownMenuItem::new("None").checked(current.is_none()).on_select(move |s: &mut App| s.set_cat_workspace(&cat, None))).separator();
    for w in s.workspaces.iter().filter(|w| w.is_active()) {
        let (cat, ws) = (c.id.clone(), w.id.clone());
        menu = menu.item(DropdownMenuItem::new(w.name.clone()).checked(c.workspace.as_deref() == Some(w.id.as_str())).on_select(move |s: &mut App| s.set_cat_workspace(&cat, Some(ws.clone()))));
    }
    picker(s, t, layer, "cat-workspace-menu", "Workspaces", chip_el, menu)
}

/// One list of the cat's tasks, as rows that open them.
fn task_list(t: &Theme, key: &str, heading: &str, tasks: &[&Task], open: Option<&neko_protocol::TaskId>) -> El {
    let n = tasks.len();
    let mut list = col().key(format!("cat-{key}-list")).role("listbox").label(heading.to_owned()).w_full().gap(1.0).focus_group();
    for (i, task) in tasks.iter().enumerate() {
        let (status, word) = status_mark(task.status);
        let id = task.id.clone();
        list = list.child(
            SidebarItem::new(format!("cat-{key}-{}", task.id.0), task.title.clone())
                .lead(crate::ui::agent::mark(t, status, t.control.icon))
                .list_row(i + 1, n)
                .description(word)
                .selected(open == Some(&task.id))
                .on_press(move |s: &mut App| s.open_task_id(&id))
                .build(t),
        );
    }
    col().key(format!("cat-{key}")).w_full().gap(t.space.at(2.0)).child(label(t, heading)).child(list)
}

pub fn cat_view(s: &App, t: &Theme, c: &Cat, layer: &mut Vec<El>) -> El {
    let state = s.cat_state(&c.id);
    let draft = s.cat_drafts.get(&c.id).cloned().unwrap_or_else(|| crate::state::CatDraft::of(c));
    let unsaved = draft.unsaved(c);
    let id = c.id.clone();
    let st = match state {
        crate::state::CatState::Working(_) => Some(neko_protocol::TaskStatus::Working),
        crate::state::CatState::NeedsYou => Some(neko_protocol::TaskStatus::NeedsYou),
        crate::state::CatState::Napping => None,
    };
    let ink = match state {
        crate::state::CatState::Working(_) => t.c.accent.fg,
        crate::state::CatState::NeedsYou => t.c.warning.fg,
        crate::state::CatState::Napping => t.ink_faint(),
    };
    let actions = row()
        .gap(t.space.at(2.0))
        .center_y()
        .child({
            let id = id.clone();
            Button::new("cat-rename", "Rename…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.start_rename_cat(&id)).build(t)
        })
        .child({
            let id = id.clone();
            let last = s.active_cats().len() <= 1;
            Button::new("cat-retire", "Retire…").variant(ButtonVariant::Ghost).size(Size::Sm).disabled(last || !c.is_active()).on_press(move |s: &mut App| s.ask_retire_cat(&id)).build(t)
        });
    let mut said = capital(&state.words());
    if !c.is_active() {
        said = "Retired: takes no new tasks".to_owned();
    }
    let who = col()
        .grow(1.0)
        .min_w(0.0)
        .pt(t.space.at(6.0))
        .gap(t.space.at(1.0))
        .child(text(c.name.clone()).key("cat-title").role("heading").level(1).font(t.ty.xxl).color(t.ink()).truncate().min_w(0.0))
        .child(text(said).key("cat-state").role("status").font(weight(t.ty.base, t.ty.semibold)).color(ink));
    let who = match c.purpose.as_ref().filter(|p| !p.is_empty()) {
        Some(p) => who.child(text(p.clone()).key("cat-purpose-line").font(weight(t.ty.lg, t.ty.regular)).color(t.ink_muted()).wrap().min_w(0.0).pt(t.space.at(1.0))),
        None => who,
    };
    let wash = crate::faces::wash(t, &c.face);
    // The hero: the portrait large, on a wash of its own ground, the name at display size, what it is doing, and its two actions.
    let hero = row()
        .key("cat-hero")
        .w_full()
        .padding(t.space.at(5.0))
        .gap(t.space.at(5.0))
        .items_start()
        .radius(t.radius.xl + t.space.at(5.0) - 8.0)
        .round_as(t)
        .fill(wash)
        .child(crate::faces::portrait_in(t, &c.face, 112.0, st, wash))
        .child(who)
        .child(actions);
    let facts = col()
        .key("cat-facts")
        .w_full()
        .gap(t.space.at(5.0))
        .child(col().w_full().gap(t.space.at(2.5)).child(label(t, "Face")).child(face_picker(t, c)))
        .child(row().gap(t.space.at(3.0)).center_y().child(label(t, "Default workspace")).child(workspace_picker(s, t, c, layer)));
    let purpose = {
        let id = id.clone();
        let save = id.clone();
        Input::new("cat-purpose", &draft.purpose, move |s: &mut App| &mut s.cat_draft_mut(&id).purpose).name("Purpose").placeholder("What this cat is for, in a line").on_submit(move |s: &mut App| s.save_cat(&save)).build(t)
    };
    let rules_box = {
        let id = id.clone();
        Textarea::new("cat-rules", &draft.rules, move |s: &mut App| &mut s.cat_draft_mut(&id).rules).name(format!("{}'s standing rules", c.name)).placeholder("For example: Write tests first. Use British spelling. Never push.").rows(6, 18).build(t)
    };
    let mut buttons = row().key("cat-rules-actions").w_full().gap(t.space.at(2.0)).center_y();
    {
        let (a, b) = (id.clone(), id.clone());
        buttons = buttons
            .child(Button::new("cat-save", "Save").size(Size::Sm).disabled(!unsaved).on_press(move |s: &mut App| s.save_cat(&a)).build(t))
            .child(Button::new("cat-revert", "Revert").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(!unsaved).on_press(move |s: &mut App| s.revert_cat(&b)).build(t));
    }
    if unsaved {
        buttons = buttons.child(text("Unsaved changes").key("cat-unsaved").role("status").font(t.ty.sm).color(t.ink_muted()));
    }
    buttons = buttons.child(row().grow(1.0));
    if c.previous_rules.is_some() {
        let id = id.clone();
        buttons = buttons.child(Button::new("cat-restore", "Restore previous rules").variant(ButtonVariant::Ghost).size(Size::Sm).disabled(unsaved).on_press(move |s: &mut App| s.restore_cat_rules(&id)).build(t));
    }
    let size = draft.rules.text.len();
    let mut rules = col()
        .key("cat-rules-section")
        .w_full()
        .gap(t.space.at(2.0))
        .child(label(t, "Standing rules"))
        .child(
            text(format!("{} reads these on every run of its tasks, ahead of your words. They are words for the agent only: they never change what Neko asks before a step.", c.name))
                .font(t.ty.sm)
                .color(t.ink_muted())
                .wrap()
                .min_w(0.0),
        )
        .child(rules_box)
        .child(buttons);
    if size > neko_protocol::cats::MAX_RULES {
        rules = rules.child(text(format!("That is {} KB; a cat's rules can be at most 16 KB.", size.div_ceil(1024))).key("cat-rules-long").role("status").font(t.ty.sm).color(t.c.warning.fg));
    }
    // The cat's rounds: its standing work that starts by itself, and a way to make another.
    let mine_rounds = s.rounds_of(&c.id);
    let mut rounds = col().key("cat-rounds").w_full().gap(t.space.at(2.5)).child(label(t, "Rounds"));
    if mine_rounds.is_empty() {
        rounds = rounds.child(text(format!("{} has no rounds. A round starts work by itself at the times you choose.", c.name)).key("cat-no-rounds").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    } else {
        rounds = rounds.child(crate::rounds_view::cards(s, t, "cat-rounds-list", &format!("{}'s rounds", c.name), &mine_rounds, layer));
    }
    let new_round = {
        let id = id.clone();
        Button::new("cat-new-round", format!("New round for {}…", c.name)).variant(ButtonVariant::Secondary).size(Size::Sm).disabled(!c.is_active()).on_press(move |s: &mut App| s.start_new_round(Some(&id))).build(t)
    };
    rounds = rounds.child(row().child(new_round));
    let mine: Vec<&Task> = s.tasks.iter().filter(|x| x.cat == c.id).collect();
    let waiting: Vec<&Task> = mine.iter().copied().filter(|x| s.visible_needs().iter().any(|i| i.task == x.id) || x.status == neko_protocol::TaskStatus::Asked).collect();
    let working: Vec<&Task> = mine.iter().copied().filter(|x| x.status.is_active() && !waiting.iter().any(|w| w.id == x.id)).collect();
    let recent: Vec<&Task> = mine.iter().copied().filter(|x| !working.iter().chain(waiting.iter()).any(|w| w.id == x.id)).take(RECENT).collect();
    let mut tasks = col().key("cat-tasks").w_full().gap(t.space.at(5.0));
    if mine.is_empty() {
        tasks = tasks.child(label(t, "Tasks")).child(text(format!("{} has no tasks yet. Pick {} in the composer, or let Neko choose.", c.name, c.name)).key("cat-no-tasks").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    }
    if !working.is_empty() {
        tasks = tasks.child(task_list(t, "working", "Working", &working, s.open.as_ref()));
    }
    if !waiting.is_empty() {
        tasks = tasks.child(task_list(t, "waiting", "Needs you", &waiting, s.open.as_ref()));
    }
    if !recent.is_empty() {
        tasks = tasks.child(task_list(t, "recent", "Recent", &recent, s.open.as_ref()));
    }
    let page = col()
        .key("cat-page")
        .w_full()
        .max_w(COLUMN)
        .gap(t.space.at(6.0))
        .child(hero)
        .child(facts)
        .child(col().w_full().gap(t.space.at(2.0)).child(label(t, "Purpose")).child(purpose))
        .child(rules)
        .child(rounds)
        .child(tasks);
    col().key("cat-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.space.at(8.0)).pt(t.space.at(6.0)).pb(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
