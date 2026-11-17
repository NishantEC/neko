//! Rune tells the daemon what it shows, the way the web UI does (`web/src/shell/inspect.ts`), so `InspectApp` works with Rune open:
//! `ReportApp` (the open view, the selected session, the panel's tab, the registered viewers, pages and plugins with their source,
//! the theme and the window's size) on change and at most every two seconds, and once a minute while open; `ReportArtifactShown`
//! when a Rune viewer draws an artifact; `ReportArtifactProblem` when one cannot be drawn. Every report says the client is `rune`,
//! so the daemon keeps Rune's window apart from the web UI's. Nothing here reads anything the person has not already got on screen.

use neko_protocol::{AppArtifact, AppFence, AppPage, AppPlugin, AppReport, AppTab, AppTheme, AppViewer};

use crate::app::App;
use crate::link::ExtraCall;
use crate::panel::{Content, Tab, Target};

/// The least time between two reports (the web debounces by 400 ms; the brief says two seconds).
pub const MIN_GAP_MS: i64 = 2000;
/// A window reports at least this often while open: the daemon forgets one silent for three minutes.
pub const HEARTBEAT_MS: i64 = 60_000;

/// A random page id for this run: `r` and twelve base-36 digits, which the daemon's id rule takes (1-64 letters, digits, `-`, `_`).
pub fn new_page_id() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mut n = nanos ^ (u128::from(std::process::id()) << 64);
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut id = String::from("r");
    for _ in 0..12 {
        id.push(digits[(n % 36) as usize] as char);
        n /= 36;
    }
    id
}

/// What the web calls the route of what Rune shows: `tasks?task=<id>`, `setup/tools`, `settings`, a plugin page's id; empty is Home.
pub fn route_of(app: &App) -> String {
    if app.onboarding_shown() && app.home_shown() {
        return "setup".to_owned();
    }
    if let Some(page) = &app.plugin_view {
        return page.id.clone();
    }
    if let Some(view) = app.view {
        return match view {
            crate::views::FullView::Setup => format!("setup/{}", app.setup.section.key()),
            crate::views::FullView::Settings => format!("settings?section={}", app.pages.section.key()),
            other => other.web_id().to_owned(),
        };
    }
    match &app.selected {
        Some(id) => format!("tasks?task={id}"),
        None => String::new(),
    }
}

fn artifact_on_screen(app: &App) -> Option<AppArtifact> {
    let Some(Target::Artifact { id, version }) = app.panel_target() else { return None };
    let sp = app.selected.as_ref().and_then(|t| app.panel.sessions.get(t))?;
    if sp.tab != Some(Tab::Canvas) || !app.layout.context.is_open() {
        return None;
    }
    let kind = app.panel.artifacts.get(&id).and_then(|a| a.versions.iter().find(|v| v.n == version)).and_then(|v| v.file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())).unwrap_or_default();
    let viewer = crate::viewers::resolve(&kind).map(|s| s.id.to_owned());
    Some(AppArtifact { id, version, kind, viewer, place: "canvas".to_owned() })
}

/// What this window reports now. `full` is true while the window has the keyboard (the web's "used last").
pub fn build(app: &App, page_id: &str, size: Option<[u32; 2]>) -> AppReport {
    let base = AppReport { page_id: page_id.to_owned(), client: "rune".to_owned(), size, full: app.focused, focused: app.focused, visible: true, route: route_of(app), title: "Neko (Rune)".to_owned(), ..AppReport::default() };
    if !app.focused {
        return base;
    }
    let route = base.route.clone();
    let tabs = match app.selected.as_ref().and_then(|t| app.panel.sessions.get(t)) {
        Some(sp) => Tab::ALL.iter().map(|tab| AppTab { key: format!("panel:{}", tab.title().to_lowercase()), title: tab.title().to_owned(), href: format!("#/{route}"), active: sp.tab == Some(*tab) && app.layout.context.is_open(), pinned: false }).collect(),
        None => Vec::new(),
    };
    let mut pages: Vec<AppPage> = crate::views::FullView::ALL.iter().map(|v| AppPage { id: v.web_id().to_owned(), title: v.title().to_owned(), plugin: None }).collect();
    for p in app.plugin_pages.iter().flatten() {
        pages.push(AppPage { id: p.id.clone(), title: p.title.clone(), plugin: Some(p.plugin.clone()) });
    }
    let viewers = crate::viewers::listed()
        .into_iter()
        .map(|(group, spec)| AppViewer {
            id: spec.id.to_owned(),
            title: spec.title.to_owned(),
            kinds: spec.kinds.iter().map(|k| (*k).to_owned()).collect(),
            priority: i64::from(spec.priority),
            plugin: (group == "web-plugin").then(|| spec.id.split(':').nth(1).unwrap_or_default().to_owned()).filter(|p| !p.is_empty()),
        })
        .collect();
    let fences = ["csv", "tsv", "3d"].iter().map(|l| AppFence { language: (*l).to_owned(), viewer: Some(if *l == "3d" { "model3d" } else { "table" }.to_owned()), plugin: None }).collect();
    let plugins = app
        .pages
        .plugins
        .iter()
        .flatten()
        .map(|p| AppPlugin { id: p.id.clone(), loaded: p.ok && p.problem.is_none(), disabled: app.plugin_ui.listings.iter().flatten().any(|l| l.id == p.id && l.disabled), problem: p.problem.clone() })
        .collect();
    let (light, dark) = app.theme_choice();
    let dark_now = app.theme_now_dark();
    let id = if dark_now { dark } else { light };
    let name = app.theme_entries().into_iter().find(|t| t.id == id).map_or_else(|| id.clone(), |t| t.name);
    let artifact = artifact_on_screen(app);
    AppReport {
        tabs,
        pages,
        viewers,
        fences,
        plugins,
        artifacts: artifact.iter().cloned().collect(),
        canvas: artifact,
        theme: Some(AppTheme { mode: app.pages.appearance.clone().unwrap_or_else(|| "system".to_owned()), appearance: if dark_now { "dark" } else { "light" }.to_owned(), id, name }),
        ..base
    }
}

/// What the last report held (its JSON) and when it was sent.
#[derive(Default)]
pub struct Reporter {
    pub page_id: String,
    last: String,
    sent_at: Option<i64>,
    /// `(artifact, version)` already told to the daemon as shown or as a problem.
    told: std::collections::HashSet<(String, u32, bool)>,
}

impl Reporter {
    /// The report to send now, if there is one: it changed and two seconds passed, or a minute passed. None says nothing.
    pub fn due(&mut self, now_ms: i64, report: &AppReport) -> Option<()> {
        let json = serde_json::to_string(report).ok()?;
        let since = self.sent_at.map(|at| now_ms - at);
        let changed = json != self.last;
        let send = match since {
            None => true,
            Some(s) => (changed && s >= MIN_GAP_MS) || s >= HEARTBEAT_MS,
        };
        if send {
            self.last = json;
            self.sent_at = Some(now_ms);
            Some(())
        } else {
            None
        }
    }

    /// The milliseconds until a changed report may go out (to ask for a wake-up), none when nothing is waiting.
    pub fn waiting(&self, now_ms: i64, report: &AppReport) -> Option<i64> {
        let json = serde_json::to_string(report).ok()?;
        (json != self.last).then(|| self.sent_at.map_or(0, |at| (MIN_GAP_MS - (now_ms - at)).max(0)))
    }

    pub fn first_time(&mut self, id: &str, version: u32, problem: bool) -> bool {
        self.told.insert((id.to_owned(), version, problem))
    }
}

impl App {
    /// Sends the report when one is due. Returns how long to wait before looking again, if a change is waiting.
    pub fn tick_report(&mut self, size: Option<[u32; 2]>) -> Option<i64> {
        if !self.linked || matches!(self.dispatch, crate::app::Dispatch::None) {
            return None;
        }
        if self.reporter.page_id.is_empty() {
            self.reporter.page_id = new_page_id();
        }
        let now = (self.clock)();
        let report = build(self, &self.reporter.page_id.clone(), size);
        if self.reporter.due(now, &report).is_some() {
            self.extra_call(ExtraCall::Report(Box::new(report)));
            return None;
        }
        self.reporter.waiting(now, &report)
    }

    /// A Rune viewer drew an artifact, or could not: the daemon is told once per version (the web's `reportShown` and `reportProblem`).
    pub fn report_artifact(&mut self, target: &Target) {
        let Target::Artifact { id, version } = target else { return };
        let Some(open) = self.panel.open.as_ref().filter(|o| &o.target == target) else { return };
        match &open.content {
            Content::Ready(loaded) => match crate::viewers::resolve_for(&loaded.kind, Some(loaded.bytes())) {
                // The web page's own viewers report themselves from inside the page.
                Some(spec) if crate::viewers::web::is_web(spec.id) => {}
                Some(spec) => {
                    if self.reporter.first_time(id, *version, false) {
                        let viewer = spec.id.to_owned();
                        self.extra_call(ExtraCall::ArtifactShown { id: id.clone(), version: *version, viewer });
                    }
                }
                None => {
                    if self.reporter.first_time(id, *version, true) {
                        let message = format!("No viewer draws .{} files.", loaded.kind);
                        self.extra_call(ExtraCall::ArtifactProblem { id: id.clone(), version: *version, kind: "viewer".to_owned(), message, viewer: None });
                    }
                }
            },
            Content::Failed(message) => {
                if self.reporter.first_time(id, *version, true) {
                    self.extra_call(ExtraCall::ArtifactProblem { id: id.clone(), version: *version, kind: "viewer".to_owned(), message: message.chars().take(500).collect(), viewer: None });
                }
            }
            Content::Loading | Content::TooLarge { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_id_is_one_the_daemon_takes() {
        let id = new_page_id();
        assert!(id.len() == 13 && id.starts_with('r') && id.bytes().all(|b| b.is_ascii_alphanumeric()), "{id}");
    }

    #[test]
    fn a_report_says_rune_and_what_is_open() {
        let mut app = App::headless();
        app.focused = true;
        let home = build(&app, "r1", Some([1360, 840]));
        assert_eq!((home.client.as_str(), home.route.as_str(), home.size, home.full), ("rune", "", Some([1360, 840]), true));
        assert!(home.viewers.iter().any(|v| v.id == "image" && v.plugin.is_none()) && home.pages.iter().any(|p| p.id == "settings"));
        assert_eq!(home.theme.as_ref().map(|t| t.mode.as_str()), Some("system"));
        app.open_view(crate::views::FullView::Settings);
        assert_eq!(route_of(&app), "settings?section=general");
        app.selected = Some("t1".into());
        app.view = None;
        assert_eq!(route_of(&app), "tasks?task=t1");
    }

    #[test]
    fn a_window_without_the_keyboard_says_only_where_it_is() {
        let mut app = App::headless();
        app.focused = false;
        let r = build(&app, "r1", None);
        assert!(!r.full && r.viewers.is_empty() && r.pages.is_empty() && r.theme.is_none() && r.client == "rune");
    }

    #[test]
    fn a_report_goes_out_on_change_at_most_every_two_seconds_and_once_a_minute_otherwise() {
        let mut rep = Reporter::default();
        let mut r = AppReport { page_id: "r1".into(), client: "rune".into(), ..AppReport::default() };
        assert!(rep.due(0, &r).is_some(), "the first one goes at once");
        assert!(rep.due(500, &r).is_none(), "nothing changed");
        r.route = "settings".into();
        assert!(rep.due(1_000, &r).is_none(), "changed, but not two seconds yet");
        assert_eq!(rep.waiting(1_000, &r), Some(1_000));
        assert!(rep.due(2_000, &r).is_some());
        assert!(rep.due(30_000, &r).is_none());
        assert!(rep.due(62_000, &r).is_some(), "the heartbeat");
    }

    #[test]
    fn an_artifact_is_told_about_once_per_version() {
        let mut rep = Reporter::default();
        assert!(rep.first_time("a", 1, false) && !rep.first_time("a", 1, false));
        assert!(rep.first_time("a", 2, false) && rep.first_time("a", 1, true));
    }
}
