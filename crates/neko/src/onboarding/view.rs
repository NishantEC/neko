//! Native six-step setup lifecycle and real Mac permission/shortcut controls.
//! Rendering and daemon setup forms live in setup.rs. Completion is acknowledged
//! before closing; a native close clears the handle without marking setup done.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, ParentElement, Render,
    SharedString, Styled, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, div, point, prelude::*, px, size,
};
use neko_client::NekoClient;
use neko_protocol::{HotkeyCombo, Modifier as ProtoModifier, Request, Response};

use crate::accessibility::AccessibilityChecker;
use crate::hotkey_client::{HotkeyController, SystemRegistrar};
use crate::theme;

use super::state::{Flow, RecordingState, Step, canonicalize_key_name, validate_candidate};
#[path = "setup.rs"]
mod setup;

pub type SharedHotkeyController = Rc<RefCell<HotkeyController<SystemRegistrar>>>;
pub type SharedAccessibility = Rc<dyn AccessibilityChecker>;
/// Cleared by `OnboardingRoot` itself the moment it closes, so a caller
/// (`main.rs`'s summon loop) can tell "onboarding is still up" from "it
/// finished" without needing its own completion event channel.
pub type SharedOnboardingSlot = Rc<RefCell<Option<WindowHandle<OnboardingRoot>>>>;

pub struct OnboardingRoot {
    setup: setup::Setup,
    flow: Flow,
    client: NekoClient,
    accessibility: SharedAccessibility,
    hotkey: SharedHotkeyController,
    slot: SharedOnboardingSlot,
    focus_handle: FocusHandle,
    poll_generation: u64,
    capture_generation: u64,
    capture_pending: Option<u64>,
}

fn capture_is_current(generation: u64, expected: u64, recording: &RecordingState) -> bool {
    generation == expected && matches!(recording, RecordingState::Recording)
}

fn tab_direction(key: &str, shift: bool, recording: &RecordingState) -> Option<bool> {
    (key == "tab" && !matches!(recording, RecordingState::Recording)).then_some(shift)
}

fn busy_after_capture_cancel(busy: bool, capture_owns_gate: bool) -> bool {
    busy && !capture_owns_gate
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    #[test]
    fn escape_does_not_release_an_inflight_poll_gate() {
        assert!(busy_after_capture_cancel(true, false));
        assert!(!busy_after_capture_cancel(true, true));
        assert!(!busy_after_capture_cancel(false, false));
    }
    #[test]
    fn cancelled_or_replaced_conflict_lookup_cannot_rebind() {
        assert!(capture_is_current(1, 1, &RecordingState::Recording));
        assert!(!capture_is_current(2, 1, &RecordingState::Recording));
        assert!(!capture_is_current(1, 1, &RecordingState::Idle));
        assert!(!capture_is_current(
            1,
            1,
            &RecordingState::Rejected("invalid".into())
        ));
    }
    #[test]
    fn tab_traversal_yields_to_shortcut_capture() {
        assert_eq!(
            tab_direction("tab", false, &RecordingState::Idle),
            Some(false)
        );
        assert_eq!(
            tab_direction("tab", true, &RecordingState::Idle),
            Some(true)
        );
        assert_eq!(tab_direction("tab", true, &RecordingState::Recording), None);
        assert_eq!(tab_direction("enter", false, &RecordingState::Idle), None);
    }
}

/// Opens the onboarding window and records its handle in `slot` — a slot
/// the caller creates up front (typically before it's known whether
/// onboarding needs to run at all) so `App::on_reopen` and the summon
/// loop's hotkey-press handling can check "is onboarding currently up?"
/// without waiting on this async-orchestrated open to complete.
/// `OnboardingRoot` clears `slot` itself the moment it closes.
pub fn open_window(
    cx: &mut App,
    client: NekoClient,
    hotkey: SharedHotkeyController,
    accessibility: SharedAccessibility,
    initial_combo: HotkeyCombo,
    slot: SharedOnboardingSlot,
) {
    let window_size = size(px(1120.), px(720.));
    let bounds = centered_bounds(cx, window_size);

    let window_slot = slot.clone();
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // Frameless-inset chrome, no system title text — the captain
                // asked for comet/waku's approach in place of the standard
                // grey title-bar strip. `render_header` below draws the real
                // chrome; the traffic lights stay native macOS controls,
                // repositioned into that strip via `traffic_light_position`.
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.), px(14.))),
                }),
                kind: WindowKind::Normal,
                is_movable: true,
                is_resizable: false,
                is_minimizable: true,
                focus: !setup::evidence(),
                show: true,
                window_background: crate::material::window_background(),
                ..Default::default()
            },
            move |_window, cx| {
                OnboardingRoot::new(
                    client,
                    hotkey,
                    accessibility,
                    initial_combo,
                    window_slot,
                    cx,
                )
            },
        )
        .expect("failed to open the onboarding window");

    *slot.borrow_mut() = Some(window);
    let _ = window.update(cx, |root, window, cx| {
        let _ = crate::material::install(window);
        if setup::evidence() {
            let _ = crate::material::order_front_regardless(window);
            eprintln!(
                "neko: onboarding step {:?}, window number {:?}, key {:?}",
                root.setup.step,
                crate::material::window_number(window),
                crate::material::is_key_window(window)
            );
            if crate::material::is_key_window(window).unwrap_or(true) {
                let _ = crate::material::order_out(window);
            }
        } else {
            window.focus(&root.focus_handle(cx), cx);
        }
        root.setup_start(cx);
    });
}

fn centered_bounds(cx: &App, window_size: gpui::Size<gpui::Pixels>) -> gpui::Bounds<gpui::Pixels> {
    let Some(display) = cx.primary_display() else {
        return gpui::Bounds {
            origin: point(px(0.), px(0.)),
            size: window_size,
        };
    };
    let display_bounds = display.bounds();
    let x = display_bounds.origin.x + (display_bounds.size.width - window_size.width) / 2.0;
    let y = display_bounds.origin.y + (display_bounds.size.height - window_size.height) / 2.0;
    gpui::Bounds {
        origin: point(x, y),
        size: window_size,
    }
}

impl OnboardingRoot {
    fn new(
        client: NekoClient,
        hotkey: SharedHotkeyController,
        accessibility: SharedAccessibility,
        initial_combo: HotkeyCombo,
        slot: SharedOnboardingSlot,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            cx.on_release(|root: &mut Self, _| {
                *root.slot.borrow_mut() = None;
            })
            .detach();
            Self {
                setup: setup::Setup::new(cx),
                flow: Flow::new(initial_combo),
                client,
                accessibility,
                hotkey,
                slot,
                focus_handle: cx.focus_handle(),
                capture_generation: 0,
                capture_pending: None,
                poll_generation: 0,
            }
        })
    }

    /// Testing the hotkey confirms it without skipping the remaining setup.
    pub fn handle_global_hotkey_press(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.setup.step == setup::SetupStep::Mac
            && !matches!(self.flow.hotkey_recording, RecordingState::Recording)
        {
            self.setup.notice = Some("Shortcut tested. Continue when you’re ready.".into());
            cx.notify();
        }
    }

    fn request_accessibility(&mut self, cx: &mut Context<Self>) {
        self.flow.request_accessibility();
        self.accessibility.request_prompt();
        self.start_accessibility_poll(cx);
    }

    fn start_accessibility_poll(&mut self, cx: &mut Context<Self>) {
        self.poll_generation += 1;
        let generation = self.poll_generation;
        let accessibility = self.accessibility.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let trusted = accessibility.is_trusted();
                let stop = this
                    .update(cx, |root, cx| {
                        if root.poll_generation != generation
                            || root.flow.step != Step::AccessibilityWaiting
                        {
                            return true;
                        }
                        if !trusted {
                            return false;
                        }
                        root.flow.on_accessibility_granted();
                        root.ensure_hotkey_registered();
                        cx.notify();
                        true
                    })
                    .unwrap_or(true);
                if stop {
                    break;
                }
            }
        })
        .detach();
    }

    fn ensure_hotkey_registered(&mut self) {
        let mut controller = self.hotkey.borrow_mut();
        if controller.current_hotkey_id().is_none() {
            let _ = controller.apply_initial(&neko_protocol::HotkeyConfig {
                combo: self.flow.current_combo.clone(),
                updated_at_unix_ms: 0,
            });
        }
    }

    fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        if !self.flow.accessibility_granted {
            return;
        }
        self.flow.start_recording_hotkey();
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.flow.hotkey_recording, RecordingState::Recording) {
            if let Some(backward) = tab_direction(
                &event.keystroke.key,
                event.keystroke.modifiers.shift,
                &self.flow.hotkey_recording,
            ) {
                if backward {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
            }
            return;
        }
        if event.keystroke.key == "escape" {
            self.capture_generation = self.capture_generation.wrapping_add(1);
            self.setup.busy =
                busy_after_capture_cancel(self.setup.busy, self.capture_pending.take().is_some());
            self.flow.hotkey_recording = RecordingState::Idle;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.setup.busy {
            cx.stop_propagation();
            return;
        }
        let mut modifiers = Vec::new();
        if event.keystroke.modifiers.platform {
            modifiers.push(ProtoModifier::Cmd);
        }
        if event.keystroke.modifiers.alt {
            modifiers.push(ProtoModifier::Alt);
        }
        if event.keystroke.modifiers.control {
            modifiers.push(ProtoModifier::Ctrl);
        }
        if event.keystroke.modifiers.shift {
            modifiers.push(ProtoModifier::Shift);
        }
        let key = canonicalize_key_name(&event.keystroke.key);

        match validate_candidate(&modifiers, &key) {
            Err(reason) => {
                self.flow.hotkey_capture_rejected(reason);
                cx.notify();
            }
            Ok(candidate) => self.try_rebind(candidate, cx),
        }
    }

    fn try_rebind(&mut self, candidate: HotkeyCombo, cx: &mut Context<Self>) {
        if self.setup.busy {
            return;
        }
        self.setup.busy = true;
        self.capture_generation = self.capture_generation.wrapping_add(1);
        let generation = self.capture_generation;
        self.capture_pending = Some(generation);
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let heuristic_reason = match client
                .request(Request::CheckHotkeyConflict {
                    candidate: candidate.clone(),
                })
                .await
            {
                Ok(Response::HotkeyConflict { reason }) => reason,
                _ => None,
            };
            let _ = this.update(cx, |root, cx| {
                if !capture_is_current(
                    root.capture_generation,
                    generation,
                    &root.flow.hotkey_recording,
                ) {
                    return;
                }
                root.capture_pending = None;
                root.setup.busy = false;
                let rebind_result = root.hotkey.borrow_mut().rebind(candidate.clone());
                match rebind_result {
                    Ok(()) => {
                        root.flow.hotkey_rebind_succeeded(candidate.clone());
                        root.persist_hotkey_commit(candidate, cx);
                    }
                    Err(e) => {
                        let reason = heuristic_reason.unwrap_or_else(|| e.to_string());
                        root.flow.hotkey_rebind_failed(reason);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn persist_hotkey_commit(&mut self, candidate: HotkeyCombo, cx: &mut Context<Self>) {
        self.setup.busy = true;
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::CommitHotkey { candidate: candidate.clone() }).await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                match result {
                    Ok(Response::Hotkey { config }) if config.combo == candidate => root.setup.notice = Some("Shortcut saved.".into()),
                    _ => root.setup.error = Some("The shortcut works for this session but could not be saved. Choose it again to retry before restarting Neko.".into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn close_and_persist_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.setup.busy {
            return;
        }
        self.setup.busy = true;
        let handle = window.window_handle();
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client
                .request(Request::SetOnboardingComplete { completed: true })
                .await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                match result {
                    Ok(Response::Error { message }) => root.setup.error = Some(message),
                    Err(error) => root.setup.error = Some(error.to_string()),
                    Ok(Response::OnboardingState {
                        completed: true, ..
                    }) => {
                        *root.slot.borrow_mut() = None;
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                        crate::workspace::open(root.client.clone(), cx);
                    }
                    Ok(_) => {
                        root.setup.error =
                            Some("Setup completion was not acknowledged. Try again.".into())
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Focusable for OnboardingRoot {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// GPUI's macOS key names are lowercase (`"space"`, `"escape"`, `"up"`,
/// `"f1"`); `global-hotkey`'s parser is case-insensitive and accepts both
/// long names and bare single characters, so this only needs to produce
/// something readable for display — capitalized the way the rest of the
/// UI capitalizes key names (`HotkeyCombo::default_summon()` is `"Space"`,
/// not `"space"`).
impl Render for OnboardingRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_setup(window, cx)
    }
}
