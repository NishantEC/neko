//! ⌘K: the command palette, as the web's `shell/CommandPalette.tsx` lists it, in this order: **Sessions** (the latest, or what the
//! words match), **Go to** (every full view), **Actions** (for the selected session, and the shell's own), **Appearance** (the mode
//! and the theme) and **Plugins** (what the plugins registered). Words are matched with `frizbee` (typo tolerant, like fzf). Up and
//! Down move, Return runs, Esc closes. The view is `view/palette.rs`.

use neko_protocol::workbench::TaskStatus;
use rune::Icon;

use crate::app::App;
use crate::icons;
use crate::task_actions::Act;
use crate::views::FullView;

/// How many sessions the palette lists when nothing is typed (the web's six).
pub const RECENT: usize = 6;
/// How many session matches it lists for words.
pub const MATCHES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Sessions,
    GoTo,
    Actions,
    Appearance,
    Plugins,
}

impl Section {
    pub const ALL: [Section; 5] = [Section::Sessions, Section::GoTo, Section::Actions, Section::Appearance, Section::Plugins];

    pub fn title(self) -> &'static str {
        match self {
            Section::Sessions => "Sessions",
            Section::GoTo => "Go to",
            Section::Actions => "Actions",
            Section::Appearance => "Appearance",
            Section::Plugins => "Plugins",
        }
    }
}

/// What a row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    OpenSession(String),
    GoTo(FullView),
    Home,
    /// One of the selected session's menu items.
    Task(String, Act),
    Approve(String),
    Complete(String),
    Start(String),
    OpenPullRequest(String),
    StopAll,
    ApproveWaiting,
    Shortcuts,
    ToggleSidebar,
    /// System, light or dark.
    Mode(&'static str),
    /// A theme: the mode that shows it.
    Theme(&'static str),
    Plugin(neko_protocol::PluginPage),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub section: Section,
    pub title: String,
    /// Quiet words after the title ("3 waiting").
    pub hint: Option<String>,
    /// Right-aligned: a shortcut, or a state word.
    pub right: Option<String>,
    pub icon: &'static Icon,
    pub keywords: &'static [&'static str],
    pub run: Do,
}

fn item(section: Section, title: impl Into<String>, icon: &'static Icon, run: Do) -> Item {
    Item { section, title: title.into(), hint: None, right: None, icon, keywords: &[], run }
}

impl App {
    /// Every row the palette could show, in the web's order, before the words narrow them.
    pub fn palette_all(&self) -> Vec<Item> {
        let mut out = Vec::new();
        let Some(summary) = self.summary.as_ref() else { return out };
        // Sessions, the latest first.
        let mut tasks: Vec<&crate::link::TaskRow> = summary.tasks.iter().collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.updated_at_ms));
        for task in tasks {
            let mut it = item(Section::Sessions, task.title.clone(), &icons::CHEVRON_RIGHT, Do::OpenSession(task.id.clone()));
            it.right = Some(format!("{} · {}", crate::groups::status_word(task.status), crate::groups::age(self.now_ms, task.updated_at_ms)));
            it.hint = task.key.clone();
            out.push(it);
        }
        // Go to: every full view.
        let mut home = item(Section::GoTo, "Go to Home", &icons::SPARK, Do::Home);
        home.right = Some("⌘N".to_owned());
        out.push(home);
        for view in FullView::ALL.into_iter().chain([FullView::AllSessions]) {
            out.push(item(Section::GoTo, format!("Go to {}", view.title()), view.icon(), Do::GoTo(view)));
        }
        // Actions: the selected session's, then the shell's.
        if let Some(task) = self.session() {
            let id = task.id.clone();
            let meta_pr = summary.meta_of(&id).and_then(|m| m.pull_request.clone());
            match task.status {
                TaskStatus::AwaitingApproval if self.detail().is_none_or(|d| crate::work::waiting_reason(&d.task).is_none()) => out.push(item(Section::Actions, "Approve selected session", &icons::CHECK_LINE, Do::Approve(id.clone()))),
                TaskStatus::ReadyForReview => {
                    if let Some(pr) = meta_pr {
                        out.push(item(Section::Actions, "Open the selected session’s pull request", &icons::PULL_REQUEST, Do::OpenPullRequest(pr)));
                    }
                    out.push(item(Section::Actions, "Mark selected session done", &icons::CHECK_LINE, Do::Complete(id.clone())));
                }
                TaskStatus::Failed | TaskStatus::Cancelled => out.push(item(Section::Actions, "Start selected session again", &icons::PLAY, Do::Start(id.clone()))),
                _ => {}
            }
            for e in crate::task_actions::entries(self, &id) {
                let title = match e.act {
                    Act::Rename => "Rename selected session".to_owned(),
                    // The web's palette offers only these (the ones `taskActions` gives a palette sentence).
                    Act::Retry | Act::Publish | Act::LandOnMain | Act::MarkUnread | Act::CopyLink | Act::OpenWeb => continue,
                    Act::RunNext => "Run selected session next".to_owned(),
                    Act::Stop => "Stop selected session".to_owned(),
                    Act::Cancel => "Cancel selected session".to_owned(),
                    Act::Delete => "Delete selected session".to_owned(),
                };
                out.push(item(Section::Actions, title, if e.danger { &icons::CLOSE_SMALL } else { &icons::CHEVRON_RIGHT }, Do::Task(id.clone(), e.act)));
            }
        }
        let mut stop = item(Section::Actions, "Stop all work", &icons::CLOSE_SMALL, Do::StopAll);
        stop.right = Some("⌘⇧Esc".to_owned());
        stop.keywords = &["cancel", "halt", "kill", "agents"];
        out.push(stop);
        let waiting = crate::attention::pending_decisions(summary).len();
        if waiting > 0 {
            let mut a = item(Section::Actions, "Approve what is waiting", &icons::CHECK_LINE, Do::ApproveWaiting);
            a.hint = Some(format!("{waiting} waiting"));
            a.right = Some("⌘↵".to_owned());
            out.push(a);
        }
        let mut keys = item(Section::Actions, "Keyboard shortcuts", &icons::KEYBOARD, Do::Shortcuts);
        keys.right = Some("?".to_owned());
        keys.keywords = &["keys", "help", "hotkeys"];
        out.push(keys);
        let mut side = item(Section::Actions, "Show or hide the sidebar", &icons::SIDEBAR, Do::ToggleSidebar);
        side.right = Some("⌘B".to_owned());
        out.push(side);
        // Appearance: the mode, then the themes this window has.
        for (title, mode, glyph) in [("Mode: follow the system", "system", &icons::GAUGE), ("Mode: always light", "light", &icons::EYE), ("Mode: always dark", "dark", &icons::SPARK)] {
            let mut it = item(Section::Appearance, title, glyph, Do::Mode(mode));
            it.keywords = &["appearance", "theme"];
            if self.pages.appearance.as_deref().unwrap_or("system") == mode {
                it.right = Some("current".to_owned());
            }
            out.push(it);
        }
        for (name, mode) in [("Theme: Neko Neutral", "dark"), ("Theme: Neko Neutral Light", "light")] {
            let mut it = item(Section::Appearance, name, &icons::SPARK, Do::Theme(mode));
            it.right = Some(mode.to_owned());
            it.keywords = &["appearance", "theme"];
            out.push(it);
        }
        // Plugins: what they registered.
        for page in self.plugin_pages.iter().flatten() {
            let mut it = item(Section::Plugins, format!("Open {}", page.title), &icons::WRENCH, Do::Plugin(page.clone()));
            it.hint = Some(page.plugin.clone());
            out.push(it);
        }
        out
    }

    /// What the palette shows now: with no words, the latest sessions and everything else; with words, the rows that match, best
    /// first inside their section. Sections keep their order.
    pub fn palette_items(&self) -> Vec<Item> {
        let all = self.palette_all();
        let q = self.palette_query.text.trim();
        let mut out = Vec::new();
        for section in Section::ALL {
            let rows: Vec<&Item> = all.iter().filter(|i| i.section == section).collect();
            if q.is_empty() {
                let keep = if section == Section::Sessions { RECENT } else { usize::MAX };
                out.extend(rows.into_iter().take(keep).cloned());
                continue;
            }
            let hay: Vec<String> = rows.iter().map(|i| format!("{} {} {}", i.title, i.hint.as_deref().unwrap_or(""), i.keywords.join(" "))).collect();
            let refs: Vec<&str> = hay.iter().map(String::as_str).collect();
            let mut matcher = frizbee::Matcher::new(q, &frizbee::Config::default());
            let mut found = matcher.match_list(&refs);
            found.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
            let keep = if section == Section::Sessions { MATCHES } else { usize::MAX };
            out.extend(found.into_iter().take(keep).map(|m| rows[m.index as usize].clone()));
        }
        out
    }

    /// Runs a row and closes the palette.
    pub fn palette_run(&mut self, run: Do) {
        self.palette = None;
        match run {
            Do::OpenSession(id) => self.select(&id),
            Do::GoTo(view) => {
                if view == FullView::AllSessions {
                    self.open_all_sessions();
                } else {
                    self.open_view(view);
                }
            }
            Do::Home => self.open_new(),
            Do::Task(id, act) => self.do_action(&id, act),
            Do::Approve(task_id) => self.call(crate::link::Call::Approve { task_id }),
            Do::Complete(task_id) => self.call(crate::link::Call::Complete { task_id }),
            Do::Start(task_id) => self.call(crate::link::Call::Start { task_id }),
            Do::OpenPullRequest(url) => self.open_link(&url),
            Do::StopAll => self.ask_stop_all(),
            Do::ApproveWaiting => self.approve_first_decision(),
            Do::Shortcuts => self.shortcuts_open = true,
            Do::ToggleSidebar => self.toggle_sidebar(),
            Do::Mode(mode) => self.set_appearance(mode),
            Do::Theme(mode) => self.set_appearance(mode),
            Do::Plugin(page) => self.open_plugin_page(page),
        }
    }

    /// The right-click "Search for …": the palette with the words already typed (at most 200 characters).
    pub fn palette_search(&mut self, words: &str) {
        self.palette_toggle();
        if self.palette.is_some() {
            self.palette_query.replace_text(words.chars().take(200).collect::<String>());
        }
    }

    /// ⌘B (and the palette's row): the sessions column away and back.
    pub fn toggle_sidebar(&mut self) {
        let open = self.layout.sessions.is_open();
        self.layout.sessions.set_open(!open);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Summary, TaskRow, Wait};
    use TaskStatus::*;

    fn row(id: &str, title: &str, status: TaskStatus, at: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: title.into(), status, updated_at_ms: at, goal: String::new(), wait: Wait::None, progress: None }
    }

    fn app() -> App {
        let mut app = App::headless();
        let tasks: Vec<TaskRow> = (0..9).map(|i| row(&format!("t{i}"), &format!("Task number {i} about {}", ["login", "billing", "search"][i % 3]), if i == 8 { Failed } else { Building }, i as i64)).collect();
        app.summary = Some(Summary { tasks, ..Summary::default() });
        app
    }

    fn sections(items: &[Item]) -> Vec<Section> {
        let mut out: Vec<Section> = Vec::new();
        for i in items {
            if out.last() != Some(&i.section) {
                out.push(i.section);
            }
        }
        out
    }

    #[test]
    fn the_sections_come_in_the_webs_order_and_sessions_are_latest_first() {
        let mut app = app();
        app.plugin_pages = Some(vec![neko_protocol::PluginPage { plugin: "p".into(), id: "p:board".into(), title: "Board".into() }]);
        app.select("t8");
        let items = app.palette_items();
        assert_eq!(sections(&items), Section::ALL.to_vec());
        let sessions: Vec<&str> = items.iter().filter(|i| i.section == Section::Sessions).map(|i| i.title.as_str()).collect();
        assert_eq!(sessions.len(), RECENT);
        assert!(sessions[0].starts_with("Task number 8"), "the newest first: {sessions:?}");
        let go: Vec<&str> = items.iter().filter(|i| i.section == Section::GoTo).map(|i| i.title.as_str()).collect();
        for want in ["Go to Home", "Go to Routines", "Go to Calendar", "Go to Settings", "Go to Usage", "Go to Setup", "Go to Plugins", "Go to All sessions"] {
            assert!(go.contains(&want), "{want} in {go:?}");
        }
        assert!(items.iter().any(|i| i.title == "Start selected session again"), "the selected session is failed: it can be started again");
        assert!(items.iter().any(|i| i.title == "Open Board" && i.section == Section::Plugins));
    }

    #[test]
    fn words_narrow_every_section_and_a_typo_still_finds_it() {
        let mut app = app();
        app.palette_toggle();
        app.palette_query.replace_text("biling");
        let items = app.palette_items();
        assert!(!items.is_empty() && items.iter().filter(|i| i.section == Section::Sessions).all(|i| i.title.contains("billing")), "{:?}", items.iter().map(|i| &i.title).collect::<Vec<_>>());
        app.palette_query.replace_text("calendr");
        assert!(app.palette_items().iter().any(|i| i.title == "Go to Calendar"));
        app.palette_query.replace_text("zzzzqq");
        assert!(app.palette_items().is_empty());
    }

    #[test]
    fn running_a_row_does_what_it_says_and_closes_the_palette() {
        let mut app = app();
        app.palette_toggle();
        app.palette_run(Do::GoTo(FullView::Calendar));
        assert!(app.palette.is_none());
        assert_eq!(app.view, Some(FullView::Calendar));
        app.palette_toggle();
        app.palette_run(Do::OpenSession("t3".into()));
        assert_eq!(app.selected.as_deref(), Some("t3"));
        app.palette_toggle();
        app.palette_run(Do::Mode("light"));
        assert_eq!(app.pages.appearance.as_deref(), Some("light"));
        assert!(app.outbox.iter().any(|c| matches!(c, crate::link::Call::SetAppearance { mode } if mode == "light")));
        app.palette_run(Do::ToggleSidebar);
        assert!(!app.layout.sessions.is_open());
        app.palette_run(Do::ToggleSidebar);
        assert!(app.layout.sessions.is_open());
    }

    #[test]
    fn search_for_opens_the_palette_with_the_words() {
        let mut app = app();
        app.palette_search("billing");
        assert!(app.palette.is_some());
        assert_eq!(app.palette_query.text, "billing");
    }
}
