//! Settings ▸ Notifications: the system's permission in words, three switches and a test (docs/knowledge/needs-you.md).

use rune::{El, col, row, text};
use rune_ui_style::{ButtonVariant, Size};

use super::{Found, note};
use crate::notify::Allowed;
use crate::state::App;
use crate::ui::base::{Theme, glyph};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::switch::Switch;

/// System Settings ▸ Notifications, at Neko's own entry.
pub const SYSTEM_NOTIFICATIONS: &str = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.neko.app";

/// What the page says about the system's permission, and the one step to fix it.
pub fn permission_words(a: Allowed) -> (&'static str, Option<&'static str>) {
    match a {
        Allowed::Granted => ("Notifications are on for Neko.", None),
        Allowed::NotAsked | Allowed::Unknown => ("Neko asks the Mac for permission the first time it has something to tell you.", None),
        Allowed::Denied => ("Notifications are off for Neko in System Settings.", Some("Open System Settings, choose Notifications, then Neko, and turn on Allow notifications.")),
        Allowed::Unavailable => ("This copy of Neko cannot send notifications: it is not running as an app.", Some("Open Neko from the Applications folder to get notifications.")),
    }
}

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let sw = s.notifier.switches;
    let mut part = col().key("settings-notifications").w_full().gap(t.space.at(3.0));
    let allowed = s.allowed();
    let (words, step) = permission_words(allowed);
    let (icon, ink) = match allowed {
        Allowed::Denied | Allowed::Unavailable => (&icons::WARNING, t.c.warning.fg),
        _ => (&icons::INFO, t.ink_faint()),
    };
    let mut said = col().gap(t.space.at(1.0)).min_w(0.0).shrink().child(text(words).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).a11y_hidden());
    if let Some(step) = step {
        said = said.child(text(step).key("settings-notify-step").font(t.ty.base).color(t.ink()).wrap().min_w(0.0).a11y_hidden());
    }
    let label = match step {
        Some(st) => format!("{words} {st}"),
        None => words.to_owned(),
    };
    let permission = row()
        .key("settings-notify-permission")
        .role("status")
        .label(label)
        .w_full()
        .gap(t.metrics.gap)
        .items_start()
        .child(row().size(t.control.icon, t.ty.base.line_height).center().child(glyph(icon, t.control.icon, ink)))
        .child(said);
    part = part.child(f.lit(t, "settings-notify-permission", permission));
    if allowed == Allowed::Denied {
        let open = Button::<App>::new("settings-notify-open", "Open Notification Settings").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(|_: &mut App, tx: &mut rune::Tx| {
            let _ = tx.open_url_then(SYSTEM_NOTIFICATIONS, |_: &mut App, _, _| {});
        });
        part = part.child(row().child(open));
    }
    let needs = Switch::<App>::new("notify-needs", "When a thread needs you", sw.needs).help("A request to answer, a question, or a failure you can fix.").on_toggle(|s: &mut App| {
        let mut w = s.notifier.switches;
        w.needs = !w.needs;
        s.set_switches(w);
    });
    let finished = Switch::<App>::new("notify-finished", "When a thread finishes", sw.finished).help("“Done”, with the first line of the answer.").on_toggle(|s: &mut App| {
        let mut w = s.notifier.switches;
        w.finished = !w.finished;
        s.set_switches(w);
    });
    let badge = Switch::<App>::new("notify-badge", "Show the count on the Dock icon", sw.badge).help("How many things need you.").on_toggle(|s: &mut App| {
        let mut w = s.notifier.switches;
        w.badge = !w.badge;
        s.set_switches(w);
    });
    let switches = col()
        .key("settings-notify-switches")
        .role("group")
        .label("Notify me")
        .w_full()
        .gap(t.space.at(1.0))
        .child(f.lit(t, "notify-needs", needs.build(t)))
        .child(f.lit(t, "notify-finished", finished.build(t)))
        .child(f.lit(t, "notify-badge", badge.build(t)));
    part = part.child(switches);
    part = part.child(note(
        t,
        "settings-notify-rules",
        "Neko tells you only when it is not in front, or you are in another thread. It never plays a sound of its own, and Focus decides what shows. The morning Briefing's notification is in Rounds & wakes.",
    ));
    let test = Button::new("settings-notify-test", "Send a test notification").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(allowed == Allowed::Unavailable).on_press(|s: &mut App| s.send_test()).build(t);
    part.child(row().child(f.lit_hug(t, "settings-notify-test", test)))
}
