//! Review (docs/knowledge/undo.md): what a run changed, or the whole task ("All changes"), in place of the conversation. Each file
//! is rune-ui's Diff, folded until opened; its lines come from the daemon when it opens. Binary files are named with their sizes,
//! very long diffs are cut with "Show all", and an open file has Copy diff, Copy path and Reveal in Finder.

use std::rc::Rc;

use neko_protocol::undo::{self, CUT_LINES};
use neko_protocol::{DiffFile, DiffPatch, FileStatus};
use rune::{El, Tx, col, row, text};
use rune_ui_style::weight;

use crate::state::{App, Review};
use crate::ui::agent::{PressTx, icon_button};
use crate::ui::base::{ButtonVariant, Size, Theme};
use crate::ui::button::Button;
use crate::ui::diff::{Diff, FileDiff, Line};
use crate::ui::icons;
use crate::ui::segmented_control::SegmentedControl;
use crate::view::COLUMN;

/// "This run" or the whole task.
const SHOW: [&str; 2] = ["This run", "All changes"];

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

/// The lines rune-ui's Diff draws. Folded, only its counts matter (the header shows them), so it gets that many marks; open, the
/// file's own lines, or one line saying what there is instead.
pub fn file_lines(f: &DiffFile, open: bool, patch: Option<&DiffPatch>) -> Vec<(Line, String)> {
    if !open {
        let mut v = vec![(Line::Added, String::new()); f.added as usize];
        v.extend(vec![(Line::Removed, String::new()); f.removed as usize]);
        return v;
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

/// An open file's foot, inside its slab: a note (moved from, mode, cut), then Show all, Copy diff, Copy path, Reveal in Finder.
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
    let mut r = row().key(format!("{key}-foot")).w_full().px(t.space.at(3.0)).py(t.space.at(1.5)).gap(t.space.at(1.5)).center_y().border_sides([1.0, 0.0, 0.0, 0.0], t.c.line);
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

/// The review, in place of the conversation.
pub fn review_view(s: &App, t: &Theme, r: &Review) -> El {
    let back = Button::new("review-back", "Back to the conversation").variant(ButtonVariant::Ghost).size(Size::Sm).icon(&icons::CHEVRON_LEFT).on_press(|s: &mut App| s.close_review()).build(t);
    let mut top = row().w_full().gap(t.space.at(3.0)).center_y().child(back).child(row().grow(1.0));
    if let Some(from) = r.from.clone() {
        let task = r.task.clone();
        top = top.child(
            SegmentedControl::<App>::new("review-show", "Show", &SHOW, usize::from(r.run.is_none()))
                .size(Size::Sm)
                .on_change(move |s: &mut App, i| s.open_review(&task, if i == 0 { Some(from.clone()) } else { None }))
                .build(t),
        );
    }
    let heading = if r.run.is_some() { "Changes in this run" } else { "All changes in this task" };
    let mut page = col().key("review").w_full().max_w(COLUMN).gap(t.space.at(3.0)).child(top).child(text(heading).key("review-title").role("heading").level(2).font(weight(t.ty.lg, t.ty.medium)).color(t.ink()));
    // Whether the run's changes are in the folder now.
    let book = s.books.get(&r.task);
    if let (Some(run), Some(b)) = (&r.run, book)
        && let Some(i) = b.runs.iter().position(|x| &x.id == run)
        && undo::offers(&b.runs)[i].undone
    {
        page = page.child(text("Undone: these changes are not in the folder now.").key("review-undone").role("status").font(t.ty.base).color(t.ink_muted()));
    }
    match (&r.summary, &r.error) {
        (_, Some(e)) => page = page.child(text(e.clone()).key("review-error").role("status").font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0)),
        (None, None) => page = page.child(text("Reading the changes…").key("review-loading").role("status").font(t.ty.base).color(t.ink_faint())),
        (Some(d), None) if d.files.is_empty() => page = page.child(text("Nothing changed.").key("review-empty").role("status").font(t.ty.base).color(t.ink_muted())),
        (Some(d), None) => {
            let n = d.files.len();
            let summary = format!("{n} file{} changed, {} line{} added, {} removed", if n == 1 { "" } else { "s" }, d.added, if d.added == 1 { "" } else { "s" }, d.removed);
            page = page.child(
                row()
                    .key("review-summary")
                    .role("note")
                    .label(summary)
                    .gap(t.space.at(2.0))
                    .center_y()
                    .child(text(format!("{n} file{}", if n == 1 { "" } else { "s" })).font(t.ty.sm).color(t.ink_muted()).a11y_hidden())
                    .child(text(format!("+{}", d.added)).font(rune_ui_style::tabular(t.ty.sm)).color(t.c.success.fg).a11y_hidden())
                    .child(text(format!("\u{2212}{}", d.removed)).font(rune_ui_style::tabular(t.ty.sm)).color(t.c.danger.fg).a11y_hidden()),
            );
            let mut list = col().key("review-files").w_full().gap(t.space.at(2.0));
            for (i, f) in d.files.iter().enumerate() {
                let key = format!("diff-{i}");
                let open = r.open.contains(&f.path);
                let patch = r.patches.get(&f.path);
                let fd = FileDiff { path: file_label(f), lines: file_lines(f, open, patch) };
                let p = f.path.clone();
                let mut el = Diff::<App>::new(key.clone(), &fd, open).on_toggle(move |s: &mut App| s.review_flip(&p)).build(t);
                if open {
                    el = el.child(foot(t, &key, &d.root, f, patch));
                }
                list = list.child(el);
            }
            page = page.child(list);
            if d.files.iter().any(|f| r.patches.get(&f.path).is_some_and(|p| p.cut)) || d.files.iter().any(|f| (f.added + f.removed) as usize > CUT_LINES) {
                page = page.child(text(format!("Files with more than {} changed lines show the first {} until Show all.", thousands(CUT_LINES as u64), thousands(CUT_LINES as u64))).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
            }
        }
    }
    col().key("review-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.metrics.inset).pt(t.space.at(4.0)).pb(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
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
        assert_eq!((folded.iter().filter(|l| l.0 == Line::Added).count(), folded.iter().filter(|l| l.0 == Line::Removed).count()), (2, 1), "folded, the header's counts are the file's");
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
