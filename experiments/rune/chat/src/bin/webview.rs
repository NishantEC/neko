//! Question 1: can a WKWebView live inside a Rune window, in a region Rune lays out?
//!
//! `webview [child|panel|under] [hide] [selftest]`
//!   child  (default): the WKWebView is a subview of the window's content view, framed to the rect
//!                     `El::on_layout` reports for the keyed slot (after scrolling and resizing).
//!   panel:            the WKWebView is the content of a borderless NSPanel added as a child window.
//!   under:            transparent window, the WKWebView is a sibling placed BELOW the content view.
//!   hide:             hide the WKWebView while a Rune overlay (the model dropdown) is open.
//!   selftest:         drives synthetic AppKit events through `NSWindow::sendEvent` (no OS focus
//!                     needed), prints `FINDING ...` lines, takes screenshots, quits.

use std::cell::RefCell;
use std::process::Command;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, msg_send_id};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSEventModifierFlags, NSEventType, NSPanel, NSView, NSWindow, NSWindowOrderingMode,
    NSWindowStyleMask,
};
use objc2_foundation::{CGFloat, MainThreadMarker, NSPoint, NSRange, NSRect, NSSize, NSString};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};
use raw_window_handle::RawWindowHandle;
use rune::{AppSpec, Chrome, Cx, El, ScrollSpec, TextEdit, Tx, WindowOptions, col, el, row, text};
use rune_kit::{Button, ButtonVariant, Kit, Select, TextArea};

const HTML: &str = include_str!("../../assets/canvas.html");
const SLOT: &str = "web-slot";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Child,
    Panel,
    Under,
}

struct Native {
    mode: Mode,
    web: Retained<WKWebView>,
    host: Retained<NSView>,
    window: Retained<NSWindow>,
    panel: Option<Retained<NSPanel>>,
    /// Child mode: a clipping container so the web view is cut off at the Rune scroll viewport.
    clip: Option<Retained<NSView>>,
}

thread_local! {
    static RECTS: std::cell::Cell<([f32; 4], [f32; 4])> = const { std::cell::Cell::new(([0.0; 4], [0.0; 4])) };
    static NATIVE: RefCell<Option<Native>> = const { RefCell::new(None) };
}

fn flag(name: &str) -> bool {
    rune::extra_args().iter().any(|a| a == name)
}

fn mode() -> Mode {
    if flag("panel") {
        Mode::Panel
    } else if flag("under") {
        Mode::Under
    } else {
        Mode::Child
    }
}

struct State {
    presses: u32,
    open: bool,
    choice: Option<String>,
    composer: TextEdit,
    slot: [f32; 4],
    composer_rect: [f32; 4],
    t0: f64,
    step: u32,
}

fn init() -> State {
    State {
        presses: 0,
        open: false,
        choice: None,
        composer: TextEdit::new(""),
        slot: [0.0; 4],
        composer_rect: [0.0; 4],
        t0: -1.0,
        step: 0,
    }
}

const MODELS: [&str; 4] = ["Opus", "Sonnet", "Haiku", "Local"];

fn view(s: &State, _cx: &mut Cx<State>) -> El {
    let under = mode() == Mode::Under;
    if flag("hide") {
        set_hidden(s.open);
    }
    let k = Kit::neutral(true);
    // A transparent window shows the desktop wherever Rune draws nothing, so in `under` mode every
    // region fills itself and only the slot is left clear.
    let paint = |e: El| if under { e.fill(k.c.bg_base) } else { e };
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
    let toolbar = paint(row().w_full().gap(10.0).padding(10.0).items_center())
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
    let slot = el().key(SLOT).w_full().h(330.0).on_layout_tx(|s: &mut State, r, tx: &mut Tx| {
        s.slot = r;
        RECTS.with(|c| c.set((r, c.get().1)));
        sync_frame();
        tx.live_only();
    });
    let slot = if under { slot } else { slot.fill(rune::oklch(0.3, 0.05, 250.0)) };
    let mut body = col().w_full().gap(8.0).padding(10.0).child(slot);
    for i in 0..40 {
        body = body.child(paint(row().key(format!("row-{i}")).w_full()).child(text(format!("Rune text row {i}: native content scrolling with the slot")).color(k.c.text_base)));
    }
    let scroller = col()
        .key("scroller")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .on_layout_tx(|_: &mut State, r, tx: &mut Tx| {
            RECTS.with(|c| c.set((c.get().0, r)));
            sync_frame();
            tx.live_only();
        })
        .child(body);
    let composer = TextArea::new(&s.composer)
        .placeholder("Rune composer (IME into Rune)")
        .rows(2)
        .on_edit(|s: &mut State| &mut s.composer)
        .build(&k)
        .key("composer")
        .on_layout_tx(|s: &mut State, r, tx: &mut Tx| {
            s.composer_rect = r;
            tx.live_only();
        });
    let base = paint(col().w_full().h_full()).child(toolbar).child(scroller).child(paint(col().padding(10.0)).child(composer));
    rune_kit::root(base, layer)
}

fn set_hidden(hidden: bool) {
    NATIVE.with(|n| {
        if let Some(n) = n.borrow().as_ref() {
            unsafe {
                if n.web.isHidden() != hidden {
                    n.web.setHidden(hidden);
                }
            }
        }
    });
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

/// Frame the web view to the slot (window points, top-left origin, y down), cut off at the viewport.
fn sync_frame() {
    let (slot, viewport) = RECTS.with(|c| c.get());
    NATIVE.with(|n| {
        let n = n.borrow();
        let Some(n) = n.as_ref() else { return };
        let bounds = n.host.bounds();
        let flipped = n.host.isFlipped();
        let to_host = |r: [f32; 4]| {
            let (x, y, w, h) = (r[0] as CGFloat, r[1] as CGFloat, r[2] as CGFloat, r[3] as CGFloat);
            NSRect::new(NSPoint::new(x, if flipped { y } else { bounds.size.height - y - h }), NSSize::new(w, h))
        };
        match n.mode {
            Mode::Child => unsafe {
                let Some(clip) = &n.clip else { return };
                match if viewport[2] > 0.0 { intersect(slot, viewport) } else { Some(slot) } {
                    Some(vis) => {
                        clip.setHidden(false);
                        clip.setFrame(to_host(vis));
                        // The web view keeps the slot's full size; the clip shows the visible part.
                        let rel = [slot[0] - vis[0], slot[1] - vis[1], slot[2], slot[3]];
                        let (cw, ch) = (vis[2] as CGFloat, vis[3] as CGFloat);
                        n.web.setFrame(NSRect::new(
                            NSPoint::new(rel[0] as CGFloat, ch - rel[1] as CGFloat - rel[3] as CGFloat),
                            NSSize::new(rel[2] as CGFloat, rel[3] as CGFloat),
                        ));
                        let _ = cw;
                    }
                    None => clip.setHidden(true),
                }
            },
            Mode::Under => unsafe {
                let sup = n.host.superview();
                n.web.setFrame(n.host.convertRect_toView(to_host(slot), sup.as_deref()));
            },
            Mode::Panel => {
                let in_window = unsafe { n.host.convertRect_toView(to_host(slot), None) };
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
    let mut clip = None;
    match m {
        Mode::Child => unsafe {
            let c = NSView::initWithFrame(mtm.alloc(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 10.0)));
            c.setWantsLayer(true);
            let layer: Option<Retained<AnyObject>> = msg_send_id![&*c, layer];
            if let Some(layer) = layer {
                let _: () = msg_send![&*layer, setMasksToBounds: true];
            }
            c.addSubview(&web);
            host.addSubview(&c);
            clip = Some(c);
        },
        Mode::Under => unsafe {
            window.setOpaque(false);
            window.setBackgroundColor(Some(&NSColor::clearColor()));
            let layer: Option<Retained<AnyObject>> = msg_send_id![&*host, layer];
            if let Some(layer) = layer {
                let _: () = msg_send![&*layer, setOpaque: false];
            }
            let sup = host.superview().expect("superview");
            sup.addSubview_positioned_relativeTo(&web, NSWindowOrderingMode::NSWindowBelow, Some(&host));
        },
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
    NATIVE.with(|n| *n.borrow_mut() = Some(Native { mode: m, web, host, window, panel, clip }));
    println!("INSTALLED mode={}", match m { Mode::Child => "child", Mode::Panel => "panel", Mode::Under => "under" });
}

fn class_of(v: Option<&AnyObject>) -> String {
    v.map_or("nil".into(), |o| o.class().name().to_owned())
}

fn title() -> String {
    NATIVE.with(|n| n.borrow().as_ref().and_then(|n| unsafe { n.web.title() }).map(|t| t.to_string()).unwrap_or_default())
}

fn host_point(host: &NSView, x: f32, y: f32) -> NSPoint {
    let b = host.bounds();
    if host.isFlipped() { NSPoint::new(x as f64, y as f64) } else { NSPoint::new(x as f64, b.size.height - y as f64) }
}

/// A point in window points (top-left origin) as window coordinates (bottom-left origin).
fn window_point(host: &NSView, x: f32, y: f32) -> NSPoint {
    unsafe { host.convertPoint_toView(host_point(host, x, y), None) }
}

fn hit(host: &NSView, x: f32, y: f32) -> String {
    let v = unsafe {
        let sup = host.superview();
        let q = host.convertPoint_toView(host_point(host, x, y), sup.as_deref());
        // For `under`, the window's frame view decides; ask the superview for the deepest hit.
        match &sup {
            Some(s) if s.superview().is_some() => host.hitTest(q),
            Some(s) => s.hitTest(q),
            None => host.hitTest(q),
        }
    };
    class_of(v.as_deref().map(|v| &**v as &AnyObject))
}

fn click(window: &NSWindow, p: NSPoint) {
    for ty in [NSEventType::LeftMouseDown, NSEventType::LeftMouseUp] {
        let ev = unsafe {
            NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                ty, p, NSEventModifierFlags::empty(), 0.0, window.windowNumber(), None, 0, 1, 1.0,
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

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateScrollWheelEvent2(source: *const std::ffi::c_void, units: u32, count: u32, w1: i32, w2: i32, w3: i32) -> *mut std::ffi::c_void;
    fn CGEventSetLocation(event: *mut std::ffi::c_void, p: NSPoint);
}

/// A trackpad-style pixel scroll delivered at a window point (top-left origin).
fn scroll(window: &NSWindow, host: &NSView, x: f32, y: f32, dy: i32) {
    unsafe {
        let wp = window_point(host, x, y);
        let on_screen = window.convertPointToScreen(wp);
        // CGEvent locations are top-left origin on the main display.
        let screen_h = objc2_app_kit::NSScreen::screens(MainThreadMarker::new().unwrap()).objectAtIndex(0).frame().size.height;
        let cg = CGEventCreateScrollWheelEvent2(std::ptr::null(), 0, 1, dy, 0, 0);
        CGEventSetLocation(cg, NSPoint::new(on_screen.x, screen_h - on_screen.y));
        let ev: Option<Retained<NSEvent>> = msg_send_id![objc2::class!(NSEvent), eventWithCGEvent: cg];
        if let Some(ev) = ev {
            // Re-target the event at this window; the location comes from the CGEvent.
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

/// Where the web view really is, in window points with a top-left origin.
fn web_rect() -> [f32; 4] {
    NATIVE.with(|n| {
        let n = n.borrow();
        let Some(n) = n.as_ref() else { return [0.0; 4] };
        let (window_rect, win_h) = unsafe {
            let r = match n.mode {
                Mode::Panel => {
                    let screen = n.panel.as_ref().map(|p| p.frame()).unwrap_or_default();
                    n.window.convertRectFromScreen(screen)
                }
                _ => {
                    // The visible part: the clip container for child mode, else the web view.
                    let v: Retained<NSView> = match &n.clip {
                        Some(c) => c.clone(),
                        None => Retained::into_super(n.web.clone()),
                    };
                    let f = v.frame();
                    let sup = v.superview();
                    sup.map_or(f, |s| s.convertRect_toView(f, None))
                }
            };
            (r, n.window.contentView().map_or(0.0, |v| v.frame().size.height))
        };
        let top_offset = n.window.frame().size.height - win_h; // title bar for a titled window
        let _ = top_offset;
        [window_rect.origin.x as f32, (win_h - window_rect.origin.y - window_rect.size.height) as f32, window_rect.size.width as f32, window_rect.size.height as f32]
    })
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
        let _: () = msg_send![&*r, insertText: &*c, replacementRange: none];
    }
    format!("{cls} took marked={marked:?} commit={commit:?}")
}

fn first_responder(win: &NSWindow) -> String {
    class_of(win.firstResponder().as_deref().map(|v| &*v as &AnyObject))
}

fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    if !flag("selftest") {
        return false;
    }
    tx.wake_after(std::time::Duration::from_millis(50));
    if s.t0 < 0.0 {
        s.t0 = t;
    }
    let dt = t - s.t0;
    let (win, host, web) = match NATIVE.with(|n| n.borrow().as_ref().map(|n| (n.window.clone(), n.host.clone(), n.web.clone()))) {
        Some(x) => x,
        None => return false,
    };
    let go = |step: u32, when: f64, s: &State| s.step == step && dt > when;
    let centre = |r: [f32; 4]| (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    if go(0, 1.5, s) {
        s.step = 1;
        println!("FINDING slot rect (layout) {:?}  web view rect {:?}", s.slot, web_rect());
        let (cx, cy) = centre(s.slot);
        println!("FINDING hit_test slot centre -> {}", hit(&host, cx, cy));
        println!("FINDING hit_test toolbar -> {}", hit(&host, 30.0, 12.0));
        println!("FINDING hit_test below slot (Rune text) -> {}", hit(&host, 30.0, s.slot[1] + s.slot[3] + 40.0));
        shot(&win, "01-composited");
        click(&win, window_point(&host, cx, cy));
        return true;
    }
    if go(1, 2.2, s) {
        s.step = 2;
        println!("FINDING after click in slot, page title = {}", title());
        // Click Rune's button (left toolbar) and read Rune's state.
        click(&win, window_point(&host, 200.0, 30.0));
        return true;
    }
    if go(2, 2.8, s) {
        s.step = 3;
        println!("FINDING Rune button presses after click = {}", s.presses);
        s.open = true;
        return true;
    }
    if go(3, 3.6, s) {
        s.step = 4;
        shot(&win, "02-dropdown-open");
        s.open = false;
        return true;
    }
    if go(4, 4.2, s) {
        s.step = 5;
        // Click the page's <input>: in the strip below the canvas (70% of the slot's height).
        let (x, y, w, h) = (s.slot[0], s.slot[1], s.slot[2], s.slot[3]);
        let _ = w;
        click(&win, window_point(&host, x + 100.0, y + h * 0.7 + 18.0));
        return true;
    }
    if go(5, 5.0, s) {
        s.step = 6;
        println!("FINDING firstResponder after click on web <input> = {}", first_responder(&win));
        key(&win, "h", 4);
        key(&win, "i", 34);
        return true;
    }
    if go(6, 5.8, s) {
        s.step = 7;
        println!("FINDING after typing 'hi', page title = {}", title());
        println!("FINDING web IME: {}", ime(&win, "にほん", "日本"));
        return true;
    }
    if go(7, 6.6, s) {
        s.step = 8;
        println!("FINDING after IME, page title = {}", title());
        // Click Rune's composer; first responder should move back to Rune's view.
        let r = s.composer_rect;
        click(&win, window_point(&host, r[0] + r[2] / 2.0, r[1] + 14.0));
        return true;
    }
    if go(8, 7.4, s) {
        s.step = 9;
        println!("FINDING firstResponder after click on Rune composer = {}", first_responder(&win));
        key(&win, "r", 15);
        println!("FINDING Rune IME: {}", ime(&win, "ちゅう", "中"));
        return true;
    }
    if go(9, 8.2, s) {
        s.step = 10;
        println!("FINDING Rune composer text = {:?}", s.composer.text);
        shot(&win, "03-rune-composer");
        // Scroll Rune's column over a text row (below the slot) and compare the slot with the web view.
        println!("FINDING before scroll: slot {:?} web {:?}", s.slot, web_rect());
        let (x, y) = (300.0, s.slot[1] + s.slot[3] + 60.0);
        for _ in 0..6 {
            scroll(&win, &host, x, y, -30);
        }
        tx.scroll_into_view("row-8");
        return true;
    }
    if go(10, 9.4, s) {
        s.step = 11;
        println!("FINDING after scroll: slot {:?} web {:?}", s.slot, web_rect());
        shot(&win, "04-scrolled");
        win.setContentSize(NSSize::new(700.0, 560.0));
        return true;
    }
    if go(11, 10.4, s) {
        s.step = 12;
        println!("FINDING after resize to 700x560: slot {:?} web {:?}", s.slot, web_rect());
        shot(&win, "05-resized");
        let _ = &web;
        tx.quit();
        return true;
    }
    false
}

fn main() {
    let chrome = Chrome { full_size_content: false, traffic_lights: None, drag_regions: false };
    let spec = AppSpec { poll: Some(poll), ..AppSpec::new("Rune + WKWebView", (900.0, 640.0), init, view) };
    let options = WindowOptions { transparent: flag("under"), ..WindowOptions::default() };
    rune::App::new(spec).chrome(chrome).window_options(options).on_window(install).run();
}
