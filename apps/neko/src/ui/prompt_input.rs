//! rune-ui registry: prompt_input 0.10.0 Copied into your app by `rune-ui add prompt_input`; yours to change.
//!
//! Where a person writes to the agent: a well holding attachment chips, a text area that grows with the text (one line at first, eight
//! at most, then it scrolls while the toolbar stays put), and a toolbar row (an attach button, whatever the app puts there, such as a
//! model selector and tool toggles, then the submit button at the right). Return sends; Shift+Return makes a new line. While the agent
//! answers the submit button becomes Stop and Escape stops; disabled, nothing can be typed or sent.
//!
//! **Mentions and commands** (optional; nothing changes without them). With [`PromptInput::mentions`], typing `@` after a space opens
//! a list over the composer of what can be mentioned (files, people), filtered by what follows; with [`PromptInput::commands`], a `/`
//! at the very start opens the commands. Both lists are keyboard-first and leave the caret in the text: Up and Down move the
//! highlight, Return or Tab picks, Escape closes the list (and only the list). A mention goes in as one token (a Rune pill) that
//! Backspace deletes whole; a command runs its callback and inserts nothing. Return with an open list that matches nothing does
//! nothing (it never sends `/typo`). The lists open from the overlay layer: build with [`PromptInput::build_in`].
//!
//! ```ignore
//! PromptInput::new("composer", &s.draft, |s: &mut App| &mut s.draft).phase(s.phase).tool(model_selector)
//!     .on_submit(|s: &mut App| s.send()).on_stop(|s: &mut App| s.stop()).build(t)
//! // with mentions and commands:
//! PromptInput::new("composer", &s.draft, |s: &mut App| &mut s.draft).menu(&s.menu, |s: &mut App| &mut s.menu)
//!     .mentions(people_and_files).commands(commands, |s: &mut App, id: &str| s.run(id)).build_in(t, &mut layer)
//! ```

use std::rc::Rc;

use rune::{Align, El, Icon, Key, KeyEvent, Side, Spring, SpringPair, TextEdit, Transition, Tx, Vec2, col, row, text};
use rune_ui_style::Radius;

use super::agent::{PressTx, icon_button};
use super::base::{Choose, Press, Shape, Theme, glyph, hit_area, paint, paint_state, recipes, resolve, states};
use super::icons;

/// Whether the composer can send, is waiting on an answer, or is off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Ready,
    /// An answer is arriving: submit is Stop.
    Streaming,
    Disabled,
}

/// Something a mention or a command list offers.
#[derive(Clone, Debug, Default)]
pub struct Suggestion {
    /// What a mention token carries, and what a command's callback is handed.
    pub id: String,
    /// The words shown, and a mention token's words after its `@`.
    pub label: String,
    /// Quiet words at the right ("src/main.rs", "Clears the chat").
    pub detail: Option<String>,
    pub icon: Option<&'static Icon>,
}

impl Suggestion {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Suggestion {
        Suggestion { id: id.into(), label: label.into(), detail: None, icon: None }
    }

    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }

    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
        self
    }
}

/// The open list's highlight, and the trigger Escape closed (so it stays closed until a new `@` or `/`). Keep one in the app's state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    pub highlight: usize,
    pub dismissed: Option<(Kind, usize)>,
}

/// Which list is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mention,
    Command,
}

/// The list the text asks for, if any: its kind, where its `@` or `/` is (a byte offset) and the words typed after it. A command is
/// a `/` at the very start with no space before the caret; a mention is an `@` at the start or after a space, outside any token,
/// with no space between it and the caret.
pub fn trigger(edit: &TextEdit) -> Option<(Kind, usize, String)> {
    let (a, b) = edit.range();
    if a != b {
        return None;
    }
    let before = edit.text.get(..b)?;
    if let Some(q) = before.strip_prefix('/')
        && !q.contains(char::is_whitespace)
    {
        return Some((Kind::Command, 0, q.to_owned()));
    }
    let at = before.rfind('@')?;
    let q = &before[at + 1..];
    let opens = at == 0 || before[..at].ends_with(char::is_whitespace);
    let in_token = edit.pills().any(|(r, _)| r.start <= at && at < r.end);
    (opens && !in_token && !q.contains(char::is_whitespace)).then(|| (Kind::Mention, at, q.to_owned()))
}

/// What `query` keeps of `items`, best first: labels that start with it, then labels that contain it (case aside); at most eight.
pub fn matches<'b>(items: &'b [Suggestion], query: &str) -> Vec<&'b Suggestion> {
    let q = query.to_lowercase();
    let mut first: Vec<&Suggestion> = items.iter().filter(|s| s.label.to_lowercase().starts_with(&q)).collect();
    first.extend(items.iter().filter(|s| { let l = s.label.to_lowercase(); !l.starts_with(&q) && l.contains(&q) }));
    first.truncate(8);
    first
}

type MenuAccess<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut MenuState>;
type OnCommand<S> = Rc<dyn Fn(&mut S, &str)>;

pub struct PromptInput<'a, S: 'static> {
    key: String,
    edit: &'a TextEdit,
    access: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    placeholder: String,
    phase: Phase,
    attachments: Vec<El>,
    tools: Vec<El>,
    lead: Option<El>,
    chips: Vec<El>,
    rows: (usize, usize),
    submit: Option<Press<S>>,
    stop: Option<Press<S>>,
    attach: Option<Press<S>>,
    mentions: Vec<Suggestion>,
    commands: Vec<Suggestion>,
    command: Option<OnCommand<S>>,
    menu: MenuState,
    menu_access: Option<MenuAccess<S>>,
    spellcheck: bool,
}

impl<'a, S: 'static> PromptInput<'a, S> {
    pub fn new(key: impl Into<String>, edit: &'a TextEdit, access: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static) -> PromptInput<'a, S> {
        PromptInput { key: key.into(), edit, access: Rc::new(access), placeholder: "Ask anything".to_owned(), phase: Phase::Ready, attachments: Vec::new(), tools: Vec::new(), lead: None, chips: Vec::new(), rows: (1, 8), submit: None, stop: None, attach: None, mentions: Vec::new(), commands: Vec::new(), command: None, menu: MenuState::default(), menu_access: None, spellcheck: false }
    }

    /// Marks misspelt words as the person writes, with the system's checker. Off by default: turn it on where people write prose.
    pub fn spellcheck(mut self, on: bool) -> Self {
        self.spellcheck = on;
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    pub fn phase(mut self, p: Phase) -> Self {
        self.phase = p;
        self
    }

    /// An attachment chip above the text (`Attachment`).
    pub fn attachment(mut self, e: El) -> Self {
        self.attachments.push(e);
        self
    }

    /// Something for the toolbar, after the attach button (a model selector, a tool toggle).
    pub fn tool(mut self, e: El) -> Self {
        self.tools.push(e);
        self
    }

    /// An element at the left of the text, top-aligned with its first line: the picture of who the message is for, a mode switch.
    /// It is its own Tab stop, reached before the text (reading order), and the text keeps the room that is left. Without one the
    /// composer is as before.
    pub fn lead(mut self, e: El) -> Self {
        self.lead = Some(e);
        self
    }

    /// An element in the row of chips under the text and above the toolbar (the folder the work will happen in, the branch, a
    /// model): chips wrap, keep their order, and are Tab stops after the text and before the toolbar. The row is keyed
    /// `<key>-chips` and exists only when there is a chip.
    pub fn chip(mut self, e: El) -> Self {
        self.chips.push(e);
        self
    }

    /// Lines at first and at most (1 and 8).
    pub fn rows(mut self, min: usize, max: usize) -> Self {
        self.rows = (min.max(1), max.max(min.max(1)));
        self
    }

    /// Return or the submit button, with words in the field and the composer ready.
    pub fn on_submit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.submit = Some(Rc::new(f));
        self
    }

    /// Stop or Escape while streaming.
    pub fn on_stop(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.stop = Some(Rc::new(f));
        self
    }

    /// Shows the attach button.
    pub fn on_attach(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.attach = Some(Rc::new(f));
        self
    }

    /// What `@` offers (files, people). Needs [`Self::menu`] and [`Self::build_in`].
    pub fn mentions(mut self, items: Vec<Suggestion>) -> Self {
        self.mentions = items;
        self
    }

    /// What `/` at the start offers; picking one runs `f` with its id and inserts nothing. Needs [`Self::menu`] and [`Self::build_in`].
    pub fn commands(mut self, items: Vec<Suggestion>, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.commands = items;
        self.command = Some(Rc::new(f));
        self
    }

    /// Where the open list keeps its highlight.
    pub fn menu(mut self, state: &MenuState, access: impl for<'b> Fn(&'b mut S) -> &'b mut MenuState + 'static) -> Self {
        self.menu = state.clone();
        self.menu_access = Some(Rc::new(access));
        self
    }

    /// The composer, with its mention or command list (when one is open) pushed into `layer`, over the composer.
    pub fn build_in(self, t: &Theme, layer: &mut Vec<El>) -> El {
        let open = self.open_list();
        let key = self.key.clone();
        let (well, list) = self.compose(t, open);
        if let Some(list) = list {
            let from = Vec2 { x: 0.0, y: 1.0 };
            let placed = list.anchored_fit(key, Side::Top, Align::Start, t.space.at(1.5));
            layer.push(if t.reduced_motion { placed } else { placed.transition(Transition::scale_anchor(0.97, from).springs(SpringPair::new(Spring::new(0.14, 0.0), Spring::new(0.09, 0.0)))) });
        }
        well
    }

    /// The open list, if the text asks for one this composer offers and Escape did not close it: its kind, trigger, and matches.
    fn open_list(&self) -> Option<(Kind, usize, String)> {
        if self.phase == Phase::Disabled || !self.edit.focused || self.menu_access.is_none() {
            return None;
        }
        let (kind, at, q) = trigger(self.edit)?;
        let offered = match kind {
            Kind::Mention => !self.mentions.is_empty(),
            Kind::Command => !self.commands.is_empty() && self.command.is_some(),
        };
        (offered && self.menu.dismissed != Some((kind, at))).then_some((kind, at, q))
    }

    pub fn build(self, t: &Theme) -> El {
        self.compose(t, None).0
    }

    fn compose(mut self, t: &Theme, open: Option<(Kind, usize, String)>) -> (El, Option<El>) {
        let parts = recipes::field(t);
        let disabled = self.phase == Phase::Disabled;
        let streaming = self.phase == Phase::Streaming;
        let st = states(None, disabled, false);
        let mut well = col().key(self.key.clone()).role("group").label("Message composer").w_full().pt(t.space.at(2.5)).pb(t.space.at(2.0)).px(t.space.at(3.0)).gap(t.space.at(2.0)).radius(t.radius.xl).round_as(t);
        well = paint(well, &resolve(&parts.root, st));
        if !disabled && let Some((w, o, c)) = parts.root.live().focus_ring {
            well = well.focus_within_ring(w, o, c);
        }
        if !self.attachments.is_empty() {
            well = well.child(row().key(format!("{}-attachments", self.key)).w_full().gap(t.space.at(2.0)).flex_wrap().children(std::mem::take(&mut self.attachments)));
        }
        let has_words = !self.edit.text.trim().is_empty();
        let can_send = self.phase == Phase::Ready && has_words;
        let send = self.submit.clone().filter(|_| can_send);
        let stop = self.stop.clone().filter(|_| streaming);
        let (access, words_in) = (self.access.clone(), self.access.clone());
        let (on_enter, on_escape) = (self.submit.clone(), self.stop.clone());
        let ready = self.phase == Phase::Ready;
        // The open list: what it offers now, and what picking each does.
        let offered: Vec<Suggestion> = match &open {
            Some((Kind::Mention, _, q)) => matches(&self.mentions, q).into_iter().cloned().collect(),
            Some((Kind::Command, _, q)) => matches(&self.commands, q).into_iter().cloned().collect(),
            None => Vec::new(),
        };
        let highlight = self.menu.highlight.min(offered.len().saturating_sub(1));
        let pick: Option<Choose<S>> = open.as_ref().map(|(kind, at, _)| {
            let (kind, at, items, edit, menu, command) = (*kind, *at, offered.clone(), self.access.clone(), self.menu_access.clone(), self.command.clone());
            let f: Choose<S> = Rc::new(move |s: &mut S, i: usize| {
                let Some(item) = items.get(i) else { return };
                let e = edit(s);
                let caret = e.range().1;
                match kind {
                    Kind::Mention => {
                        e.replace_with_pill(at..caret, item.id.clone(), &format!("@{}", item.label));
                        e.insert(" ");
                    }
                    Kind::Command => {
                        // A command inserts nothing: the `/` and what was typed after it go.
                        e.set_selection(0, caret);
                        e.insert("");
                    }
                }
                if let Some(m) = &menu {
                    *m(s) = MenuState::default();
                }
                if kind == Kind::Command && let Some(f) = &command {
                    f(s, &item.id);
                }
            });
            f
        });
        let list_open = open.clone();
        let (menu_keys, pick_keys, n_offered) = (self.menu_access.clone(), pick.clone(), offered.len());
        let menu_reset = self.menu_access.clone();
        let mut input = rune::text_input(self.edit, t.ty.md)
            .key(format!("{}-input", self.key))
            .w_full()
            .color(t.ink())
            .placeholder(self.placeholder.clone(), t.ink_faint())
            .caret(t.ink(), t.c.accent.fg.alpha(0.28))
            .label("Message")
            .spellcheck(self.spellcheck)
            .pill_colors(t.c.selected, Some(t.ink()))
            .on_submit(move |s: &mut S, _: &mut Tx| {
                if ready && !words_in(s).text.trim().is_empty() && let Some(f) = &on_enter {
                    f(s);
                }
            })
            .on_input_key(move |s: &mut S, k: &KeyEvent, _: &mut Tx| {
                // An open list takes Up, Down, Return, Tab and Escape first; Return with nothing to pick does nothing.
                if let (Some((kind, at, _)), Some(menu)) = (&list_open, &menu_keys)
                    && !k.mods.cmd && !k.mods.ctrl && !k.mods.alt
                {
                    let m = menu(s);
                    match k.key {
                        Key::Down if n_offered > 0 => {
                            m.highlight = (m.highlight.min(n_offered - 1) + 1) % n_offered;
                            return true;
                        }
                        Key::Up if n_offered > 0 => {
                            m.highlight = (m.highlight.min(n_offered - 1) + n_offered - 1) % n_offered;
                            return true;
                        }
                        Key::Enter | Key::Tab if !k.mods.shift => {
                            if let Some(p) = &pick_keys
                                && n_offered > 0
                            {
                                let i = m.highlight.min(n_offered - 1);
                                p(s, i);
                            }
                            return true;
                        }
                        Key::Escape => {
                            *m = MenuState { highlight: 0, dismissed: Some((*kind, *at)) };
                            return true;
                        }
                        _ => {}
                    }
                }
                if k.key == Key::Escape && streaming && let Some(f) = &on_escape {
                    f(s);
                    return true;
                }
                false
            })
            .on_input_change(move |s: &mut S, _: &mut Tx| {
                // Typing moves the highlight back to the best match.
                if let Some(m) = &menu_reset {
                    m(s).highlight = 0;
                }
            });
        // A disabled composer is not bound to the draft, so nothing typed can reach it.
        if disabled {
            input = input.disabled(true).read_only(true).focusable(false);
        } else {
            input = input.on_edit(rune::edit_access(move |s: &mut S| access(s)));
        }
        let field = rune::auto_grow(input, self.rows.0, self.rows.1);
        well = match self.lead.take() {
            // Beside the text: top-aligned with its first line, the text taking what is left.
            Some(lead) => well.child(row().key(format!("{}-body", self.key)).w_full().gap(t.space.at(3.0)).items_start().child(lead).child(col().grow(1.0).shrink().min_w(0.0).child(field))),
            None => well.child(field),
        };
        if !self.chips.is_empty() {
            well = well.child(row().key(format!("{}-chips", self.key)).w_full().gap(t.space.at(2.0)).flex_wrap().center_y().children(std::mem::take(&mut self.chips)));
        }
        let mut bar = row().key(format!("{}-toolbar", self.key)).w_full().gap(t.space.at(3.5)).center_y();
        if let Some(f) = self.attach.clone() {
            let p: PressTx<S> = Rc::new(move |s: &mut S, _: &mut Tx| f(s));
            bar = bar.child(icon_button(t, &format!("{}-attach", self.key), "Attach files", &icons::PAPERCLIP, None, disabled, Some(p)));
        }
        bar = bar.children(std::mem::take(&mut self.tools)).child(row().grow(1.0));
        // The submit button: round, an arrow; Stop while streaming. With something to send it is ink. With nothing to send it is
        // still a button you can see the shape of: the raised material at rest with a faint arrow, not ink dimmed to a flat grey
        // disc. It is off either way (not focusable, no press, announced as disabled).
        let h = t.control.sm + 4.0;
        let key = format!("{}-submit", self.key);
        let live = if streaming { stop.is_some() } else { send.is_some() };
        let face = row().key(key.clone()).role("button").label(if streaming { "Stop" } else { "Send" }).help(if streaming { "Stop (Escape)" } else { "Send (Return)" }).focusable(live).size(h, h).center().radius(h / 2.0);
        let (material, arrow) = if live { (&t.mat.ink, t.c.primary.fg) } else { (&t.mat.raised, t.ink_faint()) };
        let mut face = paint_state(face, material, states(None, false, false)).child(glyph(if streaming { &icons::STOP } else { &icons::ARROW_UP }, t.control.icon, arrow));
        if !live {
            face = face.disabled(true);
        }
        let press = if streaming { stop } else { send };
        if let Some(f) = press.clone() {
            face = face.on_press(move |s: &mut S| f(s));
        }
        let slop = t.control.slop(h);
        bar = bar.child(hit_area(&key, face, slop, slop, press));
        let list = open.map(|(kind, _, _)| self.list(t, kind, &offered, highlight, pick));
        (well.child(bar), list)
    }

    /// The open list's surface: rows of what it offers, the highlighted one filled; a press picks. It never takes focus.
    fn list(&self, t: &Theme, kind: Kind, offered: &[Suggestion], highlight: usize, pick: Option<Choose<S>>) -> El {
        let (k, name) = match kind {
            Kind::Mention => (format!("{}-mentions", self.key), "Mention"),
            Kind::Command => (format!("{}-commands", self.key), "Commands"),
        };
        let inset = 4.0;
        let radius = Radius::inner(t.radius.lg, inset);
        let mut rows = col().key(format!("{k}-list")).role("listbox").label(name).w_full().gap(1.0);
        if offered.is_empty() {
            rows = rows.child(row().key(format!("{k}-none")).h(t.metrics.row).px(t.metrics.pad_md - inset).center_y().child(text(if kind == Kind::Command { "No such command" } else { "No matches" }).font(t.ty.base).color(t.ink_faint())));
        }
        for (i, item) in offered.iter().enumerate() {
            let on = i == highlight;
            let mut r = row().key(format!("{k}-{i}")).role("option").label(item.label.clone()).checked(on).min_h(t.metrics.row).px(t.metrics.pad_md - inset).gap(t.metrics.gap).center_y().radius(radius).round_as(t);
            r = if on { r.fill(t.c.selected) } else { r.hover(t.c.hover) };
            if let Some(i) = item.icon {
                r = r.child(glyph(i, t.control.icon, t.ink_muted()));
            }
            let words = if kind == Kind::Command { format!("/{}", item.label) } else { item.label.clone() };
            r = r.child(text(words).font(t.ty.base).color(t.ink()).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden());
            if let Some(d) = &item.detail {
                r = r.child(text(d.clone()).font(t.ty.sm).color(t.ink_faint()).truncate().shrink().min_w(0.0).a11y_hidden());
            }
            if let Some(p) = pick.clone() {
                r = r.on_press(move |s: &mut S| p(s, i));
            }
            rows = rows.child(r);
        }
        paint(col().key(k).w(320.0).padding(inset).radius(t.radius.lg).round_as(t), &t.mat.floating.base).child(rows)
    }
}
