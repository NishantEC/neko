//! Part 2: the activity capsule. A small glass pill at the top-centre of the display, in a
//! transparent non-activating overlay that takes no input.
//!
//! The overlay window is larger than the pill so Rune's own shadow has room; the system shadow
//! stays off (see `System materials` in Rune's README). Glass is `NSGlassEffectView` on macOS 26+
//! and the popover material before; headless renders draw the element's fill instead.

use rune::{Color, Cx, El, Shadow, SystemMaterial, TextStyle, col, el, row, text};

pub const PILL_H: f32 = 38.0;
pub const MARGIN: f32 = 30.0;
/// Overlay window size: room around the pill for the shadow.
pub const WINDOW: (f32, f32) = (420.0, PILL_H + 2.0 * MARGIN);

pub struct Capsule {
    pub agents: u32,
    pub ticket: String,
}

impl Capsule {
    pub fn new() -> Self {
        Self { agents: 2, ticket: "NK-14".into() }
    }
    fn summary(&self) -> String {
        let noun = if self.agents == 1 { "agent" } else { "agents" };
        format!("{} {noun} working · {}", self.agents, self.ticket)
    }
}

pub fn view(s: &Capsule, cx: &mut Cx<Capsule>) -> El {
    let dark = cx.system_dark();
    let ink = if dark { Color::WHITE } else { Color::BLACK };
    let frost = if dark { Color { r: 0.12, g: 0.12, b: 0.13, a: 1.0 } } else { Color::WHITE }.alpha(0.55);
    let green = Color { r: 0.20, g: 0.82, b: 0.45, a: 1.0 };
    let r = PILL_H / 2.0;
    let content = row().key("content").h_full().px(16.0).gap(9.0).center_y()
        .child(el().key("status").size(8.0, 8.0).radius(4.0).fill(green)
            .box_shadow(Shadow::core_animation(0.0, 0.0, 3.0, green.alpha(0.7))))
        .child(text(s.summary()).font(TextStyle::new(13.0, 560.0)).color(ink.alpha(0.88)));
    let frosted = el().key("frost").h_full().radius(r).continuous_corners().clip_rounded(r)
        .fill(frost).border(1.0, ink.alpha(0.10)).child(content);
    let pill = el().key("pill").h(PILL_H).radius(r).continuous_corners()
        .box_shadow(Shadow::core_animation(0.0, 0.5, 1.0, Color::BLACK.alpha(0.28)))
        .box_shadow(Shadow::core_animation(0.0, 6.0, 10.0, Color::BLACK.alpha(0.26)))
        .system_material(SystemMaterial::Glass)
        .child(frosted);
    col().key("root").w_full().h_full().center().child(pill)
}
