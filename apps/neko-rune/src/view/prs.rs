//! The Pull requests view (`pull_requests.rs` holds the rules; the web file is `pages/work/PullRequests.tsx`): the pull requests
//! Neko opened, in the web's groups (Ready to merge, Suggested to close, Needs your call, Merging when checks pass, Being updated,
//! Not checked on GitHub yet, and merged and closed on request), each folding and counted. A row is 32 high: the verdict glyph, the
//! repository and number (faint), the title, who wrote it (muted), the checks (coral while failing) and the age. On hover or focus
//! the row's actions replace the checks and age, and every one is in its right-click menu: the "Merge when ready" switch, Close…
//! (a dialog with the reason and a coral button), Open on GitHub, Open its task.

use rune::{Color, El, TextStyle, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};

use super::{icon_button, primary_button, quiet_button, ui};
use crate::app::App;
use crate::groups::age;
use crate::icons;
use crate::link::{Checks, PrRow, Verdict};
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;

/// The status language's glyph for a verdict: sky for ready, amber for what needs you or is in progress, coral for close.
fn glyph(verdict: Verdict) -> StatusKind {
    match verdict {
        Verdict::Ready => StatusKind::Review,
        Verdict::Merged => StatusKind::Done,
        Verdict::Close => StatusKind::Failed,
        Verdict::NeedsYou | Verdict::Merging | Verdict::Working => StatusKind::InProgress,
        Verdict::Closed => StatusKind::Cancelled,
        Verdict::Unchecked => StatusKind::Todo,
    }
}

/// The checks as a few words: passing, how many failed, running, none.
pub fn checks_words(checks: &Checks) -> String {
    match checks {
        Checks::Passing => "passing".to_owned(),
        Checks::Failing(names) => format!("{} failing", names.len().max(1)),
        Checks::Pending => "running".to_owned(),
        Checks::None => "no checks".to_owned(),
        Checks::Unknown => "—".to_owned(),
    }
}

fn canmerge(v: Verdict) -> bool {
    matches!(v, Verdict::Ready | Verdict::Close)
}

/// The "Merge when ready" switch of a row.
fn merge_switch(t: &Neko, r: &PrRow) -> El {
    let id = r.id.clone();
    let on = r.merging;
    ui::switch(t, &format!("pr-merge-{}", r.id), &format!("Merge {} when ready", r.number), on, move |s: &mut App| s.set_merge_when_ready(&id, !on))
}

fn action_button(t: &Neko, key: String, label: &str, glyph: &'static rune::Icon, on_press: impl Fn(&mut App) + 'static) -> El {
    row().key(key).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(28.0, 28.0).center().radius(t.radius.control).hover(t.surface.active).focus_ring(2.0, 2.0, t.surface.ring).on_press(on_press).child(icon(glyph, 16.0).color(t.text.muted).a11y_hidden())
}

fn pr_row(s: &App, t: &Neko, r: &PrRow, layer: &mut Vec<El>) -> El {
    let failing = matches!(r.checks, Checks::Failing(_));
    let place = [r.number.as_str(), r.repository.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · ");
    let problems = r.problems();
    let mut label = format!("{}, {}, {}", r.title, r.verdict.title(), place);
    if !problems.is_empty() {
        label.push_str(&format!(", {}", problems.join(", ")));
    }
    let tip = [problems.join(" · "), r.reason.clone().unwrap_or_default()].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");

    // What shows at rest at the right: the checks and the age. On hover the actions take its place.
    let mut meta = row().key(format!("pr-meta-{}", r.id)).gap(10.0).center_y().justify_end().absolute(0.0, 0.0).w_full().h_full();
    meta = meta.child(
        row()
            .key(format!("pr-checks-{}", r.id))
            .gap(4.0)
            .center_y()
            .child(if matches!(r.checks, Checks::Passing) { icon(&icons::CHECK_LINE, 14.0).color(t.text.muted).a11y_hidden() } else { el().size(0.0, 0.0) })
            .child(text(checks_words(&r.checks)).font(t.ty.meta).color(if failing { t.meaning.coral } else { t.text.muted }).a11y_hidden()),
    );
    meta = meta.child(text(age(s.now_ms, r.updated_ms)).key(format!("pr-age-{}", r.id)).font(t.ty.id).color(t.text.faint).w(32.0).a11y_hidden());
    meta = meta.group_opacity(1.0, 0.0);

    let (id_close, id_task, url) = (r.id.clone(), r.id.clone(), r.url.clone());
    let mut actions = row().key(format!("pr-actions-{}", r.id)).gap(2.0).center_y().justify_end().absolute(0.0, 0.0).w_full().h_full().group_opacity(0.0, 1.0);
    if canmerge(r.verdict) || r.verdict == Verdict::Merging {
        actions = actions.child(row().key(format!("pr-switch-{}", r.id)).px(6.0).center_y().gap(6.0).child(text("Merge when ready").font(t.ty.meta).color(t.text.muted).a11y_hidden()).child(merge_switch(t, r)));
    }
    if canmerge(r.verdict) {
        actions = actions.child(action_button(t, format!("pr-close-{}", r.id), &format!("Close {}…", r.number), &icons::CLOSE_SMALL, move |s: &mut App| s.ask_close_pr(&id_close)));
    }
    actions = actions.child(action_button(t, format!("pr-task-{}", r.id), &format!("Open the task for {}", r.number), &icons::REASON_LIST, move |s: &mut App| s.select(&id_task)));
    actions = actions.child(action_button(t, format!("pr-github-{}", r.id), &format!("Open {} on GitHub", r.number), &icons::EXTERNAL, move |s: &mut App| s.open_link(&url)));

    let right = el().key(format!("pr-right-{}", r.id)).w(260.0).h(32.0).child(meta).child(actions);
    let author_cell = text(r.author.clone()).font(t.ty.meta).color(t.text.muted).truncate().w(64.0).a11y_hidden();
    let mut line = row()
        .key(format!("pr-row-{}", r.id))
        .role("button")
        .label(label)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(8.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .hover_group()
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press({
            let id = r.id.clone();
            move |s: &mut App| s.select(&id)
        })
        .child(status_glyph(glyph(r.verdict), t, 14.0, Some("")))
        .child(text(place).key(format!("pr-place-{}", r.id)).font(t.ty.id).color(t.text.faint).truncate().max_w(150.0).a11y_hidden())
        .child(text(r.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(author_cell)
        .child(right);
    if !tip.is_empty() {
        line = line.help(tip);
    }

    // The same actions, on a right-click.
    let mut entries: Vec<ui::MenuEntry> = Vec::new();
    if canmerge(r.verdict) {
        let id = r.id.clone();
        entries.push(("Merge when ready".to_owned(), std::rc::Rc::new(move |s: &mut App| s.set_merge_when_ready(&id, true))));
    }
    if r.verdict == Verdict::Merging {
        let id = r.id.clone();
        entries.push(("Stop merging when ready".to_owned(), std::rc::Rc::new(move |s: &mut App| s.set_merge_when_ready(&id, false))));
    }
    let id = r.id.clone();
    entries.push(("Open task".to_owned(), std::rc::Rc::new(move |s: &mut App| s.select(&id))));
    let url = r.url.clone();
    entries.push(("Open on GitHub".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_link(&url))));
    if canmerge(r.verdict) {
        let id = r.id.clone();
        entries.push(("Close…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.ask_close_pr(&id))));
    }
    ui::with_menu(s, t, layer, &format!("pr-{}", r.id), line, entries)
}

/// A group: a quiet heading with its count that folds, then its rows.
fn group(s: &App, t: &Neko, layer: &mut Vec<El>, verdict: Verdict, rows: &[&PrRow], first: bool) -> El {
    let folded = s.prs.folded.contains(&verdict);
    let title = verdict.title();
    let head = row()
        .key(format!("pr-group-{title}"))
        .role("button")
        .label(format!("{title}, {}", rows.len()))
        .expanded(!folded)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(t.radius.row)
        .mt(if first { 0.0 } else { 12.0 })
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.pr_toggle_group(verdict))
        .child(icon(if folded { &icons::CHEVRON_RIGHT } else { &icons::CHEVRON_DOWN }, 16.0).color(t.text.muted).a11y_hidden())
        .child(text(title).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).truncate().min_w(0.0).a11y_hidden())
        .child(text(rows.len().to_string()).key(format!("pr-group-count-{title}")).font(t.ty.meta).color(t.text.faint).a11y_hidden());
    let mut c = col().key(format!("pr-section-{title}")).w_full().gap(0.0).child(head);
    if !folded {
        for r in rows {
            c = c.child(pr_row(s, t, r, layer));
        }
    }
    c
}

/// The header's overflow: Review pull requests, merged and closed, and "Merge without review" for each repository.
fn overflow(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let trigger = icon_button(&icons::MORE, "Pull request options", "pr-more", t, |s: &mut App| s.prs.menu_open = !s.prs.menu_open);
    let mut menu = Menu::new("pr-menu").trigger(trigger).open(s.prs.menu_open).placement(Placement::BottomEnd).min_width(280.0).on_close(|s: &mut App| s.prs.menu_open = false);
    menu = menu.item(MenuItem::new("Review pull requests").on_select(|s: &mut App| {
        s.prs.menu_open = false;
        s.ask_review_prs();
    }));
    menu = menu.item(MenuItem::new("Show merged and closed").checked(s.prs.show_finished).on_select(|s: &mut App| {
        s.prs.menu_open = false;
        s.prs.show_finished = !s.prs.show_finished;
    }));
    let repos = s.pr_repositories();
    if !repos.is_empty() {
        menu = menu.separator().group_label("Merge without an approving review");
        for (path, name, on) in repos {
            menu = menu.item(MenuItem::new(name).checked(on).on_select(move |s: &mut App| {
                s.prs.menu_open = false;
                s.toggle_merge_without_review(&path);
            }));
        }
    }
    menu.build(&t.kit, layer)
}

pub fn page(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let ready = s.ready_pull_requests().len();
    let mut trailing: Vec<El> = Vec::new();
    if ready > 0 {
        trailing.push(primary_button("Merge all ready", "pr-merge-all", t, |s: &mut App| s.ask_merge_all()));
    }
    trailing.push(quiet_button("Check GitHub", "pr-check", t, |s: &mut App| s.check_github()));
    trailing.push(overflow(s, t, layer));

    let body = match s.summary.as_ref() {
        None => ui::quiet_line(t, "pr-none", "Neko isn't running yet."),
        Some(_) => {
            let groups = s.pr_groups();
            let all = s.pr_rows();
            if all.is_empty() {
                ui::quiet_line(t, "pr-empty", "No pull requests yet. When a task’s work passes its independent review, Neko opens one and it appears here.")
            } else if groups.is_empty() {
                ui::quiet_line(t, "pr-nothing-open", &format!("Nothing is open. {} merged or closed.", s.pr_hidden_finished()))
            } else {
                let mut c = col().key("pr-list").role("list").label("Pull requests").focus_group().w_full().gap(0.0);
                for (i, (verdict, rows)) in groups.iter().enumerate() {
                    c = c.child(group(s, t, layer, *verdict, rows, i == 0));
                }
                let hidden = s.pr_hidden_finished();
                if hidden > 0 {
                    c = c.child(row().pt(12.0).px(8.0).child(quiet_button(&format!("Show {hidden} merged and closed"), "pr-show-finished", t, |s: &mut App| s.prs.show_finished = true)));
                }
                c
            }
        }
    };
    let open = s.pr_rows().iter().filter(|r| !r.verdict.finished()).count();
    let _ = Color::TRANSPARENT;
    ui::page(t, "prs", "Pull requests", Some(open), trailing, body)
}
