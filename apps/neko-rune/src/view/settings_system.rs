//! Settings, "This Mac": a port of `web/src/pages/setup/settings/System.tsx`, section by section with the web's groups, words,
//! order and controls: the quick panel's folders, what Neko can access, diagnostics, Neko updates and About.
//! Rows the web does not have (Calendar, Reminders and Screen Recording access, the app's own diagnostics, the version, "Open the
//! web UI", and editing where updates come from) sit last in the group they belong to.

use neko_protocol::calendar::CalendarAccess;
use neko_protocol::phone::RemindersAccess;
use rune::{El, col, el, icon, row, text};

use super::settings::{Item, item, short, with_note};
use super::ui::switch;
use super::webkit::{self, Tone, button, ghost_button, info_row, setting_row, setting_row_stacked, skeleton, status};
use crate::app::App;
use crate::icons;
use crate::link::Call;
use crate::pages::Section;
use crate::permissions::{Access, Pane};
use crate::theme::Neko;

/// The project's own public repository (the web's `PROJECT_REPOSITORY`).
const PROJECT_REPOSITORY: &str = "https://github.com/NishantEC/neko"; // privacy-ok: the project's own public repository
const DEFAULT_SOURCE: &str = "NishantEC/neko"; // privacy-ok: the project's own public repository

pub(super) fn all(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    quick_panel(s, t, out);
    permissions(s, t, out);
    diagnostics(s, t, out);
    updates(s, t, layer, out);
    about(s, t, out);
}

/// A dim 12.5 point line, the description of an info row.
fn dim(t: &Neko, key: &str, words: &str) -> El {
    text(words.to_owned()).key(key.to_owned()).font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted).truncate().grow(1.0).min_w(0.0)
}

fn mono_dim(t: &Neko, key: &str, words: &str) -> El {
    text(words.to_owned()).key(key.to_owned()).font(rune::TextStyle { size: 12.0, line_height: 16.0, ..t.ty.mono }).color(t.text.muted).truncate().grow(1.0).min_w(0.0)
}

/// A 13 point muted paragraph in a panel (`px-3 py-4`).
fn paragraph(t: &Neko, key: &str, words: &str, pad_y: f32) -> El {
    col().key(key.to_owned()).w_full().px(12.0).py(pad_y).child(text(words.to_owned()).font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted).wrap().min_w(0.0))
}

/// The default size (`Button` with no `size`): 40 high, 16 at the sides, 8 between the icon and the words.
fn big(b: El) -> El {
    b.h(40.0).px(16.0).gap(8.0)
}

fn dim_if(enabled: bool, b: El) -> El {
    if enabled { b } else { b.disabled(true).opacity(0.5) }
}

// ---- Quick panel --------------------------------------------------------------------------------

fn quick_panel(s: &App, t: &Neko, out: &mut Vec<Item>) {
    const G: &str = "Included folders";
    let list = match &s.extra.folders {
        None => col().key("set-folders").w_full().padding(20.0).child(skeleton(t, "folders-loading", 256.0, 16.0)),
        Some(f) if f.is_empty() => row().key("set-folders").w_full().px(12.0).py(20.0).gap(8.0).center_y().child(icon(&icons::FOLDER, 13.0).color(t.text.muted).a11y_hidden()).child(text("No folders added".to_owned()).key("folders-none").font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted)),
        Some(f) => {
            let mut list = col().key("set-folders").w_full();
            for (i, folder) in f.iter().enumerate() {
                let id = folder.id.clone();
                let leading = row().size(20.0, 20.0).center().child(icon(&icons::FOLDER, 16.0).color(t.text.muted));
                let title = text(folder.title.clone()).key(format!("folder-{i}-title")).font(rune::TextStyle { size: 13.0, line_height: 20.25, ..t.ty.mono }).color(t.text.base).truncate().min_w(0.0);
                let remove = ghost_button(t, &format!("folder-remove-{i}"), "Remove", Some(&icons::CROSS), move |s: &mut App| s.remove_folder(&id)).label(format!("Remove {} from search", folder.title));
                list = list.child(info_row(t, &format!("folder-{i}"), title, None, Some(leading), vec![remove], None));
            }
            list
        }
    };
    out.push(item(Section::QuickPanel, G, "folders", "Included folders", "", with_note(s, t, "setting:folders", list)));
    let typed = !s.pages.text("folder-path").trim().is_empty();
    let help = "An absolute path on this Mac. The daemon checks that it is a folder.";
    let add = big(button(t, "folder-add", "Add folder", None, true, false, webkit::gated(typed, |s: &mut App| s.add_folder())));
    let form = row().w_full().gap(8.0).center_y().child(webkit::input(s, t, "folder-path", "Folder path", "/Users/you/Documents", 100.0, true, |s: &mut App| s.add_folder()).grow(1.0)).child(dim_if(typed, add));
    out.push(item(Section::QuickPanel, G, "add-folder", "Add a folder", help, setting_row_stacked(t, "set-add-folder", "Add a folder", Some(help), form)));
}

// ---- Permissions --------------------------------------------------------------------------------

/// What a permission row shows: the Mac's answer when this app can tell, then (for the two the web lists) its badge "Managed in the
/// native app", which opens that pane of System Settings. The others get the plain button.
fn access_control(t: &Neko, id: &str, access: Access, pane: Pane, badge: bool) -> El {
    let ink = match access {
        Access::Granted => t.meaning.mint,
        Access::Denied => t.meaning.amber,
        Access::Unknown => t.text.muted,
    };
    let open = move |s: &mut App| {
        if (s.opener)(pane.url()).is_err() {
            s.notice = Some("Couldn’t open System Settings.".to_owned());
        }
    };
    let state = text(access.words()).key(format!("perm-{id}-state")).font(webkit::font(t, 12.5, 400.0, 18.75)).color(ink);
    if badge {
        let badge = status(t, &format!("perm-{id}-open"), Tone::Neutral, "Managed in the native app").role("button").label("Open System Settings").focusable(true).hover(t.text.base.alpha(0.08)).on_press(open);
        // The web's row holds only the badge, so its words get the width the badge leaves: the Mac's answer is drawn in the margin to the badge's
        // left (a negative margin the width of the words and the gap), taking none of the row's width.
        let style = webkit::font(t, 12.5, 400.0, 18.75);
        let width = rune::text::measure(access.words(), &[], style, None).0;
        return row().center_y().child(state.ml(-(width + 8.0)).mr(8.0).shrink()).child(badge);
    }
    row().gap(10.0).center_y().child(state).child(ghost_button(t, &format!("perm-{id}-open"), "Open System Settings", None, open))
}

fn permissions(s: &App, t: &Neko, out: &mut Vec<Item>) {
    const G: &str = "What Neko can access";
    let local = s.pages.access.unwrap_or_default();
    let calendar = match s.pages.calendar.as_ref().map(|c| c.status.access) {
        Some(CalendarAccess::Granted) => Access::Granted,
        Some(CalendarAccess::Denied) => Access::Denied,
        _ => Access::Unknown,
    };
    let reminders = match s.pages.phone.as_ref().map(|p| p.status.reminders_access) {
        Some(RemindersAccess::Granted) => Access::Granted,
        Some(RemindersAccess::Denied) => Access::Denied,
        _ => Access::Unknown,
    };
    // Clipboard history: a switch.
    let help = "Saves what you copy, only on this Mac. Turning it off stops new capture; existing history stays until you clear it.";
    let control = match s.extra.clipboard {
        Some(on) => switch(t, "set-clipboard-switch", "Clipboard history", on, move |s: &mut App| s.set_clipboard(!on)),
        None => skeleton(t, "set-clipboard-loading", 36.0, 20.0),
    };
    out.push(item(Section::Permissions, G, "clipboard-history", "Clipboard history", help, with_note(s, t, "setting:clipboard", setting_row(t, "set-clipboard", "Clipboard history", Some(help), control))));
    let rows: [(&str, &str, &str, Access, Pane); 5] = [
        ("full-disk", "Full Disk Access", "Allows access to protected folders, including Documents, Desktop and iCloud Drive. macOS grants this to the Neko app itself, so it is managed in the native app.", local.full_disk, Pane::FullDisk),
        ("accessibility", "Accessibility", "Lets Neko paste for you and read the text you have selected. Nothing else on screen is read. Granted to the native app in System Settings.", local.accessibility, Pane::Accessibility),
        ("calendar", "Calendar", "Shows your meetings beside Neko’s dated work.", calendar, Pane::Calendars),
        ("reminders", "Reminders", "A reminder in the Neko list for each thing that needs you.", reminders, Pane::Reminders),
        ("screen-recording", "Screen Recording", "Only for agents you let see the screen.", local.screen_recording, Pane::ScreenRecording),
    ];
    // The web lists two of these; the others (Calendar, Reminders, Screen Recording) follow the page, in a group of their own.
    for (id, label, help, access, pane) in &rows[..2] {
        out.push(item(Section::Permissions, G, format!("perm-{id}"), label, help, setting_row(t, &format!("set-perm-{id}"), label, Some(help), access_control(t, id, *access, *pane, id == &"full-disk" || id == &"accessibility"))));
    }

    // Workspace folders: the protected folders the workspaces live in.
    const W: &str = "Workspace folders";
    let paths: Vec<String> = s.summary.as_ref().map(|m| m.settings.workspaces.iter().flat_map(|w| if w.folders.is_empty() && !w.repository.is_empty() { vec![w.repository.clone()] } else { w.folders.clone() }).collect()).unwrap_or_default();
    let mut protected: Vec<(String, usize)> = Vec::new();
    for p in &paths {
        if let Some(folder) = protected_folder(p) {
            match protected.iter_mut().find(|(f, _)| *f == folder) {
                Some((_, n)) => *n += 1,
                None => protected.push((folder, 1)),
            }
        }
    }
    if protected.is_empty() {
        out.push(item(Section::Permissions, W, "workspace-folders", "Workspace folders", "", paragraph(t, "set-protected-none", "None of your workspaces are in a folder macOS protects, so tasks open them without asking.", 16.0)));
    }
    for (i, (folder, count)) in protected.iter().enumerate() {
        let words = format!("{} here. macOS asks once, the first time a task opens it, and there is no way to check the answer without asking.", if *count == 1 { "1 workspace is".to_owned() } else { format!("{count} workspaces are") });
        let title = text(folder.clone()).font(webkit::font(t, 14.0, 500.0, 20.0)).color(t.text.base).truncate().min_w(0.0);
        out.push(item(Section::Permissions, W, format!("workspace-folder-{i}"), folder, &words, info_row(t, &format!("set-protected-{i}"), title, Some(dim(t, &format!("set-protected-{i}-words"), &words)), None, vec![status(t, &format!("set-protected-{i}-badge"), Tone::Success, "Asked on first use")], None)));
    }
    // Agent access: a paragraph.
    let words = "Neko does not record your screen or control your mouse. Authorized agents are local processes with full filesystem access; their worktrees are separate copies, not a security boundary. Publishing or changing external systems needs separate authority.";
    out.push(item(Section::Permissions, "Agent access", "agent-access", "Agent access", words, paragraph(t, "set-agent-access", words, 16.0)));
    for (id, label, help, access, pane) in &rows[2..] {
        out.push(item(Section::Permissions, "More access", format!("perm-{id}"), label, help, setting_row(t, &format!("set-perm-{id}"), label, Some(help), access_control(t, id, *access, *pane, id == &"full-disk" || id == &"accessibility"))));
    }
}

/// `Documents`, `Desktop`, `Downloads` or `iCloud Drive` when the path is inside one of the folders macOS protects.
fn protected_folder(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/Users/")?;
    let (_, rest) = rest.split_once('/')?;
    let (first, tail) = rest.split_once('/').map_or((rest, ""), |(a, b)| (a, b));
    match first {
        "Desktop" | "Documents" | "Downloads" => Some(first.to_owned()),
        "Library" if tail == "Mobile Documents" || tail.starts_with("Mobile Documents/") => Some("iCloud Drive".to_owned()),
        _ => None,
    }
}

// ---- Diagnostics --------------------------------------------------------------------------------

fn diagnostics(s: &App, t: &Neko, out: &mut Vec<Item>) {
    const G: &str = "Diagnostics";
    let help = "Times each runtime and shows which installs Neko uses.";
    let running = s.extra.diag_running;
    let run = big(button(t, "diag-run", if running { "Running…" } else { "Run diagnostics" }, Some(&icons::GAUGE), true, false, webkit::gated(!running, |s: &mut App| s.run_diagnostics())));
    let mut r = col().key("diag-checks").w_full().child(setting_row(t, "set-diag-run", "Run diagnostics", Some(help), dim_if(!running, run)));
    match &s.extra.diag {
        None if !running => r = r.child(col().key("diag-none").w_full().child(el().w_full().h(1.0).fill(t.surface.border)).child(col().w_full().px(12.0).py(24.0).child(text("Run a check to see connection and runtime details.".to_owned()).font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted).wrap().min_w(0.0).self_center()))),
        None => {}
        Some((ms, checks)) => {
            let line = row().key("diag-roundtrip").w_full().px(12.0).py(14.0).gap(12.0).center_y().justify_between().child(text("Neko window ↔ daemon".to_owned()).font(webkit::font(t, 13.0, 400.0, 19.5)).color(t.text.muted)).child(text(format!("{ms} ms")).key("diag-roundtrip-ms").font(rune::TextStyle { size: 13.0, line_height: 19.5, ..t.ty.mono }).color(t.text.base));
            r = r.child(line);
            for (i, c) in checks.iter().enumerate() {
                let (glyph, ink) = if c.ok { (&icons::CHECK_LINE, t.meaning.mint) } else { (&icons::INFO, t.meaning.amber) };
                let leading = row().size(20.0, 20.0).center().child(icon(glyph, 16.0).color(ink));
                let title = text(c.name.clone()).key(format!("diag-check-{i}-name")).font(webkit::font(t, 14.0, 500.0, 20.0)).color(t.text.base).truncate().min_w(0.0);
                let trailing = if c.millis > 0 { vec![text(format!("{} ms", c.millis)).font(rune::TextStyle { size: 12.0, line_height: 16.0, ..t.ty.mono }).color(t.text.muted)] } else { Vec::new() };
                r = r.child(info_row(t, &format!("diag-check-{i}"), title, Some(dim(t, &format!("diag-check-{i}-detail"), &c.detail)), Some(leading), trailing, None));
            }
        }
    }
    out.push(item(Section::Diagnostics, G, "diagnostics", "Run diagnostics", help, with_note(s, t, "setting:diagnostics", r)));

    // Not in the web: this app's own versions, the data folder and the daemon, and a button that copies them.
    let home = std::env::var("HOME").unwrap_or_default();
    let data = crate::terminal::tilde(&neko_protocol::support_dir().to_string_lossy(), &home);
    let pin = include_str!("../../RUNE_REV").lines().next().unwrap_or("").chars().take(9).collect::<String>();
    let app = format!("Neko for Rune {} · Rune {pin}", env!("CARGO_PKG_VERSION"));
    let u = s.summary.as_ref().and_then(|m| m.settings.self_update.clone());
    let neko = u.as_ref().map_or("unknown".to_owned(), |u| format!("app {} · web {}", short(&u.native_commit), short(&u.web_commit)));
    let daemon = crate::app::status_line(&s.link, s.summary.as_ref());
    let rows = [("app", "This app", app), ("neko", "Neko", neko), ("data", "Data folder", data), ("daemon", "Daemon", daemon)];
    for (id, label, words) in &rows {
        out.push(item(Section::Diagnostics, G, format!("diag-{id}"), label, words, setting_row(t, &format!("set-diag-{id}"), label, None, super::settings::value(t, &format!("diag-{id}-value"), words))));
    }
    let report: String = rows.iter().map(|(_, label, words)| format!("{label}: {words}")).collect::<Vec<_>>().join("\n");
    let copy = button(t, "diag-copy", "Copy diagnostics", None, false, false, |_: &mut App| {}).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
        tx.copy(report.clone());
        s.show_toast("Copied the diagnostics");
    });
    let help = "Versions, the data folder and the daemon's state, as text to paste into a report.";
    out.push(item(Section::Diagnostics, G, "diag-copy", "Copy diagnostics", help, setting_row(t, "set-diag-copy", "Copy diagnostics", Some(help), copy)));
}

// ---- Neko updates -------------------------------------------------------------------------------

fn sha8(commit: &Option<String>) -> String {
    commit.as_deref().map_or("unknown".to_owned(), |c| c.chars().take(8).collect())
}

fn updates(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    const G: &str = "Neko updates";
    // `--app` while the Swift app runs: it keeps the menu bar, notifications and hotkeys (`shell/mod.rs`); read-only, and it goes when Rune takes them over.
    if s.shell.duties.swift_holds {
        let words = crate::shell::SWIFT_ROW_WORDS;
        let title = text(crate::shell::SWIFT_ROW_TITLE.to_owned()).font(webkit::font(t, 14.0, 500.0, 20.0)).color(t.text.base).truncate().min_w(0.0);
        out.push(item(Section::Updates, G, "swift-app", crate::shell::SWIFT_ROW_TITLE, words, info_row(t, "set-swift-app", title, Some(dim(t, "set-swift-app-words", words)), None, Vec::new(), None)));
    }
    let Some(u) = s.summary.as_ref().and_then(|m| m.settings.self_update.clone()) else { return };
    let busy = u.requested || u.installing.is_some();
    let title = |words: &str| text(words.to_owned()).font(webkit::font(t, 14.0, 500.0, 20.0)).color(t.text.base).truncate().min_w(0.0);

    // Where updates come from (read-only in the web; "Change…" opens the field here).
    let source = if u.source.is_empty() { DEFAULT_SOURCE.to_owned() } else { u.source.clone() };
    let editing = s.open("update-source:edit");
    let change = webkit::link_button(t, "update-source-edit", "Change…", |s: &mut App| s.toggle("update-source:edit"));
    let mut block = col().w_full().child(info_row(t, "set-update-source-row", title("Updates come from"), Some(mono_dim(t, "set-update-source-value", &source)), None, vec![change], None));
    if editing {
        let dirty = s.pages.edited("update-source", false);
        let help = "owner/name or a github.com address. Empty follows Neko’s own repository.";
        let save = big(button(t, "update-source-save", "Save", None, false, false, webkit::gated(dirty, |s: &mut App| s.save_update_source())));
        let form = row().w_full().gap(8.0).center_y().child(webkit::input(s, t, "update-source", "Updates come from", "owner/name", 100.0, true, |s: &mut App| s.save_update_source()).grow(1.0)).child(dim_if(dirty, save));
        block = block.child(with_note(s, t, "setting:update-source", setting_row_stacked(t, "set-update-source", "Updates come from", Some(help), form)));
    } else if let Some(n) = super::ui::note(s, t, "setting:update-source") {
        block = block.child(n);
    }
    out.push(item(Section::Updates, G, "update-source", "Updates come from", "owner/name or a github.com address. Empty follows Neko’s own repository.", block));

    // Last check.
    let last = if !u.check.error.is_empty() { format!("Couldn’t reach it: {}", u.check.error) } else if u.check.at_ms > 0 { webkit::relative_time(s.now_ms, u.check.at_ms) } else { "Not yet".to_owned() };
    let now = big(button(t, "update-now", "Update now", Some(&icons::REFRESH), false, false, webkit::gated(!busy, |s: &mut App| s.call(Call::UpdateNow)))).h(36.0).px(14.0).gap(6.0);
    out.push(item(Section::Updates, G, "update-check", "Last check", "Looks at the published repository now and applies everything new.", with_note(s, t, "setting:updates", info_row(t, "set-update-check", title("Last check"), Some(dim(t, "set-update-last", &last)), None, vec![dim_if(!busy, now)], None))));

    // In use.
    let summary = u.last_update.as_ref().map(|r| r.summary.clone());
    let ui_badge = if u.web_commit.is_some() { webkit::status_nodes(t, "set-version-ui", Tone::Success, &["UI", &sha8(&u.web_commit)]) } else { status(t, "set-version-ui", Tone::Neutral, "UI from the app") };
    let in_use = info_row(t, "set-version", title("In use"), summary.as_deref().map(|d| dim(t, "set-version-summary", d)), None, vec![webkit::status_nodes(t, "set-version-app", Tone::Neutral, &["App", &sha8(&u.native_commit)]), ui_badge], None);
    out.push(item(Section::Updates, G, "update-version", "In use", "", in_use));

    // Waiting, working, failed or nothing.
    let working = if let Some(i) = &u.installing { format!("Reinstalling at {}…", i.to.chars().take(8).collect::<String>()) } else { u.status.clone() };
    let newer = |r: &&neko_protocol::workbench::UpdateRecord| r.at_ms > u.last_update.as_ref().map_or(0, |l| l.at_ms);
    let trouble = [(u.last_failure.as_ref().filter(newer), false), (u.last_rollback.as_ref().filter(newer), true)].into_iter().filter_map(|(r, rolled)| r.map(|r| (r, rolled))).max_by_key(|(r, _)| r.at_ms);
    if let Some(p) = &u.pending {
        let apply = button(t, "update-apply", "Apply update", Some(&icons::REFRESH), true, false, |s: &mut App| s.call(Call::ApplyUpdate)).h(36.0).px(14.0).gap(6.0);
        let reason = p.reason.clone();
        out.push(item(Section::Updates, G, "pending-update", "Apply update", &reason, info_row(t, "set-pending", title(&format!("Update waiting: {}", p.to.chars().take(8).collect::<String>())), Some(dim(t, "set-pending-reason", &reason)), None, vec![dim_if(!busy, apply)], None)));
    } else if !working.is_empty() {
        out.push(item(Section::Updates, G, "update-status", "Now", &working, info_row(t, "set-update-status", title("Now"), Some(dim(t, "set-update-status-value", &working)), None, Vec::new(), None)));
    } else if let Some((r, rolled)) = trouble {
        let badge = status(t, "set-update-failure-sha", if rolled { Tone::Error } else { Tone::Warning }, &r.commit.chars().take(8).collect::<String>());
        let when = text(webkit::relative_time(s.now_ms, r.at_ms)).font(rune::TextStyle { size: 12.0, line_height: 16.0, ..t.ty.mono }).color(t.text.muted);
        out.push(item(Section::Updates, G, "update-failure", if rolled { "Rolled back" } else { "Failed" }, &r.summary, info_row(t, "set-update-failure", title(if rolled { "Rolled back" } else { "Failed" }), Some(dim(t, "set-update-failure-words", &r.summary)), None, vec![badge, when], None)));
    } else {
        out.push(item(Section::Updates, G, "update-pending", "Pending", "Nothing waiting or failed.", info_row(t, "set-update-pending", title("Pending"), Some(dim(t, "set-update-pending-words", "Nothing waiting or failed.")), None, Vec::new(), None)));
    }

    // Which Mac app: the web's select, its words and its confirmation (`ask_set_app_shell`); Cancel changes nothing.
    {
        use neko_protocol::workbench::AppShell;
        let help = "Swift is the current app. Rune is the new one built in Rust; switching rebuilds and reinstalls Neko, and the previous app is kept as a backup.";
        let current = u.app_shell;
        let now = if current == AppShell::Rune { "Rune" } else { "Swift" };
        let options = ["Swift".to_owned(), "Rune".to_owned()];
        let control = dim_if(!busy, webkit::select(s, t, layer, "set-app-shell-pick", "Mac app", &options, now, 112.0, move |s: &mut App, chosen: &str| {
            let shell = if chosen == "Rune" { AppShell::Rune } else { AppShell::Swift };
            if shell != current {
                s.ask_set_app_shell(shell);
            }
        }));
        out.push(item(Section::Updates, G, "app-shell", "Mac app", help, setting_row(t, "set-app-shell", "Mac app", Some(help), control)));
    }

    // The two switches.
    let (ui, native) = (u.auto_ui, u.auto_native);
    let help = "Builds the published web UI, checks it and swaps it in.";
    out.push(item(Section::Updates, G, "auto-update-ui", "Update the web UI automatically", help, setting_row(t, "set-auto-ui", "Update the web UI automatically", Some(help), switch(t, "set-auto-ui-switch", "Update the web UI automatically", ui, move |s: &mut App| s.call(Call::SetSelfUpdate { auto_ui: !ui, auto_native: native })))));
    let help = "When Neko is idle and the Mac is calm. An unhealthy update is rolled back.";
    out.push(item(Section::Updates, G, "auto-update-app", "Reinstall the app automatically", help, setting_row(t, "set-auto-native", "Reinstall the app automatically", Some(help), switch(t, "set-auto-native-switch", "Reinstall the app automatically", native, move |s: &mut App| s.call(Call::SetSelfUpdate { auto_ui: ui, auto_native: !native })))));

    // Development checkout: a stacked row with its field and Save.
    let dirty = s.pages.edited("dev-checkout", false);
    let help = "Where Neko’s own tasks change Neko. They push to the published main; the updater never touches it.";
    let save = big(button(t, "dev-checkout-save", "Save", None, false, false, webkit::gated(dirty, |s: &mut App| s.save_dev_checkout())));
    let form = row().w_full().gap(8.0).center_y().child(webkit::input(s, t, "dev-checkout", "Development checkout", "None", 100.0, true, |s: &mut App| s.save_dev_checkout()).grow(1.0)).child(dim_if(dirty, save));
    out.push(item(Section::Updates, G, "dev-checkout", "Development checkout", help, with_note(s, t, "setting:dev-checkout", setting_row_stacked(t, "set-dev-checkout", "Development checkout", Some(help), form))));

    // Roll back.
    let help = "Back to the build it replaced, or the one the app shipped with.";
    let roll = big(button(t, "roll-back-web", "Roll back", Some(&icons::UNDO), false, false, webkit::gated(!busy, |s: &mut App| s.ask_roll_back_web())));
    out.push(item(Section::Updates, G, "roll-back-web", "Roll back the web UI", help, with_note(s, t, "setting:updates-roll", setting_row(t, "set-roll-back", "Roll back the web UI", Some(help), dim_if(!busy, roll)))));
}

// ---- About --------------------------------------------------------------------------------------

fn about(s: &App, t: &Neko, out: &mut Vec<Item>) {
    const G: &str = "About";
    let title = |words: &str| text(words.to_owned()).font(webkit::font(t, 14.0, 500.0, 20.0)).color(t.text.base).truncate().min_w(0.0);
    let live = matches!(s.link, crate::app::LinkState::Live);
    let line = crate::app::status_line(&s.link, s.summary.as_ref());
    let served = match &s.web {
        Some(w) => format!("Served by the Neko daemon at http://127.0.0.1:{}. Everything on these pages is read from, and written back to, that daemon.", w.port),
        None => "Served by the Neko daemon. Everything on these pages is read from, and written back to, that daemon.".to_owned(),
    };
    out.push(item(Section::About, G, "about-web-interface", "Web interface", &served, info_row(t, "set-about-web-interface", title("Web interface"), Some(dim(t, "set-about-web-interface-value", &served)), None, vec![status(t, "set-about-link-badge", if live { Tone::Success } else { Tone::Warning }, if live { "Connected" } else { "Reconnecting…" })], None)));
    let (workspaces, tasks, connections) = s.summary.as_ref().map_or((0, 0, 0), |m| (m.settings.workspaces.len(), m.tasks.len(), m.settings.connections));
    let this_mac = format!("{workspaces} workspaces · {tasks} tasks in history · {connections} connections");
    out.push(item(Section::About, G, "about-this-mac", "This Mac", &this_mac, info_row(t, "set-about-mac", title("This Mac"), Some(dim(t, "set-about-mac-value", &this_mac)), None, Vec::new(), None)));
    let help = "Neko is built from source, so an update is newer commits on main. The installed app follows the published main by itself (see Neko updates), and the native app can check GitHub too.";
    // The web's button is 221.5 wide (its border and a trailing space): 2 off the right padding keeps the words where they are there.
    let commits = big(webkit::button_after(t, "about-commits", "Open commits on GitHub", &icons::EXTERNAL, |s: &mut App| {
        if (s.opener)(&format!("{PROJECT_REPOSITORY}/commits/main")).is_err() {
            s.notice = Some("Couldn’t open the browser.".to_owned());
        }
    }))
    .pr(14.0);
    out.push(item(Section::About, G, "updates", "Updates and version", help, setting_row(t, "set-about-updates", "Updates and version", Some(help), commits)));
    let code = row().w_full().child(col().px(11.0).py(5.0).radius(8.0).fill(t.text.base.alpha(0.03)).border(1.0, t.surface.border).child(text("git pull && bash scripts/install-native.sh".to_owned()).key("about-by-hand-code").font(rune::TextStyle { size: 12.0, line_height: 16.0, ..t.ty.mono }).color(t.text.base.alpha(0.85)).wrap()));
    out.push(item(Section::About, G, "about-by-hand", "Update by hand", "", info_row(t, "set-about-by-hand", title("Update by hand"), Some(dim(t, "set-about-by-hand-value", "From a source checkout on main with nothing uncommitted:")), None, Vec::new(), Some(code))));

    // Not in the web: the connection in words, the version in use, and the way to the web UI.
    out.push(item(Section::About, G, "about-link", "Connection to Neko", &line, setting_row(t, "set-about-link", "Connection to Neko", None, super::settings::value(t, "set-about-link-value", &line))));
    let u = s.summary.as_ref().and_then(|m| m.settings.self_update.clone());
    let version = u.as_ref().map_or("unknown".to_owned(), |u| format!("app {} · web {}", short(&u.native_commit), short(&u.web_commit)));
    out.push(item(Section::About, G, "about-version", "Version", &version, setting_row(t, "set-about-version", "Version", None, super::settings::value(t, "set-about-version-value", &version))));
    let help = "The web UI stays the default while this app reaches parity.";
    let open = row().gap(8.0).center_y().child(icon(&icons::EXTERNAL, 14.0).color(t.text.muted).a11y_hidden()).child(button(t, "about-open-web", "Open the web UI", None, false, false, |s: &mut App| s.open_in_web(None)));
    out.push(item(Section::About, G, "about-web", "The web UI", help, setting_row(t, "set-about-web", "The web UI", Some(help), open)));
}

#[cfg(test)]
mod tests {
    use super::protected_folder;

    #[test]
    fn the_folders_macos_protects_are_named_as_the_web_names_them() {
        assert_eq!(protected_folder("/Users/example/Documents/acme").as_deref(), Some("Documents"));
        assert_eq!(protected_folder("/Users/example/Desktop").as_deref(), Some("Desktop"));
        assert_eq!(protected_folder("/Users/example/Downloads/x/y").as_deref(), Some("Downloads"));
        assert_eq!(protected_folder("/Users/example/Library/Mobile Documents/com~apple~CloudDocs").as_deref(), Some("iCloud Drive"));
        assert_eq!(protected_folder("/Users/example/code/neko"), None);
        assert_eq!(protected_folder("/Users/example/DocumentsOld"), None);
        assert_eq!(protected_folder("/work/neko"), None);
    }
}
