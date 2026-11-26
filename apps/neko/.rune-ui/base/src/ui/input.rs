//! rune-ui registry: input 0.5.0. Copied into your app by `rune-ui add input`; yours to change.
//!
//! One line of text on the **well** material: a tint pressed into the page with a soft shadow inside its top edge, no outline at rest.
//! Focus rings the well (the editor inside has focus, so it is the well's `focus_within` ring); invalid rings it in the danger colour.
//! A prefix (words or an icon) and a suffix sit inside; a clear button shows while there is text. [`Input::search`] is the search form.
//! Editing is Rune's `text_input` (caret, selection, IME, undo, accessibility). Labels, help and errors live in `Field`.
//!
//! ```ignore
//! Input::new("name", &s.name, |s: &mut App| &mut s.name).placeholder("Project name").build(t)
//! ```

use std::rc::Rc;

use rune::{El, Icon, TextEdit, row, text};

use super::base::{Press, Shape, Size, SizeExt, State, Theme, glyph, press, recipes, states};
use super::icons;

/// How a field reaches its text in the app's state.
pub type Access<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut TextEdit>;

enum Side {
    Words(String),
    Glyph(&'static Icon),
}

pub struct Input<'a, S: 'static> {
    key: String,
    edit: &'a TextEdit,
    access: Access<S>,
    name: String,
    placeholder: String,
    description: Option<String>,
    prefix: Option<Side>,
    suffix: Option<Side>,
    clear: bool,
    secure: bool,
    size: Size,
    width: Option<f32>,
    invalid: bool,
    disabled: bool,
    pinned: Option<State>,
    submit: Option<Press<S>>,
}

impl<'a, S: 'static> Input<'a, S> {
    pub fn new(key: impl Into<String>, edit: &'a TextEdit, access: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static) -> Input<'a, S> {
        let key = key.into();
        Input { name: key.clone(), key, edit, access: Rc::new(access), placeholder: String::new(), description: None, prefix: None, suffix: None, clear: false, secure: false, size: Size::Md, width: None, invalid: false, disabled: false, pinned: None, submit: None }
    }

    /// The search form: a magnifier, "Search", and a clear button once there is text.
    pub fn search(key: impl Into<String>, edit: &'a TextEdit, access: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static) -> Input<'a, S> {
        let mut i = Input::new(key, edit, access);
        i.prefix = Some(Side::Glyph(&icons::SEARCH));
        i.clear = true;
        i.name = "Search".to_owned();
        i.placeholder = "Search".to_owned();
        i
    }

    /// Its name for assistive technology (a `Field` around it gives the visible label).
    pub fn name(mut self, n: impl Into<String>) -> Self {
        self.name = n.into();
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// What assistive technology reads after the name (the help or the error `Field` shows).
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    pub fn prefix(mut self, words: impl Into<String>) -> Self {
        self.prefix = Some(Side::Words(words.into()));
        self
    }

    pub fn prefix_icon(mut self, i: &'static Icon) -> Self {
        self.prefix = Some(Side::Glyph(i));
        self
    }

    /// Words inside the well after the text ("per day", or a shortcut while it is empty).
    pub fn suffix(mut self, words: impl Into<String>) -> Self {
        self.suffix = Some(Side::Words(words.into()));
        self
    }

    pub fn clearable(mut self) -> Self {
        self.clear = true;
        self
    }

    /// A password: dots instead of the text.
    pub fn secure(mut self) -> Self {
        self.secure = true;
        self
    }

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
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

    /// Return runs this.
    pub fn on_submit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.submit = Some(press(f));
        self
    }

    fn side(&self, t: &Theme, key: String, side: &Side, ink: rune::Color) -> El {
        match side {
            Side::Words(w) => text(w.clone()).key(key).font(self.size.text(t, t.ty.regular)).color(ink).a11y_hidden(),
            Side::Glyph(i) => glyph(i, self.size.icon(t), ink),
        }
    }

    pub fn build(self, t: &Theme) -> El {
        let parts = recipes::field(t);
        let mut st = states(self.pinned, self.disabled, false);
        st.invalid = self.invalid;
        let mut well = row().key(self.key.clone()).role("group").h(self.size.height(t)).gap(t.metrics.gap).px(self.size.pad(t) - 2.0).center_y().radius(self.size.radius(t)).round_as(t);
        // The well shows focus while its editor has it: `focus_within`, not the well's own focus ring.
        well = rune_ui_style::paint(well, &rune_ui_style::resolve(&parts.root, st));
        if st.live() && !st.pinned() && !self.invalid
            && let Some((w, o, c)) = parts.root.live().focus_ring
        {
            well = well.focus_within_ring(w, o, c);
        }
        well = match self.width {
            Some(w) => well.w(w),
            None => well.w_full(),
        };
        let side_ink = parts.icon.resolve(st);
        if let Some(p) = &self.prefix {
            well = well.child(self.side(t, format!("{}-prefix", self.key), p, side_ink));
        }
        let access = self.access.clone();
        let mut input = rune::text_input(self.edit, self.size.text(t, t.ty.regular))
            .key(format!("{}-input", self.key))
            .single_line()
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .color(parts.label.resolve(st))
            .placeholder(self.placeholder.clone(), t.ink_faint())
            .caret(t.ink(), t.c.accent.fg.alpha(0.28))
            .label(self.name.clone())
            .invalid(self.invalid)
            .on_edit(rune::edit_access(move |s: &mut S| access(s)));
        if self.secure {
            input = input.secure();
        }
        if let Some(d) = &self.description {
            input = input.description(d.clone());
        }
        if let Some(f) = self.submit.clone() {
            input = input.on_submit(move |s: &mut S, _| f(s));
        }
        if self.disabled {
            input = input.disabled(true).read_only(true).focusable(false);
        }
        well = well.child(input);
        if self.clear && !self.edit.text.is_empty() && !self.disabled {
            let access = self.access.clone();
            let x = row().key(format!("{}-clear", self.key)).role("button").label("Clear").help("Clear").focusable(true).size(18.0, 18.0).center().radius(9.0).hover(t.c.hover).focus_ring(t.control.ring, t.control.ring_offset, t.c.ring).on_press(move |s: &mut S| access(s).clear()).child(glyph(&icons::CLOSE, t.control.icon_sm, side_ink));
            well = well.child(x);
        } else if let Some(sfx) = &self.suffix {
            well = well.child(self.side(t, format!("{}-suffix", self.key), sfx, side_ink));
        }
        well
    }
}
