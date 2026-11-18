//! P4, the overlays (docs/knowledge/rune-overlays.md): the activity pill (`presence.rs`), wired to the window here. (The launcher panel and
//! the stop-all key follow in the next commit.)
//!
//! **The gate.** Until P5 the Swift app is the default client and owns the summon hotkey, ⌘⇧Esc and its own pill, so all three are OFF here
//! unless this client is started with `--overlays` or `NEKO_RUNE_OVERLAYS=1`. Off, nothing is created and no shortcut is registered
//! ([`plan`] is empty, [`install`] makes nothing). P5 flips the default and retires the Swift ones.
//!
//! Every Rune API the overlays use is named in the knowledge note; the shape is one `OverlaySpec` for the pill, one `launcher_panel()`
//! window for the launcher, and two `register_shortcut` calls (the daemon's summon combo, which follows `GetHotkey`, and ⌘⇧Esc). A shortcut
//! that cannot be registered (the Swift app holds it) is logged to stderr and the client carries on.

use std::time::Duration;

use rune::{Align, AppHandle, DisplayTarget, KeyPolicy, OverlayHandle, OverlayInput, OverlayLevel, OverlayOptions, OverlaySpec, ScreenEdge, Tx};

use crate::app::App;
use crate::presence::{self, Facts, Pill, PresenceState, Step, Tracker};

/// The command-line flag and the environment variable that turn the overlays on.
pub const FLAG: &str = "--overlays";
pub const ENV: &str = "NEKO_RUNE_OVERLAYS";
/// How often the pill reads the workbench (it reads the summary's tasks, so not every frame).
const LOOK_EVERY_MS: i64 = 250;
/// Whether the overlays are on: the flag among the arguments, or the variable set to `1`.
pub fn gate(args: &[String], env: Option<&str>) -> bool {
    args.iter().any(|a| a == FLAG) || env == Some("1")
}

/// What to make. Empty when the gate is off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub pill: bool,
}

pub fn plan(on: bool) -> Plan {
    Plan { pill: on }
}

impl Plan {
    pub fn any(&self) -> bool {
        self.pill
    }
}

/// What [`install`] made: the handles the window keeps. Every field is empty when the plan is.
#[derive(Default)]
pub struct Installed {
    pub plan: Plan,
    pub pill: Option<OverlayHandle<Pill>>,
}

/// Creates what the plan asks for on the app builder (nothing opens until it runs): the pill's overlay.
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
    out
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

/// The overlays as the window holds them: the handles and the pill's tracker.
#[derive(Default)]
pub struct Overlays {
    pub plan: Plan,
    pub pill: Option<OverlayHandle<Pill>>,
    pub main: Option<AppHandle<App>>,
    pub presence: Tracker,
    last_look: i64,
}

impl Overlays {
    /// Nothing made yet; `presence_on` is the pill's saved preference.
    pub fn new(presence_on: bool) -> Overlays {
        Overlays { presence: Tracker::with_enabled(presence_on), ..Overlays::default() }
    }

    /// Takes what [`install`] made.
    pub fn attach(&mut self, installed: Installed, main: AppHandle<App>) {
        self.plan = installed.plan;
        self.pill = installed.pill;
        self.main = Some(main);
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
        assert!(plan(true).pill);
    }

    #[test]
    fn install_with_the_gate_off_creates_no_overlay() {
        let mut app = rune::App::new(rune::AppSpec::new("t", (100.0, 100.0), App::headless, crate::view::view));
        let installed = install(&mut app, plan(false));
        assert!(installed.pill.is_none(), "no overlay");
        assert_eq!(installed.plan, Plan::default());
        let mut o = Overlays::default();
        o.attach(installed, app.handle());
        assert!(o.pill.is_none() && !o.plan.any());
    }

    #[test]
    fn install_with_the_gate_on_makes_the_overlay() {
        let mut app = rune::App::new(rune::AppSpec::new("t", (100.0, 100.0), App::headless, crate::view::view));
        let installed = install(&mut app, plan(true));
        assert!(installed.pill.is_some());
    }

    #[test]
    fn the_pill_clears_the_dock_and_keeps_swifts_twenty_points() {
        assert_eq!(pill_margins(Insets::default()), (4.0, 4.0), "20 from the corner, less the 16 of clear window around the capsule");
        assert_eq!(pill_margins(Insets { bottom: 70.0, right: 0.0, ..Insets::default() }), (74.0, 4.0));
    }
}
