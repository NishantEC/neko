//! Settings ▸ Storage: what Neko keeps on disk for the person: undo's checkpoints (with Clear), the worktrees it keeps, and the
//! archived threads (docs/knowledge/undo.md, docs/knowledge/workspaces.md, docs/knowledge/threads.md).

use rune::{El, col, row, text};
use rune_ui_style::{ButtonVariant, Size};

use super::{Found, note, section};
use crate::state::App;
use crate::ui::base::Theme;
use crate::ui::button::Button;

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    col().w_full().gap(t.space.at(8.0)).child(f.lit(t, "settings-undo", undo_storage(s, t))).child(f.lit(t, "settings-worktrees", worktrees(t))).child(f.lit(t, "settings-archived", archived(s, t)))
}

/// "Undo history": the shadow repositories' bytes and the checkpoints kept, with "Clear undo history…" (it asks first).
fn undo_storage(s: &App, t: &Theme) -> El {
    let words = match s.undo_storage {
        None => "Storage used by undo: reading…".to_owned(),
        Some(u) if u.checkpoints == 0 => "Storage used by undo: none. No run has a checkpoint now.".to_owned(),
        Some(u) => format!(
            "Storage used by undo: {} in Neko's folder, {} checkpoint{} in {} thread{}. Checkpoints of a git repository are kept in the repository itself.",
            crate::review_view::size_words(u.bytes),
            u.checkpoints,
            if u.checkpoints == 1 { "" } else { "s" },
            u.tasks,
            if u.tasks == 1 { "" } else { "s" }
        ),
    };
    let none = s.undo_storage.is_none_or(|u| u.checkpoints == 0 && u.bytes == 0);
    let clear = Button::new("settings-clear-undo", "Clear undo history…").variant(ButtonVariant::Secondary).size(Size::Sm).disabled(none).on_press(|s: &mut App| s.ask_clear_undo()).build(t);
    section(t, "settings-undo", "Undo history", Some("Each run's start and end is a checkpoint, so Review, Undo and Redo can put files back."))
        .child(text(words).key("settings-undo-storage").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0))
        .child(row().child(clear))
}

/// "Worktrees": what Neko keeps and what it tidies, in words.
fn worktrees(t: &Theme) -> El {
    section(t, "settings-worktrees", "Worktrees", None).child(note(
        t,
        "settings-worktrees-words",
        "A thread in a git project works in a worktree in Neko's own folder. After each run one that changed nothing is removed; one with commits or work not yet committed is kept until you remove its thread. A kitten's own copy follows the same rule.",
    ))
}

/// "Archived threads": how many sleep, and where they are.
fn archived(s: &App, t: &Theme) -> El {
    let n = s.tasks.iter().filter(|x| x.is_thread() && x.archived).count();
    let words = match n {
        0 => "No archived threads.".to_owned(),
        1 => "1 archived thread, kept as it was.".to_owned(),
        n => format!("{n} archived threads, kept as they were."),
    };
    section(t, "settings-archived", "Archived threads", None)
        .child(text(words).key("settings-archived-count").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0))
        .child(note(t, "settings-archived-words", "Search (⌘K) finds them, and General's \u{201c}Show archived threads\u{201d} lists them in the sidebar. Restore one from its menu, or write to it to wake it."))
}
