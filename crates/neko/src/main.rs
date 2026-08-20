mod accessibility;
mod components;
mod daemon_launcher;
mod display_placement;
mod edge_fade;
mod evidence;
mod hotkey_client;
mod material;
mod menu_frost;
mod modes;
mod motion;
mod onboarding;
mod panel;
mod pasteboard;
mod row_icon_cache;
mod spaces;
mod text_field;
mod theme;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use gpui::{
    App, AsyncApp, Bounds, Focusable, KeyBinding, WindowBounds, WindowKind, WindowOptions, point,
    px, size,
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

    // `gpui::Application::new()` no longer exists on the `wingleeio/zed`
    // fork this crate depends on (`AGENTS.md`, "The GPUI dependency
    // decision") — every real bootstrap on that fork goes through
    // `gpui_platform::application()`, which selects the mac platform
    // backend (`gpui_macos::MacPlatform`) internally.
    let app = gpui_platform::application();
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
            root.reset_for_summon(window, cx);
            reposition_to_cursor_display(window);
            window.activate_window();
            window.focus(&root.focus_handle(cx), cx);
        });
        cx.activate(true);
    });

    app.run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("backspace", text_field::Backspace, Some("TextField")),
            KeyBinding::new("left", text_field::Left, Some("TextField")),
            KeyBinding::new("right", text_field::Right, Some("TextField")),
            // Standard macOS single-line editing shortcuts — see
            // `text_field.rs`'s own doc comment on the handlers for exactly
            // what each does and why selection/paste aren't part of this.
            KeyBinding::new("cmd-backspace", text_field::DeleteLineStart, Some("TextField")),
            KeyBinding::new("cmd-delete", text_field::DeleteLineEnd, Some("TextField")),
            KeyBinding::new("alt-backspace", text_field::DeleteWordBackward, Some("TextField")),
            KeyBinding::new("alt-delete", text_field::DeleteWordForward, Some("TextField")),
            KeyBinding::new("cmd-left", text_field::LineStart, Some("TextField")),
            KeyBinding::new("cmd-right", text_field::LineEnd, Some("TextField")),
            KeyBinding::new("alt-left", text_field::WordBackward, Some("TextField")),
            KeyBinding::new("alt-right", text_field::WordForward, Some("TextField")),
            // Selection and clipboard — see `text_field.rs`'s own module
            // doc comment. Deliberately no `cmd-k` binding here — that's
            // the panel-level actions menu, a parallel task's own seam.
            KeyBinding::new("shift-left", text_field::SelectLeft, Some("TextField")),
            KeyBinding::new("shift-right", text_field::SelectRight, Some("TextField")),
            KeyBinding::new("shift-alt-left", text_field::SelectWordLeft, Some("TextField")),
            KeyBinding::new("shift-alt-right", text_field::SelectWordRight, Some("TextField")),
            KeyBinding::new("shift-cmd-left", text_field::SelectLineStart, Some("TextField")),
            KeyBinding::new("shift-cmd-right", text_field::SelectLineEnd, Some("TextField")),
            KeyBinding::new("cmd-a", text_field::SelectAll, Some("TextField")),
            KeyBinding::new("cmd-c", text_field::Copy, Some("TextField")),
            KeyBinding::new("cmd-x", text_field::Cut, Some("TextField")),
            KeyBinding::new("cmd-v", text_field::Paste, Some("TextField")),
            KeyBinding::new("down", panel::SelectNext, Some("Panel")),
            KeyBinding::new("up", panel::SelectPrevious, Some("Panel")),
            KeyBinding::new("enter", panel::Confirm, Some("Panel")),
            KeyBinding::new("cmd-k", panel::OpenActionsMenu, Some("Panel")),
            KeyBinding::new("escape", DismissWindow, Some("Panel")),
            KeyBinding::new("enter", onboarding::view::Primary, Some("Onboarding")),
            KeyBinding::new("escape", onboarding::view::Secondary, Some("Onboarding")),
        ]);
        cx.on_action(|_: &DismissWindow, cx| cx.hide());

        // Verification-only, inert unless `NEKO_BACKDROP_IMAGE` is set —
        // see `evidence.rs`'s own doc comment.
        if let Some(path) = evidence::backdrop_image_path() {
            evidence::open_backdrop_window(cx, path);
        }

        let (client, event_rx) = NekoClient::connect(neko_protocol::socket_path());
        // Cheap to construct (no OS handshake, unlike `SystemRegistrar::new`)
        // so it's created once, up front, and shared by the summon panel
        // (the step 08 banner) and onboarding alike.
        let accessibility: onboarding::SharedAccessibility = Rc::new(SystemAccessibilityChecker);

        // Always `PANEL_WIDTH_WITH_DETAIL_PX` — the real `NSWindow` never
        // resizes for a mode transition any more (see `AGENTS.md`, "Mode
        // view resize seam"), and since "One constant panel width" the panel
        // `div` itself is always exactly this wide too, root list and
        // clipboard mode alike, so window and panel never diverge.
        let bounds = upper_third(None, size(px(theme::PANEL_WIDTH_WITH_DETAIL_PX), px(panel::PANEL_HEIGHT_PX)), cx);
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
                    move |window, cx| {
                        // Installed once here, right after the window opens
                        // — this window is resident for the process
                        // lifetime (only ever hidden, never closed), so one
                        // install covers every future summon. On `Err`,
                        // the window is already `Transparent`
                        // (`material::window_background`), so it has to be
                        // flipped back to `Opaque` explicitly or the panel
                        // would render over whatever is genuinely behind it
                        // on screen instead of a solid fill.
                        let translucent = match material::install(window) {
                            Ok(installed) => {
                                eprintln!("neko: window material installed: {installed:?}");
                                // Non-visual proof `install`'s claim is
                                // real: reads the live view straight back
                                // off the window rather than trusting the
                                // setter calls took effect (see
                                // `material::verify_installed`'s own doc
                                // comment).
                                match material::verify_installed(window, installed) {
                                    Ok(readback) => eprintln!("neko: material verified: {readback}"),
                                    Err(e) => eprintln!("neko: material readback verification FAILED: {e}"),
                                }
                                true
                            }
                            Err(e) => {
                                eprintln!("neko: native window material install failed, falling back to opaque: {e}");
                                window.set_background_appearance(gpui::WindowBackgroundAppearance::Opaque);
                                false
                            }
                        };
                        // Permanent runtime sanity check, same reasoning as
                        // the material readback just above: proves the
                        // Spaces/full-screen reachability the audit could
                        // only grep for (`data/neko-audit/report.md` Part 4
                        // item 10) against the real live window, not just
                        // gpui's source.
                        match spaces::verify(window) {
                            Ok(readback) => eprintln!("neko: Spaces/full-screen reachability verified: {readback}"),
                            Err(e) => eprintln!("neko: Spaces/full-screen reachability readback FAILED: {e}"),
                        }
                        // No native-backdrop narrowing call here any more —
                        // the panel is now always `PANEL_WIDTH_WITH_DETAIL_PX`
                        // wide too (`AGENTS.md`, "One constant panel width"),
                        // so `install`'s own full-`contentView`-bounds
                        // material install already matches it exactly, at
                        // rest and through every mode transition.
                        // The fixed-width window itself is always
                        // `PANEL_WIDTH_WITH_DETAIL_PX` now, but AppKit's own
                        // automatic window shadow doesn't know the visible
                        // panel is narrower — it shadows the whole `NSWindow`
                        // frame, producing a second, wrongly-sized "surface"
                        // around the real one (`AGENTS.md`, "Window
                        // material"; `docs/evidence/double-panel-shadow-fix-report.md`).
                        // The panel `div`'s own `.shadow_lg()` is the only
                        // shadow this app needs.
                        if let Err(e) = material::disable_native_shadow(window) {
                            eprintln!("neko: could not disable the native window shadow: {e}");
                        } else {
                            match material::verify_shadow_disabled(window) {
                                Ok(()) => eprintln!("neko: native window shadow disabled (verified)"),
                                Err(e) => eprintln!("neko: native window shadow disable verification FAILED: {e}"),
                            }
                        }
                        // The `⌘K` actions menu's own smaller frost surface
                        // — only meaningful when the whole-window material
                        // itself installed (`translucent`); with the opaque
                        // fallback there is no ambient glass for a menu-
                        // scoped patch of it to look distinct against, so
                        // the menu just keeps its existing fully-opaque
                        // `SURFACE_RAISED` fill instead (`panel.rs`'s own
                        // honest-fallback branch). Installed once, hidden
                        // and zero-sized, right after the whole-window
                        // material — see `material::install_menu_overlay`'s
                        // own doc comment for the invariant this depends on.
                        let menu_frost = translucent
                            && match material::install_menu_overlay(window) {
                                Ok(installed) => {
                                    eprintln!("neko: menu overlay material installed: {installed:?}");
                                    match material::verify_menu_overlay_installed(window, installed) {
                                        Ok(readback) => eprintln!("neko: menu overlay material verified: {readback}"),
                                        Err(e) => eprintln!("neko: menu overlay material readback verification FAILED: {e}"),
                                    }
                                    true
                                }
                                Err(e) => {
                                    eprintln!("neko: menu overlay material install failed, actions menu stays opaque: {e}");
                                    false
                                }
                            };
                        Root::new(client.clone(), accessibility.clone(), translucent, menu_frost, cx)
                    }
                },
            )
            .expect("failed to open the summon window");

        // Frozen spec (`data/neko-design/report.md` §2): "dismisses on
        // `Esc` or on losing focus." Escape is the `DismissWindow` binding
        // above; this is the focus-loss half — registered once, on the one
        // resident summon window, never on the separate onboarding window,
        // so declining/stepping through onboarding is never affected by it.
        // `cx.hide()` here is exactly `confirm()`'s own hide
        // (`panel.rs::confirm`) — both just tell the OS the window is no
        // longer active, so there's nothing to reconcile between the two
        // paths.
        let _ = window.update(cx, |_root, window, cx| {
            cx.observe_window_activation(window, |_root, window, cx| {
                if !window.is_window_active() {
                    eprintln!("neko: summon window lost activation, hiding");
                    cx.hide();
                }
            })
            .detach();
        });

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

        // Evidence/verification-only, both inert unless their env var is
        // set — see `evidence.rs`'s own doc comment.
        if let Some(iterations) = evidence::bench_iterations() {
            let client = client.clone();
            cx.spawn(async move |cx| evidence::run_bench(&client, window, cx, iterations).await)
                .detach();
        } else if let Some(iterations) = evidence::bench_real_iterations() {
            let client = client.clone();
            cx.spawn(async move |cx| evidence::run_bench_real(&client, window, cx, iterations).await)
                .detach();
        } else if evidence::show_on_launch_requested() {
            let client = client.clone();
            cx.spawn(async move |cx| evidence::show_once(&client, window, cx).await)
                .detach();
        }

        cx.spawn(async move |cx| {
            let config = fetch_initial_hotkey(&client, cx).await;
            let onboarding_state = fetch_onboarding_state(&client, cx).await;

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

            // `NekoClient::is_connected()` is a plain poll, not a push
            // channel (see that method's own doc comment) — this loop
            // already polls hotkey events and `event_rx` on a fixed 20ms
            // interval below, so checking one more thing here is the
            // smaller addition. Seeded `true` to match `Root::connected`'s
            // own optimistic default, so a launch where the daemon is
            // already connected by the first tick (the common case) never
            // pushes a spurious no-op transition.
            let mut last_connected = true;

            if !onboarding_state.completed {
                cx.update(|cx| {
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
                        cx.update(|cx| {
                            let _ = onboarding_window.update(cx, |root, window, cx| {
                                root.handle_global_hotkey_press(window, cx);
                            });
                        });
                    } else {
                        let pressed_at = Instant::now();
                        // The window's own live activation state is the
                        // toggle's ground truth, not a separately tracked
                        // flag — a flag desyncs the moment the window is
                        // hidden by anything other than this branch (a
                        // result activation via `confirm()`, or clicking
                        // outside via the focus-loss observer registered
                        // above), which would otherwise make the *next*
                        // hotkey press silently no-op instead of
                        // re-summoning.
                        cx.update(|cx| {
                            let _ = window.update(cx, |root, window, cx| {
                                if window.is_window_active() {
                                    cx.hide();
                                } else {
                                    root.reset_for_summon(window, cx);
                                    reposition_to_cursor_display(window);
                                    window.activate_window();
                                    window.focus(&root.focus_handle(cx), cx);
                                    // The next frame is the first one painted after
                                    // activation — the closest proxy GPUI exposes for
                                    // "visible and accepting input" on screen.
                                    window.on_next_frame(move |_, _| {
                                        eprintln!("neko: summon latency {:?}", pressed_at.elapsed());
                                    });
                                    cx.activate(true);
                                }
                            });
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
                        Event::IconsUpdated => {
                            cx.update(|cx| {
                                let _ = window.update(cx, |root, _window, cx| {
                                    root.refresh_icons(cx);
                                });
                            });
                        }
                    }
                }

                let is_connected = client.is_connected();
                if is_connected != last_connected {
                    last_connected = is_connected;
                    cx.update(|cx| {
                        let _ = window.update(cx, |root, _window, cx| {
                            root.set_connected(is_connected, cx);
                        });
                    });
                }

                cx.background_executor().timer(Duration::from_millis(20)).await;
            }
        })
        .detach();
    });
}

async fn fetch_initial_hotkey(client: &NekoClient, cx: &AsyncApp) -> HotkeyConfig {
    // The client and daemon start concurrently (`daemon_launcher` just
    // spawns the daemon and returns) — the daemon's socket may not be
    // listening yet on the very first launch, so retry briefly rather than
    // falling back to the default immediately and silently ignoring a
    // rebind the user made in a previous session.
    for _ in 0..25 {
        if let Ok(Response::Hotkey { config }) = client.request(Request::GetHotkey).await {
            return config;
        }
        cx.background_executor().timer(Duration::from_millis(40)).await;
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

async fn fetch_onboarding_state(client: &NekoClient, cx: &AsyncApp) -> OnboardingStateSnapshot {
    if std::env::var_os(RESET_ONBOARDING_ENV_VAR).is_some() {
        let _ = client.request(Request::SetOnboardingComplete { completed: false }).await;
    }
    for _ in 0..25 {
        if let Ok(Response::OnboardingState { completed, .. }) = client.request(Request::GetOnboardingState).await {
            return OnboardingStateSnapshot { completed };
        }
        cx.background_executor().timer(Duration::from_millis(40)).await;
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
    let offset = display_placement::upper_third_offset(display_bounds.size, panel_size);
    Bounds {
        origin: point(display_bounds.origin.x + offset.x, display_bounds.origin.y + offset.y),
        size: panel_size,
    }
}

/// Moves the resident summon window onto the display under the cursor,
/// right before it's activated — `upper_third` above only ever runs once,
/// at the initial `open_window` call, so without this every later summon
/// keeps reopening on whichever display was primary at process launch
/// (`data/neko-audit/report.md` Part 4 item 9). Best-effort: on any error
/// (no `NSScreen` available, off the main thread, ...) the window simply
/// stays wherever it already was rather than failing the summon.
fn reposition_to_cursor_display(window: &gpui::Window) {
    // `PANEL_WIDTH_WITH_DETAIL_PX` — the real window's own fixed size
    // (`AGENTS.md`, "Mode view resize seam"), not whichever mode the panel
    // happens to be in; `upper_third_offset` positions the window itself,
    // and the window never changes size.
    let panel_size = size(px(theme::PANEL_WIDTH_WITH_DETAIL_PX), px(panel::PANEL_HEIGHT_PX));
    if let Err(e) = display_placement::reposition_to_cursor_display(window, panel_size) {
        eprintln!("neko: could not reposition the summon window to the display under the cursor: {e}");
    }
}

gpui::actions!(neko, [DismissWindow]);
