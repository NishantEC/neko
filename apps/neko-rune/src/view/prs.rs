//! The Pull requests view, as the web's `pages/work/PullRequests.tsx` draws it (`pull_requests.rs` holds the rules): the page's
//! `WorkHeader` (title, "2 ready · 2 need you", "Merge N ready" and the three-dot menu), then one group per verdict 28 apart (Ready to
//! merge, Suggested to close, Needs your call, Merging when checks pass, Being updated, Not checked on GitHub yet, and merged and
//! closed on request), each heading a folding button with its count and each group a panel of rows 12 wide in the padding.
//!
//! A row is the web's `li`: the verdict glyph (a button that merges on a ready row), the title and `#num · repo` on one line, the
//! faint note line under them (what is wrong, "Handed back N of 4", the agent's reason), the age in a 72 point column, and on hover
//! the row's actions over the end of the row: Merge or Cancel merge, Close…, Open task and Open on GitHub. Every one of them is in
//! the right-click menu too.

use rune::{Color, El, TextStyle, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};

use super::{band, ui, webkit};
use crate::app::App;
use crate::icons_phosphor as ph;
use crate::link::{PrRow, ProblemKind, Verdict};
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

/// `MAX_FOLLOW_UPS`: after four hand-backs Neko waits for the person.
const MAX_FOLLOW_UPS: u32 = 4;

fn canmerge(v: Verdict) -> bool {
    matches!(v, Verdict::Ready | Verdict::Close)
}

/// `text-[12.5px]` with the web's 18.75 line.
fn small(t: &Neko) -> TextStyle {
    webkit::font(t, 12.5, 400.0, 18.75)
}

/// The second line of a row (`<p>` of spans, "·" between them): what needs a look, in the web's order.
fn notes_line(t: &Neko, r: &PrRow) -> Option<El> {
    let style = small(t);
    // Each piece is a list of text nodes (the web's JSX makes one per `{…}`).
    let mut pieces: Vec<(Vec<String>, Color, bool)> = Vec::new();
    for (kind, words) in r.problems() {
        pieces.push((vec![words], if kind == ProblemKind::Behind { t.text.muted } else { t.meaning.amber }, false));
    }
    if r.draft {
        pieces.push((vec!["Draft".to_owned()], t.text.muted, false));
    }
    if r.follow_ups > 0 {
        pieces.push((vec!["Handed back ".to_owned(), r.follow_ups.min(MAX_FOLLOW_UPS).to_string(), " of ".to_owned(), MAX_FOLLOW_UPS.to_string()], t.text.muted, false));
    }
    if let Some(reason) = r.reason.as_ref().filter(|x| !x.is_empty()) {
        // HTML folds the reason's line breaks into spaces; a line break in a Rune text would start a second line.
        pieces.push((vec![reason.split_whitespace().collect::<Vec<_>>().join(" ")], t.text.base.alpha(0.75), true));
    }
    if pieces.is_empty() {
        return None;
    }
    let last = pieces.len() - 1;
    let mut line = row().key(format!("pr-notes-{}", r.id)).w_full().min_w(0.0).center_y();
    for (i, (nodes, colour, cut)) in pieces.into_iter().enumerate() {
        if i > 0 {
            // `before:mx-1.5 before:content-['·']`, in the muted grey at 60%.
            line = line.child(row().mx(6.0).child(text("·").font(style).color(t.text.muted.alpha(0.6)).a11y_hidden()));
        }
        if nodes.len() == 1 {
            let node = text(nodes[0].clone()).font(style).color(colour).a11y_hidden();
            line = line.child(if i == last || cut { node.truncate().grow(1.0).shrink().min_w(0.0) } else { node });
        } else {
            line = line.child(row().children(nodes.into_iter().map(|n| text(n).font(style).color(colour).a11y_hidden())));
        }
    }
    Some(line)
}

/// A `RowAction`: the web's 28 point icon button, shown with the row's hover.
fn action_button(t: &Neko, key: String, label: &str, glyph: &'static rune::Icon, on_press: impl Fn(&mut App) + 'static) -> El {
    row().key(key).role("button").label(label.to_owned()).help(label.to_owned()).focusable(true).size(28.0, 28.0).center().radius(10.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(on_press).child(icon(glyph, 15.0).color(t.text.muted).a11y_hidden())
}

fn pr_row(s: &App, t: &Neko, r: &PrRow, layer: &mut Vec<El>) -> El {
    let place = [r.number.as_str(), r.repository.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · ");
    let problems = r.problems();
    let mut label = format!("{}, {}, {}", r.title, r.verdict.title(), place);
    if !problems.is_empty() {
        label.push_str(&format!(", {}", problems.iter().map(|(_, w)| w.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let tip = [problems.iter().map(|(_, w)| w.as_str()).collect::<Vec<_>>().join(" · "), r.reason.clone().unwrap_or_default()].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");

    let (id_merge, id_task, id_close, url) = (r.id.clone(), r.id.clone(), r.id.clone(), r.url.clone());
    let merging = r.verdict == Verdict::Merging;

    // The glyph's slot: 14 by 20; on a ready row the glyph is a 22 point button that merges (it bleeds 4 each side).
    let status = status_glyph(glyph(r.verdict), t, 14.0, Some(""));
    let slot = if r.verdict == Verdict::Ready {
        let id = r.id.clone();
        let merge = row().key(format!("pr-glyph-{}", r.id)).role("button").label(format!("Merge {} once its checks pass", if r.number.is_empty() { "this pull request" } else { r.number.as_str() })).focusable(true).size(22.0, 22.0).mx(-4.0).center().radius(11.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.set_merge_when_ready(&id, true)).child(status);
        row().w(14.0).h(20.0).center_y().child(merge)
    } else {
        row().w(14.0).h(20.0).center().child(status)
    };

    // The title and where it lives on one line, 12 apart; the note line under them.
    let title = text(r.title.clone()).key(format!("pr-title-{}", r.id)).font(webkit::font(t, 13.5, 400.0, 20.25)).color(t.text.base).truncate().shrink().min_w(0.0).a11y_hidden();
    let place_text = text(place).key(format!("pr-place-{}", r.id)).font(webkit::tnum(small(t))).color(t.text.muted).a11y_hidden();
    let mut body = col().grow(1.0).shrink().min_w(0.0).gap(2.0).child(row().w_full().min_w(0.0).gap(12.0).center_y().child(title).child(el().grow(1.0).min_w(0.0)).child(place_text));
    if let Some(notes) = notes_line(t, r) {
        body = body.child(notes);
    }

    // The age: a 72 point column, right-aligned, wrapping as the web's does.
    let age = row().w(72.0).h(20.0).center_y().justify_end().child(text(super::all::relative(s.now_ms, r.updated_ms)).key(format!("pr-age-{}", r.id)).font(webkit::tnum(small(t))).color(t.text.muted.alpha(0.7)).wrap().max_w(72.0).a11y_hidden());

    // The actions, over the end of the row while it is hovered or holds focus: nothing reserves room for them.
    let mut buttons: Vec<El> = Vec::new();
    if canmerge(r.verdict) {
        let id = id_merge.clone();
        buttons.push(action_button(t, format!("pr-merge-{}", r.id), &format!("Merge {}", r.number), &ph::GIT_MERGE, move |s: &mut App| s.set_merge_when_ready(&id, true)));
    }
    if merging {
        buttons.push(action_button(t, format!("pr-merge-{}", r.id), &format!("Cancel merging {}", r.number), &ph::PROHIBIT, move |s: &mut App| s.set_merge_when_ready(&id_merge, false)));
    }
    if canmerge(r.verdict) {
        buttons.push(action_button(t, format!("pr-close-{}", r.id), &format!("Close {}…", r.number), &ph::X_CIRCLE, move |s: &mut App| s.ask_close_pr(&id_close)));
    }
    if !canmerge(r.verdict) {
        buttons.push(action_button(t, format!("pr-task-{}", r.id), &format!("Open task for {}", r.number), &ph::LIST_CHECKS, move |s: &mut App| s.select(&id_task)));
    }
    let github = url.clone();
    buttons.push(action_button(t, format!("pr-github-{}", r.id), &format!("Open {} on GitHub", r.number), &ph::ARROW_SQUARE_OUT, move |s: &mut App| s.open_link(&github)));
    let wash = t.surface.card.mix(t.text.base, 0.03);
    let actions = row().key(format!("pr-actions-{}", r.id)).absolute(0.0, 0.0).w_full().h(36.0).justify_end().group_opacity(0.0, 1.0).child(row().h(36.0).pl(12.0).pr(8.0).gap(2.0).center_y().fill(wash).children(buttons));

    let mut line = row()
        .key(format!("pr-row-{}", r.id))
        .role("button")
        .label(label)
        .focusable(true)
        .w_full()
        .px(12.0)
        .py(8.0)
        .gap(12.0)
        .items_start()
        .radius(10.0)
        .hover_group()
        .hover(t.text.base.alpha(0.03))
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press({
            let id = r.id.clone();
            move |s: &mut App| s.select(&id)
        })
        .child(slot)
        .child(body)
        .child(age)
        .child(actions);
    if !tip.is_empty() {
        line = line.help(tip);
    }
    row_menu(s, t, layer, r, line)
}

/// The same actions on a right-click, in the web's order: Merge, Cancel merge, Open task, Open on GitHub, Copy link, Close….
fn row_menu(s: &App, t: &Neko, layer: &mut Vec<El>, r: &PrRow, line: El) -> El {
    let key = format!("pr-{}", r.id);
    let owner = key.clone();
    let line = line.on_secondary_press(move |s: &mut App, p, _| s.page_menu = Some((owner.clone(), p)));
    let Some((_, at)) = s.page_menu.as_ref().filter(|(open, _)| *open == key) else { return line };
    let mut menu = rune_kit::context_menu(format!("menu-{key}"), line, Some(*at)).on_close(|s: &mut App| s.page_menu = None);
    let plain = |label: &str, glyph: &'static rune::Icon, danger: bool, act: Box<dyn Fn(&mut App)>| {
        MenuItem::new(label.to_owned()).icon(glyph).danger(danger).on_select(move |s: &mut App| {
            s.page_menu = None;
            act(s);
        })
    };
    if canmerge(r.verdict) {
        let id = r.id.clone();
        menu = menu.item(plain("Merge", &ph::GIT_MERGE, false, Box::new(move |s| s.set_merge_when_ready(&id, true))));
    }
    if r.verdict == Verdict::Merging {
        let id = r.id.clone();
        menu = menu.item(plain("Cancel merge", &ph::PROHIBIT, false, Box::new(move |s| s.set_merge_when_ready(&id, false))));
    }
    let id = r.id.clone();
    menu = menu.item(plain("Open task", &ph::LIST_CHECKS, false, Box::new(move |s| s.select(&id))));
    let url = r.url.clone();
    menu = menu.item(plain("Open on GitHub", &ph::ARROW_SQUARE_OUT, false, Box::new(move |s| s.open_link(&url))));
    menu = menu.separator();
    let link = r.url.clone();
    menu = menu.item(MenuItem::new("Copy link").icon(&ph::COPY_REGULAR).on_select_tx(move |s: &mut App, tx: &mut rune::Tx| {
        s.page_menu = None;
        tx.copy(link.clone());
        s.show_toast("Copied the link");
    }));
    if canmerge(r.verdict) {
        let id = r.id.clone();
        menu = menu.separator().item(plain("Close…", &ph::X_CIRCLE, true, Box::new(move |s| s.ask_close_pr(&id))));
    }
    menu.build(&t.kit, layer)
}

/// A group: its heading is a button (caret 11, the title, a faint count; 4 points left of the column, 2 above and below) that
/// folds it, 4 above the panel of rows.
fn group(s: &App, t: &Neko, layer: &mut Vec<El>, verdict: Verdict, rows: &[&PrRow]) -> El {
    let folded = s.prs.folded.contains(&verdict);
    let title = verdict.title();
    let style = webkit::font(t, 12.5, 500.0, 18.75);
    let label = row()
        .gap(6.0)
        .center_y()
        .child(icon(if folded { &ph::CARET_RIGHT_BOLD } else { &ph::CARET_DOWN_BOLD }, 11.0).color(t.text.muted).a11y_hidden())
        .child(text(title).font(style).color(t.text.muted).a11y_hidden())
        .child(text(rows.len().to_string()).key(format!("pr-group-count-{title}")).font(webkit::tnum(style)).color(t.text.muted.alpha(0.7)).a11y_hidden());
    let head = row()
        .key(format!("pr-group-{title}"))
        .role("button")
        .label(format!("{title}, {}", rows.len()))
        .expanded(!folded)
        .focusable(true)
        .ml(-4.0)
        .px(4.0)
        .py(2.0)
        .radius(10.0)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.pr_toggle_group(verdict))
        .child(label);
    let mut c = col().key(format!("pr-section-{title}")).w_full().gap(4.0).child(row().w_full().child(head));
    if !folded {
        c = c.child(webkit::panel(t, rows.iter().map(|r| pr_row(s, t, r, layer)).collect()));
    }
    c
}

/// The header's three-dot menu: Re-check all, Check GitHub now, Show merged and closed, "Merge without an approving review" for each
/// repository, and the web's note.
fn overflow(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let trigger = row().key("pr-more").role("button").label("Pull request options").help("Pull request options").focusable(true).size(36.0, 36.0).center().radius(18.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| s.prs.menu_open = !s.prs.menu_open).child(icon(&ph::DOTS_THREE_BOLD, 16.0).color(t.text.base).a11y_hidden());
    let reviewable = s.pr_rows().iter().filter(|r| canmerge(r.verdict)).count();
    let mut menu = Menu::new("pr-menu").trigger(trigger).open(s.prs.menu_open).placement(Placement::BottomEnd).min_width(256.0).on_close(|s: &mut App| s.prs.menu_open = false);
    menu = menu.item(MenuItem::new("Re-check all").icon(&ph::ARROWS_CLOCKWISE).disabled(reviewable == 0).on_select(|s: &mut App| {
        s.prs.menu_open = false;
        s.ask_review_prs();
    }));
    menu = menu.item(MenuItem::new("Check GitHub now").on_select(|s: &mut App| {
        s.prs.menu_open = false;
        s.check_github();
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
    menu = menu.separator().item(MenuItem::new("Neko checks GitHub every 3 minutes and hands failing checks, conflicts and review comments back to the task’s agent. Merging and closing wait for you.").disabled(true));
    menu.build(&t.kit, layer)
}

/// `pullRequestSubtitle`: "2 ready · 1 to close · 1 need you", only the parts that are not zero.
pub fn subtitle(rows: &[&PrRow]) -> String {
    let parts: Vec<String> = [(Verdict::Ready, "ready"), (Verdict::Close, "to close"), (Verdict::NeedsYou, "need you")]
        .into_iter()
        .map(|(v, label)| (rows.iter().filter(|r| r.verdict == v).count(), label))
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect();
    if parts.is_empty() { "Nothing waiting on you".to_owned() } else { parts.join(" · ") }
}

/// The page's own header (`WorkHeader`): the title (20 semibold) and its subtitle 10 apart, the actions at the right 6 apart.
fn header(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let ready = s.ready_pull_requests().len();
    let mut title = row().gap(10.0).center_y().child(text("Pull requests").key("title").font(TextStyle { tracking: -0.41, ..webkit::font(t, 20.0, 600.0, 30.0) }).color(t.text.strong).role("heading"));
    if s.summary.is_some() {
        let rows = s.pr_rows();
        // The count is set on the title's baseline, a little lower than the middle of the line.
        title = title.child(text(subtitle(&rows)).key("page-count").font(webkit::tnum(webkit::font(t, 14.0, 400.0, 21.0))).color(t.text.muted).mt(3.0));
    }
    let mut actions = row().gap(6.0).center_y();
    if ready > 0 {
        actions = actions.child(merge_button(t, ready));
    }
    actions = actions.child(overflow(s, t, layer));
    row().w_full().min_h(36.0).gap(24.0).center_y().flex_wrap().child(title).child(el().grow(1.0).min_w(0.0)).child(actions)
}

/// "Merge N ready": the web's primary `Button size="sm"`, its words in three text nodes ("Merge ", the number, " ready").
fn merge_button(t: &Neko, ready: usize) -> El {
    let style = webkit::font(t, 14.0, 500.0, 20.0);
    row()
        .key("pr-merge-all")
        .role("button")
        .label(format!("Merge {ready} ready"))
        .help("Merge every pull request in Ready to merge once its checks pass")
        .focusable(true)
        .h(36.0)
        .px(15.0)
        .center()
        .radius(18.0)
        .fill(t.meaning.iris)
        .hover(Color::WHITE.alpha(0.14))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(|s: &mut App| s.ask_merge_all())
        .child(text("Merge ").font(style).color(Color::WHITE).a11y_hidden())
        .child(text(ready.to_string()).font(style).color(Color::WHITE).a11y_hidden())
        .child(text(" ready").font(style).color(Color::WHITE).a11y_hidden())
}

pub fn page(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let header = header(s, t, layer);
    let body = match s.summary.as_ref() {
        None => ui::quiet_line(t, "pr-none", "Neko isn't running yet."),
        Some(_) => {
            let groups = s.pr_groups();
            let all = s.pr_rows();
            if all.is_empty() {
                webkit::empty(t, "pr-empty", &ph::GIT_PULL_REQUEST, "No pull requests yet", "When a task’s work passes its independent review, Neko opens a pull request and it appears here.", None)
            } else {
                let hidden = s.pr_hidden_finished();
                let mut c = col().key("pr-list").w_full();
                if groups.is_empty() {
                    // `<p className="py-10 text-center text-[13px] text-muted-foreground">`
                    let style = webkit::font(t, 13.0, 400.0, 19.5);
                    c = c.child(row().key("pr-nothing-open").w_full().py(40.0).justify_center().child(text("Nothing is open. ").font(style).color(t.text.muted)).child(text(hidden.to_string()).font(style).color(t.text.muted)).child(text(" merged or closed.").font(style).color(t.text.muted)));
                }
                let mut list = col().key("pr-groups").role("list").label("Pull requests").focus_group().w_full().gap(28.0);
                for (verdict, rows) in &groups {
                    list = list.child(group(s, t, layer, *verdict, rows));
                }
                c = c.child(list);
                if !s.prs.show_finished && hidden > 0 {
                    let style = small(t);
                    c = c.child(row().key("pr-show-finished").role("button").label(format!("Show {hidden} merged and closed")).focusable(true).h(18.75).mt(24.0).radius(6.0).center_y().focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(|s: &mut App| s.prs.show_finished = true).child(text("Show ").font(style).color(t.text.muted).a11y_hidden()).child(text(hidden.to_string()).font(style).color(t.text.muted).a11y_hidden()).child(text(" merged and closed").font(style).color(t.text.muted).a11y_hidden()));
                }
                c
            }
        }
    };
    let wide = webkit::page_width() >= 768.0;
    let column = col().key("full-prs").w_full().max_w(880.0).px(if wide { 40.0 } else { 24.0 }).pt(if wide { 64.0 } else { 48.0 }).pb(96.0).gap(16.0).child(header).child(body);
    col().w_full().h_full().child(band(t)).child(ui::scrolled(column))
}
