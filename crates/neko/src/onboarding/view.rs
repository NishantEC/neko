//! The onboarding window's GPUI rendering and async orchestration: real OS
//! accessibility prompting/polling, daemon persistence, and live hotkey
//! capture, all driving the pure `state::Flow` machine. Step 03 (the real
//! macOS dialog) is not rendered here at all — `request_prompt` triggers it
//! and macOS draws it natively, on top of whatever this window shows.
//!
//! Deviations from the mockups, and why:
//! - **Fonts.** The mockups specify Instrument Serif / IBM Plex Sans /
//!   Martian Mono; `panel.rs` (already shipped) uses GPUI's default system
//!   font rather than embedding+registering three font families (design
//!   report §6: "fonts must be embedded and explicitly registered — no
//!   `@font-face` negotiation"). Onboarding follows that same, already-set
//!   precedent rather than introducing a font-asset pipeline as a side
//!   effect of this task. Sizes/weights/spacing follow the mockup's values
//!   on the default font.
//! - **Permission-row and status icons** (the small Accessibility/Clipboard
//!   glyphs) are painted as simple tinted squares, matching `panel.rs`'s
//!   own row-icon fallback style, rather than traced as exact vector paths
//!   — secondary visual polish, not load-bearing. The ⌥ glyph itself *is*
//!   traced exactly (see `components::glyphs::opt_glyph`) because the
//!   report specifically flags it as unreliable as font text.
//! - **The hotkey-recording sub-state** (after clicking "Use a different
//!   combination") has no mockup of its own — the frozen sequence shows
//!   only the idle keycap and the dismissal-by-pressing-it interaction.
//!   Its look reuses the same dialog/keycap/status-pill components as the
//!   rest of the arc rather than inventing new visual language.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, ClickEvent, Context, CursorStyle, Entity, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, Styled, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, actions,
    div, point, prelude::*, px, size,
};
use neko_client::NekoClient;
use neko_protocol::{HotkeyCombo, Modifier as ProtoModifier, Request, Response};

use crate::accessibility::AccessibilityChecker;
use crate::components::glyphs::opt_glyph;
use crate::hotkey_client::{HotkeyController, SystemRegistrar};
use crate::theme;

use super::state::{Flow, RecordingState, Step, canonicalize_key_name, validate_candidate};

pub type SharedHotkeyController = Rc<RefCell<HotkeyController<SystemRegistrar>>>;
pub type SharedAccessibility = Rc<dyn AccessibilityChecker>;
/// Cleared by `OnboardingRoot` itself the moment it closes, so a caller
/// (`main.rs`'s summon loop) can tell "onboarding is still up" from "it
/// finished" without needing its own completion event channel.
pub type SharedOnboardingSlot = Rc<RefCell<Option<WindowHandle<OnboardingRoot>>>>;

actions!(onboarding, [Primary, Secondary]);

pub struct OnboardingRoot {
    flow: Flow,
    client: NekoClient,
    accessibility: SharedAccessibility,
    hotkey: SharedHotkeyController,
    slot: SharedOnboardingSlot,
    focus_handle: FocusHandle,
    poll_generation: u64,
    /// Armed by mouse-down on the custom header strip; the next mouse-move
    /// with the button still held hands the drag to the compositor
    /// (`Window::start_window_move`) — same pattern comet and waku use for
    /// their own app-owned titlebar drag regions.
    header_drag_armed: bool,
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
    let window_size = size(px(theme::PANEL_WIDTH_PX), px(theme::ONBOARDING_HEIGHT_PX));
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
                focus: true,
                show: true,
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            move |_window, cx| {
                OnboardingRoot::new(client, hotkey, accessibility, initial_combo, window_slot, cx)
            },
        )
        .expect("failed to open the onboarding window");

    *slot.borrow_mut() = Some(window);
    let _ = window.update(cx, |root, window, cx| {
        window.focus(&root.focus_handle(cx), cx);
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
        cx.new(|cx| Self {
            flow: Flow::new(initial_combo),
            client,
            accessibility,
            hotkey,
            slot,
            focus_handle: cx.focus_handle(),
            poll_generation: 0,
            header_drag_armed: false,
        })
    }

    /// Called by `main.rs`'s summon loop when the live global hotkey fires
    /// while this window is the active onboarding window — the mechanism
    /// behind step 09's "pressing the real combination dismisses the
    /// screen."
    pub fn handle_global_hotkey_press(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.flow.hotkey_recording, RecordingState::Recording) {
            return;
        }
        self.flow.hotkey_pressed();
        if self.flow.finished {
            self.finish_and_close(window, cx);
        } else {
            cx.notify();
        }
    }

    fn on_primary_action(&mut self, _: &Primary, window: &mut Window, cx: &mut Context<Self>) {
        self.primary_action(window, cx);
    }

    /// Keyboard-reachable, bound to Escape: "Not now" is drawn as "a
    /// first-class, equally-sized choice, not a hidden link" (design
    /// report §3, step 02) — that should hold for keyboard-only use too,
    /// not just the mouse.
    fn on_secondary_action(&mut self, _: &Secondary, _window: &mut Window, cx: &mut Context<Self>) {
        self.secondary_action(cx);
    }

    fn primary_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.flow.hotkey_recording, RecordingState::Recording) {
            return;
        }
        match self.flow.step {
            Step::Welcome => self.flow.advance_from_welcome(),
            Step::WhatNekoNeeds => self.flow.continue_to_accessibility_ask(),
            Step::AccessibilityAsk => self.request_accessibility(cx),
            Step::AccessibilityWaiting => {}
            Step::AccessibilityGranted => self.flow.continue_from_accessibility_granted(),
            Step::ClipboardAsk => self.enable_clipboard(cx),
            Step::ClipboardGranted => self.flow.continue_from_clipboard_granted(),
            Step::LearnHotkey => self.finish_and_close(window, cx),
        }
        cx.notify();
    }

    fn secondary_action(&mut self, cx: &mut Context<Self>) {
        match self.flow.step {
            Step::AccessibilityAsk | Step::AccessibilityWaiting => self.flow.skip_accessibility(),
            Step::ClipboardAsk => {
                self.flow.decline_clipboard();
                self.persist_clipboard_enabled(false, cx);
            }
            _ => {}
        }
        cx.notify();
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
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let trusted = accessibility.is_trusted();
                let stop = this
                    .update(cx, |root, cx| {
                        if root.poll_generation != generation || root.flow.step != Step::AccessibilityWaiting {
                            return true;
                        }
                        if !trusted {
                            return false;
                        }
                        root.flow.on_accessibility_granted();
                        root.ensure_hotkey_registered();
                        root.schedule_auto_advance(Step::AccessibilityGranted, 900, cx);
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

    fn schedule_auto_advance(&mut self, from: Step, delay_ms: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(delay_ms)).await;
            let _ = this.update(cx, |root, cx| {
                if root.flow.step != from {
                    return;
                }
                match from {
                    Step::AccessibilityGranted => root.flow.continue_from_accessibility_granted(),
                    Step::ClipboardGranted => root.flow.continue_from_clipboard_granted(),
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn enable_clipboard(&mut self, cx: &mut Context<Self>) {
        self.flow.enable_clipboard();
        self.persist_clipboard_enabled(true, cx);
        self.schedule_auto_advance(Step::ClipboardGranted, 700, cx);
    }

    fn persist_clipboard_enabled(&self, enabled: bool, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |_this, _cx| {
            let _ = client.request(Request::SetClipboardHistoryEnabled { enabled }).await;
        })
        .detach();
    }

    fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        if !self.flow.accessibility_granted {
            return;
        }
        self.flow.start_recording_hotkey();
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.flow.hotkey_recording, RecordingState::Recording) {
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
        let client = self.client.clone();
        let hotkey = self.hotkey.clone();
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
            let rebind_result = hotkey.borrow_mut().rebind(candidate.clone());
            let _ = this.update(cx, |root, cx| {
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

    fn persist_hotkey_commit(&self, candidate: HotkeyCombo, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |_this, _cx| {
            let _ = client.request(Request::CommitHotkey { candidate }).await;
        })
        .detach();
    }

    fn finish_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flow.finish();
        self.close_and_persist_completion(window, cx);
    }

    /// The persistent "Skip setup" footer control — jumps straight to the
    /// working launcher from any step, for a captain re-testing the app who
    /// doesn't want to walk the whole arc again. Reuses exactly the same
    /// completion/close path `finish_and_close` uses, so the graceful
    /// degradation is identical to declining accessibility from step 02:
    /// nothing skip touches grants accessibility or enables clipboard on
    /// its own, so `main.rs`'s summon loop simply never sees a live hotkey
    /// id when it wasn't already granted, and `App::on_reopen`'s Dock-icon
    /// path remains the way back in exactly as it already is for that case.
    fn skip_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.flow.hotkey_recording, RecordingState::Recording) {
            return;
        }
        self.flow.skip_onboarding();
        self.close_and_persist_completion(window, cx);
    }

    fn close_and_persist_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |_this, _cx| {
            let _ = client.request(Request::SetOnboardingComplete { completed: true }).await;
        })
        .detach();
        *self.slot.borrow_mut() = None;
        window.remove_window();
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
        div()
            .key_context("Onboarding")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_primary_action))
            .on_action(cx.listener(Self::on_secondary_action))
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::active().surface_panel)
            .text_color(theme::active().text_primary)
            .child(self.render_header(window, cx))
            .child(self.render_content(cx))
            .child(self.render_footer(cx))
    }
}

impl OnboardingRoot {
    /// The window's only chrome: no system title bar (see `open_window`'s
    /// `TitlebarOptions`), so this strip both draws the neko identity and
    /// drags the window. Reserves left clearance for the real macOS traffic
    /// lights repositioned into it, unless fullscreen has hidden them.
    fn render_header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let left_padding = if !cfg!(target_os = "macos") {
            theme::ONBOARDING_HEADER_BASE_PADDING_PX
        } else if window.is_fullscreen() {
            theme::ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_FULLSCREEN_PX
        } else {
            theme::ONBOARDING_TRAFFIC_LIGHT_CLEARANCE_PX
        };

        div()
            .id("onboarding-header")
            .window_control_area(gpui::WindowControlArea::Drag)
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(theme::INPUT_ROW_HEIGHT_PX))
            .pl(px(left_padding))
            .pr(px(theme::ONBOARDING_HEADER_BASE_PADDING_PX))
            .gap_3()
            .border_b_1()
            .border_color(theme::active().border_hairline)
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.header_drag_armed = false))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.header_drag_armed = false),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.header_drag_armed = true),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, _| {
                if this.header_drag_armed && event.pressed_button == Some(gpui::MouseButton::Left)
                {
                    this.header_drag_armed = false;
                    window.start_window_move();
                }
            }))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
            .child(
                gpui::svg()
                    .path(crate::assets::icon::MARK)
                    .w(px(18.))
                    .h(px(18.))
                    .text_color(theme::active().text_primary),
            )
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("neko"),
            )
    }

    fn setup_phase(&self) -> u8 {
        match self.flow.step {
            Step::Welcome => 1,
            Step::WhatNekoNeeds | Step::AccessibilityAsk | Step::AccessibilityWaiting | Step::AccessibilityGranted => 2,
            Step::ClipboardAsk | Step::ClipboardGranted => 3,
            Step::LearnHotkey => 4,
        }
    }

    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let container = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .gap_4()
            .px(px(32.))
            .pt(px(28.))
            .pb(px(24.));

        let container = if self.flow.step == Step::Welcome {
            container
        } else {
            container.child(progress_dots(self.setup_phase()))
        };

        match self.flow.step {
            Step::Welcome => container.children(self.render_welcome()),
            Step::WhatNekoNeeds => container.children(self.render_what_neko_needs()),
            Step::AccessibilityAsk => container.children(self.render_accessibility_ask(cx)),
            Step::AccessibilityWaiting => container.children(self.render_accessibility_waiting(cx)),
            Step::AccessibilityGranted => container.children(self.render_accessibility_granted(cx)),
            Step::ClipboardAsk => container.children(self.render_clipboard_ask(cx)),
            Step::ClipboardGranted => container.children(self.render_clipboard_granted(cx)),
            Step::LearnHotkey => container.children(self.render_learn_hotkey(cx)),
        }
    }

    fn render_welcome(&self) -> Vec<gpui::AnyElement> {
        vec![
            eyebrow("Welcome").into_any_element(),
            title("This is neko.").into_any_element(),
            body(
                "A fast way to find apps, search what you've copied, and keep an eye \
                 on the coding agents running on this Mac. Two quick permissions to \
                 set up — about 30 seconds, and you won't see this again.",
            )
            .into_any_element(),
        ]
    }

    fn render_what_neko_needs(&self) -> Vec<gpui::AnyElement> {
        vec![
            eyebrow("Two permissions").into_any_element(),
            title("Here's everything neko will ask for.").into_any_element(),
            // **Scoped, because the unscoped version stopped being true.**
            // It used to read "Nothing you copy or open ever leaves this
            // Mac", written when neko only searched apps and the clipboard.
            // `ask.rs` now sends what you type to Anthropic, `usage.rs`
            // reads three vendors' quota APIs, and everything under Agents
            // talks to Paseo. Those are opt-in and none of them is
            // searching — but a promise that covers them by omission is a
            // false one, and this screen is titled "everything neko will
            // ask for".
            body("Both up front, so nothing surprises you later. What you search and what you copy stay on this Mac.")
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap_3()
                .mt_2()
                .child(permission_row(
                    "Accessibility",
                    "So ⌥Space opens neko from any app — a macOS system prompt.",
                ))
                .child(permission_row(
                    "Clipboard History",
                    "So your copy history is searchable — asked by neko itself, macOS has no prompt for this one.",
                ))
                .into_any_element(),
        ]
    }

    fn render_accessibility_ask(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        vec![
            icon_badge().into_any_element(),
            title("Enable accessibility").into_any_element(),
            body("neko listens for ⌥Space in the background, even when another app is focused — macOS calls that Accessibility access. It's what lets the hotkey work from anywhere.")
                .into_any_element(),
            div()
                .text_size(px(12.5))
                .text_color(theme::active().text_tertiary)
                .max_w(px(460.))
                .child("macOS will show its own window for this next. Deny it there and neko still runs — just not from the hotkey.")
                .into_any_element(),
            div()
                .flex()
                .items_center()
                .gap_4()
                .mt_2()
                .child(primary_button(
                    "onboarding-primary",
                    "Open System Settings",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.primary_action(window, cx)),
                ))
                .child(link_button(
                    "onboarding-secondary",
                    "Not now",
                    cx.listener(|this, _: &ClickEvent, _window, cx| this.secondary_action(cx)),
                ))
                .into_any_element(),
        ]
    }

    fn render_accessibility_waiting(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        vec![
            status_pill("Waiting for Accessibility access", PillTone::Pending).into_any_element(),
            title("Turn it on in System Settings.").into_any_element(),
            body("System Settings → Privacy & Security → Accessibility → turn on neko. This screen updates itself the moment you do — nothing to click here.")
                .into_any_element(),
            link_button(
                "onboarding-continue-without",
                "Not now",
                cx.listener(|this, _: &ClickEvent, _window, cx| this.secondary_action(cx)),
            )
            .into_any_element(),
        ]
    }

    fn render_accessibility_granted(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        vec![
            status_pill("Accessibility enabled", PillTone::Success).into_any_element(),
            title_with_glyph("Space is live.").into_any_element(),
            body("One down. Next: your clipboard.").into_any_element(),
            div()
                .flex()
                .child(primary_button(
                    "onboarding-primary",
                    "Continue",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.primary_action(window, cx)),
                ))
                .into_any_element(),
        ]
    }

    fn render_clipboard_ask(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        vec![
            icon_badge().into_any_element(),
            title("Turn on Clipboard History").into_any_element(),
            body("macOS won't ask you this one — so neko is. It can watch what you copy and make it searchable. Anything that looks like a password or API key is never stored.")
                .into_any_element(),
            div()
                .flex()
                .items_center()
                .gap_4()
                .mt_2()
                .child(primary_button(
                    "onboarding-primary",
                    "Enable clipboard history",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.primary_action(window, cx)),
                ))
                .child(link_button(
                    "onboarding-secondary",
                    "Not now",
                    cx.listener(|this, _: &ClickEvent, _window, cx| this.secondary_action(cx)),
                ))
                .into_any_element(),
        ]
    }

    fn render_clipboard_granted(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        vec![
            status_pill("Clipboard History is on", PillTone::Success).into_any_element(),
            title("Unlimited history, by default.").into_any_element(),
            // Was "Change retention anytime in Settings." There is no
            // retention setting — `clipboard::HISTORY_LIMIT` is a hard 200
            // and Preferences has five rows, none of them this. Promising a
            // control that does not exist sends somebody looking for it
            // forever.
            body("No 3-month cutoff, no subscription. The last 200 copies, kept locally.")
                .into_any_element(),
            div()
                .flex()
                .child(primary_button(
                    "onboarding-primary",
                    "Continue",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.primary_action(window, cx)),
                ))
                .into_any_element(),
        ]
    }

    fn render_learn_hotkey(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let mut items = vec![eyebrow("Last thing").into_any_element()];

        match &self.flow.hotkey_recording {
            RecordingState::Recording => {
                items.push(title("Press your new combination…").into_any_element());
                items.push(
                    body("Include at least one modifier key — ⌘, ⌥, ⌃, or ⇧.")
                        .into_any_element(),
                );
            }
            RecordingState::Idle | RecordingState::Rejected(_) => {
                items.push(title("This opens neko. From anywhere.").into_any_element());
                items.push(keycap_row(&self.flow.current_combo).into_any_element());
                if let RecordingState::Rejected(reason) = &self.flow.hotkey_recording {
                    items.push(status_pill(reason, PillTone::Danger).into_any_element());
                }
                if self.flow.accessibility_granted {
                    items.push(
                        body("Press it now to see neko close. That's the whole interaction — open, act, gone.")
                            .into_any_element(),
                    );
                    items.push(
                        link_button(
                            "onboarding-use-different-combo",
                            "Use a different combination",
                            cx.listener(|this, _: &ClickEvent, _window, cx| this.toggle_recording(cx)),
                        )
                        .into_any_element(),
                    );
                } else {
                    items.push(
                        body("Accessibility is off, so the hotkey isn't live this session. You can grant it later and rebind from Settings.")
                            .into_any_element(),
                    );
                }
            }
        }

        items.push(
            div()
                .flex()
                .mt_2()
                .child(primary_button(
                    "onboarding-primary",
                    "Done — take me in",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.primary_action(window, cx)),
                ))
                .into_any_element(),
        );

        items
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let phase = self.setup_phase();
        let (label, show_kbd): (SharedString, bool) = match self.flow.step {
            Step::Welcome => ("Get started".into(), true),
            Step::WhatNekoNeeds => ("Continue".into(), true),
            Step::AccessibilityAsk => ("Open System Settings".into(), true),
            Step::AccessibilityWaiting => ("Watching for permission…".into(), false),
            Step::AccessibilityGranted => ("Continue".into(), true),
            Step::ClipboardAsk => ("Enable".into(), true),
            Step::ClipboardGranted => ("Continue".into(), true),
            Step::LearnHotkey => match &self.flow.hotkey_recording {
                RecordingState::Recording => ("Listening…".into(), false),
                _ if self.flow.accessibility_granted => {
                    (format!("Try {}", self.flow.current_combo.display()).into(), false)
                }
                _ => ("Done — take me in".into(), true),
            },
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(px(theme::FOOTER_HEIGHT_PX))
            .px_5()
            .border_t_1()
            .border_color(theme::active().border_hairline)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::active().text_tertiary)
                            .child(format!("Setup · {phase} of 4")),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::active().border_hairline_strong)
                            .child("·"),
                    )
                    .child(
                        div()
                            .id("onboarding-skip")
                            .text_size(px(11.))
                            .text_color(theme::active().text_tertiary)
                            .cursor(CursorStyle::PointingHand)
                            .hover(|s| s.text_color(theme::active().text_secondary))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.skip_onboarding(window, cx)
                            }))
                            .child("Skip setup"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.5))
                    .text_color(if show_kbd { theme::active().text_secondary } else { theme::active().text_tertiary })
                    .child(label)
                    .when(show_kbd, |row| {
                        row.child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::active().text_secondary)
                                .px_1p5()
                                .py_0p5()
                                .rounded(px(theme::CHIP_RADIUS_PX))
                                .border_1()
                                .border_color(theme::active().border_hairline_strong)
                                .child("↵"),
                        )
                    }),
            )
    }
}

fn eyebrow(label: &'static str) -> impl IntoElement {
    div()
        .text_size(px(11.))
        .text_color(theme::active().text_tertiary)
        .child(label)
}

fn title(label: &'static str) -> impl IntoElement {
    div()
        .text_size(px(26.))
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(theme::active().text_primary)
        .child(label)
}

/// `title` for the two headlines that lead with the ⌥ glyph inline
/// (step 05's "⌥Space is live.") — the painted glyph can't sit inside a
/// plain string child, so this composes it beside the rest of the text.
fn title_with_glyph(rest: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(opt_glyph(px(22.), theme::active().text_primary))
        .child(
            div()
                .text_size(px(26.))
                .text_color(theme::active().text_primary)
                .child(rest),
        )
}

fn body(text: &'static str) -> impl IntoElement {
    div()
        .text_size(px(14.))
        .line_height(px(21.7))
        .text_color(theme::active().text_secondary)
        .max_w(px(460.))
        .child(text)
}

fn permission_row(title: &'static str, description: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap_3()
        .p_3()
        .rounded(px(theme::ROW_RADIUS_PX))
        .border_1()
        .border_color(theme::active().border_hairline)
        .child(
            div()
                .w(px(30.))
                .h(px(30.))
                .flex_shrink_0()
                .rounded(px(6.))
                .bg(theme::active().row_icon_socket_bg),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_size(px(13.5)).font_weight(gpui::FontWeight::MEDIUM).child(title))
                .child(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(18.))
                        .text_color(theme::active().text_tertiary)
                        .child(description),
                ),
        )
}

fn icon_badge() -> impl IntoElement {
    div().w(px(44.)).h(px(44.)).rounded(px(10.)).bg(theme::active().row_icon_socket_bg)
}

enum PillTone {
    Pending,
    Success,
    Danger,
}

fn status_pill(label: impl Into<SharedString>, tone: PillTone) -> impl IntoElement {
    let (dot_color, border_color) = match tone {
        PillTone::Pending => (theme::active().text_tertiary, theme::active().border_hairline_strong),
        PillTone::Success => (theme::active().state_success, theme::active().state_success_border),
        PillTone::Danger => (theme::active().state_danger, theme::active().state_danger_border),
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(px(20.))
        .border_1()
        .border_color(border_color)
        .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(dot_color))
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::active().text_secondary)
                .child(label.into()),
        )
}

fn keycap_shell() -> gpui::Div {
    div()
        .min_w(px(52.))
        .h(px(52.))
        .px_3p5()
        .rounded(px(12.))
        .bg(theme::active().keycap_shell_bg)
        .border_1()
        .border_color(theme::active().border_hairline_strong)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(20.))
        .text_color(theme::active().text_primary)
}

fn keycap(label: impl Into<SharedString>) -> impl IntoElement {
    keycap_shell().child(label.into())
}

fn keycap_opt() -> impl IntoElement {
    keycap_shell().child(opt_glyph(px(18.), theme::active().text_primary))
}

fn keycap_row(combo: &HotkeyCombo) -> impl IntoElement {
    let mut row = div().flex().items_center().gap_2().my_2();
    for modifier in &combo.modifiers {
        row = match modifier {
            ProtoModifier::Alt => row.child(keycap_opt()),
            ProtoModifier::Cmd => row.child(keycap("⌘")),
            ProtoModifier::Ctrl => row.child(keycap("⌃")),
            ProtoModifier::Shift => row.child(keycap("⇧")),
        };
        row = row.child(div().text_size(px(16.)).text_color(theme::active().text_tertiary).child("+"));
    }
    row.child(keycap(combo.key.clone()))
}

fn primary_button(
    id: &'static str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(34.))
        .px_4()
        .rounded(px(theme::BTN_RADIUS_PX))
        .bg(theme::active().text_primary)
        .flex()
        .items_center()
        .justify_center()
        .cursor(CursorStyle::PointingHand)
        .active(|s| s.opacity(0.85))
        .on_click(on_click)
        .child(
            div()
                .text_size(px(13.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme::active().text_on_light)
                .child(label.into()),
        )
}

fn link_button(
    id: &'static str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .text_size(px(13.))
        .text_color(theme::active().text_tertiary)
        .border_b_1()
        .border_color(theme::active().border_hairline_strong)
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.text_color(theme::active().text_primary))
        .on_click(on_click)
        .child(label.into())
}

fn progress_dots(phase: u8) -> impl IntoElement {
    let mut row = div().flex().gap_1p5().mb_1();
    for step in 1..=4u8 {
        let color = if step < phase {
            theme::active().text_tertiary
        } else if step == phase {
            theme::active().text_primary
        } else {
            theme::active().border_hairline_strong
        };
        row = row.child(div().w(px(14.)).h(px(3.)).rounded(px(2.)).bg(color));
    }
    row
}
