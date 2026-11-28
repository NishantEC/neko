//! Settings ▸ Permissions: how much a thread's agent may do without asking, what the person's own Claude Code rules allow, and
//! that kittens get no more than their thread (docs/knowledge/permissions.md).

use neko_protocol::PermissionMode;
use rune::{El, col, row, text};

use super::{Found, note, section};
use crate::state::App;
use crate::ui::base::{Theme, glyph};
use crate::ui::icons;
use crate::ui::radio_group::RadioGroup;

/// What Settings says about full access, plainly.
pub const FULL_ACCESS_WARNING: &str =
    "With full access the agent runs any command, changes any file and reaches the internet without asking you. Use it only for work you could lose.";

/// A line led by a glyph, read as one status.
fn said(t: &Theme, key: &str, icon: &'static rune::Icon, ink: rune::Color, words: &str, strong: bool) -> El {
    row()
        .key(key.to_owned())
        .role("status")
        .label(words.to_owned())
        .w_full()
        .gap(t.metrics.gap)
        .items_start()
        .child(row().size(t.control.icon, t.ty.base.line_height).center().child(glyph(icon, t.control.icon, ink)))
        .child(text(words.to_owned()).font(t.ty.base).color(if strong { t.ink() } else { t.ink_muted() }).wrap().min_w(0.0).shrink().a11y_hidden())
}

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let Some((list, settings)) = &s.runtimes else {
        return col().w_full().child(text("Waiting for Neko…").key("settings-looking").role("status").font(t.ty.base).color(t.ink_faint()));
    };
    let mode = settings.permission_mode;
    let modes = [
        (
            "Follow Claude Code's own rules",
            Some("Neko adds nothing. What your Claude Code settings allow runs without asking, as in Terminal. Anything Claude Code would ask about is asked in the thread."),
        ),
        (
            "Ask before risky steps",
            Some("Neko asks before every command, any change outside the thread's folder and the internet, whatever your Claude Code settings allow. Reading, and editing in the thread's own folder, go ahead."),
        ),
        ("Full access", Some("Everything goes ahead without asking.")),
    ];
    let chosen = match mode {
        PermissionMode::Follow => 0,
        PermissionMode::Ask => 1,
        PermissionMode::Full => 2,
    };
    let mut perms = section(t, "settings-permissions", "How much the agent may do without asking", None);
    // What the person's own Claude Code settings let it do, read from them (never written), when Neko follows them. It sits
    // right under the heading, so it is in sight without scrolling in the default window.
    let rules = list.iter().find(|r| r.id == settings.runtime).and_then(|r| r.rules.clone());
    if let (PermissionMode::Follow, Some(line)) = (mode, rules) {
        perms = perms.child(said(t, "settings-rules", &icons::INFO, t.ink_faint(), &line, false));
    }
    perms = perms.child(
        RadioGroup::<App>::new("settings-mode", "How much the agent may do without asking", &modes, Some(chosen))
            .on_change(|s: &mut App, i| {
                s.set_permission_mode(match i {
                    1 => PermissionMode::Ask,
                    2 => PermissionMode::Full,
                    _ => PermissionMode::Follow,
                })
            })
            .build(t),
    );
    if mode == PermissionMode::Full {
        perms = perms.child(said(t, "settings-full-warning", &icons::WARNING, t.c.warning.fg, FULL_ACCESS_WARNING, true));
    }
    let kittens = section(t, "settings-kitten-access", "Kittens", None).child(note(
        t,
        "settings-kitten-access-words",
        "A kitten works under the same choice as the thread that started it, never more. Its \u{201c}Always allow\u{201d} answers are its own and start empty; what it asks comes to Needs you.",
    ));
    col().w_full().gap(t.space.at(8.0)).child(f.lit(t, "settings-permissions", perms)).child(f.lit(t, "settings-kitten-access", kittens))
}
