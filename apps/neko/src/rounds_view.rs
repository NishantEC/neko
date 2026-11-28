//! The Rounds page (⌘3) and the New round / Edit panel (docs/knowledge/rounds.md): every round as a raised card led by its Cattain's
//! face (or the Rounds mark for a round of nobody's), with its schedule in words, when it runs next or why it does not, how it last
//! went, a switch, Run now and a menu; Pause automatic work (rounds, timers and watches); and a panel that says when the first run
//! will be before the round is saved.

use neko_protocol::{GENERAL, Round, RoundStatus, TaskStatus};
use rune::{El, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size, tabular, weight};

use crate::rounds::{KINDS, RoundForm, first_run_words, last_words, line_words, schedule_words, when, why_not};
use crate::state::App;
use crate::ui::agent::{FoldLine, chip, icon_button};
use crate::ui::base::{Shape, Theme, glyph};
use crate::ui::button::Button;
use crate::ui::dialog::Dialog;
use crate::ui::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use crate::ui::empty::Empty;
use crate::ui::input::Input;
use crate::ui::segmented_control::SegmentedControl;
use crate::ui::switch::Switch;
use crate::ui::textarea::Textarea;
use crate::view::{COLUMN, menu_named, raised};

/// How many history lines a card shows.
const HISTORY_SHOWN: usize = 8;

fn lead_ink(t: &Theme, r: &Round) -> rune::Color {
    match r.status {
        RoundStatus::Braked | RoundStatus::CatRetired | RoundStatus::FolderMissing | RoundStatus::WorkspaceRemoved => t.c.warning.fg,
        RoundStatus::Scheduled => t.c.accent.fg,
        _ => t.ink_muted(),
    }
}

/// "Next: tomorrow 09:00", or why the round does not run now ("Paused", "Off", "mochi is retired", "Folder missing").
fn next_line(s: &App, t: &Theme, r: &Round) -> Option<El> {
    let zone = s.zone();
    let (words, ink) = match (r.next_due_ms, why_not(r)) {
        (Some(ms), _) => (format!("Next: {}", when(ms, s.now_ms, &*zone)), lead_ink(t, r)),
        // A manual round's schedule line already says it runs only by hand.
        (None, _) if r.status == RoundStatus::Manual => return None,
        // The brake's own line says it all, in the card's note.
        (None, _) if r.status == RoundStatus::Braked && r.note.is_some() => return None,
        (None, Some(why)) => (why, lead_ink(t, r)),
        (None, None) => ("Nothing is due".to_owned(), t.ink_muted()),
    };
    Some(text(words).key(format!("round-{}-next", r.id)).font(weight(t.ty.sm, t.ty.semibold)).color(ink).truncate().min_w(0.0))
}

/// "Last: Done, today at 09:00" with a link to the task.
fn last_line(s: &App, t: &Theme, r: &Round) -> Option<El> {
    let l = r.last.as_ref()?;
    let zone = s.zone();
    let status = l.task.as_ref().and_then(|id| s.task_status(id));
    let mut line = row().key(format!("round-{}-last", r.id)).w_full().gap(t.space.at(2.0)).center_y().min_w(0.0);
    line = line.child(text(last_words(l, status, s.now_ms, &*zone)).key(format!("round-{}-last-words", r.id)).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink());
    if let Some(id) = l.task.clone().filter(|id| s.tasks.iter().any(|x| &x.id == id)) {
        // A standing round's line is its Cattain's own thread.
        let open = match r.cat.as_deref().and_then(|c| s.cat(c)).filter(|_| r.standing) {
            Some(c) => format!("Open {}'s thread", c.name),
            None => "Open thread".to_owned(),
        };
        line = line.child(Button::new(format!("round-{}-open", r.id), open).variant(ButtonVariant::Link).size(Size::Sm).on_press(move |s: &mut App| s.open_task_id(&id)).build(t));
    }
    Some(line)
}

fn history(s: &App, t: &Theme, r: &Round) -> El {
    let open = s.round_history_open.contains(&r.id);
    let id = r.id.clone();
    let lines = s.round_history.get(&r.id).cloned().unwrap_or_default();
    let mut body = Vec::new();
    if open {
        let zone = s.zone();
        let mut list = col().key(format!("round-{}-history-list", r.id)).role("list").label(format!("History of {}", r.name)).w_full().gap(t.space.at(1.0)).pl(t.space.at(1.0));
        if lines.is_empty() {
            list = list.child(text("Nothing yet: no time has come.").font(t.ty.sm).color(t.ink_faint()));
        }
        for l in lines.iter().take(HISTORY_SHOWN) {
            let status = l.task.as_ref().and_then(|id| s.task_status(id));
            let due = l.due_ms.map_or_else(|| "Run now".to_owned(), |d| when(d, s.now_ms, &*zone));
            let mut line = row().role("listitem").w_full().gap(t.space.at(3.0)).center_y().min_w(0.0);
            line = line.child(text(due).font(tabular(t.ty.sm)).color(t.ink()).min_w(96.0).truncate());
            line = line.child(text(line_words(l, status)).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink().grow(1.0));
            if let Some(task) = l.task.clone().filter(|id| s.tasks.iter().any(|x| &x.id == id)) {
                line = line.child(Button::new(format!("round-{}-line-{}", r.id, l.id), "Open").variant(ButtonVariant::Link).size(Size::Sm).on_press(move |s: &mut App| s.open_task_id(&task)).build(t));
            }
            list = list.child(line);
        }
        body.push(list);
    }
    let lead = glyph(&crate::ui::icons::CLOCK, t.control.icon, t.ink_faint()).a11y_hidden();
    FoldLine::<App>::new(format!("round-{}-history", r.id), "History", open).lead(lead).on_toggle(move |s: &mut App| s.toggle_round_history(&id)).build(t, body)
}

/// Who a round is and where it goes, in a few words: "miso · in its own thread · Weekdays at 09:00", "Weekdays at 09:00" for a round of
/// nobody's.
pub fn who_words(s: &App, r: &Round) -> String {
    let cat = r.cat.as_deref().and_then(|c| s.cat(c)).map(|c| c.name.clone());
    match (cat, r.standing) {
        (Some(c), true) => format!("{c} · in its own thread · {}", schedule_words(&r.schedule)),
        (Some(c), false) => format!("{c} · {}", schedule_words(&r.schedule)),
        (None, _) => schedule_words(&r.schedule),
    }
}

/// What leads a round's card: its Cattain's face, or the Rounds mark in the same round well for a round of nobody's.
fn lead(s: &App, t: &Theme, r: &Round) -> El {
    let key = format!("round-{}-face", r.id);
    match r.cat.as_deref().filter(|c| s.cat(c).is_some()) {
        Some(c) => crate::faces::portrait_in(t, &key, s.cat_face(c), 44.0, None, crate::view::raised_fill(t)),
        // The same room and corner as a face's tile, so the cards' words line up.
        None => el().key(key).size(crate::faces::ringed(44.0), crate::faces::ringed(44.0)).center().a11y_hidden().child(
            el().size(44.0, 44.0).radius(crate::faces::radius(44.0)).round_as(t).fill(t.c.accent.bg).center().child(glyph(&crate::icons::ROUNDS, 20.0, t.c.accent.fg)),
        ),
    }
}

/// One round: its Cattain's face (or the Rounds mark), the name, its schedule and next time, how it last went, and its controls.
fn card(s: &App, t: &Theme, r: &Round, i: usize, n: usize, layer: &mut Vec<El>) -> El {
    let id = r.id.clone();
    let on_label = format!("{} is {}", r.name, if r.on { "on" } else { "off" });
    let switch = {
        let id = id.clone();
        let on = r.on;
        Switch::<App>::new(format!("round-{id}-switch"), on_label, on).on_toggle(move |s: &mut App| s.set_round_on(&id, !on)).build(t)
    };
    let run = {
        let id = id.clone();
        Button::new(format!("round-{id}-run"), "Run now").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.run_round_now(&id)).build(t)
    };
    let key = format!("round-menu-{id}");
    let open = s.menu_open.as_deref() == Some(key.as_str());
    let toggle = {
        let k = key.clone();
        let face: crate::ui::agent::PressTx<App> = std::rc::Rc::new(move |s: &mut App, _: &mut rune::Tx| s.menu_open = if s.menu_open.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) });
        icon_button(t, &format!("round-{id}-menu"), &format!("Options for {}", r.name), &crate::ui::icons::MORE, None, false, Some(face)).expanded(open)
    };
    let menu = DropdownMenu::new(key)
        .trigger(toggle)
        .open(open)
        .min_width(160.0)
        .on_close(|s: &mut App| s.menu_open = None)
        .item({
            let id = id.clone();
            DropdownMenuItem::new("Edit…").on_select(move |s: &mut App| s.start_edit_round(&id))
        })
        .item({
            let id = id.clone();
            DropdownMenuItem::new("Remove…").danger(true).on_select(move |s: &mut App| s.ask_remove_round(&id))
        });
    let menu = menu_named(menu, t, layer, "Round options", open);
    // A manual round has nothing to switch: it runs when Run now is pressed.
    let mut controls = row().key(format!("round-{id}-controls")).gap(t.space.at(2.5)).center_y();
    if r.schedule.is_automatic() {
        controls = controls.child(switch);
    }
    let controls = controls.child(run).child(menu);
    let head = row()
        .w_full()
        .h(t.control.md)
        .gap(t.space.at(3.0))
        .center_y()
        .child(text(r.name.clone()).key(format!("round-{id}-name")).role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).shrink().grow(1.0))
        .child(controls);
    let who = text(who_words(s, r)).key(format!("round-{id}-schedule")).font(t.ty.base).color(t.ink_muted()).truncate().min_w(0.0);
    let mut body = col().grow(1.0).min_w(0.0).gap(t.space.at(1.0)).child(head).child(who);
    if let Some(next) = next_line(s, t, r) {
        body = body.child(next);
    }
    if let Some(note) = r.note.as_ref().filter(|_| matches!(r.status, RoundStatus::Braked)) {
        body = body.child(text(note.clone()).key(format!("round-{id}-note")).role("status").font(t.ty.sm).color(t.c.warning.fg).wrap().min_w(0.0));
    }
    if let Some(last) = last_line(s, t, r) {
        body = body.child(last);
    }
    body = body.child(history(s, t, r));
    let shell = col()
        .key(format!("round-{id}"))
        .role("listitem")
        .label(format!("{}: {}", r.name, who_words(s, r).replace(" · ", ", ")))
        .position_in_set(i + 1, n)
        .w_full()
        .padding(t.space.at(4.0))
        .radius(t.radius.xl + 4.0)
        .round_as(t);
    raised(t, shell).child(row().w_full().gap(t.space.at(4.0)).items_start().child(lead(s, t, r)).child(body))
}

/// The rounds as cards (the page's, and a cat's page's), led by `label` for assistive technology.
pub fn cards(s: &App, t: &Theme, key: &str, label: &str, rounds: &[&Round], layer: &mut Vec<El>) -> El {
    let n = rounds.len();
    let mut list = col().key(key.to_owned()).role("list").label(label.to_owned()).w_full().gap(t.space.at(3.0));
    for (i, r) in rounds.iter().enumerate() {
        list = list.child(card(s, t, r, i, n, layer));
    }
    list
}

pub fn rounds_view(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let n = s.rounds.len();
    let mut page = col().key("rounds").w_full().max_w(COLUMN).gap(t.space.at(6.0));
    // One primary action at a time: while every round is paused, Resume is it and New round… steps back.
    let new = Button::new("rounds-new", "New round…").variant(if s.rounds_paused && n > 0 { ButtonVariant::Secondary } else { ButtonVariant::Default }).size(Size::Sm).on_press(|s: &mut App| s.start_new_round(None)).build(t);
    let mut head = row().w_full().gap(t.space.at(3.0)).center_y().child(text("Rounds").key("rounds-title").role("heading").level(1).font(weight(t.ty.xl, t.ty.medium)).color(t.ink()));
    if n > 0 {
        head = head.child(text(if n == 1 { "1 round".to_owned() } else { format!("{n} rounds") }).key("rounds-count").font(tabular(t.ty.base)).color(t.ink_faint()));
    }
    head = head.child(el().grow(1.0));
    if n > 0 {
        let paused = s.rounds_paused;
        let b = Button::new("rounds-pause", if paused { "Resume automatic work" } else { "Pause automatic work" })
            .variant(if paused { ButtonVariant::Default } else { ButtonVariant::Secondary })
            .size(Size::Sm)
            .on_press(move |s: &mut App| s.set_rounds_paused(!paused))
            .build(t);
        head = head.child(b);
    }
    page = page.child(head.child(new));
    if n == 0 {
        let action = Button::new("rounds-new-empty", "New round…").on_press(|s: &mut App| s.start_new_round(None)).build(t);
        let empty = Empty::new("rounds-empty", &crate::icons::ROUNDS, "No rounds yet")
            .description("A round is work that starts by itself at its times, like “every weekday at 09:00, check the docs for broken links”. Each time is an ordinary thread you can follow, stop and undo, or a message in a Cattain's own thread.")
            .action(action)
            .build(t);
        return wrap(t, page.child(empty).child(honest_note(t)));
    }
    if s.rounds_paused {
        page = page.child(
            row()
                .key("rounds-paused")
                .role("status")
                .label(PAUSED_WORDS)
                .w_full()
                .padding(t.space.at(3.0))
                .gap(t.space.at(3.0))
                .center_y()
                .radius(t.radius.lg)
                .round_as(t)
                .fill(t.c.warning.bg)
                .child(glyph(&crate::ui::icons::WARNING, t.control.icon, t.c.warning.fg))
                .child(text(PAUSED_WORDS).font(weight(t.ty.sm, t.ty.medium)).color(t.c.warning.fg).wrap().min_w(0.0).shrink().a11y_hidden()),
        );
    }
    let all: Vec<&Round> = s.rounds.iter().collect();
    page = page.child(cards(s, t, "rounds-list", "Rounds", &all, layer)).child(honest_note(t));
    wrap(t, page)
}

/// What the paused band says: everything that starts by itself waits.
pub const PAUSED_WORDS: &str = "Automatic work is paused. No round starts, and no timer or watch wakes a thread, until you resume.";

fn honest_note(t: &Theme) -> El {
    text("Rounds run while Neko is running and this Mac is awake. A time that passes while it sleeps runs once when it wakes, if it was at most a day ago.")
        .key("rounds-honest")
        .font(t.ty.sm)
        .color(t.ink_faint())
        .wrap()
        .min_w(0.0)
}

fn wrap(t: &Theme, page: El) -> El {
    col().key("rounds-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.space.at(8.0)).pt(t.space.at(6.0)).pb(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}

fn field(t: &Theme, label: &str, input: El) -> El {
    col().w_full().gap(t.space.at(1.5)).child(text(label.to_owned()).role("heading").level(3).font(weight(t.ty.sm, t.ty.medium)).color(t.ink())).child(input)
}

/// A pop-up on a chip in the panel. Its menu is built into `menus`, which the caller puts above the panel.
fn picker(s: &App, t: &Theme, menus: &mut Vec<El>, key: &str, name: &str, face: El, menu: DropdownMenu<App>) -> El {
    let open = s.menu_open.as_deref() == Some(key);
    let k = key.to_owned();
    let face = face.expanded(open).focusable(true).child(glyph(&crate::ui::icons::CHEVRON_UP_DOWN, t.control.icon_sm, t.ink_faint())).on_press(move |s: &mut App| {
        s.menu_open = if s.menu_open.as_deref() == Some(k.as_str()) { None } else { Some(k.clone()) };
    });
    let menu = menu.trigger(face).open(open).on_close(|s: &mut App| s.menu_open = None);
    menu_named(menu, t, menus, name, open)
}

fn pickers(s: &App, t: &Theme, f: &RoundForm, menus: &mut Vec<El>) -> El {
    let cat = s.cat(&f.cat);
    let cat_name = cat.map_or_else(|| "No Cattain".to_owned(), |c| c.name.clone());
    let mut cat_chip = chip(t, "round-cat", None).role("button").label(format!("Cattain: {cat_name}"));
    match cat {
        Some(c) => cat_chip = cat_chip.child(crate::faces::portrait(t, "round-cat-face", &c.face, 20.0)),
        None => cat_chip = cat_chip.child(glyph(&crate::icons::ROUNDS, t.control.icon_sm, t.ink_muted())),
    }
    cat_chip = cat_chip.child(text(cat_name).font(t.ty.sm).color(t.ink()).truncate().max_w(160.0).min_w(0.0).a11y_hidden());
    let mut cat_menu = DropdownMenu::new("round-cat-menu").min_width(200.0).label("Cattain");
    cat_menu = cat_menu.item(DropdownMenuItem::new("No Cattain").checked(f.cat.is_empty()).on_select(|s: &mut App| {
        let f = s.form_mut();
        f.cat.clear();
        f.standing = false;
        s.menu_open = None;
    }));
    for c in s.active_cats() {
        let id = c.id.clone();
        cat_menu = cat_menu.item(DropdownMenuItem::new(c.name.clone()).checked(c.id == f.cat).on_select(move |s: &mut App| {
            s.form_mut().cat = id.clone();
            s.menu_open = None;
        }));
    }
    let ws = s.workspace_of(&f.workspace).map_or_else(|| "No project".to_owned(), |w| w.name.clone());
    let ws_chip = chip(t, "round-workspace", None)
        .role("button")
        .label(format!("Project: {ws}"))
        .child(glyph(&crate::ui::icons::FOLDER, t.control.icon_sm, t.ink_muted()))
        .child(text(ws).font(t.ty.sm).color(t.ink()).truncate().max_w(180.0).min_w(0.0).a11y_hidden());
    let mut ws_menu = DropdownMenu::new("round-workspace-menu").min_width(220.0).label("Project");
    for w in s.workspaces.iter().filter(|w| w.is_active()) {
        let id = w.id.clone();
        ws_menu = ws_menu.item(DropdownMenuItem::new(w.name.clone()).checked(w.id == f.workspace).on_select(move |s: &mut App| {
            s.form_mut().workspace = id.clone();
            s.menu_open = None;
        }));
    }
    let mut chips = row().w_full().gap(t.space.at(2.0)).center_y().child(picker(s, t, menus, "round-cat-menu", "Cattains", cat_chip, cat_menu));
    // A standing round posts into the Cattain's own thread, which has its own folder: no project to choose.
    if !f.standing {
        chips = chips.child(picker(s, t, menus, "round-workspace-menu", "Projects", ws_chip, ws_menu));
    }
    let mut out = col().w_full().gap(t.space.at(2.5)).child(chips);
    if let Some(c) = cat {
        let on = f.standing;
        let label = format!("Post in {}'s own thread", c.name);
        let sw = Switch::<App>::new("round-standing", label.clone(), on).on_toggle(move |s: &mut App| s.form_mut().standing = !on).build(t);
        let words = if on { format!("Each time, the words go to {} as a message in its own thread.", c.name) } else { "Each time starts a thread of its own.".to_owned() };
        out = out.child(
            row()
                .key("round-standing-row")
                .w_full()
                .gap(t.space.at(2.5))
                .center_y()
                .child(sw)
                .child(col().min_w(0.0).shrink().gap(t.space.at(0.5)).child(text(label).font(t.ty.sm).color(t.ink()).a11y_hidden()).child(text(words).key("round-standing-words").font(t.ty.xs).color(t.ink_muted()).wrap().min_w(0.0))),
        );
    }
    out
}

fn schedule_fields(s: &App, t: &Theme, f: &RoundForm) -> El {
    let kinds = SegmentedControl::<App>::new("round-kind", "How often", &KINDS, f.kind).size(Size::Sm).on_change(|s: &mut App, i| s.form_mut().kind = i).build(t);
    let time = |f: &RoundForm| {
        Input::new("round-time", &f.time, |s: &mut App| &mut s.form_mut().time).name("Time").placeholder("09:00").description("Hours and minutes, 24-hour, like 09:00").width(120.0).invalid(f.schedule().is_err() && f.kind < 2).build(t)
    };
    let mut col_ = col().key("round-schedule").w_full().gap(t.space.at(2.5)).child(kinds);
    match f.kind {
        0 => col_ = col_.child(row().gap(t.space.at(2.0)).center_y().child(text("at").font(t.ty.base).color(t.ink_muted())).child(time(f))),
        1 => {
            let mut days = row().key("round-days").role("group").label("Days").gap(t.space.at(1.5)).flex_wrap();
            for d in neko_protocol::schedule::Weekday::ALL {
                let on = f.days[d.index()];
                let b = Button::new(format!("round-day-{}", d.index()), d.short()).variant(if on { ButtonVariant::Default } else { ButtonVariant::Secondary }).size(Size::Sm).on_press(move |s: &mut App| {
                    let f = s.form_mut();
                    f.days[d.index()] = !f.days[d.index()];
                });
                days = days.child(b.build(t).role("checkbox").label(d.name()).checked(on));
            }
            col_ = col_.child(days).child(row().gap(t.space.at(2.0)).center_y().child(text("at").font(t.ty.base).color(t.ink_muted())).child(time(f)));
        }
        2 => {
            let hours = Input::new("round-hours", &f.hours, |s: &mut App| &mut s.form_mut().hours).name("Hours").placeholder("6").width(80.0).invalid(f.schedule().is_err()).build(t);
            col_ = col_.child(row().gap(t.space.at(2.0)).center_y().child(text("Every").font(t.ty.base).color(t.ink_muted())).child(hours).child(text("hours, from the moment you save").font(t.ty.base).color(t.ink_muted())));
        }
        _ => {}
    }
    let first = first_run_words(f, s.now_ms, &*s.zone());
    col_.child(text(first).key("round-first").role("status").font(weight(t.ty.sm, t.ty.semibold)).color(if f.schedule().is_ok() { t.c.accent.fg } else { t.c.warning.fg }).wrap().min_w(0.0))
}

/// The New round / Edit panel.
pub fn dialog(s: &App, t: &Theme, layer: &mut Vec<El>) {
    let Some(f) = &s.round_form else { return };
    let mut menus = Vec::new();
    let editing = f.id.is_some();
    let name = Input::new("round-name", &f.name, |s: &mut App| &mut s.form_mut().name).name("Name").placeholder("Docs check").build(t);
    let words = Textarea::new("round-words", &f.words, |s: &mut App| &mut s.form_mut().words).name("What to do").placeholder("Check the docs for broken links and fix the ones you can.").rows(3, 8).build(t);
    let mut body = col()
        .key("round-form")
        .w_full()
        .gap(t.space.at(4.0))
        .child(field(t, "Name", name))
        .child(field(t, "What to do", words))
        .child(field(t, "Who and where", pickers(s, t, f, &mut menus)))
        .child(field(t, "How often", schedule_fields(s, t, f)));
    if let Some(e) = &f.error {
        body = body.child(text(e.clone()).key("round-error").role("alert").font(t.ty.sm).color(t.c.danger.fg).wrap().min_w(0.0));
    }
    body = body.child(text("Rounds run while Neko is running and this Mac is awake.").key("round-honest").font(t.ty.xs).color(t.ink_faint()).wrap().min_w(0.0));
    let cancel = Button::new("round-cancel", "Cancel").variant(ButtonVariant::Secondary).on_press(|s: &mut App| s.round_form = None).build(t);
    let save = Button::new("round-save", if editing { "Save" } else { "Make round" }).disabled(!f.ready()).on_press(|s: &mut App| s.save_round()).build(t);
    let back = match &f.id {
        Some(id) => format!("round-{id}-menu"),
        None => "rounds-new".to_owned(),
    };
    let _ = GENERAL;
    Dialog::new("round", if editing { "Edit round" } else { "New round" })
        .description("Work that starts by itself at its times.")
        .child(body)
        .footer(vec![cancel, save])
        .width(540.0)
        .open(true)
        .return_focus(back)
        .on_close(|s: &mut App| s.round_form = None)
        .build(t, layer);
    // The pop-up lists sit above the panel, never under it.
    layer.extend(menus);
}

#[allow(dead_code)]
fn unused(_: TaskStatus) {}
