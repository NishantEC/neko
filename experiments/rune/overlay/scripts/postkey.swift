// Posts one key chord (default ctrl+opt+shift+space) as HID events, to test the Carbon global hotkey.
// The chord is registered by the app under test, so a working hotkey consumes it before any app sees it.
// Needs no permission to build; posting may need Accessibility, in which case nothing happens.
import CoreGraphics
import Foundation

let keycode: CGKeyCode = 49 // space
let flags: CGEventFlags = [.maskControl, .maskAlternate, .maskShift]
let src = CGEventSource(stateID: .hidSystemState)
let t0 = DispatchTime.now().uptimeNanoseconds
for down in [true, false] {
    let e = CGEvent(keyboardEventSource: src, virtualKey: keycode, keyDown: down)
    e?.flags = flags
    e?.post(tap: .cghidEventTap)
}
print("posted chord at uptime_ns=\(t0)")
