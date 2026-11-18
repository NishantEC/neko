//! P5a: Rune as the Mac app (docs/knowledge/rune-as-the-app.md). The Swift app is still the default client and does these duties today:
//! it starts and watches the daemon, owns the menu bar item, posts the routine notifications, takes the global keys and the `neko://` links.
//!
//! **The gate.** All of it is OFF unless this client is started with `--app` or `NEKO_RUNE_APP=1` ([`gate`]). Without the flag Rune is
//! the plain client it has been, to the byte: nothing here is created.
//!
//! **The Swift-running guard.** Two apps must never hold the same duty (two menu bar icons, two daemons being started, two banners per
//! item, two pills in one corner). In `--app` mode Rune therefore asks every 10 s whether the Swift app (bundle id [`SWIFT_BUNDLE_ID`],
//! not this process) is running; while it is, [`duties`] gives Rune none of them and Settings, Neko updates, says so. When the Swift app
//! quits, Rune takes the duties over without a restart. The takeover is one way: once Rune holds the duties it keeps them for this run.

pub mod app_menu;
pub mod daemon;
pub mod open_in;
pub mod status_item;

use std::path::PathBuf;
use std::time::Duration;

use rune::{AppHandle, Tx};

use crate::app::App;

/// The command-line flag and the environment variable that make Rune the Mac app.
pub const FLAG: &str = "--app";
pub const ENV: &str = "NEKO_RUNE_APP";
/// The Swift app's bundle id (`scripts/install-native.sh` checks the same one).
pub const SWIFT_BUNDLE_ID: &str = "dev.neko.launcher";
/// How often the guard looks for the Swift app.
pub const GUARD_EVERY: Duration = Duration::from_secs(10);
/// The one read-only row Settings, Neko updates, shows while the Swift app holds the duties.
pub const SWIFT_ROW_TITLE: &str = "Neko app";
pub const SWIFT_ROW_WORDS: &str = "The Swift Neko app is running. It keeps the menu bar, notifications and hotkeys until it quits.";

/// Whether `--app` mode is on: the flag among the arguments, or the variable set to `1`.
pub fn gate(args: &[String], env: Option<&str>) -> bool {
    args.iter().any(|a| a == FLAG) || env == Some("1")
}

/// Which duties this process holds right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Duties {
    /// Starts and watches the daemon (`daemon.rs`).
    pub daemon: bool,
    /// The menu bar item and its menu (`status_item.rs`).
    pub status_item: bool,
    /// The application menu (`App::app_menu`).
    pub app_menu: bool,
    /// Routine notifications (`notify.rs`).
    pub notifications: bool,
    /// `neko://` links (`App::open_url`).
    pub links: bool,
    /// The pill, the launcher and the global keys (`overlays.rs`): `--app` implies `--overlays`.
    pub overlays: bool,
    /// Rune is in `--app` mode but the Swift app holds the duties: Settings says so.
    pub swift_holds: bool,
}

impl Duties {
    pub fn any(&self) -> bool {
        self.daemon || self.status_item || self.app_menu || self.notifications || self.links || self.overlays
    }
}

/// What Rune does, from the flag and whether the Swift app runs. No flag: nothing. The flag with the Swift app running: nothing,
/// and the row says why. The flag with no Swift app: everything.
pub fn duties(app_flag: bool, swift_running: bool) -> Duties {
    match (app_flag, swift_running) {
        (false, _) => Duties::default(),
        (true, true) => Duties { swift_holds: true, ..Duties::default() },
        (true, false) => Duties { daemon: true, status_item: true, app_menu: true, notifications: true, links: true, overlays: true, swift_holds: false },
    }
}

/// Whether the Swift app is among the running applications. `find` lists the process ids that run with a bundle id (the system's
/// answer, replaced in tests); this process is never counted (a bundled Rune may carry the same id).
pub fn swift_running(find: impl Fn(&str) -> Vec<i32>, me: i32) -> bool {
    find(SWIFT_BUNDLE_ID).into_iter().any(|pid| pid != me)
}

/// The process ids of the running applications with this bundle id (`NSRunningApplication`).
#[cfg(target_os = "macos")]
pub fn running_with_bundle_id(id: &str) -> Vec<i32> {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(id));
    apps.iter().map(|a| a.processIdentifier()).collect()
}

#[cfg(not(target_os = "macos"))]
pub fn running_with_bundle_id(_id: &str) -> Vec<i32> {
    Vec::new()
}

/// The real check: the Swift app, as the system lists it.
pub fn swift_app_running() -> bool {
    swift_running(running_with_bundle_id, std::process::id() as i32)
}

/// What a start in `--app` mode says on stderr, once, when the Swift app holds the duties.
pub const GUARD_LOG: &str = "neko-rune: --app: the Swift Neko app is running, so Rune keeps the menu bar, notifications and hotkeys to itself until it quits (checking every 10 s)";
/// And when it quits.
pub const TAKEOVER_LOG: &str = "neko-rune: --app: the Swift Neko app has quit; Rune takes over the menu bar, notifications, hotkeys and the daemon watch";

/// What the window keeps of the Mac-app mode.
#[derive(Default)]
pub struct Shell {
    /// `--app` (or `NEKO_RUNE_APP=1`) was given.
    pub app_flag: bool,
    /// `--overlays` was given without `--app`: the overlays alone, as before P5a.
    pub overlays_flag: bool,
    /// The last answer of the guard; None until it has asked.
    pub swift_running: Option<bool>,
    /// What is held now. Once held, kept for the run (the takeover is one way).
    pub duties: Duties,
    /// The daemon's socket, for the daemon watch and the notifier.
    pub socket: PathBuf,
    daemon_started: bool,
    /// The window's own handle: what the menu bar item, the app menu and the notifier call back into.
    pub handle: Option<AppHandle<App>>,
    /// Where "Open Neko" goes (`open_in.rs`), kept in this client's saved state.
    pub open_in: open_in::OpenIn,
    /// The application menu as last set (it is only replaced when the model changes).
    pub app_menu_shown: Option<Vec<app_menu::Item>>,
}

impl Shell {
    pub fn new(app_flag: bool, overlays_flag: bool, socket: PathBuf) -> Shell {
        Shell { app_flag, overlays_flag, socket, open_in: crate::app::load_open_in(), ..Shell::default() }
    }

    /// Whether the overlays (pill, launcher, global keys) should be attached now.
    pub fn wants_overlays(&self) -> bool {
        self.overlays_flag || self.duties.overlays
    }

    /// Takes the guard's answer; true when the duties changed.
    pub fn observe_swift(&mut self, running: bool) -> bool {
        let before = self.duties;
        if self.swift_running != Some(running) && self.app_flag {
            if running && !self.duties.any() {
                eprintln!("{GUARD_LOG}");
            } else if self.swift_running == Some(true) {
                eprintln!("{TAKEOVER_LOG}");
            }
        }
        self.swift_running = Some(running);
        let now = duties(self.app_flag, running);
        // Once Rune holds the duties it keeps them: the Swift app starting later does not take them back.
        if now.any() || !self.duties.any() {
            self.duties = now;
        }
        before != self.duties
    }
}

impl App {
    /// The guard answered (or answered again): starts what Rune now holds. True when something on screen changed (the Settings row).
    pub fn set_swift_running(&mut self, running: bool) -> bool {
        let changed = self.shell.observe_swift(running);
        self.start_duties();
        changed
    }

    /// Starts each duty once, the first time Rune holds it.
    pub fn start_duties(&mut self) {
        let d = self.shell.duties;
        if d.daemon && !self.shell.daemon_started {
            self.shell.daemon_started = true;
            daemon::spawn_watch(self.shell.socket.clone());
        }
        if self.shell.wants_overlays() {
            self.overlays.attach_pending();
        }
    }
}

impl App {
    /// Once a poll: the menu bar item and the application menu follow the snapshot (only what changed is touched).
    pub fn tick_shell(&mut self) {
        let d = self.shell.duties;
        let Some(handle) = self.shell.handle.clone() else { return };
        if d.status_item {
            let counts = self.summary.as_ref().map(status_item::Counts::of).unwrap_or_default();
            // Uninstall is offered from an installed app; the uninstaller arrives in the next commit, so it is not offered yet.
            let entries = status_item::menu(&counts, self.shell.open_in, false);
            status_item::sync(&handle, status_item::IconState::of(&counts), &entries);
        }
        if d.app_menu {
            let model = app_menu::model(self.pages.appearance.as_deref());
            if self.shell.app_menu_shown.as_ref() != Some(&model) && handle.set_menu(app_menu::items(&model)).is_ok() {
                self.shell.app_menu_shown = Some(model);
            }
        }
    }

    /// A choice in the menu bar item's menu.
    pub fn shell_action(&mut self, action: status_item::Action, _tx: &mut Tx) {
        use status_item::Action;
        match action {
            Action::QuickPanel => self.quick_panel(),
            Action::OpenNeko => self.open_neko(None),
            Action::Settings => self.open_neko(Some("settings")),
            Action::OpenIn(choice) => self.set_open_in(choice),
            Action::StopAll => self.stop_all_key(),
            // The menu does not offer it until the uninstaller is built.
            Action::Uninstall => {}
            Action::Quit => {
                if let Some(h) = &self.shell.handle {
                    h.quit();
                }
            }
        }
    }

    /// "Quick panel": the launcher panel when the overlays are on, else the palette in the window.
    pub fn quick_panel(&mut self) {
        if self.overlays.launcher.is_some() {
            self.summon_launcher();
        } else {
            self.palette_toggle();
            if let Some(h) = &self.shell.handle {
                h.open();
            }
        }
    }

    /// "Open Neko" (and "Settings…"): where the person chose (`OpenNeko.workspace`). The browser needs the daemon's address; without it the
    /// window opens, as Swift's `usesBrowser` falls back. The window opens at the route.
    pub fn open_neko(&mut self, route: Option<&str>) {
        if self.shell.open_in == open_in::OpenIn::Browser && self.web.is_some() {
            let route = route.unwrap_or_default().to_owned();
            self.extra.browser_routes.push(route.clone());
            self.call(crate::link::Call::SignedWebUrl { route });
            return;
        }
        if let Some(route) = route {
            self.open_route(route);
        }
        if let Some(h) = &self.shell.handle {
            h.open();
        }
    }

    /// The Open-in choice: kept, and the menu follows.
    pub fn set_open_in(&mut self, choice: open_in::OpenIn) {
        self.shell.open_in = choice;
        if self.persist_layout {
            crate::app::write_open_in(choice.key());
        }
    }
}

/// Starts the guard's thread: it asks every [`GUARD_EVERY`] and tells the window only when the answer changed.
pub fn spawn_guard(handle: AppHandle<App>, first: bool) {
    let _ = std::thread::Builder::new().name("neko-swift-guard".into()).spawn(move || {
        let mut last = first;
        loop {
            std::thread::sleep(GUARD_EVERY);
            let now = swift_app_running();
            if now != last {
                last = now;
                handle.update_if(move |s, tx| {
                    let changed = s.set_swift_running(now);
                    tx.instant();
                    changed
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_gate_is_the_flag_or_the_variable_set_to_one() {
        assert!(!gate(&args(&["neko-rune"]), None));
        assert!(!gate(&args(&["neko-rune", "--overlays"]), None), "the overlays flag is not the app flag");
        assert!(!gate(&args(&[]), Some("0")));
        assert!(!gate(&args(&[]), Some("")));
        assert!(gate(&args(&["neko-rune", "--app"]), None));
        assert!(gate(&args(&[]), Some("1")));
    }

    #[test]
    fn duties_by_flag_and_swift() {
        // (flag, swift running) -> what is held
        assert_eq!(duties(false, false), Duties::default(), "no flag: a plain client");
        assert_eq!(duties(false, true), Duties::default(), "no flag: the guard is not even mentioned");
        assert!(!duties(false, true).swift_holds);
        let held = duties(true, true);
        assert!(!held.any(), "the Swift app holds every duty: {held:?}");
        assert!(held.swift_holds, "and Settings says so");
        let ours = duties(true, false);
        assert!(ours.daemon && ours.status_item && ours.app_menu && ours.notifications && ours.links && ours.overlays);
        assert!(!ours.swift_holds);
    }

    #[test]
    fn the_swift_app_is_any_other_process_with_its_bundle_id() {
        let find = |pids: Vec<i32>| move |id: &str| if id == SWIFT_BUNDLE_ID { pids.clone() } else { Vec::new() };
        assert!(!swift_running(find(vec![]), 100));
        assert!(swift_running(find(vec![4242]), 100));
        assert!(!swift_running(find(vec![100]), 100), "this process is not the Swift app, even with the same bundle id");
        assert!(swift_running(find(vec![100, 4242]), 100));
        assert!(!swift_running(|id| if id == "com.example.other" { vec![9] } else { vec![] }, 100), "only the Swift app's bundle id counts");
    }

    #[test]
    fn takeover_happens_when_the_swift_app_quits_and_is_kept_when_it_comes_back() {
        let mut s = Shell::new(true, false, PathBuf::from("/tmp/none.sock"));
        assert!(s.observe_swift(true), "first answer: the row appears");
        assert!(s.duties.swift_holds && !s.duties.any());
        assert!(!s.wants_overlays());
        assert!(!s.observe_swift(true), "the same answer changes nothing");
        assert!(s.observe_swift(false), "the Swift app quit: Rune takes over");
        assert!(s.duties.daemon && s.duties.status_item && s.wants_overlays() && !s.duties.swift_holds);
        assert!(!s.observe_swift(true), "Swift launching again does not take the duties back");
        assert!(s.duties.daemon && !s.duties.swift_holds);
    }

    #[test]
    fn without_the_flag_nothing_is_ever_held_and_no_row_appears() {
        let mut s = Shell::new(false, false, PathBuf::from("/tmp/none.sock"));
        s.observe_swift(true);
        assert_eq!(s.duties, Duties::default());
        s.observe_swift(false);
        assert_eq!(s.duties, Duties::default());
        assert!(!s.wants_overlays());
        let legacy = Shell::new(false, true, PathBuf::from("/tmp/none.sock"));
        assert!(legacy.wants_overlays(), "`--overlays` alone still means the overlays");
        assert!(!legacy.duties.any());
    }

    #[test]
    fn the_guard_words_say_what_is_held_and_until_when() {
        assert_eq!(SWIFT_ROW_WORDS, "The Swift Neko app is running. It keeps the menu bar, notifications and hotkeys until it quits.");
        assert_eq!(GUARD_EVERY, Duration::from_secs(10));
    }
}
