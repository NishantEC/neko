//! rune-ui registry: receipt 0.10.0. Copied into your app by `rune-ui add receipt`; yours to change.
//!
//! What a piece of work leaves behind, as a card: a header (a leading element, one sentence, the totals added and removed), the files
//! it touched each with its own figures, and a foot with the branch and the next actions. It is the summary under a finished run, a
//! commit, a deploy or an import: a raised card with 6 points of padding whose header is 40 points high, the files in a well at the
//! card's inner radius (so the two corners are concentric) in 32 point rows, and a foot of the same height.
//!
//! * **Figures** show only what is not zero: a file that only grew shows `+12`, one that only shrank `−3`, one with neither none
//!   ([`figures`] draws them, for your own rows). They use the success and danger inks and are always said in words to assistive
//!   technology ("12 added, 3 removed").
//! * **Files**: paths are monospaced and cut in the middle, so the folder and the file name both stay. After five rows
//!   (`max_files`) the rest are "and 3 more". With `on_file` a row is a button that gets its path; without it, rows are plain.
//! * **Settling**: the card comes in 8 points up with a fade, and appears in place under Reduce Motion (`Theme::reduced_motion`).
//!
//! ```ignore
//! Receipt::new("receipt", "Fixed the login test").lead(portrait).totals(14, 3)
//!     .files(&[ReceiptFile::new("src/login.rs", 12, 3), ReceiptFile::new("tests/login.rs", 2, 0)])
//!     .branch("fix/login-test").action(review_button).action(undo_button).build(t)
//! ```

use std::rc::Rc;

use rune::{El, Transition, col, row, text};
use rune_ui_style::{tabular, weight};

use super::agent::mono;
use super::base::{Shape, Theme, glyph, paint};
use super::icons;

/// One file in the receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptFile {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    /// A quiet word after the path ("new", "renamed", "binary").
    pub note: Option<String>,
}

impl ReceiptFile {
    pub fn new(path: impl Into<String>, added: usize, removed: usize) -> ReceiptFile {
        ReceiptFile { path: path.into(), added, removed, note: None }
    }

    pub fn note(mut self, words: impl Into<String>) -> Self {
        self.note = Some(words.into());
        self
    }
}

/// `+12` and `−3` in the success and danger inks, a side left out when it is zero (nothing at all when both are). Hidden from
/// assistive technology: say them with [`figures_words`].
pub fn figures(t: &Theme, added: usize, removed: usize) -> El {
    let mut r = row().gap(t.space.at(1.5)).center_y();
    if added > 0 {
        r = r.child(text(format!("+{added}")).font(tabular(t.ty.sm)).color(t.c.success.fg).a11y_hidden());
    }
    if removed > 0 {
        r = r.child(text(format!("\u{2212}{removed}")).font(tabular(t.ty.sm)).color(t.c.danger.fg).a11y_hidden());
    }
    r.a11y_hidden()
}

/// The figures as words: "12 added, 3 removed", only the sides that are not zero ("" when both are).
pub fn figures_words(added: usize, removed: usize) -> String {
    match (added, removed) {
        (0, 0) => String::new(),
        (a, 0) => format!("{a} added"),
        (0, r) => format!("{r} removed"),
        (a, r) => format!("{a} added, {r} removed"),
    }
}

type OnFile<S> = Rc<dyn Fn(&mut S, &str)>;

pub struct Receipt<S: 'static> {
    key: String,
    sentence: String,
    lead: Option<El>,
    totals: Option<(usize, usize)>,
    files: Vec<ReceiptFile>,
    max_files: usize,
    note: Option<String>,
    branch: Option<String>,
    actions: Vec<El>,
    on_file: Option<OnFile<S>>,
    settle: bool,
}

impl<S: 'static> Receipt<S> {
    /// `sentence`: what was done, in one line ("Fixed the login test").
    pub fn new(key: impl Into<String>, sentence: impl Into<String>) -> Receipt<S> {
        Receipt { key: key.into(), sentence: sentence.into(), lead: None, totals: None, files: Vec::new(), max_files: 5, note: None, branch: None, actions: Vec::new(), on_file: None, settle: true }
    }

    /// An element at the header's start (a 24 point portrait, a status mark).
    pub fn lead(mut self, e: El) -> Self {
        self.lead = Some(e);
        self
    }

    /// The totals at the header's end: lines added and removed.
    pub fn totals(mut self, added: usize, removed: usize) -> Self {
        self.totals = Some((added, removed));
        self
    }

    /// The files, each with its own figures.
    pub fn files(mut self, files: &[ReceiptFile]) -> Self {
        self.files = files.to_vec();
        self
    }

    /// How many files to list before "and N more" (5).
    pub fn max_files(mut self, n: usize) -> Self {
        self.max_files = n.max(1);
        self
    }

    /// A quiet line under the header (what to do next, what could not be done).
    pub fn note(mut self, words: impl Into<String>) -> Self {
        self.note = Some(words.into());
        self
    }

    /// The branch (or any place the work went), monospaced, at the foot's start.
    pub fn branch(mut self, name: impl Into<String>) -> Self {
        self.branch = Some(name.into());
        self
    }

    /// An action at the foot's end (a `Button`, in the order given). Each is its own Tab stop.
    pub fn action(mut self, e: El) -> Self {
        self.actions.push(e);
        self
    }

    /// Makes each file row a button that gets its path (open it, show its diff).
    pub fn on_file(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_file = Some(Rc::new(f));
        self
    }

    /// Whether the card settles in when it first appears (on by default; never under Reduce Motion).
    pub fn settle(mut self, on: bool) -> Self {
        self.settle = on;
        self
    }

    fn file_row(&self, t: &Theme, i: usize, f: &ReceiptFile) -> El {
        let said = [f.path.clone(), figures_words(f.added, f.removed)].into_iter().chain(f.note.clone()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
        let mut r = row().key(format!("{}-file{i}", self.key)).role("listitem").label(said).w_full().h(t.space.at(8.0)).px(t.space.at(2.5)).gap(t.space.at(2.0)).center_y().radius(t.radius.md).round_as(t)
            .child(glyph(&icons::FILE, t.control.icon, t.ink_faint()))
            .child(text(f.path.clone()).font(mono(t, t.ty.sm)).color(t.ink()).truncate_middle().grow(1.0).shrink().min_w(0.0).a11y_hidden());
        if let Some(n) = &f.note {
            r = r.child(text(n.clone()).font(t.ty.sm).color(t.ink_faint()).a11y_hidden());
        }
        r = r.child(figures(t, f.added, f.removed));
        if let Some(on) = self.on_file.clone() {
            let path = f.path.clone();
            r = r.role("button").focusable(true).hover(t.c.hover).on_press(move |s: &mut S| on(s, &path));
        }
        r
    }

    pub fn build(self, t: &Theme) -> El {
        let pad = 6.0;
        let radius = t.radius.lg;
        // Header: 40 points.
        let mut head = row().key(format!("{}-head", self.key)).w_full().h(t.space.at(10.0)).px(t.space.at(2.5)).gap(t.space.at(2.5)).center_y();
        if let Some(e) = self.lead.as_ref() {
            head = head.child(e.clone());
        }
        head = head.child(text(self.sentence.clone()).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden());
        let totals = self.totals.map(|(a, r)| figures_words(a, r)).unwrap_or_default();
        if let Some((a, r)) = self.totals {
            head = head.child(figures(t, a, r));
        }
        let mut card = paint(col().key(self.key.clone()).role("group").label(if totals.is_empty() { self.sentence.clone() } else { format!("{}: {totals}", self.sentence) }).w_full().padding(pad).radius(radius + pad).round_as(t), &t.mat.raised.base).child(head);
        if let Some(n) = &self.note {
            card = card.child(row().key(format!("{}-note", self.key)).w_full().px(t.space.at(2.5)).pb(t.space.at(2.0)).child(text(n.clone()).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0)));
        }
        if !self.files.is_empty() {
            let shown = self.files.len().min(self.max_files);
            let mut well = paint(col().key(format!("{}-files", self.key)).role("list").label("Files changed").w_full().padding(2.0).radius(radius).round_as(t), &t.mat.well.base);
            for (i, f) in self.files.iter().take(shown).enumerate() {
                well = well.child(self.file_row(t, i, f));
            }
            if self.files.len() > shown {
                well = well.child(row().key(format!("{}-more", self.key)).role("listitem").label(format!("and {} more", self.files.len() - shown)).w_full().h(t.space.at(8.0)).pl(t.space.at(2.5) + t.control.icon + t.space.at(2.0)).center_y().child(text(format!("and {} more", self.files.len() - shown)).font(t.ty.sm).color(t.ink_faint()).a11y_hidden()));
            }
            card = card.child(well);
        }
        if self.branch.is_some() || !self.actions.is_empty() {
            let mut foot = row().key(format!("{}-foot", self.key)).w_full().h(t.space.at(10.0)).px(t.space.at(2.5)).gap(t.space.at(2.0)).center_y();
            foot = foot.child(match &self.branch {
                Some(b) => text(b.clone()).key(format!("{}-branch", self.key)).font(mono(t, t.ty.sm)).color(t.ink_muted()).truncate_middle().grow(1.0).shrink().min_w(0.0).label(format!("Branch {b}")),
                None => text(String::new()).grow(1.0).a11y_hidden(),
            });
            card = card.child(foot.children(self.actions));
        }
        // Settles in 8 points up with a fade; in place under Reduce Motion.
        let mut motion = Transition::offset(0.0, 8.0);
        motion.insertion.opacity = 0.0;
        if self.settle { if t.reduced_motion { card } else { card.transition(motion) } } else { card }
    }
}
