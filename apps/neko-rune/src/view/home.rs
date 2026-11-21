//! Home, as the approved prototype draws it (`~/Documents/neko-design/prototype-2026-11-21`, `home-0-dark.png`): one column, 720 wide at
//! most, flush on the surface colour with no art and no glass.
//!
//! Top to bottom: the greeting (28 semibold), a status line ("1 working", "1 needs you", what today has cost), the Briefing (a short
//! paragraph with its time), the composer ("What should Neko work on?", the cat picker "Let Neko choose" and the model chip), a row of the
//! cats (face, name, area, one line of what each is doing), then Needs you (Use default, Answer…, Later, Dismiss), Ready to merge and Done.
//!
//! Everything here is read from data the client already has (`briefing.rs`); the buttons are the Inbox's own actions.

use neko_protocol::cats::Cat;
use rune::{Color, El, Key, KeyEvent, col, el, icon, row, text};
use rune_kit::{Menu, MenuItem, Placement};

use super::look;
use super::webkit::{font, tnum};
use super::{band_px, composer};
use crate::account;
use crate::app::App;
use crate::cat_face::{self, Marks};
use crate::clock;
use crate::icons;
use crate::link::{InboxItem, TaskRow};
use crate::new_session::{MAX_REQUEST, NewMenu};
use crate::theme::Neko;

/// The column's widest, in points.
pub const COLUMN_MAX: f32 = 720.0;

/// A model's words on the chip: "Claude · Sonnet" is "Claude Sonnet".
fn model_words(label: &str) -> String {
    label.replace(" · ", " ")
}

fn toggle_menu(menu: NewMenu) -> impl Fn(&mut App) + 'static {
    move |s: &mut App| {
        if let Some(f) = s.new_form.as_mut() {
            f.menu = if f.menu == Some(menu) { None } else { Some(menu) };
        }
    }
}

fn close_menu(s: &mut App) {
    if let Some(f) = s.new_form.as_mut() {
        f.menu = None;
    }
}

/// "1 working · 1 needs you · $6.20 today" (`.stat`): each part only when it has something to say.
fn status_line(s: &App, t: &Neko) -> El {
    let (working, needs) = (s.working_count(), s.needs_you_count());
    let style = font(t, 13.5, 400.0, 19.0);
    let mut line = row().key("home-status").mt(8.0).gap(14.0).center_y();
    if working > 0 {
        line = line.child(row().key("home-running").gap(7.0).center_y().child(look::ring(t, 12.0)).child(text(format!("{working} working")).font(style).color(t.text.muted)));
    }
    if needs > 0 {
        line = line.child(row().key("home-needs").gap(7.0).center_y().child(look::dot(t.meaning.amber)).child(text(format!("{needs} need{} you", if needs == 1 { "s" } else { "" })).font(style).color(t.text.muted)));
    }
    if working == 0 && needs == 0 {
        line = line.child(text("Nothing running").key("home-status-none").font(style).color(t.text.muted));
    }
    if let Some(v) = s.spend_today() {
        line = line.child(text(format!("${v:.2} today")).key("home-spend").font(tnum(style)).color(t.text.faint));
    }
    line
}

/// The Briefing (`.brief`): its label with the sparkle and the time, then the paragraph. Nothing until Neko has written one.
fn briefing(s: &App, t: &Neko) -> Option<El> {
    let d = s.neko.digest.as_ref()?;
    let words = d.headline.trim();
    if words.is_empty() {
        return None;
    }
    let at = clock::local(d.at_ms);
    let head = row().w_full().h(22.0).gap(8.0).center_y().child(icon(&icons::SPARKLES, 14.0).color(t.text.faint).a11y_hidden()).child(text("Briefing").font(font(t, 12.0, 500.0, 16.0)).color(t.text.faint).grow(1.0).a11y_hidden()).child(text(format!("{:02}:{:02}", at.hour, at.minute)).key("home-briefing-time").font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint).a11y_hidden());
    Some(col().key("home-briefing").w_full().mt(20.0).child(head).child(look::briefing_text(t, "home-briefing-text", words, font(t, 14.0, 400.0, 22.4)).mt(4.0)))
}

/// The composer (`.bigc .composer`): the request, then the bar: the attach clip, the cat picker, the model chip and the round send button.
fn composer_card(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(f) = s.new_form.as_ref() else { return el() };
    let k = &t.kit;
    let cats = s.cats_in_order();
    let chosen = f.cat.as_deref().and_then(|id| cats.iter().copied().find(|c| c.id == id));
    let workspaces = s.summary.as_ref().map(|m| m.workspaces.clone()).unwrap_or_default();
    // With no cats yet the picker names the workspace the request goes to; with cats it is "Who takes it".
    let cat_words = if cats.is_empty() { workspaces.get(f.workspace).map_or("No workspace".to_owned(), |w| w.name.clone()) } else { chosen.map_or("Let Neko choose".to_owned(), |c| if c.area.is_empty() { c.name.clone() } else { format!("{} · {}", c.name, c.area) }) };
    let mut cat_menu = Menu::new("new-cat-menu")
        .trigger(look::chip(t, "new-cat", Some(&icons::CAT), &cat_words, true).on_press(toggle_menu(NewMenu::Workspace)))
        .open(f.menu == Some(NewMenu::Workspace))
        .placement(Placement::TopStart)
        .min_width(260.0)
        .on_close(close_menu)
        .group_label(if cats.is_empty() { "Workspace" } else { "Who takes it" });
    if cats.is_empty() {
        for (i, w) in workspaces.iter().enumerate() {
            cat_menu = cat_menu.item(MenuItem::new(w.name.clone()).icon(&icons::FOLDER).checked(f.workspace == i).on_select(move |s: &mut App| {
                if let Some(f) = s.new_form.as_mut() {
                    f.workspace = i;
                    f.menu = None;
                }
            }));
        }
    } else {
        cat_menu = cat_menu.item(MenuItem::new("Let Neko choose").icon(&icons::SPARKLES).description("Neko picks the cat for the area").checked(f.cat.is_none()).on_select(|s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.cat = None;
                f.menu = None;
            }
        }));
    }
    for cat in &cats {
        let (id, workspace) = (cat.id.clone(), cat.workspace_id.clone());
        let words = if cat.area.is_empty() { cat.name.clone() } else { format!("{} · {}", cat.name, cat.area) };
        cat_menu = cat_menu.item(MenuItem::new(words).icon(cat_face::face_icon(&cat.face.0)).description(s.workspace_name_of(&cat.workspace_id)).checked(f.cat.as_deref() == Some(cat.id.as_str())).on_select(move |s: &mut App| {
            let at = s.summary.as_ref().and_then(|m| m.workspaces.iter().position(|w| w.id == workspace));
            if let Some(f) = s.new_form.as_mut() {
                f.cat = Some(id.clone());
                if let Some(at) = at {
                    f.workspace = at;
                }
                f.menu = None;
            }
        }));
    }

    // The model chip lists every model of every runtime the daemon offers, the one Neko picks first.
    let provider = f.runtime.clone().unwrap_or_else(|| s.default_provider());
    let model_label = f.model.as_ref().and_then(|m| f.choices.iter().find(|c| c.provider == provider && &c.model == m)).map_or("Auto".to_owned(), |c| model_words(&c.label));
    let mut model_menu = Menu::new("new-model-menu")
        .trigger(look::chip(t, "new-model", Some(&icons::SLIDERS), &model_label, true).on_press(toggle_menu(NewMenu::Model)))
        .open(f.menu == Some(NewMenu::Model))
        .placement(Placement::TopStart)
        .min_width(220.0)
        .on_close(close_menu)
        .item(MenuItem::new("Auto").description("The runtime picks").checked(f.model.is_none()).on_select(|s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.model = None;
                f.runtime = None;
                f.menu = None;
            }
        }));
    for c in &f.choices {
        let (p, m) = (c.provider.clone(), c.model.clone());
        model_menu = model_menu.item(MenuItem::new(model_words(&c.label)).checked(f.model.as_deref() == Some(c.model.as_str()) && provider == c.provider).on_select(move |s: &mut App| {
            if let Some(f) = s.new_form.as_mut() {
                f.runtime = Some(p.clone());
                f.model = Some(m.clone());
                f.menu = None;
            }
        }));
    }

    let request = composer::field(t, &s.new_request, "new-request-box", "new-request", "Request", "What should Neko work on?", 12, |s: &mut App| &mut s.new_request, |s: &mut App, ev: &KeyEvent| {
        if ev.key == Key::Enter && ev.mods.cmd {
            s.submit_new();
            return true;
        }
        false
    });
    // The prototype's text area is 56 high (`.bigc textarea`).
    let request = composer::with_chips(s, t, crate::compose::Dest::New, request).min_h(52.0);

    let blocker = s.new_blocker();
    let can_send = blocker.is_none() && !f.busy;
    let send = look::send(t, "new-start", "Send", can_send, |s: &mut App| s.submit_new());
    let attach = composer::attach(t, "new-attach", crate::compose::Dest::New);
    let mut card = composer::card(t, "home-composer", request, vec![attach, cat_menu.build(k, layer), model_menu.build(k, layer)], Vec::new(), send);
    let used = s.new_request.text.len();
    if let Some(e) = &f.error {
        card = card.child(text(e.clone()).key("new-error").font(t.ty.meta).color(t.meaning.amber).wrap().live_region(rune::LiveRegion::Polite));
    } else if let Some(b) = blocker.filter(|_| !s.new_request.text.is_empty() || s.summary.as_ref().is_none_or(|m| m.workspaces.is_empty())) {
        card = card.child(text(b).font(t.ty.meta).color(t.text.faint));
    }
    if used > MAX_REQUEST * 9 / 10 {
        card = card.child(text(format!("{used}/{MAX_REQUEST}")).font(t.ty.id).color(t.text.faint));
    }
    row().key("home-bigc").w_full().mt(20.0).child(card)
}

/// What a cat's card says it is doing (`cardLine`): what waits for you, else the task it is working on with its step, else a pull request
/// ready to merge, else "Quiet".
fn card_line(s: &App, t: &Neko, cat: &Cat) -> El {
    use neko_protocol::workbench::TaskStatus as St;
    let small = font(t, 12.0, 400.0, 16.0);
    let mut line = row().key(format!("home-line-{}", cat.id)).gap(6.0).center_y().mt(2.0).min_w(0.0);
    let waiting = s.cat_tasks(&cat.id, |st| st == St::AwaitingApproval).into_iter().next();
    let working = s.cat_tasks(&cat.id, |st| matches!(st, St::Planning | St::Building | St::Reviewing)).into_iter().next();
    let ready = s.cat_tasks(&cat.id, |st| st == St::ReadyForReview).into_iter().next();
    if let Some(task) = waiting {
        line = line.child(look::dot(t.meaning.amber)).child(text(task.title.clone()).font(small).color(t.meaning.amber_text).truncate().min_w(0.0));
    } else if let Some(task) = working {
        line = line.child(look::ring(t, 12.0)).child(text(task.title.clone()).font(small).color(t.text.faint).truncate().min_w(0.0).shrink());
        if let Some((done, of)) = task.progress {
            line = line.child(text(format!("· {done} of {of}")).font(tnum(small)).color(t.text.faint));
        }
    } else if let Some(task) = ready {
        let words = s.pr_of(task).map_or("Ready to merge".to_owned(), |p| format!("PR {p} ready to merge"));
        line = line.child(look::merge(t, 14.0)).child(text(words).font(small).color(t.text.faint).truncate().min_w(0.0));
    } else {
        line = line.child(text("Quiet").font(small).color(t.text.faint));
    }
    line
}

/// One cat (`.cc`): a 62 point card with its 36 point face, its name and area, and its line.
fn cat_card(s: &App, t: &Neko, cat: &Cat) -> El {
    let id = cat.id.clone();
    let marks: Marks = s.cat_marks(&cat.id);
    let mut name = row().gap(0.0).child(text(cat.name.clone()).font(font(t, 13.5, 500.0, 18.0)).color(t.text.base).a11y_hidden());
    if !cat.area.is_empty() {
        name = name.child(text(format!(" · {}", cat.area)).font(font(t, 13.5, 400.0, 18.0)).color(t.text.faint).truncate().min_w(0.0).a11y_hidden());
    }
    row()
        .key(format!("home-cat-{}", cat.id))
        .role("button")
        .label(format!("{}{}", cat.name, if cat.area.is_empty() { String::new() } else { format!(", {}", cat.area) }))
        .focusable(true)
        .grow(1.0)
        .w(0.0)
        .min_w(0.0)
        .h(62.0)
        .px(12.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.card)
        .border(1.0, t.surface.line_strong)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.open_cat(&id))
        .child(cat_face::face(&cat.face.0, 36.0, marks, t.surface.window, t))
        .child(col().grow(1.0).min_w(0.0).child(name).child(card_line(s, t, cat)))
}

/// The row of cats: three to a line, 12 apart.
fn cats_row(s: &App, t: &Neko) -> Option<El> {
    let cats = s.cats_in_order();
    if cats.is_empty() {
        return None;
    }
    let mut grid = col().key("home-cats").w_full().mt(14.0).gap(12.0);
    for chunk in cats.chunks(3) {
        let mut line = row().w_full().gap(12.0);
        for cat in chunk {
            line = line.child(cat_card(s, t, cat));
        }
        for i in chunk.len()..3 {
            line = line.child(el().key(format!("home-cat-gap-{i}")).grow(1.0).w(0.0));
        }
        grid = grid.child(line);
    }
    Some(grid)
}

/// A section heading (`.hs`): 28 high, 26 above and 4 below, 10 at the sides, the title in the third grey with its count and, at the right,
/// a quiet summary.
fn section(t: &Neko, key: &str, title: &str, count: usize, right: Option<String>) -> El {
    let mut r = row().key(key.to_owned()).w_full().h(28.0).mt(25.0).mb(4.0).px(10.0).gap(8.0).center_y().child(text(title.to_owned()).font(font(t, 12.0, 500.0, 16.0)).color(t.text.faint).role("heading").a11y_hidden()).child(text(count.to_string()).key(format!("{key}-n")).font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint).a11y_hidden());
    r = r.child(el().grow(1.0));
    if let Some(right) = right {
        r = r.child(text(right).key(format!("{key}-r")).font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint).a11y_hidden());
    }
    r
}

/// The sub line of a need (`.hn .sub`): the context in the second grey, then the rest in the third.
fn need_sub(t: &Neko, key: &str, context: &str, rest: &str) -> El {
    let style = font(t, 12.5, 400.0, 17.0);
    let mut plain = context.to_owned();
    let n = plain.len();
    if !rest.is_empty() {
        plain.push_str(" · ");
        plain.push_str(rest);
    }
    rune::rich(plain, vec![rune::Span::new(0, n).color(t.text.muted)]).key(key.to_owned()).font(style).color(t.text.faint).mt(2.0).min_w(0.0)
}

/// One Inbox item as a need (`.hn`): the amber dot, its question, where it came from and its default, and the four actions. A goal's item
/// has the goal's own buttons; "Later" dims it until you bring it back.
fn need_item(s: &App, t: &Neko, item: &InboxItem) -> El {
    let seq = item.seq;
    let later = s.pages.inbox_flags.contains(&format!("later:{seq}"));
    let detail = item.detail.lines().map(str::trim).find(|l| !l.is_empty() && l.chars().count() <= 24).unwrap_or("");
    let rest = match (&item.default, item.goal.is_some()) {
        (Some(d), false) => format!("{}{}default: {d}", detail, if detail.is_empty() { "" } else { " · " }),
        _ => detail.to_owned(),
    };
    let rest = if later { format!("{}later", if rest.is_empty() { String::new() } else { format!("{rest} · ") }) } else { rest };
    let from = if item.from.is_empty() { "Neko".to_owned() } else { item.from.clone() };
    let mut acts = row().mt(10.0).gap(4.0).flex_wrap();
    if later {
        acts = acts.child(look::act(t, &format!("home-again-{seq}"), "Show again", false, move |s: &mut App| s.toggle_inbox_flag(&format!("later:{seq}"))));
    } else if item.goal.is_some() {
        for (i, a) in s.item_actions(item).into_iter().enumerate() {
            let act = a.act.clone();
            acts = acts.child(look::act(t, &format!("home-goal-{seq}-{i}"), &a.label, i == 0, move |s: &mut App| s.run_item_action(act.clone())));
        }
    } else {
        if item.default.is_some() {
            acts = acts.child(look::act(t, &format!("home-use-{seq}"), "Use default", true, move |s: &mut App| s.answer_default(seq)));
        }
        if let Some(link) = item.link.clone() {
            acts = acts.child(look::act(t, &format!("home-answer-{seq}"), "Answer…", false, move |s: &mut App| s.run_item_action(crate::work::ItemAct::Link(link.clone()))));
        }
        acts = acts.child(look::act(t, &format!("home-later-{seq}"), "Later", false, move |s: &mut App| s.toggle_inbox_flag(&format!("later:{seq}"))));
        if !item.routine_id.is_empty() {
            let done = item.clone();
            acts = acts.child(look::act(t, &format!("home-dismiss-{seq}"), "Dismiss", false, move |s: &mut App| s.mark_item_done(&done)));
        }
    }
    let body = col().grow(1.0).min_w(0.0).child(text(item.title.clone()).key(format!("home-need-title-{seq}")).font(font(t, 14.5, 400.0, 19.5)).color(t.text.base).wrap().min_w(0.0)).child(need_sub(t, &format!("home-need-sub-{seq}"), &from, &rest)).child(acts);
    row()
        .key(format!("home-need-{seq}"))
        .w_full()
        .padding(10.0)
        .gap(12.0)
        .radius(10.0)
        .hover(t.surface.hover)
        .opacity(if later { 0.6 } else { 1.0 })
        .child(el().mt(3.0).w(16.0).h(16.0).center().child(look::dot(t.meaning.amber).opacity(if later { 0.5 } else { 1.0 })))
        .child(body)
        .child(text(crate::groups::age(s.now_ms, item.at_ms)).key(format!("home-need-age-{seq}")).font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint).a11y_hidden())
}

/// A task that waits on a decision, as a need: its title, the cat and the reason, and one button that opens it.
fn need_task(s: &App, t: &Neko, task: &TaskRow) -> El {
    let id = task.id.clone();
    let cat = s.cat_of(&task.id).map_or("Neko".to_owned(), |c| c.name.clone());
    let reason = if task.line.note.is_empty() { crate::status::status_label(task.status).to_owned() } else { task.line.note.clone() };
    let acts = row().mt(10.0).gap(4.0).child(look::act(t, &format!("home-open-{}", task.id), "Open", true, move |s: &mut App| s.select(&id)));
    let body = col().grow(1.0).min_w(0.0).child(text(task.title.clone()).font(font(t, 14.5, 400.0, 19.5)).color(t.text.base).wrap().min_w(0.0)).child(need_sub(t, &format!("home-task-sub-{}", task.id), &cat, &reason)).child(acts);
    row()
        .key(format!("home-need-task-{}", task.id))
        .w_full()
        .padding(10.0)
        .gap(12.0)
        .radius(10.0)
        .hover(t.surface.hover)
        .child(el().mt(3.0).w(16.0).h(16.0).center().child(look::dot(t.meaning.amber)))
        .child(body)
        .child(text(crate::groups::age(s.now_ms, task.updated_at_ms)).font(tnum(font(t, 12.0, 400.0, 16.0))).color(t.text.faint).a11y_hidden())
}

/// A row of Ready to merge or Done (`.hrow`): the glyph, the title, who, the pull request, and a button or what it cost.
fn task_row(s: &App, t: &Neko, task: &TaskRow, ready: bool) -> El {
    let id = task.id.clone();
    let who = match s.cat_of(&task.id) {
        Some(cat) => row().w(88.0).gap(8.0).center_y().child(cat_face::face(&cat.face.0, 16.0, Marks::default(), t.surface.window, t)).child(text(cat.name.clone()).font(font(t, 13.0, 400.0, 18.0)).color(t.text.muted).truncate().min_w(0.0).a11y_hidden()),
        None => row().w(88.0),
    };
    let glyph = row().w(16.0).h(16.0).center().child(if ready { look::merge(t, 14.0) } else { look::dot(t.meaning.mint) });
    let pr = text(s.pr_of(task).map_or(String::new(), |p| format!("PR {p}"))).font(tnum(font(t, 13.0, 400.0, 18.0))).color(t.text.faint).w(72.0).a11y_hidden();
    let last = if ready {
        let merge_id = task.id.clone();
        row().w(52.0).justify_end().child(look::act(t, &format!("home-merge-{}", task.id), "Merge", false, move |s: &mut App| s.set_merge_when_ready(&merge_id, true)))
    } else {
        row().w(52.0)
    };
    row()
        .key(format!("home-{}-{}", if ready { "ready" } else { "done" }, task.id))
        .role("button")
        .label(task.title.clone())
        .focusable(true)
        .w_full()
        .h(40.0)
        .px(10.0)
        .gap(12.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.select(&id))
        .child(glyph)
        .child(text(task.title.clone()).font(font(t, 14.0, 400.0, 19.0)).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(who)
        .child(pr)
        .child(last)
}

/// Home, filling the content region.
pub fn home(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let hour = clock::local(s.now_ms).hour;
    let name = s.account.first_name().to_owned();
    let words = if name.is_empty() { account::greeting(hour).to_owned() } else { format!("{}, {name}", account::greeting(hour)) };
    let title = text(words).key("home-title").font(t.ty.greeting).color(t.text.base).role("heading");
    let mut c = col().key("home-column").w_full().max_w(COLUMN_MAX).pt(44.0).pb(40.0).child(title).child(status_line(s, t));
    if let Some(b) = briefing(s, t) {
        c = c.child(b);
    }
    c = c.child(composer_card(s, t, layer));
    if let Some(cats) = cats_row(s, t) {
        c = c.child(cats);
    }
    // Needs you: the Inbox's items and the tasks that wait on a decision.
    let inbox = s.open_inbox();
    let tasks = s.needs_you_tasks();
    let n = inbox.len() + tasks.len();
    c = c.child(section(t, "home-sec-needs", "Needs you", n, None));
    if n == 0 {
        c = c.child(text("Nothing is waiting for you.").key("home-needs-none").font(font(t, 12.5, 400.0, 17.0)).color(t.text.faint).px(10.0).py(10.0));
    }
    for item in &inbox {
        c = c.child(need_item(s, t, item));
    }
    for task in tasks {
        c = c.child(need_task(s, t, task));
    }
    let ready = s.ready_to_merge();
    if !ready.is_empty() {
        c = c.child(section(t, "home-sec-ready", "Ready to merge", ready.len(), None));
        for task in ready {
            c = c.child(task_row(s, t, task, true));
        }
    }
    let done = s.done_today();
    if !done.is_empty() {
        let merged = done.iter().filter(|t| s.pr_of(t).is_some()).count();
        c = c.child(section(t, "home-sec-done", "Done", done.len(), (merged > 0).then(|| format!("{merged} merged"))));
        for task in done {
            c = c.child(task_row(s, t, task, false));
        }
    }
    let scroll = look::scroller("home-scroll", row().w_full().justify_center().child(c));
    // The top 48 points drag the window, as the title bar does; the folded sidebar's button sits at its left.
    let drag = band_px(t, 12.0).absolute(0.0, 0.0).key("home-band").fill(Color::TRANSPARENT);
    col().key("home").w_full().grow(1.0).min_h(0.0).fill(t.surface.window).child(scroll).child(drag)
}
