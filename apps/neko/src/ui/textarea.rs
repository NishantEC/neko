//! rune-ui registry: textarea 0.11.0 Copied into your app by `rune-ui add textarea`; yours to change.
//!
//! Several lines on the **well** material, `rows` tall at first and growing with the text to `max_rows`. Focus rings the well; invalid
//! rings it in the danger colour. Editing is Rune's `text_input`.
//!
//! **A limit and a counter** (optional): `limit(500)` shows "123 / 500" at the well's foot in tabular figures, in the warning ink
//! from 90% and the danger ink over the limit (the well is then invalid, and the text box says "N characters over"). Characters, not
//! bytes, are counted. The limit is soft, so a pasted text is never silently cut; `hard_limit(true)` makes it firm: what would pass
//! the limit is taken out again as it is typed or pasted.
//!
//! **Mono** (`mono(true)`): the text in a monospaced face (the theme's own when it is monospaced, the system's otherwise), for
//! code, logs and prompts that are read as code.
//!
//! ```ignore
//! Textarea::new("notes", &s.notes, |s: &mut App| &mut s.notes).placeholder("Notes").rows(3, 8).build(t)
//! ```

use std::rc::Rc;

use rune::{El, Family, TextEdit, TextStyle, col, row, text};
use rune_ui_style::tabular;

use super::base::{Shape, State, Theme, paint, recipes, resolve, states};

pub struct Textarea<'a, S: 'static> {
    key: String,
    edit: &'a TextEdit,
    access: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    name: String,
    placeholder: String,
    rows: (usize, usize),
    width: Option<f32>,
    invalid: bool,
    disabled: bool,
    pinned: Option<State>,
    limit: Option<usize>,
    hard: bool,
    mono: bool,
}

impl<'a, S: 'static> Textarea<'a, S> {
    pub fn new(key: impl Into<String>, edit: &'a TextEdit, access: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static) -> Textarea<'a, S> {
        let key = key.into();
        Textarea { name: key.clone(), key, edit, access: Rc::new(access), placeholder: String::new(), rows: (3, 10), width: None, invalid: false, disabled: false, pinned: None, limit: None, hard: false, mono: false }
    }

    pub fn name(mut self, n: impl Into<String>) -> Self {
        self.name = n.into();
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// Rows at first, and the most it grows to.
    pub fn rows(mut self, min: usize, max: usize) -> Self {
        self.rows = (min.max(1), max.max(min.max(1)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// At most this many characters, with a counter at the foot ("12 / 500").
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// With a limit: keep to it. What would go past it is taken out as it is typed or pasted (the excess just before the caret).
    pub fn hard_limit(mut self, on: bool) -> Self {
        self.hard = on;
        self
    }

    /// The text in a monospaced face, a little smaller so it sits level with the words around it.
    pub fn mono(mut self, on: bool) -> Self {
        self.mono = on;
        self
    }

    pub fn invalid(mut self, on: bool) -> Self {
        self.invalid = on;
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    pub fn state(mut self, s: State) -> Self {
        self.pinned = Some(s);
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let parts = recipes::field(t);
        let count = self.edit.text.chars().count();
        let over = self.limit.map_or(0, |l| count.saturating_sub(l));
        let invalid = self.invalid || over > 0;
        let mut st = states(self.pinned, self.disabled, false);
        st.invalid = invalid;
        let mut well = col().key(self.key.clone()).role("group").px(t.metrics.pad_md - 2.0).py(t.metrics.gap).radius(t.radius.lg).round_as(t);
        well = paint(well, &resolve(&parts.root, st));
        if st.live() && !st.pinned() && !invalid
            && let Some((w, o, c)) = parts.root.live().focus_ring
        {
            well = well.focus_within_ring(w, o, c);
        }
        well = match self.width {
            Some(w) => well.w(w),
            None => well.w_full(),
        };
        let access = self.access.clone();
        let style = if self.mono { mono_style(t, t.ty.base) } else { t.ty.base };
        let mut input = rune::text_input(self.edit, style)
            .key(format!("{}-input", self.key))
            .w_full()
            .color(parts.label.resolve(st))
            .placeholder(self.placeholder.clone(), t.ink_faint())
            .caret(t.ink(), t.c.accent.fg.alpha(0.28))
            .label(self.name.clone())
            .invalid(invalid)
            .on_edit(rune::edit_access(move |s: &mut S| access(s)));
        if let Some(l) = self.limit {
            input = input.description(if over > 0 { format!("{over} characters over the limit of {l}") } else { format!("{count} of {l} characters") });
            if self.hard {
                let trim = self.access.clone();
                input = input.on_input_change(move |s: &mut S, _: &mut rune::Tx| trim_to(trim(s), l));
            }
        }
        if self.disabled {
            input = input.disabled(true).read_only(true).focusable(false);
        }
        let well = well.child(rune::auto_grow(input, self.rows.0, self.rows.1));
        match self.limit {
            Some(l) => {
                let ink = if over > 0 { t.c.danger.fg } else if count * 10 >= l * 9 { t.c.warning.fg } else { t.ink_faint() };
                well.child(row().key(format!("{}-count-row", self.key)).w_full().justify_end().child(text(format!("{count} / {l}")).key(format!("{}-count", self.key)).font(tabular(t.ty.sm)).color(ink).a11y_hidden()))
            }
            None => well,
        }
    }
}

/// `style` in a monospaced face: the theme's own font when it is monospaced, the platform's otherwise.
fn mono_style(t: &Theme, style: TextStyle) -> TextStyle {
    let family = if t.font.mono { t.font.family } else { Family::SystemMono };
    TextStyle { family, size: (style.size * 0.93 * 2.0).round() / 2.0, ..style }
}

/// Takes what is past `limit` characters out of `e`: the excess just before the caret first (what was typed or pasted), then from
/// the end if that was not enough.
fn trim_to(e: &mut TextEdit, limit: usize) {
    let n = e.text.chars().count();
    if n <= limit {
        return;
    }
    let caret = e.range().1.min(e.text.len());
    let before: Vec<usize> = e.text[..caret].char_indices().map(|(i, _)| i).collect();
    let k = (n - limit).min(before.len());
    if k > 0 {
        e.set_selection(before[before.len() - k], caret);
        e.insert("");
    }
    if e.text.chars().count() > limit {
        let at = e.text.char_indices().nth(limit).map_or(e.text.len(), |(i, _)| i);
        let end = e.text.len();
        e.set_selection(at, end);
        e.insert("");
    }
}
