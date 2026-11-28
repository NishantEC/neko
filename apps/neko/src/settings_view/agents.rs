//! Settings ▸ Agents & models: each agent CLI Neko knows, as found on this Mac, which one new threads use, and the model. Neko asks
//! for no API key: it uses the CLIs already installed and signed in.

use neko_protocol::RuntimeInfo;
use rune::{El, col, row, text};
use rune_ui_style::weight;

use super::{Found, h4, note, section};
use crate::state::App;
use crate::task_view::command_line;
use crate::ui::badge::Badge;
use crate::ui::base::{Shape, Theme, Tone, paint};
use crate::ui::radio_group::RadioGroup;

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
        .child(text(r.name.clone()).role("heading").level(4).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).grow(1.0).truncate().min_w(0.0).shrink())
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
    let frame = col().key(key.clone()).role("group").label(format!("{}: {word}", r.name)).w_full().padding(t.metrics.inset).gap(t.space.at(2.5)).radius(t.radius.xl).round_as(t);
    let frame = paint(frame, &t.mat.slab.base);
    let mut card = frame.child(head);
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

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let mut agents = section(t, "settings-agents", "Agent CLIs", Some("Neko uses the agent CLIs already installed and signed in on this Mac. It asks for no API key and never stores one."));
    let Some((list, settings)) = &s.runtimes else {
        return agents.child(text("Looking for agent CLIs…").key("settings-looking").role("status").font(t.ty.base).color(t.ink_faint()));
    };
    for r in list {
        let key = format!("agent-{}", r.id);
        agents = agents.child(f.lit(t, &key, agent_card(t, r)));
    }
    let usable: Vec<&RuntimeInfo> = list.iter().filter(|r| r.supported).collect();
    let names: Vec<String> = usable.iter().map(|r| r.name.clone()).collect();
    let options: Vec<(&str, Option<&str>)> = names.iter().map(|n| (n.as_str(), None)).collect();
    let chosen = usable.iter().position(|r| r.id == settings.runtime);
    let ids: Vec<String> = usable.iter().map(|r| r.id.clone()).collect();
    let runtime = RadioGroup::<App>::new("settings-runtime", "New threads use", &options, chosen)
        .on_change(move |s: &mut App, i| {
            if let Some(id) = ids.get(i) {
                s.set_runtime(id);
            }
        })
        .build(t);
    let uses = col().key("settings-uses").w_full().gap(t.space.at(3.0)).child(h4(t, "New threads use")).child(runtime);
    let name = list.iter().find(|r| r.id == settings.runtime).map_or("the agent", |r| r.name.as_str());
    let model = section(t, "settings-model", "Model", None).child(note(
        t,
        "settings-model-words",
        format!("Neko chooses no model: a thread uses the one {name} is set to. Change it there and every new run follows."),
    ));
    col().w_full().gap(t.space.at(8.0)).child(agents.child(f.lit(t, "settings-runtime", uses))).child(f.lit(t, "settings-model", model))
}
