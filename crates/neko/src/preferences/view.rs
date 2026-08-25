//! The Preferences window: GPUI, and every piece of real I/O. The pure state
//! it drives is [`super::state`].
//!
//! **Chrome matches onboarding's exactly** — frameless inset title bar,
//! native traffic lights repositioned into it, `WindowKind::Normal` so the
//! window appears in the Dock and can become key. Becoming key is not
//! cosmetic here: the summon panel is a *non-activating* popup, which is
//! precisely why it was the wrong surface for a text field you type a path
//! into and a control that records raw key presses.
//!
//! **The window owns no settings of its own.** Every value on screen is read
//! from the daemon through the same scoped `Request::Search` the panel used,
//! and every change is a `Request::Activate`. If a setting is wrong here, the
//! bug is in `neko_core::preferences`, not in this file.

use std::rc::Rc;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, KeyDownEvent, ParentElement, Render, SharedString,
    Styled, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, actions, div, point, prelude::*, px,
};
use neko_client::NekoClient;
use neko_protocol::{HotkeyCombo, Request, Response};

use super::SharedPreferencesSlot;
use super::state::{Recording, Tab, candidate_from_press};
use crate::components::keycap;
use crate::hotkey_client::SharedRebinder;
use crate::text_field::TextField;
use crate::theme;

actions!(preferences, [Submit]);

/// Fixed, and deliberately not resizable: every control here is a row in a
/// two-column grid, so extra width buys nothing and extra height buys empty
/// space. Same reasoning as onboarding's own fixed window.
const WINDOW_WIDTH_PX: f32 = 720.0;
const WINDOW_HEIGHT_PX: f32 = 520.0;
/// The right-aligned label column, Raycast's own settings layout: labels end
/// at a common right edge so the controls all start at one left edge.
const LABEL_COLUMN_WIDTH_PX: f32 = 168.0;

pub fn open_window(
    cx: &mut App,
    client: NekoClient,
    rebinder: SharedRebinder,
    slot: SharedPreferencesSlot,
) {
    // Already open: focus it rather than opening a second. A settings window
    // is singular by nature, and two of them could disagree on screen about
    // what a setting currently is.
    if let Some(existing) = *slot.borrow() {
        // `cx.activate` as well as `activate_window`, for the same reason the
        // fresh-open path below needs it — see that comment.
        cx.activate(true);
        let _ = existing.update(cx, |_root, window, _cx| window.activate_window());
        return;
    }

    let window_size = gpui::size(px(WINDOW_WIDTH_PX), px(WINDOW_HEIGHT_PX));
    let bounds = centered_bounds(cx, window_size);
    let window_slot = slot.clone();
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.), px(14.))),
                }),
                kind: WindowKind::Normal,
                is_movable: true,
                is_resizable: false,
                is_minimizable: true,
                focus: true,
                show: true,
                // Opaque, unlike the summon panel: this window sits still and
                // is read for minutes at a time, so legibility beats the
                // vibrancy that makes a transient overlay feel light.
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            move |_window, cx| cx.new(|cx| PreferencesRoot::new(client, rebinder, window_slot, cx)),
        )
        .expect("failed to open the preferences window");

    *slot.borrow_mut() = Some(window);

    // **This window is opened from a non-activating panel, so it does not
    // become key on its own — and a window that is not key receives no key
    // events at all.** That is precisely the shape of the first bug here:
    // the Record Hotkey button responded to clicks (AppKit delivers those to
    // a non-key window) while every key press went nowhere, so the control
    // sat on "Listening…" forever. The summon panel is
    // `NSNonactivatingPanelMask` by design — clicking it must not steal
    // activation from whatever you were working in — which means opening a
    // real window from it has to ask for activation explicitly.
    //
    // Two calls, and both are needed: `cx.activate` makes *neko* the
    // frontmost application, `activate_window` makes *this* window the key
    // one within it. `window.focus` below is a third, separate thing — GPUI's
    // own internal focus, which routes actions and draws the caret but cannot
    // pull real OS keystrokes into a window the OS does not consider key.
    // **An evidence run never becomes key.** The ring exists to be
    // photographed, and this repo's standing rule is that a throwaway
    // window must never be able to take the captain's real keystrokes
    // (`AGENTS.md`, "Standing safety rule"). The consequence is stated
    // rather than hidden: the readback below correctly reports `false` on
    // such a run, and no key press would reach the window if one were made.
    let evidence_focus = crate::evidence::preferences_focus();
    if evidence_focus.is_none() {
        cx.activate(true);
    }
    let _ = window.update(cx, |root, window, cx| {
        match evidence_focus {
            Some(index) => {
                root.focused = index;
                let _ = crate::material::order_front_regardless(window);
                match crate::material::window_number(window) {
                    Ok(number) => eprintln!(
                        "neko: preferences window number {number} \u{2014} focus ring on control {index} ({:?})",
                        root.focused_control()
                    ),
                    Err(e) => eprintln!("neko: preferences window number unavailable: {e}"),
                }
            }
            None => window.activate_window(),
        }
        window.focus(&root.focus_handle(cx), cx);
        root.reload(cx);
        // Verified, not trusted — the same discipline `material::verify_installed`
        // and `verify_shadow_disabled` follow for every other native window
        // property this app depends on. A `false` here means keyboard input
        // cannot reach this window, which is not otherwise visible until
        // somebody tries to type.
        match crate::material::is_key_window(window) {
            Ok(true) => eprintln!("neko: preferences window is key (keyboard input will reach it)"),
            Ok(false) => eprintln!(
                "neko: preferences window is NOT key — the hotkey recorder cannot receive key presses"
            ),
            Err(e) => eprintln!("neko: could not read preferences window key state: {e}"),
        }
    });
}

fn centered_bounds(cx: &App, window_size: gpui::Size<gpui::Pixels>) -> gpui::Bounds<gpui::Pixels> {
    let display_size = cx
        .primary_display()
        .map(|display| display.bounds().size)
        .unwrap_or_else(|| gpui::size(px(1440.), px(900.)));
    let origin = point(
        (display_size.width - window_size.width) / 2.,
        (display_size.height - window_size.height) / 2.,
    );
    gpui::Bounds::new(origin, window_size)
}


/// One keyboard stop in the Preferences window.
///
/// The window's controls had **no keyboard path at all**: `on_key_down`
/// returned early unless the hotkey recorder was listening, so the four
/// tabs, both toggles and every folder's Remove answered only the mouse —
/// in an app whose panel is otherwise keyboard-first.
///
/// The search-folder text field is deliberately **not** a stop: it already
/// has a working path (type, then Enter to add), it holds gpui's own focus,
/// and putting it in this ring would mean intercepting the keys it needs.
#[derive(Debug, Clone, PartialEq)]
enum Control {
    Tab(Tab),
    /// "Click to record a new combination."
    RecordHotkey,
    /// A switch, named by the `neko_core::preferences` row it drives.
    Toggle(&'static str),
    RemoveFolder(String),
}


/// The keyboard's position, drawn.
///
/// A 2px ring in `text_primary` rather than a tinted fill: several of these
/// controls already use fill to mean *selected* (the current tab) or *on*
/// (a switch), and a second fill on top of either is unreadable. Drawn as
/// an outline outside the control's own bounds so it never resizes what it
/// surrounds — a ring that reflows the row it is on reads as a glitch.
fn focus_ring<E: Styled>(element: E, focused: bool) -> E {
    if focused {
        element
            .border_2()
            .border_color(theme::active().text_primary)
    } else {
        element.border_2().border_color(gpui::transparent_black())
    }
}

pub struct PreferencesRoot {
    client: NekoClient,
    rebinder: SharedRebinder,
    slot: SharedPreferencesSlot,
    focus_handle: FocusHandle,
    tab: Tab,
    /// Which control the keyboard is on, as an index into the tab's own
    /// ordered control list ([`PreferencesRoot::controls`]).
    ///
    /// A hand-rolled focus ring rather than gpui's `tab_stop`/`focus_next`,
    /// for the same reason `panel::Root::selected` is: it is the model the
    /// rest of this app already uses, it survives the control list changing
    /// under it (adding a search folder does exactly that), and it is
    /// testable without a live window — which matters here, because a real
    /// keypress into this window cannot be synthesised under this repo's
    /// own rules.
    focused: usize,
    /// The summon combination as the daemon reports it. A `String`, not a
    /// `HotkeyCombo`, because it arrives as the already-rendered accessory of
    /// the Summon Hotkey row — this window does not re-derive a display form
    /// that `neko_protocol::HotkeyCombo::display` already owns.
    hotkey_label: String,
    recording: Recording,
    launch_at_login: bool,
    agents_enabled: bool,
    agents_include_idle: bool,
    /// What the agent backend can currently see, as
    /// `(backend label, running, idle)`. Shown rather than merely
    /// "configured": a source of truth you cannot see is one you cannot
    /// debug when the list looks wrong.
    agent_census: Option<(String, usize, usize)>,
    folders: Vec<String>,
    folder_input: Entity<TextField>,
    /// The last failed change, shown next to the control that failed rather
    /// than as a transient toast — a settings window has room to keep an
    /// error on screen until it stops being true.
    error: Option<String>,
}

impl PreferencesRoot {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn recording_state(&self) -> &Recording {
        &self.recording
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn begin_recording_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_recording(window, cx);
    }

    pub(crate) fn new(
        client: NekoClient,
        rebinder: SharedRebinder,
        slot: SharedPreferencesSlot,
        cx: &mut Context<Self>,
    ) -> Self {
        let folder_input = TextField::new(cx);
        folder_input.update(cx, |field, cx| {
            field.set_placeholder("Add a folder, e.g. ~/Projects", cx)
        });
        Self {
            client,
            rebinder,
            slot,
            focus_handle: cx.focus_handle(),
            tab: Tab::General,
            focused: 0,
            hotkey_label: String::new(),
            recording: Recording::default(),
            launch_at_login: false,
            agents_enabled: true,
            agents_include_idle: false,
            agent_census: None,
            folders: Vec::new(),
            folder_input,
            error: None,
        }
    }

    /// Re-reads every value from the daemon. Called on open and after every
    /// change, rather than mutating local state optimistically: the daemon is
    /// the only thing that knows whether a change actually took (installing a
    /// LaunchAgent can genuinely fail), and a settings window that shows a
    /// value it merely *hopes* is true is the exact defect
    /// `neko_core::preferences::set_launch_at_login` is written to avoid.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let settings = client
                .request(Request::Search {
                    query: String::new(),
                    limit: 50,
                    provider: Some("preference".to_string()),
                })
                .await;
            let folders = client
                .request(Request::Search {
                    query: String::new(),
                    limit: 200,
                    provider: Some("folder-scope".to_string()),
                })
                .await;
            // Scoped, so it is unaffected by the root-list empty-query guard
            // and by the "include idle" setting's effect on *ranking* — this
            // is a census of what the backend can see, not a search.
            let agents = client
                .request(Request::Search {
                    query: String::new(),
                    limit: 200,
                    provider: Some("agent".to_string()),
                })
                .await;
            let _ = this.update(cx, |root, cx| {
                if let Ok(Response::SearchResults { items, .. }) = settings {
                    for item in items {
                        match item.id.as_str() {
                            neko_core_ids::HOTKEY => {
                                root.hotkey_label = item.accessory.unwrap_or_default()
                            }
                            neko_core_ids::LAUNCH_AT_LOGIN => {
                                root.launch_at_login = item.accessory.as_deref() == Some("On")
                            }
                            neko_core_ids::AGENTS_ENABLED => {
                                root.agents_enabled = item.accessory.as_deref() == Some("On")
                            }
                            neko_core_ids::AGENTS_INCLUDE_IDLE => {
                                root.agents_include_idle = item.accessory.as_deref() == Some("On")
                            }
                            _ => {}
                        }
                    }
                }
                if let Ok(Response::SearchResults { items, .. }) = folders {
                    root.folders = items.into_iter().map(|item| item.id).collect();
                }
                if let Ok(Response::SearchResults { items, .. }) = agents {
                    let running = items.iter().filter(|i| i.badge.as_deref() == Some("LIVE")).count();
                    root.agent_census = Some(("Paseo".to_string(), running, items.len() - running));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Sends one change and re-reads. `Response::Error`'s message is shown
    /// verbatim: the daemon already phrases these for a person
    /// ("not a folder: /nope"), and rewording them here would only lose
    /// detail.
    fn apply(&mut self, request: Request, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let outcome = client.request(request).await;
            let error = match outcome {
                Ok(Response::Error { message }) => Some(message),
                Ok(_) => None,
                Err(_) => Some(crate::panel::DAEMON_UNREACHABLE.to_string()),
            };
            let _ = this.update(cx, |root, cx| {
                root.error = error;
                root.reload(cx);
                cx.notify();
            });
        })
        .detach();
    }


    /// Every keyboard stop on the current tab, in reading order: the tab bar
    /// first, then the tab's own controls top to bottom.
    ///
    /// Rebuilt per press rather than cached, because it genuinely changes
    /// under the focus index — adding or removing a search folder adds or
    /// removes a stop — and a stale list would put the ring on a control
    /// that is no longer there.
    fn controls(&self) -> Vec<Control> {
        let mut controls: Vec<Control> = Tab::ALL.iter().map(|t| Control::Tab(*t)).collect();
        match self.tab {
            Tab::General => {
                controls.push(Control::RecordHotkey);
                controls.push(Control::Toggle(neko_core_ids::LAUNCH_AT_LOGIN));
            }
            Tab::Search => {
                controls.extend(self.folders.iter().cloned().map(Control::RemoveFolder));
            }
            Tab::Agents => {
                controls.push(Control::Toggle(neko_core_ids::AGENTS_ENABLED));
                controls.push(Control::Toggle(neko_core_ids::AGENTS_INCLUDE_IDLE));
            }
            Tab::About => {}
        }
        controls
    }

    fn focused_control(&self) -> Option<Control> {
        self.controls().get(self.focused).cloned()
    }

    /// Move the focus ring onto `control`.
    ///
    /// **Called from every `on_click`, not just from Tab.** The ring is this
    /// window's only statement of where the keyboard is, and a click that
    /// activates one control while the ring sits on another leaves the next
    /// Enter or Space acting somewhere the eye is not — which is worse than
    /// no ring, because it is a confident wrong answer. Pointer and keyboard
    /// share one cursor; they do not each keep their own.
    ///
    /// A control that has vanished from the list (the Remove button of a
    /// folder that was just removed) leaves the ring where it was rather than
    /// clearing it — `focus_next` clamps, so the next Tab lands somewhere real.
    fn focus_control(&mut self, control: &Control) {
        if let Some(index) = self.controls().iter().position(|c| c == control) {
            self.focused = index;
        }
    }

    fn is_focused(&self, control: &Control) -> bool {
        self.focused_control().as_ref() == Some(control)
    }

    /// Tab and Shift-Tab, wrapping. Wrapping rather than stopping at the
    /// ends because this is a small closed ring, not a document.
    fn move_focus(&mut self, forward: bool) {
        let n = self.controls().len();
        if n == 0 {
            return;
        }
        self.focused = if forward { (self.focused + 1) % n } else { (self.focused + n - 1) % n };
    }

    /// Switching tabs changes the control list under the focus index, so it
    /// goes back to the tab bar — landing on the tab just opened, which is
    /// where the eye already is.
    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        // Recording is a live OS listener; leaving the tab it belongs to
        // must stop it, or key presses aimed at another tab would be
        // captured as hotkeys.
        self.recording = Recording::Idle;
        self.error = None;
        self.focused = Tab::ALL.iter().position(|t| *t == tab).unwrap_or(0);
        cx.notify();
    }

    fn toggle_row(&mut self, row_id: &str, cx: &mut Context<Self>) {
        self.apply(
            Request::Activate {
                kind: "preference".to_string(),
                id: row_id.to_string(),
                action: None,
                query: String::new(),
            },
            cx,
        );
    }

    /// Enter and Space on the focused control — the same work its own
    /// `on_click` does, routed through the same methods so the two can
    /// never drift apart.
    fn activate_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.focused_control() {
            Some(Control::Tab(tab)) => self.select_tab(tab, cx),
            Some(Control::RecordHotkey) => self.start_recording(window, cx),
            Some(Control::Toggle(row)) => self.toggle_row(row, cx),
            Some(Control::RemoveFolder(path)) => self.remove_folder(path, cx),
            None => {}
        }
    }


    /// Keyboard navigation for everything that is not the hotkey recorder.
    ///
    /// Runs in the capture phase alongside the recorder (see `render`), and
    /// claims only the keys it handles — anything else, including every
    /// character, falls through to the search-folder text field, which is
    /// why typing a path still works while this is installed.
    ///
    /// Left and Right move between tabs only while a tab is focused, which
    /// is the ARIA tabs pattern; elsewhere they belong to the text field's
    /// own cursor.
    fn on_navigation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let shift = keystroke.modifiers.shift;
        match keystroke.key.as_str() {
            "tab" => self.move_focus(!shift),
            "left" | "right" => match self.focused_control() {
                Some(Control::Tab(tab)) => {
                    self.select_tab(tab.step(keystroke.key == "right"), cx);
                    cx.stop_propagation();
                    return;
                }
                _ => return,
            },
            "enter" | "space" => {
                // **The guard this replaces was dead, and it broke the one
                // thing the Search tab is for.** It read
                // `if self.focused_control().is_none() { return; }`, meaning
                // "Enter belongs to the folder field when the ring is on
                // nothing" — but `controls()` always begins with the four
                // tabs, so the ring is never on nothing. Arriving at the
                // Search tab always parks it on `Tab(Search)`, so typing a
                // path and pressing Enter re-selected the already-selected
                // tab and `stop_propagation`'d in the capture phase, and the
                // keystroke never reached the field. "Press ↵ to add" could
                // not add, ever, and on a fresh install there are no folder
                // rows to Tab onto to escape it.
                //
                // The real question is not "is the ring on something" but
                // "is the ring on something *other than a tab*" — a tab is
                // already switched by Left/Right, so Enter on one does
                // nothing a person wanted, and the field is the only other
                // thing on that screen that Enter means anything to.
                if matches!(self.focused_control(), None | Some(Control::Tab(_))) {
                    return;
                }
                self.activate_focused(window, cx);
                cx.stop_propagation();
                return;
            }
            // The window has no menu bar, so this is the only key that
            // closes it. Its own traffic light still works.
            "escape" => {
                window.remove_window();
                return;
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn add_folder(&mut self, cx: &mut Context<Self>) {
        let typed = self.folder_input.read(cx).content().trim().to_string();
        if typed.is_empty() {
            return;
        }
        self.folder_input.update(cx, |field, cx| field.set_content("", cx));
        self.apply(
            Request::Activate {
                kind: "folder-scope".to_string(),
                id: typed,
                action: Some("add".to_string()),
                // This window has no search field; `query` only carries one
                // for a row whose action takes what was typed as an argument
                // (`new_agent`), and every provider else ignores it.
                query: String::new(),
            },
            cx,
        );
    }

    fn remove_folder(&mut self, path: String, cx: &mut Context<Self>) {
        self.apply(
            Request::Activate { kind: "folder-scope".to_string(), id: path, action: None, query: String::new() },
            cx,
        );
    }

    /// Arms the recorder, re-taking GPUI focus as it does.
    ///
    /// The focus call is belt-and-braces rather than the fix it was first
    /// believed to be: a live readback showed focus was already on the root
    /// when this ran (`focused before=true`). It stays because a key handler
    /// does require its div to be focused, and nothing should depend on a
    /// future control not moving focus when it is clicked. **What actually
    /// broke the recorder was dispatch phase, not focus** — see the
    /// `capture_key_down` call in `render`.
    fn start_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.recording = Recording::Listening;
        self.error = None;
        cx.notify();
    }

    /// Every key press while listening is a hotkey candidate. Escape stops
    /// listening rather than being recorded — it is refused as a hotkey
    /// anyway, and "get me out of this control" is what a person means by it.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.recording != Recording::Listening && !matches!(self.recording, Recording::Rejected(_)) {
            self.on_navigation_key(event, window, cx);
            return;
        }
        let keystroke = &event.keystroke;
        if keystroke.key == "escape" {
            self.recording = Recording::Idle;
            cx.notify();
            return;
        }
        let modifiers = &keystroke.modifiers;
        let Some(candidate) = candidate_from_press(
            &keystroke.key,
            modifiers.platform,
            modifiers.alt,
            modifiers.control,
            modifiers.shift,
        ) else {
            return;
        };
        cx.stop_propagation();
        match candidate {
            Err(reason) => {
                self.recording = Recording::Rejected(reason);
                cx.notify();
            }
            Ok(combo) => self.commit_hotkey(combo, cx),
        }
    }

    /// **Register live first, persist only on success** — the same order
    /// onboarding's step 09 uses, and the reason a combination the OS refuses
    /// never reaches storage and never costs the working hotkey.
    fn commit_hotkey(&mut self, candidate: HotkeyCombo, cx: &mut Context<Self>) {
        let Some(rebinder) = self.rebinder.borrow().clone() else {
            self.recording =
                Recording::Rejected("neko is still starting up — try again in a moment.".to_string());
            cx.notify();
            return;
        };
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            // The daemon's heuristic supplies a *reason* a live failure
            // cannot: a Carbon registration failure is an opaque status code.
            let heuristic = match client
                .request(Request::CheckHotkeyConflict { candidate: candidate.clone() })
                .await
            {
                Ok(Response::HotkeyConflict { reason }) => reason,
                _ => None,
            };
            let outcome = rebinder.rebind(candidate.clone());
            let _ = this.update(cx, |root, cx| {
                match outcome {
                    Ok(()) => {
                        root.recording = Recording::Idle;
                        let client = root.client.clone();
                        let combo = candidate.clone();
                        cx.spawn(async move |this, cx| {
                            let _ = client.request(Request::CommitHotkey { candidate: combo }).await;
                            let _ = this.update(cx, |root, cx| root.reload(cx));
                        })
                        .detach();
                    }
                    Err(e) => root.recording = Recording::Rejected(heuristic.unwrap_or(e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn on_submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        if self.tab == Tab::Search {
            self.add_folder(cx);
        }
    }
}

/// The row ids `neko_core::preferences` defines. Named here rather than
/// spelled inline so a rename on the daemon side is a compile error in one
/// place instead of a silently dead branch in three.
mod neko_core_ids {
    pub const HOTKEY: &str = "hotkey";
    pub const LAUNCH_AT_LOGIN: &str = "launch-at-login";
    pub const AGENTS_ENABLED: &str = "agents-enabled";
    pub const AGENTS_INCLUDE_IDLE: &str = "agents-include-idle";
}

impl Focusable for PreferencesRoot {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PreferencesRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A window that has been closed leaves its slot behind; clearing it
        // here would need a close observer. `open_window` re-checks liveness
        // by updating the handle, which fails for a closed window, so a stale
        // slot degrades to "open a fresh one" rather than to a dead reference.
        let _ = &self.slot;
        div()
            .key_context("Preferences")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_submit))
            // **Capture phase, not bubble.** GPUI matches key bindings and
            // dispatches actions *between* the capture and bubble phases, so
            // a bubble-phase handler only ever sees keystrokes nothing else
            // wanted. That is fine for Escape and wrong for everything a
            // hotkey is made of: measured live, an unmodified Escape reached
            // a bubble handler while ⌃⇧K never did. A recorder has to see the
            // key *before* the rest of the app gets an opinion about it —
            // the same reason the earlier in-panel version of this control
            // used `capture_key_down` too.
            .capture_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::active().surface_panel)
            .text_color(theme::active().text_primary)
            .child(self.render_tab_bar(cx))
            .child(self.render_body(cx))
    }
}

impl PreferencesRoot {
    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut bar = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.))
            // Clears the native traffic lights, which sit at 14pt from the
            // top-left inside this same strip — the identical clearance
            // problem `onboarding::view::render_header` already solved once.
            .pt(px(26.))
            .pb(px(10.))
            .px(px(theme::ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_PX))
            .border_b_1()
            .border_color(theme::active().border_hairline);
        for tab in Tab::ALL {
            let tab = *tab;
            let selected = tab == self.tab;
            bar = bar.child(
                div()
                    .id(SharedString::from(tab.title()))
                    .px(px(14.))
                    .py(px(6.))
                    .rounded(px(theme::BTN_RADIUS_PX))
                    .when(selected, |el| el.bg(theme::active().surface_selected))
                    .text_size(px(12.5))
                    .text_color(if selected {
                        theme::active().text_primary
                    } else {
                        theme::active().text_secondary
                    })
                    .cursor_pointer()
                    // A hover tint distinctly weaker than the selected tab's
                    // own fill, so hovering reads as the pointer being
                    // somewhere rather than as a second selection.
                    .when(!selected, |el| {
                        el.hover(|el| el.bg(theme::active().row_icon_socket_bg))
                    })
                    .map(|el| focus_ring(el, self.is_focused(&Control::Tab(tab))))
                    .on_click(cx.listener(move |root, _event, _window, cx| {
                        root.select_tab(tab, cx);
                    }))
                    .child(tab.title()),
            );
        }
        bar
    }

    fn render_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let body = div().flex().flex_col().flex_1().min_h(px(0.)).gap(px(18.)).px(px(28.)).py(px(22.));
        match self.tab {
            Tab::General => body
                .child(self.render_hotkey_row(cx))
                .child(self.render_launch_row(cx))
                .child(self.render_error()),
            Tab::Search => body.child(self.render_folders(cx)).child(self.render_error()),
            Tab::Agents => body
                .child(self.render_agent_source())
                .child(self.render_toggle_row(
                    "Show agents",
                    "List running coding agents in search results.",
                    self.agents_enabled,
                    neko_core_ids::AGENTS_ENABLED,
                    cx,
                ))
                .child(self.render_toggle_row(
                    "Include idle agents",
                    "Match agents that are not currently running. Off by default — idle agents outnumber live ones heavily.",
                    self.agents_include_idle,
                    neko_core_ids::AGENTS_INCLUDE_IDLE,
                    cx,
                ))
                .child(self.render_error()),
            Tab::About => body.child(self.render_about()),
        }
    }

    /// One settings row: a right-aligned label, then its control. The label
    /// column is a fixed width so every control on a tab starts at the same
    /// left edge.
    fn labelled(label: &'static str, control: impl IntoElement) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .gap(px(16.))
            .child(
                div()
                    .w(px(LABEL_COLUMN_WIDTH_PX))
                    .flex_shrink_0()
                    .pt(px(6.))
                    .text_size(px(12.5))
                    .text_color(theme::active().text_secondary)
                    .text_right()
                    .child(label),
            )
            .child(div().flex().flex_col().flex_1().min_w(px(0.)).gap(px(6.)).child(control))
    }

    fn render_hotkey_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let listening = matches!(self.recording, Recording::Listening);
        let rejected = matches!(self.recording, Recording::Rejected(_));
        // While listening there is no combination to draw yet, so the caps
        // are replaced by the instruction — the control is the same size and
        // in the same place either way, so arming it does not make the row
        // jump.
        let body = if listening {
            div()
                .px(px(12.))
                .py(px(8.))
                .text_size(px(13.))
                .text_color(theme::active().text_primary)
                .child("Listening… press a combination")
                .into_any_element()
        } else {
            keycap::keycaps(&self.hotkey_label, rejected).into_any_element()
        };
        let control = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .id("record-hotkey")
                    .self_start()
                    .p(px(4.))
                    .rounded(px(theme::BTN_RADIUS_PX))
                    // Listening is its own state and outranks the focus
                    // ring: while it is live, every key is going here, so
                    // saying where the keyboard *is* would be redundant and
                    // the two borders would fight.
                    .map(|el| {
                        if listening {
                            el.border_2().border_color(theme::active().state_danger_border)
                        } else {
                            focus_ring(el, self.is_focused(&Control::RecordHotkey))
                        }
                    })
                    .cursor_pointer()
                    .when(!listening, |el| {
                        el.hover(|el| el.bg(theme::active().row_icon_socket_bg))
                    })
                    .on_click(cx.listener(|root, _event, window, cx| {
                        root.focus_control(&Control::RecordHotkey);
                        root.start_recording(window, cx)
                    }))
                    .child(body),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(if rejected {
                        theme::active().state_danger
                    } else {
                        theme::active().text_tertiary
                    })
                    .child(SharedString::from(match &self.recording {
                        Recording::Rejected(reason) => reason.clone(),
                        Recording::Listening => {
                            "Include at least one modifier — ⌘, ⌥, ⌃ or ⇧. Escape cancels.".to_string()
                        }
                        Recording::Idle => "Click to record a new combination.".to_string(),
                    })),
            );
        Self::labelled("Summon hotkey", control)
    }

    fn render_launch_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_toggle_row(
            "Launch at login",
            "Starts neko automatically when you log in.",
            self.launch_at_login,
            neko_core_ids::LAUNCH_AT_LOGIN,
            cx,
        )
    }

    /// One labelled switch. A real painted switch rather than a checkbox,
    /// and hand-painted from two divs because this codebase has no control
    /// library and no SVG pipeline — the same approach every glyph in
    /// `panel.rs` uses.
    ///
    /// `row_id` is the `neko_core::preferences` row this drives, so a toggle
    /// is one call and can never disagree with what the daemon thinks it is:
    /// the value comes from `reload`, and flipping it re-reads rather than
    /// assuming.
    fn render_toggle_row(
        &self,
        label: &'static str,
        help: &'static str,
        on: bool,
        row_id: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let track = div()
            .id(SharedString::from(row_id))
            .w(px(40.))
            .h(px(24.))
            .rounded(px(12.))
            .bg(if on { theme::active().state_success } else { theme::active().surface_input })
            .border_1()
            .border_color(if on {
                theme::active().state_success_border
            } else {
                theme::active().border_hairline_strong
            })
            .relative()
            .cursor_pointer()
            // The switch is already a fill, so hover moves the *border*
            // instead — a second fill on top of the first is unreadable, the
            // same reason `focus_ring` is an outline here and not a fill.
            .hover(|el| el.border_color(theme::active().text_secondary))
            .map(|el| focus_ring(el, self.is_focused(&Control::Toggle(row_id))))
            .on_click(cx.listener(move |root, _event, _window, cx| {
                root.focus_control(&Control::Toggle(row_id));
                root.toggle_row(row_id, cx);
            }))
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(if on { 19. } else { 3. }))
                    .w(px(16.))
                    .h(px(16.))
                    .rounded(px(8.))
                    .bg(theme::active().text_primary),
            );
        let control = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(track)
            .child(div().text_size(px(11.5)).text_color(theme::active().text_tertiary).child(help));
        Self::labelled(label, control)
    }

    /// Where agents are read from, and what that source can currently see.
    ///
    /// It is a **statement, not a picker**: there is exactly one backend, and
    /// a dropdown with one entry is a promise the app cannot keep. When a
    /// second backend exists this becomes a real control; until then the
    /// honest thing is to say which one is in use, where its data lives, and
    /// how many agents it can see — the last of which is what tells you
    /// whether an empty list means "nothing running" or "wrong path".
    fn render_agent_source(&self) -> impl IntoElement {
        let (label, running, idle) = self
            .agent_census
            .clone()
            .unwrap_or_else(|| ("Paseo".to_string(), 0, 0));
        let control = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .self_start()
                    .px(px(12.))
                    .py(px(7.))
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .bg(theme::active().surface_input)
                    .border_1()
                    .border_color(theme::active().border_hairline_strong)
                    .text_size(px(13.))
                    .text_color(theme::active().text_primary)
                    .child(SharedString::from(label)),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme::active().text_tertiary)
                    .child(SharedString::from(format!(
                        "{running} running · {idle} idle · read from ~/.paseo/agents"
                    ))),
            );
        Self::labelled("Source", control)
    }

    fn render_folders(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().flex().flex_col().gap(px(4.));
        if self.folders.is_empty() {
            list = list.child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::active().text_tertiary)
                    .child("No folders — file search is off until you add one."),
            );
        }
        for path in &self.folders {
            let path_for_remove = path.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .bg(theme::active().surface_input)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.5))
                            .text_color(theme::active().text_primary)
                            .child(SharedString::from(path.clone())),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("remove-{path}")))
                            .px(px(9.))
                            // **5, not 3.** At 3 this chip measured ~21pt tall
                            // against WCAG 2.5.8's 24pt floor — the only
                            // control in this window under it, and the one
                            // that destroys something. The label stays 11pt;
                            // the padding does the work, so nothing reflows
                            // around it.
                            .py(px(5.))
                            .rounded(px(theme::CHIP_RADIUS_PX))
                            .border_1()
                            .border_color(theme::active().state_danger_border)
                            .text_size(px(11.))
                            .text_color(theme::active().state_danger)
                            .cursor_pointer()
                            .hover(|el| el.bg(theme::active().banner_danger_bg))
                            .map(|el| {
                                focus_ring(el, self.is_focused(&Control::RemoveFolder(path.clone())))
                            })
                            .on_click(cx.listener(move |root, _event, _window, cx| {
                                root.focus_control(&Control::RemoveFolder(
                                    path_for_remove.clone(),
                                ));
                                root.remove_folder(path_for_remove.clone(), cx)
                            }))
                            .child("Remove"),
                    ),
            );
        }
        let input = div()
            .flex()
            .items_center()
            .h(px(36.))
            .px(px(10.))
            .rounded(px(theme::ROW_RADIUS_PX))
            .bg(theme::active().surface_input)
            .border_1()
            .border_color(theme::active().border_hairline_strong)
            .child(self.folder_input.clone());
        let control = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(list)
            .child(input)
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme::active().text_tertiary)
                    .child("Press ↵ to add. Paths starting with ~ are expanded."),
            );
        Self::labelled("Search folders", control)
    }

    fn render_error(&self) -> impl IntoElement {
        div().when_some(self.error.clone(), |el, message| {
            el.child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(theme::ROW_RADIUS_PX))
                    .bg(theme::active().banner_danger_bg)
                    .border_1()
                    .border_color(theme::active().state_danger_border)
                    .text_size(px(12.))
                    .text_color(theme::active().state_danger)
                    .child(SharedString::from(message)),
            )
        })
    }

    fn render_about(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().text_size(px(16.)).child("neko"))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(theme::active().text_secondary)
                    .child(SharedString::from(format!("Version {}", env!("CARGO_PKG_VERSION")))),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::active().text_tertiary)
                    .max_w(px(420.))
                    .child(
                        "A hotkey-summoned launcher and agent control plane for macOS, written \
                         from scratch in Rust on GPUI. Searching is local; Usage, Ask neko and \
                         the agent features reach the services you are already signed in to.",
                    ),
            )
    }
}

/// Convenience for any caller that has the pieces but not a `panel::Root`.
pub fn opener(
    client: NekoClient,
    rebinder: SharedRebinder,
    slot: SharedPreferencesSlot,
) -> crate::panel::PreferencesOpener {
    Rc::new(move |panel_window: &Window, cx: &mut App| {
        // Order the *panel window* out, not `cx.hide()`. `App::hide` is
        // `[NSApp hide:]` — it hides every window this app owns, which now
        // includes the Preferences window being opened in the same breath.
        // The panel is the only thing that should disappear here.
        let _ = crate::material::order_out(panel_window);
        open_window(cx, client.clone(), rebinder.clone(), slot.clone());
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Keystroke, TestAppContext};

    /// Builds the window's root headlessly. No real window is opened —
    /// GPUI's test platform panics on that — so this proves the recorder's
    /// *logic* given a delivered key press, and deliberately not that a real
    /// key press reaches it. `Window::dispatch_keystroke` enters below
    /// AppKit's responder chain (`AGENTS.md`, "The top line"), so it cannot
    /// prove delivery either way; that half is verified by the live log.
    fn test_root(cx: &mut TestAppContext) -> gpui::WindowHandle<PreferencesRoot> {
        let (client, _events) = NekoClient::connect(std::path::PathBuf::from(format!(
            "/tmp/neko-prefs-view-test-{}.sock",
            std::process::id()
        )));
        let rebinder: SharedRebinder = Rc::new(std::cell::RefCell::new(None));
        let slot: SharedPreferencesSlot = Rc::new(std::cell::RefCell::new(None));
        cx.add_window(|_window, cx| PreferencesRoot::new(client, rebinder, slot, cx))
    }

    #[gpui::test]
    fn a_delivered_key_press_while_listening_becomes_a_candidate(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window.update(cx, |root, window, cx| root.begin_recording_for_test(window, cx)).unwrap();
        window
            .update(cx, |root, window, cx| {
                let event = gpui::KeyDownEvent {
                    keystroke: Keystroke::parse("ctrl-shift-k").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                root.on_key_down(&event, window, cx);
                // No registrar is wired in a test, so the honest outcome is
                // the "not ready" rejection — which still proves the press was
                // parsed into a real candidate rather than dropped.
                assert!(
                    matches!(root.recording_state(), Recording::Rejected(reason) if reason.contains("starting up")),
                    "expected the press to reach the registrar, got {:?}",
                    root.recording_state()
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_bare_modifier_press_leaves_the_control_listening_rather_than_rejecting(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window.update(cx, |root, window, cx| root.begin_recording_for_test(window, cx)).unwrap();
        window
            .update(cx, |root, window, cx| {
                let event = gpui::KeyDownEvent {
                    keystroke: Keystroke::parse("alt").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                root.on_key_down(&event, window, cx);
                assert_eq!(
                    root.recording_state(),
                    &Recording::Listening,
                    "holding a modifier down must not flash an error"
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn escape_stops_listening_without_recording_anything(cx: &mut TestAppContext) {
        let window = test_root(cx);
        window.update(cx, |root, window, cx| root.begin_recording_for_test(window, cx)).unwrap();
        window
            .update(cx, |root, window, cx| {
                let event = gpui::KeyDownEvent {
                    keystroke: Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                root.on_key_down(&event, window, cx);
                assert_eq!(root.recording_state(), &Recording::Idle);
            })
            .unwrap();
    }
}
