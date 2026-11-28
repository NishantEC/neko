//! Settings ▸ Cattains: every Cattain, the active ones with Open, the retired ones with Bring back, New Cattain…, and whether a
//! thread's agent may make one (docs/knowledge/cats.md).

use rune::{El, row, text};
use rune_ui_style::{ButtonVariant, Size, weight};

use super::Found;
use crate::state::App;
use crate::ui::badge::Badge;
use crate::ui::base::{Shape, Theme, Tone, paint};
use crate::ui::button::Button;

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn card(s: &App, t: &Theme, c: &neko_protocol::Cat) -> El {
    let key = format!("settings-cat-{}", c.id);
    let n = s.tasks.iter().filter(|x| x.listed() && x.crew_of() == Some(c.id.as_str())).count();
    let count = match n {
        0 => "No crew yet".to_owned(),
        1 => "1 thread in its crew".to_owned(),
        n => format!("{n} threads in its crew"),
    };
    let mut head = row().w_full().gap(t.space.at(2.0)).center_y().child(text(c.name.clone()).role("heading").level(4).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).shrink());
    if !c.is_active() {
        head = head.child(Badge::new(format!("{key}-retired"), "Retired").tone(Tone::Neutral).dot().build(t));
    }
    let mut facts = rune::col().grow(1.0).min_w(0.0).gap(t.space.at(1.0)).child(head);
    if let Some(p) = c.purpose.as_ref().filter(|p| !p.is_empty()) {
        facts = facts.child(text(p.clone()).key(format!("{key}-purpose")).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0));
    }
    let mut state = if c.is_active() { format!("{} · {count}", capital(&crate::sidebar::cat_words(s, c))) } else { count };
    // One an agent made says where, while that thread is there.
    if let Some(by) = c.made_in.as_ref().and_then(|m| s.tasks.iter().find(|x| &x.id == m)) {
        state.push_str(&format!(" · made in \u{201c}{}\u{201d}", by.title));
    }
    facts = facts.child(text(state).key(format!("{key}-facts")).font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0));
    let id = c.id.clone();
    let action = if c.is_active() {
        Button::new(format!("{key}-open"), "Open").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| {
            s.settings_open = false;
            s.open_cattain(&id);
        })
        .build(t)
    } else {
        Button::new(format!("{key}-back"), "Bring back").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.bring_back_cat(&id)).build(t)
    };
    let st = if c.is_active() { crate::cattain_view::cattain_status(s, c) } else { None };
    let face = crate::faces::portrait_in(t, &format!("{key}-face"), &c.face, 40.0, st, t.c.surface.bg);
    let face = if c.is_active() { face } else { face.opacity(0.55) };
    paint(row().key(key.clone()).role("group").label(c.name.clone()).w_full().padding(t.space.at(3.0)).gap(t.space.at(3.0)).center_y().radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(face).child(facts).child(action)
}

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let active = s.active_cats().len();
    let about = format!(
        "Cats with a thread of their own to talk to, their own files (RULES.md, CREW.md, NOTES.md, DIGEST.md) and a crew of threads. A retired Cattain keeps its threads readable and takes no new crew until you bring it back. {active} of at most {} are active.",
        neko_protocol::cats::MAX_ACTIVE
    );
    let about = text(about).key("settings-cats-about").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0);
    let mut part = rune::col().key("settings-cats").w_full().gap(t.space.at(3.0)).child(f.lit(t, "settings-cats-about", about));
    // Cattains are made here (the sidebar lists only the ones at work): name it, and its thread opens on its Profile.
    let new = Button::new("settings-new-cat", "New Cattain…").variant(ButtonVariant::Secondary).size(Size::Sm).icon(&crate::ui::icons::PLUS).disabled(active >= neko_protocol::cats::MAX_ACTIVE).on_press(|s: &mut App| s.start_new_cat()).build(t);
    part = part.child(row().child(f.lit_hug(t, "settings-new-cat", new)));
    // A thread's agent may make one when the person asks ("make a Cattain for the iOS work"), unless they keep it to this page.
    if let Some((_, settings)) = &s.runtimes {
        let on = !settings.cattains_by_hand;
        let sw = crate::ui::switch::Switch::<App>::new("settings-agents-make-cattains", "Agents may make Cattains", on)
            .help("When you ask in a thread (\u{201c}make a Cattain for the iOS work\u{201d}), or when work clearly needs a standing owner, the agent makes one and tells you. Undo takes it back.")
            .on_toggle(move |s: &mut App| s.set_agents_make_cattains(!on))
            .build(t);
        part = part.child(f.lit(t, "settings-agents-make-cattains", sw));
    }
    for c in &s.cats {
        part = part.child(f.lit(t, &format!("settings-cat-{}", c.id), card(s, t, c)));
    }
    part
}
