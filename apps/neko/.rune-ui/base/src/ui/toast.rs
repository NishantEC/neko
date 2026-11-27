//! rune-ui registry: toast 0.11.0. Copied into your app by `rune-ui add toast`; yours to change.
//!
//! A short message that comes and goes: the **floating** material, an optional meaning icon, a title and a line under it, one action
//! ("Undo") and a close button. It is a polite live region, so assistive technology reads each new toast without moving focus.
//!
//! [`Toast`] is one message. [`Toaster`] stacks messages you built yourself (bottom trailing or bottom centre, newest last) in the
//! overlay layer, with timing left to you. For the usual case use the **host**:
//!
//! * [`ToastHost`] is plain data the app owns: `show` a [`ToastSpec`] (title, line, tone, one action named by a tag, a timeout in
//!   seconds; `sticky()` for none), `dismiss` one, and `poll` it from the app's poll hook, which removes what ran out and asks for the
//!   next wake. While the pointer is over the stack or focus is inside it, the timers stop and carry on from where they were.
//! * [`Toasts`] draws the host: `Toasts::new("toasts", &s.toasts, |s| &mut s.toasts).on_action(|s, tag| ..).mount(t, &mut layer)`.
//!   Pressing the action dismisses the toast and tells you its tag; Escape on a focused toast dismisses it. Which key reaches the
//!   newest toast is yours: bind it and send focus to [`ToastHost::focus_target`].
//!
//! ```ignore
//! // the app's poll hook:  fn poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool { s.toasts.poll(clock, tx) }
//! s.toasts.show(ToastSpec::new("Thread settled").action("Undo", "undo-settle"), tx);
//! Toasts::new("toasts", &s.toasts, |s: &mut App| &mut s.toasts).position(Position::BottomCenter).on_action(|s: &mut App, tag: &str| s.undo(tag)).mount(t, &mut layer)
//! ```

use std::rc::Rc;

use rune::{Edge, El, Key, KeyEvent, Shadow, Spring, SpringPair, Transition, Tx, col, row, text};
use rune_ui_style::{ButtonVariant, Recipe, Size, weight};

use super::base::{Pick, Press, Shape, Theme, Tone, glyph, paint};
use super::button::Button;
use super::icons;

pub struct Toast<S: 'static> {
    key: String,
    title: String,
    description: Option<String>,
    tone: Option<Tone>,
    action: Option<(String, Press<S>)>,
    close: Option<Press<S>>,
}

impl<S: 'static> Toast<S> {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Toast<S> {
        Toast { key: key.into(), title: title.into(), description: None, tone: None, action: None, close: None }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    /// A meaning's icon before the words.
    pub fn tone(mut self, t: Tone) -> Self {
        self.tone = Some(t);
        self
    }

    pub fn action(mut self, words: impl Into<String>, f: impl Fn(&mut S) + 'static) -> Self {
        self.action = Some((words.into(), Rc::new(f)));
        self
    }

    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let mut r = paint(row().key(self.key.clone()).role("status").label(self.title.clone()).w(360.0).padding(t.metrics.pad_md).gap(t.metrics.pad_md - 4.0).radius(t.radius.lg).round_as(t), &Recipe::new().fill(t.c.floating.bg).shadows(&t.elevation.md).shadow(Shadow { spread: 1.0, color: t.c.line, ..Shadow::default() })).live_region(rune::LiveRegion::Polite);
        // One line of words sits level with its buttons; with a second line the icon, words and buttons start at the top.
        r = if self.description.is_some() { r.items_start() } else { r.center_y() };
        if let Some(tone) = self.tone {
            let icon = match tone {
                Tone::Success => &icons::CIRCLE_CHECK,
                Tone::Warning => &icons::WARNING,
                Tone::Danger => &icons::CIRCLE_X,
                _ => &icons::INFO,
            };
            r = r.child(glyph(icon, t.control.icon_lg, tone.colors(t).2).mt(if self.description.is_some() { 1.0 } else { 0.0 }));
        }
        let mut words = col().grow(1.0).shrink().min_w(0.0).gap(t.space.at(0.5)).child(text(self.title).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).wrap().min_w(0.0));
        if let Some(d) = self.description {
            words = words.child(text(d).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
        }
        r = r.child(words);
        if let Some((w, f)) = self.action {
            r = r.child(Button::<S>::new(format!("{}-action", self.key), w).variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut S| f(s)).build(t));
        }
        if let Some(f) = self.close {
            let g = f.clone();
            r = r.child(Button::<S>::new(format!("{}-close", self.key), "Dismiss").variant(ButtonVariant::Ghost).size(Size::Sm).icon_only(&icons::CLOSE).on_press(move |s: &mut S| f(s)).build(t));
            // Escape on the toast, or on its action or close button, dismisses it.
            r = r.on_key(move |s: &mut S, ev: &KeyEvent, _: &mut Tx| {
                if ev.key == Key::Escape && !ev.mods.cmd && !ev.mods.alt {
                    g(s);
                    true
                } else {
                    false
                }
            });
        }
        r
    }
}

/// Where the stack sits in the window.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    /// The bottom right (the bottom left in a right-to-left window): the default.
    #[default]
    BottomTrailing,
    /// Centred along the bottom edge.
    BottomCenter,
}

/// The stack's own element and the full-window layer that places it. `hook` puts handlers on the stack (not on the layer, which
/// covers the window).
fn stack_layer(t: &Theme, key: String, position: Position, toasts: Vec<El>, hook: impl FnOnce(El) -> El) -> El {
    let stack = hook(col().key(format!("{key}-stack")).gap(t.space.at(2.0)).items_end().children(toasts));
    let layer = col().key(key).role("region").label("Notifications").absolute(0.0, 0.0).w_full().h_full().padding(t.space.at(4.0)).justify_end();
    match position {
        Position::BottomTrailing => layer.items_end().child(stack),
        Position::BottomCenter => layer.items_center().child(stack),
    }
}

/// Where toasts stack: the bottom of the window, newest last.
pub struct Toaster {
    key: String,
    toasts: Vec<El>,
    position: Position,
}

impl Toaster {
    pub fn new(key: impl Into<String>) -> Toaster {
        Toaster { key: key.into(), toasts: Vec::new(), position: Position::BottomTrailing }
    }

    pub fn toast(mut self, t: El) -> Self {
        self.toasts.push(t);
        self
    }

    /// Bottom trailing (the default) or bottom centre.
    pub fn position(mut self, p: Position) -> Self {
        self.position = p;
        self
    }

    /// Mounts the stack into the overlay layer (nothing while it is empty).
    pub fn mount(self, t: &Theme, layer: &mut Vec<El>) {
        if self.toasts.is_empty() {
            return;
        }
        layer.push(stack_layer(t, self.key, self.position, self.toasts, |s| s));
    }
}

/// One message for a [`ToastHost`].
#[derive(Clone, Debug)]
pub struct ToastSpec {
    pub title: String,
    pub description: Option<String>,
    pub tone: Option<Tone>,
    /// The action's words and the tag the app hears when it is pressed.
    pub action: Option<(String, String)>,
    /// Seconds on screen; 0 never goes by itself.
    pub timeout: f64,
}

impl ToastSpec {
    /// A message that stays 5 seconds (8 with an action, so there is time to reach it).
    pub fn new(title: impl Into<String>) -> ToastSpec {
        ToastSpec { title: title.into(), description: None, tone: None, action: None, timeout: 5.0 }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    pub fn tone(mut self, t: Tone) -> Self {
        self.tone = Some(t);
        self
    }

    /// One action: its words ("Undo") and the tag `Toasts::on_action` is given. Gives the toast 8 seconds unless set after.
    pub fn action(mut self, words: impl Into<String>, tag: impl Into<String>) -> Self {
        self.action = Some((words.into(), tag.into()));
        if self.timeout == 5.0 {
            self.timeout = 8.0;
        }
        self
    }

    pub fn timeout(mut self, seconds: f64) -> Self {
        self.timeout = seconds.max(0.0);
        self
    }

    /// Stays until dismissed.
    pub fn sticky(mut self) -> Self {
        self.timeout = 0.0;
        self
    }
}

#[derive(Clone, Debug)]
struct Item {
    id: u64,
    spec: ToastSpec,
    /// Seconds still to run (what is left when paused or not started).
    left: f64,
    /// The clock time it goes at, while running.
    due: Option<f64>,
}

/// The toasts showing and their timers: plain data the app keeps in its state. Nothing here draws; [`Toasts`] does.
#[derive(Clone, Debug, Default)]
pub struct ToastHost {
    items: Vec<Item>,
    next: u64,
    hovered: bool,
    focused: bool,
}

impl ToastHost {
    pub fn new() -> ToastHost {
        ToastHost::default()
    }

    /// Adds a toast and starts its timer on the next `poll` (the clock is not known here). Returns its id.
    pub fn push(&mut self, spec: ToastSpec) -> u64 {
        self.next += 1;
        self.items.push(Item { id: self.next, left: spec.timeout, due: None, spec });
        self.next
    }

    /// [`push`](Self::push) from a handler that has the `Tx`: the timer starts now and the wake is asked for.
    pub fn show(&mut self, spec: ToastSpec, tx: &mut Tx) -> u64 {
        let id = self.push(spec);
        self.tick(tx.clock());
        if let Some(due) = self.next_due() {
            tx.wake_at(due);
        }
        id
    }

    /// Removes a toast; whether there was one.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let n = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != n
    }

    /// Removes the newest toast; its id.
    pub fn dismiss_latest(&mut self) -> Option<u64> {
        self.items.pop().map(|i| i.id)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The toasts, oldest first: (id, message).
    pub fn items(&self) -> impl Iterator<Item = (u64, &ToastSpec)> {
        self.items.iter().map(|i| (i.id, &i.spec))
    }

    /// Whether the timers are stopped (the pointer is over the stack, or focus is inside it).
    pub fn paused(&self) -> bool {
        self.hovered || self.focused
    }

    /// Starts the timers that have not started, removes what has run out and returns whether anything was removed. Nothing runs
    /// while paused.
    pub fn tick(&mut self, clock: f64) -> bool {
        if self.paused() {
            return false;
        }
        for i in &mut self.items {
            if i.due.is_none() && i.spec.timeout > 0.0 {
                i.due = Some(clock + i.left);
            }
        }
        let n = self.items.len();
        self.items.retain(|i| i.due.is_none_or(|d| d > clock + 1e-9));
        self.items.len() != n
    }

    /// The clock time the next toast goes (none while paused or when nothing is timed).
    pub fn next_due(&self) -> Option<f64> {
        if self.paused() {
            return None;
        }
        self.items.iter().filter_map(|i| i.due).min_by(f64::total_cmp)
    }

    /// For the app's poll hook: ticks, asks for the next wake, and says whether the view must be rebuilt.
    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let changed = self.tick(clock);
        if let Some(due) = self.next_due() {
            tx.wake_at(due);
        }
        changed
    }

    fn set_paused(&mut self, hovered: bool, focused: bool, clock: f64) {
        let was = self.paused();
        self.hovered = hovered;
        self.focused = focused;
        let now = self.paused();
        if now && !was {
            for i in &mut self.items {
                if let Some(d) = i.due.take() {
                    i.left = (d - clock).max(0.0);
                }
            }
        } else if !now && was {
            for i in &mut self.items {
                if i.spec.timeout > 0.0 {
                    i.due = Some(clock + i.left);
                }
            }
        }
    }

    /// The pointer entered or left the stack: the timers stop and carry on.
    pub fn set_hover(&mut self, on: bool, clock: f64) {
        self.set_paused(on, self.focused, clock);
    }

    /// Focus entered or left the stack.
    pub fn set_focus(&mut self, on: bool, clock: f64) {
        self.set_paused(self.hovered, on, clock);
    }

    /// The key to give focus to for the newest toast (its action, else its close button), for the shortcut that reaches the stack.
    pub fn focus_target(&self, stack_key: &str) -> Option<String> {
        self.items.last().map(|i| if i.spec.action.is_some() { format!("{stack_key}-{}-action", i.id) } else { format!("{stack_key}-{}-close", i.id) })
    }
}

/// A [`ToastHost`] drawn: the stack at the window's bottom, over the page, mounted into the overlay layer.
pub struct Toasts<S: 'static> {
    key: String,
    host: ToastHost,
    access: Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut ToastHost>,
    position: Position,
    limit: usize,
    action: Option<Pick<S>>,
}

impl<S: 'static> Toasts<S> {
    pub fn new(key: impl Into<String>, host: &ToastHost, access: impl for<'a> Fn(&'a mut S) -> &'a mut ToastHost + 'static) -> Toasts<S> {
        Toasts { key: key.into(), host: host.clone(), access: Rc::new(access), position: Position::BottomTrailing, limit: 4, action: None }
    }

    pub fn position(mut self, p: Position) -> Self {
        self.position = p;
        self
    }

    /// How many show at once: the newest (default 4).
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n.max(1);
        self
    }

    /// A toast's action was pressed: its tag. The toast is already dismissed.
    pub fn on_action(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.action = Some(Rc::new(f));
        self
    }

    pub fn mount(self, t: &Theme, layer: &mut Vec<El>) {
        if self.host.is_empty() {
            return;
        }
        let skip = self.host.len().saturating_sub(self.limit);
        let motion = Transition::move_edge(Edge::Bottom).springs(SpringPair::new(Spring::new(0.24, 0.0), Spring::new(0.16, 0.0)));
        let mut built = Vec::new();
        for (id, spec) in self.host.items().skip(skip) {
            let k = format!("{}-{id}", self.key);
            let mut toast = Toast::<S>::new(k, spec.title.clone());
            if let Some(d) = &spec.description {
                toast = toast.description(d.clone());
            }
            if let Some(tone) = spec.tone {
                toast = toast.tone(tone);
            }
            let acc = self.access.clone();
            if let Some((words, tag)) = spec.action.clone() {
                let (acc, on) = (acc.clone(), self.action.clone());
                toast = toast.action(words, move |s: &mut S| {
                    acc(s).dismiss(id);
                    if let Some(f) = &on {
                        f(s, &tag);
                    }
                });
            }
            let el = toast.on_close(move |s: &mut S| {
                acc(s).dismiss(id);
            }).build(t);
            built.push(if t.reduced_motion { el } else { el.transition(motion) });
        }
        let (hover, focus) = (self.access.clone(), self.access.clone());
        let hook = move |stack: El| {
            stack
                .on_hover_change(move |s: &mut S, on: bool, tx: &mut Tx| {
                    hover(s).set_hover(on, tx.clock());
                    if let Some(due) = hover(s).next_due() {
                        tx.wake_at(due);
                    }
                })
                .on_focus_within_change(move |s: &mut S, on: bool, tx: &mut Tx| {
                    focus(s).set_focus(on, tx.clock());
                    if let Some(due) = focus(s).next_due() {
                        tx.wake_at(due);
                    }
                })
        };
        layer.push(stack_layer(t, self.key, self.position, built, hook));
    }
}
