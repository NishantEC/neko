//! Settings ▸ Rounds & wakes: the brake on work that starts by itself (rounds, timers and watches), the limits on timers and
//! watches, and the morning Briefing's notification and time (docs/knowledge/rounds.md, docs/knowledge/waits.md,
//! docs/knowledge/briefing.md).

use rune::{El, col, row, text};
use rune_ui_style::{ButtonVariant, Size};

use super::{Found, h4, note, section};
use crate::state::App;
use crate::ui::base::{Theme, glyph};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::input::Input;
use crate::ui::switch::Switch;

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    col()
        .w_full()
        .gap(t.space.at(8.0))
        .child(f.lit(t, "settings-rounds-pause", automatic(s, t)))
        .child(f.lit(t, "settings-waits", waits(t)))
        .child(f.lit(t, "settings-briefing", briefing(s, t)))
}

/// "Timers and watches": what a thread's agent may set, said plainly (the limits are the daemon's, `neko_protocol::waits`).
fn waits(t: &Theme) -> El {
    use neko_protocol::waits::{DAY_MS, TIMERS_PER_THREAD, WATCH_MAX_MS, WATCHES_IN_ALL, WATCHES_PER_THREAD};
    let words = format!(
        "A thread's agent can ask to be woken later: up to {TIMERS_PER_THREAD} timers, from a minute to a week ahead, and {WATCHES_PER_THREAD} watches, for files changing in its folder or a check that succeeds, with at most {WATCHES_IN_ALL} watches across Neko. A watch lasts at most {} days. Each shows on its thread, with Stop watching.",
        WATCH_MAX_MS / DAY_MS
    );
    section(t, "settings-waits", "Timers and watches", None).child(note(t, "settings-waits-words", words))
}

/// "Automatic work": Pause automatic work, the same brake as the Rounds page's button: rounds, timers and watches.
fn automatic(s: &App, t: &Theme) -> El {
    let paused = s.rounds_paused;
    let sw = Switch::<App>::new("settings-rounds-pause", "Pause automatic work", paused)
        .help("While paused no round starts, no timer wakes a thread and no watch checks; what was due is not made up after. It lasts until you resume, across restarts.")
        .on_toggle(move |s: &mut App| s.set_rounds_paused(!paused))
        .build(t);
    let words = match s.rounds.len() {
        0 => "No rounds yet. Make one on the Rounds page (⌘3).".to_owned(),
        1 => "1 round, on the Rounds page (⌘3).".to_owned(),
        n => format!("{n} rounds, on the Rounds page (⌘3)."),
    };
    section(t, "settings-rounds", "Automatic work", Some("Rounds start work on a schedule; timers and watches wake a thread when it asked to be woken.")).child(sw).child(note(t, "settings-rounds-count", words))
}

/// The morning Briefing: what it shows, its switch and its time.
fn briefing(s: &App, t: &Theme) -> El {
    let mut part = section(
        t,
        "settings-briefing",
        "Morning Briefing",
        Some("What happened while you were away, made from what Neko already keeps. No model writes it, it asks for no key, and it costs nothing."),
    );
    let shows = ["What still waits for you, first", "What finished, with the first line of each answer", "What your rounds did", "What is still working", "The totals, with the cost as Claude Code reported it"];
    let mut list = col().key("settings-briefing-shows").role("list").label("What the briefing shows").w_full().gap(t.space.at(1.5)).pl(t.space.at(1.0));
    for line in shows {
        list = list.child(row().role("listitem").label(line).gap(t.space.at(2.5)).center_y().child(glyph(&icons::CHECK, t.control.icon_sm, t.ink_faint())).child(text(line).font(t.ty.base).color(t.ink()).wrap().min_w(0.0).shrink().a11y_hidden()));
    }
    part = part.child(h4(t, "What it shows").pt(t.space.at(2.0))).child(list);
    let Some(cfg) = &s.briefing_settings else {
        return part.child(text("Waiting for Neko…").key("settings-briefing-waiting").role("status").font(t.ty.base).color(t.ink_faint()));
    };
    let on = cfg.morning_on;
    part = part.child(h4(t, "Each morning").pt(t.space.at(3.0))).child(
        Switch::<App>::new("briefing-morning-on", "Tell me when my briefing is ready", on).help("One quiet notification, only when something happened. Focus decides what shows.").on_toggle(move |s: &mut App| s.save_briefing_settings(Some(!on))).build(t),
    );
    let typed = s.typed_morning();
    let changed = typed.as_deref().is_some_and(|a| a != cfg.morning_at);
    let invalid = typed.is_none();
    let input = Input::new("briefing-morning-time", &s.briefing_time, |s: &mut App| &mut s.briefing_time)
        .name("Morning time")
        .placeholder("08:00")
        .description("Hours and minutes, 24-hour, like 08:00")
        .width(120.0)
        .invalid(invalid && on)
        .on_submit(|s: &mut App| s.save_briefing_settings(None))
        .build(t);
    let save = Button::<App>::new("briefing-morning-save", "Save").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(!on || !changed).on_press(|s: &mut App| s.save_briefing_settings(None)).build(t);
    part = part.child(row().key("briefing-morning-row").gap(t.space.at(3.0)).center_y().child(text("At").font(t.ty.base).color(if on { t.ink() } else { t.ink_faint() })).child(input).child(save));
    if invalid && on {
        part = part.child(text("Write the time as hours and minutes, like 08:00.").key("briefing-morning-invalid").role("status").font(t.ty.sm).color(t.c.danger.fg).wrap().min_w(0.0));
    }
    part.child(note(
        t,
        "settings-briefing-rules",
        "Neko posts it when it is running and this Mac is awake. If the Mac slept through the time, it posts once when it wakes, if it is still that day. It posts nothing when nothing happened.",
    ))
}
