//! A cat's face: one of the six presets the daemon accepts (`neko_protocol::cats::FACE_PRESETS`), drawn as a flat vector cat with
//! one prop on a rounded-square tile (radius 26% of the side), at 16, 20, 24, 36 and 44 points.
//!
//! The drawing is the approved mockups' flat cat (`cats.svg.html`: ears, head, muzzle, eyes, nose, whiskers, 2.5 unit ink line)
//! with a coat, an eye colour and a prop per preset. Each face is an [`rune::Icon`] whose body names its own colours, so Rune
//! rasterises it in colour with resvg and draws it as an image at the size asked (`rune::text::color_icon`); nothing is read
//! from a file, and the same six bodies serve every size. A face is the same in light and dark: the tile carries its colour.
//!
//! Two marks sit on a face (the prototype's `.face .wait` and `.face .work`): an amber dot at the top right when the cat needs
//! you (from 20 points up) and a small block of dots at the bottom right when it has tasks running (from 26 points up). Both
//! are ringed in the colour the face sits on, so the mark reads on a hovered or open row too.

use std::sync::OnceLock;

use neko_protocol::cats::FACE_PRESETS;
use rune::{Color, El, Icon, Shadow, el, icon};

use crate::theme::Neko;

/// The sizes a face is drawn at (the design's one scale).
pub const SIZES: [f32; 5] = [16.0, 20.0, 24.0, 36.0, 44.0];

/// The ink of every outline.
const INK: &str = "#0e0e12";

/// What a preset wears: the coat, its shade, the muzzle, the eyes, the tile and which prop.
struct Look {
    id: &'static str,
    tile: &'static str,
    coat: &'static str,
    shade: &'static str,
    muzzle: &'static str,
    eye: &'static str,
    ink: &'static str,
    prop: Prop,
}

#[derive(Clone, Copy)]
enum Prop {
    Glasses,
    Headphones,
    Scarf,
    Bowtie,
    Welding,
}

const LOOKS: [Look; 6] = [
    Look { id: "black-glasses", tile: "#93b08b", coat: "#2b2b31", shade: "#2b2b31", muzzle: "#efe7da", eye: "#f0a030", ink: INK, prop: Prop::Glasses },
    Look { id: "persian-glasses", tile: "#a9c3a3", coat: "#efe9e0", shade: "#d9d0c2", muzzle: "#fbf7f0", eye: "#6fa3c7", ink: "#3a342d", prop: Prop::Glasses },
    Look { id: "tabby-headphones", tile: "#1f8e94", coat: "#e8923a", shade: "#b8641f", muzzle: "#f6e3c8", eye: "#7fb069", ink: "#5a2f10", prop: Prop::Headphones },
    Look { id: "grey-scarf", tile: "#b9aed8", coat: "#8d939c", shade: "#6f7580", muzzle: "#e4e6ea", eye: "#d6d36a", ink: "#2a2d33", prop: Prop::Scarf },
    Look { id: "cream-bowtie", tile: "#f0b99b", coat: "#f1dfc0", shade: "#d9c196", muzzle: "#fff6e6", eye: "#7a4b2a", ink: "#4a3520", prop: Prop::Bowtie },
    Look { id: "calico-welding", tile: "#e8c468", coat: "#f4f1ea", shade: "#e08a3c", muzzle: "#fffaf0", eye: "#4f8a5b", ink: INK, prop: Prop::Welding },
];

/// The face ids, in the picker's order: exactly the daemon's list.
pub fn ids() -> [&'static str; 6] {
    FACE_PRESETS
}

/// The face for an id; an unknown id (a newer daemon's) draws the first.
pub fn face_icon(id: &str) -> &'static Icon {
    let faces = FACES.get_or_init(|| LOOKS.iter().map(|l| Icon { name: leak(format!("neko-cat-{}", l.id)), view: 64.0, body: leak(body(l)) }).collect());
    let at = LOOKS.iter().position(|l| l.id == id).unwrap_or(0);
    &faces[at]
}

/// The tile's colour for an id (the picker's selected ring and the previews use none of it; the tests do).
pub fn tile(id: &str) -> &'static str {
    LOOKS.iter().find(|l| l.id == id).map_or(LOOKS[0].tile, |l| l.tile)
}

static FACES: OnceLock<Vec<Icon>> = OnceLock::new();

/// Six small strings that live as long as the program (an `Icon` wants `&'static`), made once.
fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn body(l: &Look) -> String {
    let Look { tile, coat, shade, muzzle, eye, ink, prop, .. } = l;
    // The tile (26% of 64 is 16.64), then the cat, centred and nearly filling it, so a prop stays inside the corners.
    let mut s = format!(r##"<rect width="64" height="64" rx="16.64" fill="{tile}"/><g transform="translate(1.6 -.4) scale(.95)">"##);
    // Ears, the pink inside them, the head and the muzzle.
    s += &format!(r##"<path d="M14 30 L12 10 L26 20 Z M50 30 L52 10 L38 20 Z" fill="{coat}" stroke="{ink}" stroke-width="2.5" stroke-linejoin="round"/>"##);
    s += r##"<path d="M16 13.5 L24 19.5 M48 13.5 L40 19.5" stroke="#f0a7b4" stroke-width="2" stroke-linecap="round" fill="none"/>"##;
    s += &format!(r##"<ellipse cx="32" cy="38" rx="21" ry="18" fill="{coat}" stroke="{ink}" stroke-width="2.5"/>"##);
    if l.id == "calico-welding" {
        // Two patches, clipped to the head.
        s += r##"<clipPath id="h"><ellipse cx="32" cy="38" rx="21" ry="18"/></clipPath><g clip-path="url(#h)"><ellipse cx="21" cy="28" rx="9" ry="8" fill="#e08a3c"/><ellipse cx="46" cy="48" rx="9" ry="7" fill="#2b2b31"/></g>"##;
        s += &format!(r##"<ellipse cx="32" cy="38" rx="21" ry="18" fill="none" stroke="{ink}" stroke-width="2.5"/>"##);
    }
    if matches!(prop, Prop::Headphones) {
        // Tabby stripes between the ears.
        s += &format!(r##"<path d="M32 21 V26 M26 22.5 L27.5 26.5 M38 22.5 L36.5 26.5" stroke="{shade}" stroke-width="2" stroke-linecap="round" fill="none"/>"##);
    }
    s += &format!(r##"<ellipse cx="32" cy="45" rx="9" ry="6.5" fill="{muzzle}"/>"##);
    // Eyes: the iris, the slit pupil, a catchlight.
    s += &format!(r##"<ellipse cx="24.5" cy="36" rx="3.4" ry="4" fill="{eye}"/><ellipse cx="39.5" cy="36" rx="3.4" ry="4" fill="{eye}"/>"##);
    s += &format!(r##"<ellipse cx="25" cy="36.6" rx="1.4" ry="2.6" fill="{ink}"/><ellipse cx="40" cy="36.6" rx="1.4" ry="2.6" fill="{ink}"/>"##);
    // Nose, mouth, whiskers.
    s += r##"<path d="M30 42.5 h4 l-2 2.2 Z" fill="#e88f9e"/>"##;
    s += &format!(r##"<path d="M32 44.7 q-2 2.6 -4.6 1.6 M32 44.7 q2 2.6 4.6 1.6" stroke="{ink}" stroke-width="1.3" fill="none" stroke-linecap="round"/>"##);
    s += r##"<path d="M8 41 h10 M8 45 l10 -2 M56 41 h-10 M56 45 l-10 -2" stroke="#efe7da" stroke-width="1" stroke-linecap="round" opacity=".7" fill="none"/>"##;
    s += &match prop {
        // Round lenses over the eyes, joined at the bridge.
        Prop::Glasses => format!(r##"<g fill="#8cc8ff" fill-opacity=".4" stroke="{INK}" stroke-width="2.4"><circle cx="24.5" cy="36" r="6.4"/><circle cx="39.5" cy="36" r="6.4"/></g><path d="M30.9 36 h2.2" stroke="{INK}" stroke-width="2.4"/>"##),
        // A band over the head and a cup on each side.
        Prop::Headphones => format!(r##"<path d="M11 36 q0 -22 21 -22 q21 0 21 22" fill="none" stroke="{INK}" stroke-width="3.2"/><rect x="6.5" y="31" width="8" height="13" rx="3.5" fill="#ef6b6b" stroke="{INK}" stroke-width="2.4"/><rect x="49.5" y="31" width="8" height="13" rx="3.5" fill="#ef6b6b" stroke="{INK}" stroke-width="2.4"/>"##),
        // A scarf round the neck with a tail.
        Prop::Scarf => format!(r##"<path d="M13 51 Q32 61 51 51 L53 58 Q32 68 11 58 Z" fill="#d9604c" stroke="{INK}" stroke-width="2.2" stroke-linejoin="round"/><path d="M42 59 l3 8 l6 -2 l-3 -8 Z" fill="#d9604c" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>"##),
        Prop::Bowtie => format!(r##"<path d="M26 57 l-7 -4.5 v9 Z M38 57 l7 -4.5 v9 Z" fill="#e65d8b" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/><circle cx="32" cy="57" r="3" fill="#e65d8b" stroke="{INK}" stroke-width="1.8"/>"##),
        // A welding helmet pushed up on the forehead, its visor lifted.
        Prop::Welding => format!(r##"<path d="M12 27 q20 -24 40 0 Z" fill="#4a505c" stroke="{INK}" stroke-width="2.5" stroke-linejoin="round"/><rect x="9" y="25" width="46" height="5" rx="2.5" fill="#f2b33d" stroke="{INK}" stroke-width="2.5"/><path d="M32 7.5 v10" stroke="{INK}" stroke-width="2"/>"##),
    };
    s += "</g>";
    s
}

/// What a face shows besides the cat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// An amber dot at the top right: something waits for the person.
    pub needs_you: bool,
    /// A block of dots at the bottom right: the cat has tasks running.
    pub working: bool,
}

/// The face at `size` points, with its marks. `ring` is the colour the face sits on (the dot and the block are ringed in it).
pub fn face(id: &str, size: f32, marks: Marks, ring: Color, t: &Neko) -> El {
    let mut stack = el().size(size, size).role("img").label(label(id)).help(label(id));
    stack = stack.child(icon(face_icon(id), size).absolute(0.0, 0.0).a11y_hidden());
    if marks.needs_you && size >= 20.0 {
        let d = (size * 0.2).clamp(8.0, 9.0);
        stack = stack.child(el().key("badge-needs").size(d, d).radius(d / 2.0).fill(t.meaning.amber).box_shadow(Shadow { spread: 2.0, color: ring, ..Shadow::default() }).absolute(size - d + 3.0, -3.0).a11y_hidden());
    }
    if marks.working && size >= 26.0 {
        let mut block = rune::row().key("badge-working").size(13.0, 13.0).radius(4.0).fill(ring).center().a11y_hidden().absolute(size - 10.0, size - 10.0);
        let mut dots = rune::col().gap(1.5);
        for r in 0..3 {
            let mut line = rune::row().gap(1.5);
            for c in 0..3 {
                // A quiet pattern: the corners and the middle are the brighter dots.
                let on = (r + c) % 2 == 0;
                line = line.child(el().size(2.0, 2.0).radius(1.0).fill(t.meaning.sky.alpha(if on { 1.0 } else { 0.45 })));
            }
            dots = dots.child(line);
        }
        block = block.child(dots);
        stack = stack.child(block);
    }
    stack
}

/// How a screen reader names a face: "tabby with headphones" for `tabby-headphones`.
pub fn label(id: &str) -> String {
    match id {
        "black-glasses" => "Black cat with glasses",
        "persian-glasses" => "Persian cat with glasses",
        "tabby-headphones" => "Tabby cat with headphones",
        "grey-scarf" => "Grey cat with a scarf",
        "cream-bowtie" => "Cream cat with a bow tie",
        "calico-welding" => "Calico cat with a welding helmet",
        _ => "Cat",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_six_looks_are_exactly_the_daemons_presets_in_order() {
        let looks: Vec<&str> = LOOKS.iter().map(|l| l.id).collect();
        assert_eq!(looks, FACE_PRESETS.to_vec());
        assert_eq!(ids(), FACE_PRESETS);
    }

    #[test]
    fn every_face_names_its_own_colours_so_rune_draws_it_in_colour() {
        for id in FACE_PRESETS {
            let icon = face_icon(id);
            assert_eq!(icon.view, 64.0);
            assert!(icon.body.contains("fill=\"#"), "{id} paints its own colours");
            assert!(!icon.body.contains("currentColor"), "{id} is the same in light and dark");
            assert!(icon.body.contains(&format!("fill=\"{}\"", tile(id))), "{id} has its tile");
            // Radius 26% of the 64 unit side.
            assert!(icon.body.contains("rx=\"16.64\""));
        }
        let names: std::collections::HashSet<&str> = FACE_PRESETS.iter().map(|id| face_icon(id).name).collect();
        assert_eq!(names.len(), 6, "each face is its own icon (Rune keys images by icon)");
    }

    #[test]
    fn an_unknown_id_draws_the_first_face_and_no_face_has_a_stray_emoji_or_gradient() {
        assert_eq!(face_icon("newer-face").name, face_icon("black-glasses").name);
        for id in FACE_PRESETS {
            let b = face_icon(id).body;
            assert!(!b.contains("Gradient"), "{id}: flat fills only");
            assert!(b.is_ascii(), "{id}: no emoji");
        }
    }

    #[test]
    fn each_face_is_a_valid_svg_resvg_can_draw() {
        for id in FACE_PRESETS {
            let icon = face_icon(id);
            let doc = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">{}</svg>"##, icon.body);
            let (rgba, (w, h)) = crate::viewers::image_view::rasterize_svg(doc.as_bytes()).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(w > 0 && h > 0 && rgba.iter().any(|b| *b != 0), "{id} draws something");
        }
    }
}
