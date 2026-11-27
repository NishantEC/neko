//! The cats' portraits (docs/knowledge/cats.md, docs/knowledge/design.md): one drawn face per face id the daemon knows
//! (`neko_protocol::cats::FACES`), each on a ground colour of its own, so eight cats are told apart by colour at 20 points and by
//! their markings at 76.
//!
//! A portrait is rune-ui's `Portrait` tile with the cat's head and shoulders (cut by its foot) drawn as its face, on the face's
//! ground. [`portrait_in`] puts it in its state: a turning ring while the cat works (still under Reduce Motion), a dot at its foot
//! when it needs the person or could not go on.
//!
//! Development builds only: when `NEKO_CAT_ART` names a folder holding `<face>.png` (`ginger.png`, `calico.png`, …), that picture is
//! drawn in place of the face. No artwork ships with Neko; a release build never reads the variable.

use std::cell::RefCell;
use std::collections::HashMap;

use neko_protocol::TaskStatus;
use rune::{Color, El, LineCap, Pixels, Shape, el};
use rune_ui_style::hex;

use crate::ui::base::Theme;
use crate::ui::portrait::{Activity, Portrait};
pub use crate::ui::portrait::{radius, ringed};

/// How a face is drawn: its ground, coat, inner ear, muzzle, markings and eyes.
pub struct Coat {
    pub id: &'static str,
    /// The face's name in words ("Ginger").
    pub words: &'static str,
    /// The portrait's ground: the colour that tells cats apart at a glance.
    pub ground: &'static str,
    coat: &'static str,
    ear: &'static str,
    muzzle: &'static str,
    eyes: (&'static str, &'static str),
    marks: Marks,
    /// The silhouette: how high the ear tips reach (lower is taller), how much wider the cheeks are, and whether the cheeks are
    /// ruffed (a long coat's tufts).
    ears: f32,
    cheeks: f32,
    ruff: bool,
}

/// The markings that make a face itself.
#[derive(Clone, Copy)]
enum Marks {
    /// An M on the forehead and stripes on the cheeks, in `.0`.
    Tabby(&'static str),
    /// Forehead and cheek stripes in `.0`.
    Stripes(&'static str),
    /// A white blaze, muzzle and bib on a dark coat.
    Tuxedo,
    /// An orange patch over one ear and a black one over the other.
    Patches,
    /// Dark points: ears and a mask.
    Points(&'static str),
    /// Nothing: a solid coat.
    Plain,
}

/// Every face the daemon knows, in its order.
pub const COATS: &[Coat] = &[
    Coat { id: "tabby", words: "Tabby", ground: "#a9cfe6", coat: "#a08a72", ear: "#e6b3a4", muzzle: "#e3d6c4", eyes: ("#d9a23a", "#d9a23a"), marks: Marks::Tabby("#5d4c3c"), ears: 5.0, cheeks: 0.8, ruff: true },
    Coat { id: "ginger", words: "Ginger", ground: "#2f9c94", coat: "#ec9a4e", ear: "#f6c6a3", muzzle: "#f8e1c6", eyes: ("#93c24c", "#93c24c"), marks: Marks::Stripes("#c26a26"), ears: 5.0, cheeks: 0.3, ruff: false },
    Coat { id: "tuxedo", words: "Tuxedo", ground: "#e48a6c", coat: "#2a292d", ear: "#5b4a4c", muzzle: "#f6f2ec", eyes: ("#cdd64c", "#cdd64c"), marks: Marks::Tuxedo, ears: 4.6, cheeks: 0.0, ruff: false },
    Coat { id: "calico", words: "Calico", ground: "#f3abba", coat: "#f8f2e9", ear: "#f0b8b4", muzzle: "#fffaf3", eyes: ("#e0a23c", "#e0a23c"), marks: Marks::Patches, ears: 5.6, cheeks: 0.2, ruff: false },
    Coat { id: "siamese", words: "Siamese", ground: "#bba8ea", coat: "#f1e5d1", ear: "#5a4034", muzzle: "#f1e5d1", eyes: ("#5aa8e8", "#5aa8e8"), marks: Marks::Points("#5a4034"), ears: 2.6, cheeks: -1.0, ruff: false },
    Coat { id: "void", words: "Black", ground: "#8fb27f", coat: "#29262c", ear: "#54474f", muzzle: "#36323a", eyes: ("#d8d24c", "#d8d24c"), marks: Marks::Plain, ears: 3.8, cheeks: -0.5, ruff: false },
    Coat { id: "snow", words: "White", ground: "#7894c6", coat: "#fcf9f4", ear: "#f3b5b3", muzzle: "#ffffff", eyes: ("#62a9ea", "#e3a33f"), marks: Marks::Plain, ears: 8.2, cheeks: 1.8, ruff: true },
    Coat { id: "smoke", words: "Grey", ground: "#e4b23e", coat: "#9ba1a9", ear: "#d8c3c3", muzzle: "#cdd0d5", eyes: ("#d98f3a", "#d98f3a"), marks: Marks::Plain, ears: 6.8, cheeks: 1.2, ruff: false },
];

/// The coat of a face id; an unknown id (a face a newer daemon knows) gets the first.
pub fn coat(face: &str) -> &'static Coat {
    COATS.iter().find(|c| c.id == face).unwrap_or(&COATS[0])
}

/// A face's name in words ("Tabby").
pub fn words(face: &str) -> &'static str {
    COATS.iter().find(|c| c.id == face).map_or("Cat", |c| c.words)
}

/// An ellipse as SVG path data (four cubic arcs).
fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
    let (kx, ky) = (rx * 0.5523, ry * 0.5523);
    let (l, r, t, b) = (cx - rx, cx + rx, cy - ry, cy + ry);
    format!(
        "M{l} {cy} C{l} {} {} {t} {cx} {t} C{} {t} {r} {} {r} {cy} C{r} {} {} {b} {cx} {b} C{} {b} {l} {} {l} {cy} Z",
        cy - ky,
        cx - kx,
        cx + kx,
        cy - ky,
        cy + ky,
        cx + kx,
        cx - kx,
        cy + ky
    )
}

// The drawing, on a 48 point square: shoulders cut by the foot, the head a little above the middle.
const SHOULDERS: &str = "M5 48 C6 39.5 13.5 35 24 35 C34.5 35 42 39.5 43 48 Z";
const NOSE: &str = "M22.3 26.5 L25.7 26.5 Q25 28.1 24 28.5 Q23 28.1 22.3 26.5 Z";
const MOUTH: &str = "M24 28.5 Q24 30.3 21.9 30.7 M24 28.5 Q24 30.3 26.1 30.7";
const WHISKERS: &str = "M16.6 28.6 L8.6 27.6 M16.8 30.2 L9.2 31.4 M31.4 28.6 L39.4 27.6 M31.2 30.2 L38.8 31.4";

/// The head's outline: ear tips at `e` (the tips' height), cheeks `c` wider than the plain head on each side.
fn head(e: f32, c: f32) -> String {
    format!(
        "M{} 20 L11.6 {} Q12.1 {e} 13.5 {} L19.6 11.1 Q24 10.1 28.4 11.1 L34.5 {} Q35.9 {e} 36.4 {} L{} 20 Q{} 24 {} 28.6 Q37 36.2 24 36.6 Q11 36.2 {} 28.6 Q{} 24 {} 20 Z",
        10.5 - c * 0.4,
        e + 1.6,
        e + 0.9,
        e + 0.9,
        e + 1.6,
        37.5 + c * 0.4,
        40.0 + c,
        39.0 + c,
        9.0 - c,
        8.0 - c,
        10.5 - c * 0.4
    )
}

/// The inner ears, following the tips.
fn inner_ears(e: f32) -> String {
    format!("M13.3 {} L18.1 12.4 Q15.3 13.6 13.9 15.6 Z M34.7 {} L29.9 12.4 Q32.7 13.6 34.1 15.6 Z", e + 3.7, e + 3.7)
}

/// A long coat's ruff: tufts at the cheeks.
fn ruff(c: f32) -> String {
    let (l, r) = (8.6 - c, 39.4 + c);
    format!("M{} 25 L{} 27.4 L{} 28.2 L{} 30.2 L{} 30 Z M{} 25 L{} 27.4 L{} 28.2 L{} 30.2 L{} 30 Z", l + 0.6, l - 2.4, l - 0.4, l - 1.8, l + 1.6, r - 0.6, r + 2.4, r + 0.4, r + 1.8, r - 1.6)
}

/// A face, drawn at `size`: Neko's own vector, no artwork. Each face has its own silhouette (tall ears and a narrow head for the
/// siamese, small ears and a broad ruffed head for the white long coat), its own markings and its own eyes. Finer marks (whiskers,
/// eye highlights and rims) only where they can be seen.
pub fn drawing(face: &str, size: f32) -> El {
    let c = coat(face);
    let fur = hex(c.coat);
    let dark_coat = fur.luminance() < 0.08;
    let edge = if dark_coat { rune_ui_style::mix(fur, Color::WHITE, 0.18) } else { rune_ui_style::mix(fur, Color::BLACK, 0.38) };
    let body = rune_ui_style::mix(fur, Color::BLACK, if dark_coat { 0.2 } else { 0.08 });
    let ink = hex("#231c19");
    let outline = head(c.ears, c.cheeks);
    let fine = size >= 36.0;
    let mut s = Shape::new().view_box(48.0, 48.0).fill_svg(SHOULDERS, body);
    s = match c.marks {
        // The tuxedo's bib, and a light chest on the striped coats, under the head.
        Marks::Tuxedo => s.fill_svg("M16.6 48 Q17.6 39 24 37 Q30.4 39 31.4 48 Z", hex(c.muzzle)),
        Marks::Tabby(_) | Marks::Stripes(_) => s.fill_svg("M19 48 Q19.6 40 24 38 Q28.4 40 29 48 Z", hex(c.muzzle).alpha(0.85)),
        _ => s,
    };
    s = s.stroke_svg(SHOULDERS, 0.8, LineCap::Round, edge.alpha(0.6));
    if c.ruff {
        s = s.fill_svg(&ruff(c.cheeks), fur).stroke_svg(&ruff(c.cheeks), 0.7, LineCap::Round, edge);
    }
    s = s.fill_svg(&outline, fur);
    let e = c.ears;
    match c.marks {
        Marks::Patches => {
            s = s
                .fill_svg(&format!("M10.6 19.6 L11.6 {} Q12.1 {e} 13.5 {} L19.6 11.1 Q19.2 16.4 15.4 19.4 Q12.8 21 10.6 19.6 Z", e + 1.6, e + 0.9), hex("#e7964a"))
                .fill_svg(&format!("M28.4 11.1 L34.5 {} Q35.9 {e} 36.4 {} L37.2 15.2 Q33 16.8 29.4 14.6 Z", e + 0.9, e + 1.6), hex("#2e2a28"))
                .fill_svg(&ellipse(31.4, 33.0, 2.0, 1.5), hex("#e7964a").alpha(0.9));
        }
        Marks::Points(p) => {
            s = s
                .fill_svg(&format!("M11.6 {} Q12.1 {e} 13.5 {} L19.6 11.1 Q16 13 12.2 15.8 Z M36.4 {} Q35.9 {e} 34.5 {} L28.4 11.1 Q32 13 35.8 15.8 Z", e + 1.6, e + 0.9, e + 1.6, e + 0.9), hex(p))
                .fill_svg(&ellipse(24.0, 27.4, 7.4, 6.8), hex(p).alpha(0.92));
        }
        _ => {}
    }
    s = s.fill_svg(&inner_ears(e), hex(c.ear));
    // The muzzle: two cheeks and a chin, lighter than the coat (the siamese's mask and a solid dark coat stay as they are).
    if !matches!(c.marks, Marks::Points(_) | Marks::Plain) || matches!(face, "snow" | "smoke") {
        let m = hex(c.muzzle);
        s = s.fill_svg(&ellipse(21.0, 29.6, 3.9, 3.1), m).fill_svg(&ellipse(27.0, 29.6, 3.9, 3.1), m).fill_svg(&ellipse(24.0, 31.6, 2.6, 2.0), m);
    }
    match c.marks {
        Marks::Tabby(m) => {
            let m = hex(m);
            s = s
                .stroke_svg("M19.4 16.6 L21 12.8 L24 15.6 L27 12.8 L28.6 16.6", 1.3, LineCap::Round, m)
                .stroke_svg("M24 17.4 L24 19.2", 1.1, LineCap::Round, m)
                .stroke_svg("M10.2 22.6 L14.6 23.4 M10 25.4 L14 25.6 M37.8 22.6 L33.4 23.4 M38 25.4 L34 25.6", 1.1, LineCap::Round, m)
                .stroke_svg("M14 41 Q16 43 15.4 46 M34 41 Q32 43 32.6 46", 1.2, LineCap::Round, m.alpha(0.7));
        }
        Marks::Stripes(m) => {
            let m = hex(m);
            s = s
                .stroke_svg("M24 12.4 L24 17 M20.9 12.9 L21.5 16.3 M27.1 12.9 L26.5 16.3", 1.3, LineCap::Round, m)
                .stroke_svg("M10.2 22.6 L14.6 23.4 M10 25.4 L14 25.6 M37.8 22.6 L33.4 23.4 M38 25.4 L34 25.6", 1.1, LineCap::Round, m);
        }
        Marks::Tuxedo => {
            s = s.fill_svg("M24 15.4 Q22.4 21 19.6 26.4 Q24 25 28.4 26.4 Q25.6 21 24 15.4 Z", hex(c.muzzle));
        }
        _ => {}
    }
    s = s.stroke_svg(&outline, 0.9, LineCap::Round, edge);
    // The eyes: an iris, a slit pupil, a rim and a highlight. A siamese's are a little almond; a white long coat's rounder.
    let (rx, ry) = match face {
        "siamese" => (2.9, 2.5),
        "snow" | "smoke" => (2.8, 3.1),
        _ => (2.7, 3.0),
    };
    for (cx, iris) in [(18.3, c.eyes.0), (29.7, c.eyes.1)] {
        s = s.fill_svg(&ellipse(cx, 22.4, rx, ry), hex(iris)).fill_svg(&ellipse(cx, 22.6, 0.95, ry - 0.7), ink);
        if fine {
            s = s.stroke_svg(&ellipse(cx, 22.4, rx, ry), 0.5, LineCap::Round, ink.alpha(0.55)).fill_svg(&ellipse(cx + 0.9, 21.3, 0.75, 0.75), Color::WHITE.alpha(0.9));
        }
    }
    s = s.fill_svg(NOSE, hex("#e7868a")).stroke_svg(MOUTH, 0.8, LineCap::Round, if dark_coat { edge } else { rune_ui_style::mix(fur, ink, 0.6) });
    if size >= 40.0 {
        s = s.stroke_svg(WHISKERS, 0.55, LineCap::Round, if dark_coat { Color::WHITE.alpha(0.55) } else { ink.alpha(0.35) });
    }
    el().size(size, size).shape(s).a11y_hidden()
}

thread_local! {
    static ART: RefCell<Option<HashMap<&'static str, Pixels>>> = const { RefCell::new(None) };
}

/// The development override: `NEKO_CAT_ART/<face>.png` for each face that has one. Empty in a release build.
fn load_art() -> HashMap<&'static str, Pixels> {
    let mut out = HashMap::new();
    if !cfg!(debug_assertions) {
        return out;
    }
    let Some(dir) = std::env::var_os("NEKO_CAT_ART").filter(|d| !d.is_empty()) else { return out };
    let dir = std::path::PathBuf::from(dir);
    for c in COATS {
        let Ok(bytes) = std::fs::read(dir.join(format!("{}.png", c.id))) else { continue };
        let Ok(img) = rune::image::png_rgba8(&bytes) else { continue };
        let (w, h) = img.dimensions();
        let p = Pixels::new();
        p.set(w, h, img.into_raw());
        out.insert(c.id, p);
    }
    out
}

fn art(face: &str) -> Option<Pixels> {
    ART.with(|a| a.borrow_mut().get_or_insert_with(load_art).get(face).cloned())
}

/// How far a wash is from the page, as the biggest step in any colour channel as the eye's encoding has it (0 to 1).
pub fn wash_step(page: Color, wash: Color) -> f32 {
    let e = rune::color::srgb_encode;
    (e(page.r) - e(wash.r)).abs().max((e(page.g) - e(wash.g)).abs()).max((e(page.b) - e(wash.b)).abs())
}

/// The quiet colour behind a cat's hero on its page: the face's ground mixed into the page until every wash is the same colour
/// step from the page, so the deep blue of the white cat is no heavier than the yellow of the grey one (a fixed share of the
/// ground made the cool, dark grounds heavy and the pale ones faint). The share stays between a hint and a gentle tint.
pub fn wash(t: &Theme, face: &str) -> Color {
    let (page, ground) = (t.c.page.bg, hex(coat(face).ground));
    let far = wash_step(page, ground).max(0.01);
    let aim = if t.dark { 0.070 } else { 0.078 };
    rune_ui_style::mix(page, ground, (aim / far).clamp(0.04, 0.30))
}


/// A cat's portrait: rune-ui's `Portrait` tile with the cat's drawn face on its ground (or the development art). Hidden from
/// assistive technology: the row, turn or tile it sits in says the cat's name. `key` is the tile's own key (unique on the page).
pub fn portrait(t: &Theme, key: &str, face: &str, size: f32) -> El {
    tile(t, key, face, size, Activity::None, false, t.c.page.bg)
}

/// A portrait in its state: a calm turning ring in the identity colour while the cat works (a still arc under Reduce Motion), a
/// dot at its foot when it needs the person (amber) or could not go on (red). `under` is the surface it sits on (the dot's ring).
/// It takes the same room in every state ([`ringed`]), so nothing moves when the state changes.
pub fn portrait_in(t: &Theme, key: &str, face: &str, size: f32, st: Option<TaskStatus>, under: Color) -> El {
    let activity = match st {
        Some(TaskStatus::Working) => Activity::Working,
        Some(TaskStatus::NeedsYou | TaskStatus::Asked) => Activity::NeedsYou,
        Some(TaskStatus::Failed) => Activity::Failed,
        _ => Activity::None,
    };
    let tile = tile(t, key, face, size, activity, true, under);
    // A thread that waits on its subagents: the working ring, held still, in the second ink (design.md).
    if st == Some(TaskStatus::Waiting) {
        let (_, stroke) = crate::ui::portrait::ring_metrics(size);
        let outer = ringed(size);
        let r = outer / 2.0 - stroke / 2.0;
        let ink = crate::ui::portrait::seen_on(t.ink_muted(), t.ink(), under, crate::ui::portrait::RING_CONTRAST);
        let track = Shape::new().stroke_path(&still_circle(outer / 2.0, r), stroke, LineCap::Round, ink.alpha(if t.dark { 0.28 } else { 0.22 }));
        let sweep = Shape::new().stroke_path(&still_arc(outer / 2.0, r, 120.0), stroke, LineCap::Round, ink);
        return tile.child(el().size(outer, outer).shape(track).absolute(0.0, 0.0)).child(el().key(format!("{key}-still")).size(outer, outer).shape(sweep).absolute(0.0, 0.0));
    }
    tile
}

/// A circle as a path (four cubic arcs).
fn still_circle(c: f32, r: f32) -> rune::Path {
    let k = 0.5523 * r;
    rune::Path::new()
        .move_to([c, c - r])
        .cubic_to([c + k, c - r], [c + r, c - k], [c + r, c])
        .cubic_to([c + r, c + k], [c + k, c + r], [c, c + r])
        .cubic_to([c - k, c + r], [c - r, c + k], [c - r, c])
        .cubic_to([c - r, c - k], [c - k, c - r], [c, c - r])
        .close()
}

/// An arc of `sweep` degrees clockwise from twelve o'clock.
fn still_arc(c: f32, r: f32, sweep: f32) -> rune::Path {
    let pieces = (sweep / 60.0).ceil().max(1.0) as usize;
    let step = (sweep / pieces as f32).to_radians();
    let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
    let at = |a: f32| (c + r * a.sin(), c - r * a.cos());
    let tangent = |a: f32| (a.cos(), a.sin());
    let (x0, y0) = at(0.0);
    let mut p = rune::Path::new().move_to([x0, y0]);
    for i in 0..pieces {
        let (a, b) = (step * i as f32, step * (i + 1) as f32);
        let ((x1, y1), (x2, y2)) = (at(a), at(b));
        let ((tx1, ty1), (tx2, ty2)) = (tangent(a), tangent(b));
        p = p.cubic_to([x1 + k * tx1, y1 + k * ty1], [x2 - k * tx2, y2 - k * ty2], [x2, y2]);
    }
    p
}

fn tile(t: &Theme, key: &str, face: &str, size: f32, activity: Activity, ringed: bool, under: Color) -> El {
    let px = art(face);
    let p = Portrait::new(key, words(face), size).ground(hex(coat(face).ground)).state(activity).ringed(ringed).under(under).decorative(true);
    match &px {
        Some(px) => p.image(px),
        None => p.face(drawing(face, size)),
    }
    .build(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_face_the_daemon_knows_is_drawn_once_on_its_own_ground() {
        let ids: Vec<&str> = COATS.iter().map(|c| c.id).collect();
        assert_eq!(ids, neko_protocol::cats::FACES);
        // Told apart at 20 points: no two grounds are the same colour, and no two are near each other in hue and lightness.
        for (i, a) in COATS.iter().enumerate() {
            for b in &COATS[i + 1..] {
                let (x, y) = (hex(a.ground), hex(b.ground));
                let e = rune::color::srgb_encode;
                let d = ((e(x.r) - e(y.r)).powi(2) + (e(x.g) - e(y.g)).powi(2) + (e(x.b) - e(y.b)).powi(2)).sqrt();
                assert!(d > 0.12, "{} and {} have grounds too alike ({d:.3})", a.id, b.id);
            }
        }
        assert_eq!(coat("a-face-from-a-newer-daemon").id, "tabby", "an unknown face is still drawn");
    }

    #[test]
    fn the_ring_reads_at_small_sizes_and_takes_the_same_room_in_every_state() {
        for size in [20.0, 24.0, 28.0] {
            let (_, stroke) = crate::ui::portrait::ring_metrics(size);
            assert!(stroke >= 2.5, "the ring at {size} is {stroke}");
        }
        assert_eq!(ringed(28.0), 28.0 + 2.0 * (2.0 + 2.5));
    }
}
