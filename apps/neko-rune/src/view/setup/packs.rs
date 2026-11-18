//! Setup, Packs: what is installed from a pack (a bundle of tools, skills, a plugin and routines). One 32 point row per pack
//! (health glyph, title, version, where it came from) with Check health, Update (only when a newer version is known) and Remove
//! (after a confirmation); a row opens its detail inside the section: permissions, contents with links to the sections that show
//! them, requirements with the install hint and a Copy button, and the last health output. "Add pack" is a dialog
//! (`pack_dialog.rs`). The answers are the daemon's `PackListing`, not the workbench.

use neko_protocol::packs::{InstalledPack, PackInfo, PackStatus};
use rune::{El, col, icon, row, text};

use super::Page;
use super::kit::{self, details_block, glyph};
use super::state::SetupSection;
use super::words;
use crate::app::App;
use crate::icons;
use crate::icons_phosphor as ph;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::view::webkit;
use crate::view::{primary_button, quiet_button};
use crate::views::FullView;

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(listing) = s.packs() else {
        let mut body = col().w_full().gap(2.0);
        if let Some(n) = note(s, t, "setup:page") {
            body = body.child(n);
        }
        return Page { count: None, trailing: Vec::new(), body: body.child(super::super::ui::quiet_line(t, "packs-none", "Looking for your packs…")), title: None, back: None };
    };
    if let Some(p) = s.setup.pack.as_deref().and_then(|name| listing.installed.iter().find(|p| p.info.name == name)) {
        return detail(s, t, layer, p);
    }
    // `<Button size="sm"><Plus /> Add pack</Button>`: the primary button with its plus.
    let add = webkit::button(t, "packs-add", "Add pack", Some(&icons::PLUS), true, false, |s: &mut App| s.open_pack_dialog());
    let mut body = col().w_full().gap(32.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    // The "Installed" section: a faint count when there are packs, then the panel of rows or "No packs installed.".
    let count = (!listing.installed.is_empty()).then(|| listing.installed.len().to_string());
    let inner = if listing.installed.is_empty() {
        text("No packs installed.".to_owned()).key("packs-empty").font(webkit::font(t, 13.0, 400.0, 21.125)).color(t.text.muted)
    } else {
        webkit::panel(listing.installed.iter().map(|p| pack_row(s, t, layer, p)).collect())
    };
    body = body.child(webkit::section(t, "packs-installed", "Installed", count.as_deref(), None, inner));
    Page { count: (!listing.installed.is_empty()).then_some(listing.installed.len()), trailing: vec![add], body, title: None, back: None }
}

fn update_label(version: &str) -> String {
    format!("Update to {version}")
}

/// One pack as the web's `PackRow`: the health glyph, a button (title, version, where it came from), Review and Update as small
/// outline buttons when they apply, the row's actions (Check health and the three-dot menu).
fn pack_row(s: &App, t: &Neko, layer: &mut Vec<El>, p: &InstalledPack) -> El {
    let name = p.info.name.clone();
    let status = words::pack_status_label(p.status);
    let source = words::pack_source_label(&p.source);
    let busy = s.setup.pack_busy.contains(&name);
    let open = name.clone();
    let small = webkit::font(t, 12.5, 400.0, 20.3125);
    let button = row()
        .key(format!("pack-{name}"))
        .role("button")
        .label(format!("{}, version {}, {status}, {source}", p.info.title, p.info.version))
        .focusable(true)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.open_pack(&open))
        .child(text(p.info.title.clone()).font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.base).truncate().min_w(0.0).a11y_hidden())
        .child(text(p.info.version.clone()).key(format!("pack-version-{name}")).font(webkit::tnum(small)).color(t.text.muted.alpha(0.6)).a11y_hidden())
        .child(text(source).key(format!("pack-source-{name}")).font(small).color(t.text.muted).truncate().grow(1.0).shrink().min_w(0.0).help(p.source.clone()).a11y_hidden());
    let changed = p.status == PackStatus::Changed;
    let mut line = vec![glyph(t, words::pack_glyph(p.status), status), button];
    if busy {
        // Rune-only: the row says it is working while a request of its own is out.
        line.push(text("Working…".to_owned()).key(format!("pack-busy-{name}")).font(small).color(t.text.faint).a11y_hidden());
    }
    if changed {
        let rv = name.clone();
        line.push(webkit::outline_xs_button(t, &format!("pack-review-{name}"), "Review", &format!("Review {}", p.info.title), webkit::gated(!busy, move |s: &mut App| s.review_pack(&rv))));
    }
    if let Some(version) = &p.update_available {
        let up = name.clone();
        line.push(webkit::outline_xs_button(t, &format!("pack-update-{name}"), &update_label(version), &format!("Update {} to {version}", p.info.title), webkit::gated(!busy, move |s: &mut App| s.update_pack(&up))));
    }
    let check = name.clone();
    let check_action = webkit::row_action(t, &format!("pack-health-{name}"), &format!("Check health of {}", p.info.title), &ph::ARROWS_CLOCKWISE, move |s: &mut App| s.check_pack(&check));
    let mut entries: Vec<webkit::MoreEntry> = Vec::new();
    let (c, rv, up, rm) = (name.clone(), name.clone(), name.clone(), name.clone());
    entries.push(Some(("Check health".to_owned(), &ph::ARROWS_CLOCKWISE, false, !busy, std::rc::Rc::new(move |s: &mut App| s.check_pack(&c)) as std::rc::Rc<dyn Fn(&mut App)>)));
    if changed {
        entries.push(Some(("Review…".to_owned(), &ph::EYE_REGULAR, false, !busy, std::rc::Rc::new(move |s: &mut App| s.review_pack(&rv)))));
    }
    if let Some(version) = &p.update_available {
        entries.push(Some((update_label(version), &ph::ARROW_UP_REGULAR, false, !busy, std::rc::Rc::new(move |s: &mut App| s.update_pack(&up)))));
    }
    entries.push(None);
    entries.push(Some(("Remove…".to_owned(), &ph::TRASH_REGULAR, true, !busy, std::rc::Rc::new(move |s: &mut App| s.confirm_remove_pack(&rm)))));
    let more = webkit::more_menu(s, t, layer, &format!("pack-more-{name}"), &format!("Actions for {}", p.info.title), false, entries);
    line.push(webkit::row_actions(vec![check_action, more]));

    // Everything the row does is also in its right-click menu.
    let mut menu: Vec<MenuEntry> = Vec::new();
    let o = name.clone();
    menu.push(("Open".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_pack(&o))));
    if changed {
        let rv = name.clone();
        menu.push(("Review…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.review_pack(&rv))));
    }
    let c = name.clone();
    menu.push(("Check health".to_owned(), std::rc::Rc::new(move |s: &mut App| s.check_pack(&c))));
    if let Some(version) = &p.update_available {
        let u = name.clone();
        menu.push((update_label(version), std::rc::Rc::new(move |s: &mut App| s.update_pack(&u))));
    }
    let r = name.clone();
    menu.push(("Remove…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_remove_pack(&r))));
    let mut below: Vec<El> = Vec::new();
    if let Some(n) = note(s, t, &format!("setup:pack:{name}")) {
        below.push(n);
    }
    with_menu(s, t, layer, &format!("pack:{name}"), webkit::item_row(t, &format!("pack-block-{name}"), line, below), menu)
}

/// A copy button: puts `value` on the clipboard and says so.
pub fn copy_button(t: &Neko, key: &str, label: &str, value: String) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .help(label.to_owned())
        .focusable(true)
        .size(26.0, 26.0)
        .center()
        .radius(t.radius.control)
        .hover(t.surface.active)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
            tx.copy(value.clone());
            s.show_toast("Copied");
        })
        .child(icon(&icons::COPY, 14.0).color(t.text.muted).a11y_hidden())
}

/// The requirements of a pack: a program or tool it needs, whether it was found, and what to run to get it.
pub fn requirements(t: &Neko, prefix: &str, info: &PackInfo) -> El {
    let mut c = col().key(format!("{prefix}-requirements")).w_full().gap(6.0);
    for (i, r) in info.requirements.iter().enumerate() {
        let mut head = row().key(format!("{prefix}-req-{i}")).w_full().gap(8.0).center_y().child(glyph(t, if r.found { crate::status::StatusKind::Done } else { crate::status::StatusKind::InProgress }, if r.found { "Found" } else { "Missing" })).child(text(r.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0));
        head = head.child(text(if r.found { "Found" } else { "Missing" }.to_owned()).key(format!("{prefix}-req-state-{i}")).font(t.ty.meta).color(if r.found { t.text.muted } else { t.meaning.amber }));
        if let Some(path) = r.path.as_deref().filter(|p| r.found && !p.is_empty()) {
            head = head.child(text(path.to_owned()).font(t.ty.mono_meta).color(t.text.faint).truncate().min_w(0.0).help(path.to_owned()));
        }
        c = c.child(head);
        if !r.hint.trim().is_empty() {
            c = c.child(
                row()
                    .key(format!("{prefix}-req-hint-{i}"))
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    .pl(22.0)
                    .child(text(r.hint.clone()).key(format!("{prefix}-req-hint-text-{i}")).font(t.ty.mono_meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0))
                    .child(copy_button(t, &format!("{prefix}-req-copy-{i}"), &format!("Copy the install command for {}", r.name), r.hint.clone())),
            );
        }
    }
    c
}

/// One content line of a pack: a label, what it holds and (when a section shows it) a link to that section.
fn content_row(t: &Neko, key: &str, label: &str, value: &str, link: Option<(&str, kit::Act)>) -> El {
    let mut r = kit::detail(t, key, label, value, false);
    if let Some((go, f)) = link {
        r = r.child(quiet_button(go, &format!("{key}-go"), t, f));
    }
    r
}

/// One pack, inside the section: a back arrow, check health, update and remove, then everything it says about itself.
fn detail(s: &App, t: &Neko, layer: &mut Vec<El>, p: &InstalledPack) -> Page {
    let _ = layer;
    let name = p.info.name.clone();
    let back = row()
        .key("pack-back")
        .role("button")
        .label("Back to packs")
        .help("Back to packs")
        .focusable(true)
        .size(28.0, 28.0)
        .center()
        .radius(t.radius.control)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(|s: &mut App| s.close_pack())
        .child(icon(&icons::CHEVRON_LEFT, 16.0).color(t.text.muted).a11y_hidden());
    let busy = s.setup.pack_busy.contains(&name);
    let mut body = col().w_full().gap(10.0);
    let mut actions = row().w_full().gap(8.0).center_y().wrap();
    // A pack that changed on disk runs nothing until the person has read what is there and approved it.
    if p.status == PackStatus::Changed {
        let n = name.clone();
        actions = actions.child(primary_button("Review changes", "pack-review", t, move |s: &mut App| s.review_pack(&n)));
    }
    if let Some(version) = &p.update_available {
        let n = name.clone();
        actions = actions.child(primary_button(&update_label(version), "pack-update", t, move |s: &mut App| s.update_pack(&n)));
    }
    // The plugin installs switched off: it is switched on here, through the Plugins page's own path.
    if let Some(id) = p.plugin_dir.as_deref().and_then(words::plugin_id_of) {
        let _ = id;
        let (n, on) = (name.clone(), !p.plugin_disabled);
        let label = if on { "Disable plugin" } else { "Enable plugin" };
        actions = actions.child(if s.plugin_ui.busy.is_some() { quiet_button(label, "pack-plugin-switch", t, |_| {}).disabled(true) } else { quiet_button(label, "pack-plugin-switch", t, move |s: &mut App| s.pack_plugin_set(&n, !on)) });
    }
    let n = name.clone();
    actions = actions.child(if busy { quiet_button("Check health", "pack-check", t, |_| {}).disabled(true) } else { quiet_button("Check health", "pack-check", t, move |s: &mut App| s.check_pack(&n)) });
    let n = name.clone();
    actions = actions.child(quiet_button("Remove", "pack-remove", t, move |s: &mut App| s.confirm_remove_pack(&n)));
    if busy {
        actions = actions.child(text("Working…".to_owned()).key("pack-busy").font(t.ty.meta).color(t.text.faint));
    }
    body = body.child(actions);
    if let Some(n) = note(s, t, &format!("setup:pack:{name}")) {
        body = body.child(n);
    }
    if let Some(n) = s.plugin_ui.note.as_deref().filter(|_| p.plugin_dir.is_some()) {
        body = body.child(kit::amber(t, "pack-plugin-note", n));
    }
    if p.status == PackStatus::Changed {
        body = body.child(kit::amber(t, "pack-changed-line", "The files changed on disk since you approved this pack. Nothing of it runs until you review them."));
    }
    // What a failed update or removal had to leave behind.
    if !p.orphans.is_empty() {
        let reasons = p.orphans.iter().map(|o| o.reason.clone()).collect::<Vec<_>>().join(" ");
        let n = name.clone();
        let button = if busy { quiet_button("Clean up", "pack-cleanup", t, |_| {}).disabled(true) } else { quiet_button("Clean up", "pack-cleanup", t, move |s: &mut App| s.clean_up_pack(&n)) };
        body = body.child(row().key("pack-orphans").w_full().gap(12.0).px(8.0).center_y().child(text(format!("Left behind: {reasons}")).key("pack-orphans-text").font(t.ty.meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0)).child(button));
    }
    if !p.info.description.trim().is_empty() {
        body = body.child(text(p.info.description.clone()).key("pack-description").font(t.ty.row).color(t.text.base).wrap().px(8.0).min_w(0.0));
    }
    // Details.
    body = body.child(kit::heading(t, "pack-details-head", "Details", None, true));
    let mut d = details_block("pack-details".to_owned()).pl(8.0);
    d = d.child(kit::detail(t, "pack-d-status", "Status", words::pack_status_label(p.status), false));
    d = d.child(kit::detail(t, "pack-d-version", "Version", &p.info.version, false));
    if let Some(version) = &p.update_available {
        d = d.child(kit::detail(t, "pack-d-update", "Newer version", version, false));
    }
    d = d.child(kit::detail(t, "pack-d-source", "Source", &p.source, true));
    if !p.info.licence.trim().is_empty() {
        d = d.child(kit::detail(t, "pack-d-licence", "Licence", &p.info.licence, false));
    }
    if !p.info.homepage.trim().is_empty() {
        d = d.child(kit::detail(t, "pack-d-homepage", "Homepage", &p.info.homepage, true));
    }
    if p.plugin_dir.is_some() {
        d = d.child(kit::detail(t, "pack-d-plugin", "Plugin", if p.plugin_disabled { "Switched off" } else { "Switched on" }, false));
    }
    d = d.child(kit::detail(t, "pack-d-installed", "Installed", &words::relative_time(Some(p.installed_ms), s.now_ms), false));
    d = d.child(kit::detail(t, "pack-d-approved", "You approved", &words::relative_time(Some(p.approved_ms), s.now_ms), false));
    body = body.child(d);
    // Permissions.
    body = body.child(kit::heading(t, "pack-permissions-head", "Permissions", Some(p.info.permissions.len()), false));
    let mut perms = col().key("pack-permissions").w_full().gap(2.0).px(8.0);
    if p.info.permissions.is_empty() {
        perms = perms.child(kit::words(t, "pack-no-permissions", "It asks for none."));
    }
    for (i, line) in p.info.permissions.iter().enumerate() {
        perms = perms.child(row().key(format!("pack-permission-{i}")).w_full().gap(8.0).child(text("•".to_owned()).font(t.ty.meta).color(t.text.faint)).child(text(line.clone()).font(t.ty.meta).color(t.text.base).wrap().grow(1.0).min_w(0.0)));
    }
    body = body.child(perms);
    // Contents.
    let c = &p.info.contents;
    body = body.child(kit::heading(t, "pack-contents-head", "Contents", None, false));
    let mut contents = col().key("pack-contents").w_full().gap(2.0).px(0.0);
    let mut any = false;
    if !c.connections.is_empty() {
        any = true;
        contents = contents.child(content_row(t, "pack-c-connections", "Connections", &c.connections.join(", "), Some(("Open Tools", Box::new(|s: &mut App| s.open_setup(SetupSection::Tools))))));
    }
    if !c.skills.is_empty() {
        any = true;
        contents = contents.child(content_row(t, "pack-c-skills", "Skills", &c.skills.join(", "), Some(("Open Skills", Box::new(|s: &mut App| s.open_setup(SetupSection::Skills))))));
    }
    if let Some(plugin) = c.plugin.as_deref().filter(|p| !p.is_empty()) {
        any = true;
        contents = contents.child(content_row(t, "pack-c-plugin", "Plugin", plugin, Some(("Open Plugins", Box::new(|s: &mut App| s.open_view(FullView::Plugins))))));
    }
    if !c.routines.is_empty() {
        any = true;
        contents = contents.child(content_row(t, "pack-c-routines", "Routines", &c.routines.join(", "), Some(("Open Routines", Box::new(|s: &mut App| s.open_view(FullView::Routines))))));
    }
    if !c.luau.is_empty() {
        any = true;
        contents = contents.child(content_row(t, "pack-c-luau", "Native panels", &c.luau.join(", "), None));
    }
    if !any {
        contents = contents.child(kit::words(t, "pack-no-contents", "It adds nothing."));
    }
    body = body.child(contents);
    // Each Luau file: running (a mint check) or stopped (a coral cross) and why.
    if !c.luau.is_empty() {
        body = body.child(kit::heading(t, "pack-luau-head", "Native panels", Some(c.luau.len()), false));
        body = body.child(match super::super::luau::pack_files(s, t, &name) {
            Some(rows) => rows,
            None => kit::words(t, "pack-luau-none", "Not asked for yet."),
        });
    }
    // Requirements.
    if !p.info.requirements.is_empty() {
        body = body.child(kit::heading(t, "pack-requirements-head", "Requirements", Some(p.info.requirements.len()), false));
        body = body.child(requirements(t, "pack", &p.info).px(8.0));
    }
    // The last health check.
    body = body.child(kit::heading(t, "pack-health-head", "Last health check", None, false));
    body = body.child(match &p.health {
        Some(h) => {
            let when = words::relative_time(Some(h.at_ms), s.now_ms);
            let mut c = col().key("pack-health").w_full().gap(4.0).px(8.0);
            c = c.child(text(format!("{} · {when}", if h.ok { "Passed" } else { "Failed" })).key("pack-health-when").font(t.ty.meta).color(if h.ok { t.text.muted } else { t.meaning.coral }));
            if !h.output.trim().is_empty() {
                c = c.child(text(h.output.clone()).key("pack-health-output").font(t.ty.mono_meta).color(t.text.muted).wrap().px(8.0).py(8.0).radius(t.radius.row).fill(t.surface.hover).min_w(0.0));
            }
            c
        }
        None => kit::words(t, "pack-health-none", "No check has run yet."),
    });
    Page { count: None, trailing: Vec::new(), body, title: Some(p.info.title.clone()), back: Some(back) }
}
