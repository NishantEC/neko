//! Part 1: the agent pointer. A full-display, transparent, click-through overlay that draws a soft
//! dot with a label, moved by springs, plus a trail of lagging dots.
//!
//! Motion: one `Live<[f32; 2]>` holds the target. Every dot binds it through `Live::smooth(Spring)`
//! with a different spring, so the head leads and each trail dot lags a little more. The producer
//! (stdin thread) only calls `set`; no view rebuild, no layout, no app tick. The label is state, so
//! changing it rebuilds the view once, and the dot's offset survives the rebuild.

use rune::{Color, Cx, El, Live, Spring, TextStyle, Transition, el, row, text};

pub const TRAIL: usize = 6;
const HEAD_R: f32 = 9.0;
const ACCENT: Color = Color { r: 0.30, g: 0.62, b: 1.0, a: 1.0 };

pub struct Pointer {
    pub cursor: Live<[f32; 2]>,
    pub label: String,
}

impl Pointer {
    pub fn new() -> Self {
        Self { cursor: Live::new([160.0, 140.0]), label: String::new() }
    }
}

/// Where the demo goes, in points from the display's top-left, and what the label says there.
pub const DEMO: [(f32, f32, &str); 5] = [
    (300.0, 220.0, "Opening the file menu"),
    (720.0, 180.0, "Clicking Save"),
    (860.0, 480.0, "Typing a commit message"),
    (420.0, 560.0, "Scrolling the diff"),
    (240.0, 300.0, ""),
];
pub const DEMO_STEP_SECONDS: f64 = 1.4;

/// Move to demo waypoint `k`.
pub fn demo_step(s: &mut Pointer, k: usize) {
    let (x, y, label) = DEMO[k % DEMO.len()];
    s.cursor.set([x, y]);
    s.label = label.to_string();
}

/// One dot: a zero-size anchor that rides the spring, with the dot centred on it.
fn dot(key: String, cursor: &Live<[f32; 2]>, spring: Spring, r: f32, alpha: f32) -> El {
    let soft = [(0.0, ACCENT.alpha(alpha)), (0.55, ACCENT.alpha(alpha * 0.85)), (1.0, ACCENT.alpha(0.0))];
    el().key(key).size(0.0, 0.0).pin(Some(0.0), Some(0.0), None, None).live_offset(cursor.smooth(spring))
        .child(el().absolute(-r * 1.8, -r * 1.8).size(r * 3.6, r * 3.6).radius(r * 1.8)
            .radial_gradient([0.5, 0.5], r * 1.8, soft))
}

fn label_chip(label: &str) -> El {
    row().key("label").absolute(HEAD_R + 12.0, -HEAD_R - 14.0).h(26.0).px(11.0).center_y().radius(13.0)
        .fill(Color { r: 0.05, g: 0.06, b: 0.09, a: 0.82 }).border(1.0, Color::WHITE.alpha(0.14))
        .transition(Transition::opacity())
        .child(text(label).font(TextStyle::new(12.5, 560.0)).color(Color::WHITE.alpha(0.95)))
}

pub fn view(s: &Pointer, _: &mut Cx<Pointer>) -> El {
    let mut root = el().key("root").w_full().h_full();
    // Back to front: the oldest (slowest) dot first, the head last.
    for i in (0..TRAIL).rev() {
        let t = i as f32 / TRAIL as f32;
        let spring = Spring::new(0.20 + 0.075 * (i as f32 + 1.0), 0.0);
        root = root.child(dot(format!("trail-{i}"), &s.cursor, spring, HEAD_R * (0.95 - 0.55 * t), 0.55 * (1.0 - t)));
    }
    let head = dot("head".into(), &s.cursor, Spring::new(0.20, 0.12), HEAD_R, 0.95)
        .child(el().absolute(-3.0, -3.0).size(6.0, 6.0).radius(3.0).fill(Color::WHITE))
        .children((!s.label.is_empty()).then(|| label_chip(&s.label)));
    root.child(head)
}

/// Headless only: drives the demo from the virtual clock, so `--snap --at T` is deterministic.
pub fn poll(s: &mut Pointer, t: f64, _: &mut rune::Tx) -> bool {
    let k = (t / DEMO_STEP_SECONDS) as usize;
    let (x, y, label) = DEMO[k % DEMO.len()];
    if s.cursor.get() == [x, y] && s.label == label { return false; }
    s.cursor.set([x, y]);
    s.label = label.to_string();
    true
}
