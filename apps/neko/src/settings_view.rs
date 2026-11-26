//! Settings (⌘,): one page. "Agents": each agent CLI Neko knows, as found on this Mac, which one new tasks use, and how much a
//! task's agent may do without asking. Neko asks for no API key: it uses the CLIs already installed and signed in.

use neko_protocol::{PermissionMode, RuntimeInfo};
use rune::{El, col, row, text};
use rune_ui_style::weight;

use crate::ui::base::Shape;

use crate::state::App;
use crate::task_view::command_line;
use crate::ui::badge::Badge;
use crate::ui::base::{Theme, Tone, glyph, paint};
use crate::ui::icons;
use crate::ui::radio_group::RadioGroup;
use crate::view::COLUMN;

/// What Settings says about full access, plainly.
pub const FULL_ACCESS_WARNING: &str =
    "With full access the agent runs any command, changes any file and reaches the internet without asking you. Use it only for work you could lose.";

fn state_of(r: &RuntimeInfo) -> (&'static str, Tone) {
    match (r.found, r.supported, r.signed_in) {
        (false, _, _) => ("Not installed", Tone::Neutral),
        (true, false, _) => ("Not supported yet", Tone::Neutral),
        (true, true, Some(false)) => ("Not signed in", Tone::Warning),
        (true, true, _) => ("Ready", Tone::Success),
    }
}

fn agent_card(t: &Theme, r: &RuntimeInfo) -> El {
    let (word, tone) = state_of(r);
    let key = format!("agent-{}", r.id);
    let head = row()
        .w_full()
        .gap(t.space.at(3.0))
        .center_y()
        .child(text(r.name.clone()).role("heading").level(3).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).grow(1.0).truncate().min_w(0.0).shrink())
        .child(Badge::new(format!("{key}-state"), word).tone(tone).dot().build(t));
    let mut facts: Vec<String> = Vec::new();
    if let Some(v) = &r.version {
        facts.push(format!("Version {v}"));
    }
    match (r.signed_in, &r.login) {
        (Some(true), Some(l)) => facts.push(format!("Signed in with {l}")),
        (Some(true), None) => facts.push("Signed in".into()),
        (Some(false), _) => facts.push("Not signed in".into()),
        (None, _) if r.found && r.supported => facts.push("Could not tell whether it is signed in".into()),
        _ => {}
    }
    if r.found && !r.supported {
        facts.push(format!("This Neko cannot run {} yet", r.name));
    }
    let mut card = paint(col().key(key.clone()).role("group").label(format!("{}: {word}", r.name)).w_full().padding(t.metrics.inset).gap(t.space.at(2.5)).radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(head);
    if !facts.is_empty() {
        card = card.child(text(facts.join(" · ")).key(format!("{key}-facts")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    }
    if !r.found {
        card = card.child(text("Install it in Terminal:").font(t.ty.sm).color(t.ink_faint())).child(command_line(t, &format!("{key}-install"), &r.install));
    } else if r.signed_in == Some(false) {
        card = card.child(text("Sign in in Terminal:").font(t.ty.sm).color(t.ink_faint())).child(command_line(t, &format!("{key}-sign-in"), &r.sign_in));
    }
    card
}

pub fn settings_view(s: &App, t: &Theme) -> El {
    let mut page = col().key("settings").w_full().max_w(COLUMN).gap(t.space.at(6.0));
    page = page.child(text("Settings").key("settings-title").role("heading").level(1).font(weight(t.ty.xl, t.ty.medium)).color(t.ink()));
    let mut agents = col().key("settings-agents").w_full().gap(t.space.at(3.0)).child(text("Agents").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())).child(
        text("Neko uses the agent CLIs already installed and signed in on this Mac. It asks for no API key and never stores one.").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0),
    );
    match &s.runtimes {
        None => agents = agents.child(text("Looking for agent CLIs…").key("settings-looking").role("status").font(t.ty.base).color(t.ink_faint())),
        Some((list, settings)) => {
            for r in list {
                agents = agents.child(agent_card(t, r));
            }
            let usable: Vec<&RuntimeInfo> = list.iter().filter(|r| r.supported).collect();
            let names: Vec<String> = usable.iter().map(|r| r.name.clone()).collect();
            let options: Vec<(&str, Option<&str>)> = names.iter().map(|n| (n.as_str(), None)).collect();
            let chosen = usable.iter().position(|r| r.id == settings.runtime);
            let ids: Vec<String> = usable.iter().map(|r| r.id.clone()).collect();
            agents = agents.child(text("New tasks use").role("heading").level(3).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).pt(t.space.at(2.0))).child(
                RadioGroup::<App>::new("settings-runtime", "New tasks use", &options, chosen)
                    .on_change(move |s: &mut App, i| {
                        if let Some(id) = ids.get(i) {
                            s.set_runtime(id);
                        }
                    })
                    .build(t),
            );
            let mode = settings.permission_mode;
            let modes = [
                ("Ask before risky steps", Some("Reading, and editing inside the task's own folder, go ahead. Commands, other folders, the internet and anything that deletes ask first.")),
                ("Full access", Some("Everything goes ahead without asking.")),
            ];
            let mut perms = col().key("settings-permissions").w_full().gap(t.space.at(3.0)).child(text("Permissions").role("heading").level(2).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())).child(
                RadioGroup::<App>::new("settings-mode", "How much the agent may do without asking", &modes, Some(if mode == PermissionMode::Full { 1 } else { 0 }))
                    .on_change(|s: &mut App, i| s.set_permission_mode(if i == 1 { PermissionMode::Full } else { PermissionMode::Ask }))
                    .build(t),
            );
            if mode == PermissionMode::Full {
                perms = perms.child(
                    row()
                        .key("settings-full-warning")
                        .role("status")
                        .label(FULL_ACCESS_WARNING)
                        .w_full()
                        .gap(t.metrics.gap)
                        .items_start()
                        .child(row().size(t.control.icon, t.ty.base.line_height).center().child(glyph(&icons::WARNING, t.control.icon, t.c.warning.fg)))
                        .child(text(FULL_ACCESS_WARNING).font(t.ty.base).color(t.ink()).wrap().min_w(0.0).shrink().a11y_hidden()),
                );
            }
            page = page.child(agents).child(perms);
            return wrap(t, page);
        }
    }
    wrap(t, page.child(agents))
}

fn wrap(t: &Theme, page: El) -> El {
    col().key("settings-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.space.at(8.0)).pt(t.space.at(6.0)).pb(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}
