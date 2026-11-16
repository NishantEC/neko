//! The Diff tab: the whole task's diff, first start to last end, in the kit's `DiffView`, with a list of
//! its files on top (path, then +N −M) whose rows scroll the diff to that file, and a "Run N" select
//! that shows one run's diff instead. It reads what the session view's runs already read (`CheckpointDiff`).

use rune::{El, ScrollSpec, col, row, text};
use rune_kit::diff::{DiffView, LineKind};

use super::{placeholder, ui};
use crate::app::App;
use crate::theme::Neko;
use crate::work::{DiffLoad, diff_key};

/// The most the file list takes before it scrolls: five rows.
const LIST_ROWS: f32 = 5.0;
const ROW_H: f32 = 26.0;

/// The run select's options: the whole task, then each run.
pub fn run_options(runs: &[u32]) -> Vec<String> {
    std::iter::once("Whole task".to_owned()).chain(runs.iter().map(|n| format!("Run {n}"))).collect()
}

/// (added, removed) lines of each file of a diff.
pub fn counts(file: &rune_kit::diff::FileDiff) -> (usize, usize) {
    let lines = file.hunks.iter().flat_map(|h| h.lines.iter());
    lines.fold((0, 0), |(a, r), l| match l.kind {
        LineKind::Added => (a + 1, r),
        LineKind::Removed => (a, r + 1),
        LineKind::Context => (a, r),
    })
}

pub fn tab(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let Some(id) = s.selected.clone() else { return placeholder(&["Nothing to show yet."], t) };
    let run = s.panel.session(&id).and_then(|p| p.run);
    let key = diff_key(&id, run);
    // The select: the whole task, or one run.
    let runs: Vec<u32> = s.work.checkpoints.get(&id).map(|l| l.runs.iter().map(|r| r.n).collect()).unwrap_or_default();
    let options = run_options(&runs);
    let current = run.map_or("Whole task".to_owned(), |n| format!("Run {n}"));
    let select = ui::pick(s, t, layer, "context-run", "Diff of", &options, &current, 140.0, |s: &mut App, chosen: &str| {
        s.panel_pick_run(chosen.strip_prefix("Run ").and_then(|n| n.parse().ok()));
    });
    let mut head = row().key("diff-head").w_full().h(32.0).px(12.0).gap(8.0).center_y().child(select);
    let body = match s.work.diff_load.get(&key) {
        Some(DiffLoad::Failed(message)) => placeholder(&[message.as_str()], t),
        Some(DiffLoad::Ready { truncated }) => match s.work.diffs.get(&key) {
            Some(diff) if diff.files().is_empty() => placeholder(&["This changed no files."], t),
            Some(diff) => {
                let (added, removed) = diff.files().iter().map(counts).fold((0, 0), |(a, r), (x, y)| (a + x, r + y));
                head = head.child(text(format!("{} {} · +{added} −{removed}", diff.files().len(), if diff.files().len() == 1 { "file" } else { "files" })).font(t.ty.meta).color(t.text.muted).a11y_hidden());
                let mut list = col().key("diff-files").w_full().px(8.0).pb(4.0);
                for (i, file) in diff.files().iter().enumerate() {
                    let (a, r) = counts(file);
                    let (path, k) = (file.path().to_owned(), key.clone());
                    list = list.child(
                        row()
                            .key(format!("diff-file-{i}"))
                            .role("button")
                            .label(format!("{path}, {a} added, {r} removed"))
                            .focusable(true)
                            .w_full()
                            .h(ROW_H)
                            .px(8.0)
                            .gap(8.0)
                            .center_y()
                            .radius(t.radius.row)
                            .hover(t.surface.hover)
                            .focus_ring(2.0, 0.0, t.kit.c.border_focus)
                            .on_press_tx(move |s: &mut App, tx| {
                                if let Some(d) = s.work.diffs.get_mut(&k) {
                                    d.scroll_to_file(i, tx);
                                }
                            })
                            .child(text(path).font(t.ty.mono_meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden())
                            .child(text(format!("+{a}")).font(t.ty.mono).color(t.meaning.mint).a11y_hidden())
                            .child(text(format!("−{r}")).font(t.ty.mono).color(t.meaning.coral).a11y_hidden()),
                    );
                }
                let (a, b) = (key.clone(), key.clone());
                let view = DiffView::new(move |s: &App| s.work.diffs.get(&a).unwrap_or(&s.work.empty_diff), move |s: &mut App| s.work.diffs.entry(b.clone()).or_insert_with(|| rune_kit::diff::DiffState::new(Vec::new())))
                    .key(format!("panel-diff-{key}"))
                    .build(s, &t.kit);
                let mut c = col().w_full().grow(1.0).min_h(0.0).child(col().key("diff-scroll").w_full().max_h(LIST_ROWS * ROW_H + 8.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
                c = c.child(col().key("diff-view").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).child(view));
                if *truncated {
                    c = c.child(text("The diff is long, so it is cut short here.").font(t.ty.meta).color(t.text.faint).px(12.0).py(6.0));
                }
                c
            }
            None => placeholder(&["Loading the diff…"], t),
        },
        _ => placeholder(&["Loading the diff…"], t),
    };
    col().w_full().grow(1.0).min_h(0.0).child(head).child(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_select_offers_the_whole_task_then_each_run() {
        assert_eq!(run_options(&[1, 2, 3]), ["Whole task", "Run 1", "Run 2", "Run 3"]);
        assert_eq!(run_options(&[]), ["Whole task"]);
    }
}
