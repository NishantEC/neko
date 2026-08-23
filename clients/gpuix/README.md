# neko — GPUIX client (spike)

A second client for the same `neko-daemon`, written in React on
[GPUIX](https://github.com/remorses/gpuix) (React bindings for Zed's GPUI).

**The daemon is untouched.** It speaks length-prefixed JSON over a Unix socket
(`neko-protocol`), so a client in any language can drive it — the app index,
SQLite, ranking, clipboard capture and every provider stay in Rust. Both
clients can run against it at the same time, which is how this was tested.

```sh
npm install
node src/probe.mjs    # protocol only — prints real search results
node src/panel.mjs    # the panel
```

## What works

Live search with two-phase results, section grouping, arrow selection, and
Enter to activate — all against the real daemon.

## What does not exist yet

The global hotkey, and the window behaviour that makes a launcher a launcher:
GPUIX's `WindowOptions` is `title/width/height/minWidth/minHeight/resizable/
fullscreen/transparent/titlebarTransparent/windowBackground/trafficLight{X,Y}`
— there is **no window kind, no window level, and no non-activating panel**, so
a summon window would steal focus from whatever you were working in. Also
missing here: icons, the agent grid, modes, themes, preferences.

## Notes for whoever picks this up

- **Enter is `onSubmit` on the `input` element**, not `onKeyDown` on a
  container — a `div` never takes focus, so a key handler there never fires.
  `examples/chat.tsx` upstream is the reference.
- Key events arrive as
  `{elementId, eventType, key, keyChar, isHeld, modifiers:{shift,ctrl,alt,cmd}}`.
- No JSX here, so it runs on plain `node` with no build step. That is a
  property of the spike, not a recommendation.
- Measured against the Rust client on the same machine: RSS 141.5 MB vs
  70.5 MB, idle CPU 4.3% vs 0.8%.
- `@gpuix/react` and `@gpuix/native` ship **no licence** — see `AGENTS.md`'s
  licence rule before this becomes anything but a spike.
