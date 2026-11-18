//! Setup, Improvements: what Neko suggests after it notices the same trouble again, as the web's `pages/setup/Improvements.tsx` draws it
//! (and `improve/ImproveDetail.tsx` for the detail). The page is the title with its count, a line when suggestions are off, the daemon's
//! one-line note, then "Suggestions": one 44 point row per proposal, newest first (glyph, title, what kind, what became of it, how old),
//! with Apply and Reject while it is open, Undo once it can be undone, and "Open evidence" on hover. A pack suggestion never installs:
//! Apply opens that pack's preview on the Packs page. Apply, Reject and Undo are the daemon's `Improve` command; every answer is the
//! whole listing. Undo asks first only when the daemon says it is hard to take back.

use neko_protocol::improve::{ImproveKind, ImproveState, Improvement};
use rune::{El, col, el, row, text};

use super::Page;
use super::kit::glyph;
use super::words;
use crate::app::App;
use crate::icons_phosphor as ph;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::view::webkit;

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let reply = s.improvements();
    let items: &[Improvement] = reply.map_or(&[], |r| r.proposals.as_slice());
    let mut body = col().key("improve-body").w_full().gap(32.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    let small = webkit::font(t, 13.0, 400.0, 21.125);
    match reply {
        // `LoadingRows`: a panel of three rows, each a round and a bar.
        None => {
            let rows = (0..3).map(|i| row().key(format!("improve-loading-{i}")).w_full().px(12.0).py(10.0).gap(12.0).center_y().child(webkit::skeleton(t, &format!("improve-skeleton-dot-{i}"), 14.0, 14.0).radius(7.0)).child(webkit::skeleton(t, &format!("improve-skeleton-{i}"), 120.0 + (i * 17 % 36) as f32 * 2.0, 16.0))).collect();
            body = body.child(webkit::panel(rows));
        }
        Some(r) if !r.enabled => {
            // `improve-off`: the one line and its small outline button.
            let busy = s.improve_is_busy("enable");
            let turn_on = webkit::outline_xs_button(t, "improve-enable", "Turn on suggestions", "Turn on suggestions", webkit::gated(!busy, |s: &mut App| s.improve_enable()));
            body = body.child(row().key("improve-off").w_full().gaps(12.0, 12.0).flex_wrap().center_y().child(text("Suggestions are off. Neko is not looking for patterns.".to_owned()).font(small).color(t.text.muted)).child(turn_on));
        }
        Some(_) => {}
    }
    if let Some(words) = s.setup.improve_note.as_ref().filter(|w| !w.is_empty()) {
        body = body.child(text(words.clone()).key("improve-note").font(small).color(t.text.muted).wrap().min_w(0.0).live_region(rune::LiveRegion::Polite));
    }
    if let Some(r) = reply {
        if items.is_empty() && r.enabled {
            body = body.child(webkit::empty_titled(t, "improve-empty", &ph::LIGHTBULB, "Neko hasn't suggested anything yet."));
        }
        if !items.is_empty() {
            let rows: Vec<El> = items.iter().map(|p| improve_row(s, t, layer, p)).collect();
            body = body.child(webkit::section(t, "improve-suggestions", "Suggestions", Some(&items.len().to_string()), None, webkit::panel(rows)));
        }
    }
    Page { count: (!items.is_empty()).then_some(items.len()), trailing: Vec::new(), body, title: None, back: None }
}

fn improve_row(s: &App, t: &Neko, layer: &mut Vec<El>, p: &Improvement) -> El {
    let id = p.id.clone();
    let open = p.state == ImproveState::Open;
    let busy = s.improve_is_busy(&id);
    let small = webkit::font(t, 12.5, 400.0, 20.3125);
    let kind = words::improve_kind_label(p.kind);
    let state_words = words::improve_state_words(p.state);
    let detail = id.clone();
    // Left: the title (which gives way), what kind, what became of it; right: how old, with at least 12 between (`ms-auto`).
    let mut left = row().grow(1.0).shrink().min_w(0.0).gap(12.0).center_y().child(text(p.title.clone()).font(webkit::font(t, 13.5, 400.0, 21.9375)).color(if p.state == ImproveState::Rejected { t.text.muted } else { t.text.base }).truncate().min_w(0.0).shrink().a11y_hidden()).child(text(kind.to_owned()).key(format!("improve-kind-{id}")).font(small).color(t.text.muted).a11y_hidden());
    if let Some(word) = words::improve_state_word(p.state) {
        left = left.child(text(word.to_owned()).key(format!("improve-word-{id}")).font(small).color(t.text.muted).a11y_hidden());
    }
    let button = row()
        .key(format!("improve-{id}"))
        .role("button")
        .label(format!("{}, {kind}, {state_words}. Open details", p.title))
        .focusable(true)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.improve_open(&detail, false))
        .child(left)
        .child(text(words::relative_time(Some(p.created_ms), s.now_ms)).key(format!("improve-age-{id}")).font(webkit::tnum(small)).color(t.text.muted.alpha(0.6)).a11y_hidden());

    let mut line = vec![glyph(t, words::improve_glyph(p.state), state_words), button];
    if open {
        let (a, r) = (id.clone(), id.clone());
        line.push(webkit::outline_xs_button(t, &format!("improve-apply-{id}"), "Apply", &format!("Apply {}", p.title), webkit::gated(!busy, move |s: &mut App| s.improve_apply(&a))));
        line.push(webkit::ghost_xs_button(t, &format!("improve-reject-{id}"), "Reject", None, &format!("Reject {}", p.title), webkit::gated(!busy, move |s: &mut App| s.improve_reject(&r))));
    }
    if p.can_undo {
        let u = id.clone();
        line.push(webkit::outline_xs_button(t, &format!("improve-undo-{id}"), "Undo", &format!("Undo {}", p.title), webkit::gated(!busy, move |s: &mut App| s.improve_undo(&u))));
    }
    let ev = id.clone();
    line.push(webkit::row_actions(vec![webkit::row_action(t, &format!("improve-evidence-{id}"), &format!("Open evidence for {}", p.title), &ph::EYE_REGULAR, move |s: &mut App| s.improve_open(&ev, true))]));

    let mut below: Vec<El> = Vec::new();
    if let Some(n) = note(s, t, &format!("setup:improve:{id}")) {
        below.push(n);
    }
    // The right-click menu, as the web's: Apply and Reject while open, Undo when it can be, Open evidence.
    let mut entries: Vec<MenuEntry> = Vec::new();
    if open {
        let i = id.clone();
        entries.push(("Apply".to_owned(), std::rc::Rc::new(move |s: &mut App| s.improve_apply(&i))));
        let i = id.clone();
        entries.push(("Reject".to_owned(), std::rc::Rc::new(move |s: &mut App| s.improve_reject(&i))));
    }
    if p.can_undo {
        let i = id.clone();
        entries.push(("Undo".to_owned(), std::rc::Rc::new(move |s: &mut App| s.improve_undo(&i))));
    }
    let i = id.clone();
    entries.push(("Open evidence".to_owned(), std::rc::Rc::new(move |s: &mut App| s.improve_open(&i, true))));
    with_menu(s, t, layer, &format!("improve:{id}"), webkit::item_row(t, &format!("improve-block-{id}"), line, below), entries)
}

/// `Heading` (`packs/parts.tsx`): 12.5 medium muted.
fn heading(t: &Neko, key: &str, words: &str) -> El {
    text(words.to_owned()).key(key.to_owned()).font(webkit::font(t, 12.5, 500.0, 20.3125)).color(t.text.muted)
}

/// `Mono`: a command, path or evidence in the mono face, selectable, on a faint fill.
fn mono_block(t: &Neko, key: &str, words: &str) -> El {
    text(words.to_owned()).key(key.to_owned()).font(webkit::mono(webkit::font(t, 12.0, 400.0, 19.5))).color(t.text.base).wrap().min_w(0.0).selectable(true).px(12.0).py(8.0).radius(8.0).fill(t.text.base.alpha(0.05))
}

/// `Diff`: a unified diff one line each: additions mint, removals coral, the rest muted.
fn diff(t: &Neko, text_of: &str) -> El {
    let style = webkit::mono(webkit::font(t, 12.0, 400.0, 19.5));
    let mut lines = col().key("improve-diff").w_full().px(12.0).py(8.0).radius(8.0).fill(t.text.base.alpha(0.05));
    for (i, l) in text_of.split('\n').enumerate() {
        let ink = if l.starts_with('+') && !l.starts_with("+++") {
            t.meaning.mint
        } else if l.starts_with('-') && !l.starts_with("---") {
            t.meaning.coral
        } else {
            t.text.muted
        };
        lines = lines.child(text(if l.is_empty() { " ".to_owned() } else { l.to_owned() }).key(format!("improve-diff-{i}")).font(style).color(ink).selectable(true));
    }
    col().w_full().max_h(288.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(lines)
}

/// The suggestion's detail, as the web's dialog: why it was made, exactly what would change, the evidence, what happened to it, and
/// once applied whether the trouble went away.
pub fn detail(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, id: &str, evidence: bool) {
    let Some(p) = s.improvements().and_then(|r| r.proposals.iter().find(|p| p.id == id)) else { return };
    let busy = s.improve_is_busy(id);
    let body13 = webkit::font(t, 13.0, 400.0, 21.125);
    let mut body = col().key("improve-detail").w_full().gap(20.0);

    // The state, kind and age under the title (`DialogDescription`).
    let small = webkit::font(t, 12.5, 400.0, 20.3125);
    body = body.child(row().key("improve-state").gap(6.0).center_y().child(glyph(t, words::improve_glyph(p.state), "")).child(text(words::improve_state_words(p.state).to_owned()).font(small).color(t.text.muted)).child(text(format!(" · {} · {}", words::improve_kind_label(p.kind), words::relative_time(Some(p.created_ms), s.now_ms))).font(small).color(t.text.muted)));

    // The buttons, at the right: the default one (named by the daemon) and Reject while open, Undo once applied.
    let mut buttons = row().w_full().gap(8.0).flex_wrap().center_y().child(el().grow(1.0).min_w(0.0));
    if p.state == ImproveState::Open {
        let (a, r) = (p.id.clone(), p.id.clone());
        let label = if p.default_label.is_empty() { "Apply" } else { p.default_label.as_str() };
        buttons = buttons.child(webkit::button(t, "improve-d-apply", label, None, true, false, webkit::gated(!busy, move |s: &mut App| s.improve_apply(&a))));
        buttons = buttons.child(webkit::button(t, "improve-d-reject", "Reject", None, false, false, webkit::gated(!busy, move |s: &mut App| s.improve_reject(&r))));
    }
    if p.can_undo {
        let u = p.id.clone();
        buttons = buttons.child(webkit::button(t, "improve-d-undo", "Undo", Some(&ph::ARROW_COUNTER_CLOCKWISE_REGULAR), false, false, webkit::gated(!busy, move |s: &mut App| s.improve_undo(&u))));
    }
    body = body.child(buttons);
    if let Some(n) = note(s, t, &format!("setup:improve:{}", p.id)) {
        body = body.child(n);
    }
    if let Some(counts) = words::improve_counts(p.before, p.after, p.applied_ms) {
        body = body.child(text(counts).key("improve-counts").font(body13).color(t.text.base.alpha(0.85)).wrap().min_w(0.0));
    }
    if p.state == ImproveState::Sent {
        body = body.child(text("The change waits for your approval in Skills.".to_owned()).font(body13).color(t.text.muted).wrap().min_w(0.0));
    }
    body = body.child(col().key("improve-why").w_full().gap(8.0).child(heading(t, "improve-why-head", "Why")).child(text(p.why.clone()).font(body13).color(t.text.base.alpha(0.9)).wrap().min_w(0.0)));

    let mut change = col().key("improve-change").w_full().gap(8.0).child(heading(t, "improve-change-head", "The change"));
    if p.kind == ImproveKind::InstallPack {
        // "Opens the preview of the " {target in mono} " pack. Nothing installs by itself; …": one text node per part.
        let mut flow = row().flex_wrap().center_y().child(text("Opens the preview of the ".to_owned()).font(body13).color(t.text.base.alpha(0.9)));
        if !p.target.is_empty() {
            flow = flow.child(text(p.target.clone()).font(webkit::mono(webkit::font(t, 12.5, 400.0, 20.3125))).color(t.text.base.alpha(0.9)));
        }
        flow = flow.child(text(" pack. Nothing installs by itself; you read what it would do and install it there.".to_owned()).font(body13).color(t.text.base.alpha(0.9)));
        change = change.child(flow);
    } else if p.kind == ImproveKind::SkillPatch && !p.diff.is_empty() {
        change = change.child(diff(t, &p.diff));
    } else {
        change = change.child(text(p.change.clone()).font(body13).color(t.text.base.alpha(0.9)).wrap().min_w(0.0));
    }
    body = body.child(change);

    let mut ev = col().key("improve-evidence").w_full().gap(8.0).child(heading(t, "improve-evidence-head", "Evidence"));
    ev = ev.child(if p.evidence.is_empty() { text("No evidence was kept.".to_owned()).font(body13).color(t.text.muted) } else { mono_block(t, "improve-evidence-text", &p.evidence) });
    body = body.child(ev);

    if !p.history.is_empty() {
        let mut list = col().key("improve-history").w_full().gap(4.0).child(heading(t, "improve-history-head", "History"));
        for (i, h) in p.history.iter().enumerate() {
            list = list.child(row().key(format!("improve-history-{i}")).w_full().gap(12.0).child(text(words::relative_time(Some(h.at_ms), s.now_ms)).font(webkit::tnum(small)).color(t.text.muted.alpha(0.7)).w(96.0)).child(text(h.text.clone()).font(body13).color(t.text.base.alpha(0.9)).wrap().grow(1.0).min_w(0.0)));
        }
        body = body.child(list);
    }
    // The evidence heading is `improve-evidence`; opened by "Open evidence", the detail starts as it does for every open.
    let _ = evidence;
    super::kit::dialog(t, layer, "dlg-improve", &p.title, None, 672.0, win_h, body);
}
