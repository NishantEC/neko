//! Setup, Packs: the pack dialog. "Add pack" has two tabs, Catalog (the packs that ship with Neko) and From folder or URL, then
//! the preview of what the pack would add. The same preview step serves "Update to <version>" and "Review" (a pack that changed on
//! disk). Every preview carries a digest of everything the person is asked to approve; the button that follows sends it back, so
//! the approval is of exactly what was shown. The dialog waits for its own answer: its buttons are off while a request is in
//! flight, and a refusal shows in the daemon's words and gives them back. Nothing here picks, recommends or nudges any pack.

use neko_protocol::packs::{CatalogEntry, PackPreview, PluginPower};
use rune::{El, col, row, text};
use rune_kit::SegmentedControl;

use super::kit::{self, field, footer, form_note, text_field};
use super::packs::requirements;
use super::state::{PackIntent, PackTab};
use crate::app::App;
use crate::theme::Neko;
use crate::view::{primary_button, quiet_button};

pub fn pack(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, tab: PackTab, preview: Option<&PackPreview>, intent: &PackIntent) {
    match (preview, intent) {
        (Some(p), _) => previewed(s, t, layer, win_h, p, intent),
        (None, PackIntent::Install) => choose(s, t, layer, win_h, tab),
        (None, _) => looking(s, t, layer, win_h, intent),
    }
}

/// An update or a review has asked the daemon what it would be; nothing shows until it answers.
fn looking(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, intent: &PackIntent) {
    let (title, line) = match intent {
        PackIntent::Review { .. } => ("Review", "Reading the files on disk…"),
        _ => ("Update", "Looking at the newer version…"),
    };
    let mut body = col().w_full().gap(12.0);
    if s.setup.awaiting.is_some() {
        body = body.child(kit::words(t, "dlg-pack-looking", line).px(0.0));
    }
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    body = body.child(footer(vec![quiet_button("Cancel", "dlg-pack-cancel", t, |s: &mut App| s.close_dialog())]));
    kit::dialog(t, layer, "dlg-pack", title, None, 560.0, win_h, body);
}

fn choose(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, tab: PackTab) {
    let tabs = ["Catalog", "From folder or URL"];
    let now = [PackTab::Catalog, PackTab::Source].iter().position(|x| *x == tab);
    let switcher = SegmentedControl::new(&tabs, now).label("Where the pack comes from").width(None).on_pick(|s: &mut App, i: usize, _| s.pack_dialog_tab([PackTab::Catalog, PackTab::Source][i.min(1)])).build(&t.kit).key("dlg-pack-tabs");
    let mut body = col().w_full().gap(12.0).child(switcher);
    let waiting = s.setup.awaiting.is_some();
    match tab {
        PackTab::Catalog => {
            match s.packs() {
                None => body = body.child(kit::words(t, "dlg-pack-loading", "Looking for the bundled packs…")),
                Some(l) if l.catalog.is_empty() => body = body.child(kit::words(t, "dlg-pack-no-catalog", "No bundled packs.")),
                Some(l) => {
                    for entry in &l.catalog {
                        body = body.child(catalog_row(t, entry, waiting));
                    }
                }
            }
            if let Some(n) = form_note(s, t) {
                body = body.child(n);
            }
            body = body.child(footer(vec![quiet_button("Cancel", "dlg-pack-cancel", t, |s: &mut App| s.close_dialog())]));
        }
        PackTab::Source => {
            body = body.child(field(
                t,
                "pack-source",
                "Folder path or repository address",
                Some("A folder on this Mac, or an https or ssh repository. Nothing is installed until you have seen what it adds."),
                text_field(s, t, "pack:source", "Folder path or repository address", "/path/to/pack or https://example.com/team/pack.git", true, false, |s: &mut App| s.pack_preview_typed()),
            ));
            if let Some(n) = form_note(s, t) {
                body = body.child(n);
            }
            let ok = !s.pages.text("pack:source").trim().is_empty() && !waiting;
            let preview = if ok { primary_button("Preview", "dlg-pack-preview", t, |s: &mut App| s.pack_preview_typed()) } else { primary_button("Preview", "dlg-pack-preview", t, |_| {}).disabled(true).fill(t.meaning.iris.alpha(0.4)) };
            body = body.child(footer(vec![quiet_button("Cancel", "dlg-pack-cancel", t, |s: &mut App| s.close_dialog()), preview]));
        }
    }
    kit::dialog(t, layer, "dlg-pack", "Add pack", Some("A pack adds tools, skills, a plugin and routines in one step."), 560.0, win_h, body);
}

/// A bundled pack: its title, version and description, and Add (or "Installed" when it is).
fn catalog_row(t: &Neko, entry: &CatalogEntry, waiting: bool) -> El {
    let name = entry.info.name.clone();
    let source = entry.source.clone();
    let head = row().w_full().gap(8.0).center_y().child(text(entry.info.title.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0)).child(text(entry.info.version.clone()).font(t.ty.meta).color(t.text.faint)).child(kit::fill());
    let head = if entry.installed {
        head.child(text("Installed".to_owned()).key(format!("dlg-pack-installed-{name}")).font(t.ty.meta).color(t.text.muted))
    } else if waiting {
        head.child(quiet_button("Add", &format!("dlg-pack-add-{name}"), t, |_| {}).disabled(true))
    } else {
        head.child(quiet_button("Add", &format!("dlg-pack-add-{name}"), t, move |s: &mut App| s.pack_preview(&source)))
    };
    let mut c = col().key(format!("dlg-pack-entry-{name}")).w_full().px(10.0).py(8.0).gap(2.0).radius(t.radius.row).fill(t.surface.hover).child(head);
    if !entry.info.description.trim().is_empty() {
        c = c.child(text(entry.info.description.clone()).font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0));
    }
    c
}

/// A bulleted list of plain lines.
fn bullets(t: &Neko, prefix: &str, lines: &[String], ink: rune::Color) -> El {
    let mut c = col().key(prefix.to_owned()).w_full().gap(2.0);
    for (i, line) in lines.iter().enumerate() {
        c = c.child(row().key(format!("{prefix}-{i}")).w_full().gap(8.0).child(text("•".to_owned()).font(t.ty.meta).color(t.text.faint)).child(text(line.clone()).font(t.ty.meta).color(ink).wrap().grow(1.0).min_w(0.0)));
    }
    c
}

/// What the pack's web plugin declares: it runs inside Neko with access to Neko's own API, so all of it is listed.
fn plugin_power(t: &Neko, power: &PluginPower) -> El {
    let mut c = col().key("dlg-pack-plugin").w_full().gap(4.0);
    c = c.child(kit::detail(t, "dlg-pack-plugin-title", "Title", &power.title, false).px(0.0));
    for (key, label, list) in [("pages", "Pages", &power.pages), ("viewers", "Viewers", &power.viewers), ("fences", "Code fences", &power.fences)] {
        if !list.is_empty() {
            c = c.child(kit::detail(t, &format!("dlg-pack-plugin-{key}"), label, &list.join(", "), false).px(0.0));
        }
    }
    if !power.prompt.trim().is_empty() {
        c = c.child(kit::words(t, "dlg-pack-plugin-prompt-head", "Adds this to the model’s instructions").px(0.0));
        c = c.child(text(power.prompt.clone()).key("dlg-pack-plugin-prompt").font(t.ty.mono_meta).color(t.text.muted).wrap().px(8.0).py(4.0).radius(t.radius.row).fill(t.surface.hover).min_w(0.0));
    }
    c
}

/// What the pack would add (or, for an update or a review, what it contains), before the person says yes.
fn previewed(s: &App, t: &Neko, layer: &mut Vec<El>, win_h: f32, p: &PackPreview, intent: &PackIntent) {
    let info = &p.info;
    let installing = matches!(intent, PackIntent::Install);
    let mut body = col().w_full().gap(10.0);
    let mut meta = vec![format!("Version {}", info.version)];
    if !info.licence.trim().is_empty() {
        meta.push(format!("Licence {}", info.licence));
    }
    body = body.child(text(meta.join(" · ")).key("dlg-pack-meta").font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0));
    if !info.homepage.trim().is_empty() {
        body = body.child(text(info.homepage.clone()).key("dlg-pack-homepage").font(t.ty.mono_meta).color(t.text.faint).wrap().min_w(0.0));
    }
    match intent {
        PackIntent::Install => {
            if let Some(v) = &p.installed_version {
                body = body.child(kit::amber(t, "dlg-pack-installed-note", &format!("Version {v} is already installed.")).px(0.0));
            }
        }
        PackIntent::Update { .. } => {
            let from = p.installed_version.as_deref().unwrap_or("the installed version");
            body = body.child(kit::words(t, "dlg-pack-update-line", &format!("Updates {from} to {}. Your grants and which routines are on are kept.", info.version)).px(0.0));
            if p.needs_approval {
                body = body.child(kit::amber(t, "dlg-pack-needs-approval", "Update needs approval").px(0.0));
            }
        }
        PackIntent::Review { .. } => {
            body = body.child(kit::amber(t, "dlg-pack-review-line", "The files on disk no longer match what you approved, so nothing of this pack runs. Read what is there now, then approve it or remove the pack.").px(0.0));
        }
    }
    // Things to read twice, highlighted.
    if !info.warnings.is_empty() {
        let mut w = col().key("dlg-pack-warnings").w_full().px(12.0).py(8.0).gap(4.0).radius(t.radius.row).fill(t.meaning.amber.alpha(0.12));
        w = w.child(bullets(t, "dlg-pack-warning", &info.warnings, t.text.base));
        body = body.child(w);
    }
    // What changed in what the pack can do.
    if !p.changes.is_empty() {
        body = body.child(kit::heading(t, "dlg-pack-changes-head", "What changed", Some(p.changes.len()), false).px(0.0));
        body = body.child(bullets(t, "dlg-pack-change", &p.changes, t.text.base));
    }
    if !info.description.trim().is_empty() {
        body = body.child(text(info.description.clone()).key("dlg-pack-description").font(t.ty.row).color(t.text.base).wrap().min_w(0.0));
    }
    // The permission summary: the button approves it.
    body = body.child(kit::heading(t, "dlg-pack-permissions-head", "Permissions", Some(info.permissions.len()), false).px(0.0));
    if info.permissions.is_empty() {
        body = body.child(kit::words(t, "dlg-pack-no-permissions", "It asks for none.").px(0.0));
    }
    body = body.child(bullets(t, "dlg-pack-permission", &info.permissions, t.text.base));
    // Every command line it would run on this Mac.
    if !info.runs.is_empty() {
        body = body.child(kit::heading(t, "dlg-pack-runs-head", "What it runs", Some(info.runs.len()), false).px(0.0));
        let mut runs = col().key("dlg-pack-runs").w_full().gap(4.0);
        for (i, line) in info.runs.iter().enumerate() {
            runs = runs.child(text(line.clone()).key(format!("dlg-pack-run-{i}")).font(t.ty.mono_meta).color(t.text.muted).wrap().px(8.0).py(4.0).radius(t.radius.row).fill(t.surface.hover).min_w(0.0));
        }
        body = body.child(runs);
    }
    // The plugin's power and the text of each routine.
    if let Some(power) = &info.plugin {
        body = body.child(kit::heading(t, "dlg-pack-plugin-head", "Plugin", None, false).px(0.0));
        body = body.child(plugin_power(t, power));
    }
    if !info.routine_texts.is_empty() {
        body = body.child(kit::heading(t, "dlg-pack-routines-head", "Routines", Some(info.routine_texts.len()), false).px(0.0));
        for (i, r) in info.routine_texts.iter().enumerate() {
            let mut c = col().key(format!("dlg-pack-routine-{i}")).w_full().gap(2.0).px(10.0).py(6.0).radius(t.radius.row).fill(t.surface.hover);
            c = c.child(row().w_full().gap(8.0).child(text(r.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0)).child(text(r.cadence.clone()).font(t.ty.meta).color(t.text.faint)));
            c = c.child(text(r.instruction.clone()).key(format!("dlg-pack-routine-text-{i}")).font(t.ty.mono_meta).color(t.text.muted).wrap().min_w(0.0));
            body = body.child(c);
        }
    }
    // What it will add.
    let c = &info.contents;
    body = body.child(kit::heading(t, "dlg-pack-adds-head", if installing { "What it will add" } else { "What it contains" }, None, false).px(0.0));
    let mut adds = col().key("dlg-pack-adds").w_full().gap(2.0);
    let mut any = false;
    for (key, label, value) in [
        ("connections", "Connections", c.connections.join(", ")),
        ("skills", "Skills", c.skills.join(", ")),
        ("plugin", "Plugin", c.plugin.clone().unwrap_or_default()),
        ("routines", "Routines", c.routines.join(", ")),
    ] {
        if !value.is_empty() {
            any = true;
            adds = adds.child(kit::detail(t, &format!("dlg-pack-add-{key}"), label, &value, false).px(0.0));
        }
    }
    if !any {
        adds = adds.child(kit::words(t, "dlg-pack-adds-nothing", "It adds nothing.").px(0.0));
    }
    if installing {
        adds = adds.child(kit::words(t, "dlg-pack-adds-note", "Routines are installed switched off, the plugin is installed switched off, and tools are granted to no workspace until you grant them in Tools.").px(0.0));
    }
    body = body.child(adds);
    // Requirements.
    if !info.requirements.is_empty() {
        body = body.child(kit::heading(t, "dlg-pack-requirements-head", "Requirements", Some(info.requirements.len()), false).px(0.0));
        body = body.child(requirements(t, "dlg-pack", info));
    }
    // The values it asks for, masked (an install only: an update keeps what is stored).
    if installing && !info.env.is_empty() {
        body = body.child(kit::heading(t, "dlg-pack-env-head", "Values it needs", Some(info.env.len()), false).px(0.0));
        body = body.child(kit::words(t, "dlg-pack-env-note", "Kept in the Keychain. They are never shown again.").px(0.0));
        for name in &info.env {
            let key = format!("pack:env:{name}");
            body = body.child(field(t, &format!("pack-env-{name}"), name, None, text_field(s, t, &key, name, "", true, true, |_| {})));
        }
    }
    if let Some(n) = form_note(s, t) {
        body = body.child(n);
    }
    let button = match intent {
        PackIntent::Install => "Install",
        PackIntent::Update { .. } if p.needs_approval => "Approve and update",
        PackIntent::Update { .. } => "Update",
        PackIntent::Review { .. } => "Approve",
    };
    body = body.child(kit::words(t, "dlg-pack-approval", &format!("Pressing {button} approves exactly what is shown above.")).px(0.0));
    let waiting = s.setup.awaiting.is_some();
    let mut buttons = vec![quiet_button("Cancel", "dlg-pack-cancel", t, |s: &mut App| s.close_dialog())];
    if installing {
        buttons.push(if waiting { quiet_button("Back", "dlg-pack-back", t, |_| {}).disabled(true) } else { quiet_button("Back", "dlg-pack-back", t, |s: &mut App| s.pack_preview_back()) });
    }
    buttons.push(if waiting { primary_button(button, "dlg-pack-install", t, |_| {}).disabled(true).fill(t.meaning.iris.alpha(0.4)) } else { primary_button(button, "dlg-pack-install", t, |s: &mut App| s.pack_confirm()) });
    let foot = footer(buttons);
    let title = match intent {
        PackIntent::Install => info.title.clone(),
        PackIntent::Update { .. } => format!("Update {}", info.title),
        PackIntent::Review { .. } => format!("Review {}", info.title),
    };
    kit::dialog_with_footer(t, layer, "dlg-pack", &title, None, 560.0, win_h, body, Some(foot));
}
