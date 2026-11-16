//! The viewer registry: plugin slot (a) of docs/decisions/0002, "context-panel viewers, by kind".
//! A viewer is registered for the kinds (file extensions) it shows, with a priority; the context
//! panel resolves the artifact's or file's kind to the best one and calls its `build` with what it
//! has read. Built-ins register at priority 0; later phases (Luau, web) register at 10 and so win
//! for a kind both claim. At equal priority the last one registered wins.
//!
//! A viewer is a function of the input and its [`ViewerState`], and returns an element. The state
//! is what the person set on this one artifact version (camera, page, scroll) and what decoding it
//! cost (pixels, rendered PDF pages, a mesh); it is kept by `(artifact id, version)` and the
//! decoded part is released as soon as the viewer is not on screen ([`ViewerStates::release_except`]),
//! so a closed panel holds no pictures. The small things (camera, page, zoom) stay.
//!
//! **Heavy work runs off the UI thread** ([`ViewerState::load`], P3b): an image decode, a PDF's parse and
//! page renders, a CSV's row index. The viewer shows a skeleton block until the result is in; results are
//! kept in the state of the `(artifact id, version)` that asked, so a result for a version that has left the
//! screen is dropped with that state. The bytes are one `Arc<[u8]>` shared with the panel's copy.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Once, OnceLock};

use rune::El;

pub mod code;
pub mod image_view;
pub mod markdown;
pub mod model3d;
pub mod model3d_gpu;
pub mod pdf;
pub mod table;
pub mod web_pending;

/// What a viewer is given: one version of one thing to show.
pub struct ViewerInput<'a> {
    /// The kind, lowercase, as the file's extension names it (`png`, `csv`, `glb`).
    pub kind: &'a str,
    /// The file's bytes, when they were read (a file the preview may open: at most 5 MB). The same
    /// allocation the panel holds: cloning it is a count, never a copy.
    pub bytes: Option<Arc<[u8]>>,
    /// Where the file is, for a viewer that needs a path (a web view, later).
    pub path: Option<&'a Path>,
    /// The file as text, when it is UTF-8.
    pub text: Option<&'a str>,
    pub theme_dark: bool,
    pub title: &'a str,
}

impl ViewerInput<'_> {
    /// The bytes as a slice (empty when none were read).
    pub fn slice(&self) -> &[u8] {
        self.bytes.as_deref().unwrap_or(&[])
    }

    /// The bytes for a job on another thread (a count, not a copy).
    pub fn shared(&self) -> Arc<[u8]> {
        self.bytes.clone().unwrap_or_else(|| Arc::from(Vec::new()))
    }
}

/// A viewer for some kinds.
pub struct ViewerSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub kinds: &'static [&'static str],
    pub priority: i32,
    pub build: fn(&ViewerInput, &mut ViewerState) -> El,
}

/// The same artifact or file at the same version: what a viewer's state is kept by.
pub type ViewerKey = (String, u32);

/// A model's camera: orbit angles and zoom, and the wireframe switch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    /// 1 is "fitted": the model's bounding sphere just fills the narrower side.
    pub zoom: f32,
    pub wire: bool,
    /// Where the drag that is going on began (yaw, pitch).
    pub drag_from: (f32, f32),
}

impl Default for Camera {
    fn default() -> Self {
        Camera { yaw: 0.7, pitch: 0.45, zoom: 1.0, wire: false, drag_from: (0.0, 0.0) }
    }
}

/// How work given to [`ViewerState::spawn`] runs.
enum Mode {
    /// On the calling thread, at once (tests and headless runs: a scripted run sees the result).
    Inline,
    /// On a thread of its own; the waker has the window run its view when the result is in.
    Threads(Arc<dyn Fn() + Send + Sync>),
    /// Kept until [`ViewerStates::run_held`] (a test that wants to see the skeleton first).
    Held,
}

/// Where viewers' jobs run: shared by every [`ViewerState`] of a window.
pub struct Runner {
    mode: RefCell<Mode>,
    held: RefCell<Vec<Box<dyn FnOnce() + Send>>>,
}

impl Default for Runner {
    fn default() -> Runner {
        Runner { mode: RefCell::new(Mode::Inline), held: RefCell::new(Vec::new()) }
    }
}

impl Runner {
    fn run(&self, work: Box<dyn FnOnce() + Send>) {
        let wake = match &*self.mode.borrow() {
            Mode::Inline => None,
            Mode::Held => {
                self.held.borrow_mut().push(work);
                return;
            }
            Mode::Threads(wake) => Some(wake.clone()),
        };
        let Some(wake) = wake else { return work() };
        // A thread that cannot start leaves the work for the calling thread: slower, never lost.
        let slot = Arc::new(Mutex::new(Some(work)));
        let again = slot.clone();
        let spawned = std::thread::Builder::new().name("neko-viewer".into()).spawn(move || {
            if let Some(work) = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                work();
            }
            wake();
        });
        if spawned.is_err()
            && let Some(work) = again.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
        {
            work();
        }
    }
}

/// A job's answer, waiting for the viewer to pick it up. Dropped with the state that asked.
type Slot = Arc<Mutex<Option<Box<dyn Any + Send>>>>;

/// Everything a viewer remembers about one version.
#[derive(Default)]
pub struct ViewerState {
    pub key: ViewerKey,
    /// Where "Open in web UI" goes (`artifact?id=<id>&v=<n>`), when there is a place.
    pub route: Option<String>,
    /// The first row on screen (a table's first record, a document's first line).
    pub scroll: usize,
    /// The page a document is on (0-based).
    pub page: usize,
    /// How far in (a PDF's, an image's); 0 means "not set yet" and reads as 1.
    pub zoom: f32,
    pub camera: Camera,
    /// What decoding it made, released when the viewer is off screen.
    data: Option<Box<dyn Any>>,
    /// Jobs asked for by tag, and where their answers land. Released with `data`.
    jobs: HashMap<String, Slot>,
    runner: Option<Rc<Runner>>,
}

impl ViewerState {
    pub fn new(key: ViewerKey) -> ViewerState {
        ViewerState { key, ..ViewerState::default() }
    }

    /// The decoded content, made by `make` the first time it is asked for after the last release.
    pub fn data<T: 'static>(&mut self, make: impl FnOnce() -> T) -> &mut T {
        if !self.data.as_ref().is_some_and(|d| d.is::<T>()) {
            self.data = Some(Box::new(make()));
        }
        self.data.as_mut().and_then(|d| d.downcast_mut::<T>()).expect("the data was just made as this type")
    }

    /// The decoded content, if there is some of this type.
    pub fn peek<T: 'static>(&mut self) -> Option<&mut T> {
        self.data.as_mut().and_then(|d| d.downcast_mut::<T>())
    }

    /// Puts `value` as the decoded content.
    pub fn set_data<T: 'static>(&mut self, value: T) {
        self.data = Some(Box::new(value));
    }

    /// Runs `job` off the UI thread under `tag` (once: asking again for a tag that is in flight or
    /// done does nothing). Its answer is taken with [`ViewerState::ready`].
    pub fn spawn<T: Send + 'static>(&mut self, tag: &str, job: impl FnOnce() -> T + Send + 'static) {
        if self.jobs.contains_key(tag) {
            return;
        }
        let slot: Slot = Arc::new(Mutex::new(None));
        self.jobs.insert(tag.to_owned(), slot.clone());
        let work: Box<dyn FnOnce() + Send> = Box::new(move || {
            let answer = job();
            *slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(answer));
        });
        match &self.runner {
            Some(runner) => runner.run(work),
            None => work(),
        }
    }

    /// Whether a job with this tag was asked for and its answer has not been taken.
    pub fn asked(&self, tag: &str) -> bool {
        self.jobs.contains_key(tag)
    }

    /// The answer of the job under `tag`, once it is in (taken: the tag may be asked again).
    pub fn ready<T: Send + 'static>(&mut self, tag: &str) -> Option<T> {
        let slot = self.jobs.get(tag)?;
        let answer = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()?;
        self.jobs.remove(tag);
        answer.downcast::<T>().ok().map(|b| *b)
    }

    /// The decoded content `D`, made from the answer of `job` (which runs off the UI thread, once).
    /// `None` while the job is out: the viewer shows its skeleton and is built again when it lands.
    pub fn load<J: Send + 'static, D: 'static>(&mut self, tag: &str, job: impl FnOnce() -> J + Send + 'static, make: impl FnOnce(J) -> D) -> Option<&mut D> {
        if !self.data.as_ref().is_some_and(|d| d.is::<D>()) {
            if !self.asked(tag) {
                self.spawn(tag, job);
            }
            let made = make(self.ready::<J>(tag)?);
            self.data = Some(Box::new(made));
        }
        self.data.as_mut().and_then(|d| d.downcast_mut::<D>())
    }

    /// Forgets the jobs whose tag starts with `prefix` (their answers are dropped when they land).
    pub fn drop_jobs(&mut self, prefix: &str) {
        self.jobs.retain(|tag, _| !tag.starts_with(prefix));
    }

    /// Lets go of what decoding made, and of jobs still out (their answers are dropped when they
    /// land); the camera, page and zoom stay.
    pub fn release(&mut self) {
        self.data = None;
        self.jobs.clear();
    }

    pub fn resident(&self) -> bool {
        self.data.is_some() || !self.jobs.is_empty()
    }

    pub fn zoom(&self) -> f32 {
        if self.zoom > 0.0 { self.zoom } else { 1.0 }
    }
}

/// The viewer states of this run, by key. Interior mutability, because a view is built from `&App`
/// and a viewer decodes the first time it is drawn.
#[derive(Default)]
pub struct ViewerStates {
    map: RefCell<HashMap<ViewerKey, ViewerState>>,
    runner: Rc<Runner>,
}

impl ViewerStates {
    /// Runs `f` on the state for `key`, making it the first time. Do not call this from inside `f`.
    pub fn with<R>(&self, key: &ViewerKey, f: impl FnOnce(&mut ViewerState) -> R) -> R {
        let mut map = self.map.borrow_mut();
        f(map.entry(key.clone()).or_insert_with(|| ViewerState { runner: Some(self.runner.clone()), ..ViewerState::new(key.clone()) }))
    }

    /// Jobs run on threads of their own from now on; `wake` has the window build its view again.
    pub fn use_threads(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        *self.runner.mode.borrow_mut() = Mode::Threads(wake);
    }

    /// Holds jobs (tests): they run at [`ViewerStates::run_held`].
    pub fn hold(&self) {
        *self.runner.mode.borrow_mut() = Mode::Held;
    }

    /// Runs the held jobs, and says how many there were.
    pub fn run_held(&self) -> usize {
        let held: Vec<_> = self.runner.held.borrow_mut().drain(..).collect();
        let n = held.len();
        for work in held {
            work();
        }
        n
    }

    /// Releases what every viewer decoded except the one named (the one on screen), and returns how
    /// many held something.
    pub fn release_except(&self, keep: Option<&ViewerKey>) -> usize {
        let mut released = 0;
        for (key, state) in self.map.borrow_mut().iter_mut() {
            if Some(key) != keep && state.resident() {
                state.release();
                released += 1;
            }
        }
        released
    }

    /// How many viewers hold decoded content (a test reads this).
    pub fn resident(&self) -> usize {
        self.map.borrow().values().filter(|s| s.resident()).count()
    }

    /// Forgets states that hold nothing worth keeping, so the map does not grow without end.
    pub fn len(&self) -> usize {
        self.map.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.borrow().is_empty()
    }
}

fn registry() -> &'static Mutex<Vec<&'static ViewerSpec>> {
    static REGISTRY: OnceLock<Mutex<Vec<&'static ViewerSpec>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Adds a viewer. It lives as long as the process (a registry entry is never removed).
pub fn register(spec: ViewerSpec) {
    let leaked: &'static ViewerSpec = Box::leak(Box::new(spec));
    registry().lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(leaked);
}

/// The built-in viewers, at priority 0 (once).
pub fn ensure_builtins() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        for spec in builtins() {
            register(spec);
        }
    });
}

/// The kinds the code viewer shows by name; any other text up to [`TEXT_LIMIT`] bytes falls to it too.
pub const CODE_KINDS: &[&str] = &["rs", "ts", "tsx", "js", "py", "go", "json", "toml", "yaml", "yml", "sh", "txt", "log", "css", "sql"];
/// The most text the code viewer takes for a kind it was not named for.
pub const TEXT_LIMIT: usize = 2 * 1024 * 1024;

fn builtins() -> Vec<ViewerSpec> {
    vec![
        ViewerSpec { id: "image", title: "Image", kinds: &["png", "jpg", "jpeg", "webp", "gif"], priority: 0, build: image_view::build_image },
        ViewerSpec { id: "svg", title: "SVG", kinds: &["svg"], priority: 0, build: image_view::build_svg },
        ViewerSpec { id: "markdown", title: "Markdown", kinds: &["md", "markdown"], priority: 0, build: markdown::build },
        ViewerSpec { id: "code", title: "Code", kinds: CODE_KINDS, priority: 0, build: code::build },
        ViewerSpec { id: "table", title: "Table", kinds: &["csv", "tsv"], priority: 0, build: table::build },
        ViewerSpec { id: "pdf", title: "PDF", kinds: &["pdf"], priority: 0, build: pdf::build },
        ViewerSpec { id: "model3d", title: "3D model", kinds: &["glb", "gltf", "stl", "obj"], priority: 0, build: model3d::build },
        ViewerSpec { id: "web-pending", title: "Page", kinds: &["html", "htm", "excalidraw"], priority: 0, build: web_pending::build },
    ]
}

/// The viewer for `kind`: the highest priority wins; at equal priority the last one registered.
pub fn resolve(kind: &str) -> Option<&'static ViewerSpec> {
    ensure_builtins();
    let kind = kind.trim_start_matches('.').to_ascii_lowercase();
    let all = registry().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    all.iter().enumerate().filter(|(_, s)| s.kinds.contains(&kind.as_str())).max_by_key(|(i, s)| (s.priority, *i)).map(|(_, s)| *s)
}

/// Whether bytes are text the code viewer may show for a kind nobody named: UTF-8, no NUL, at most
/// [`TEXT_LIMIT`].
pub fn is_text(bytes: &[u8]) -> bool {
    bytes.len() <= TEXT_LIMIT && !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// The viewer for a file of `kind` whose bytes are `bytes`: the named one, else the code viewer when
/// the bytes are text, else none (the panel shows the fallback).
pub fn resolve_for(kind: &str, bytes: Option<&[u8]>) -> Option<&'static ViewerSpec> {
    resolve(kind).or_else(|| bytes.filter(|b| is_text(b)).and_then(|_| resolve("txt")))
}

/// A thing a viewer built, the kinds it has, for the panel's icon: which family a kind belongs to.
pub fn family(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" => "image",
        "md" | "markdown" | "txt" | "log" => "text",
        "csv" | "tsv" => "table",
        "pdf" => "document",
        "glb" | "gltf" | "stl" | "obj" => "model",
        "html" | "htm" | "excalidraw" => "page",
        _ => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nothing(_: &ViewerInput, _: &mut ViewerState) -> El {
        rune::el()
    }

    #[test]
    fn every_builtin_kind_resolves_to_its_viewer() {
        let cases = [
            ("png", "image"), ("jpg", "image"), ("jpeg", "image"), ("webp", "image"), ("gif", "image"),
            ("svg", "svg"), ("md", "markdown"), ("markdown", "markdown"),
            ("rs", "code"), ("ts", "code"), ("tsx", "code"), ("js", "code"), ("py", "code"), ("go", "code"), ("json", "code"), ("toml", "code"),
            ("yaml", "code"), ("yml", "code"), ("sh", "code"), ("txt", "code"), ("log", "code"), ("css", "code"), ("sql", "code"),
            ("csv", "table"), ("tsv", "table"), ("pdf", "pdf"),
            ("glb", "model3d"), ("gltf", "model3d"), ("stl", "model3d"), ("obj", "model3d"),
            ("html", "web-pending"), ("htm", "web-pending"), ("excalidraw", "web-pending"),
        ];
        for (kind, id) in cases {
            assert_eq!(resolve(kind).map(|s| s.id), Some(id), "{kind}");
        }
        assert_eq!(resolve("PNG").map(|s| s.id), Some("image"), "kinds are case-blind");
        assert_eq!(resolve(".csv").map(|s| s.id), Some("table"));
        assert!(resolve("xyz").is_none(), "an unknown kind has no viewer");
        assert!(ensure_builtins_priority_is_zero());
    }

    fn ensure_builtins_priority_is_zero() -> bool {
        builtins().iter().all(|s| s.priority == 0)
    }

    #[test]
    fn the_highest_priority_wins_and_a_tie_goes_to_the_last_registered() {
        ensure_builtins();
        register(ViewerSpec { id: "test-low", title: "low", kinds: &["zzlow"], priority: 0, build: nothing });
        register(ViewerSpec { id: "test-low-2", title: "low 2", kinds: &["zzlow"], priority: 0, build: nothing });
        assert_eq!(resolve("zzlow").map(|s| s.id), Some("test-low-2"), "the last registered wins a tie");
        register(ViewerSpec { id: "test-high", title: "high", kinds: &["zzlow"], priority: 10, build: nothing });
        register(ViewerSpec { id: "test-after", title: "after", kinds: &["zzlow"], priority: 0, build: nothing });
        assert_eq!(resolve("zzlow").map(|s| s.id), Some("test-high"), "a later phase at 10 beats the ones at 0, whatever came after");
    }

    #[test]
    fn any_text_up_to_two_megabytes_falls_to_the_code_viewer() {
        assert_eq!(resolve_for("weirdext", Some(b"hello\nworld")).map(|s| s.id), Some("code"));
        assert!(resolve_for("weirdext", Some(&[0u8, 1, 2, 3])).is_none(), "binary is not text");
        assert!(resolve_for("weirdext", Some(&[0xff, 0xfe, 0xfd])).is_none(), "not UTF-8");
        let big = vec![b'a'; TEXT_LIMIT + 1];
        assert!(resolve_for("weirdext", Some(&big)).is_none(), "over 2 MB");
        assert!(resolve_for("weirdext", Some(&big[..TEXT_LIMIT])).is_some(), "at 2 MB");
        assert!(resolve_for("weirdext", None).is_none());
        assert_eq!(resolve_for("png", None).map(|s| s.id), Some("image"));
    }

    #[test]
    fn state_is_kept_per_version_and_decoded_data_is_released_when_off_screen() {
        let states = ViewerStates::default();
        let (a, b): (ViewerKey, ViewerKey) = (("art1".into(), 2), ("art1".into(), 3));
        states.with(&a, |s| {
            s.page = 4;
            s.camera.yaw = 1.5;
            s.data(|| vec![1u8; 1000]);
        });
        states.with(&b, |s| {
            s.data(|| vec![2u8; 1000]);
        });
        assert_eq!((states.len(), states.resident()), (2, 2));
        // Version 3 is on screen: version 2 lets go of its decoded bytes but keeps what the person set.
        assert_eq!(states.release_except(Some(&b)), 1);
        assert_eq!(states.resident(), 1);
        states.with(&a, |s| {
            assert_eq!((s.page, s.camera.yaw), (4, 1.5));
            assert!(!s.resident());
            assert!(s.peek::<Vec<u8>>().is_none());
        });
        // Nothing on screen: nothing is held.
        assert_eq!(states.release_except(None), 1);
        assert_eq!(states.resident(), 0);
        // Asking for the data again decodes again.
        states.with(&a, |s| assert_eq!(s.data(|| vec![9u8; 2]).len(), 2));
    }

    #[test]
    fn a_job_runs_off_the_call_and_its_answer_is_picked_up_once_and_keyed_to_its_version() {
        let states = ViewerStates::default();
        states.hold();
        let (v1, v2): (ViewerKey, ViewerKey) = (("art".into(), 1), ("art".into(), 2));
        // Nothing is made until the job has run: the viewer is told "not yet" (it draws a skeleton).
        assert!(states.with(&v1, |s| s.load("decode", || 40 + 2, |n: i32| n.to_string()).is_none()));
        assert!(states.with(&v1, |s| s.load("decode", || unreachable!("asked once"), |n: i32| n.to_string()).is_none()), "asking again while it is out does not start another");
        assert_eq!(states.resident(), 1, "a job in flight counts as held, so leaving the screen releases it");
        assert_eq!(states.run_held(), 1);
        assert_eq!(states.with(&v1, |s| s.load("decode", || 0, |n: i32| n.to_string()).cloned()), Some("42".to_owned()));
        // The decoded content stays; the job is not asked again.
        assert_eq!(states.with(&v1, |s| s.load("decode", || unreachable!("done"), |n: i32| n.to_string()).cloned()), Some("42".to_owned()));
        // A result that lands after the version left the screen is dropped with it.
        assert!(states.with(&v2, |s| s.load("decode", || 7, |n: i32| n.to_string()).is_none()));
        assert_eq!(states.release_except(Some(&v1)), 1, "version 2 left the screen with its job out");
        assert_eq!(states.run_held(), 1, "the job still finishes");
        assert!(states.with(&v2, |s| !s.resident() && s.ready::<i32>("decode").is_none()), "but nobody keeps what it made");
        // Real threads: the waker is called once the answer is in.
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let w = woken.clone();
        let threaded = ViewerStates::default();
        threaded.use_threads(Arc::new(move || {
            w.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        let key: ViewerKey = ("t".into(), 1);
        threaded.with(&key, |s| s.spawn("x", || 5usize));
        let started = std::time::Instant::now();
        while woken.load(std::sync::atomic::Ordering::SeqCst) == 0 && started.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(threaded.with(&key, |s| s.ready::<usize>("x")), Some(5));
    }

    #[test]
    fn a_zoom_that_was_never_set_reads_as_one() {
        let mut s = ViewerState::new(("a".into(), 1));
        assert_eq!(s.zoom(), 1.0);
        s.zoom = 1.5;
        assert_eq!(s.zoom(), 1.5);
    }
}
