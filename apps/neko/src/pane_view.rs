//! The right pane (docs/knowledge/app-shell.md, "The right pane"): a header of tabs on the band's line, then the tab's body. Overview
//! says in a few seconds what needs the person, which kittens work, what changed, where and on what branch, what the thread waits on
//! and what it used. Changes is the diff (`review_view.rs`). A kitten's tab is its own conversation, read here. The state and the
//! words are `pane.rs`.
//!
//! Docked, the pane is a region of the window (rune-ui's Split) on the sidebar's panel material with a hairline at its edge; in a
//! window under 1100 points it is a sheet over the conversation's right edge. Either way the same header and body are drawn.

use std::rc::Rc;

use neko_protocol::undo::{Action, Offer};
use neko_protocol::{DiffSource, NeedsItem, Need, Overview, Task, TaskId, TaskStatus};
use rune::{El, Tx, col, el, row, text};
use rune_ui_style::{ButtonVariant, tabular, weight};

use crate::pane::Tab;
use crate::state::App;
use crate::subagents::{kitten_word, name, order_words, place_words};
use crate::subagents_view::{still_ring, status_glyph};
use crate::ui::agent::{PressTx, icon_button, mono};
use crate::ui::base::{Shape, Size, States, Theme, glyph, paint_state};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::tabs::{Tabs, TabsVariant};
use crate::view::BAND;

/// A section's heading: its words, and how many are in it.
fn heading(t: &Theme, key: &str, words: &str, count: Option<usize>) -> El {
    let mut h = row().key(key.to_owned()).w_full().gap(t.space.at(1.5)).center_y().child(text(words.to_owned()).role("heading").level(3).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted()));
    if let Some(n) = count {
        h = h.child(text(n.to_string()).font(tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden());
    }
    h
}

/// A section: its heading, then its rows.
fn section(t: &Theme, key: &str, words: &str, count: Option<usize>, rows: Vec<El>) -> El {
    col().key(key.to_owned()).role("group").label(words.to_owned()).w_full().gap(t.space.at(1.5)).child(heading(t, &format!("{key}-heading"), words, count)).children(rows)
}

/// The age of a thing, in a few words.
fn age(s: &App, ms: i64) -> String {
    crate::view::since(s, ms)
}

/// A row that opens something: the row material's hover and focus ring, a button's role.
fn press_row(t: &Theme, key: String, label: String, h: f32) -> El {
    let r = row().key(key).role("button").label(label).focusable(true).w_full().h(h).px(t.space.at(1.5)).gap(t.space.at(2.0)).center_y().radius(t.radius.md).round_as(t).hover_group();
    paint_state(r, &t.mat.row, States::default())
}

/// A small word-button in a row's right slot.
fn slot_button(t: &Theme, key: String, words: &str, name: &str, v: ButtonVariant, f: impl Fn(&mut App) + 'static) -> El {
    Button::<App>::new(key, words).variant(v).size(Size::Sm).on_press(f).build(t).label(name.to_owned())
}

// ---- Overview -------------------------------------------------------------------------------------------------------------------------

/// What waits for the person in this thread, its kittens' too: an amber dot, what is asked, how long ago; the answers show under the pointer.
fn needs(s: &App, t: &Theme, root: &TaskId) -> Option<El> {
    let items: Vec<&NeedsItem> = s.visible_needs().into_iter().filter(|i| !i.task.0.is_empty() && &s.root_of(&i.task) == root).collect();
    if items.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    for item in &items {
        let key = format!("pane-need-{}", item.id.replace(':', "-"));
        let words = match (&item.subagent, &item.need) {
            (Some(n), Need::Permission { .. }) => format!("{} asks: {}", name(n), crate::needs_view::what(item)),
            (Some(n), _) => format!("{}: {}", name(n), crate::needs_view::what(item)),
            (None, _) => crate::needs_view::what(item),
        };
        let since = age(s, item.since_ms);
        let mut line = press_row(t, key.clone(), format!("{words}, {since}"), t.control.md + 4.0);
        let target = item.subagent.as_ref().map(|_| item.task.clone());
        line = line.child(el().size(8.0, 8.0).radius(4.0).fill(t.c.warning.fg).a11y_hidden());
        line = line.child(text(words).font(t.ty.sm).color(t.ink()).truncate().grow(1.0).min_w(0.0).a11y_hidden());
        // The age gives way to the answers under the pointer, as a thread's row gives way to its menu.
        let slot_w = if matches!(item.need, Need::Permission { .. }) { 138.0 } else { 36.0 };
        let said = row().absolute(0.0, 0.0).size(slot_w, t.control.md).justify_end().center_y().child(text(since).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden()).group_opacity(1.0, 0.0);
        let mut acts = row().absolute(0.0, 0.0).size(slot_w, t.control.md).justify_end().center_y().gap(t.space.at(1.0)).group_opacity(0.0, 1.0);
        if let Need::Permission { permission } = &item.need {
            let (a, b) = (item.id.clone(), item.id.clone());
            let destructive = permission.destructive;
            acts = acts
                .child(slot_button(t, format!("{key}-allow"), "Allow once", &format!("Allow once: {}", permission.title), if destructive { ButtonVariant::Destructive } else { ButtonVariant::Secondary }, move |s: &mut App| s.answer_need(&a, neko_protocol::Decision::AllowOnce)))
                .child(slot_button(t, format!("{key}-deny"), "Deny", &format!("Deny: {}", permission.title), ButtonVariant::Ghost, move |s: &mut App| s.answer_need(&b, neko_protocol::Decision::Deny)));
        }
        line = line.child(el().size(slot_w, t.control.md).child(said).child(acts));
        rows.push(line.on_press(move |s: &mut App| match &target {
            Some(k) => s.open_kitten_tab(k),
            None => s.close_sheet(),
        }));
    }
    Some(section(t, "pane-needs", "Needs you", Some(items.len()), rows))
}

/// The kittens of the thread as a tree, numbered `1`, `1.1`, `1.2`, each step in 12 points.
fn kittens(s: &App, t: &Theme, root: &TaskId) -> Option<El> {
    let mut list: Vec<&Task> = s.tasks.iter().filter(|x| x.subagent.as_ref().is_some_and(|o| &o.root == root)).collect();
    if list.is_empty() {
        return None;
    }
    list.sort_by_key(|x| x.subagent.as_ref().map(|o| o.number.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>()));
    let mut rows = Vec::new();
    for k in &list {
        let Some(of) = &k.subagent else { continue };
        let who = name(&of.number);
        let word = kitten_word(k);
        let depth = of.number.matches('.').count() as f32;
        // The model the run reported (the one that did most of its work, else the one it began on); nothing while that is not known.
        let model = s.books.get(&k.id).and_then(|b| b.runs.last()).and_then(|r| r.usage.as_ref().and_then(|u| u.model.clone()).or_else(|| r.model.clone())).filter(|m| !m.trim().is_empty());
        // Where it works and where it stands in line come first: they are what the person came for, and the model is what gives way
        // when the line is cut.
        let mut facts: Vec<String> = Vec::new();
        facts.push(place_words(k).replace("\u{b7} branch ", "\u{b7} "));
        let order = order_words(k);
        // A kitten that waits its turn has not run: its line is where it works and whom it follows.
        let waiting_in_line = order.is_some();
        facts.extend(order);
        if !waiting_in_line {
            facts.extend(model);
        }
        let key = format!("pane-kitten-{}", of.number);
        let label = format!("{who}: {}, {word}, {}", k.title, age(s, k.created_ms));
        let live = k.is_live_subagent();
        let id = k.id.clone();
        let mut line = press_row(t, key.clone(), label, t.control.lg + 8.0).grow(1.0).min_w(0.0);
        line = line.child(status_glyph(t, k.status, t.control.icon - 2.0));
        let first = row().w_full().gap(t.space.at(1.5)).center_y().child(text(who.clone()).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink()).a11y_hidden()).child(text(k.title.clone()).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).grow(1.0).a11y_hidden());
        let second = text(facts.join(" \u{b7} ")).font(t.ty.xs).color(t.ink_faint()).truncate().min_w(0.0).a11y_hidden();
        line = line.child(col().grow(1.0).min_w(0.0).gap(1.0).child(first).child(second));
        // The state word at the right; under the pointer a live kitten shows Stop in its place.
        let ink = match k.status {
            TaskStatus::Working => t.c.accent.fg,
            TaskStatus::NeedsYou | TaskStatus::Asked => t.c.warning.fg,
            TaskStatus::Failed => t.c.danger.fg,
            _ => t.ink_muted(),
        };
        let w = 96.0;
        let said = row()
            .absolute(0.0, 0.0)
            .size(w, t.control.md)
            .justify_end()
            .center_y()
            .gap(t.space.at(1.5))
            .child(text(word).font(weight(t.ty.xs, t.ty.medium)).color(ink).a11y_hidden())
            .child(text(age(s, k.created_ms)).font(tabular(t.ty.xs)).color(t.ink_faint()).a11y_hidden());
        let mut slot = el().size(w, t.control.md);
        if live {
            let sid = id.clone();
            let stop = row().absolute(0.0, 0.0).size(w, t.control.md).justify_end().center_y().child(slot_button(t, format!("{key}-stop"), "Stop", &format!("Stop {who}"), ButtonVariant::Ghost, move |s: &mut App| s.stop_task(&sid)));
            slot = slot.child(said.group_opacity(1.0, 0.0)).child(stop.group_opacity(0.0, 1.0));
        } else {
            slot = slot.child(said);
        }
        line = line.child(slot);
        // Each step down the tree is 12 points in.
        rows.push(row().w_full().pl(depth * 12.0).child(line.on_press(move |s: &mut App| s.open_kitten_tab(&id))));
    }
    Some(section(t, "pane-kittens", "Kittens", Some(list.len()), rows))
}

/// The newest run of the thread that offers Undo or Redo, with what it offers.
fn offered(s: &App, root: &TaskId) -> Option<(neko_protocol::RunId, Offer)> {
    offered_for(s, root, None)
}

/// What a run offers (`run`), else what the newest run that offers Undo or Redo offers.
pub fn offered_for(s: &App, root: &TaskId, run: Option<&neko_protocol::RunId>) -> Option<(neko_protocol::RunId, Offer)> {
    let b = s.books.get(root)?;
    let all = neko_protocol::undo::offers(&b.runs);
    match run {
        Some(id) => b.runs.iter().zip(all).find(|(r, _)| &r.id == id).map(|(r, o)| (r.id.clone(), o)),
        None => b.runs.iter().zip(all).rev().find(|(_, o)| o.action.is_some()).map(|(r, o)| (r.id.clone(), o)),
    }
}

/// A file with its own counts: name in mono, cut in the middle, then only the figures that say something.
fn file_line(t: &Theme, key: String, path: &str, counts: Option<(u32, u32)>, muted: bool) -> El {
    let mut line = row()
        .key(key)
        .role("listitem")
        .label(match counts {
            Some((a, b)) => format!("{path}, {a} added, {b} removed"),
            None => path.to_owned(),
        })
        .w_full()
        .h(t.control.md)
        .gap(t.space.at(2.0))
        .center_y()
        .child(glyph(&icons::FILE, t.control.icon_sm, t.ink_faint()))
        .child(text(path.to_owned()).font(mono(t, t.ty.sm)).color(if muted { t.ink_muted() } else { t.ink() }).truncate_middle().min_w(0.0).grow(1.0).a11y_hidden());
    if let Some((a, b)) = counts {
        line = line.children(crate::turn::file_figures(t, a, b));
    }
    line
}

/// How many files Overview lists before "Show all".
const FILES: usize = 5;

/// What the thread changed: the figures, the first files, Review, Undo or Redo, and the one quiet line undo sometimes has.
fn changes(s: &App, t: &Theme, root: &TaskId, o: Option<&Overview>) -> Option<El> {
    let c = o.and_then(|o| o.changes.as_ref()).filter(|c| !c.files.is_empty());
    let offer = offered(s, root);
    if c.is_none() && offer.is_none() {
        return None;
    }
    let mut body = Vec::new();
    let undone = offer.as_ref().is_some_and(|(_, o)| o.undone);
    if let Some(c) = c {
        let n = c.files.len() as u32;
        let figures = row()
            .key("pane-changes-figures")
            .role("note")
            .label(format!("{} changed, {} lines added, {} removed{}", crate::pane::files_words(n), c.added, c.removed, if undone { ", now undone" } else { "" }))
            .gap(t.space.at(2.0))
            .center_y()
            .child(text(format!("+{}", c.added)).font(weight(tabular(t.ty.sm), t.ty.semibold)).color(t.c.success.fg).a11y_hidden())
            .child(text(format!("\u{2212}{}", c.removed)).font(weight(tabular(t.ty.sm), t.ty.semibold)).color(t.c.danger.fg).a11y_hidden())
            .child(text(format!("\u{b7} {}", crate::pane::files_words(n))).font(t.ty.sm).color(t.ink_muted()).a11y_hidden());
        body.push(if undone { figures.child(text("\u{b7} now undone").font(t.ty.sm).color(t.ink_muted()).a11y_hidden()) } else { figures });
        let mut list = col().key("pane-changes-files").role("list").label("Files changed").w_full();
        for (i, f) in c.files.iter().take(FILES).enumerate() {
            list = list.child(file_line(t, format!("pane-file-{i}"), &crate::review_view::file_label(f), (!f.binary).then_some((f.added, f.removed)), undone));
        }
        body.push(list);
        if c.files.len() > FILES {
            body.push(row().child(Button::<App>::new("pane-changes-all", format!("Show all {}", c.files.len())).variant(ButtonVariant::Ghost).size(Size::Sm).on_press(|s: &mut App| s.open_changes_source(DiffSource::Thread)).build(t)));
        }
    }
    if let Some((_, offer)) = &offer
        && let Some(line) = &offer.line
    {
        body.push(text(line.clone()).key("pane-changes-note").role("note").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    let mut actions = row().key("pane-changes-actions").w_full().gap(t.space.at(2.0)).center_y().flex_wrap();
    actions = actions.child(Button::<App>::new("overview-review", "Review").variant(ButtonVariant::Secondary).size(Size::Sm).icon(&icons::FILE_CODE).on_press(|s: &mut App| s.open_changes(None)).build(t));
    if let Some((run, offer)) = offer
        && let Some((action, enabled)) = offer.action
    {
        let root = root.clone();
        let (words, icon, redo, key) = match action {
            Action::Undo => ("Undo last run", &icons::RESTORE, false, "overview-undo"),
            Action::Redo => ("Redo", &icons::RETRY, true, "overview-redo"),
        };
        actions = actions.child(Button::<App>::new(key, words).variant(ButtonVariant::Secondary).size(Size::Sm).icon(icon).disabled(!enabled).on_press(move |s: &mut App| s.undo_run(&root, &run, redo)).build(t));
    }
    body.push(actions);
    Some(col().key("pane-changes").role("group").label("Changes").w_full().gap(t.space.at(2.5)).child(heading(t, "pane-changes-heading", "Changes", None)).children(body))
}

/// A label and its value, on one line.
fn fact(t: &Theme, key: &str, label: &str, value: El, end: Vec<El>) -> El {
    let mut r = row().key(key.to_owned()).w_full().min_h(t.control.md).gap(t.space.at(2.0)).center_y().child(text(label.to_owned()).font(t.ty.sm).color(t.ink_faint()).w(52.0).a11y_hidden());
    r = r.child(value);
    r.child(el().grow(1.0)).children(end)
}

/// Where the thread works: the folder (`~/…`, cut in the middle), the branch, Reveal in Finder, Copy branch.
fn folder(s: &App, t: &Theme, o: Option<&Overview>) -> Option<El> {
    let folder = o.and_then(|o| o.folder.clone()).or_else(|| s.open_task().and_then(|x| x.folder.clone()))?;
    let branch = o.and_then(|o| o.branch.clone());
    let shown = crate::task_view::show_path(&folder, "", s.home.as_deref());
    let place = text(shown.clone()).key("pane-folder-path").role("note").label(format!("Folder {shown}")).help(folder.clone()).font(mono(t, t.ty.sm)).color(t.ink()).truncate_middle().min_w(0.0).shrink().grow(1.0);
    let f2 = folder.clone();
    let reveal: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| {
        let _ = tx.reveal_path(&f2);
    });
    let mut rows = vec![fact(t, "pane-folder", "Folder", place, vec![icon_button(t, "overview-reveal", "Reveal in Finder", &icons::FOLDER_OPEN, None, false, Some(reveal))])];
    if let Some(b) = branch {
        let b2 = b.clone();
        let copy: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(b2.clone()));
        let value = row().key("pane-branch").role("note").label(format!("Branch {b}")).min_w(0.0).shrink().gap(t.space.at(1.5)).center_y().child(glyph(&icons::COMMIT, t.control.icon_sm, t.ink_faint())).child(text(b).font(mono(t, t.ty.sm)).color(t.ink_muted()).truncate_middle().min_w(0.0).shrink().a11y_hidden());
        rows.push(fact(t, "pane-branch-row", "Branch", value, vec![icon_button(t, "overview-copy-branch", "Copy branch", &icons::COPY, None, false, Some(copy))]));
    }
    Some(col().key("pane-folder-section").role("group").label("Folder and branch").w_full().gap(0.0).children(rows))
}

/// What the thread rests on, each name a link to its tab.
fn waiting(s: &App, t: &Theme, root: &TaskId) -> Option<El> {
    let task = s.tasks.iter().find(|x| &x.id == root)?;
    if task.status != TaskStatus::Waiting {
        return None;
    }
    let live = s.live_subagents(root);
    let names: Vec<String> = live.iter().filter_map(|x| x.subagent_name()).collect();
    let said = if names.is_empty() { "Reading the reports".to_owned() } else { crate::subagents::names_sentence(&names) };
    let line = row().key("pane-waiting-line").role("status").label(said.clone()).w_full().gap(t.space.at(2.0)).center_y().child(still_ring(t, t.control.icon - 2.0).a11y_hidden()).child(text(said).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).shrink().a11y_hidden());
    Some(section(t, "pane-waiting", "Waiting on", None, vec![line]))
}

/// "61.4k", "820".
fn tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3).replace(".0k", "k"),
        _ => format!("{:.1}M", n as f64 / 1e6).replace(".0M", "M"),
    }
}

/// The context ring's numbers in one line (the cost stays in the ring's popover).
fn usage(s: &App, t: &Theme, root: &TaskId) -> Option<El> {
    let u = crate::task_view::task_usage(s.books.get(root)?)?;
    let pct = if u.window == 0 { 0 } else { ((u.context as f64 / u.window as f64) * 100.0).round() as u64 };
    let said = format!("{pct}% of context \u{b7} {} of {} tokens", tokens(u.context), tokens(u.window));
    // Only the context ring's popover says what a run cost (nothing else in the app shows money).
    let rows = vec![text(said.clone()).key("pane-usage-line").role("note").label(format!("Usage: {said}")).font(tabular(t.ty.sm)).color(t.ink_muted()).wrap().min_w(0.0)];
    Some(section(t, "pane-usage", "Usage", None, rows))
}

/// The Overview tab.
fn overview_body(s: &App, t: &Theme, root: &TaskId) -> El {
    let o = s.pane.overview.get(root);
    let mut page = col().key("pane-overview").w_full().gap(t.space.at(6.0));
    let parts = [needs(s, t, root), kittens(s, t, root), changes(s, t, root, o), folder(s, t, o), waiting(s, t, root), usage(s, t, root)];
    let mut any = false;
    for p in parts.into_iter().flatten() {
        any = true;
        page = page.child(p);
    }
    if !any {
        page = page.child(text("Nothing here yet. What needs you, the kittens, the changes and the folder show here as they come.").key("pane-overview-empty").font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
    }
    col().key("pane-overview-scroll").w_full().grow(1.0).min_h(0.0).px(t.space.at(4.0)).pt(t.space.at(3.0)).pb(t.space.at(8.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}

// ---- A kitten's tab -------------------------------------------------------------------------------------------------------------------

/// A kitten's own conversation, read here: the brief folded, Stop and Open in main view, the report first once it is done, then what it said and did.
fn kitten_body(s: &App, t: &Theme, kitten: &TaskId, layer: &mut Vec<El>) -> El {
    let Some(task) = s.tasks.iter().find(|x| &x.id == kitten) else { return el() };
    let who = task.subagent_name().unwrap_or_default();
    let live = task.is_live_subagent();
    let word = kitten_word(task);
    let mut top = col().key("pane-kitten-top").w_full().gap(t.space.at(2.0)).px(t.space.at(4.0)).pt(t.space.at(2.0)).pb(t.space.at(2.0));
    let head = row()
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .child(status_glyph(t, task.status, t.control.icon - 2.0))
        .child(text(task.title.clone()).key("pane-kitten-title").role("heading").level(3).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).grow(1.0))
        .child(text(word).font(weight(t.ty.sm, t.ty.medium)).color(t.ink_muted()).a11y_hidden());
    top = top.child(head);
    let id = task.id.clone();
    let mut buttons = row().w_full().gap(t.space.at(2.0)).center_y().flex_wrap();
    if live {
        let sid = id.clone();
        buttons = buttons.child(Button::<App>::new("pane-kitten-stop", "Stop").variant(ButtonVariant::Secondary).size(Size::Sm).icon(&icons::STOP).on_press(move |s: &mut App| s.stop_task(&sid)).build(t).label(format!("Stop {who}")));
    }
    let mid = id.clone();
    buttons = buttons.child(Button::<App>::new("pane-kitten-main", "Open in main view").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(move |s: &mut App| s.open_kitten_main(&mid)).build(t));
    top = top.child(buttons);
    // The brief the thread gave it, folded.
    let open = s.pane.brief_open;
    let fold = crate::ui::agent::FoldLine::<App>::new("pane-kitten-brief", "Brief", open).lead(crate::ui::agent::hollow_dot(t, t.control.icon)).on_toggle(|s: &mut App| s.pane.brief_open = !s.pane.brief_open);
    let brief = if open { vec![text(task.words.clone()).key("pane-kitten-brief-words").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).selectable(true).pl(t.space.at(1.5))] } else { Vec::new() };
    top = top.child(fold.build(t, brief));
    // A finished kitten's report comes first: its final words, in the second ink. They are said once: the conversation below leaves
    // them out (its last words are the report, never a second copy).
    let book = s.books.get(kitten);
    // One waiting on an answer has no report yet: its question ends its conversation instead.
    let asking = task.subagent.as_ref().is_some_and(|o| o.question.is_some());
    let report = if live || asking { None } else { book.and_then(|b| b.parts.iter().rev().find(|p| matches!(&p.body, neko_protocol::PartBody::Agent { text, .. } if !text.trim().is_empty()))) };
    if let Some(p) = report
        && let neko_protocol::PartBody::Agent { text: words, .. } = &p.body
    {
        top = top.child(
            col()
                .key("pane-kitten-report")
                .role("group")
                .label(format!("Report from {who}"))
                .w_full()
                .gap(t.space.at(1.0))
                .child(heading(t, "pane-kitten-report-heading", "Report", None))
                .child(crate::ui::response::Response::new("pane-kitten-report-words", words.clone()).selectable(true).muted(true).column(crate::view::COLUMN).build(t)),
        );
    }
    let without_report;
    let book = match (book, report) {
        (Some(b), Some(r)) => {
            let mut c = b.clone();
            c.parts.retain(|p| p.id != r.id);
            without_report = c;
            Some(&without_report)
        }
        (b, _) => b,
    };
    let items = match book {
        Some(b) if b.loaded || !b.parts.is_empty() => crate::task_view::kitten_items(s, t, task, b),
        _ => vec![text("Reading what it did…").key("pane-kitten-loading").role("status").font(t.ty.sm).color(t.ink_faint())],
    };
    let inset = t.space.at(4.0);
    let conversation = crate::ui::conversation::Conversation::<App>::new("pane-transcript", s.pane.at_bottom).column(crate::view::COLUMN + 2.0 * inset).children(items).on_at_bottom(|s: &mut App, at| s.pane.at_bottom = at).build(t);
    let _ = layer;
    col().key("pane-kitten").w_full().grow(1.0).min_h(0.0).child(top).child(el().w_full().h(1.0).fill(t.c.line).a11y_hidden()).child(col().w_full().grow(1.0).min_h(0.0).child(conversation))
}

// ---- The header and the whole -----------------------------------------------------------------------------------------------------------

/// The pane button: shows or hides the pane (⌥⌘B).
pub fn toggle_button(t: &Theme, shown: bool) -> El {
    let (key, words) = if shown { ("pane-hide", "Hide pane") } else { ("pane-show", "Show pane") };
    let b = Button::new(key, words).variant(ButtonVariant::Ghost).size(Size::Sm).icon_only(&icons::PANEL_RIGHT).on_press(|s: &mut App| s.toggle_pane()).build(t);
    crate::ui::tooltip::Tooltip::new(words).keys("\u{2325}\u{2318}B").wrap(b)
}

/// The tab strip: Overview, Changes, a tab for each kitten opened, folded into a More menu when they do not fit.
fn tab_strip(s: &App, t: &Theme, root: &TaskId, layer: &mut Vec<El>) -> El {
    let tabs = s.tabs_of(root);
    let selected = tabs.iter().position(|x| x == &s.tab_of(root)).unwrap_or(0);
    let mut strip = Tabs::<App>::new("pane-tabs", "Pane").variant(TabsVariant::Pane).selected(selected);
    for tab in &tabs {
        strip = match tab {
            Tab::Overview => strip.tab("Overview", None).permanent(),
            Tab::Changes => {
                let n = s.pane.overview.get(root).and_then(|o| o.changes.as_ref()).map(|c| c.files.len()).filter(|n| *n > 0);
                strip.tab("Changes", n).permanent()
            }
            Tab::Kitten(k) => strip.tab(s.tasks.iter().find(|x| &x.id == k).and_then(|x| x.subagent_name()).unwrap_or_else(|| "Kitten".to_owned()), None),
        };
    }
    strip.on_change(|s: &mut App, i| s.select_tab(i)).on_close(|s: &mut App, i| s.close_tab(i)).overflow(&s.pane.tabs, |s: &mut App| &mut s.pane.tabs).build_in(t, layer)
}

/// The pane's header, on the band's line: its tabs, and the pane button when the pane is docked (a sheet has its own close).
fn header(s: &App, t: &Theme, root: &TaskId, docked: bool, layer: &mut Vec<El>) -> El {
    let mut h = row().key("pane-band").w_full().h(BAND).center_y().px(t.space.at(3.0)).gap(t.space.at(2.0)).window_drag();
    h = h.child(row().grow(1.0).min_w(0.0).child(tab_strip(s, t, root, layer)));
    if docked {
        h = h.child(toggle_button(t, true));
    }
    h
}

/// The body of the tab shown.
fn body(s: &App, t: &Theme, root: &TaskId, layer: &mut Vec<El>) -> El {
    match s.tab_of(root) {
        Tab::Overview => overview_body(s, t, root),
        Tab::Changes => crate::review_view::changes_tab(s, t, root),
        Tab::Kitten(k) => kitten_body(s, t, &k, layer),
    }
}

/// The pane, docked: a region on the sidebar's material.
pub fn pane(s: &App, t: &Theme, root: &TaskId, layer: &mut Vec<El>) -> El {
    // The body that replaces another is gone in that frame and the new one fades in (never two drawn over each other).
    let page = crate::view::page_swap(body(s, t, root, layer));
    col().key("pane").role("complementary").label("Details").w_full().h_full().fill(crate::sidebar::panel_bg(t)).child(header(s, t, root, true, layer)).child(page)
}

/// The pane as a sheet over the conversation's right edge (a window under 1100 points): the same tabs and bodies under a close button.
pub fn sheet(s: &App, t: &Theme, root: &TaskId, layer: &mut Vec<El>) {
    let tabs = tab_strip(s, t, root, layer);
    let page = crate::view::page_swap(body(s, t, root, layer));
    crate::ui::sheet::Sheet::<App>::new("pane", "Details").side(crate::ui::sheet::Side::Right).size(crate::pane::DEFAULT_WIDTH).child(row().w_full().child(tabs)).child(page).open(true).return_focus("pane-show").on_close(|s: &mut App| s.close_sheet()).build(t, layer);
}
