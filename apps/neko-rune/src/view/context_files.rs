//! The Files tab: the session's working folder as the kit's `FileTree`, with changed files marked by a
//! 6 point Amber dot. A file opens in the viewer the registry picks for it, inside the tab, with a back
//! arrow. The panel reads the folder itself, read-only and confined to it (`readers.rs`).

use rune::{El, col, icon, row, text};
use rune_kit::{FileNode, Tree, TreeItem};

use super::context::viewer;
use super::placeholder;
use crate::app::App;
use crate::icons;
use crate::panel::Target;
use crate::theme::Neko;

pub fn tab(s: &App, t: &Neko) -> El {
    let Some(id) = s.selected.clone() else { return placeholder(&["Nothing to show yet."], t) };
    if let Some(path) = s.panel.session(&id).and_then(|p| p.file.clone()) {
        let name = path.clone();
        let back = row()
            .key("files-back")
            .role("button")
            .label("Back to the files")
            .focusable(true)
            .h(32.0)
            .px(8.0)
            .gap(6.0)
            .center_y()
            .radius(t.radius.control)
            .hover(t.surface.hover)
            .focus_ring(2.0, 0.0, t.kit.c.border_focus)
            .on_press(|s: &mut App| s.panel_close_file())
            .child(icon(&icons::CHEVRON_LEFT, 16.0).color(t.text.muted).a11y_hidden())
            .child(text(name).font(t.ty.mono_meta).color(t.text.muted).truncate().min_w(0.0).a11y_hidden());
        return col().w_full().grow(1.0).min_h(0.0).child(row().key("files-head").w_full().px(8.0).child(back)).child(viewer(s, t, &Target::File { task_id: id, path }));
    }
    let Some(meta) = s.panel.files_meta.as_ref().filter(|m| m.task_id == id) else { return placeholder(&["Reading the folder…"], t) };
    if s.panel.files_tree.is_empty() {
        return placeholder(&["This folder has no files."], t);
    }
    let amber = t.meaning.amber;
    let tree = Tree::new(|s: &App| &s.panel.files_tree, |s: &mut App| &mut s.panel.files_tree, FileNode::label)
        .row(move |n: &FileNode<bool>, r| {
            let item = TreeItem::file(n, r);
            if n.data == Some(true) { item.dot(amber) } else { item }
        })
        .on_select(|s: &mut App, id: &str, _| {
            let is_file = s.panel.files_tree.get(id).is_some_and(|n| !n.dir);
            if is_file {
                s.panel_open_file(id);
            }
        })
        .name("Files")
        .build(s, &t.kit);
    let mut c = col().key("files-tree").w_full().grow(1.0).min_h(0.0).px(4.0).child(col().w_full().grow(1.0).min_h(0.0).child(tree));
    if meta.truncated {
        c = c.child(text(format!("Showing the first {} files.", super::super::viewers::table::group_digits(meta.count))).font(t.ty.id).color(t.text.faint).px(12.0).py(6.0));
    }
    c
}
