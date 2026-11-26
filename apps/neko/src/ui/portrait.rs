//! rune-ui registry: portrait 0.10.0. Copied into your app by `rune-ui add portrait`; yours to change.
//!
//! One identity as a rounded tile at any size from 20 to 112 points: a picture, or a drawn fallback (the initials of its name, or an
//! element you supply, such as a drawn face), on a ground colour. Where [`Avatar`](super::avatar) is a small circle for a person in a
//! list, a Portrait is the tile an app gives its own agents, projects or contacts, and it can show what they are doing:
//!
//! * **A ring** (`ringed(true)`): while the identity is working, a faint track with a third of a circle turning round it
//!   (indeterminate; still under Reduce Motion). The ring's room is reserved whatever the state, so the tile never moves when the
//!   state changes: lay out with [`ringed`] (the outer size).
//! * **A dot** on the tile's corner for what needs a person (a meaning from the theme: warning, danger, success, info), with a ring
//!   of the surface colour round it (`under`) so it reads on any picture.
//! * **Words**: the tile is an image named `name`, described by the state in words ("Working", "Needs you"), so no state is carried by
//!   colour alone. Hide it with `decorative(true)` where the words round it already name the identity.
//!
//! The tile's corner radius is a quarter of its size (continuous when the theme says so), with a hairline (half a point under 24,
//! one above) and a two-layer shadow from 24 up. The ring and dot colours are taken from the theme (the identity colour when it sets
//! one, else info) and made at least 4.5:1 (ring) and 3:1 (dot) against `under`.
//!
//! ```ignore
//! Portrait::new("asha", "Asha", 28.0).state(Activity::Working).ringed(true).under(t.c.surface.bg).build(t)
//! Portrait::new("acme", "Acme", 76.0).image(&logo).build(t)
//! Portrait::new("lee", "Lee", 40.0).face(my_drawn_face(40.0)).ground(my_colour).build(t)
//! ```

use rune::{Color, El, LineCap, LoopSpec, Path, Pixels, Shadow, Shape, el};
use rune_ui_style::{contrast, mix, tabular, weight};

use super::base::{Shape as _, Theme};

/// What the identity is doing: the ring and the dot, and the words that say it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    /// Nothing to show (the ring's room is still kept).
    #[default]
    None,
    /// Busy: the ring turns.
    Working,
    /// Waiting for a person: a warning dot.
    NeedsYou,
    /// Could not go on: a danger dot.
    Failed,
    /// Finished well: a success dot.
    Done,
    /// Something to read: an info dot.
    Info,
}

impl Activity {
    /// The state in words, as assistive technology reads it (none for [`Activity::None`]).
    pub fn word(self) -> Option<&'static str> {
        match self {
            Activity::None => None,
            Activity::Working => Some("Working"),
            Activity::NeedsYou => Some("Needs you"),
            Activity::Failed => Some("Failed"),
            Activity::Done => Some("Done"),
            Activity::Info => Some("Has news"),
        }
    }
}

/// The smallest and largest tile.
pub const MIN: f32 = 20.0;
pub const MAX: f32 = 112.0;

fn clamped(size: f32) -> f32 {
    size.clamp(MIN, MAX)
}

/// A tile's corner radius: a quarter of its size.
pub fn radius(size: f32) -> f32 {
    clamped(size) * 0.24
}

/// The ring's gap from the tile and its stroke, at a tile size: a gap of 6% of the size (2 to 5 points), a stroke of 4% (2.5 to
/// 3.5), so the ring reads at 20 points.
pub fn ring_metrics(size: f32) -> (f32, f32) {
    let s = clamped(size);
    ((s * 0.06).clamp(2.0, 5.0).round(), (s * 0.04).clamp(2.5, 3.5))
}

/// The room a ringed tile of `size` takes, in every state: the tile and the ring's gap and stroke on each side.
pub fn ringed(size: f32) -> f32 {
    let (gap, stroke) = ring_metrics(size);
    clamped(size) + 2.0 * (gap + stroke)
}

/// The first letters of the first two words, upper-case.
pub fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

/// A circle as a path (four cubic arcs).
fn circle(c: f32, r: f32) -> Path {
    let k = 0.5523 * r;
    Path::new()
        .move_to([c, c - r])
        .cubic_to([c + k, c - r], [c + r, c - k], [c + r, c])
        .cubic_to([c + r, c + k], [c + k, c + r], [c, c + r])
        .cubic_to([c - k, c + r], [c - r, c + k], [c - r, c])
        .cubic_to([c - r, c - k], [c - k, c - r], [c, c - r])
        .close()
}

/// An arc of `sweep` degrees clockwise from twelve o'clock, in cubic pieces of at most 60 degrees.
fn arc(c: f32, r: f32, sweep: f32) -> Path {
    let pieces = (sweep / 60.0).ceil().max(1.0) as usize;
    let step = (sweep / pieces as f32).to_radians();
    let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
    let at = |a: f32| (c + r * a.sin(), c - r * a.cos());
    let tangent = |a: f32| (a.cos(), a.sin());
    let (x0, y0) = at(0.0);
    let mut p = Path::new().move_to([x0, y0]);
    for i in 0..pieces {
        let (a, b) = (step * i as f32, step * (i + 1) as f32);
        let ((x1, y1), (x2, y2)) = (at(a), at(b));
        let ((tx1, ty1), (tx2, ty2)) = (tangent(a), tangent(b));
        p = p.cubic_to([x1 + k * tx1, y1 + k * ty1], [x2 - k * tx2, y2 - k * ty2], [x2, y2]);
    }
    p
}

/// The ring's contrast against the surface it turns on, and the dot's (non-text graphics need 3:1; the ring is thin, so it takes more).
pub const RING_CONTRAST: f32 = 4.5;
pub const DOT_CONTRAST: f32 = 3.0;

/// `base` taken toward `strong` until it reaches `min`:1 against `under` (or all the way): a ring or dot that always shows.
pub fn seen_on(base: Color, strong: Color, under: Color, min: f32) -> Color { // guard: allow (a return type, not a colour)
    (0..=10).map(|i| mix(base, strong, i as f32 / 10.0)).find(|c| contrast(*c, under) >= min).unwrap_or(strong)
}

/// What the tile shows.
enum Face<'a> {
    Initials,
    Image(&'a Pixels),
    Element(El),
}

pub struct Portrait<'a> {
    key: String,
    name: String,
    size: f32,
    face: Face<'a>,
    ground: Option<Color>,
    activity: Activity,
    ringed: bool,
    under: Option<Color>,
    words: Option<String>,
    decorative: bool,
}

impl<'a> Portrait<'a> {
    /// A tile named `name` (its accessible name, and the source of its initials) at `size` points, clamped to 20 to 112.
    pub fn new(key: impl Into<String>, name: impl Into<String>, size: f32) -> Portrait<'a> {
        Portrait { key: key.into(), name: name.into(), size: clamped(size), face: Face::Initials, ground: None, activity: Activity::None, ringed: false, under: None, words: None, decorative: false }
    }

    /// A picture, clipped to the tile and given a hairline so it never bleeds into the page.
    pub fn image(mut self, p: &'a Pixels) -> Self {
        self.face = Face::Image(p);
        self
    }

    /// A drawn face in place of the initials: any element, clipped to the tile (build it at the tile's size).
    pub fn face(mut self, e: El) -> Self {
        self.face = Face::Element(e);
        self
    }

    /// The tile's ground colour behind initials or a drawn face (the theme's quiet tint by default). Initials take the ink that
    /// reads best on it.
    pub fn ground(mut self, c: Color) -> Self {
        self.ground = Some(c);
        self
    }

    /// What the identity is doing: a turning ring while working (with `ringed`), a dot on the corner for the rest.
    pub fn state(mut self, a: Activity) -> Self {
        self.activity = a;
        self
    }

    /// Reserves room for the ring ([`ringed`] gives the outer size) in every state, so the layout does not shift when the state
    /// changes; the ring shows while the state is [`Activity::Working`]. Off, the tile is exactly `size` and a working state shows
    /// no ring.
    pub fn ringed(mut self, on: bool) -> Self {
        self.ringed = on;
        self
    }

    /// The colour of the surface the tile sits on (the page by default): the ring round the dot, and what ring and dot are checked
    /// against.
    pub fn under(mut self, c: Color) -> Self {
        self.under = Some(c);
        self
    }

    /// The state in your own words ("Building", "Waiting for a review") in place of the usual one, for assistive technology.
    pub fn state_words(mut self, words: impl Into<String>) -> Self {
        self.words = Some(words.into());
        self
    }

    /// Hides the tile from assistive technology: the words round it already name the identity and its state.
    pub fn decorative(mut self, on: bool) -> Self {
        self.decorative = on;
        self
    }

    /// The tile alone: ground, face, hairline and shadow.
    fn tile(&mut self, t: &Theme) -> El {
        let size = self.size;
        let r = radius(size);
        let mut p = el().size(size, size).radius(r).round_as(t).clip_rounded(r);
        p = match std::mem::replace(&mut self.face, Face::Initials) {
            Face::Image(px) => p.pixels(px),
            face => {
                let ground = self.ground.unwrap_or(t.c.muted.bg);
                p = p.fill(ground).gradient(t.glint(0.16), t.glint(0.0));
                match face {
                    Face::Element(e) => p.child(e),
                    _ => {
                        // The ink that reads best on the ground: the first ink or the ink's own text.
                        let under = self.under.unwrap_or(t.c.page.bg);
                        let flat = rune_ui_style::over(under, ground);
                        let ink = if contrast(t.c.page.fg, flat) >= contrast(t.c.primary.fg, flat) { t.c.page.fg } else { t.c.primary.fg };
                        let style = tabular(weight(t.ty.base, t.ty.semibold));
                        let style = rune::TextStyle { size: (size * 0.4).round().max(9.0), line_height: (size * 0.5).round(), ..style };
                        p.center().child(rune::text(initials(&self.name)).font(style).color(ink).a11y_hidden())
                    }
                }
            }
        };
        let edge = if t.dark { t.glint(0.14) } else { t.shade(0.10) };
        let mut p = p.border(if size < 24.0 { 0.5 } else { 1.0 }, edge);
        if size >= 24.0 {
            p = p
                .box_shadow(Shadow { y: 1.0, blur: 2.0, color: t.shade(if t.dark { 0.45 } else { 0.10 }), ..Shadow::default() })
                .box_shadow(Shadow { y: size * 0.06, blur: size * 0.18, spread: -size * 0.04, color: t.shade(if t.dark { 0.5 } else { 0.14 }), ..Shadow::default() });
        }
        p.a11y_hidden()
    }

    pub fn build(mut self, t: &Theme) -> El {
        let size = self.size;
        let under = self.under.unwrap_or(t.c.page.bg);
        let (gap, stroke) = ring_metrics(size);
        let outer = if self.ringed { ringed(size) } else { size };
        let inset = if self.ringed { gap + stroke } else { 0.0 };
        let tile = self.tile(t).absolute(inset, inset);
        let mut b = el().key(self.key.clone()).size(outer, outer).child(tile);
        match self.activity {
            Activity::Working if self.ringed => {
                let id = t.identity();
                let c = seen_on(id.solid, id.fg, under, RING_CONTRAST);
                let r = outer / 2.0 - stroke / 2.0;
                let track = Shape::new().stroke_path(&circle(outer / 2.0, r), stroke, LineCap::Round, c.alpha(if t.dark { 0.28 } else { 0.22 }));
                let sweep = Shape::new().stroke_path(&arc(outer / 2.0, r, 120.0), stroke, LineCap::Round, c);
                let mut moving = el().key(format!("{}-arc", self.key)).size(outer, outer).shape(sweep).absolute(0.0, 0.0);
                if !t.reduced_motion {
                    moving = moving.looping(LoopSpec::rotation(2.4));
                }
                b = b.child(el().size(outer, outer).shape(track).absolute(0.0, 0.0)).child(moving);
            }
            Activity::NeedsYou | Activity::Failed | Activity::Done | Activity::Info => {
                let tone = match self.activity {
                    Activity::NeedsYou => t.c.warning,
                    Activity::Failed => t.c.danger,
                    Activity::Done => t.c.success,
                    _ => t.c.info,
                };
                let d = (size * 0.3).round().clamp(9.0, 18.0);
                let halo = if size < 32.0 { 1.5 } else { 2.0 };
                // On the tile's corner: inside the ring's room when ringed, a quarter of its width past the tile when not.
                let at = if self.ringed { outer - d - 1.0 } else { size - d * 0.75 };
                b = b.child(el().key(format!("{}-dot", self.key)).size(d, d).radius(d / 2.0).fill(seen_on(tone.solid, tone.fg, under, DOT_CONTRAST)).box_shadow(Shadow { spread: halo, color: under, ..Shadow::default() }).absolute(at, at));
            }
            _ => {}
        }
        if self.decorative {
            return b.a11y_hidden();
        }
        let b = b.role("img").label(self.name.clone());
        let words = self.words.or_else(|| self.activity.word().map(str::to_owned));
        let b = match words {
            Some(w) => b.description(w),
            None => b,
        };
        if self.activity == Activity::Working { b.busy(true) } else { b }
    }
}
