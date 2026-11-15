//! Read-only AppKit dump of the process's windows, run on the UI thread (`AppHandle::update`).
//! Used to check the overlay properties the spike cares about from the inside: level, Spaces
//! behaviour, click-through, key/main status. Rune exposes none of these as getters.

use objc2_app_kit::NSApplication;
use objc2_foundation::MainThreadMarker;

pub fn dump() {
    let Some(mtm) = MainThreadMarker::new() else {
        println!("diag: not on the main thread");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: read-only property getters on the main thread.
    unsafe {
        println!("diag app active={} policy={:?}", app.isActive(), app.activationPolicy());
        for w in app.windows().iter() {
            // Collection behaviour bits: 1 CanJoinAllSpaces, 16 Stationary, 64 IgnoresCycle, 256 FullScreenAuxiliary, 262144 CanJoinAllApplications.
            println!(
                "diag win={} title={:?} level={} behavior={:#x} ignoresMouse={} key={} main={} canKey={} visible={} alpha={:.2} opaque={} shadow={} style={:#x}",
                w.windowNumber(), w.title().to_string(), w.level(), w.collectionBehavior().0, w.ignoresMouseEvents(), w.isKeyWindow(),
                w.isMainWindow(), w.canBecomeKeyWindow(), w.isVisible(), w.alphaValue(), w.isOpaque(), w.hasShadow(), w.styleMask().0,
            );
        }
    }
}
