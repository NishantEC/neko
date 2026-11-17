//! Setup, Profiles: named sets of instructions and memories for different kinds of work. Save a profile,
//! assign it to a workspace, make it the default, and let one profile read another's general memories (which
//! never shares tools or workspace access). Ported from `pages/setup/Profiles.tsx`.

use neko_protocol::agent_profiles::AgentProfile;
use rune::{El, col, row, text};

use super::Page;
use super::kit::{self, action, details_block, empty, list_row};
use crate::app::App;
use crate::icons;
use crate::link::SetupCall;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::view::{primary_button, quiet_button};

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "profiles-none", "Looking for your profiles…") };
    let new = primary_button("New profile", "profile-new", t, |s: &mut App| s.open_profile_dialog(None));
    let mut body = col().w_full().gap(2.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    let profiles = &data.profiles.profiles;
    if profiles.is_empty() {
        body = body.child(empty(t, "profiles-empty", "No profiles. A profile bundles instructions and memories for a kind of work.", Some(("New profile", Box::new(|s: &mut App| s.open_profile_dialog(None))))));
    }
    for p in profiles {
        body = body.child(profile_row(s, t, layer, p));
    }
    Page { count: (!profiles.is_empty()).then_some(profiles.len()), trailing: vec![new], body, title: None, back: None }
}

fn profile_row(s: &App, t: &Neko, layer: &mut Vec<El>, p: &AgentProfile) -> El {
    let Some(data) = s.setup_data() else { return col() };
    let info = &data.profiles;
    let id = p.id.clone();
    let key = format!("profile:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let active = info.active_profile_id == p.id;
    let used: Vec<String> = info.assignments.iter().filter(|a| a.profile_id == p.id).filter_map(|a| data.workspace(&a.workspace_id)).map(|w| w.name.clone()).collect();
    let mut line = list_row(t, format!("profile-{id}"), format!("{}{}", p.name, if active { ", default" } else { "" }), Some(open))
        .on_press(move |s: &mut App| s.toggle_row(&toggle))
        .child(text(p.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(220.0).a11y_hidden());
    if active {
        line = line.child(text("Default".to_owned()).key(format!("profile-default-{id}")).font(t.ty.meta).color(t.meaning.iris).a11y_hidden());
    }
    line = line.child(text(if p.instructions.trim().is_empty() { "No special instructions".to_owned() } else { p.instructions.lines().next().unwrap_or("").to_owned() }).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if !used.is_empty() {
        line = line.child(text(format!("Used in {}", used.join(", "))).key(format!("profile-used-{id}")).font(t.ty.meta).color(t.text.faint).truncate().min_w(0.0).max_w(220.0).a11y_hidden());
    }
    let (d, e) = (id.clone(), id.clone());
    if !active {
        line = line.child(action(t, format!("profile-default-btn-{id}"), "Make default", &icons::CHECK_LINE, move |s: &mut App| s.setup_call(SetupCall::ActivateProfile { id: d.clone() })));
    }
    line = line.child(action(t, format!("profile-edit-{id}"), "Edit", &icons::PENCIL, move |s: &mut App| s.open_profile_dialog(Some(&e))));

    // The right-click menu: edit, make default, and "Use in <workspace>" for each.
    let mut entries: Vec<MenuEntry> = Vec::new();
    let me = id.clone();
    entries.push(("Edit…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_profile_dialog(Some(&me)))));
    if !active {
        let md = id.clone();
        entries.push(("Make default".to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupCall::ActivateProfile { id: md.clone() }))));
    }
    for w in &data.workspaces {
        let (ws, pid) = (w.id.clone(), id.clone());
        entries.push((format!("Use in {}", w.name), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupCall::AssignProfile { workspace_id: ws.clone(), profile_id: pid.clone() }))));
    }
    let mut out = col().key(format!("profile-block-{id}")).w_full().child(with_menu(s, t, layer, &key, line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    if open {
        let mut c = details_block(format!("profile-details-{id}"));
        c = c.child(text(if p.instructions.trim().is_empty() { "No special instructions. Edit this profile to add a voice or working rules.".to_owned() } else { p.instructions.clone() }).key(format!("profile-instructions-{id}")).font(t.ty.row).color(if p.instructions.trim().is_empty() { t.text.muted } else { t.text.base }).wrap().px(8.0).min_w(0.0));
        if !data.workspaces.is_empty() {
            let mut uses = row().key(format!("profile-uses-{id}")).w_full().gap(6.0).px(8.0).wrap().center_y().child(text("Use in".to_owned()).font(t.ty.meta).color(t.text.muted));
            for w in &data.workspaces {
                let assigned = info.owner(&w.id) == p.id;
                let (ws, pid) = (w.id.clone(), id.clone());
                let b = quiet_button(&w.name, &format!("profile-use-{id}-{}", w.id), t, move |s: &mut App| s.setup_call(SetupCall::AssignProfile { workspace_id: ws.clone(), profile_id: pid.clone() }));
                uses = uses.child(if assigned { b.fill(t.surface.selected).help("Used in this workspace") } else { b });
            }
            c = c.child(uses);
        }
        let others: Vec<&AgentProfile> = info.profiles.iter().filter(|o| o.id != p.id).collect();
        if !others.is_empty() {
            c = c.child(kit::words(t, &format!("profile-grants-note-{id}"), "This only shares memories. It never shares tools or workspace access."));
        }
        for other in others {
            let granted = info.read_grants.iter().any(|g| g.reader_id == p.id && g.source_id == other.id);
            let (r, o) = (id.clone(), other.id.clone());
            c = c.child(row().w_full().px(8.0).min_h(28.0).center_y().child(kit::check(t, &format!("profile-grant-{id}-{}", other.id), &format!("Can read {}’s general memories", other.name), granted, move |s: &mut App| s.setup_call(SetupCall::ReadGrant { reader: r.clone(), source: o.clone(), allowed: !granted }))));
        }
        out = out.child(c);
    }
    out
}
