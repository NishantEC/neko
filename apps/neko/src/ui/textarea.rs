//! rune-ui registry: textarea 0.5.0. Copied into your app by `rune-ui add textarea`; yours to change.
//!
//! Several lines on the **well** material, `rows` tall at first and growing with the text to `max_rows`. Focus rings the well; invalid
//! rings it in the danger colour. Editing is Rune's `text_input`.
//!
//! ```ignore
//! Textarea::new("notes", &s.notes, |s: &mut App| &mut s.notes).placeholder("Notes").rows(3, 8).build(t)
//! ```

use std::rc::Rc;

use rune::{El, TextEdit, col};

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
}

impl<'a, S: 'static> Textarea<'a, S> {
    pub fn new(key: impl Into<String>, edit: &'a TextEdit, access: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static) -> Textarea<'a, S> {
        let key = key.into();
        Textarea { name: key.clone(), key, edit, access: Rc::new(access), placeholder: String::new(), rows: (3, 10), width: None, invalid: false, disabled: false, pinned: None }
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
        let mut st = states(self.pinned, self.disabled, false);
        st.invalid = self.invalid;
        let mut well = col().key(self.key.clone()).role("group").px(t.metrics.pad_md - 2.0).py(t.metrics.gap).radius(t.radius.lg).round_as(t);
        well = paint(well, &resolve(&parts.root, st));
        if st.live() && !st.pinned() && !self.invalid
            && let Some((w, o, c)) = parts.root.live().focus_ring
        {
            well = well.focus_within_ring(w, o, c);
        }
        well = match self.width {
            Some(w) => well.w(w),
            None => well.w_full(),
        };
        let access = self.access.clone();
        let mut input = rune::text_input(self.edit, t.ty.base)
            .key(format!("{}-input", self.key))
            .w_full()
            .color(parts.label.resolve(st))
            .placeholder(self.placeholder.clone(), t.ink_faint())
            .caret(t.ink(), t.c.accent.fg.alpha(0.28))
            .label(self.name.clone())
            .invalid(self.invalid)
            .on_edit(rune::edit_access(move |s: &mut S| access(s)));
        if self.disabled {
            input = input.disabled(true).read_only(true).focusable(false);
        }
        well.child(rune::auto_grow(input, self.rows.0, self.rows.1))
    }
}
