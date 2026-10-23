## Ambient awareness: the attention poll and the Dock badge

L3 of `docs/plan-agent-control-plane.md`. The inbox's rows arrive through the
ordinary search path, which covers every moment the panel is **open** — and
the panel is hidden almost all of the time. An agent that blocks while you
are working elsewhere would wait until your next summon to be noticed.

`server::run_attention_poll` (a `neko-daemon` thread, `ATTENTION_POLL_INTERVAL`
= **1.2s, deliberately shorter than `permissions::CACHE_TTL`'s 1.5s**) does
two jobs. The relationship between those two numbers is the feature: at the
5s it originally shipped with, the cache was warm for 1.5 seconds out of
every 5, so `defers_for` found it cold most of the time and the inbox landed
in the panel's *second* frame — L3 working one second in three. Caught by
timing a root query on a freshly started daemon and getting two frames back;
now **12 of 12 root queries answer in one frame**, where it was intermittent.
The TTL bounds how stale an answer may be before it is refetched on demand;
the poll interval bounds how long the cache may sit cold. Measured at the new
cadence: **0.0–0.2% CPU, 14.3 MB RSS**, flat.

- **Keeps the inbox cache warm**, which is what makes
  `PermissionsProvider::defers_for` answer `false` — deferring exists to keep
  a socket round trip off the fast path, and a warm cache has no round trip.
  Verified live: an empty root query went from **two frames to one**, so
  blocked agents land in the panel's *first* paint.
- **Broadcasts `Event::AttentionChanged { count }` on change only.** A client
  repainting every five seconds forever would be a wakeup per tick for a
  number that is almost always the same. It rediscovers the daemon each tick
  rather than holding it, because Paseo restarts often and a poller that
  resolved once would stay silently dead; and if Paseo disappears while
  agents were waiting it broadcasts `0`, since a stale badge is a standing
  claim that is no longer true.

Measured cost with the poller running: **0.0% CPU, 15 MB RSS** steady.

**`crate::dock_badge` is the only ambient surface neko actually has**, and
the alternatives were checked rather than assumed. `UNUserNotificationCenter`
needs a bundle identifier and neko is a bare Mach-O — the same fact that
sends "Launch at login" through a LaunchAgent plist. A menu-bar item is
equally unavailable: gpui's `status_item.rs` is never wired into a `mod`
declaration, so it is dead code rather than an API. `NSApp.dockTile.badgeLabel`
needs neither, and gpui hardcodes `NSApplicationActivationPolicyRegular`, so
this app has a Dock icon whether it wants one or not.

**Zero is no badge, not a badge reading zero** — a badge claims something
needs doing, and the resting state of the world does not deserve a permanent
mark. Past 99 it reads `99+`: nobody acts differently on 100 than on 140.

**The badge cannot be photographed** — it is on the Dock, and this repo
permits only window-scoped `screencapture -l<windowid>`. So `set_waiting_count`
reads the tile back and returns what it says, the same "verified, not
trusted" discipline `material::verify_installed` follows. `NEKO_DOCK_BADGE=<n>`
drives it; proven live: `3` → `Some("3")`, `0` → `None`, `250` → `Some("99+")`.

**Heartbeats are not buildable from neko, and this was tested rather than
assumed.** `create_heartbeat` exists in the tool list and Paseo answers a
top-level caller with `create_heartbeat requires an agent-scoped session` —
it sends a prompt to *the calling agent*, and neko is not one. The plan named
it; the honest outcome is that it is out of reach until neko has an agent
identity of its own.
