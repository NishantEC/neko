mod components;
mod daemon_launcher;
mod hotkey_client;
mod material;
mod panel;
mod text_field;
mod theme;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use gpui::{
    App, Application, Bounds, Focusable, KeyBinding, Timer, WindowBounds, WindowKind,
    WindowOptions, point, px, size,
};
use neko_client::NekoClient;
use neko_protocol::{Event, HotkeyCombo, HotkeyConfig, Request, Response};

use hotkey_client::{HotkeyController, SystemRegistrar};
use panel::Root;

fn main() {
    daemon_launcher::ensure_daemon_running();

    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("backspace", text_field::Backspace, Some("TextField")),
            KeyBinding::new("left", text_field::Left, Some("TextField")),
            KeyBinding::new("right", text_field::Right, Some("TextField")),
            KeyBinding::new("down", panel::SelectNext, Some("Panel")),
            KeyBinding::new("up", panel::SelectPrevious, Some("Panel")),
            KeyBinding::new("enter", panel::Confirm, Some("Panel")),
            KeyBinding::new("escape", DismissWindow, None),
        ]);
        cx.on_action(|_: &DismissWindow, cx| cx.hide());

        let (client, event_rx) = NekoClient::connect(neko_protocol::socket_path());

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
                |_window, cx| Root::new(client.clone(), cx),
            )
            .expect("failed to open the summon window");

        let visible = Rc::new(Cell::new(false));

        cx.spawn(async move |cx| {
            let config = fetch_initial_hotkey(&client).await;

            let registrar = SystemRegistrar::new()
                .expect("failed to talk to the OS hotkey service");
            let mut controller = HotkeyController::new(registrar);
            controller
                .apply_initial(&config)
                .unwrap_or_else(|e| {
                    panic!(
                        "failed to register the summon hotkey {}: {e:?} — is it already bound to another app?",
                        config.combo.display()
                    )
                });

            loop {
                if let Ok(hotkey_event) = GlobalHotKeyEvent::receiver().try_recv()
                    && Some(hotkey_event.id) == controller.current_hotkey_id()
                    && hotkey_event.state == HotKeyState::Pressed
                {
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

                while let Ok(daemon_event) = event_rx.try_recv() {
                    match daemon_event {
                        Event::HotkeyChanged { config } => {
                            if let Err(e) = controller.apply_remote_change(&config) {
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
