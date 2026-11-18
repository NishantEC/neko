//! The Board layout of All sessions, as the web draws it (`work/board/TaskBoard.tsx`, `BoardCard.tsx`, `NotchCard.tsx`; `board.rs`
//! holds the rules). Calm columns with no panel: 12 points of room between them, each under a 32 high header (its status glyph, name,
//! count, and on hover a menu), a column with nothing in it a slim 144 note, the others at least 208 wide, scrolling sideways. A card
//! is the web's compact `NotchCard`: a rounded surface (the foreground at 9% over the page in dark) with a 1.5 point frame that is
//! Iris while it is the open task, the title on at most two lines, why it waits, and a last line that says where it stands, its one
//! action (Approve, Answer, Start again) laid over the end of that line on hover or focus. Done cards end on their meta line. Done
//! shows its newest five and "Show all N". Rune keeps its own drag (`board.rs`), the right-click menu and the unseen dot.

use rune::{Color, Cursor, El, Phase, Shadow, TextStyle, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement, context_menu};

use super::{ui, webkit};
use crate::all_sessions::Stage;
use crate::app::App;
use crate::board::{CardAction, Decision, card_words, decide, stage_empty};
use crate::icons_phosphor as ph;
use crate::link::TaskRow;
use crate::status::{StatusKind, status_glyph, status_of};
use crate::theme::Neko;
use neko_protocol::workbench::TaskStatus;

/// How many Done cards show until "Show all".
const DONE_CAP: usize = 5;

/// The web's `STAGE_GLYPH`.
fn stage_glyph(stage: Stage) -> StatusKind {
    match stage {
        Stage::NeedsYou | Stage::Running => StatusKind::InProgress,
        Stage::Review => StatusKind::Review,
        Stage::UpNext => StatusKind::Todo,
        Stage::Done => StatusKind::Done,
    }
}

/// What dropping the carried card on `stage` would do (None when nothing is carried).
fn verdict(s: &App, stage: Stage) -> Option<Decision> {
    let carry = s.all.carry.as_ref().filter(|c| c.moved)?;
    let status = s.summary.as_ref()?.task(&carry.id)?.status;
    Some(decide(status, stage))
}

/// The card's surface (`--fuse`): the foreground at 9% over the page in dark, at 6% over the card in light; a step more on hover.
fn fuse(t: &Neko, step: f32) -> Color {
    // Measured on the web page: the dark card is rgb(29, 29, 29) (`--sky-pebble`).
    let (base, ink) = if t.dark { (rune::color::hex("#1d1d1d").unwrap_or(t.surface.window), 0.0) } else { (t.surface.card, 0.06) };
    base.mix(t.text.base, ink + step)
}

/// The web's `shortAge`: "18m", "6h", "3d".
pub fn short_age(now_ms: i64, ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let m = ((now_ms - ms).max(0) as f64 / 60_000.0).round() as i64;
    match m {
        0 => "now".to_owned(),
        1..=59 => format!("{m}m"),
        60..=1439 => format!("{}h", (m as f64 / 60.0).round() as i64),
        _ => format!("{}d", (m as f64 / 1440.0).round() as i64),
    }
}

/// The web's `clock`: "2m 08s", "1h 05m".
fn clock(seconds: i64) -> String {
    let s = seconds.max(0);
    if s >= 3600 { format!("{}h {:02}m", s / 3600, (s % 3600) / 60) } else { format!("{}m {:02}s", s / 60, s % 60) }
}

fn measure(words: &str, style: TextStyle) -> f32 {
    rune::text::measure(words, &[], style, None).0
}

/// How tall `words` is when wrapped at `width`.
fn measure_h(words: &str, style: TextStyle, width: f32) -> f32 {
    rune::text::measure(words, &[], style, Some(width)).1
}

/// What `cardFor` decides for a task, from its row.
struct Face {
    reason: Option<String>,
    action: Option<CardAction>,
    label: &'static str,
    age: String,
    done: bool,
    live: bool,
    /// "Cancelled · repo · source · 8d".
    meta: String,
}

fn face_of(s: &App, task: &TaskRow) -> Face {
    let (label, action) = card_words(task);
    let line = &task.line;
    let reason = match task.status {
        TaskStatus::Failed | TaskStatus::AwaitingApproval if !line.note.is_empty() => Some(line.note.strip_prefix("Needs your input: ").unwrap_or(&line.note).to_owned()),
        _ => None,
    };
    let age = short_age(s.now_ms, task.updated_at_ms);
    let meta = [(task.status == TaskStatus::Cancelled).then(|| "Cancelled".to_owned()), Some(line.repo.clone()), Some(line.source.clone()), Some(age.clone())].into_iter().flatten().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");
    Face { reason, action, label, age, done: Stage::of(task.status) == Stage::Done, live: matches!(task.status, TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing), meta }
}

/// The accessible name (`cardLabel`): title first, then where it stands, then the rest.
fn card_label(s: &App, task: &TaskRow) -> String {
    let f = face_of(s, task);
    let state = if f.done || f.live { crate::board::friendly_status(task.status) } else { f.label };
    let line = &task.line;
    [Some(task.title.clone()), Some(state.to_owned()), f.reason.clone(), Some(line.repo.clone()), Some(line.source.clone()), Some(line.pr.clone()).filter(|p| !p.is_empty()).map(|p| format!("pull request {p}")), Some(line.short_id.clone())]
        .into_iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn small(t: &Neko) -> TextStyle {
    webkit::font(t, 12.0, 400.0, 19.5)
}

/// The state line of a card that has not finished (`NotchState`): the glyph, the state in words with its age, and the repository.
fn state_line(s: &App, t: &Neko, task: &TaskRow, f: &Face, inner: f32) -> El {
    let line = &task.line;
    let glyph = status_glyph(status_of(task.status), t, 12.0, Some(""));
    let pr = (!line.pr.is_empty()).then(|| {
        let w = measure(&line.pr, webkit::tnum(small(t))) + 13.0;
        (row().gap(1.0).center_y().child(icon(&ph::GIT_PULL_REQUEST, 12.0).color(t.text.muted).a11y_hidden()).child(text(line.pr.clone()).font(webkit::tnum(small(t))).color(t.text.muted).a11y_hidden()), w)
    });
    let mut left = row().key(format!("board-state-{}", task.id)).min_w(0.0).gap(6.0).center_y().child(glyph);
    if f.live {
        let step = crate::board::friendly_status(task.status);
        let seconds = ((s.now_ms - task.updated_at_ms) / 1000).max(0);
        left = left.child(row().child(text(step).font(webkit::tnum(small(t))).color(t.text.muted).a11y_hidden()).child(text(format!(" · {}", clock(seconds))).font(webkit::tnum(small(t))).color(t.text.muted).a11y_hidden()));
    } else {
        // As the web's flex-wrap does it: the age shows only when label, age and detail all fit; otherwise it wraps onto a second line the
        // state's box clips (its place stays, so the detail still starts after it) and the detail gives way.
        let avail = inner - 18.0 - pr.as_ref().map_or(0.0, |(_, w)| w + 8.0);
        let label_w = measure(f.label, small(t));
        let age_w = measure(&f.age, webkit::tnum(small(t)));
        let detail = if line.pr.is_empty() { line.repo.clone() } else { String::new() };
        let detail_words = format!("· {detail}");
        let detail_w = if detail.is_empty() { 0.0 } else { measure(&detail_words, small(t)) };
        let state_w = if f.age.is_empty() { label_w } else { label_w + 6.0 + age_w };
        let with_age = !f.age.is_empty() && state_w + if detail.is_empty() { 0.0 } else { 6.0 + detail_w } <= avail + 0.01;
        let mut state = row().key(format!("board-words-{}", task.id)).gap(6.0).center_y().w(state_w.min(avail)).child(text(f.label).font(small(t)).color(t.text.base.alpha(0.75)).truncate().w(label_w.min(avail)).shrink().a11y_hidden());
        if with_age {
            state = state.child(text(f.age.clone()).key(format!("board-age-{}", task.id)).font(webkit::tnum(small(t))).color(t.text.muted).a11y_hidden());
        }
        left = left.child(state);
        let room = avail - state_w.min(avail) - 6.0;
        if !detail.is_empty() && room > 8.0 {
            // JSX makes two text nodes of `· {detail}`; the dot is 7 points before the words.
            let dot_w = measure("· ", small(t));
            left = left.child(
                row().key(format!("board-detail-{}", task.id)).w(detail_w.min(room)).min_w(0.0).child(text("·").font(small(t)).color(t.text.muted).mr(dot_w - measure("·", small(t))).a11y_hidden()).child(
                    text(detail.clone()).font(small(t)).color(t.text.muted).truncate().shrink().a11y_hidden(),
                ),
            );
        }
    }
    let mut r = row().key(format!("board-notch-{}", task.id)).w_full().min_h(22.0).mb(-2.0).gap(8.0).center_y().justify_between().child(left);
    if let Some((mark, _)) = pr {
        r = r.child(mark);
    }
    if let Some(a) = f.action {
        // The action lies over the end of the line on hover or focus: the card's own colour under it, so the words do not show through.
        let id = task.id.clone();
        let button = row()
            .key(format!("board-action-{}", task.id))
            .role("button")
            .label(a.label())
            .focusable(true)
            .h(22.0)
            .px(8.0)
            .gap(6.0)
            .center()
            .radius(11.0)
            .fill(fuse(t, 0.0))
            .hover(t.text.base.alpha(0.08))
            .focus_ring(2.0, 2.0, t.kit.c.border_focus)
            .on_press(move |s: &mut App| s.card_act(&id, a))
            .child(icon(action_icon(a), 12.0).color(t.text.base).a11y_hidden())
            .child(text(a.label()).font(webkit::font(t, 12.0, 500.0, 19.5)).color(t.text.base).a11y_hidden());
        r = r.child(row().key(format!("board-action-slot-{}", task.id)).pin(None, Some(0.0), Some(0.0), Some(0.0)).center_y().group_opacity(0.0, 1.0).child(button.mr(-4.0)));
    }
    r
}

fn action_icon(a: CardAction) -> &'static rune::Icon {
    match a {
        CardAction::Approve => &ph::CHECK_BOLD,
        CardAction::Answer => &ph::ARROW_BEND_UP_LEFT_BOLD,
        CardAction::StartAgain => &ph::ARROW_COUNTER_CLOCKWISE_BOLD,
    }
}

/// The card's face (`CardFace`): what the carried copy shows too. `w` is the card's width.
fn face(s: &App, t: &Neko, task: &TaskRow, key: String, w: f32, selected: bool, with_action: bool) -> El {
    let mut f = face_of(s, task);
    if !with_action {
        f.action = None;
    }
    let unseen = s.unseen.contains(&task.id);
    let inner = w - 3.0 - 24.0;
    let title_style = webkit::font(t, 14.0, 500.0, 22.75);
    let lines_full = (measure_h(&task.title, title_style, inner) / 22.0).round().max(1.0);
    let mut title = text(task.title.clone()).key(format!("board-title-{}", task.id)).font(title_style).color(t.text.strong).max_lines(2).w_full().h(lines_full.min(2.0) * 22.75).a11y_hidden();
    if unseen {
        title = title.pr(12.0);
    }
    let mut surface = col().key(format!("board-surface-{}", task.id)).w_full().pl(12.0).pr(12.0).pt(12.0).pb(if f.done { 12.0 } else { 8.0 }).gap(6.0).radius(14.5).fill(if selected { t.meaning.iris.mix(fuse(t, 0.0), 0.94) } else { Color::TRANSPARENT }).child(title);
    if lines_full > 2.0 {
        // The web's title is its whole text, clipped to two lines by `line-clamp`; its box is as tall as all of it. The same words,
        // laid out in full and drawn with no ink, so the card's text is where the web's is (the visible title is the clamped one).
        surface = surface.child(text(task.title.clone()).key(format!("board-title-full-{}", task.id)).font(title_style).color(t.text.strong).wrap().w(inner).opacity(0.0).absolute(12.0, 12.0).a11y_hidden());
    }
    if let Some(reason) = f.reason.clone() {
        surface = surface.child(text(reason).key(format!("board-reason-{}", task.id)).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.base.alpha(0.7)).max_lines(1).w_full().a11y_hidden());
    }
    if f.done {
        let line = &task.line;
        let id_style = webkit::tnum(small(t));
        let mut meta = row().key(format!("board-meta-{}", task.id)).w_full().gap(8.0).center_y().child(
            row().min_w(0.0).grow(1.0).center_y().child(status_glyph(status_of(task.status), t, 12.0, Some("")).mr(4.0)).child(text(line.short_id.clone()).font(id_style).color(t.text.muted.alpha(0.85)).a11y_hidden()).child(
                text(if f.meta.is_empty() { String::new() } else { format!(" · {}", f.meta) }).font(small(t)).color(t.text.muted).truncate().shrink().a11y_hidden(),
            ),
        );
        if !line.pr.is_empty() {
            meta = meta.child(row().gap(1.0).center_y().child(icon(&ph::GIT_PULL_REQUEST, 12.0).color(t.text.muted).a11y_hidden()).child(text(line.pr.clone()).font(webkit::tnum(small(t))).color(t.text.muted).a11y_hidden()));
        }
        surface = surface.child(meta);
    } else {
        surface = surface.child(state_line(s, t, task, &f, inner));
    }
    let mut card = col().key(key).w(w).padding(1.5).radius(16.0).fill(if selected { t.meaning.iris } else { fuse(t, 0.0) }).child(surface);
    if unseen {
        card = card.child(el().key(format!("board-unseen-{}", task.id)).pin(None, Some(13.0), Some(11.0), None).size(6.0, 6.0).radius(3.0).fill(t.meaning.iris).a11y_hidden());
    }
    card
}

/// One card: a button that selects, drags, and has the same right-click menu as the list's rows.
fn card(s: &App, t: &Neko, task: &TaskRow, w: f32, cursor_on: bool, layer: &mut Vec<El>) -> El {
    let selected = cursor_on || s.selected.as_deref() == Some(task.id.as_str());
    let (_, action) = card_words(task);
    let id = task.id.clone();
    let carried = s.all.carry.as_ref().is_some_and(|c| c.moved && c.id == task.id);
    let carrying = s.all.carry.as_ref().is_some_and(|c| c.moved);
    let (press_id, drag_id, menu_id) = (id.clone(), id.clone(), id.clone());
    let unseen = s.unseen.contains(&task.id);
    let label = format!("{}{}", card_label(s, task), if unseen { ", not yet seen" } else { "" });
    let mut c = face(s, t, task, format!("board-card-{id}"), w, selected, true)
        .role("button")
        .label(label)
        .help("Drag to another column to move it")
        .focusable(true)
        .hover_group()
        .hover(if selected { t.meaning.iris } else { fuse(t, 0.04) })
        .focus_ring(2.0, 2.0, t.surface.ring)
        .opacity(if carried { 0.4 } else { 1.0 })
        .on_press(move |s: &mut App| {
            // The press that ends a carry is not a click.
            if std::mem::take(&mut s.all.suppress_press) {
                return;
            }
            s.select(&press_id);
        })
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _| {
            let travel = (d.dx, d.dy);
            s.board_drag(&drag_id, d.phase, (d.start_x, d.start_y), travel, d.window_point(), d.phase == Phase::Up && d.cancelled());
        })
        .on_secondary_press(move |s: &mut App, p, _| s.row_menu = Some((menu_id.clone(), p)));
    if carrying {
        c = c.cursor(match s.all.carry.as_ref().and_then(|carry| carry.over.and_then(|over| verdict(s, over))) {
            Some(Decision::Refuse(_)) | None => Cursor::NotAllowed,
            _ => Cursor::Grabbing,
        });
    }
    match s.row_menu.as_ref().filter(|(open, _)| *open == task.id) {
        Some((_, at)) => {
            let mut menu = context_menu(format!("row-menu-{}", task.id), c, Some(*at)).on_close(|s: &mut App| s.row_menu = None);
            if let Some(a) = action {
                let owner = task.id.clone();
                menu = menu.item(MenuItem::new(a.label()).on_select(move |s: &mut App| {
                    s.row_menu = None;
                    s.card_act(&owner, a);
                }));
            }
            let owner = task.id.clone();
            menu = menu.item(MenuItem::new("Open").on_select(move |s: &mut App| {
                s.row_menu = None;
                s.select(&owner);
            }));
            menu = menu.separator();
            crate::task_actions::fill_menu(menu, s, &task.id).build(&t.kit, layer)
        }
        None => c,
    }
}

/// The dashed slot a column shows at its foot while an allowed card is over it: what the drop will do.
fn drop_slot(t: &Neko, words: &str, stage: Stage) -> El {
    row()
        .key(format!("board-slot-{}", stage.title()))
        .role("status")
        .label(words.to_owned())
        .w_full()
        .min_h(64.0)
        .px(12.0)
        .py(12.0)
        .center()
        .radius(12.0)
        .fill(t.meaning.iris.alpha(0.06))
        .border(1.0, t.meaning.iris.alpha(0.5))
        .border_dash(6.0, 4.0)
        .child(text(words.to_owned()).font(webkit::font(t, 12.5, 400.0, 17.5)).color(t.meaning.iris).a11y_hidden())
}

/// `ColumnHeader`: 32 high, 2 at the sides, the stage's glyph (13), its name, its count, then at the right the menu.
fn header(s: &App, t: &Neko, layer: &mut Vec<El>, stage: Stage, count: usize, solo: bool) -> El {
    let style = webkit::font(t, 12.5, 500.0, 20.3125);
    let mut head = row()
        .key(format!("board-head-{}", stage.title()))
        .w_full()
        .h(32.0)
        .px(2.0)
        .gap(6.0)
        .center_y()
        .hover_group()
        .child(status_glyph(stage_glyph(stage), t, 13.0, Some("")))
        .child(text(stage.title()).key(format!("board-title-col-{}", stage.title())).font(style).color(t.text.muted).truncate().min_w(0.0).a11y_hidden())
        .child(text(count.to_string()).key(format!("board-count-{}", stage.title())).font(webkit::tnum(TextStyle { weight: 400.0, ..style })).color(t.text.muted.alpha(0.7)).a11y_hidden())
        .child(el().grow(1.0).min_w(0.0));
    let ghost = |key: String, label: String, glyph: &'static rune::Icon, size: f32| {
        row().key(key).role("button").label(label.clone()).help(label).focusable(true).size(28.0, 28.0).center().radius(14.0).hover(t.text.base.alpha(0.06)).focus_ring(2.0, 2.0, t.kit.c.border_focus).child(icon(glyph, size).color(t.text.base.alpha(0.8)).a11y_hidden())
    };
    if stage == Stage::UpNext {
        head = head.child(ghost("board-new".to_owned(), "New task".to_owned(), &ph::PLUS, 14.0).on_press(|s: &mut App| s.open_new()));
    }
    if count > 0 && !solo {
        let title = stage.title();
        let trigger = ghost(format!("board-more-{title}"), format!("{title} options"), &ph::DOTS_THREE_BOLD, 14.0).on_press(move |s: &mut App| s.all.board_menu = Some(stage));
        let menu = Menu::new(format!("board-menu-{title}"))
            .trigger(row().group_opacity(0.0, 1.0).child(trigger))
            .open(s.all.board_menu == Some(stage))
            .placement(Placement::BottomEnd)
            .on_close(|s: &mut App| s.all.board_menu = None)
            .item(MenuItem::new(format!("Show only {title}")).on_select(move |s: &mut App| {
                s.all.board_menu = None;
                s.all.stage = Some(stage);
                s.all.cursor = 0;
            }))
            .build(&t.kit, layer);
        head = head.child(menu);
    }
    head
}

fn column(s: &App, t: &Neko, stage: Stage, cursor_id: Option<&str>, solo: bool, layer: &mut Vec<El>) -> El {
    let tasks = s.board_column(stage);
    let decision = verdict(s, stage);
    let over = s.all.carry.as_ref().is_some_and(|c| c.moved && c.over == Some(stage));
    let empty = tasks.is_empty();
    let width = crate::board::column_width(empty, solo);
    let card_w = width - 12.0;
    let capped = stage == Stage::Done && !s.all.done_all && tasks.len() > DONE_CAP;
    let shown = if capped { &tasks[..DONE_CAP] } else { &tasks[..] };
    let mut cards = col().key(format!("board-cards-{}", stage.title())).w_full().pt(6.0).pb(16.0).gap(8.0);
    for task in shown {
        cards = cards.child(card(s, t, task, card_w, cursor_id == Some(task.id.as_str()), layer));
    }
    if stage == Stage::Done && tasks.len() > DONE_CAP {
        let words = if s.all.done_all { "Show fewer".to_owned() } else { format!("Show all {}", tasks.len()) };
        cards = cards.child(
            row()
                .key("board-show-all")
                .role("button")
                .label(words.clone())
                .expanded(s.all.done_all)
                .focusable(true)
                .w_full()
                .h(32.0)
                .center()
                .radius(20.0)
                .hover(t.text.base.alpha(0.04))
                .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                .on_press(|s: &mut App| s.all.done_all = !s.all.done_all)
                .child(text(words).font(webkit::font(t, 12.5, 500.0, 20.3125)).color(t.text.muted).a11y_hidden()),
        );
    }
    if let Some(Decision::Move(m)) = &decision
        && over
    {
        cards = cards.child(drop_slot(t, m.verb(), stage));
    }
    if empty && !matches!(decision, Some(Decision::Move(_))) {
        cards = cards.child(text(stage_empty(stage)).key(format!("board-empty-{}", stage.title())).font(webkit::font(t, 12.5, 400.0, 18.75)).color(t.text.muted).wrap().px(2.0).py(4.0));
    }
    let head = header(s, t, layer, stage, tasks.len(), solo);
    let mut c = col()
        .key(format!("board-col-{}", stage.title()))
        .role("group")
        .label(format!("{}, {}", stage.title(), tasks.len()))
        .w(width)
        .h_full()
        .min_h(0.0)
        .pl(6.0)
        .pr(6.0)
        .radius(20.0)
        .child(head)
        .child(col().key(format!("board-scroll-{}", stage.title())).w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(cards));
    match decision {
        // An allowed column lights up Iris, brighter under the pointer; a refused one dims, shows the not-allowed cursor, and does not light up.
        Some(Decision::Move(_)) => c = c.cursor(Cursor::Grabbing).fill(t.meaning.iris.alpha(if over { 0.06 } else { 0.0 })).border(1.0, t.meaning.iris.alpha(if over { 0.45 } else { 0.15 })),
        Some(Decision::Refuse(_)) => c = c.cursor(Cursor::NotAllowed).opacity(0.55),
        _ => {}
    }
    c
}

/// The carried copy, drawn over the window at the pointer where the card was taken.
fn ghost(s: &App, t: &Neko, w: f32) -> Option<El> {
    let carry = s.all.carry.as_ref().filter(|c| c.moved)?;
    let task = s.summary.as_ref()?.task(&carry.id)?;
    let (x, y) = (carry.at.0 - carry.grab.0, carry.at.1 - carry.grab.1);
    Some(face(s, t, task, "board-ghost".to_owned(), w, false, false).box_shadow(Shadow { y: 10.0, blur: 24.0, spread: -6.0, color: Color::BLACK.alpha(0.28), ..Default::default() }).pin(Some(x), Some(y), None, None))
}

/// The board: the columns side by side, scrolling sideways; the carried card floats above.
pub fn body(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    if s.summary.is_none() {
        return ui::quiet_line(t, "board-none", "Neko isn't running yet.");
    }
    let cursor_id: Option<String> = if s.all.cursor_shown { s.nav_ids().get(s.all.cursor).cloned() } else { None };
    let stages = s.board_stages();
    let solo = stages.len() == 1;
    // The columns bleed 6 either side (`-mx-1.5`), so the first one starts 22 in.
    let mut columns = row().key("board-columns").h_full().pl(22.0).pr(22.0).pt(4.0);
    for stage in stages {
        columns = columns.child(column(s, t, stage, cursor_id.as_deref(), solo, layer));
    }
    if let Some(g) = ghost(s, t, crate::board::column_width(false, false) - 20.0) {
        layer.push(g);
    }
    col().key("board").w_full().grow(1.0).min_h(0.0).child(row().key("board-scroll-x").w_full().h_full().min_h(0.0).scroll_x().child(columns))
}
