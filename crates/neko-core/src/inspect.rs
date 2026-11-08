//! What open Neko pages tell the daemon about the screen, so an agent can
//! check its work instead of assuming it
//! (`docs/superpowers/specs/2026-11-08-everything-renders-design.md`, B1 and B2).
//!
//! * [`App`]: each open window reports where it is (`ReportApp`); the window
//!   the person used last also sends what it has registered and draws (tabs,
//!   pages, viewers, fences, plugins, artifacts, theme). `InspectApp` answers
//!   from the last full report, with its age, or says that no page reported.
//! * [`Artifacts`]: what pages saw while drawing each artifact version (when
//!   one was last shown, with which viewer, and its errors), for `Artifact`.
//!
//! Both live in memory only, like `PluginStatus`: nothing here is written to
//! disk, and every string and list is bounded before it is kept.
use std::collections::BTreeMap;

use neko_protocol::{AppArtifact, AppReport, AppWindow, ArtifactProblem, ArtifactVersionStatus};

/// Problems kept per artifact version (the oldest goes first).
pub const MAX_PROBLEMS: usize = 20;
/// Characters kept of one problem's message.
pub const MAX_MESSAGE: usize = 500;
/// Artifact versions tracked at once (the least recently touched goes first).
pub const MAX_TRACKED: usize = 200;
/// What a problem may be.
pub const PROBLEM_KINDS: &[&str] = &["error", "rejection", "console", "viewer"];
/// Windows report at least once a minute; one silent for this long is gone.
pub const WINDOW_GONE_MS: i64 = 180_000;
const MAX_WINDOWS: usize = 20;

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn plain_id(text: &str) -> bool {
    (1..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn bound_artifact(a: &mut AppArtifact) {
    a.id = clip(&a.id, 64);
    a.kind = clip(&a.kind, 16);
    a.viewer = a.viewer.as_deref().map(|v| clip(v, 100));
    a.place = clip(&a.place, 16);
}

/// Every list and string of a report cut to a size worth keeping.
fn bound(report: &mut AppReport) {
    report.route = clip(&report.route, 300);
    report.title = clip(&report.title, 200);
    report.tabs.truncate(60);
    for tab in &mut report.tabs {
        tab.key = clip(&tab.key, 200);
        tab.title = clip(&tab.title, 200);
        tab.href = clip(&tab.href, 300);
    }
    report.pages.truncate(200);
    for page in &mut report.pages {
        page.id = clip(&page.id, 100);
        page.title = clip(&page.title, 200);
        page.plugin = page.plugin.as_deref().map(|p| clip(p, 64));
    }
    report.viewers.truncate(100);
    for viewer in &mut report.viewers {
        viewer.id = clip(&viewer.id, 100);
        viewer.title = clip(&viewer.title, 200);
        viewer.kinds.truncate(40);
        viewer.kinds.iter_mut().for_each(|k| *k = clip(k, 16));
        viewer.plugin = viewer.plugin.as_deref().map(|p| clip(p, 64));
    }
    report.fences.truncate(100);
    for fence in &mut report.fences {
        fence.language = clip(&fence.language, 32);
        fence.viewer = fence.viewer.as_deref().map(|v| clip(v, 100));
        fence.plugin = fence.plugin.as_deref().map(|p| clip(p, 64));
    }
    report.plugins.truncate(100);
    for plugin in &mut report.plugins {
        plugin.id = clip(&plugin.id, 64);
        plugin.problem = plugin.problem.as_deref().map(|p| clip(p, MAX_MESSAGE));
    }
    report.artifacts.truncate(20);
    report.artifacts.iter_mut().for_each(bound_artifact);
    if let Some(canvas) = &mut report.canvas { bound_artifact(canvas); }
    if let Some(theme) = &mut report.theme {
        theme.mode = clip(&theme.mode, 16);
        theme.appearance = clip(&theme.appearance, 16);
        theme.id = clip(&theme.id, 100);
        theme.name = clip(&theme.name, 100);
    }
}

/// The open windows, and the last full report.
#[derive(Debug, Default)]
pub struct App {
    windows: BTreeMap<String, AppWindow>,
    full: Option<AppReport>,
    /// Some window has reported since the daemon started.
    heard: bool,
}

/// `InspectApp`'s answer, before it becomes a `Response::App`.
#[derive(Debug, PartialEq, Eq)]
pub struct Inspection {
    pub note: String,
    pub window: Option<AppReport>,
    pub age_ms: Option<i64>,
    pub windows: Vec<AppWindow>,
}

fn seconds(ms: i64) -> String {
    let s = (ms.max(0) + 500) / 1000;
    if s == 1 { "1 second".into() } else { format!("{s} seconds") }
}

impl App {
    /// Keeps one window's report. A full report replaces the last full one,
    /// whichever window sent it; a closing window is forgotten.
    pub fn report(&mut self, mut report: AppReport, now: i64) -> Result<(), String> {
        if !plain_id(&report.page_id) {
            return Err("A page id is 1-64 letters, digits, - or _.".into());
        }
        if report.closed {
            self.windows.remove(&report.page_id);
            if self.full.as_ref().is_some_and(|f| f.page_id == report.page_id) { self.full = None; }
            return Ok(());
        }
        bound(&mut report);
        report.at_ms = now;
        let window = AppWindow {
            page_id: report.page_id.clone(),
            focused: report.focused,
            visible: report.visible,
            route: report.route.clone(),
            title: report.title.clone(),
            at_ms: now,
            reporting: false,
        };
        if !self.windows.contains_key(&window.page_id) && self.windows.len() >= MAX_WINDOWS {
            if let Some(oldest) = self.windows.values().min_by_key(|w| w.at_ms).map(|w| w.page_id.clone()) {
                self.windows.remove(&oldest);
            }
        }
        self.windows.insert(window.page_id.clone(), window);
        self.heard = true;
        if report.full {
            self.full = Some(report);
        } else if let Some(full) = self.full.as_mut().filter(|f| f.page_id == report.page_id) {
            // The reporting window, between full reports: keep where it is current.
            full.focused = report.focused;
            full.visible = report.visible;
            full.route = report.route;
            full.title = report.title;
            full.at_ms = now;
        }
        Ok(())
    }

    /// What the window used last shows, and every window still open.
    pub fn inspect(&mut self, now: i64) -> Inspection {
        self.windows.retain(|_, w| now - w.at_ms < WINDOW_GONE_MS);
        if self.full.as_ref().is_some_and(|f| !self.windows.contains_key(&f.page_id)) { self.full = None; }
        let reporting = self.full.as_ref().map(|f| f.page_id.clone());
        let mut windows: Vec<AppWindow> = self.windows.values().cloned().map(|mut w| {
            w.reporting = reporting.as_deref() == Some(w.page_id.as_str());
            w
        }).collect();
        windows.sort_by_key(|w| std::cmp::Reverse(w.at_ms));
        let others = windows.len().saturating_sub(usize::from(reporting.is_some()));
        let note = match &self.full {
            Some(full) => {
                let route = if full.route.is_empty() { "home".to_string() } else { full.route.clone() };
                let state = match (full.focused, full.visible) {
                    (true, _) => "focused",
                    (false, true) => "visible, not focused",
                    (false, false) => "hidden (another app or tab is in front)",
                };
                let more = match others { 0 => String::new(), 1 => " 1 other Neko window is open.".into(), n => format!(" {n} other Neko windows are open.") };
                format!("The Neko window used last ({state}) reported {} ago; its route is #/{route}.{more}", seconds(now - full.at_ms))
            }
            None if windows.is_empty() && !self.heard => "No open Neko page has reported since the daemon started, so nothing is known about the screen: open Neko (or `neko-daemon --open-page <route>`) and ask again.".into(),
            None if windows.is_empty() => "No Neko window has reported in the last 3 minutes (open ones report once a minute), so none seems open and nothing is known about the screen: open Neko (or `neko-daemon --open-page <route>`) and ask again.".into(),
            None => format!("{} Neko window(s) are open, but none has been used since the one used last closed; what they show is not known until one is focused.", windows.len()),
        };
        let age_ms = self.full.as_ref().map(|f| (now - f.at_ms).max(0));
        Inspection { note, window: self.full.clone(), age_ms, windows }
    }
}

#[derive(Debug, Default)]
struct Seen {
    shown_at_ms: Option<i64>,
    viewer: Option<String>,
    problems: Vec<ArtifactProblem>,
    touched: u64,
}

/// What open pages saw while drawing artifact versions.
#[derive(Debug, Default)]
pub struct Artifacts {
    versions: BTreeMap<(String, u32), Seen>,
    clock: u64,
}

impl Artifacts {
    fn entry(&mut self, id: &str, version: u32) -> &mut Seen {
        self.clock += 1;
        let key = (id.to_string(), version);
        if !self.versions.contains_key(&key) && self.versions.len() >= MAX_TRACKED {
            if let Some(oldest) = self.versions.iter().min_by_key(|(_, s)| s.touched).map(|(k, _)| k.clone()) {
                self.versions.remove(&oldest);
            }
        }
        let clock = self.clock;
        let seen = self.versions.entry(key).or_default();
        seen.touched = clock;
        seen
    }

    /// Keeps one problem; the same kind and message again counts up instead.
    /// The caller has checked that the version exists.
    pub fn problem(&mut self, id: &str, version: u32, kind: &str, message: &str, viewer: Option<&str>, now: i64) -> Result<(), String> {
        if !PROBLEM_KINDS.contains(&kind) {
            return Err(format!("A problem's kind is one of {}.", PROBLEM_KINDS.join(", ")));
        }
        let message = clip(message.trim(), MAX_MESSAGE);
        if message.is_empty() {
            return Err("A problem needs a message.".into());
        }
        let seen = self.entry(id, version);
        if seen.viewer.is_none() { seen.viewer = viewer.map(|v| clip(v, 100)); }
        if let Some(same) = seen.problems.iter_mut().find(|p| p.kind == kind && p.message == message) {
            same.count = same.count.saturating_add(1);
            same.last_ms = now;
            return Ok(());
        }
        if seen.problems.len() >= MAX_PROBLEMS { seen.problems.remove(0); }
        seen.problems.push(ArtifactProblem { kind: kind.to_string(), message, count: 1, first_ms: now, last_ms: now });
        Ok(())
    }

    /// A page drew this version with `viewer`.
    pub fn shown(&mut self, id: &str, version: u32, viewer: &str, now: i64) {
        let seen = self.entry(id, version);
        seen.shown_at_ms = Some(now);
        seen.viewer = Some(clip(viewer, 100));
    }

    /// Every version of `id` a page has drawn or reported on, oldest first.
    pub fn status(&self, id: &str) -> Vec<ArtifactVersionStatus> {
        self.versions
            .iter()
            .filter(|((artifact, _), _)| artifact == id)
            .map(|((_, version), seen)| ArtifactVersionStatus {
                version: *version,
                shown_at_ms: seen.shown_at_ms,
                viewer: seen.viewer.clone(),
                problems: seen.problems.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::{AppTab, AppViewer};

    fn window(page: &str, full: bool) -> AppReport {
        AppReport { page_id: page.into(), full, focused: full, visible: true, route: "tasks".into(), title: "Neko".into(), ..AppReport::default() }
    }

    #[test]
    fn inspect_says_when_no_page_has_reported() {
        let mut app = App::default();
        let seen = app.inspect(1_000);
        assert!(seen.window.is_none() && seen.age_ms.is_none() && seen.windows.is_empty());
        assert!(seen.note.starts_with("No open Neko page has reported"), "{}", seen.note);
    }

    #[test]
    fn the_full_report_of_the_window_used_last_is_what_inspect_returns() {
        let mut app = App::default();
        let mut first = window("a", true);
        first.tabs = vec![AppTab { key: "home".into(), title: "Home".into(), href: "#/".into(), active: true, pinned: true }];
        first.viewers = vec![AppViewer { id: "html".into(), title: "Page".into(), kinds: vec!["html".into()], priority: 0, plugin: None }];
        first.canvas = Some(AppArtifact { id: "0a1b2c3d4e5f".into(), version: 2, kind: "html".into(), viewer: Some("html".into()), place: "canvas".into() });
        app.report(first, 1_000).unwrap();
        // Another window, not used last, only says where it is.
        app.report(window("b", false), 2_000).unwrap();
        let seen = app.inspect(3_000);
        let shown = seen.window.as_ref().unwrap();
        assert_eq!(shown.page_id, "a");
        assert_eq!(shown.tabs[0].key, "home");
        assert_eq!(shown.viewers[0].kinds, vec!["html".to_string()]);
        assert_eq!(shown.canvas.as_ref().unwrap().version, 2);
        assert_eq!(seen.age_ms, Some(2_000));
        assert_eq!(seen.windows.len(), 2);
        assert!(seen.windows.iter().any(|w| w.page_id == "a" && w.reporting) && seen.windows.iter().any(|w| w.page_id == "b" && !w.reporting));
        assert!(seen.note.contains("focused") && seen.note.contains("2 seconds ago") && seen.note.contains("#/tasks") && seen.note.contains("1 other Neko window"), "{}", seen.note);
        // The JSON an agent reads keeps the field names the protocol documents.
        let json = serde_json::to_value(shown).unwrap();
        for field in ["page_id", "route", "tabs", "pages", "viewers", "fences", "plugins", "artifacts", "canvas", "theme", "at_ms"] {
            assert!(json.get(field).is_some(), "{field}");
        }

        // Its light reports keep the full one current; the other window using it takes over.
        let mut moved = window("a", false);
        moved.route = "calendar".into();
        app.report(moved, 4_000).unwrap();
        assert_eq!(app.inspect(4_000).window.unwrap().route, "calendar");
        app.report(window("b", true), 5_000).unwrap();
        assert_eq!(app.inspect(5_000).window.unwrap().page_id, "b");
    }

    #[test]
    fn closed_and_silent_windows_are_forgotten() {
        let mut app = App::default();
        app.report(window("a", true), 0).unwrap();
        app.report(AppReport { page_id: "a".into(), closed: true, ..AppReport::default() }, 10).unwrap();
        let seen = app.inspect(20);
        assert!(seen.window.is_none() && seen.windows.is_empty());
        app.report(window("b", true), 100).unwrap();
        app.report(window("c", false), WINDOW_GONE_MS).unwrap();
        let seen = app.inspect(WINDOW_GONE_MS + 100);
        assert!(seen.window.is_none(), "b went silent");
        assert_eq!(seen.windows.len(), 1);
        assert!(seen.note.contains("none has been used"), "{}", seen.note);
        // Every window gone quiet: not "since the daemon started".
        let quiet = app.inspect(WINDOW_GONE_MS * 3);
        assert!(quiet.windows.is_empty() && quiet.note.starts_with("No Neko window has reported in the last 3 minutes"), "{}", quiet.note);
    }

    #[test]
    fn reports_are_bounded_and_page_ids_checked() {
        let mut app = App::default();
        assert!(app.report(AppReport { page_id: "../x".into(), ..AppReport::default() }, 0).is_err());
        assert!(app.report(AppReport { page_id: String::new(), ..AppReport::default() }, 0).is_err());
        let mut big = window("a", true);
        big.route = "r".repeat(5_000);
        big.tabs = (0..500).map(|i| AppTab { key: format!("k{i}"), href: "#/".into(), ..AppTab::default() }).collect();
        app.report(big, 0).unwrap();
        let shown = app.inspect(0).window.unwrap();
        assert_eq!(shown.route.len(), 300);
        assert_eq!(shown.tabs.len(), 60);
        // A viewer with a fractional or odd priority does not void the report.
        let parsed: AppReport = serde_json::from_value(serde_json::json!({
            "page_id": "p", "full": true,
            "viewers": [{ "id": "a", "priority": 1.5 }, { "id": "b", "priority": -2 }, { "id": "c", "priority": null }, { "id": "d" }],
        })).unwrap();
        assert_eq!(parsed.viewers.iter().map(|v| v.priority).collect::<Vec<_>>(), vec![1, -2, 0, 0]);
        for i in 0..40 { app.report(window(&format!("w{i}"), false), i).unwrap(); }
        assert!(app.inspect(50).windows.len() <= MAX_WINDOWS);
    }

    #[test]
    fn problems_are_bounded_counted_and_kept_per_version() {
        let mut seen = Artifacts::default();
        assert!(seen.problem("a", 1, "fatal", "x", None, 0).is_err(), "unknown kind");
        assert!(seen.problem("a", 1, "error", "   ", None, 0).is_err(), "empty message");
        seen.problem("a", 1, "error", &"e".repeat(2_000), Some("html"), 1).unwrap();
        let status = seen.status("a");
        assert_eq!(status[0].problems[0].message.chars().count(), MAX_MESSAGE);
        assert_eq!(status[0].viewer.as_deref(), Some("html"));
        assert_eq!(status[0].shown_at_ms, None);
        // The same problem again counts up.
        seen.problem("a", 2, "console", "boom", None, 5).unwrap();
        seen.problem("a", 2, "console", "boom", None, 9).unwrap();
        let v2 = seen.status("a").into_iter().find(|s| s.version == 2).unwrap();
        assert_eq!((v2.problems.len(), v2.problems[0].count, v2.problems[0].first_ms, v2.problems[0].last_ms), (1, 2, 5, 9));
        // At most MAX_PROBLEMS per version, the oldest dropped.
        for i in 0..50 { seen.problem("a", 3, "error", &format!("e{i}"), None, i).unwrap(); }
        let v3 = seen.status("a").into_iter().find(|s| s.version == 3).unwrap();
        assert_eq!(v3.problems.len(), MAX_PROBLEMS);
        assert_eq!(v3.problems[0].message, "e30");
        assert_eq!(v3.problems.last().unwrap().message, "e49");
        // Shown is per version, and other artifacts are not mixed in.
        seen.shown("a", 3, "html", 77);
        seen.shown("b", 1, "csv", 78);
        assert_eq!(seen.status("a").iter().map(|s| s.version).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(seen.status("a")[2].shown_at_ms, Some(77));
        assert_eq!(seen.status("b").len(), 1);
    }

    #[test]
    fn at_most_max_tracked_versions_are_remembered() {
        let mut seen = Artifacts::default();
        for i in 0..(MAX_TRACKED as u32 + 25) { seen.shown(&format!("x{i}"), 1, "html", 0); }
        assert_eq!(seen.versions.len(), MAX_TRACKED);
        assert!(seen.status("x0").is_empty(), "the least recently touched went first");
        assert_eq!(seen.status(&format!("x{}", MAX_TRACKED + 24)).len(), 1);
    }
}
