mod accessibility;
mod components;
mod daemon_launcher;
mod hotkey_client;
mod material;
mod onboarding;
mod panel;
mod text_field;
mod theme;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use gpui::{
    App, Application, Bounds, Focusable, KeyBinding, Timer, WindowBounds, WindowKind,
    WindowOptions, point, px, size,
};
use neko_client::NekoClient;
use neko_protocol::{Event, HotkeyCombo, HotkeyConfig, Request, Response};

use accessibility::SystemAccessibilityChecker;
use hotkey_client::{HotkeyController, SystemRegistrar};
use onboarding::SharedOnboardingSlot;
use panel::Root;

/// Set to any value to force onboarding to show again on the next launch —
/// resets the persisted "completed" flag before checking it, same as a
/// genuinely fresh install. See `README.md`.
const RESET_ONBOARDING_ENV_VAR: &str = "NEKO_RESET_ONBOARDING";

/// Where the Dock-icon "way back" (design report §3, step 08) routes once
/// it exists — set as a GPUI global from inside `run` because
/// `Application::on_reopen` must be registered before `.run()` is called,
/// before the summon window or the onboarding slot exist yet.
struct ReopenTargets {
    window: gpui::WindowHandle<Root>,
    active_onboarding: SharedOnboardingSlot,
}

impl gpui::Global for ReopenTargets {}

fn main() {
    daemon_launcher::ensure_daemon_running();

    let app = Application::new();
    app.on_reopen(|cx| {
        let Some((window, active_onboarding)) = cx
            .try_global::<ReopenTargets>()
            .map(|t| (t.window, t.active_onboarding.clone()))
        else {
            return;
        };
        if let Some(onboarding_window) = *active_onboarding.borrow() {
            let _ = onboarding_window.update(cx, |_root, window, _cx| {
                window.activate_window();
            });
            return;
        }
        // Works whether or not the global hotkey is live, so declining
        // Accessibility never leaves neko unreachable.
        let _ = window.update(cx, |root, window, cx| {
            root.reset_for_summon(cx);
            window.activate_window();
            window.focus(&root.focus_handle(cx));
        });
        cx.activate(true);
    });

    app.run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("backspace", text_field::Backspace, Some("TextField")),
            KeyBinding::new("left", text_field::Left, Some("TextField")),
            KeyBinding::new("right", text_field::Right, Some("TextField")),
            KeyBinding::new("down", panel::SelectNext, Some("Panel")),
            KeyBinding::new("up", panel::SelectPrevious, Some("Panel")),
            KeyBinding::new("enter", panel::Confirm, Some("Panel")),
            KeyBinding::new("escape", DismissWindow, Some("Panel")),
            KeyBinding::new("enter", onboarding::view::Primary, Some("Onboarding")),
            KeyBinding::new("escape", onboarding::view::Secondary, Some("Onboarding")),
        ]);
        cx.on_action(|_: &DismissWindow, cx| cx.hide());

        let (client, event_rx) = NekoClient::connect(neko_protocol::socket_path());
        // Cheap to construct (no OS handshake, unlike `SystemRegistrar::new`)
        // so it's created once, up front, and shared by the summon panel
        // (the step 08 banner) and onboarding alike.
        let accessibility: onboarding::SharedAccessibility = Rc::new(SystemAccessibilityChecker);

        let bounds = upper_third(None, size(px(theme::PANEL_WIDTH_PX), px(panel::PANEL_HEIGHT_PX)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    focus: false,
                    show: false,
                    window_background: material::window_background(),
                    ..Default::default()
                },
                {
                    let client = client.clone();
                    let accessibility = accessibility.clone();
                    move |_window, cx| Root::new(client.clone(), accessibility.clone(), cx)
                },
            )
            .expect("failed to open the summon window");

        let visible = Rc::new(Cell::new(false));

        // Populated once onboarding decides it needs to run (see the async
        // block below); checked by both the hotkey-press loop and
        // `on_reopen` so either entry point routes to onboarding instead of
        // the summon panel while it's up. `OnboardingRoot` clears this
        // itself the moment it closes.
        let active_onboarding: SharedOnboardingSlot = Rc::new(RefCell::new(None));
        cx.set_global(ReopenTargets {
            window,
            active_onboarding: active_onboarding.clone(),
        });

        cx.spawn(async move |cx| {
            let config = fetch_initial_hotkey(&client).await;
            let onboarding_state = fetch_onboarding_state(&client).await;

            let registrar = SystemRegistrar::new()
                .expect("failed to talk to the OS hotkey service");
            let controller: onboarding::SharedHotkeyController = Rc::new(RefCell::new(HotkeyController::new(registrar)));

            // Accessibility is only required for the global hotkey (design
            // report §3, "Permission refusal — graceful degradation"): a
            // rejected registration here is expected, not fatal, whenever
            // it hasn't been granted yet — the summon loop below simply
            // never sees a matching hotkey id, and `on_reopen`/onboarding
            // remain the way in.
            if accessibility.is_trusted()
                && let Err(e) = controller.borrow_mut().apply_initial(&config)
            {
                eprintln!(
                    "neko: could not register the summon hotkey {}: {e} — is it already bound to another app?",
                    config.combo.display()
                );
            }

            if !onboarding_state.completed {
                let _ = cx.update(|cx| {
                    onboarding::open_window(
                        cx,
                        client.clone(),
                        controller.clone(),
                        accessibility.clone(),
                        config.combo.clone(),
                        active_onboarding.clone(),
                    );
                });
            }

            loop {
                if let Ok(hotkey_event) = GlobalHotKeyEvent::receiver().try_recv()
                    && Some(hotkey_event.id) == controller.borrow().current_hotkey_id()
                    && hotkey_event.state == HotKeyState::Pressed
                {
                    let onboarding_window = *active_onboarding.borrow();
                    if let Some(onboarding_window) = onboarding_window {
                        let _ = cx.update(|cx| {
                            let _ = onboarding_window.update(cx, |root, window, cx| {
                                root.handle_global_hotkey_press(window, cx);
                            });
                        });
                    } else {
                        let pressed_at = Instant::now();
                        let opening = !visible.get();
                        visible.set(opening);
                        let _ = cx.update(|cx| {
                            if opening {
                                let _ = window.update(cx, |root, window, cx| {
                                    root.reset_for_summon(cx);
                                    window.activate_window();
                                    window.focus(&root.focus_handle(cx));
                                    // The next frame is the first one painted after
                                    // activation — the closest proxy GPUI exposes for
                                    // "visible and accepting input" on screen.
                                    window.on_next_frame(move |_, _| {
                                        eprintln!("neko: summon latency {:?}", pressed_at.elapsed());
                                    });
                                });
                                cx.activate(true);
                            } else {
                                cx.hide();
                            }
                        });
                    }
                }

                while let Ok(daemon_event) = event_rx.try_recv() {
                    match daemon_event {
                        Event::HotkeyChanged { config } => {
                            if let Err(e) = controller.borrow_mut().apply_remote_change(&config) {
                                eprintln!("neko: failed to apply hotkey change from daemon: {e:?}");
                            }
                        }
                    }
                }

                Timer::after(Duration::from_millis(20)).await;
            }
        })
        .detach();
    });
}

async fn fetch_initial_hotkey(client: &NekoClient) -> HotkeyConfig {
    // The client and daemon start concurrently (`daemon_launcher` just
    // spawns the daemon and returns) — the daemon's socket may not be
    // listening yet on the very first launch, so retry briefly rather than
    // falling back to the default immediately and silently ignoring a
    // rebind the user made in a previous session.
    for _ in 0..25 {
        if let Ok(Response::Hotkey { config }) = client.request(Request::GetHotkey).await {
            return config;
        }
        Timer::after(Duration::from_millis(40)).await;
    }
    eprintln!("neko: could not reach neko-daemon for the configured hotkey in time, using the default");
    HotkeyConfig {
        combo: HotkeyCombo::default_summon(),
        updated_at_unix_ms: 0,
    }
}

struct OnboardingStateSnapshot {
    completed: bool,
}

async fn fetch_onboarding_state(client: &NekoClient) -> OnboardingStateSnapshot {
    if std::env::var_os(RESET_ONBOARDING_ENV_VAR).is_some() {
        let _ = client.request(Request::SetOnboardingComplete { completed: false }).await;
    }
    for _ in 0..25 {
        if let Ok(Response::OnboardingState { completed, .. }) = client.request(Request::GetOnboardingState).await {
            return OnboardingStateSnapshot { completed };
        }
        Timer::after(Duration::from_millis(40)).await;
    }
    // Fail toward *not* re-showing onboarding: a daemon that's merely slow
    // to answer shouldn't force a captain who already finished onboarding
    // through it again.
    eprintln!("neko: could not reach neko-daemon for onboarding state in time, assuming already completed");
    OnboardingStateSnapshot { completed: true }
}

/// The design report's §2 panel geometry: "positioned upper-third, not
/// vertically centered" — a true screen-center reads as a modal interrupting;
/// an upper placement reads as a tool summoned into view.
fn upper_third(display_id: Option<gpui::DisplayId>, panel_size: gpui::Size<gpui::Pixels>, cx: &App) -> Bounds<gpui::Pixels> {
    let display = display_id
        .and_then(|id| cx.find_display(id))
        .or_else(|| cx.primary_display());

    let Some(display) = display else {
        return Bounds {
            origin: point(px(0.), px(0.)),
            size: panel_size,
        };
    };

    let display_bounds = display.bounds();
    let x = display_bounds.origin.x + (display_bounds.size.width - panel_size.width) / 2.0;
    let y = display_bounds.origin.y + display_bounds.size.height / 3.0 - panel_size.height / 4.0;
    Bounds {
        origin: point(x, y),
        size: panel_size,
    }
}

gpui::actions!(neko, [DismissWindow]);
