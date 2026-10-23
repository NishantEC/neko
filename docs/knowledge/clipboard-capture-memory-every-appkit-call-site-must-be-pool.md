## Clipboard capture memory: every AppKit call site must be pool-wrapped

The capture thread ran forever, on a plain background thread with no
`NSApplication`/`CFRunLoop`, calling `NSPasteboard`/`NSWorkspace` every
400ms without ever pushing an autorelease pool — measured taking the daemon
to 14+ GB resident (and 21+ GB of swap) in the wild. Root-caused,
fixed, and measured in `docs/evidence/clipboard-autoreleasepool-leak.md` —
read that file for the full diagnosis and the before/after numbers (same
stress, same machine, back to back: unpatched hit 19 GB resident and
climbing in 90 seconds; patched oscillated in a ~1-2.4 GB band over the
same window with no growth trend).

**The fix is structural, not just "remembered to wrap it this time."**
`clipboard.rs`'s `pasteboard` module exposes exactly two public entry
points into AppKit — `poll` and `write_string` — each wrapping its entire
body in `objc2::rc::autoreleasepool`; the raw `NSPasteboard`/`NSWorkspace`
calls are private functions only reachable from inside that pooled closure.
There is no unpooled path left into this module's AppKit calls for a future
change to fall into by accident — if you add a new AppKit call anywhere in
this daemon, it needs the same treatment, and the "audited every objc2 call
site" list in the evidence doc is where to add it. `icons.rs`'s
`extract_icon_png` (one-shot per app, not a forever-loop, so its own
version of this bug was bounded rather than unbounded) got the identical
wrap for the same reason. `apps.rs`'s Spotlight-based discovery (see
"Application discovery" above) has zero `objc2` usage — it shells out to
`mdfind` — so it was never part of this defect class; confirmed by grep,
not assumed. `crates/neko/src/material.rs` does call AppKit but runs in the
*client* process inside a real Cocoa run loop that drains its own pool
every cycle, and is a different task's file — out of scope here, not
touched.
