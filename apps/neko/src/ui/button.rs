//! rune-ui registry: button 0.8.0 Copied into your app by `rune-ui add button`; yours to change.
//!
//! shadcn's six variants on the materials: **Default** (ink, a view's one primary action), **Secondary** (raised), **Outline**,
//! **Ghost**, **Destructive** and **Link**. Three sizes. Words, an icon and words, a trailing icon, or an icon alone (the label is then
//! its name and its tooltip, so an icon button always has a name). A press sinks raised, ink and danger buttons; they do not change
//! colour on hover. Loading keeps the width and shows a spinner; a disabled or loading button is not focusable and never runs.
//!
//! ```ignore
//! Button::new("save", "Save").icon(&icons::CHECK).on_press(|s: &mut App| s.save()).build(t)
//! Button::new("add", "Add item").variant(ButtonVariant::Ghost).icon_only(&icons::PLUS).build(t)
//! ```

use rune::{El, Icon, row, text};

use super::base::{ButtonVariant, Parts, Press, Shape, Size, SizeExt, State, Theme, glyph, hit_area, paint_state, press, recipes, spinner_in, states};

pub struct Button<S: 'static> {
    key: String,
    label: String,
    variant: ButtonVariant,
    size: Size,
    icon: Option<&'static Icon>,
    trailing: Option<&'static Icon>,
    icon_only: bool,
    loading: bool,
    disabled: bool,
    selected: Option<bool>,
    pinned: Option<State>,
    full: bool,
    recipe: Option<Box<dyn Fn(Parts) -> Parts>>,
    press: Option<Press<S>>,
}

impl<S: 'static> Button<S> {
    /// A Default (ink) button. `label` is its words, or its name when it shows only an icon.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Button<S> {
        Button { key: key.into(), label: label.into(), variant: ButtonVariant::Default, size: Size::Md, icon: None, trailing: None, icon_only: false, loading: false, disabled: false, selected: None, pinned: None, full: false, recipe: None, press: None }
    }

    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }

    /// An icon before the words.
    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
        self
    }

    /// An icon after the words.
    pub fn trailing_icon(mut self, i: &'static Icon) -> Self {
        self.trailing = Some(i);
        self
    }

    /// The icon alone, square; the label becomes its name and tooltip.
    pub fn icon_only(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
        self.icon_only = true;
        self
    }

    pub fn loading(mut self, on: bool) -> Self {
        self.loading = on;
        self
    }

    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// A button that stays down while on (it says so to assistive technology).
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = Some(on);
        self
    }

    /// Pins a live state (galleries, tests).
    pub fn state(mut self, s: State) -> Self {
        self.pinned = Some(s);
        self
    }

    /// As wide as what holds it.
    pub fn full(mut self) -> Self {
        self.full = true;
        self
    }

    /// Changes the parts before painting (the escape hatch).
    pub fn recipe(mut self, f: impl Fn(Parts) -> Parts + 'static) -> Self {
        self.recipe = Some(Box::new(f));
        self
    }

    pub fn on_press(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.press = Some(press(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let h = self.size.height(t);
        let mut parts = recipes::button(t, self.variant);
        if let Some(f) = &self.recipe {
            parts = f(parts);
        }
        let mut st = states(self.pinned, self.disabled, self.selected == Some(true));
        st.loading = self.loading;
        let ink = parts.label.resolve(st);
        let strong = !matches!(self.variant, ButtonVariant::Ghost | ButtonVariant::Link);
        let live = st.live();
        let mut face = row().key(self.key.clone()).role(if self.variant == ButtonVariant::Link { "link" } else { "button" }).label(self.label.clone()).focusable(live).h(h).gap(if self.size == Size::Sm { t.metrics.gap - 2.0 } else { t.metrics.gap }).center().radius(self.size.radius(t)).round_as(t);
        if let Some(v) = self.selected {
            face = face.checked(v);
        }
        if self.icon_only {
            face = face.help(self.label.clone()).w(h);
        } else {
            let pad = self.size.pad(t);
            face = face.pl(if self.icon.is_some() || self.loading { pad - 2.0 } else { pad }).pr(if self.trailing.is_some() { pad - 2.0 } else { pad });
        }
        if self.full {
            face = face.w_full();
        }
        face = paint_state(face, &parts.root, st);
        let size = if self.icon_only { t.control.icon_lg } else { self.size.icon(t) };
        if self.loading {
            face = face.busy(true).child(spinner_in(t, size, ink, &self.label));
        } else if let Some(i) = self.icon {
            face = face.child(glyph(i, size, parts.icon.resolve(st)));
        }
        if !self.icon_only {
            face = face.child(text(self.label.clone()).font(self.size.text(t, if strong { t.ty.medium } else { t.ty.regular })).color(ink).truncate().min_w(0.0).a11y_hidden());
        }
        if let Some(i) = self.trailing {
            face = face.child(glyph(i, size, parts.icon.resolve(st)));
        }
        if !live {
            face = face.disabled(true);
        }
        let press = self.press.filter(|_| live);
        if let Some(f) = press.clone() {
            face = face.on_press(move |s: &mut S| f(s));
        }
        let x = if self.icon_only { t.control.slop(h) } else { 0.0 };
        let hit = hit_area(&self.key, face, x, t.control.slop(h), press);
        if self.full { hit.w_full() } else { hit }
    }
}
