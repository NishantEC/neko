//! P4, the overlays (docs/knowledge/rune-overlays.md): the activity pill (`presence.rs`), the launcher panel (`launcher.rs`) and the stop-all
//! key, wired to the window here.
//!
//! **The gate.** Until P5 the Swift app is the default client and owns the summon hotkey, ⌘⇧Esc and its own pill, so all three are OFF here
//! unless this client is started with `--overlays` or `NEKO_RUNE_OVERLAYS=1`. Off, nothing is created and no shortcut is registered
//! ([`plan`] is empty, [`install`] makes nothing). P5 flips the default and retires the Swift ones.
//!
//! Every Rune API the overlays use is named in the knowledge note; the shape is one `OverlaySpec` for the pill, one `launcher_panel()`
//! window for the launcher, and two `register_shortcut` calls (the daemon's summon combo, which follows `GetHotkey`, and ⌘⇧Esc). A shortcut
//! that cannot be registered (the Swift app holds it) is logged to stderr and the client carries on.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use neko_protocol::{HotkeyCombo, Modifier};
use rune::{Align, AppHandle, DisplayTarget, KeyPolicy, OverlayHandle, OverlayInput, OverlayLevel, OverlayOptions, OverlayPlacement, OverlaySpec, ScreenEdge, Shortcut, ShortcutId, Tx, WindowOptions, WindowSpec};

use crate::app::App;
use crate::launcher::{self, Launcher, Presentation};
use crate::presence::{self, Facts, Pill, PresenceState, Step, Tracker};
use crate::theme::Hex;

/// The command-line flag and the environment variable that turn the overlays on.
pub const FLAG: &str = "--overlays";
pub const ENV: &str = "NEKO_RUNE_OVERLAYS";
/// How often the pill reads the workbench (it reads the summary's tasks, so not every frame).
const LOOK_EVERY_MS: i64 = 250;
/// ⌘⇧Esc, the stop-all key (`StopAllWork.shortcutLabel`, registered by `PaletteController` with Carbon).
pub const STOP_ACCELERATOR: &str = "cmd+shift+escape";

/// Whether the overlays are on: the flag among the arguments, or the variable set to `1`.
pub fn gate(args: &[String], env: Option<&str>) -> bool {
    args.iter().any(|a| a == FLAG) || env == Some("1")
}

/// What to make. Empty when the gate is off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub pill: bool,
    pub launcher: bool,
    pub summon_shortcut: bool,
    pub stop_shortcut: bool,
}

pub fn plan(on: bool) -> Plan {
    Plan { pill: on, launcher: on, summon_shortcut: on, stop_shortcut: on }
}

impl Plan {
    pub fn any(&self) -> bool {
        self.pill || self.launcher || self.summon_shortcut || self.stop_shortcut
    }
}

/// What [`install`] made: the handles the window keeps. Every field is empty when the plan is.
#[derive(Default)]
pub struct Installed {
    pub plan: Plan,
    pub pill: Option<OverlayHandle<Pill>>,
    pub launcher: Option<AppHandle<Launcher>>,
    pub presentation: Arc<Presentation>,
}

/// Creates what the plan asks for on the app builder (nothing opens until it runs): the pill's overlay and the launcher's window.
pub fn install(app: &mut rune::App<App>, plan: Plan) -> Installed {
    let mut out = Installed { plan, ..Installed::default() };
    let main = app.handle();
    if plan.pill {
        // One overlay for the whole run, shown and hidden, never recreated: a status-level panel on every Space that takes only the clicks
        // the capsule itself asks for (`OverlayInput::Regions`), made when first shown and given back to the GPU when hidden.
        let mut spec = OverlaySpec::new("neko-activity-pill", presence::WINDOW, presence::view);
        spec.edge = ScreenEdge::Bottom;
        spec.align = Align::End;
        spec.margin = presence::MARGIN - presence::GLOW;
        spec.inset = presence::MARGIN - presence::GLOW;
        let options = OverlayOptions { display: DisplayTarget::Primary, level: OverlayLevel::Status, keys: KeyPolicy::None, input: OverlayInput::Regions, prewarm: true, ..OverlayOptions::default() };
        out.pill = Some(app.overlay(spec.options(options).release_gpu_when_hidden(), Pill::new(Some(main.clone()))));
    }
    if plan.launcher {
        let presentation = out.presentation.clone();
        let launcher_app = rune::App::new(rune::AppSpec {
            families: (crate::theme::FONT_SANS, crate::theme::FONT_MONO),
            on_key: Some(|l: &mut Launcher, ev, _| l.key(ev)),
            ..rune::AppSpec::new("Neko launcher", launcher::WINDOW, || unreachable!("the launcher is built by init"), launcher::view)
        })
        .windowless()
        .keep_running();
        let me = launcher_app.handle();
        let (for_state, for_window) = (me.clone(), me.clone());
        let launcher_app = launcher_app.init(move || {
            let mut l = Launcher::new(presentation, Some(main));
            l.me = Some(for_state);
            l
        }).on_focus(|l: &mut Launcher, focused, _| l.focus_changed(focused));
        app.window(WindowSpec::from_app("launcher", launcher_app).configured_options(WindowOptions::launcher_panel()));
        out.launcher = Some(for_window);
    }
    out
}

/// A shortcut as Rune parses it: `Cmd`/`Alt`/`Ctrl`/`Shift` and the key as the daemon names it (`Space`, `KeyK`, `Digit1`).
pub fn accelerator(combo: &HotkeyCombo) -> String {
    let mut parts: Vec<String> = combo
        .modifiers
        .iter()
        .map(|m| match m {
            Modifier::Cmd => "cmd",
            Modifier::Alt => "alt",
            Modifier::Ctrl => "ctrl",
            Modifier::Shift => "shift",
        })
        .map(str::to_owned)
        .collect();
    parts.push(combo.key.clone());
    parts.join("+")
}

/// The screen's room around its usable area, in points: the menu bar on top, the Dock at the bottom or the right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub bottom: f32,
    pub right: f32,
    /// The visible area's height.
    pub visible_height: f32,
}

/// How far the pill's window sits from the bottom edge and from the right edge of the screen, so the capsule is [`presence::MARGIN`] from
/// the visible area's corner (Swift places it from `visibleFrame`; Rune places from the screen's full edge, over the Dock).
pub fn pill_margins(i: Insets) -> (f32, f32) {
    (presence::MARGIN - presence::GLOW + i.bottom, presence::MARGIN - presence::GLOW + i.right)
}

/// The insets of the screen the pointer is on (`under_pointer`), or of the main screen.
#[cfg(target_os = "macos")]
pub fn insets(under_pointer: bool) -> Option<Insets> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};
    let mtm = MainThreadMarker::new()?;
    let screen = if under_pointer {
        let p = NSEvent::mouseLocation();
        NSScreen::screens(mtm).iter().find(|s| {
            let f = s.frame();
            p.x >= f.origin.x && p.x < f.origin.x + f.size.width && p.y >= f.origin.y && p.y < f.origin.y + f.size.height
        })
    } else {
        None
    };
    let screen = screen.or_else(|| NSScreen::mainScreen(mtm))?;
    let (f, v) = (screen.frame(), screen.visibleFrame());
    Some(Insets {
        top: ((f.origin.y + f.size.height) - (v.origin.y + v.size.height)).max(0.0) as f32,
        bottom: (v.origin.y - f.origin.y).max(0.0) as f32,
        right: ((f.origin.x + f.size.width) - (v.origin.x + v.size.width)).max(0.0) as f32,
        visible_height: v.size.height as f32,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn insets(_under_pointer: bool) -> Option<Insets> {
    None
}

/// The overlays as the window holds them: the handles, the shortcuts it registered, the pill's tracker.
#[derive(Default)]
pub struct Overlays {
    pub plan: Plan,
    pub pill: Option<OverlayHandle<Pill>>,
    pub launcher: Option<AppHandle<Launcher>>,
    pub presentation: Arc<Presentation>,
    pub main: Option<AppHandle<App>>,
    /// The summon combo the daemon last said, the accelerator registered for it and its registration.
    pub combo: Option<HotkeyCombo>,
    pub summon: Option<(String, ShortcutId)>,
    pub stop: Option<ShortcutId>,
    /// The window's theme as the last frame painted it, for the launcher (it has no `App`).
    pub theme: RefCell<Option<(bool, Hex)>>,
    pub presence: Tracker,
    last_look: i64,
    /// What went wrong registering a shortcut, in words (also printed to stderr when it happened).
    pub failures: Vec<String>,
    /// What [`install`] made, kept until the duties say to attach it (`--app` mode while the Swift app runs holds it back: `shell/mod.rs`).
    pending: Option<(Installed, AppHandle<App>)>,
}

impl Overlays {
    /// Nothing made yet; `presence_on` is the pill's saved preference.
    pub fn new(presence_on: bool) -> Overlays {
        Overlays { presence: Tracker::with_enabled(presence_on), ..Overlays::default() }
    }

    /// Keeps what [`install`] made until [`Overlays::attach_pending`] (the shell's duties decide when).
    pub fn stash(&mut self, installed: Installed, main: AppHandle<App>) {
        self.pending = Some((installed, main));
    }

    /// Attaches what was stashed, once. True when something was attached.
    pub fn attach_pending(&mut self) -> bool {
        let Some((installed, main)) = self.pending.take() else { return false };
        self.attach(installed, main);
        true
    }

    /// Hands the overlays back (the Swift app started and holds them): unregisters both shortcuts, hides the pill and the launcher and
    /// keeps what [`install`] made stashed again, so [`Overlays::attach_pending`] can take it up once more. The daemon's combo is kept, so
    /// the summon key is registered again at that time. Nothing is closed for good: the window and the overlays stay built.
    pub fn detach(&mut self) -> bool {
        let Some(main) = self.main.clone() else { return false };
        if !self.plan.any() {
            return false;
        }
        if let Some(id) = self.stop.take() {
            let _ = main.unregister_shortcut(id);
        }
        if let Some((_, id)) = self.summon.take() {
            let _ = main.unregister_shortcut(id);
        }
        self.apply(Step::Hide);
        self.presence = Tracker::with_enabled(self.presence.enabled);
        if let Some(launcher) = &self.launcher {
            self.presentation.advance();
            self.presentation.set_shown(false);
            launcher.hide();
        }
        let installed = Installed { plan: self.plan, pill: self.pill.take(), launcher: self.launcher.take(), presentation: self.presentation.clone() };
        self.plan = Plan::default();
        self.pending = Some((installed, main));
        true
    }

    /// Takes what [`install`] made and registers the stop-all key (the summon key waits for the daemon's combo).
    pub fn attach(&mut self, installed: Installed, main: AppHandle<App>) {
        self.plan = installed.plan;
        self.pill = installed.pill;
        self.launcher = installed.launcher;
        self.presentation = installed.presentation;
        self.main = Some(main);
        self.register_stop();
        self.register_summon();
        self.demo();
    }

    /// Development only, as Swift's `NEKO_PRESENCE_DEMO`: `NEKO_PRESENCE_DEMO=1` (or `expanded`, with the words showing) puts a sample
    /// capsule on screen, `NEKO_PRESENCE_DEMO_AFTER=<seconds>` after a delay, so a check can see it appear while another app has the focus.
    fn demo(&self) {
        let (Some(pill), Some(main), Ok(mode)) = (&self.pill, &self.main, std::env::var("NEKO_PRESENCE_DEMO")) else { return };
        if !matches!(mode.as_str(), "1" | "expanded") {
            return;
        }
        let after = std::env::var("NEKO_PRESENCE_DEMO_AFTER").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let (pill, main, expanded) = (pill.clone(), main.clone(), mode == "expanded");
        let _ = std::thread::Builder::new().name("neko-presence-demo".into()).spawn(move || {
            std::thread::sleep(Duration::from_secs(after));
            let state = PresenceState { activity: presence::Activity::Thinking, label: "Neko is replying".to_owned(), lingers: false, task: None };
            main.update(move |app, _| {
                pill.update(move |p, _| p.force_expanded = expanded);
                app.overlays.apply(Step::Show(state));
            });
        });
    }

    fn fail(&mut self, words: String) {
        eprintln!("neko-rune: {words}");
        self.failures.push(words);
    }

    fn register_stop(&mut self) {
        let (true, Some(main), None) = (self.plan.stop_shortcut, self.main.clone(), self.stop) else { return };
        let shortcut: Shortcut = match STOP_ACCELERATOR.parse() {
            Ok(s) => s,
            Err(e) => return self.fail(format!("the stop-all shortcut is not valid: {e}")),
        };
        match main.register_shortcut(shortcut, |app: &mut App, tx: &mut Tx| {
            app.stop_all_key();
            tx.instant();
        }) {
            Ok(id) => {
                eprintln!("neko-rune: ⌘⇧Esc (stop all work) registered");
                self.stop = Some(id);
            }
            Err(e) => self.fail(format!("⌘⇧Esc (stop all work) was not registered: {e}; another app holds it (the Swift Neko, while it runs)")),
        }
    }

    /// The daemon's summon combo: registers it, or moves the registration. A refusal keeps the old one and is logged.
    pub fn set_combo(&mut self, combo: HotkeyCombo) {
        self.combo = Some(combo);
        self.register_summon();
    }

    fn register_summon(&mut self) {
        let (true, Some(main), Some(combo)) = (self.plan.summon_shortcut, self.main.clone(), self.combo.clone()) else { return };
        let words = accelerator(&combo);
        if self.summon.as_ref().is_some_and(|(have, _)| *have == words) {
            return;
        }
        let shortcut: Shortcut = match words.parse() {
            Ok(s) => s,
            Err(e) => return self.fail(format!("the summon shortcut {words} is not valid here: {e}")),
        };
        match main.register_shortcut(shortcut, |app: &mut App, tx: &mut Tx| {
            app.summon_launcher();
            tx.instant();
        }) {
            Ok(id) => {
                eprintln!("neko-rune: summon shortcut {words} registered");
                if let Some((_, old)) = self.summon.replace((words, id)) {
                    let _ = main.unregister_shortcut(old);
                }
            }
            Err(e) => self.fail(format!("the summon shortcut {words} was not registered: {e}; another app holds it (the Swift Neko, while it runs), so the old one stays")),
        }
    }

    /// Does what the tracker decided: shows the pill saying this, or hides it.
    fn apply(&self, step: Step) {
        let Some(pill) = &self.pill else { return };
        match step {
            Step::Nothing => {}
            Step::Show(state) => {
                pill.update(move |p, _| p.state = Some(state));
                let (bottom, right) = pill_margins(insets(false).unwrap_or_default());
                pill.place(ScreenEdge::Bottom, bottom);
                pill.set_align(Align::End, right);
                pill.show();
            }
            Step::Hide => {
                pill.hide();
                pill.update(|p, _| {
                    p.state = None;
                    p.expanded = false;
                });
            }
        }
    }
}

impl App {
    /// Once a poll: feeds the pill's tracker with the workbench and the clock, and asks to be woken when an outcome's time is up. Does
    /// nothing unless the overlays are on.
    pub fn tick_presence(&mut self, tx: &mut Tx) {
        if !self.overlays.plan.pill {
            return;
        }
        let now = (self.clock)();
        // The linger timer and a changed workbench; reading the workbench at most four times a second.
        let mut step = self.overlays.presence.tick(now);
        let due = self.overlays.last_look + LOOK_EVERY_MS;
        if now >= due || step != Step::Nothing {
            if let Some(summary) = &self.summary {
                self.overlays.last_look = now;
                let seen = self.overlays.presence.observe(Facts::of(summary), now);
                if seen != Step::Nothing {
                    step = seen;
                }
            }
        } else {
            // Too soon to look again: look when it is time, so a change that arrived in between is not left waiting for the next poll.
            tx.wake_after(Duration::from_millis((due - now) as u64));
        }
        self.overlays.apply(step);
        if let Some(at) = self.overlays.presence.hide_at() {
            tx.wake_after(Duration::from_millis((at - now).clamp(50, presence::LINGER_MS) as u64));
        }
    }

    /// Settings turned the pill on or off.
    pub fn set_presence(&mut self, on: bool) {
        let step = self.overlays.presence.set_enabled(on);
        self.overlays.apply(step);
        // Only the real window keeps it (a headless run or a test must not write the person's saved state).
        if self.persist_layout {
            crate::app::write_presence(on);
        }
    }

    /// The summon shortcut: shows the launcher on the screen under the pointer with a fresh copy of the palette's rows, or hides it.
    pub fn summon_launcher(&mut self) {
        let Some(launcher) = self.overlays.launcher.clone() else { return };
        let Some(token) = self.overlays.presentation.toggle() else {
            launcher.hide();
            return;
        };
        let items = self.palette_all();
        let theme = *self.overlays.theme.borrow();
        launcher.update(move |l, _| l.present(items, theme, token));
        let top = insets(true).map_or(launcher::top_margin(0.0, 900.0), |i| launcher::top_margin(i.top, i.visible_height));
        launcher.place(DisplayTarget::UnderPointer, OverlayPlacement::Edge { edge: ScreenEdge::Top, margin: top, align: Align::Center, inset: 0.0 });
        launcher.open();
    }

    /// ⌘⇧Esc from anywhere (`StopAllWork`): the same confirmation as the window's own (`ask_stop_all`), with the window brought forward so
    /// it can be answered. Nothing running: nothing opens (Swift beeps).
    ///
    /// Swift's `StopAllWork.run` stops at once with no question; the web asks (`stopAllWork` in `shell/actions.ts`), and this client may
    /// build a destructive request only inside a `confirm(` dialog (`tests/allow_list.rs`), so it asks, like the web.
    pub fn stop_all_key(&mut self) {
        let (tasks, replying) = self.running_agents();
        if tasks == 0 && !replying {
            return;
        }
        self.ask_stop_all();
        if let Some(main) = &self.overlays.main {
            main.open();
        }
    }

    /// Remembers the theme this frame painted, for the launcher.
    pub fn remember_theme(&self, t: &crate::theme::Neko) {
        if self.overlays.plan.launcher {
            *self.overlays.theme.borrow_mut() = Some((t.dark, t.hex));
        }
    }
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
        assert!(!gate(&args(&["neko-rune", "nolink"]), Some("0")));
        assert!(!gate(&args(&[]), Some("")));
        assert!(gate(&args(&["neko-rune", "--overlays"]), None));
        assert!(gate(&args(&[]), Some("1")));
    }

    #[test]
    fn off_makes_nothing() {
        assert_eq!(plan(false), Plan::default());
        assert!(!plan(false).any());
        assert!(plan(true).pill && plan(true).launcher && plan(true).summon_shortcut && plan(true).stop_shortcut);
    }

    #[test]
    fn install_with_the_gate_off_creates_no_overlay_and_no_window() {
        let mut app = rune::App::new(rune::AppSpec::new("t", (100.0, 100.0), App::headless, crate::view::view));
        let installed = install(&mut app, plan(false));
        assert!(installed.pill.is_none() && installed.launcher.is_none(), "no overlay, no launcher window");
        assert_eq!(installed.plan, Plan::default());
        let mut o = Overlays::default();
        o.attach(installed, app.handle());
        assert!(o.stop.is_none() && o.summon.is_none(), "no shortcut was registered");
        o.set_combo(HotkeyCombo::new(vec![Modifier::Alt], "Space".to_owned()));
        assert!(o.summon.is_none() && o.failures.is_empty(), "the summon combo is remembered and never registered");
    }

    #[test]
    fn install_with_the_gate_on_makes_the_overlay_and_the_launcher_and_a_refused_shortcut_is_logged_not_fatal() {
        let mut app = rune::App::new(rune::AppSpec::new("t", (100.0, 100.0), App::headless, crate::view::view));
        let installed = install(&mut app, plan(true));
        assert!(installed.pill.is_some() && installed.launcher.is_some());
        let mut o = Overlays::default();
        // Nothing is running here, so the host refuses: this is the failure path of a held shortcut.
        o.attach(installed, app.handle());
        o.set_combo(HotkeyCombo::new(vec![Modifier::Alt], "Space".to_owned()));
        assert!(o.stop.is_none() && o.summon.is_none());
        assert_eq!(o.failures.len(), 2, "{:?}", o.failures);
        assert!(o.failures[0].contains("⌘⇧Esc") && o.failures[1].contains("summon"));
    }

    #[test]
    fn stashed_overlays_attach_only_when_the_duties_say_so_and_only_once() {
        let mut app = rune::App::new(rune::AppSpec::new("t", (100.0, 100.0), App::headless, crate::view::view));
        let installed = install(&mut app, plan(true));
        let mut o = Overlays::default();
        assert!(!o.attach_pending(), "nothing stashed yet");
        o.stash(installed, app.handle());
        assert!(o.pill.is_none() && !o.plan.any(), "stashed is not attached: no shortcut, no pill, no settings row (the Swift app may hold them)");
        assert!(o.attach_pending());
        assert!(o.plan.any() && o.pill.is_some());
        assert!(!o.attach_pending(), "attached once");
    }

    #[test]
    fn detached_overlays_hold_nothing_and_attach_again_when_handed_back() {
        let mut app = rune::App::new(rune::AppSpec::new("t", (100.0, 100.0), App::headless, crate::view::view));
        let installed = install(&mut app, plan(true));
        let mut o = Overlays::default();
        assert!(!o.detach(), "nothing attached: nothing to hand back");
        o.attach(installed, app.handle());
        o.set_combo(HotkeyCombo::new(vec![Modifier::Alt], "Space".to_owned()));
        assert!(o.plan.any() && o.pill.is_some() && o.launcher.is_some());
        assert!(o.detach());
        assert!(!o.plan.any() && o.pill.is_none() && o.launcher.is_none(), "no pill, no launcher, no gate");
        assert!(o.stop.is_none() && o.summon.is_none(), "no shortcut is held");
        assert!(o.combo.is_some(), "the daemon's combo is remembered for the way back");
        assert!(!o.presentation.shown());
        assert!(!o.detach(), "handed back once");
        assert!(o.attach_pending(), "the Swift app quit: the same overlays come back");
        assert!(o.plan.any() && o.pill.is_some() && o.launcher.is_some());
    }

    #[test]
    fn the_daemons_combo_parses_as_a_rune_shortcut() {
        for (mods, key, want) in [
            (vec![Modifier::Alt], "Space", "alt+space"),
            (vec![Modifier::Cmd, Modifier::Shift], "KeyK", "shift+super+k"),
            (vec![Modifier::Ctrl], "Digit1", "ctrl+1"),
            (vec![Modifier::Cmd], "Backquote", "super+backquote"),
            (vec![Modifier::Alt], "ArrowUp", "alt+up"),
        ] {
            let combo = HotkeyCombo::new(mods, key.to_owned());
            let parsed: Shortcut = accelerator(&combo).parse().unwrap_or_else(|e| panic!("{combo:?}: {e}"));
            assert_eq!(parsed.to_string(), want);
        }
        assert_eq!(STOP_ACCELERATOR.parse::<Shortcut>().unwrap().to_string(), "shift+super+escape");
    }

    #[test]
    fn the_pill_clears_the_dock_and_keeps_swifts_twenty_points() {
        assert_eq!(pill_margins(Insets::default()), (4.0, 4.0), "20 from the corner, less the 16 of clear window around the capsule");
        assert_eq!(pill_margins(Insets { bottom: 70.0, right: 0.0, ..Insets::default() }), (74.0, 4.0));
    }
}
