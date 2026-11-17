//! The web panel (P3b, docs/knowledge/rune-p3b-the-web-panel.md): Neko's own web UI, in a web view that is
//! a child of the Rune window, for what Rune does not draw itself: HTML artifacts, Excalidraw, any viewer a
//! JS plugin registers and the pages plugins add. The page is the web UI's own route in **embed mode**
//! (`#/artifact/<id>/<n>?embed=1`, `#/<plugin>:<page>?embed=1`), at an address the daemon signed
//! (`SignedWebUrl`): the web UI talks to the daemon by itself, so there is no bridge between the two
//! sides (no IPC in v1).
//!
//! What this file decides, and nothing of it needs a window to be tested:
//!
//! - **The navigation lock** ([`decide`]): only the signed daemon origin may load. Another http(s) address
//!   opens in the person's browser and is cancelled in the panel; anything else is refused. It opens the
//!   browser only when it follows a click or a keypress inside the view within the last second
//!   ([`external_decision`], fed by a trusted-event script over the view's IPC); a page that navigates on
//!   its own is blocked, and the block is logged once.
//! - **Downloads** ([`downloads_destination`]): a download link saves to `~/Downloads` under its own name
//!   (never a path, never over a file), and the window says "Saved to Downloads".
//! - **Where the view is** ([`fit`]): the slot's rect, cut to the region it sits in.
//! - **When it shows** ([`Overlays`], [`WebHost::visible`]): never while any Rune popover, menu, dialog,
//!   ⌘K palette or toast is open (a native view always draws above Rune's own surface).
//! - **How long it lives** ([`WebHost::tick`]): made on first need, gone when the panel closes, or after
//!   five minutes off screen.
//!
//! The native part sits behind [`Factory`] and [`Native`]: [`WryFactory`] is the real one (`wry`, a
//! WKWebView built with `build_as_child` on the window's raw handle), tests use a recording fake.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use rune::WindowHandle;

/// How long a view that is not on screen is kept before it is destroyed to free its memory.
pub const OFF_SCREEN_LIFETIME_S: f64 = 300.0;

pub type Rect = [f32; 4];

// ---- the navigation lock -----------------------------------------------------------------------

/// What to do with an address the web view is about to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    /// The signed daemon origin: load it.
    Allow,
    /// Another website: open it in the person's default browser, and cancel it here.
    Browser,
    /// Not a website (a file, a script, data, a blob): cancel it.
    Refuse,
}

/// The one decision the lock makes. `daemon_port` is the port of the signed address the view was made
/// with; the host is always `127.0.0.1`. An address with a user name or password, another port, another
/// host, another scheme, or one that does not parse is never `Allow`. Only the two inert `about:` pages a
/// frame starts from are allowed besides the origin.
pub fn decide(address: &str, daemon_port: u16) -> Nav {
    if matches!(address, "about:blank" | "about:srcdoc") {
        return Nav::Allow;
    }
    let Ok(url) = url::Url::parse(address) else { return Nav::Refuse };
    let origin = url.scheme() == "http" && url.host_str() == Some("127.0.0.1") && url.port_or_known_default() == Some(daemon_port) && url.username().is_empty() && url.password().is_none();
    if origin {
        Nav::Allow
    } else if matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|h| !h.is_empty()) {
        Nav::Browser
    } else {
        Nav::Refuse
    }
}

// ---- what may open the browser ------------------------------------------------------------------

/// A click or a keypress inside the view opens the browser for this long.
pub const GESTURE_WINDOW_MS: u64 = 1000;

/// What to do with an external address the page asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum External {
    /// The person clicked or pressed a key in the view a moment ago: open it in their browser.
    Open,
    /// Nothing the person did explains it (a script set `location`): cancel it.
    Block,
}

/// Opens the browser only when the last click or keypress inside the view was at most [`GESTURE_WINDOW_MS`]
/// ago (`since_gesture_ms`, None when there has been none).
pub fn external_decision(since_gesture_ms: Option<u64>) -> External {
    match since_gesture_ms {
        Some(ms) if ms <= GESTURE_WINDOW_MS => External::Open,
        _ => External::Block,
    }
}

/// The page's report of a trusted click or keypress: a script run in every frame of the view that tells the
/// host over IPC. `isTrusted` is false for an event a script made up, so a page cannot give itself the gesture.
pub const GESTURE_SCRIPT: &str = "(function(){if(window.__nekoGesture){return;}window.__nekoGesture=true;\
function g(e){if(e.isTrusted&&window.ipc){window.ipc.postMessage('gesture');}}\
['pointerdown','mousedown','click','keydown','touchstart'].forEach(function(t){window.addEventListener(t,g,true);});})();";

/// When the person last clicked or pressed a key in the view.
#[derive(Clone, Default)]
pub struct Gesture(Rc<Cell<Option<Instant>>>);

impl Gesture {
    fn touch(&self) {
        self.0.set(Some(Instant::now()));
    }

    fn since_ms(&self) -> Option<u64> {
        self.0.get().map(|at| at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
    }
}

/// Blocks are logged once: the same address twice in a row says nothing more.
#[derive(Clone, Default)]
pub struct BlockLog(Rc<RefCell<Option<String>>>);

impl BlockLog {
    /// True when `address` is not the one logged last (so the caller logs it).
    pub fn first_time(&self, address: &str) -> bool {
        let mut last = self.0.borrow_mut();
        if last.as_deref() == Some(address) {
            return false;
        }
        *last = Some(address.to_owned());
        true
    }
}

/// What the lock does with an external address now: opens it for a person who just clicked, else blocks it and
/// logs the block once. Returns whether the browser was opened.
fn external(address: &str, gesture: &Gesture, log: &BlockLog) -> bool {
    match external_decision(gesture.since_ms()) {
        External::Open => {
            open_in_browser(address);
            true
        }
        External::Block => {
            if log.first_time(address) {
                let host = url::Url::parse(address).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
                eprintln!("neko-rune: blocked the web panel opening {host} without a click or keypress");
            }
            false
        }
    }
}

// ---- downloads ---------------------------------------------------------------------------------

/// Where a download is saved: `dir` (the person's Downloads folder) and the file's own name from `suggested`,
/// the path the web view proposed. Only the final name is used (no folders, no `..`, no leading dot), and a
/// file already there is never replaced: `report.csv` becomes `report 2.csv`.
pub fn downloads_destination(suggested: &Path, dir: &Path) -> PathBuf {
    let raw = suggested.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let clean: String = raw.chars().filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':')).collect();
    let clean = clean.trim().trim_start_matches('.').trim().to_owned();
    let name = if clean.is_empty() { "download".to_owned() } else { clean };
    let first = dir.join(&name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    (2..10_000).map(|n| dir.join(format!("{stem} {n}{ext}"))).find(|p| !p.exists()).unwrap_or_else(|| dir.join(format!("{stem} {}{ext}", std::process::id())))
}

pub fn downloads_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).filter(|d| d.is_dir())
}

/// The port of a signed address, when it is one this client may use: `http://127.0.0.1:<port>/…`.
pub fn signed_port(address: &str) -> Option<u16> {
    let url = url::Url::parse(address).ok()?;
    (url.scheme() == "http" && url.host_str() == Some("127.0.0.1") && url.username().is_empty() && url.password().is_none()).then(|| url.port_or_known_default()).flatten()
}

/// Opens an http(s) address in the default browser. Never runs for anything else.
fn open_in_browser(address: &str) {
    if matches!(url::Url::parse(address).map(|u| u.scheme().to_owned()).as_deref(), Ok("http" | "https")) {
        let _ = std::process::Command::new("/usr/bin/open").arg(address).spawn();
    }
}

// ---- geometry ----------------------------------------------------------------------------------

/// The part of `a` inside `b`, or None when they do not meet.
pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    (x1 > x0 + 0.5 && y1 > y0 + 0.5).then_some([x0, y0, x1 - x0, y1 - y0])
}

/// Where the web view sits: the slot, cut to the region it is in (the view is a native view, which no
/// Rune scroll viewport clips). None when it is not on screen at all, or the slot is not known yet.
pub fn fit(slot: Option<Rect>, region: Option<Rect>) -> Option<Rect> {
    let slot = slot.filter(|s| s[2] > 1.0 && s[3] > 1.0)?;
    match region.filter(|r| r[2] > 0.0 && r[3] > 0.0) {
        Some(region) => intersect(slot, region),
        None => Some(slot),
    }
}

// ---- overlays ----------------------------------------------------------------------------------

/// How many Rune popovers, menus, selects, dialogs, the ⌘K palette and toasts are open right now (every
/// element of the window's overlay layer is one). A native web view always draws above Rune's surface, so
/// while this is above 0 the view is hidden and the panel shows its backing surface in the theme's colour.
#[derive(Default)]
pub struct Overlays(Cell<usize>);

impl Overlays {
    pub fn set(&self, open: usize) {
        self.0.set(open);
    }

    pub fn open(&self) -> usize {
        self.0.get()
    }

    pub fn hides(&self) -> bool {
        self.0.get() > 0
    }
}

// ---- what a web view is -----------------------------------------------------------------------

/// A native web view. Everything it is told happens on the UI thread.
pub trait Native {
    fn set_bounds(&mut self, r: Rect);
    fn set_visible(&mut self, visible: bool);
    fn load(&mut self, address: &str);
    fn eval(&mut self, js: &str);
}

/// Makes web views. `bounds` and `visible` are where it starts; `port` is the daemon's, for the lock.
pub trait Factory {
    fn create(&self, address: &str, port: u16, bounds: Rect, visible: bool) -> Result<Box<dyn Native>, String>;
}

// ---- the real one ------------------------------------------------------------------------------

thread_local! {
    static WINDOW: RefCell<Option<WindowHandle>> = const { RefCell::new(None) };
}

/// Keeps the window's handle (`App::on_window`), which a view is built on when it is first needed.
pub fn remember_window(handle: &WindowHandle) {
    WINDOW.with(|w| *w.borrow_mut() = Some(handle.clone()));
}

/// `wry`'s web view: transparent (the page paints only its content over the panel), no developer tools,
/// no persistent storage (the sign-in cookie is not left on disk), and the navigation lock.
#[derive(Default)]
pub struct WryFactory {
    /// What the view tells the window ("Saved to Downloads"): the host passes it on as a toast.
    notes: Rc<RefCell<Vec<String>>>,
    /// What the embedded page said through `window.ipc` (a quoted comment, a `data-neko-send`), only from the signed origin.
    said: Rc<RefCell<Vec<crate::artifact_actions::Said>>>,
}

struct WryNative(wry::WebView);

fn wry_rect(r: Rect) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(f64::from(r[0]), f64::from(r[1])).into(),
        size: wry::dpi::LogicalSize::new(f64::from(r[2].max(1.0)), f64::from(r[3].max(1.0))).into(),
    }
}

impl Native for WryNative {
    fn set_bounds(&mut self, r: Rect) {
        let _ = self.0.set_bounds(wry_rect(r));
    }

    fn set_visible(&mut self, visible: bool) {
        let _ = self.0.set_visible(visible);
    }

    fn load(&mut self, address: &str) {
        let _ = self.0.load_url(address);
    }

    fn eval(&mut self, js: &str) {
        let _ = self.0.evaluate_script(js);
    }
}

impl Factory for WryFactory {
    fn create(&self, address: &str, port: u16, bounds: Rect, visible: bool) -> Result<Box<dyn Native>, String> {
        let window = WINDOW.with(|w| w.borrow().clone()).ok_or("The window is not ready.")?;
        let gesture = Gesture::default();
        let log = BlockLog::default();
        let (nav_gesture, nav_log, window_gesture, window_log, ipc_gesture) = (gesture.clone(), log.clone(), gesture.clone(), log.clone(), gesture);
        let (started, finished) = (self.notes.clone(), self.notes.clone());
        let said = self.said.clone();
        let builder = wry::WebViewBuilder::new()
            .with_url(address)
            // A trusted click or keypress in any frame tells the host, so a link the person pressed may open their browser.
            .with_initialization_script(GESTURE_SCRIPT)
            .with_ipc_handler(move |request| {
                if request.body() == "gesture" && decide(&request.uri().to_string(), port) == Nav::Allow {
                    ipc_gesture.touch();
                } else if let Some(message) = crate::artifact_actions::parse(request.body()).filter(|_| crate::artifact_actions::origin_ok(&request.uri().to_string(), port)) {
                    said.borrow_mut().push(message);
                }
            })
            // Downloads go to ~/Downloads under their own name, and the window says so.
            .with_download_started_handler(move |_url, path| match downloads_dir() {
                Some(dir) => {
                    *path = downloads_destination(path, &dir);
                    true
                }
                None => {
                    started.borrow_mut().push("The download could not be saved: there is no Downloads folder.".to_owned());
                    false
                }
            })
            .with_download_completed_handler(move |_url, _path, ok| {
                finished.borrow_mut().push(if ok { "Saved to Downloads".to_owned() } else { "The download did not finish.".to_owned() });
            })
            .with_bounds(wry_rect(bounds))
            .with_visible(visible)
            .with_transparent(true)
            .with_incognito(true)
            .with_devtools(false)
            .with_hotkeys_zoom(false)
            // The lock: only the signed origin loads; another website goes to the browser, here it is cancelled.
            .with_navigation_handler(move |next| match decide(&next, port) {
                Nav::Allow => true,
                Nav::Browser => {
                    external(&next, &nav_gesture, &nav_log);
                    false
                }
                Nav::Refuse => false,
            })
            // `window.open` and `target=_blank` never open a second view; the browser opens for a click only.
            .with_new_window_req_handler(move |next, _| {
                if decide(&next, port) == Nav::Browser {
                    external(&next, &window_gesture, &window_log);
                }
                wry::NewWindowResponse::Deny
            });
        // A web view that cannot start (AppKit wants the main thread, the handle is not an AppKit view) is a
        // line in the panel, not a crash of the app: wry panics on an invalid handle.
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| window.with_raw_handle(|handle| builder.build_as_child(&handle))));
        let view = built.map_err(|_| "The web view could not start in this window.".to_owned())?.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        Ok(Box::new(WryNative(view)))
    }
}

// ---- the host ----------------------------------------------------------------------------------

/// What the panel wants on screen this frame: the web route, and the appearance to paint it in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Want {
    /// The web app's hash route, as the daemon is asked to sign it: `artifact/<id>/<n>?embed=1`.
    pub route: String,
    pub dark: bool,
}

/// A signed address: the web token is in it, so it is never printed. `dark` is the appearance it was asked
/// for (the page starts in it).
#[derive(Clone)]
struct Signed(String, bool);

impl std::fmt::Debug for Signed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signed(<hidden>)")
    }
}

/// The route a request was made for (`artifact/a/1?embed=1&theme=dark` is for `artifact/a/1?embed=1`) and the
/// appearance it carries.
fn split_requested(requested: &str) -> (&str, bool) {
    match requested.rsplit_once("&theme=") {
        Some((route, theme)) => (route, theme == "dark"),
        None => (requested, true),
    }
}

struct Live {
    native: Box<dyn Native>,
    port: u16,
    route: String,
    dark: bool,
    bounds: Option<Rect>,
    visible: bool,
}

/// The one web view of the window and the rules for when it exists and shows.
pub struct WebHost {
    factory: Option<Rc<dyn Factory>>,
    live: RefCell<Option<Live>>,
    want: RefCell<Option<Want>>,
    slot: Cell<Option<Rect>>,
    region: Cell<Option<Rect>>,
    pub overlays: Overlays,
    signed: RefCell<HashMap<String, Signed>>,
    asked: RefCell<HashSet<String>>,
    off_since: Cell<Option<f64>>,
    /// Views made so far (a measurement and a test read this).
    made: Cell<usize>,
    last_error: RefCell<Option<String>>,
    /// What the view told the window to say (shared with the factory).
    notes: Rc<RefCell<Vec<String>>>,
    said: Rc<RefCell<Vec<crate::artifact_actions::Said>>>,
}

impl Default for WebHost {
    fn default() -> WebHost {
        WebHost::new(None)
    }
}

impl WebHost {
    /// `factory` None is a run with no window (headless, tests): the host decides everything and makes nothing.
    pub fn new(factory: Option<Rc<dyn Factory>>) -> WebHost {
        WebHost {
            factory,
            live: RefCell::new(None),
            want: RefCell::new(None),
            slot: Cell::new(None),
            region: Cell::new(None),
            overlays: Overlays::default(),
            signed: RefCell::new(HashMap::new()),
            asked: RefCell::new(HashSet::new()),
            off_since: Cell::new(None),
            made: Cell::new(0),
            last_error: RefCell::new(None),
            notes: Rc::new(RefCell::new(Vec::new())),
            said: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// The real one, for the window.
    pub fn windowed() -> WebHost {
        let factory = WryFactory::default();
        let notes = factory.notes.clone();
        let said = factory.said.clone();
        WebHost { notes, said, ..WebHost::new(Some(Rc::new(factory))) }
    }

    /// What the view has said since the last call ("Saved to Downloads"), for the window to show.
    pub fn drain_notes(&self) -> Vec<String> {
        std::mem::take(&mut *self.notes.borrow_mut())
    }

    /// What the embedded page said since the last call, for the window to act on.
    pub fn drain_said(&self) -> Vec<crate::artifact_actions::Said> {
        std::mem::take(&mut *self.said.borrow_mut())
    }

    /// Hands the host a message as the page's IPC would (a test; the real handler checks the origin first).
    pub fn push_said(&self, said: crate::artifact_actions::Said) {
        self.said.borrow_mut().push(said);
    }

    /// What the panel asks for in this build of the view (`None`: no web viewer is on screen).
    pub fn set_want(&self, want: Option<Want>) {
        *self.want.borrow_mut() = want;
    }

    pub fn want(&self) -> Option<Want> {
        self.want.borrow().clone()
    }

    /// The rect of the element the view sits in (the slot), and of the region that clips it.
    pub fn set_slot(&self, rect: Rect) {
        self.slot.set(Some(rect));
        self.sync();
    }

    pub fn set_region(&self, rect: Rect) {
        self.region.set(Some(rect));
        self.sync();
    }

    /// The route to ask the daemon to sign, once per route, when the panel wants one that has no address yet.
    pub fn needs_signing(&self) -> Option<String> {
        let want = self.want()?;
        (!self.signed.borrow().contains_key(&want.route) && self.asked.borrow_mut().insert(want.route.clone())).then(|| self.signed_route(&want))
    }

    /// The route as the daemon is asked to sign it: with the appearance the view starts in.
    pub fn signed_route(&self, want: &Want) -> String {
        format!("{}&theme={}", want.route, if want.dark { "dark" } else { "light" })
    }

    /// The daemon signed the route that was asked (as [`WebHost::signed_route`] spelled it).
    pub fn signed(&self, requested: &str, address: String) {
        let (route, dark) = split_requested(requested);
        self.signed.borrow_mut().insert(route.to_owned(), Signed(address, dark));
    }

    /// The daemon could not: ask again the next time it is wanted.
    pub fn signing_failed(&self, requested: &str, message: String) {
        self.asked.borrow_mut().remove(split_requested(requested).0);
        *self.last_error.borrow_mut() = Some(message);
    }

    pub fn error(&self) -> Option<String> {
        self.last_error.borrow().clone()
    }

    /// Whether a view exists now.
    pub fn exists(&self) -> bool {
        self.live.borrow().is_some()
    }

    /// How many views were made over the run.
    pub fn made(&self) -> usize {
        self.made.get()
    }

    /// Whether the view is on screen right now.
    pub fn shown(&self) -> bool {
        self.live.borrow().as_ref().is_some_and(|l| l.visible)
    }

    /// The rule for showing: something wants it, no overlay is open, and it has a place on screen.
    pub fn visible(&self) -> bool {
        self.want.borrow().is_some() && !self.overlays.hides() && fit(self.slot.get(), self.region.get()).is_some()
    }

    /// Puts the native view where the slot is and shows or hides it by the rules. Runs from the layout
    /// pass, so a menu that opens takes the view away in the same frame it appears in.
    pub fn sync(&self) {
        let mut live = self.live.borrow_mut();
        let Some(live) = live.as_mut() else { return };
        let place = fit(self.slot.get(), self.region.get());
        if let Some(r) = place
            && live.bounds != Some(r)
        {
            live.native.set_bounds(r);
            live.bounds = Some(r);
        }
        let show = self.want.borrow().is_some() && !self.overlays.hides() && place.is_some();
        if live.visible != show {
            live.native.set_visible(show);
            live.visible = show;
        }
    }

    /// Once a frame, with the clock `t` in seconds: makes the view when it is first needed and its address
    /// is known, moves it to a new route, follows the appearance, and destroys it when the panel is closed
    /// (`panel_open` false) or it has been off screen for [`OFF_SCREEN_LIFETIME_S`].
    pub fn tick(&self, t: f64, panel_open: bool) {
        match self.want() {
            Some(want) => {
                self.off_since.set(None);
                let address = self.signed.borrow().get(&want.route).cloned();
                let Some(Signed(address, baked_dark)) = address else { return };
                let Some(port) = signed_port(&address) else {
                    *self.last_error.borrow_mut() = Some("The daemon gave an address this client will not load.".to_owned());
                    return;
                };
                let have = self.exists();
                if !have {
                    let Some(factory) = self.factory.clone() else { return };
                    let bounds = fit(self.slot.get(), self.region.get()).unwrap_or([0.0, 0.0, 1.0, 1.0]);
                    // Made hidden; `sync` shows it once the slot is placed and no overlay is open.
                    match factory.create(&address, port, bounds, false) {
                        Ok(native) => {
                            self.made.set(self.made.get() + 1);
                            *self.live.borrow_mut() = Some(Live { native, port, route: want.route.clone(), dark: baked_dark, bounds: Some(bounds), visible: false });
                        }
                        Err(message) => *self.last_error.borrow_mut() = Some(message),
                    }
                } else if let Some(live) = self.live.borrow_mut().as_mut() {
                    if live.route != want.route {
                        // Same origin by construction (the daemon signed it), checked again by the lock.
                        if signed_port(&address) == Some(live.port) {
                            live.native.load(&address);
                            live.route = want.route.clone();
                            live.dark = baked_dark;
                        }
                    }
                    // Either way the page follows the appearance on screen, without a reload.
                    if live.dark != want.dark {
                        live.native.eval(&theme_script(want.dark));
                        live.dark = want.dark;
                    }
                }
                self.sync();
            }
            None => {
                self.sync();
                if !self.exists() {
                    return;
                }
                if !panel_open {
                    self.destroy();
                    return;
                }
                let since = self.off_since.get().unwrap_or(t);
                self.off_since.set(Some(since));
                if t - since >= OFF_SCREEN_LIFETIME_S {
                    self.destroy();
                }
            }
        }
    }

    /// A view that does nothing, standing where a real one would be (the demo scenes, which have no window):
    /// the slot then reads as it does with a view up that is hidden, not as "Opening…".
    pub fn pretend_up(&self) {
        struct Inert;
        impl Native for Inert {
            fn set_bounds(&mut self, _: Rect) {}
            fn set_visible(&mut self, _: bool) {}
            fn load(&mut self, _: &str) {}
            fn eval(&mut self, _: &str) {}
        }
        *self.live.borrow_mut() = Some(Live { native: Box::new(Inert), port: 0, route: String::new(), dark: true, bounds: None, visible: false });
    }

    /// Lets go of the view (the native one is removed from the window).
    pub fn destroy(&self) {
        *self.live.borrow_mut() = None;
        self.off_since.set(None);
    }

    /// The panel is closing: the view goes at once and the addresses it was given are forgotten.
    pub fn close(&self) {
        self.destroy();
        self.signed.borrow_mut().clear();
        self.asked.borrow_mut().clear();
        self.set_want(None);
    }
}

/// Changes the page's theme without reloading it: the web app follows `theme` in its own address.
pub fn theme_script(dark: bool) -> String {
    let theme = if dark { "dark" } else { "light" };
    format!("(function(){{var h=location.hash.split('?');var q=new URLSearchParams(h[1]||'');q.set('theme','{theme}');location.hash=h[0]+'?'+q.toString();}})()")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: u16 = 4747;

    #[test]
    fn only_the_signed_daemon_origin_loads() {
        let ok = [
            "http://127.0.0.1:4747/?token=abc#/artifact/a1/2?embed=1",
            "http://127.0.0.1:4747/",
            "http://127.0.0.1:4747/artifact/a1/2",
            "HTTP://127.0.0.1:4747/x",
            "about:blank",
            "about:srcdoc",
        ];
        for url in ok {
            assert_eq!(decide(url, PORT), Nav::Allow, "{url}");
        }
        // Another website goes to the browser.
        for url in ["https://example.com/", "http://example.com:4747/", "http://localhost:4747/", "http://127.0.0.1:4748/", "http://127.0.0.2:4747/", "https://127.0.0.1:4747/", "http://[::1]:4747/", "http://0.0.0.0:4747/"] {
            assert_eq!(decide(url, PORT), Nav::Browser, "{url}");
        }
        // Tricks that make an address look like the origin are another website.
        for url in ["http://127.0.0.1:4747@evil.example/", "http://user@127.0.0.1:4747/", "http://127.0.0.1:4747:80@evil.example/", "http://127.0.0.1.evil.example:4747/", "http://evil.example/http://127.0.0.1:4747/"] {
            assert_ne!(decide(url, PORT), Nav::Allow, "{url}");
        }
        // Not websites at all.
        for url in ["file:///etc/hosts", "javascript:alert(1)", "data:text/html,<h1>x</h1>", "blob:http://127.0.0.1:4747/abc", "ftp://127.0.0.1:4747/", "neko://x", "", "not a url", "about:config", "view-source:http://127.0.0.1:4747/"] {
            assert_eq!(decide(url, PORT), Nav::Refuse, "{url:?}");
        }
        // The port is the daemon's own: a view made for another daemon port loads that one.
        assert_eq!(decide("http://127.0.0.1:5000/", 5000), Nav::Allow);
        assert_eq!(decide("http://127.0.0.1:4747/", 5000), Nav::Browser);
    }

    #[test]
    fn a_signed_address_is_only_taken_for_the_loopback_daemon() {
        assert_eq!(signed_port("http://127.0.0.1:4747/?token=x#/a"), Some(4747));
        assert_eq!(signed_port("http://127.0.0.1/"), Some(80));
        for bad in ["https://127.0.0.1:4747/", "http://example.com:4747/", "http://u:p@127.0.0.1:4747/", "file:///x", "garbage"] {
            assert_eq!(signed_port(bad), None, "{bad}");
        }
        assert_eq!(format!("{:?}", Signed("http://127.0.0.1:4747/?token=SECRET".into(), true)), "Signed(<hidden>)", "the token is never printed");
    }

    #[test]
    fn the_view_is_cut_to_its_region_and_gone_when_nothing_is_left() {
        let slot = [100.0, 50.0, 400.0, 600.0];
        assert_eq!(fit(Some(slot), None), Some(slot), "no region: the slot itself");
        assert_eq!(fit(Some(slot), Some([0.0, 0.0, 1000.0, 1000.0])), Some(slot));
        // Scrolled partly out of the top of its region: the visible part only.
        assert_eq!(fit(Some([100.0, -100.0, 400.0, 600.0]), Some([0.0, 0.0, 1000.0, 1000.0])), Some([100.0, 0.0, 400.0, 500.0]));
        // Cut at the right and the bottom too.
        assert_eq!(fit(Some(slot), Some([150.0, 100.0, 200.0, 300.0])), Some([150.0, 100.0, 200.0, 300.0]));
        // Wholly outside: not on screen.
        assert_eq!(fit(Some(slot), Some([700.0, 0.0, 100.0, 100.0])), None);
        assert_eq!(fit(None, Some([0.0, 0.0, 10.0, 10.0])), None, "no slot yet");
        assert_eq!(fit(Some([0.0, 0.0, 0.0, 0.0]), None), None, "an empty slot");
    }

    #[derive(Default)]
    struct Log(Rc<RefCell<Vec<String>>>);

    struct Fake(Rc<RefCell<Vec<String>>>);

    impl Native for Fake {
        fn set_bounds(&mut self, r: Rect) {
            self.0.borrow_mut().push(format!("bounds {r:?}"));
        }

        fn set_visible(&mut self, visible: bool) {
            self.0.borrow_mut().push(format!("visible {visible}"));
        }

        fn load(&mut self, address: &str) {
            self.0.borrow_mut().push(format!("load {}", address.split('#').nth(1).unwrap_or("")));
        }

        fn eval(&mut self, js: &str) {
            self.0.borrow_mut().push(format!("eval {}", js.contains("'dark'")));
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.0.borrow_mut().push("destroyed".into());
        }
    }

    impl Factory for Log {
        fn create(&self, address: &str, port: u16, bounds: Rect, visible: bool) -> Result<Box<dyn Native>, String> {
            self.0.borrow_mut().push(format!("create {} port {port} {bounds:?} visible {visible}", address.split('#').nth(1).unwrap_or("")));
            Ok(Box::new(Fake(self.0.clone())))
        }
    }

    fn host_with_log() -> (WebHost, Rc<RefCell<Vec<String>>>) {
        host()
    }

    fn host() -> (WebHost, Rc<RefCell<Vec<String>>>) {
        let log = Log::default();
        let events = log.0.clone();
        (WebHost::new(Some(Rc::new(log))), events)
    }

    fn want(route: &str, dark: bool) -> Option<Want> {
        Some(Want { route: route.to_owned(), dark })
    }

    fn sign(host: &WebHost, w: &Want) {
        let route = host.signed_route(w);
        host.signed(&route, format!("http://127.0.0.1:4747/?token=t#/{route}"));
    }

    #[test]
    fn the_counter_hides_the_view_while_any_overlay_is_open() {
        let o = Overlays::default();
        assert!(!o.hides());
        o.set(1);
        assert!(o.hides() && o.open() == 1);
        o.set(3);
        assert!(o.hides());
        o.set(0);
        assert!(!o.hides());
    }

    #[test]
    fn the_view_is_made_when_first_needed_shows_by_the_rules_and_hides_under_an_overlay() {
        let (host, events) = host();
        // Nothing wants it: nothing is made, nothing is asked.
        host.tick(0.0, true);
        assert!(!host.exists() && host.needs_signing().is_none() && events.borrow().is_empty());
        // The panel wants an artifact: the daemon is asked once to sign it (with the theme it starts in).
        let w = want("artifact/a1/2?embed=1", true).unwrap();
        host.set_want(Some(w.clone()));
        assert_eq!(host.needs_signing().as_deref(), Some("artifact/a1/2?embed=1&theme=dark"));
        assert_eq!(host.needs_signing(), None, "once per route");
        host.tick(0.1, true);
        assert!(!host.exists(), "not made before the address is known");
        sign(&host, &w);
        host.set_slot([10.0, 10.0, 300.0, 400.0]);
        host.tick(0.2, true);
        assert!(host.exists() && host.made() == 1);
        assert_eq!(events.borrow()[0], "create /artifact/a1/2?embed=1&theme=dark port 4747 [10.0, 10.0, 300.0, 400.0] visible false", "made hidden, at the slot");
        // Placed and wanted: it shows.
        assert!(host.shown(), "{:?}", events.borrow());
        // A menu opens over it: hidden in the same call that learns of it (the layout pass), shown again when it closes.
        host.overlays.set(1);
        host.sync();
        assert!(!host.shown());
        host.overlays.set(0);
        host.sync();
        assert!(host.shown());
        // The slot moves (a resize): the view follows, in the same pass.
        host.set_slot([20.0, 10.0, 280.0, 400.0]);
        assert_eq!(events.borrow().last().unwrap(), "bounds [20.0, 10.0, 280.0, 400.0]");
        // Scrolled partly out of its region: cut, not stretched.
        host.set_region([0.0, 0.0, 1000.0, 200.0]);
        assert_eq!(events.borrow().last().unwrap(), "bounds [20.0, 10.0, 280.0, 190.0]");
        // Out of its region altogether: hidden.
        host.set_region([0.0, 500.0, 1000.0, 100.0]);
        assert!(!host.shown());
    }

    #[test]
    fn a_new_route_loads_in_the_same_view_and_a_new_theme_changes_it_without_a_reload() {
        let (host, events) = host();
        let a = want("artifact/a1/1?embed=1", true).unwrap();
        host.set_want(Some(a.clone()));
        sign(&host, &a);
        host.set_slot([0.0, 0.0, 300.0, 300.0]);
        host.tick(0.0, true);
        // Another artifact: the same web view loads it.
        let b = want("artifact/b2/1?embed=1", true).unwrap();
        host.set_want(Some(b.clone()));
        assert_eq!(host.needs_signing().as_deref(), Some("artifact/b2/1?embed=1&theme=dark"));
        sign(&host, &b);
        host.tick(0.1, true);
        assert_eq!(host.made(), 1, "one view, reused");
        assert!(events.borrow().contains(&"load /artifact/b2/1?embed=1&theme=dark".to_owned()));
        // The appearance changes: the page is told, not reloaded.
        host.set_want(Some(Want { dark: false, ..b }));
        host.tick(0.2, true);
        assert_eq!(events.borrow().last().unwrap(), "eval false");
        assert!(!events.borrow().iter().any(|e| e.starts_with("load") && e.contains("light")));
        let js = theme_script(true);
        assert!(js.contains("q.set('theme','dark')") && !js.contains("http"), "{js}");
    }

    #[test]
    fn the_view_is_destroyed_when_the_panel_closes_or_after_five_minutes_off_screen() {
        let (host, events) = host();
        let w = want("artifact/a1/1?embed=1", false).unwrap();
        host.set_want(Some(w.clone()));
        sign(&host, &w);
        host.set_slot([0.0, 0.0, 300.0, 300.0]);
        host.tick(0.0, true);
        assert!(host.exists());
        // The panel shows another tab: hidden at once, kept for a while.
        host.set_want(None);
        host.tick(10.0, true);
        assert!(host.exists() && !host.shown());
        host.tick(10.0 + OFF_SCREEN_LIFETIME_S - 1.0, true);
        assert!(host.exists(), "not yet five minutes");
        host.tick(10.0 + OFF_SCREEN_LIFETIME_S, true);
        assert!(!host.exists(), "five minutes off screen frees it");
        assert_eq!(events.borrow().last().unwrap(), "destroyed");
        // Coming back makes a new one (the address is still known).
        host.set_want(Some(w.clone()));
        host.tick(400.0, true);
        assert!(host.exists() && host.made() == 2);
        // Coming back before the time is up cancels the countdown.
        host.set_want(None);
        host.tick(500.0, true);
        host.set_want(Some(w));
        host.tick(700.0, true);
        host.set_want(None);
        host.tick(701.0, true);
        host.tick(701.0 + OFF_SCREEN_LIFETIME_S - 1.0, true);
        assert!(host.exists(), "the countdown started again at 701");
        // The panel closes: gone at once.
        host.tick(702.0, false);
        assert!(!host.exists());
        // And closing forgets what was signed, so nothing stale is reused.
        host.close();
        assert!(host.needs_signing().is_none());
    }

    #[test]
    fn a_daemon_address_this_client_will_not_load_makes_no_view() {
        let (host, events) = host();
        let w = want("artifact/a1/1?embed=1", true).unwrap();
        host.set_want(Some(w.clone()));
        host.set_slot([0.0, 0.0, 300.0, 300.0]);
        for bad in ["https://evil.example/", "http://u:p@127.0.0.1:4747/", "file:///etc/hosts", "http://localhost:4747/"] {
            host.signed(&host.signed_route(&w), bad.to_owned());
            host.tick(0.0, true);
            assert!(!host.exists(), "{bad}");
        }
        assert!(events.borrow().is_empty());
        assert!(host.error().is_some());
        // A failed signing is asked again (a host that was never given an address).
        let (fresh, _) = host_with_log();
        fresh.set_want(Some(w.clone()));
        let route = fresh.needs_signing().unwrap();
        assert_eq!(fresh.needs_signing(), None);
        fresh.signing_failed(&route, "no".into());
        assert_eq!(fresh.needs_signing().as_deref(), Some(route.as_str()));
    }

    // ---- the click rule and downloads (the P3b review's open items) -----------------------------

    #[test]
    fn the_browser_opens_only_within_a_second_of_a_click_or_keypress_in_the_view() {
        assert_eq!(GESTURE_WINDOW_MS, 1000);
        // No gesture at all (a page that set `location` by itself): blocked.
        assert_eq!(external_decision(None), External::Block);
        // After one: open while it is at most a second old.
        for ms in [0, 1, 250, 999, 1000] {
            assert_eq!(external_decision(Some(ms)), External::Open, "{ms} ms after the click");
        }
        for ms in [1001, 1500, 60_000, u64::MAX] {
            assert_eq!(external_decision(Some(ms)), External::Block, "{ms} ms after the click is too late");
        }
    }

    #[test]
    fn a_gesture_counts_from_when_it_happened_and_a_block_is_logged_once() {
        let g = Gesture::default();
        assert_eq!(g.since_ms(), None, "no gesture yet");
        g.touch();
        assert!(g.since_ms().is_some_and(|ms| ms < GESTURE_WINDOW_MS), "just now");
        let log = BlockLog::default();
        assert!(log.first_time("https://a.example/"), "the first block is logged");
        assert!(!log.first_time("https://a.example/"), "the same address again says nothing");
        assert!(log.first_time("https://b.example/"), "another address is a new line");
        assert!(log.first_time("https://a.example/"), "and back again is new once more");
    }

    #[test]
    fn the_view_hears_only_trusted_clicks_and_keys_and_says_nothing_else() {
        assert!(GESTURE_SCRIPT.contains("e.isTrusted"), "a script's own events do not count");
        for event in ["pointerdown", "click", "keydown"] {
            assert!(GESTURE_SCRIPT.contains(event), "{event}");
        }
        assert!(GESTURE_SCRIPT.contains("postMessage('gesture')") && GESTURE_SCRIPT.contains("true"), "capturing listeners");
        assert!(!GESTURE_SCRIPT.contains("fetch(") && !GESTURE_SCRIPT.contains("XMLHttpRequest"), "it only posts the one word");
    }

    #[test]
    fn a_download_saves_to_downloads_under_its_own_name_and_never_over_a_file() {
        let dir = std::env::temp_dir().join(format!("neko-rune-downloads-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = |p: PathBuf| p.file_name().unwrap().to_string_lossy().into_owned();
        // The view proposes a path; only its final name is used, and it lands in Downloads.
        let first = downloads_destination(Path::new("/private/var/tmp/report.csv"), &dir);
        assert_eq!(first, dir.join("report.csv"));
        std::fs::write(&first, b"a").unwrap();
        let second = downloads_destination(Path::new("report.csv"), &dir);
        assert_eq!(name(second.clone()), "report 2.csv", "a file there is never replaced");
        std::fs::write(&second, b"b").unwrap();
        assert_eq!(name(downloads_destination(Path::new("report.csv"), &dir)), "report 3.csv");
        // Names that try to leave the folder or hide, and names with nothing in them.
        assert_eq!(downloads_destination(Path::new("../../.ssh/authorized_keys"), &dir), dir.join("authorized_keys"));
        assert_eq!(downloads_destination(Path::new(".hidden"), &dir), dir.join("hidden"));
        assert_eq!(downloads_destination(Path::new("a\u{0}b:c/d.txt"), &dir), dir.join("d.txt"));
        assert_eq!(downloads_destination(Path::new(""), &dir), dir.join("download"));
        assert_eq!(downloads_destination(Path::new("noext"), &dir), dir.join("noext"));
        for p in [first, second] {
            assert!(p.starts_with(&dir));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_the_view_says_reaches_the_window_once() {
        let host = WebHost::windowed();
        assert!(host.drain_notes().is_empty());
        host.notes.borrow_mut().push("Saved to Downloads".to_owned());
        assert_eq!(host.drain_notes(), ["Saved to Downloads"]);
        assert!(host.drain_notes().is_empty(), "taken once");
    }
}
