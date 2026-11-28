//! Settings ▸ Kittens: the one choice (ask before a thread's first kitten) and the limits, said plainly (docs/knowledge/subagents.md).
//! Subagents are kittens to the person.

use rune::{El, col};

use super::{Found, note};
use crate::state::App;
use crate::ui::base::Theme;
use crate::ui::switch::Switch;

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let mut p = super::lead(t, "settings-kittens", "A thread can split its work into kittens: helpers it starts, numbered Kitten 1, Kitten 2, that report back to it when they are done. It rests while they work.");
    if let Some((_, settings)) = &s.runtimes {
        let on = settings.ask_first_subagent;
        let sw = Switch::<App>::new("settings-ask-first-kitten", "Ask before a thread's first kitten", on)
            .help("The first kitten a thread starts waits as a draft until you press Start.")
            .on_toggle(move |s: &mut App| s.set_ask_first_subagent(!on))
            .build(t);
        p = p.child(f.lit(t, "settings-ask-first-kitten", sw));
    }
    let limits = format!(
        "Up to {} kittens of a thread work at once and {} across Neko; the rest wait their turn. Kittens can start kittens, {} levels deep. When this Mac is warm, new kittens wait until it cools. A kitten that writes works in a copy of the project of its own.",
        neko_protocol::SUBAGENTS_PER_THREAD,
        neko_protocol::RUNS_ACROSS_NEKO,
        neko_protocol::SUBAGENT_DEPTH
    );
    let limits = col().w_full().gap(t.space.at(3.0)).pt(t.space.at(2.0)).child(super::h3(t, "How many work at once")).child(note(t, "settings-kitten-limits", limits));
    p.child(f.lit(t, "settings-kitten-limits", limits))
}
