//! The Changes tab of the right pane (docs/knowledge/undo.md): what a run changed, what the whole thread changed, or what is
//! uncommitted in its folder. Each file is a rune-ui Diff (selectable, a zero side of its figures hidden, its counts given with `stats`),
//! folded until opened; its lines come from the daemon when it opens. Binary files are named with their sizes, very long diffs are cut
//! with "Show all", and an open file has Copy diff, Copy path and Reveal in Finder.

use std::rc::Rc;

use neko_protocol::undo::Action;
use neko_protocol::undo::{self, CUT_LINES};
use neko_protocol::{DiffFile, DiffPatch, DiffSource, FileStatus, TaskId};
use rune::{El, Tx, col, el, row, text};

use crate::state::{App, Review};
use crate::ui::agent::{PressTx, icon_button};
use crate::ui::base::{ButtonVariant, Size, Theme};
use crate::ui::button::Button;
use crate::ui::diff::{Diff, FileDiff, Line};
use crate::ui::icons;
use crate::ui::segmented_control::SegmentedControl;

/// The Changes tab's source switch.
const SHOW: [&str; 3] = ["This run", "All runs", "Uncommitted"];

/// Bytes in words, as the Finder counts them.
pub fn size_words(b: u64) -> String {
    match b {
        0..1_000 => format!("{b} byte{}", if b == 1 { "" } else { "s" }),
        1_000..1_000_000 => format!("{:.1} KB", b as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b as f64 / 1e6),
        _ => format!("{:.1} GB", b as f64 / 1e9),
    }
}

/// `1234` as "1,234".
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The header's words for a file: its path, or where it moved from and to.
pub fn file_label(f: &DiffFile) -> String {
    match (&f.old_path, f.status) {
        (Some(old), FileStatus::Renamed | FileStatus::Copied) => format!("{old} → {}", f.path),
        _ => f.path.clone(),
    }
}

/// One line about a file that has no lines to show, or more to say than its lines: binary with its sizes, a mode, a submodule.
pub fn file_note(f: &DiffFile) -> Option<String> {
    if f.binary {
        return Some(match (f.old_size, f.new_size) {
            (Some(a), Some(b)) => format!("Binary file, {} before, {} now", size_words(a), size_words(b)),
            (None, Some(b)) => format!("New binary file, {}", size_words(b)),
            (Some(a), None) => format!("Binary file removed, it was {}", size_words(a)),
            (None, None) => "Binary file".to_owned(),
        });
    }
    if f.status == FileStatus::Submodule {
        return Some("The submodule now points at another commit.".to_owned());
    }
    match (&f.old_mode, &f.new_mode) {
        (Some(_), Some(n)) if n == "100755" => Some("Now executable.".to_owned()),
        (Some(o), Some(_)) if o == "100755" => Some("No longer executable.".to_owned()),
        (Some(o), Some(n)) => Some(format!("Its mode changed from {o} to {n}.")),
        _ => None,
    }
}

/// The lines rune-ui's Diff draws. Folded, none (the header's counts are `Diff::stats`); open, the file's own lines, or one line
/// saying what there is instead.
pub fn file_lines(f: &DiffFile, open: bool, patch: Option<&DiffPatch>) -> Vec<(Line, String)> {
    if !open {
        return Vec::new();
    }
    if let Some(note) = file_note(f).filter(|_| f.binary || f.status == FileStatus::Submodule) {
        return vec![(Line::Context, note)];
    }
    let Some(p) = patch else { return vec![(Line::Context, "Loading…".to_owned())] };
    if p.lines.is_empty() {
        return vec![(Line::Context, if f.status == FileStatus::Added { "An empty file.".to_owned() } else { file_note(f).unwrap_or_else(|| "No lines changed.".to_owned()) })];
    }
    p.lines
        .iter()
        .map(|l| match l.chars().next() {
            Some('@') => (Line::Hunk, l.clone()),
            Some('+') => (Line::Added, l[1..].to_owned()),
            Some('-') => (Line::Removed, l[1..].to_owned()),
            Some(' ') => (Line::Context, l[1..].to_owned()),
            _ => (Line::Context, l.clone()),
        })
        .collect()
}

fn copy_button(t: &Theme, key: String, name: &str, icon: &'static rune::Icon, words: String) -> El {
    let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(words.clone()));
    icon_button(t, &key, name, icon, None, false, Some(press))
}

/// An open file's foot items (`Diff::foot`): a note (moved from, mode, cut), then Show all, Copy diff, Copy path, Reveal in Finder.
fn foot(t: &Theme, key: &str, root: &str, f: &DiffFile, patch: Option<&DiffPatch>) -> El {
    let path = format!("{}/{}", root.trim_end_matches('/'), f.path);
    let mut notes = Vec::new();
    if let (Some(old), FileStatus::Renamed) = (&f.old_path, f.status) {
        notes.push(format!("Moved from {old}."));
    }
    if !f.binary && let Some(n) = file_note(f) {
        notes.push(n);
    }
    let cut = patch.filter(|p| p.cut);
    if let Some(p) = cut {
        notes.push(format!("Showing {} of {} lines.", thousands(p.lines.len() as u64), thousands(p.total as u64)));
    }
    let mut r = row().key(format!("{key}-foot-items")).w_full().gap(t.space.at(1.5)).center_y();
    r = r.child(text(notes.join(" ")).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).grow(1.0).shrink().selectable(true));
    if cut.is_some() {
        let p = f.path.clone();
        r = r.child(Button::new(format!("{key}-all"), "Show all").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(move |s: &mut App| s.review_all(&p)).build(t));
    }
    if let Some(p) = patch.filter(|p| !p.lines.is_empty()) {
        r = r.child(copy_button(t, format!("{key}-copy-diff"), "Copy diff", &icons::FILE_CODE, p.lines.join("\n")));
    }
    r = r.child(copy_button(t, format!("{key}-copy-path"), "Copy path", &icons::COPY, path.clone()));
    let deleted = f.status == FileStatus::Deleted;
    let reveal: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| {
        let p = std::path::Path::new(&path);
        let at = if p.exists() { p.to_path_buf() } else { p.parent().map(std::path::Path::to_path_buf).unwrap_or_default() };
        let _ = tx.reveal_path(&at);
    });
    r.child(icon_button(t, &format!("{key}-reveal"), if deleted { "Reveal its folder in Finder" } else { "Reveal in Finder" }, &icons::FOLDER_OPEN, None, false, Some(reveal)))
}

/// The sources of the Changes tab, in the order of its switch.
const SOURCES: [DiffSource; 3] = [DiffSource::Run, DiffSource::Thread, DiffSource::Uncommitted];

/// What the Changes tab says is not there, for a source with nothing to show.
fn nothing(source: DiffSource) -> &'static str {
    match source {
        DiffSource::Run => "This run changed nothing.",
        DiffSource::Thread => "Nothing changed in this thread.",
        DiffSource::Uncommitted => "Nothing is waiting to be committed.",
    }
}

/// The Changes tab of the right pane: the source switch, the files as rune-ui's Diff (folded until opened, their lines asked for when they
/// open), and under them Undo, Redo and Copy diff.
pub fn changes_tab(s: &App, t: &Theme, root: &TaskId) -> El {
    let r = s.review.as_ref().filter(|r| &r.task == root);
    let source = r.map_or(DiffSource::Thread, |r| r.source);
    let at = SOURCES.iter().position(|x| *x == source).unwrap_or(1);
    let switch = SegmentedControl::<App>::new("changes-source", "Show changes of", &SHOW, at)
        .size(Size::Sm)
        .on_change(|s: &mut App, i| s.set_changes_source(SOURCES.get(i).copied().unwrap_or(DiffSource::Thread)))
        .build(t);
    let mut page = col().key("changes").w_full().gap(t.space.at(3.0)).child(switch);
    let book = s.books.get(root);
    // Whether the run's changes are in the folder now.
    if let (Some(r), Some(b)) = (r, book)
        && let (DiffSource::Run, Some(run)) = (r.source, &r.run)
        && let Some(i) = b.runs.iter().position(|x| &x.id == run)
        && undo::offers(&b.runs)[i].undone
    {
        page = page.child(text("Undone: these changes are not in the folder now.").key("changes-undone").role("status").font(t.ty.base).color(t.ink_muted()));
    }
    match r.map(|r| (&r.summary, &r.error)) {
        None | Some((None, None)) => page = page.child(text("Reading the changes\u{2026}").key("changes-loading").role("status").font(t.ty.base).color(t.ink_faint())),
        Some((_, Some(e))) => page = page.child(text(e.clone()).key("changes-error").role("status").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0)),
        Some((Some(d), None)) if d.files.is_empty() => page = page.child(text(nothing(source)).key("changes-empty").role("status").font(t.ty.base).color(t.ink_muted())),
        Some((Some(d), None)) => {
            let r = r.expect("a summary comes with its review");
            let n = d.files.len();
            let summary = format!("{n} file{} changed, {} line{} added, {} removed", if n == 1 { "" } else { "s" }, d.added, if d.added == 1 { "" } else { "s" }, d.removed);
            page = page.child(
                row()
                    .key("changes-summary")
                    .role("note")
                    .label(summary)
                    .gap(t.space.at(2.0))
                    .center_y()
                    .child(text(format!("{n} file{}", if n == 1 { "" } else { "s" })).font(t.ty.sm).color(t.ink_muted()).a11y_hidden())
                    .child(text(format!("+{}", d.added)).font(rune_ui_style::tabular(t.ty.sm)).color(t.c.success.fg).a11y_hidden())
                    .child(text(format!("\u{2212}{}", d.removed)).font(rune_ui_style::tabular(t.ty.sm)).color(t.c.danger.fg).a11y_hidden()),
            );
            let mut list = col().key("changes-files").w_full().gap(t.space.at(2.0));
            for (i, f) in d.files.iter().enumerate() {
                let key = format!("diff-{i}");
                let open = r.open.contains(&f.path);
                let patch = r.patches.get(&f.path);
                let lines = file_lines(f, open, patch);
                let p = f.path.clone();
                let file = FileDiff { path: file_label(f), lines };
                let diff = Diff::<App>::new(key.clone(), &file, open).stats(f.added as usize, f.removed as usize).hide_zero(true).selectable(true).foot(foot(t, &key, &d.root, f, patch)).on_toggle(move |s: &mut App| s.review_flip(&p));
                list = list.child(diff.build(t));
            }
            page = page.child(list);
            if d.files.iter().any(|f| r.patches.get(&f.path).is_some_and(|p| p.cut)) || d.files.iter().any(|f| (f.added + f.removed) as usize > CUT_LINES) {
                page = page.child(text(format!("Files with more than {} changed lines show the first {} until Show all.", thousands(CUT_LINES as u64), thousands(CUT_LINES as u64))).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
            }
        }
    }
    let scroll = col().key("changes-scroll").w_full().grow(1.0).min_h(0.0).px(t.space.at(4.0)).pt(t.space.at(3.0)).pb(t.space.at(6.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page);
    col().key("changes-tab").w_full().grow(1.0).min_h(0.0).child(scroll).child(footer(s, t, root, r))
}

/// Under the files: Undo or Redo of the run shown (or the newest that offers one), and Copy diff. A quiet line says why Undo is off.
fn footer(s: &App, t: &Theme, root: &TaskId, r: Option<&Review>) -> El {
    let shown_run = r.filter(|r| r.source == DiffSource::Run).and_then(|r| r.run.clone());
    let offer = crate::pane_view::offered_for(s, root, shown_run.as_ref());
    let mut bar = row().key("changes-foot").w_full().px(t.space.at(4.0)).py(t.space.at(2.0)).gap(t.space.at(2.0)).center_y().flex_wrap().border_sides([0.5, 0.0, 0.0, 0.0], t.c.line);
    if let Some((run, offer)) = offer {
        if let Some((action, enabled)) = offer.action {
            let root = root.clone();
            let (words, icon, redo, key) = match action {
                Action::Undo => ("Undo", &icons::RESTORE, false, "changes-undo"),
                Action::Redo => ("Redo", &icons::RETRY, true, "changes-redo"),
            };
            bar = bar.child(Button::<App>::new(key, words).variant(ButtonVariant::Secondary).size(Size::Sm).icon(icon).disabled(!enabled).on_press(move |s: &mut App| s.undo_run(&root, &run, redo)).build(t));
        }
    }
    bar = bar.child(el().grow(1.0));
    let any = r.is_some_and(|r| r.summary.as_ref().is_some_and(|d| !d.files.is_empty()));
    bar.child(Button::<App>::new("changes-copy", "Copy diff").variant(ButtonVariant::Ghost).size(Size::Sm).icon(&icons::COPY).disabled(!any).on_press(|s: &mut App| s.copy_all_diff()).build(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, status: FileStatus) -> DiffFile {
        DiffFile { path: path.into(), old_path: None, status, added: 2, removed: 1, binary: false, old_size: None, new_size: None, old_mode: None, new_mode: None }
    }

    #[test]
    fn a_file_reads_as_its_path_its_lines_or_a_line_about_it() {
        let mut moved = file("docs/start.md", FileStatus::Renamed);
        moved.old_path = Some("docs/guide.md".into());
        assert_eq!(file_label(&moved), "docs/guide.md → docs/start.md");
        let folded = file_lines(&moved, false, None);
        assert!(folded.is_empty(), "folded, no lines: the header's counts are Diff::stats");
        let patch = DiffPatch { path: "a".into(), lines: vec!["@@ -1 +1 @@".into(), "-old".into(), "+new".into(), " same".into(), "\\ No newline at end of file".into()], total: 5, cut: false };
        let lines = file_lines(&file("a", FileStatus::Modified), true, Some(&patch));
        assert_eq!(lines[0], (Line::Hunk, "@@ -1 +1 @@".into()));
        assert_eq!(lines[1], (Line::Removed, "old".into()));
        assert_eq!(lines[2], (Line::Added, "new".into()));
        assert_eq!(lines[3], (Line::Context, "same".into()));
        let mut logo = file("logo.png", FileStatus::Modified);
        logo.binary = true;
        (logo.old_size, logo.new_size) = (Some(2_400), Some(18_000));
        assert_eq!(file_lines(&logo, true, None), vec![(Line::Context, "Binary file, 2.4 KB before, 18.0 KB now".into())], "a binary file is named with its sizes, never dumped");
        let mut script = file("run.sh", FileStatus::Modified);
        (script.old_mode, script.new_mode) = (Some("100644".into()), Some("100755".into()));
        assert_eq!(file_note(&script).as_deref(), Some("Now executable."));
        assert_eq!(thousands(4_210), "4,210");
    }
}
