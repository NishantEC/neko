//! Settings ▸ Projects: the folders the person added, each with its folder and how many threads it has, and what can be done to it
//! (docs/knowledge/workspaces.md).

use std::rc::Rc;

use rune::{El, col, row, text};
use rune_ui_style::{ButtonVariant, Size, weight};

use super::{Found, note, section};
use crate::state::App;
use crate::ui::badge::Badge;
use crate::ui::base::{Shape, Theme, Tone, paint};
use crate::ui::button::Button;

fn card(s: &App, t: &Theme, w: &neko_protocol::Workspace) -> El {
    let key = format!("settings-ws-{}", w.id);
    let n = s.tasks_in(&w.id).len();
    let count = match n {
        0 => "No threads yet".to_owned(),
        1 => "1 thread".to_owned(),
        n => format!("{n} threads"),
    };
    let folder = match &w.folder {
        Some(f) => crate::task_view::show_path(f, "", s.home.as_deref()),
        None => "No folder: each thread gets a scratch folder of its own".to_owned(),
    };
    let mut head = row().w_full().gap(t.space.at(2.0)).center_y().child(text(w.name.clone()).role("heading").level(4).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink());
    if w.missing {
        head = head.child(Badge::new(format!("{key}-missing"), "Folder not found").tone(Tone::Warning).dot().build(t));
    }
    let facts = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(t.space.at(1.0))
        .child(head)
        .child(text(folder).key(format!("{key}-folder")).font(t.ty.sm).color(t.ink_muted()).truncate_middle().min_w(0.0).help(w.folder.clone().unwrap_or_default()))
        .child(text(count).key(format!("{key}-count")).font(t.ty.sm).color(t.ink_faint()));
    let mut card = paint(row().key(key.clone()).role("group").label(w.name.clone()).w_full().padding(t.metrics.inset).gap(t.space.at(3.0)).center_y().radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(facts);
    if !w.is_general() {
        let mut actions = row().gap(t.space.at(1.5)).center_y();
        let id = w.id.clone();
        actions = actions.child(Button::new(format!("{key}-rename"), "Rename…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.start_rename(&id)).build(t));
        if w.missing {
            let id = w.id.clone();
            let locate: Rc<dyn Fn(&mut App, &mut rune::Tx)> = Rc::new(move |s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, crate::state::Pick::Locate(id.clone()), tx));
            actions = actions.child(Button::<App>::new(format!("{key}-locate"), "Locate…").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| locate(s, tx)));
        } else if let Some(f) = w.folder.clone() {
            actions = actions.child(Button::<App>::new(format!("{key}-reveal"), "Reveal in Finder").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(move |_: &mut App, tx: &mut rune::Tx| {
                let _ = tx.reveal_path(&f);
            }));
        }
        let id = w.id.clone();
        actions = actions.child(Button::new(format!("{key}-remove"), "Remove…").variant(ButtonVariant::Secondary).size(Size::Sm).on_press(move |s: &mut App| s.ask_remove_workspace(&id)).build(t));
        card = card.child(actions);
    }
    card
}

pub fn page(s: &App, t: &Theme, f: &Found) -> El {
    let mut part = super::lead(
        t,
        "settings-workspaces",
        "Folders you added. A new thread works in the one you choose; in No project each thread gets a scratch folder of its own. Removing a project takes it off the list only: nothing on disk is touched.",
    );
    for w in s.workspaces.iter().filter(|w| w.is_active()) {
        part = part.child(f.lit(t, &format!("settings-ws-{}", w.id), card(s, t, w)));
    }
    let add = Button::<App>::new("settings-add-workspace", "Add project…").variant(ButtonVariant::Secondary).size(Size::Sm).build(t).on_press_tx(|s: &mut App, tx: &mut rune::Tx| crate::pick_folder(s, crate::state::Pick::Add { select: false }, tx));
    part = part.child(row().child(f.lit_hug(t, "settings-add-workspace", add)));
    let removed: Vec<&str> = s.workspaces.iter().filter(|w| !w.is_active()).map(|w| w.name.as_str()).collect();
    if !removed.is_empty() {
        part = part.child(note(t, "settings-ws-removed", format!("Removed: {}. Their threads stay readable in the sidebar. Add a folder again to bring its project back.", removed.join(", "))));
    }
    let branches = section(t, "settings-branches", "Worktree branches", None).child(note(
        t,
        "settings-branches-words",
        "In a git project each new thread works in a worktree of its own, on a branch named neko/ and the thread's short id. Your own branches are never touched. Choose \u{201c}In the folder\u{201d} on New thread to work in the project's own folder instead.",
    ));
    col().w_full().gap(t.space.at(8.0)).child(part).child(f.lit(t, "settings-branches", branches))
}
