//! More, Plugins as full management (`web/src/pages/customisations/Customisations.tsx`, `plugins.tsx`): the list with
//! each plugin's status, a switch to enable or disable it, reload, remove (after a confirmation), a details panel (manifest,
//! version, viewers, pages, how it last loaded, the problem text) and "Change Neko…". Everything goes through the HTTP client
//! to the paths the web calls (`/api/plugins`, `/api/plugins/<id>/enable|disable`, the folder's deletion,
//! `/api/customisation`, `/plugins/<id>/manifest.json`), and the socket for the load status the web reports.

use crate::app::App;
use crate::compose::{HttpDone, HttpWork};
use crate::link::http::{Customisation, PluginListing};
use crate::link::Call;
use crate::task_actions::{Ask, AskField};

/// What "Change Neko" tells Neko (`web/src/pages/customisations/prompt.ts`, `CHANGE_NEKO_PROMPT`): one constant, filled in at send
/// time. A test reads the web's file and requires this to be the same text.
pub const CHANGE_NEKO_PROMPT: &str = include_str!("change_neko_prompt.txt");

const LIMIT_BYTES: usize = 4096;
const NO_CHECKOUT: &str = "(none is set: Settings, Neko updates, Development checkout)";

/// Where the person's customisations live, for the prompt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Places {
    pub plugins_dir: String,
    pub css_path: String,
    pub checkout: Option<String>,
}

fn fill(request: &str, places: &Places) -> String {
    CHANGE_NEKO_PROMPT
        .replace("{{PLUGINS_DIR}}", &places.plugins_dir)
        .replace("{{CSS_PATH}}", &places.css_path)
        .replace("{{CHECKOUT}}", places.checkout.as_deref().unwrap_or(NO_CHECKOUT))
        .replace("{{REQUEST}}", request)
}

pub fn change_prompt(request: &str, places: &Places) -> String {
    fill(request.trim(), places)
}

/// How many bytes the request may use once the rest of the prompt is in (`requestBudget`).
pub fn request_budget(places: &Places) -> usize {
    LIMIT_BYTES.saturating_sub(fill("", places).len())
}

/// The status line of a plugin (the web's `statusOf`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Off,
    Error,
}

pub fn status_of(listing: &PluginListing, problem: Option<&str>, pages: usize) -> (Tone, String) {
    if listing.disabled {
        (Tone::Off, "Off".to_owned())
    } else if !listing.error.is_empty() || problem.is_some() {
        (Tone::Error, "Error".to_owned())
    } else if pages > 0 {
        (Tone::Ok, format!("Running · {pages} {}", if pages == 1 { "page" } else { "pages" }))
    } else {
        (Tone::Ok, "Running".to_owned())
    }
}

#[derive(Default)]
pub struct PluginUi {
    pub listings: Option<Vec<PluginListing>>,
    pub custom: Option<Customisation>,
    /// The listing could not be read.
    pub failed: Option<String>,
    /// The plugin a switch or removal is in flight for.
    pub busy: Option<String>,
    pub detail: Option<String>,
    /// The manifest of the plugin in the details panel: its id and the text (or why it could not be read).
    pub manifest: Option<(String, String)>,
    pub reloading: bool,
    /// A refusal, in one calm line.
    pub note: Option<String>,
}

impl App {
    /// Plugins opens (or is read again): the listing, where things live, and how each plugin last loaded.
    pub fn plugins_ask(&mut self) {
        self.http_work(HttpWork::Plugins);
        self.call(Call::Plugins);
        self.call(Call::PluginViewers);
        self.call(Call::PluginPages);
    }

    pub fn plugin_set(&mut self, id: &str, enabled: bool) {
        if self.plugin_ui.busy.is_some() {
            return;
        }
        self.plugin_ui.busy = Some(id.to_owned());
        self.plugin_ui.note = None;
        self.http_work(HttpWork::PluginSet { id: id.to_owned(), enabled });
    }

    /// "Reload": the listing and the load status are read again (the web re-imports the module in its page; this client has none to import).
    pub fn plugin_reload(&mut self, id: Option<&str>) {
        self.plugin_ui.reloading = true;
        self.plugins_ask();
        if let Some(id) = id {
            self.plugin_details(id);
        }
    }

    pub fn plugin_details(&mut self, id: &str) {
        self.plugin_ui.detail = Some(id.to_owned());
        self.plugin_ui.manifest = None;
        self.http_work(HttpWork::PluginManifest { id: id.to_owned() });
    }

    pub fn plugin_details_close(&mut self) {
        self.plugin_ui.detail = None;
        self.plugin_ui.manifest = None;
    }

    pub fn plugin_places(&self) -> Option<Places> {
        let c = self.plugin_ui.custom.as_ref()?;
        let checkout = self.summary.as_ref().and_then(|m| m.settings.self_update.as_ref()).and_then(|u| u.dev_checkout.clone()).filter(|d| !d.is_empty());
        Some(Places { plugins_dir: c.plugins_dir.clone(), css_path: c.css_path.clone(), checkout })
    }

    /// "Change Neko…": a dialog asks what to change, then one message with the web's prompt goes to Neko's chat (as the web's dialog does).
    pub fn ask_change_neko(&mut self) {
        let Some(places) = self.plugin_places() else {
            self.plugins_ask();
            self.show_toast("Reading where your customisations live. Try again in a moment.");
            return;
        };
        self.pages.set_field(crate::task_actions::ASK_FIELD, "");
        self.confirm(Ask {
            title: "Change Neko".to_owned(),
            detail: "Describe the change. Neko opens a task, and writes a page of yours or changes its own source, whichever is lighter. You can read what it was told in Neko’s column.".to_owned(),
            ok: "Ask Neko".to_owned(),
            cancel: "Cancel".to_owned(),
            destructive: false,
            call: Call::ChangeNeko { plugins_dir: places.plugins_dir, css_path: places.css_path, checkout: places.checkout.unwrap_or_default(), wish: String::new() },
            done: "Asked Neko to change itself.".to_owned(),
            field: Some(AskField { label: "What should change?".to_owned(), placeholder: "A page that lists my open pull requests by stage…".to_owned() }),
            list: Vec::new(),
            more: Vec::new(),
            toggle: None,
        });
    }

    /// An answer over HTTP that belongs to Plugins or to a download. True when something on screen changed.
    pub fn plugin_http_done(&mut self, done: HttpDone) -> bool {
        match done {
            HttpDone::Plugins(result) => {
                self.plugin_ui.reloading = false;
                match result {
                    Ok((list, custom)) => {
                        self.plugin_ui.failed = None;
                        self.plugin_ui.listings = Some(list);
                        if custom.is_some() {
                            self.plugin_ui.custom = custom;
                        }
                    }
                    Err(e) => self.plugin_ui.failed = Some(e.0),
                }
                true
            }
            HttpDone::PluginChanged { id, removed, result } => {
                self.plugin_ui.busy = None;
                match result {
                    Ok(()) => {
                        self.plugin_ui.note = None;
                        // A pack's plugin is switched here too: its detail shows the new state.
                        if self.setup.packs.is_some() && self.view == Some(crate::views::FullView::Setup) {
                            self.packs_refresh();
                        }
                        if removed && self.plugin_ui.detail.as_deref() == Some(id.as_str()) {
                            self.plugin_details_close();
                        }
                        self.plugins_ask();
                    }
                    Err(e) => self.plugin_ui.note = Some(e.0),
                }
                true
            }
            HttpDone::Manifest { id, result } => {
                if self.plugin_ui.detail.as_deref() != Some(id.as_str()) {
                    return false;
                }
                self.plugin_ui.manifest = Some((id, result.unwrap_or_else(|e| e.0)));
                true
            }
            HttpDone::Downloaded { name, result } => {
                match result {
                    Ok(path) => {
                        let saved = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name);
                        self.show_toast(&format!("Saved {saved} to Downloads"));
                    }
                    Err(e) => self.show_toast(&e.0),
                }
                true
            }
            HttpDone::Uploaded { .. } | HttpDone::Files { .. } => false,
        }
    }

    /// The pages a plugin registered (`PluginPages`), for its row.
    pub fn plugin_pages_of(&self, id: &str) -> Vec<neko_protocol::PluginPage> {
        self.plugin_pages.as_ref().map(|p| p.iter().filter(|p| p.plugin == id).cloned().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::http::Http;

    fn listing(id: &str) -> PluginListing {
        PluginListing { id: id.into(), title: id.into(), ..PluginListing::default() }
    }

    #[test]
    fn the_change_prompt_is_the_webs_and_fits_the_daemons_limit() {
        let web = include_str!("../../../web/src/pages/customisations/prompt.ts");
        let start = web.find("export const CHANGE_NEKO_PROMPT = `").unwrap() + "export const CHANGE_NEKO_PROMPT = `".len();
        let end = start + web[start..].find("`;\n").unwrap();
        assert_eq!(CHANGE_NEKO_PROMPT, &web[start..end], "src/change_neko_prompt.txt must be the web's CHANGE_NEKO_PROMPT");
        let places = Places { plugins_dir: "/d/plugins".into(), css_path: "/d/custom.css".into(), checkout: None };
        let text = change_prompt("  a PR board ", &places);
        assert!(text.contains("What to change: a PR board\n") && text.contains("/d/plugins/<name>/") && text.contains("/d/custom.css") && text.contains(NO_CHECKOUT) && !text.contains("{{"));
        assert!(text.len() <= LIMIT_BYTES && request_budget(&places) > 100);
        let with = change_prompt("x", &Places { checkout: Some("/dev/neko".into()), ..places });
        assert!(with.contains("/dev/neko") && !with.contains(NO_CHECKOUT));
    }

    #[test]
    fn a_plugins_status_is_the_webs_words() {
        let mut p = listing("board");
        assert_eq!(status_of(&p, None, 0), (Tone::Ok, "Running".to_owned()));
        assert_eq!(status_of(&p, None, 1).1, "Running · 1 page");
        assert_eq!(status_of(&p, None, 3).1, "Running · 3 pages");
        assert_eq!(status_of(&p, Some("threw"), 3), (Tone::Error, "Error".to_owned()));
        p.error = "bad manifest".into();
        assert_eq!(status_of(&p, None, 0).0, Tone::Error);
        p.disabled = true;
        assert_eq!(status_of(&p, Some("x"), 2), (Tone::Off, "Off".to_owned()), "off wins over everything");
    }

    #[test]
    fn a_switch_asks_over_http_and_only_one_at_a_time() {
        let mut app = App::headless();
        app.http = Some(Http::new(1, &"0".repeat(64)));
        app.plugin_set("board", false);
        assert_eq!(app.plugin_ui.busy.as_deref(), Some("board"));
        assert!(app.compose.asked.contains(&HttpWork::PluginSet { id: "board".into(), enabled: false }));
        let before = app.compose.asked.len();
        app.plugin_set("other", true);
        assert_eq!(app.compose.asked.len(), before, "a second switch waits for the first");
        app.plugin_http_done(HttpDone::PluginChanged { id: "board".into(), removed: false, result: Err(crate::link::http::HttpError("That plugin is no longer installed.".into())) });
        assert!(app.plugin_ui.busy.is_none() && app.plugin_ui.note.as_deref() == Some("That plugin is no longer installed."));
    }

    #[test]
    fn details_read_the_manifest_and_ignore_a_late_answer_for_another_plugin() {
        let mut app = App::headless();
        app.http = Some(Http::new(1, &"0".repeat(64)));
        app.plugin_details("a");
        assert!(app.compose.asked.contains(&HttpWork::PluginManifest { id: "a".into() }));
        assert!(!app.plugin_http_done(HttpDone::Manifest { id: "b".into(), result: Ok("{}".into()) }));
        assert!(app.plugin_http_done(HttpDone::Manifest { id: "a".into(), result: Ok("{\n  \"title\": \"A\"\n}".into()) }));
        assert!(app.plugin_ui.manifest.as_ref().is_some_and(|(id, text)| id == "a" && text.contains("\"title\"")));
        app.plugin_details_close();
        assert!(app.plugin_ui.detail.is_none() && app.plugin_ui.manifest.is_none());
    }
}
