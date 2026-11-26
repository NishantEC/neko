//! Contrast, measured from the theme itself in light and dark (WCAG 2.2): every ink that carries words reaches 4.5:1 on every
//! surface it is drawn on, and every mark that carries meaning without words (the focus ring, the working ring, the state dots, a
//! card's edge colour) reaches 3:1 against what is around it.

use rune::Color;
use rune_ui_style::{contrast, over};

use neko_app::ui::portrait::{DOT_CONTRAST, RING_CONTRAST, seen_on};
use neko_app::view::{raised_fill, theme};

/// Each surface as painted: a translucent one (a well, a wash) laid over the paper it sits on.
fn surfaces(t: &neko_app::ui::base::Theme) -> Vec<(&'static str, Color)> {
    let paper = t.c.page.bg;
    let painted = |c: Color| if c.a < 1.0 { over(paper, c) } else { c };
    let list: Vec<(&'static str, Color)> = vec![
        ("paper", t.c.page.bg),
        ("window ground", t.c.surface.bg),
        ("sidebar panel", neko_app::sidebar::panel_bg(t)),
        ("raised", raised_fill(t)),
        ("well", t.c.input.bg),
        // A selected row and a hovered one: the row material's wash over the panel.
        ("selected row", over(neko_app::sidebar::panel_bg(t), t.c.selected)),
    ];
    let mut list: Vec<(&'static str, Color)> = list.into_iter().map(|(n, c)| (n, painted(c))).collect();
    // The wash behind a cat's hero, one per face: the name, state and purpose are set on it.
    for f in neko_protocol::cats::FACES {
        list.push(("cat wash", neko_app::faces::wash(t, f)));
    }
    list
}

#[test]
fn words_reach_four_and_a_half_to_one_on_every_surface() {
    for dark in [false, true] {
        rune_ui_style::fonts::install(neko_app::ui::FONTS);
        let t = theme(dark, false);
        let inks = [
            ("ink", t.ink()),
            ("second ink", t.ink_muted()),
            ("third ink", t.ink_faint()),
            ("working (identity)", t.c.accent.fg),
            ("needs you", t.c.warning.fg),
            ("failed", t.c.danger.fg),
            ("added", t.c.success.fg),
        ];
        for (surface, bg) in surfaces(&t) {
            for (name, ink) in inks {
                let c = contrast(ink, Color { a: 1.0, ..bg });
                assert!(c >= 4.5, "{} {name} on {surface}: {c:.2}:1", if dark { "dark" } else { "light" });
            }
        }
    }
}

#[test]
fn marks_without_words_reach_three_to_one() {
    for dark in [false, true] {
        rune_ui_style::fonts::install(neko_app::ui::FONTS);
        let t = theme(dark, false);
        let which = if dark { "dark" } else { "light" };
        for (surface, bg) in surfaces(&t) {
            let bg = Color { a: 1.0, ..bg };
            // The portrait's ring and dots are the colours rune-ui's Portrait draws on this surface (taken toward the strong ink until
            // they are seen on it).
            let id = t.identity();
            let seen = |tone: &rune_ui_style::tokens::Tone, min: f32| seen_on(tone.solid, tone.fg, bg, min);
            let marks = [
                ("focus ring", t.c.ring),
                ("working ring", seen_on(id.solid, id.fg, bg, RING_CONTRAST)),
                ("needs-you dot", seen(&t.c.warning, DOT_CONTRAST)),
                ("failed dot", seen(&t.c.danger, DOT_CONTRAST)),
            ];
            for (name, mark) in marks {
                let c = contrast(mark, bg);
                assert!(c >= 3.0, "{which} {name} on {surface}: {c:.2}:1");
            }
        }
    }
}

/// A cat's page puts its face on a wash of its ground: every wash is the same colour step from the page, so a deep ground (the
/// white cat's blue) is no heavier than a pale one (the grey cat's yellow), and none vanishes into the page.
#[test]
fn every_cats_wash_sits_the_same_step_from_the_page() {
    rune_ui_style::fonts::install(neko_app::ui::FONTS);
    for dark in [false, true] {
        let t = theme(dark, false);
        let steps: Vec<f32> = neko_protocol::cats::FACES.iter().map(|f| neko_app::faces::wash_step(t.c.page.bg, neko_app::faces::wash(&t, f))).collect();
        let (lo, hi) = (steps.iter().copied().fold(f32::MAX, f32::min), steps.iter().copied().fold(0.0, f32::max));
        assert!(hi - lo < 0.006, "dark {dark}: the washes differ in strength, {steps:?}");
        assert!(lo > 0.04, "dark {dark}: a wash vanishes into the page, {steps:?}");
    }
}
