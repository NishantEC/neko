//! Question 1: can a WKWebView live inside a Rune window, in a region Rune lays out?
//!
//! `webview [child|panel] [selftest]`
//!   child  (default): the WKWebView is a subview of the window's content view, framed to the rect
//!                     `El::on_layout` reports for the keyed slot (after scrolling and resizing).
//!   panel:            the WKWebView is the content of a borderless NSPanel added as a child window.
//!   selftest:         drives synthetic AppKit events through `NSWindow::sendEvent` (no OS focus
//!                     needed), prints findings as `FINDING ...` lines, takes screenshots, quits.

use std::cell::RefCell;
use std::process::Command;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType};
use objc2_app_kit::{
    NSBackingStoreType, NSEvent, NSEventModifierFlags, NSEventType, NSPanel, NSView, NSWindow, NSWindowOrderingMode,
    NSWindowStyleMask,
};
use objc2_foundation::{CGFloat, MainThreadMarker, NSPoint, NSRange, NSRect, NSSize, NSString};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};
use raw_window_handle::RawWindowHandle;
use rune::{AppSpec, Chrome, Cx, El, ScrollSpec, TextEdit, Tx, col, el, row, text};
use rune_kit::{Button, ButtonVariant, Kit, Select, TextArea};

const HTML: &str = include_str!("../../assets/canvas.html");
const SLOT: &str = "web-slot";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Child,
    Panel,
}

struct Native {
    mode: Mode,
    web: Retained<WKWebView>,
    host: Retained<NSView>,
    window: Retained<NSWindow>,
    panel: Option<Retained<NSPanel>>,
}

thread_local! {
    static NATIVE: RefCell<Option<Native>> = const { RefCell::new(None) };
}

fn mode() -> Mode {
    if rune::extra_args().iter().any(|a| a == "panel") { Mode::Panel } else { Mode::Child }
}

fn selftest() -> bool {
    rune::extra_args().iter().any(|a| a == "selftest")
}

struct State {
    presses: u32,
    open: bool,
    choice: Option<String>,
    composer: TextEdit,
    slot: [f32; 4],
    t0: f64,
    step: u32,
}

fn init() -> State {
    let mut composer = TextEdit::new("");
    composer.focused = false;
    State { presses: 0, open: false, choice: None, composer, slot: [0.0; 4], t0: -1.0, step: 0 }
}

const MODELS: [&str; 4] = ["Opus", "Sonnet", "Haiku", "Local"];

fn view(s: &State, _cx: &mut Cx<State>) -> El {
    let k = Kit::neutral(true);
    let mut layer = Vec::new();
    let select = Select::new("model", &MODELS)
        .current(s.choice.as_deref())
        .placeholder("Model")
        .width(140.0)
        .open(s.open)
        .on_toggle(|s: &mut State| s.open = !s.open)
        .on_close(|s: &mut State| s.open = false)
        .on_choose(|s: &mut State, v: &str, _: &mut Tx| {
            s.choice = Some(v.to_owned());
            s.open = false;
        })
        .build(&k, &mut layer);
    let toolbar = row()
        .gap(10.0)
        .padding(10.0)
        .items_center()
        .child(select.key("select"))
        .child(
            Button::new(format!("Rune button pressed {}", s.presses))
                .variant(ButtonVariant::Neutral)
                .on_press(|s: &mut State| s.presses += 1)
                .build(&k)
                .key("rune-button"),
        )
        .child(text(format!("slot {:.0},{:.0} {:.0}x{:.0}", s.slot[0], s.slot[1], s.slot[2], s.slot[3])).color(k.c.text_muted));
    // The slot sits first in a scrolling column so scrolling moves it under the toolbar.
    let slot = el()
        .key(SLOT)
        .w_full()
        .h(330.0)
        .fill(rune::oklch(0.3, 0.05, 250.0))
        .on_layout_tx(|s: &mut State, r, tx: &mut Tx| {
            s.slot = r;
            sync_frame(r);
            tx.live_only();
        });
    let mut body = col().w_full().gap(8.0).padding(10.0).child(slot);
    for i in 0..40 {
        body = body.child(text(format!("Rune text row {i}: native content scrolling with the slot")).color(k.c.text_base));
    }
    let scroller = col().key("scroller").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body);
    let composer = TextArea::new(&s.composer)
        .placeholder("Rune composer (IME into Rune)")
        .rows(2)
        .on_edit(|s: &mut State| &mut s.composer)
        .build(&k)
        .key("composer");
    let base = col().w_full().h_full().fill(k.c.bg_base).child(toolbar).child(scroller).child(col().padding(10.0).child(composer));
    rune_kit::root(base, layer)
}

/// Window points (top-left origin, y down) to the host's own coordinate space.
fn sync_frame(r: [f32; 4]) {
    NATIVE.with(|n| {
        let n = n.borrow();
        let Some(n) = n.as_ref() else { return };
        let (x, y, w, h) = (r[0] as CGFloat, r[1] as CGFloat, r[2] as CGFloat, r[3] as CGFloat);
        match n.mode {
            Mode::Child => {
                let bounds = n.host.bounds();
                let flipped = n.host.isFlipped();
                let fy = if flipped { y } else { bounds.size.height - y - h };
                unsafe { n.web.setFrame(NSRect::new(NSPoint::new(x, fy), NSSize::new(w, h))) };
            }
            Mode::Panel => {
                let bounds = n.host.bounds();
                let fy = if n.host.isFlipped() { y } else { bounds.size.height - y - h };
                let in_window = n.host.convertRect_toView(NSRect::new(NSPoint::new(x, fy), NSSize::new(w, h)), None);
                let on_screen = n.window.convertRectToScreen(in_window);
                if let Some(p) = &n.panel {
                    p.setFrame_display(on_screen, true);
                }
            }
        }
    });
}

fn install(handle: &rune::WindowHandle) {
    let mtm = MainThreadMarker::new().expect("on_window runs on the main thread");
    let result = handle.with_raw_handle(|h| match h.as_raw() {
        RawWindowHandle::AppKit(a) => Some(a.ns_view),
        _ => None,
    });
    let Ok(Some(ptr)) = result else { eprintln!("no AppKit handle"); return };
    // SAFETY: winit's NSView, alive while the window is; we retain it right away.
    let host: Retained<NSView> = unsafe { Retained::retain(ptr.as_ptr().cast::<NSView>()).expect("ns_view") };
    let window = host.window().expect("view is in a window");
    let config = unsafe { WKWebViewConfiguration::new() };
    let web = unsafe { WKWebView::initWithFrame_configuration(mtm.alloc(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 10.0)), &config) };
    unsafe { web.loadHTMLString_baseURL(&NSString::from_str(HTML), None) };
    let m = mode();
    let mut panel = None;
    match m {
        Mode::Child => unsafe { host.addSubview(&web) },
        Mode::Panel => {
            let p = unsafe {
                NSPanel::initWithContentRect_styleMask_backing_defer(
                    mtm.alloc(),
                    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 10.0)),
                    NSWindowStyleMask::Borderless,
                    NSBackingStoreType::NSBackingStoreBuffered,
                    false,
                )
            };
            p.setContentView(Some(&web));
            unsafe { window.addChildWindow_ordered(&p, NSWindowOrderingMode::NSWindowAbove) };
            p.orderFront(None);
            panel = Some(p);
        }
    }
    NATIVE.with(|n| *n.borrow_mut() = Some(Native { mode: m, web, host, window, panel }));
    println!("INSTALLED mode={}", if m == Mode::Child { "child" } else { "panel" });
}

fn class_of(v: Option<&AnyObject>) -> String {
    v.map_or("nil".into(), |o| o.class().name().to_owned())
}

fn title() -> String {
    NATIVE.with(|n| n.borrow().as_ref().and_then(|n| unsafe { n.web.title() }).map(|t| t.to_string()).unwrap_or_default())
}

/// A point in the slot or elsewhere, as window coordinates (bottom-left origin).
fn window_point(host: &NSView, x: f32, y: f32) -> NSPoint {
    let b = host.bounds();
    let p = if host.isFlipped() { NSPoint::new(x as f64, y as f64) } else { NSPoint::new(x as f64, b.size.height - y as f64) };
    unsafe { host.convertPoint_toView(p, None) }
}

fn click(window: &NSWindow, p: NSPoint, n: isize) {
    for ty in [NSEventType::LeftMouseDown, NSEventType::LeftMouseUp] {
        let ev = unsafe {
            NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                ty, p, NSEventModifierFlags::empty(), 0.0, window.windowNumber(), None, n, 1, 1.0,
            )
        };
        if let Some(ev) = ev {
            window.sendEvent(&ev);
        }
    }
}

fn key(window: &NSWindow, chars: &str, code: u16) {
    for ty in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let ev = unsafe {
            NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                ty,
                NSPoint::new(0.0, 0.0),
                NSEventModifierFlags::empty(),
                0.0,
                window.windowNumber(),
                None,
                &NSString::from_str(chars),
                &NSString::from_str(chars),
                false,
                code,
            )
        };
        if let Some(ev) = ev {
            window.sendEvent(&ev);
        }
    }
}

fn shot(window: &NSWindow, name: &str) {
    let out = format!("/tmp/rune-spike-shots/{name}.png");
    let _ = std::fs::create_dir_all("/tmp/rune-spike-shots");
    let st = Command::new("/usr/sbin/screencapture").args(["-x", "-o", "-l", &unsafe { window.windowNumber() }.to_string(), &out]).status();
    println!("FINDING screenshot {out} {:?}", st.map(|s| s.code()));
}

/// Feed marked text then a commit to whatever the window's first responder is, as an input method does.
fn ime(window: &NSWindow, marked: &str, commit: &str) -> String {
    let Some(r) = window.firstResponder() else { return "no first responder".into() };
    let cls = r.class().name().to_owned();
    let none = NSRange::new(usize::MAX, 0);
    unsafe {
        let m = NSString::from_str(marked);
        let c = NSString::from_str(commit);
        let ok: bool = msg_send![&*r, respondsToSelector: objc2::sel!(setMarkedText:selectedRange:replacementRange:)];
        if !ok {
            return format!("{cls} does not implement NSTextInputClient");
        }
        let _: () = msg_send![&*r, setMarkedText: &*m, selectedRange: NSRange::new(marked.chars().count(), 0), replacementRange: none];
        let _: () = msg_send![&*r, unmarkText];
        let _: () = msg_send![&*r, insertText: &*c, replacementRange: none];
    }
    format!("{cls} took marked={marked:?} commit={commit:?}")
}

fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    if !selftest() {
        return false;
    }
    tx.wake_after(std::time::Duration::from_millis(50));
    if s.t0 < 0.0 {
        s.t0 = t;
    }
    let dt = t - s.t0;
    let at = |step: u32, when: f64, s: &State| s.step == step && dt > when;
    let (win, host, web) = match NATIVE.with(|n| n.borrow().as_ref().map(|n| (n.window.clone(), n.host.clone(), n.web.clone()))) {
        Some(x) => x,
        None => return false,
    };
    if at(0, 1.5, s) {
        s.step = 1;
        println!("FINDING slot rect {:?}", s.slot);
        let center = |r: [f32; 4]| (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        let (cx, cy) = center(s.slot);
        let hit = |x: f32, y: f32| {
            let b = host.bounds();
            let p = if host.isFlipped() { NSPoint::new(x as f64, y as f64) } else { NSPoint::new(x as f64, b.size.height - y as f64) };
            // hitTest takes a point in the receiver's superview's coordinates.
            let v = unsafe {
                let sup = host.superview();
                let q = host.convertPoint_toView(p, sup.as_deref());
                host.hitTest(q)
            };
            class_of(v.as_deref().map(|v| &**v as &AnyObject))
        };
        println!("FINDING hit_test slot centre -> {}", hit(cx, cy));
        println!("FINDING hit_test above slot (toolbar) -> {}", hit(30.0, 12.0));
        println!("FINDING hit_test below slot (Rune text) -> {}", hit(30.0, s.slot[1] + s.slot[3] + 40.0));
        shot(&win, "01-composited");
        // Click on the web canvas and read its counter back through document.title.
        click(&win, window_point(&host, cx, cy), 1);
        return true;
    }
    if at(1, 2.2, s) {
        s.step = 2;
        println!("FINDING after click in slot, page title = {}", title());
        println!("FINDING Rune presses before = {}", s.presses);
        // Rune button: find its rect by key.
        return true;
    }
    if at(2, 2.4, s) {
        s.step = 3;
        // Open the Rune select, whose list overlaps the slot, and capture whether it draws above.
        s.open = true;
        let _ = tx;
        return true;
    }
    if at(3, 3.2, s) {
        s.step = 4;
        shot(&win, "02-dropdown-open");
        s.open = false;
        return true;
    }
    if at(4, 3.8, s) {
        s.step = 5;
        // Keyboard focus: make the web view first responder, type, read the input value back.
        let ok = win.makeFirstResponder(Some(&web));
        println!("FINDING makeFirstResponder(web) = {ok}, firstResponder = {}", class_of(win.firstResponder().as_deref().map(|v| &*v as &AnyObject)));
        // Focus the page's <input> via a click at its bar (bottom 30% strip).
        let (x, y, w, h) = (s.slot[0], s.slot[1], s.slot[2], s.slot[3]);
        click(&win, window_point(&host, x + w * 0.2, y + h - 12.0), 1);
        return true;
    }
    if at(5, 4.6, s) {
        s.step = 6;
        println!("FINDING firstResponder after click on web input = {}", class_of(win.firstResponder().as_deref().map(|v| &*v as &AnyObject)));
        key(&win, "h", 4);
        key(&win, "i", 34);
        return true;
    }
    if at(6, 5.4, s) {
        s.step = 7;
        println!("FINDING after typing 'hi' page title = {}", title());
        println!("FINDING web IME: {}", ime(&win, "にほん", "日本"));
        return true;
    }
    if at(7, 6.2, s) {
        s.step = 8;
        println!("FINDING after IME page title = {}", title());
        // Now to Rune's composer: click it, then type and IME.
        s.composer.focused = true;
        return true;
    }
    if at(8, 6.8, s) {
        s.step = 9;
        println!("FINDING firstResponder after Rune composer focus = {}", class_of(win.firstResponder().as_deref().map(|v| &*v as &AnyObject)));
        key(&win, "r", 15);
        println!("FINDING Rune IME: {}", ime(&win, "ちゅう", "中"));
        return true;
    }
    if at(9, 7.6, s) {
        s.step = 10;
        println!("FINDING Rune composer text = {:?}", s.composer.text);
        shot(&win, "03-final");
        tx.quit();
        return true;
    }
    false
}

fn main() {
    let chrome = Chrome { full_size_content: false, traffic_lights: None, drag_regions: false };
    let spec = AppSpec { poll: Some(poll), ..AppSpec::new("Rune + WKWebView", (900.0, 640.0), init, view) };
    rune::App::new(spec).chrome(chrome).on_window(install).run();
}
