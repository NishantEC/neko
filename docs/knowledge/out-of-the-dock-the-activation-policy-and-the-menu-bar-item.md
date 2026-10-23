## Out of the Dock: the activation policy and the menu bar item

**A command palette does not belong in the Dock**, and neko was in it for one
reason: gpui's mac backend calls
`setActivationPolicy(NSApplicationActivationPolicyRegular)` on every app it
starts (`gpui_macos/src/platform.rs:1253`). Because neko is a bare Mach-O with
no bundle, the tile it got was a generic `exec` block with no icon at all.

**This file used to say that was unfixable** — "there is no supported way to
hide the Dock icon via GPUI's public API, so 'no dock icon' isn't achievable
without patching GPUI itself". That is wrong, and the correction is worth
keeping: it does not need GPUI's API. `setActivationPolicy` is an ordinary
runtime call reachable from `objc2-app-kit`, and `menu_bar::hide_from_dock`
makes it **after** gpui has had its say — last call wins. No fork patch, no
bundle. Verified by reading the policy back, the same discipline
`material::verify_installed` follows.

**The Dock icon was doing two real jobs, and hiding it without replacing them
would have been a regression.** Both moved to an `NSStatusItem`:

- **The attention count.** `Event::AttentionChanged` used to badge the Dock
  tile, and an accessory app has no Dock tile. It is the menu bar button's
  own title now. Same rule as before — zero is *no* label, not a label
  reading zero — but **no `99+` cap**, because that cap only existed for a
  Dock badge's small fixed circle and the menu bar grows to fit.
- **Click to summon**, which was `App::on_reopen` and is the documented
  "never a dead end" path for somebody who declined Accessibility and has no
  hotkey. `summon_from_outside` is now a named function both callers share,
  so the two cannot drift; `on_reopen` is kept because it costs nothing and
  would come back to life the day neko ships as a bundle.

**The click reaches gpui through a flag, deliberately.** An AppKit action
fires inside the run loop with no `&mut App` in reach and no supported way to
conjure one, so the action sets an `AtomicBool` and `main.rs`'s existing 20ms
poll — already there for daemon events and connection state — acts on it. No
new thread, no second summon path, and 20ms is well under what a person can
perceive. `menu_bar::take_click` swaps rather than reads, so a flag left set
cannot summon on every tick forever.

**The icon is neko's own mark, drawn as a template image.** It was an SF
Symbol (`command`) first, on the reasoning that a symbol auto-tints and an
asset would not. Only the second half was true: a **template** `NSImage`
tints identically, so `menu_bar::mark_image` draws the mark and the symbol
stays as the fallback. See "The mark" below. A name the running OS
does not have returns `nil`, and a status item with neither image nor title
is a live, clickable, completely blank gap in the menu bar — the worst
failure available, because nothing looks wrong. `install` falls back to a text
title and the launch log states which it got.

Verified live: `activation policy Accessory (verified)`, `menu bar item
installed — title "", image set`, `System Events` reporting neko is **not** a
foreground process, and `NEKO_MENU_BAR_COUNT=3` putting the count beside the
icon.
