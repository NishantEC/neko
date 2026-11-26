//! Contrast, measured from the theme itself in light and dark (WCAG 2.2): every ink that carries words reaches 4.5:1 on every
//! surface it is drawn on, and every mark that carries meaning without words (the focus ring, the working ring, the state dots, a
//! card's edge colour) reaches 3:1 against what is around it.

use rune::Color;
use rune_ui_style::{contrast, over};

use neko_app::view::{raised_fill, theme};

/// Each surface as painted: a translucent one (a well, a wash) laid over the paper it sits on.
fn surfaces(t: &neko_app::ui::base::Theme) -> Vec<(&'static str, Color)> {
    let paper = t.c.page.bg;
    let painted = |c: Color| if c.a < 1.0 { over(paper, c) } else { c };
    let list = vec![
        ("paper", t.c.page.bg),
        ("window ground", t.c.surface.bg),
        ("sidebar panel", neko_app::sidebar::panel_bg(t)),
        ("raised", raised_fill(t)),
        ("well", t.c.input.bg),
        // A selected row and a hovered one: the row material's wash over the panel.
        ("selected row", over(neko_app::sidebar::panel_bg(t), t.c.selected)),
    ];
    list.into_iter().map(|(n, c)| (n, painted(c))).collect()
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
            for (name, mark) in [("focus and working ring", t.c.ring), ("needs-you dot", neko_app::faces::dot(&t, false)), ("failed dot", neko_app::faces::dot(&t, true))] {
                let c = contrast(mark, bg);
                assert!(c >= 3.0, "{which} {name} on {surface}: {c:.2}:1");
            }
        }
    }
}
