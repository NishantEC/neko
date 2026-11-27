//! The Needs you list (docs/knowledge/needs-you.md): one calm column of everything waiting for the person, oldest first, grouped by
//! workspace, each item answered where it is. ⌘2 or the sidebar's row opens it.
//!
//! Keys: Up and Down move between items, Return gives an item's default (Allow once for a request, the reply box for a question,
//! the task for a failure; a destructive request has none), Escape goes back. Tab reaches each item's own controls. Answering an
//! item hides it at once and the next one takes the keyboard. The answers are rune-ui's Buttons with the same words and looks as
//! the task's Confirmation; Confirmation itself is not used here because Escape inside it denies, and here Escape goes back.

use std::rc::Rc;

use neko_protocol::{Decision, FailureKind, Need, NeedsItem};
use rune::{El, Key, KeyEvent, Tx, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size, weight};

use crate::state::{App, Daemon, need_key};
use crate::ui::agent::{PressTx, icon_button, mono, return_presses};
use crate::ui::base::{Shape, States, Theme, paint, paint_state};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::input::Input;
use crate::view::COLUMN;

/// How long an item has waited, in a few words ("just now", "12 min", "3 h", "2 days").
pub fn waited(since_ms: i64, now_ms: i64) -> String {
    let mins = (now_ms - since_ms).max(0) / 60_000;
    match mins {
        0 => "just now".to_owned(),
        1..=59 => format!("{mins} min"),
        60..=1439 => format!("{} h", mins / 60),
        _ => match mins / 1440 {
            1 => "1 day".to_owned(),
            d => format!("{d} days"),
        },
    }
}

/// What an item asks, in a line or two.
pub fn what(item: &NeedsItem) -> String {
    match &item.need {
        Need::Permission { permission } => permission.title.clone(),
        Need::Question { question, .. } => question.clone(),
        Need::Failure { text, .. } => text.clone(),
        Need::Plugin { ask } => format!("Approve plugin: {}", ask.name),
    }
}

/// What Return does on an item's row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Default {
    Allow,
    Reply,
    Open,
    /// A destructive request: no default answer.
    Nothing,
}

pub fn default_of(item: &NeedsItem) -> Default {
    match &item.need {
        Need::Permission { permission } if permission.destructive => Default::Nothing,
        Need::Permission { .. } => Default::Allow,
        Need::Question { .. } => Default::Reply,
        Need::Failure { .. } => Default::Open,
        // Approving a plugin is a decision about what runs on this Mac: there is no default answer.
        Need::Plugin { .. } => Default::Nothing,
    }
}

/// A button that Return presses too while it has focus (Space always does), as the task's answers are.
fn answer(t: &Theme, key: String, words: &str, v: ButtonVariant, return_presses_it: bool, f: impl Fn(&mut App) + 'static) -> El {
    let f = Rc::new(f);
    let g = f.clone();
    let b = Button::<App>::new(key, words).variant(v).size(Size::Sm).on_press(move |s: &mut App| g(s)).build(t);
    let mut holder = row();
    if return_presses_it {
        holder = holder.on_key(move |s: &mut App, k: &KeyEvent, _: &mut Tx| return_presses(k, || f(s)));
    }
    holder.child(b)
}

/// A monospaced line with Copy (the sign-in or install line).
fn copy_line(t: &Theme, key: &str, words: &str) -> El {
    let w = words.to_owned();
    let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(w.clone()));
    let p = press.clone();
    let copy = row().on_key(move |s: &mut App, k: &KeyEvent, tx: &mut Tx| return_presses(k, || p(s, tx))).child(icon_button(t, &format!("{key}-copy"), "Copy", &icons::COPY, None, false, Some(press)));
    paint(row().key(key.to_owned()).w_full().h(t.control.md).pl(t.space.at(3.0)).pr(t.space.at(1.0)).gap(t.space.at(2.0)).center_y().radius(t.radius.md).round_as(t), &t.mat.slab.base)
        .child(text(words.to_owned()).font(mono(t, t.ty.sm)).color(t.ink()).selectable(true).truncate().min_w(0.0).grow(1.0))
        .child(copy)
}

/// The keys on an item's row: Up and Down to the item before and after, Return for its default, Escape back.
fn row_keys(s: &mut App, ev: &KeyEvent, tx: &mut Tx, item: &str, before: Option<&str>, after: Option<&str>, default: Default) -> bool {
    let plain = !(ev.mods.cmd || ev.mods.alt || ev.mods.ctrl || ev.mods.shift);
    if !plain {
        return false;
    }
    match ev.key {
        Key::Up | Key::Down => {
            if let Some(k) = if ev.key == Key::Up { before } else { after } {
                tx.focus(k.to_owned(), true);
            }
            true
        }
        Key::Enter => {
            match default {
                Default::Allow => s.answer_need(item, Decision::AllowOnce),
                Default::Reply => {
                    s.need_reply_mut(item).focused = true;
                }
                Default::Open => s.open_need(item),
                Default::Nothing => {}
            }
            true
        }
        Key::Escape => {
            s.leave_needs();
            true
        }
        _ => false,
    }
}

/// One item: its task's title, where and how long it has waited, what is asked, and its answers.
pub(crate) fn item_row(s: &App, t: &Theme, item: &NeedsItem, workspace: &str, pos: usize, n: usize, before: Option<String>, after: Option<String>) -> El {
    let key = need_key(&item.id);
    let since = waited(item.since_ms, s.now_ms);
    let asked = what(item);
    // Led by the cat: its portrait with its dot, and what it says ("miso asks", "miso could not go on"), and how long ago.
    let failed = matches!(item.need, Need::Failure { .. });
    let face = s.cat_face(&item.cat).to_owned();
    let says = match &item.need {
        Need::Permission { .. } => format!("{} asks", item.cat_name),
        Need::Question { .. } => format!("{} has a question", item.cat_name),
        Need::Failure { .. } => format!("{} could not go on", item.cat_name),
        Need::Plugin { .. } => "A plugin waits for you".to_owned(),
    };
    let lead = crate::faces::portrait_in(t, &format!("{key}-face"), &face, 44.0, Some(if failed { neko_protocol::TaskStatus::Failed } else { neko_protocol::TaskStatus::NeedsYou }), crate::view::raised_fill(t));
    let who = row()
        .key(format!("{key}-cat"))
        .w_full()
        .gap(t.space.at(1.5))
        .center_y()
        .child(text(says.clone()).font(weight(t.ty.sm, t.ty.semibold)).color(if failed { t.c.danger.fg } else { t.c.warning.fg }).truncate().min_w(0.0).shrink().a11y_hidden())
        .child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden())
        .child(text(since.clone()).key(format!("{key}-when")).font(rune_ui_style::tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden());
    let head = text(item.title.clone()).key(format!("{key}-title")).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0);
    let body = col().w_full().gap(t.space.at(2.5));
    let mut body = body.child(text(asked.clone()).key(format!("{key}-what")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).a11y_hidden());
    let id = item.id.clone();
    let open = {
        let id = id.clone();
        answer(t, format!("{key}-open"), "Open task", ButtonVariant::Ghost, true, move |s: &mut App| s.open_need(&id))
    };
    let mut controls = row().key(format!("{key}-answers")).w_full().gap(t.space.at(2.0)).center_y().flex_wrap();
    match &item.need {
        Need::Permission { permission } => {
            body = body.child(
                text(permission.detail.clone())
                    .key(format!("{key}-detail"))
                    .help(permission.detail.clone())
                    .font(mono(t, t.ty.sm))
                    .color(t.ink())
                    .truncate_middle()
                    .min_w(0.0)
                    .max_w(COLUMN - 44.0 - 3.0 * t.space.at(4.0)),
            );
            let destructive = permission.destructive;
            let (a, b, c) = (id.clone(), id.clone(), id.clone());
            controls = controls.child(answer(t, format!("{key}-allow"), "Allow once", if destructive { ButtonVariant::Destructive } else { ButtonVariant::Default }, !destructive, move |s: &mut App| {
                s.answer_need(&a, Decision::AllowOnce)
            }));
            if !destructive {
                controls = controls.child(answer(t, format!("{key}-always"), "Always allow", ButtonVariant::Secondary, true, move |s: &mut App| s.answer_need(&b, Decision::AlwaysAllow)));
            }
            controls = controls.child(answer(t, format!("{key}-deny"), "Deny", ButtonVariant::Ghost, !destructive, move |s: &mut App| s.answer_need(&c, Decision::Deny)));
        }
        Need::Question { .. } => {
            let reply = s.needs_replies.get(&item.id).cloned().unwrap_or_default();
            let (a, b, c) = (id.clone(), id.clone(), id.clone());
            let empty = reply.text.trim().is_empty();
            let field = Input::new(format!("{key}-reply"), &reply, move |s: &mut App| s.need_reply_mut(&a)).name(format!("Answer to {}", item.title)).placeholder(format!("Answer {}", item.cat_name)).on_submit(move |s: &mut App| s.reply_need(&b)).build(t);
            controls = controls.child(row().grow(1.0).min_w(200.0).child(field)).child(
                Button::<App>::new(format!("{key}-send"), "Send").size(Size::Sm).disabled(empty).on_press(move |s: &mut App| s.reply_need(&c)).build(t),
            );
        }
        Need::Failure { failure, next, command, .. } => {
            if let Some(n) = next {
                body = body.child(text(n.replace('`', "")).key(format!("{key}-next")).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0).a11y_hidden());
            }
            if let Some(c) = command.as_ref().filter(|_| matches!(failure, FailureKind::NotSignedIn | FailureKind::NotInstalled)) {
                body = body.child(copy_line(t, &format!("{key}-command"), c));
            }
        }
        // The plugin's own card comes with the plugins' screens.
        Need::Plugin { .. } => {}
    }
    // The reply box takes the room a question's row has; elsewhere Open task sits at the row's end.
    if !matches!(item.need, Need::Question { .. }) {
        controls = controls.child(el().grow(1.0));
    }
    controls = controls.child(open);
    body = body.child(controls);
    let label = format!("{says}: {}: {asked}", item.title);
    let description = format!("{workspace}, waiting {since}");
    let default = default_of(item);
    let mine = item.id.clone();
    let r = col()
        .key(key)
        .role("listitem")
        .label(label)
        .description(description)
        .position_in_set(pos, n)
        .focusable(true)
        .w_full()
        .padding(t.space.at(4.0))
        .radius(t.radius.xl + 4.0)
        .round_as(t);
    let main = col().grow(1.0).min_w(0.0).gap(t.space.at(1.5)).child(who).child(head).child(body);
    paint_state(crate::turn::settling(crate::view::raised(t, r)), &t.mat.row, States::default())
        .child(row().w_full().gap(t.space.at(4.0)).items_start().child(lead).child(main))
        .on_key(move |s: &mut App, ev: &KeyEvent, tx: &mut Tx| row_keys(s, ev, tx, &mine, before.as_deref(), after.as_deref(), default))
}

/// How long the empty line waits before it fades in: an answered item's exit (seconds).
pub const EMPTY_AFTER: f32 = 0.25;

pub fn needs_view(s: &App, t: &Theme) -> El {
    let items = s.visible_needs();
    let n = items.len();
    let mut page = col().key("needs").w_full().max_w(COLUMN).gap(t.space.at(6.0));
    let mut title = row().w_full().gap(t.space.at(3.0)).center_y().child(text("Needs you").key("needs-title").role("heading").level(1).font(weight(t.ty.xl, t.ty.medium)).color(t.ink()));
    if n > 0 {
        title = title.child(text(if n == 1 { "1 waiting".to_owned() } else { format!("{n} waiting") }).key("needs-count").font(rune_ui_style::tabular(t.ty.base)).color(t.ink_faint()));
    }
    page = page.child(title);
    if let Some(words) = &s.needs_notice {
        page = page.child(text(words.clone()).key("needs-notice").role("status").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    if items.is_empty() {
        let waiting = !s.needs_loaded && s.daemon != Daemon::Absent;
        let words = if waiting { "Loading…" } else { "Nothing needs you right now." };
        // It fades in once the last item has faded out, never over it (and over its "1 waiting").
        page = page.child(text(words).key("needs-empty").role("status").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).transition(rune::Transition::opacity()).enter_delay(EMPTY_AFTER));
        return wrap(t, page);
    }
    // The daemon's order: groups by workspace, in the order of their oldest item.
    let keys: Vec<String> = items.iter().map(|i| need_key(&i.id)).collect();
    let mut groups: Vec<(String, Vec<(usize, &NeedsItem)>)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match groups.last_mut() {
            Some((w, list)) if *w == item.workspace => list.push((i, item)),
            _ => groups.push((item.workspace.clone(), vec![(i, item)])),
        }
    }
    for (ws, list) in groups {
        let name = s.workspace_of(&ws).map_or_else(|| if ws == neko_protocol::GENERAL { "General".to_owned() } else { "Workspace".to_owned() }, |w| w.name.clone());
        let mut rows = col().key(format!("needs-list-{ws}")).role("list").label(format!("Waiting in {name}")).w_full().gap(t.space.at(3.0));
        let m = list.len();
        for (j, (i, item)) in list.into_iter().enumerate() {
            let before = i.checked_sub(1).map(|b| keys[b].clone());
            let after = keys.get(i + 1).cloned();
            rows = rows.child(item_row(s, t, item, &name, j + 1, m, before, after));
        }
        let head = text(name.clone()).key(format!("needs-group-{ws}")).role("heading").level(2).font(weight(t.ty.sm, t.ty.medium)).color(t.ink_muted()).truncate().min_w(0.0).px(t.metrics.inset);
        page = page.child(col().key(format!("needs-section-{ws}")).role("group").label(name).w_full().gap(t.space.at(2.0)).child(head).child(rows));
    }
    wrap(t, page)
}

fn wrap(t: &Theme, page: El) -> El {
    col().key("needs-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.space.at(8.0)).pt(t.space.at(6.0)).pb(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_an_item_has_waited_reads_short() {
        let now = 10 * 86_400_000;
        assert_eq!(waited(now - 20_000, now), "just now");
        assert_eq!(waited(now - 12 * 60_000, now), "12 min");
        assert_eq!(waited(now - 3 * 3_600_000, now), "3 h");
        assert_eq!(waited(now - 86_400_000, now), "1 day");
        assert_eq!(waited(now - 3 * 86_400_000, now), "3 days");
        assert_eq!(waited(now + 5_000, now), "just now", "a clock a little ahead is not negative");
    }
}
