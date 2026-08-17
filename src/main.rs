mod text_field;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use gpui::{
    App, Application, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Render, Timer,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, actions, div,
    prelude::*, px, rgb, size,
};

use text_field::TextField;

actions!(neko, [DismissWindow]);

/// The whole visible surface: a rounded panel around the text field.
struct Root {
    text_field: Entity<TextField>,
}

impl Focusable for Root {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.text_field.focus_handle(cx)
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .bg(rgb(0x1e1e22))
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x35353c))
            .shadow_lg()
            .px_4()
            .text_color(rgb(0xf2f2f5))
            .text_size(px(20.))
            .child(self.text_field.clone())
    }
}

const SUMMON_WIDTH: f32 = 640.;
const SUMMON_HEIGHT: f32 = 64.;

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("backspace", text_field::Backspace, Some("TextField")),
            KeyBinding::new("left", text_field::Left, Some("TextField")),
            KeyBinding::new("right", text_field::Right, Some("TextField")),
            KeyBinding::new("escape", DismissWindow, None),
        ]);
        // App stays resident and the window is only ever hidden, never
        // destroyed — see AGENTS.md for why that matters for summon latency.
        cx.on_action(|_: &DismissWindow, cx| cx.hide());

        let bounds = Bounds::centered(None, size(px(SUMMON_WIDTH), px(SUMMON_HEIGHT)), cx);
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
                    window_background: WindowBackgroundAppearance::Opaque,
                    ..Default::default()
                },
                |_window, cx| {
                    let text_field = TextField::new(cx);
                    cx.new(|_| Root { text_field })
                },
            )
            .expect("failed to open the summon window");

        let manager = GlobalHotKeyManager::new().expect("failed to talk to the OS hotkey service");
        let summon_hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
        manager
            .register(summon_hotkey)
            .expect("failed to register ⌥Space — is it already bound to another app?");
        let summon_hotkey_id = summon_hotkey.id;

        let visible = Rc::new(Cell::new(false));

        cx.spawn(async move |cx| {
            // The manager must outlive every hotkey it registered, so it
            // lives for as long as this poll loop does: the app's lifetime.
            let _manager = manager;
            loop {
                if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv()
                    && event.id == summon_hotkey_id
                    && event.state == HotKeyState::Pressed
                {
                    let pressed_at = Instant::now();
                    let opening = !visible.get();
                    visible.set(opening);
                    let _ = cx.update(|cx| {
                        if opening {
                            let _ = window.update(cx, |root, window, cx| {
                                window.activate_window();
                                window.focus(&root.text_field.focus_handle(cx));
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
                Timer::after(Duration::from_millis(20)).await;
            }
        })
        .detach();
    });
}
