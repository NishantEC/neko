//! Settings (⌘,): a list of pages down the left with a search field at its top, and one page beside it, as the Mac's own System
//! Settings has them (docs/knowledge/app-shell.md, "Settings"). Each page is a file of its own here. The search narrows the list
//! to the pages that have what is typed, says under each page what matched, and marks the matching options on the page.

mod agents;
mod cattains;
mod general;
mod kittens;
mod notifications;
mod permissions;
mod projects;
mod rounds;
mod shortcuts;
mod storage;

use std::collections::HashSet;

use rune::{El, Icon, Shadow, col, el, text};
use rune_ui_style::weight;

use crate::state::App;
use crate::ui::base::{Shape, Size, Theme};
use crate::ui::input::Input;
use crate::ui::sidebar::SidebarItem;

pub use notifications::{SYSTEM_NOTIFICATIONS, permission_words};
pub use permissions::FULL_ACCESS_WARNING;

/// The page list's width, without its inset.
pub const NAV: f32 = 200.0;
/// The widest a page's column grows.
pub const PAGE: f32 = 640.0;

/// One of Settings' pages, in the order the list shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    #[default]
    General,
    Agents,
    Permissions,
    Projects,
    Cattains,
    Kittens,
    Rounds,
    Notifications,
    Plugins,
    Shortcuts,
    Storage,
}

impl Page {
    pub const ALL: [Page; 11] = [
        Page::General,
        Page::Agents,
        Page::Permissions,
        Page::Projects,
        Page::Cattains,
        Page::Kittens,
        Page::Rounds,
        Page::Notifications,
        Page::Plugins,
        Page::Shortcuts,
        Page::Storage,
    ];

    /// The word in its key (`settings-page-<slug>`) and in Search's action (`settings-<slug>`).
    pub fn slug(self) -> &'static str {
        match self {
            Page::General => "general",
            Page::Agents => "agents",
            Page::Permissions => "permissions",
            Page::Projects => "projects",
            Page::Cattains => "cattains",
            Page::Kittens => "kittens",
            Page::Rounds => "rounds",
            Page::Notifications => "notifications",
            Page::Plugins => "plugins",
            Page::Shortcuts => "shortcuts",
            Page::Storage => "storage",
        }
    }

    pub fn of_slug(slug: &str) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.slug() == slug)
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Agents => "Agents & models",
            Page::Permissions => "Permissions",
            Page::Projects => "Projects",
            Page::Cattains => "Cattains",
            Page::Kittens => "Kittens",
            Page::Rounds => "Rounds & wakes",
            Page::Notifications => "Notifications",
            Page::Plugins => "Plugins",
            Page::Shortcuts => "Shortcuts",
            Page::Storage => "Storage",
        }
    }

    /// Its row's key in the list.
    pub fn key(self) -> String {
        format!("settings-page-{}", self.slug())
    }

    fn icon(self) -> &'static Icon {
        match self {
            Page::General => &crate::ui::icons::GEAR,
            Page::Agents => &crate::ui::icons::SPARKLE,
            Page::Permissions => &crate::ui::icons::SHIELD,
            Page::Projects => &crate::ui::icons::FOLDER,
            Page::Cattains => &crate::icons::CAT,
            Page::Kittens => &crate::icons::PAW,
            Page::Rounds => &crate::icons::ROUNDS,
            Page::Notifications => &crate::ui::icons::BELL,
            Page::Plugins => &crate::ui::icons::PLUG,
            Page::Shortcuts => &crate::ui::icons::KEY_COMMAND,
            Page::Storage => &crate::icons::DRIVE,
        }
    }

    /// Words that find the page itself.
    fn keywords(self) -> &'static str {
        match self {
            Page::General => "preferences sidebar threads",
            Page::Agents => "claude code cli runtime model",
            Page::Permissions => "access ask allow safety rules",
            Page::Projects => "workspaces folders repositories",
            Page::Cattains => "cats crew rules faces",
            Page::Kittens => "subagents helpers",
            Page::Rounds => "automatic schedule wake timers briefing morning",
            Page::Notifications => "alerts notify badge",
            Page::Plugins => "mcp servers tools extensions",
            Page::Shortcuts => "keyboard keys hotkeys",
            Page::Storage => "undo checkpoints disk space history worktrees archived",
        }
    }

    /// The pages after which the list leaves a group's gap: what Neko does, the work, Neko itself.
    fn ends_group(self) -> bool {
        matches!(self, Page::Permissions | Page::Rounds)
    }
}

/// One option a search can find: the page it is on, the key of what the page marks, the words shown under the page's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub page: Page,
    pub key: String,
    pub label: String,
    pub keywords: String,
}

/// The options on every page, with the words that find them. The names of cats, projects, plugins and agents are added from
/// what Neko knows ([`entries`]).
const OPTIONS: &[(Page, &str, &str, &str)] = &[
    (Page::General, "settings-sending", "While the agent works", "steer queue return option send follow-up"),
    (Page::General, "settings-grouping", "Group threads by", "project none list view sidebar"),
    (Page::General, "settings-show-settled", "Show settled threads", "settle put away view sidebar"),
    (Page::General, "settings-show-archived", "Show archived threads", "archive sleeping restore view sidebar"),
    (Page::General, "settings-auto-settle", "Auto-settle", "settle seven days week idle put away"),
    (Page::Agents, "settings-agents", "Agent CLIs", "installed signed in version login terminal api key"),
    (Page::Agents, "settings-runtime", "New threads use", "default agent runtime"),
    (Page::Agents, "settings-model", "Model", "model effort default"),
    (Page::Permissions, "settings-permissions", "How much the agent may do without asking", "follow claude code rules ask before risky steps full access mode"),
    (Page::Permissions, "settings-kitten-access", "Kittens' access", "subagents same access never more"),
    (Page::Projects, "settings-add-workspace", "Add project…", "folder add new"),
    (Page::Projects, "settings-branches", "Worktree branches", "neko/ branch worktree prefix git"),
    (Page::Cattains, "settings-new-cat", "New Cattain…", "make create cat"),
    (Page::Cattains, "settings-agents-make-cattains", "Agents may make Cattains", "agent make automatically ask"),
    (Page::Cattains, "settings-cats-about", "At most 16 active", "limit retired bring back"),
    (Page::Kittens, "settings-ask-first-kitten", "Ask before a thread's first kitten", "draft start subagent"),
    (Page::Kittens, "settings-kitten-limits", "How many work at once", "limits depth parallel warm subagents"),
    (Page::Rounds, "settings-rounds-pause", "Pause automatic work", "pause resume stop automatic rounds timers watches"),
    (Page::Rounds, "settings-waits", "Timers and watches", "timer watch wake later limits files check"),
    (Page::Rounds, "settings-briefing", "Morning Briefing", "briefing morning time notification daily summary"),
    (Page::Notifications, "settings-notify-permission", "Permission in System Settings", "system allow"),
    (Page::Notifications, "notify-needs", "When a thread needs you", "request question"),
    (Page::Notifications, "notify-finished", "When a thread finishes", "done"),
    (Page::Notifications, "notify-badge", "Show the count on the Dock icon", "badge dock"),
    (Page::Notifications, "settings-notify-test", "Send a test notification", "test"),
    (Page::Plugins, "own-mcp", "My own Claude Code MCP servers", "mcp own servers"),
    (Page::Plugins, "plugins-add", "Add MCP server…", "add paste json folder"),
    (Page::Storage, "settings-undo", "Undo history", "undo checkpoints clear storage disk"),
    (Page::Storage, "settings-worktrees", "Worktrees", "worktree tidy kept branch copies"),
    (Page::Storage, "settings-archived", "Archived threads", "archive sleeping restore"),
];

/// Everything a search can find now.
pub fn entries(s: &App) -> Vec<Entry> {
    let mut v: Vec<Entry> = OPTIONS.iter().map(|(page, key, label, keywords)| Entry { page: *page, key: (*key).to_owned(), label: (*label).to_owned(), keywords: (*keywords).to_owned() }).collect();
    let mut add = |page: Page, key: String, label: &str, keywords: &str| v.push(Entry { page, key, label: label.to_owned(), keywords: keywords.to_owned() });
    if let Some((list, _)) = &s.runtimes {
        for r in list {
            add(Page::Agents, format!("agent-{}", r.id), &r.name, "agent cli");
        }
    }
    for w in s.workspaces.iter().filter(|w| w.is_active()) {
        add(Page::Projects, format!("settings-ws-{}", w.id), &w.name, "project folder");
    }
    for c in &s.cats {
        add(Page::Cattains, format!("settings-cat-{}", c.id), &c.name, c.purpose.as_deref().unwrap_or(""));
    }
    for p in &s.plugins {
        add(Page::Plugins, format!("plugin-{}", p.id), &p.name, "plugin");
    }
    for (i, sc) in shortcuts::rows(s).iter().enumerate() {
        add(Page::Shortcuts, format!("shortcut-{i}"), &sc.what, &format!("{} {}", sc.keys.join(" "), sc.keywords));
    }
    v
}

/// What a search found on one page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub page: Page,
    /// 3: the page's own name or words; 2: an option's name; 1: only an option's other words.
    pub score: u8,
    /// The names of the options found there, for the row's second line.
    pub labels: Vec<String>,
}

/// The search as it stands: the pages it keeps, and the options it marks.
#[derive(Clone, Debug, Default)]
pub struct Found {
    /// The words typed, trimmed (empty: no search).
    pub query: String,
    pub hits: Vec<Hit>,
    lit: HashSet<String>,
}

fn has_all(hay: &str, words: &[String]) -> bool {
    words.iter().all(|w| hay.contains(w.as_str()))
}

fn has_any(hay: &str, words: &[String]) -> bool {
    words.iter().any(|w| hay.contains(w.as_str()))
}

/// What `query` finds among `entries`: every word typed must be in the page's name and words, or in an option's (with its page's).
pub fn find_in(query: &str, entries: &[Entry]) -> Found {
    let query = query.trim().to_owned();
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Found::default();
    }
    let mut hits = Vec::new();
    let mut lit = HashSet::new();
    for page in Page::ALL {
        let own = format!("{} {}", page.title(), page.keywords()).to_lowercase();
        let mut score = if has_all(&own, &words) { 3 } else { 0 };
        let mut labels = Vec::new();
        for e in entries.iter().filter(|e| e.page == page) {
            let name = e.label.to_lowercase();
            let theirs = format!("{name} {}", e.keywords.to_lowercase());
            if !has_all(&format!("{own} {theirs}"), &words) || !has_any(&theirs, &words) {
                continue;
            }
            score = score.max(if has_any(&name, &words) { 2 } else { 1 });
            if !labels.contains(&e.label) {
                labels.push(e.label.clone());
            }
            lit.insert(e.key.clone());
        }
        if score > 0 {
            hits.push(Hit { page, score, labels });
        }
    }
    Found { query, hits, lit }
}

/// What the search field finds now.
pub fn find(s: &App) -> Found {
    find_in(&s.settings_query.text, &entries(s))
}

impl Found {
    pub fn searching(&self) -> bool {
        !self.query.is_empty()
    }

    /// Whether the list shows `page`.
    pub fn shows(&self, page: Page) -> bool {
        !self.searching() || self.hits.iter().any(|h| h.page == page)
    }

    /// The page that fits best: the highest score, the first in the list on a tie.
    pub fn best(&self) -> Option<Page> {
        let top = self.hits.iter().map(|h| h.score).max()?;
        self.hits.iter().find(|h| h.score == top).map(|h| h.page)
    }

    pub fn is_lit(&self, key: &str) -> bool {
        self.lit.contains(key)
    }

    /// `e` marked as found, when the search found the option keyed `key`: the identity colour's quiet tint round it, a few
    /// points past its edges, so nothing on the page moves. As wide as the page.
    pub fn lit(&self, t: &Theme, key: &str, e: El) -> El {
        if !self.is_lit(key) {
            return e;
        }
        Self::tint(t, key, e).w_full()
    }

    /// The same mark round a control as wide as itself (a button in a row).
    pub fn lit_hug(&self, t: &Theme, key: &str, e: El) -> El {
        if !self.is_lit(key) {
            return e;
        }
        Self::tint(t, key, e)
    }

    fn tint(t: &Theme, key: &str, e: El) -> El {
        let tint = t.identity().bg;
        col().key(format!("{key}-found")).radius(t.radius.md).round_as(t).fill(tint).box_shadow(Shadow { spread: t.space.at(2.0), color: tint, ..Shadow::default() }).child(e)
    }
}

impl App {
    /// Shows a page of Settings. Notifications reads the system's permission again.
    pub fn set_settings_page(&mut self, page: Page) {
        self.settings_page = page;
        if page == Page::Notifications {
            self.read_allowed_soon();
        }
    }

    /// Return in the search field: the page that fits best.
    pub fn settings_submit(&mut self) {
        if let Some(p) = find(self).best() {
            self.set_settings_page(p);
        }
    }

    /// Each frame while Settings is open: when the search leaves the page in view out of the list, the page that fits best takes
    /// its place, so what shows is always one the list has (as in System Settings). Says whether anything changed.
    pub fn settings_follow_search(&mut self) -> bool {
        if !self.settings_open || self.settings_query.text.trim().is_empty() {
            return false;
        }
        let found = find(self);
        match found.best() {
            Some(best) if !found.shows(self.settings_page) => {
                self.set_settings_page(best);
                true
            }
            _ => false,
        }
    }
}

/// A page's section: its heading (level 3) and, when given, a line on what it is for.
pub(crate) fn section(t: &Theme, key: &str, title: &str, about: Option<&str>) -> El {
    let mut c = col().key(key.to_owned()).w_full().gap(t.space.at(3.0)).child(h3(t, title));
    if let Some(a) = about {
        c = c.child(text(a.to_owned()).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    }
    c
}

/// A page's first part when it needs no heading of its own (the page's name says it): the line on what it is for.
pub(crate) fn lead(t: &Theme, key: &str, about: &str) -> El {
    col().key(key.to_owned()).w_full().gap(t.space.at(3.0)).child(text(about.to_owned()).key(format!("{key}-about")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0))
}

pub(crate) fn h3(t: &Theme, words: &str) -> El {
    text(words.to_owned()).role("heading").level(3).font(weight(t.ty.md, t.ty.semibold)).color(t.ink())
}

pub(crate) fn h4(t: &Theme, words: &str) -> El {
    text(words.to_owned()).role("heading").level(4).font(weight(t.ty.base, t.ty.medium)).color(t.ink())
}

/// A quiet line of fact under a control: the small step in the faint ink.
pub(crate) fn note(t: &Theme, key: &str, words: impl Into<String>) -> El {
    text(words.into()).key(key.to_owned()).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0)
}

/// The page list: the search field at its top, then a row per page (one list for the keys), or, while searching, the pages that
/// have what is typed, each saying what matched.
fn nav(s: &App, t: &Theme, found: &Found) -> El {
    let search = Input::search("settings-search", &s.settings_query, |s: &mut App| &mut s.settings_query).name("Search settings").placeholder("Search").size(Size::Sm).on_submit(|s: &mut App| s.settings_submit()).build(t);
    // Escape empties the field first, as a Mac search field does; empty, it goes on to the window.
    let search = search.on_key_preview(|s: &mut App, k: &rune::KeyEvent, _: &mut rune::Tx| {
        let plain = !(k.mods.cmd || k.mods.shift || k.mods.alt || k.mods.ctrl);
        if k.key == rune::Key::Escape && plain && !s.settings_query.text.is_empty() {
            s.settings_query = rune::TextEdit::default();
            s.settings_query.focused = true;
            return true;
        }
        false
    });
    let row_gap = t.space.at(crate::sidebar::ROW_GAP_STEPS);
    let shown: Vec<Page> = Page::ALL.into_iter().filter(|p| found.shows(*p)).collect();
    let n = shown.len();
    let mut list = col().key("settings-page").role("listbox").label("Settings pages").w_full().gap(row_gap).focus_group();
    for (i, p) in shown.iter().copied().enumerate() {
        let mut item = SidebarItem::new(p.key(), p.title()).icon(p.icon()).selected(p == s.settings_page).list_row(i + 1, n).on_press(move |s: &mut App| s.set_settings_page(p));
        if let Some(h) = found.hits.iter().find(|h| h.page == p).filter(|h| !h.labels.is_empty()) {
            item = item.subtitle(h.labels.join(", "));
        }
        list = list.child(item.build(t));
        if !found.searching() && p.ends_group() {
            list = list.child(el().h(t.space.at(crate::sidebar::SECTION_GAP_STEPS) - row_gap));
        }
    }
    let mut body = col().w_full().gap(t.space.at(3.0)).child(search);
    if found.searching() && n == 0 {
        body = body.child(
            text(format!("No results for \u{201c}{}\u{201d}", found.query)).key("settings-no-results").role("status").font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0).px(t.space.at(2.0)),
        );
    } else {
        body = body.child(list);
    }
    let scroll = col().key("settings-nav-scroll").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(body.pb(t.space.at(4.0)));
    col().key("settings-nav").role("navigation").label("Settings pages").w(NAV + t.space.at(5.0)).h_full().pl(t.space.at(3.0)).pr(t.space.at(2.0)).pt(t.space.at(1.0)).child(scroll)
}

/// The page in view: its name, then its sections.
fn page(s: &App, t: &Theme, found: &Found, layer: &mut Vec<El>) -> El {
    let p = s.settings_page;
    let body = match p {
        Page::General => general::page(s, t, found),
        Page::Agents => agents::page(s, t, found),
        Page::Permissions => permissions::page(s, t, found),
        Page::Projects => projects::page(s, t, found),
        Page::Cattains => cattains::page(s, t, found),
        Page::Kittens => kittens::page(s, t, found),
        Page::Rounds => rounds::page(s, t, found),
        Page::Notifications => notifications::page(s, t, found),
        Page::Plugins => crate::plugins_view::page(s, t, found, layer),
        Page::Shortcuts => shortcuts::page(s, t, found),
        Page::Storage => storage::page(s, t, found),
    };
    let title = text(p.title()).key("settings-page-title").role("heading").level(2).font(weight(t.ty.xl, t.ty.semibold)).color(t.ink());
    // A page that another replaces is gone in that frame (`view::page_swap`), so two pages are never drawn one over the other.
    let column = crate::view::page_swap(col().key(format!("settings-{}-page", p.slug())).w_full().max_w(PAGE).gap(t.space.at(6.0)).child(title).child(body));
    col().key("settings-scroll").grow(1.0).min_w(0.0).h_full().px(t.space.at(6.0)).pt(t.space.at(1.0)).pb(t.space.at(10.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(column)
}

pub fn settings_view(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let found = find(s);
    rune::row().key("settings").w_full().grow(1.0).min_h(0.0).child(nav(s, t, &found)).child(page(s, t, &found, layer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(page: Page, key: &str, label: &str, keywords: &str) -> Entry {
        Entry { page, key: key.into(), label: label.into(), keywords: keywords.into() }
    }

    fn fixed() -> Vec<Entry> {
        OPTIONS.iter().map(|(p, k, l, w)| entry(*p, k, l, w)).collect()
    }

    #[test]
    fn every_page_has_a_slug_of_its_own_and_reads_it_back() {
        let slugs: HashSet<&str> = Page::ALL.iter().map(|p| p.slug()).collect();
        assert_eq!(slugs.len(), Page::ALL.len());
        for p in Page::ALL {
            assert_eq!(Page::of_slug(p.slug()), Some(p));
            assert_eq!(p.key(), format!("settings-page-{}", p.slug()));
        }
        assert_eq!(Page::of_slug("briefing"), None, "the Briefing's page went to Rounds & wakes");
    }

    #[test]
    fn no_search_shows_every_page_and_marks_nothing() {
        let f = find_in("   ", &fixed());
        assert!(!f.searching() && Page::ALL.iter().all(|p| f.shows(*p)) && f.best().is_none());
        assert!(!f.is_lit("notify-badge"));
    }

    #[test]
    fn undo_finds_storage_first_though_a_shortcut_also_says_undo() {
        let mut e = fixed();
        e.push(entry(Page::Shortcuts, "shortcut-9", "Take back what the newest toast offers", "⌘ Z undo"));
        let f = find_in("Undo", &e);
        assert!(f.shows(Page::Storage) && f.shows(Page::Shortcuts));
        assert!(!f.shows(Page::General) && !f.shows(Page::Projects));
        assert_eq!(f.best(), Some(Page::Storage));
        assert!(f.is_lit("settings-undo") && f.is_lit("shortcut-9"));
    }

    #[test]
    fn a_page_found_by_an_option_says_which_and_marks_it() {
        let f = find_in("badge", &fixed());
        let hit = f.hits.iter().find(|h| h.page == Page::Notifications).expect("Notifications");
        assert_eq!(hit.labels, ["Show the count on the Dock icon"]);
        assert!(f.is_lit("notify-badge") && !f.is_lit("notify-needs"));
    }

    #[test]
    fn every_word_must_be_found_and_a_page_name_narrows_its_options() {
        let f = find_in("notifications test", &fixed());
        assert_eq!(f.hits.iter().map(|h| h.page).collect::<Vec<_>>(), [Page::Notifications]);
        assert!(f.is_lit("settings-notify-test") && !f.is_lit("notify-badge"), "only the option that has the other word");
        assert!(find_in("zebra", &fixed()).hits.is_empty());
        assert!(find_in("badge zebra", &fixed()).hits.is_empty());
    }

    #[test]
    fn a_page_found_by_its_own_name_marks_none_of_its_options() {
        let f = find_in("plugins", &fixed());
        assert_eq!(f.best(), Some(Page::Plugins));
        assert!(f.hits.iter().find(|h| h.page == Page::Plugins).is_some_and(|h| h.labels.is_empty()));
        assert!(!f.is_lit("own-mcp"));
    }
}
