//! rune-ui registry: agent 0.9.0 (base). What the agent set shares: a step's [`Status`] and its mark, the quiet [`FoldLine`] every
//! tool call, reasoning and step list folds behind, a small icon button that can reach the clipboard, a chip face, the slab code and
//! output sit on, monospaced type, and the words for durations, token counts and money. `rune-ui add` copies it with any agent item.
//!
//! Colour here is state only: running is a turning ring in the second ink, done is the success ink, an error the danger ink, and
//! "needs you" the warning ink. Everything else is the page's own inks.

use std::rc::Rc;

use rune::{Color, El, Family, Icon, TextStyle, Tx, col, row, text};
use rune_ui_style::{tabular, weight};

use super::base::{Press, Shape, State, Theme, Tone, glyph, hit_area, paint, paint_state, spinner_in, states};
use super::icons;

/// What a press does when it also needs the frame's transaction (to copy, to scroll, to move focus).
pub type PressTx<S> = Rc<dyn Fn(&mut S, &mut Tx)>;

/// Where a step, a tool call or a run stands.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Status {
    /// Not started.
    #[default]
    Pending,
    Running,
    Done,
    Error,
    /// Passed over on purpose.
    Skipped,
    /// Waiting for a person (a permission, an answer).
    Waiting,
    /// A person (or a rule) said no. Neutral, not a failure: nothing went wrong.
    Denied,
}

impl Status {
    pub const ALL: [Status; 7] = [Status::Pending, Status::Running, Status::Done, Status::Error, Status::Skipped, Status::Waiting, Status::Denied];

    pub fn word(self) -> &'static str {
        match self {
            Status::Pending => "Pending",
            Status::Running => "Running",
            Status::Done => "Done",
            Status::Error => "Failed",
            Status::Skipped => "Skipped",
            Status::Waiting => "Needs you",
            Status::Denied => "Denied",
        }
    }

    /// The meaning a pill shows it in.
    pub fn tone(self) -> Tone {
        match self {
            Status::Running => Tone::Info,
            Status::Done => Tone::Success,
            Status::Error => Tone::Danger,
            Status::Waiting => Tone::Warning,
            _ => Tone::Neutral,
        }
    }

    /// The ink its mark is drawn in.
    pub fn ink(self, t: &Theme) -> Color { // guard: allow (a return type, not a colour)
        match self {
            Status::Pending | Status::Skipped | Status::Denied => t.ink_faint(),
            Status::Running => t.ink_muted(),
            Status::Done => t.c.success.fg,
            Status::Error => t.c.danger.fg,
            Status::Waiting => t.c.warning.fg,
        }
    }
}

/// A status as one small mark: a hollow ring (pending), a turning ring (running, the one thing that loops), a check, a cross, a
/// slashed ring (skipped), a warning (needs you), a shield (denied: a permission said no).
pub fn mark(t: &Theme, s: Status, size: f32) -> El {
    let icon: &'static Icon = match s {
        Status::Running => return spinner_in(t, size, s.ink(t), s.word()),
        Status::Pending => &icons::CIRCLE,
        Status::Done => &icons::CIRCLE_CHECK,
        Status::Error => &icons::CIRCLE_X,
        Status::Skipped => &icons::CIRCLE_SLASH,
        Status::Waiting => &icons::WARNING,
        Status::Denied => &icons::SHIELD,
    };
    glyph(icon, size, s.ink(t))
}

/// A duration worth showing: none under one second (a step that quick needs no figure), else [`elapsed`].
pub fn took(secs: f64) -> Option<String> {
    (secs >= 1.0).then(|| elapsed(secs))
}

/// A duration in words that stay short: "0.4s", "12s", "2m 14s", "1h 3m".
pub fn elapsed(secs: f64) -> String {
    let secs = secs.max(0.0);
    if secs < 10.0 && secs.fract() != 0.0 {
        return format!("{secs:.1}s");
    }
    let s = secs.round() as u64;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m {}s", s / 60, s % 60),
        _ => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

/// A count of tokens: "840", "12.4k", "1.2M".
pub fn tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1_000.0).replace(".0k", "k"),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M"),
    }
}

/// Money in dollars: three places under a dime ("$0.042"), two above ("$1.24").
pub fn money(usd: f64) -> String {
    if usd > 0.0 && usd < 0.1 { format!("${usd:.3}") } else { format!("${usd:.2}") }
}

/// `style` in a monospaced face: the theme's own font when it is monospaced, the platform's otherwise, a little smaller so it sits
/// level with the words around it.
pub fn mono(t: &Theme, style: TextStyle) -> TextStyle {
    let family = if t.font.mono { t.font.family } else { Family::SystemMono };
    TextStyle { family, size: (style.size * 0.93 * 2.0).round() / 2.0, ..style }
}

/// Quiet words: the second ink at the hint size, tabular so figures that change keep their place.
pub fn meta(t: &Theme, words: impl Into<String>) -> El {
    text(words.into()).font(tabular(t.ty.sm)).color(t.ink_faint())
}

/// The slab code, output and diffs sit on: one step off the page, no outline.
pub fn slab(t: &Theme, key: impl Into<String>) -> El {
    paint(col().key(key.into()).w_full().radius(t.radius.lg).round_as(t).clip_rounded(t.radius.lg), &t.mat.slab.base)
}

/// A chip's face (attachments, suggestions, citations): the chip material, a pill.
pub fn chip(t: &Theme, key: impl Into<String>, pinned: Option<State>) -> El {
    let face = row().key(key.into()).h(t.control.sm).px(t.space.at(2.5)).gap(t.space.at(1.5)).center_y().radius(t.control.sm / 2.0);
    paint_state(face, &t.mat.chip, states(pinned, false, false))
}

/// A small ghost button with one icon: its name is its tooltip and what assistive technology reads. `press` gets the transaction, so it
/// can copy to the clipboard. `on` makes it a toggle that says whether it is down. A 40 point hit area round it.
pub fn icon_button<S: 'static>(t: &Theme, key: &str, name: &str, icon: &'static Icon, on: Option<bool>, disabled: bool, press: Option<PressTx<S>>) -> El {
    let h = t.control.sm;
    let st = states(None, disabled, on == Some(true));
    let ink = if on == Some(true) { t.ink() } else { t.ink_muted() };
    let mut face = row().key(key.to_owned()).role("button").label(name.to_owned()).help(name.to_owned()).focusable(!disabled).size(h, h).center().radius(rune_ui_style::Radius::fit(t.radius.md, h)).round_as(t);
    if let Some(v) = on {
        face = face.checked(v);
    }
    face = paint_state(face, &t.mat.ghost, st).child(glyph(icon, t.control.icon, ink));
    if disabled {
        face = face.disabled(true);
    }
    let press = press.filter(|_| !disabled);
    if let Some(f) = press.clone() {
        face = face.on_press_tx(move |s: &mut S, tx: &mut Tx| f(s, tx));
    }
    let slop = t.control.slop(h);
    let hit = hit_area::<S>(key, face, slop, slop, None);
    match press {
        Some(f) if slop > 0.0 => hit.on_press_tx(move |s: &mut S, tx: &mut Tx| f(s, tx)),
        _ => hit,
    }
}

/// One quiet line that folds: a mark, the words in the second ink, quiet figures, a chevron that turns. Pressing it, or Return or Space
/// while it has focus, folds it open or shut; it says `expanded` to assistive technology and keeps focus. It never looks louder than
/// the answer: no surface until hovered, the second ink, the body indented under its words.
pub struct FoldLine<S: 'static> {
    key: String,
    label: String,
    words: Option<El>,
    lead: Option<El>,
    meta: Vec<String>,
    open: bool,
    disabled: bool,
    indent: bool,
    middle: bool,
    pinned: Option<State>,
    toggle: Option<Press<S>>,
}

impl<S: 'static> FoldLine<S> {
    /// `label` is the line's words and its name.
    pub fn new(key: impl Into<String>, label: impl Into<String>, open: bool) -> FoldLine<S> {
        FoldLine { key: key.into(), label: label.into(), words: None, lead: None, meta: Vec::new(), open, disabled: false, indent: true, middle: false, pinned: None, toggle: None }
    }

    /// Draws the words as this element (a shimmer) instead of plain text; the label still names the line.
    pub fn words(mut self, e: El) -> Self {
        self.words = Some(e);
        self
    }

    /// The mark before the words (a status mark, an icon).
    pub fn lead(mut self, e: El) -> Self {
        self.lead = Some(e);
        self
    }

    /// Quiet figures after the words ("1.2s", "+12 −3").
    pub fn meta(mut self, m: impl Into<String>) -> Self {
        self.meta.push(m.into());
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Whether the open body sits indented under the line's words (true, the default) or on the line's own left edge (false): steps
    /// inside "Worked for 2m" then share the page's left edge.
    pub fn indent(mut self, on: bool) -> Self {
        self.indent = on;
        self
    }

    /// Cuts long words in the middle instead of at the end, keeping both ends (a command, a path).
    pub fn truncate_middle(mut self, on: bool) -> Self {
        self.middle = on;
        self
    }

    pub fn state(mut self, s: State) -> Self {
        self.pinned = Some(s);
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle = Some(Rc::new(f));
        self
    }

    /// The line, and `body` under it while open.
    pub fn build(self, t: &Theme, body: Vec<El>) -> El {
        let h = t.control.md;
        let st = states(self.pinned, self.disabled, false);
        let trigger_key = format!("{}-trigger", self.key);
        let mut face = row().key(trigger_key.clone()).role("button").label(self.label.clone()).expanded(self.open).focusable(!self.disabled).h(h).pl(t.space.at(1.5)).pr(t.space.at(2.0)).gap(t.metrics.gap).center_y().radius(rune_ui_style::Radius::fit(t.radius.md, h)).round_as(t);
        face = paint_state(face, &t.mat.row, st);
        let icon = t.control.icon;
        face = face.child(row().size(icon, icon).center().child(self.lead.unwrap_or_else(|| glyph(&icons::WRENCH, icon, t.ink_faint()))));
        let middle = self.middle;
        face = face.child(self.words.unwrap_or_else(|| {
            let w = text(self.label.clone()).font(t.ty.base).color(t.ink_muted()).min_w(0.0).shrink();
            if middle { w.truncate_middle() } else { w.truncate() }
        }).a11y_hidden());
        for m in self.meta {
            face = face.child(meta(t, m).a11y_hidden());
        }
        face = face.child(glyph(if self.open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, t.control.icon_sm, t.ink_faint()));
        if self.disabled {
            face = face.disabled(true);
        }
        let press = self.toggle.filter(|_| !self.disabled);
        if let Some(f) = press.clone() {
            let g = f.clone();
            face = face.on_press(move |s: &mut S| f(s)).on_key(move |s: &mut S, k: &rune::KeyEvent, _: &mut Tx| return_presses(k, || g(s)));
        }
        let line = row().w_full().min_w(0.0).child(hit_area(&trigger_key, face.min_w(0.0).shrink(), 0.0, t.control.slop(h), press).min_w(0.0).shrink());
        let mut c = col().key(self.key.clone()).w_full().gap(t.space.at(1.0)).child(line);
        if self.open && !body.is_empty() {
            let indent = if self.indent { t.space.at(1.5) + icon + t.metrics.gap } else { 0.0 };
            c = c.child(col().key(format!("{}-content", self.key)).w_full().pl(indent).pb(t.space.at(1.0)).gap(t.space.at(2.0)).children(body));
        }
        c
    }
}

/// Return (no modifiers) runs `f`. Space presses a focused control on its own; Return, on macOS, goes to the window's default button
/// unless the control takes it, so a fold line or an answer takes it here.
pub fn return_presses(k: &rune::KeyEvent, f: impl FnOnce()) -> bool {
    if k.key == rune::Key::Enter && k.mods == rune::Mods::default() {
        f();
        true
    } else {
        false
    }
}

/// A small heading over part of a fold's body ("Input", "Output").
pub fn caption(t: &Theme, words: &str) -> El {
    text(words.to_owned()).font(weight(t.ty.xs, t.ty.medium)).color(t.ink_faint())
}

/// Preformatted text on the slab: monospaced, one line per line, scrolling sideways instead of wrapping, at most `max_h` tall (then it
/// scrolls; 0 for no limit). Tool input and output, a command's output.
pub fn pre(t: &Theme, key: &str, body: &str, ink: Color, max_h: f32) -> El {
    let style = mono(t, t.ty.sm);
    let mut lines = col().py(t.space.at(2.5)).px(t.space.at(3.0)).gap(0.0);
    for l in body.lines() {
        lines = lines.child(text(if l.is_empty() { " ".to_owned() } else { l.to_owned() }).font(style).color(ink));
    }
    let mut s = slab(t, key).role("code").label(body.lines().next().unwrap_or_default().to_owned()).child(col().key(format!("{key}-x")).w_full().scroll_x().child(lines));
    if max_h > 0.0 {
        s = s.max_h(max_h).scroll(rune::ScrollSpec { stick_bottom: false });
    }
    s
}
