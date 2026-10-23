## One client, and why "still not fixed" was true

**Two clients is a silent failure, and it cost two round trips before it was
fixed structurally.** Launch at login starts a client; rebuilding and
relaunching during development starts another. Both open a panel, both
install a menu bar item, and both attempt the summon hotkey — of which
exactly one registration receives the keypress, and *which one is not
observable from the outside*. So ⌥Space showed a build from the previous
evening while the current one sat behind it, and a fix that had been measured
and shipped simply did not appear. This file had already recorded it as
something to remember; a remember-to-check mitigation failed the second time
too, exactly like the evidence-window focus hazard it resembles.

`crate::singleton` closes it. A socket beside the daemon's own
(`client.sock`, so an isolated `HOME` gets its own and evidence runs can
never meet the real client), and **the newest client wins** — the opposite of
`bind_singleton`'s rule for the daemon, deliberately: a daemon is resident
and nobody launches one on purpose, while a person launching a client is
asking for the one they just launched. A second client connects, the first
sees the accept and stands down through the same 20ms poll every other flag
rides, and the newcomer binds. Bounded wait (`RELEASE_TIMEOUT`) so a wedged
predecessor cannot stop its replacement, and **evidence runs never
participate at all** — a throwaway client quitting the captain's real one
would be far worse than the bug being fixed.

Verified by starting two in sequence: the first logged "a newer client
started — standing down" and exited, the second holds the socket and the
hotkey, and `pgrep -x neko` shows one.

**The debugging lesson is the reusable part.** "Still not fixed" after a
measured fix has three candidate explanations and they are worth checking in
this order: the wrong binary is running, the fix addressed one of several
overlapping causes (the panel's top-edge line took three), or the
measurement could not see the thing being reported. Here it was the first,
and one `pgrep` with `lsof -d cwd` would have caught it before any code was
written.
