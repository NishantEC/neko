//! rune-ui registry: diff 0.10.0 Copied into your app by `rune-ui add diff`; yours to change.
//!
//! A unified diff, one file at a time, on the slab: a header line (a chevron, the path cut at its start so the file name stays, and
//! `+n −n` in the success and danger inks) that folds the file open or shut, then its lines in a monospaced face: added lines on the
//! success wash with `+`, removed lines on the danger wash with `−`, context plain, hunk headers faint. The lines scroll sideways.
//! [`parse`] reads `git diff` output into files.
//!
//! **Figures**: the header counts the lines it is given; `stats(added, removed)` sets the figures when the lines are not all there (a
//! folded file with no lines yet, a cut one), and `hide_zero(true)` shows only the figures that are not zero. **Selectable**: with
//! `selectable(true)` the words of every line can be selected and copied (one selection across the lines; the `+` and `−` signs are
//! not copied). **Foot**: `foot(el)` puts a row under the lines for what belongs to the file (Copy path, Reveal, "Show all", a note
//! such as "Renamed from old.rs" or "Binary file"); it shows with the open file.
//!
//! ```ignore
//! for (i, f) in diff::parse(&patch).iter().enumerate() { page = page.child(Diff::new(format!("d{i}"), f, s.open(i)).on_toggle(..).build(t)) }
//! ```

use std::rc::Rc;

use rune::{El, col, row, text};
use rune_ui_style::tabular;

use super::agent::{mono, return_presses, slab};
use super::base::{Press, Shape, Theme, glyph, hit_area, paint_state};
use super::icons;

/// What a line of a diff is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Added,
    Removed,
    Context,
    Hunk,
}

/// One file's changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub lines: Vec<(Line, String)>,
}

impl FileDiff {
    /// (added, removed).
    pub fn stats(&self) -> (usize, usize) {
        (self.lines.iter().filter(|l| l.0 == Line::Added).count(), self.lines.iter().filter(|l| l.0 == Line::Removed).count())
    }
}

/// Files from a unified diff (`diff --git`, `---`/`+++` headers, `@@` hunks).
pub fn parse(unified: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    for l in unified.lines() {
        if let Some(p) = l.strip_prefix("+++ ") {
            let p = p.strip_prefix("b/").unwrap_or(p);
            match files.last_mut() {
                Some(f) if f.path.is_empty() || f.lines.is_empty() => f.path = p.to_owned(),
                _ => files.push(FileDiff { path: p.to_owned(), lines: Vec::new() }),
            }
        } else if l.starts_with("--- ") || l.starts_with("diff --git") || l.starts_with("index ") {
            if l.starts_with("diff --git") || files.last().is_none_or(|f| !f.lines.is_empty()) {
                files.push(FileDiff::default());
            }
        } else if let Some(f) = files.last_mut() {
            let (kind, rest) = match l.chars().next() {
                Some('+') => (Line::Added, &l[1..]),
                Some('-') => (Line::Removed, &l[1..]),
                Some('@') => (Line::Hunk, l),
                Some(' ') => (Line::Context, &l[1..]),
                _ => (Line::Context, l),
            };
            f.lines.push((kind, rest.to_owned()));
        }
    }
    files.retain(|f| !f.path.is_empty());
    files
}

/// The header's name for assistive technology: the path, then the figures that are shown.
fn figures_label(path: &str, add: usize, del: usize, hide_zero: bool) -> String {
    let mut parts = vec![path.to_owned()];
    if !(hide_zero && add == 0) {
        parts.push(format!("{add} added"));
    }
    if !(hide_zero && del == 0) {
        parts.push(format!("{del} removed"));
    }
    parts.join(", ")
}

pub struct Diff<'a, S: 'static> {
    key: String,
    file: &'a FileDiff,
    open: bool,
    toggle: Option<Press<S>>,
    stats: Option<(usize, usize)>,
    hide_zero: bool,
    selectable: bool,
    foot: Option<El>,
}

impl<'a, S: 'static> Diff<'a, S> {
    pub fn new(key: impl Into<String>, file: &'a FileDiff, open: bool) -> Diff<'a, S> {
        Diff { key: key.into(), file, open, toggle: None, stats: None, hide_zero: false, selectable: false, foot: None }
    }

    /// The header's figures, when they are not the count of the lines given (a folded file has none yet; a cut one has fewer).
    pub fn stats(mut self, added: usize, removed: usize) -> Self {
        self.stats = Some((added, removed));
        self
    }

    /// Shows only the figures that are not zero ("+12" alone for a file that only grew).
    pub fn hide_zero(mut self, on: bool) -> Self {
        self.hide_zero = on;
        self
    }

    /// Lets the person select and copy the words of the lines: one selection runs across them, signs left out.
    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = on;
        self
    }

    /// A row under the lines of the open file for what belongs to it (Copy path, Reveal, Show all, a note). Keyed `<key>-foot`.
    pub fn foot(mut self, e: El) -> Self {
        self.foot = Some(e);
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.toggle = Some(Rc::new(f));
        self
    }

    pub fn build(self, t: &Theme) -> El {
        let (add, del) = self.stats.unwrap_or_else(|| self.file.stats());
        let h = t.control.md;
        let trigger = format!("{}-trigger", self.key);
        let mut head = row().key(trigger.clone()).role("button").label(figures_label(&self.file.path, add, del, self.hide_zero)).expanded(self.open).focusable(true).w_full().h(h).px(t.space.at(3.0)).gap(t.metrics.gap).center_y().radius(t.radius.lg).round_as(t);
        head = paint_state(head, &t.mat.row, Default::default())
            .child(glyph(if self.open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, t.control.icon_sm, t.ink_faint()))
            .child(glyph(&icons::FILE, t.control.icon, t.ink_muted()))
            .child(text(self.file.path.clone()).font(mono(t, t.ty.sm)).color(t.ink()).truncate_head().min_w(0.0).grow(1.0).a11y_hidden())
            .children((!(self.hide_zero && add == 0)).then(|| text(format!("+{add}")).font(tabular(t.ty.sm)).color(t.c.success.fg).a11y_hidden()))
            .children((!(self.hide_zero && del == 0)).then(|| text(format!("\u{2212}{del}")).font(tabular(t.ty.sm)).color(t.c.danger.fg).a11y_hidden()));
        if let Some(f) = self.toggle.clone() {
            let g = f.clone();
            head = head.on_press(move |s: &mut S| f(s)).on_key(move |s: &mut S, k: &rune::KeyEvent, _: &mut rune::Tx| return_presses(k, || g(s)));
        }
        let mut s = slab(t, self.key.clone()).role("group").label(format!("Changes to {}", self.file.path)).child(hit_area(&trigger, head, 0.0, t.control.slop(h), self.toggle));
        if self.open {
            let style = mono(t, t.ty.sm);
            let mut lines = col().w_full().gap(0.0);
            for (kind, words) in &self.file.lines {
                let (sign, fill, ink) = match kind {
                    Line::Added => ("+", Some(t.c.success.bg), t.ink()),
                    Line::Removed => ("\u{2212}", Some(t.c.danger.bg), t.ink()),
                    Line::Context => (" ", None, t.ink_muted()),
                    Line::Hunk => ("", Some(t.c.muted.bg), t.ink_faint()),
                };
                let mut r = row().w_full().px(t.space.at(3.0)).gap(t.space.at(2.0));
                if let Some(f) = fill {
                    r = r.fill(f);
                }
                let body = text(if words.is_empty() { " ".to_owned() } else { words.clone() }).font(style).color(ink).selectable(self.selectable);
                r = r.child(text(sign.to_owned()).font(style).color(ink).w(10.0)).child(body);
                lines = lines.child(r);
            }
            if !self.file.lines.is_empty() {
                s = s.child(col().key(format!("{}-lines", self.key)).w_full().border_sides([1.0, 0.0, 0.0, 0.0], t.c.line).py(t.space.at(2.0)).scroll_x().selection_group(self.selectable).child(lines));
            }
            if let Some(f) = self.foot {
                s = s.child(row().key(format!("{}-foot", self.key)).w_full().border_sides([1.0, 0.0, 0.0, 0.0], t.c.line).px(t.space.at(3.0)).py(t.space.at(1.5)).gap(t.space.at(2.0)).center_y().child(f));
            }
        }
        s
    }
}
