# neko

A hotkey-summoned, GPU-rendered launcher for macOS, built from scratch in Rust on GPUI.

## Status

The five-crate spine (`neko-protocol`/`neko-core`/`neko-daemon`/`neko-client`/`neko`),
application search + launch, a runtime-configurable hotkey, and the 14-screen
first-run onboarding arc are built — see `AGENTS.md` for the architecture and
what's still a seam (clipboard history capture, the WASM extension system,
agent capability).

## Running it

```sh
cargo build --release
./target/release/neko
```

(`cargo run --release` also works for development, but summon-latency numbers
in `AGENTS.md` are measured against the release binary run directly, not
through `cargo run`.)

The first launch walks onboarding: what neko needs, the Accessibility ask (a
real macOS permission prompt), the clipboard-history ask, and choosing/testing
your summon hotkey (`⌥Space` by default, rebindable inline). After that it's
one-time — the panel goes straight to summon-on-hotkey on every later launch.

To replay onboarding (e.g. for testing), set `NEKO_RESET_ONBOARDING` to any
value before launching:

```sh
NEKO_RESET_ONBOARDING=1 ./target/release/neko
```

Once onboarding is done: press your configured hotkey (**⌥Space** by default)
anywhere to summon the window; **Escape** hides it. If Accessibility was
declined, the hotkey won't be live — reopen neko from the Dock instead. The
window and process stay resident between summons — see `AGENTS.md`.

## Toolchain

Rust `1.97.1` (`rustup show` / `rustc --version` to confirm). One setup step
before the first `cargo build`/`cargo test`/`cargo clippy` (and again any
time the pinned gpui fork rev changes):

```sh
./scripts/setup-gpui-patch.sh
```

Populates the local, patched checkout of the pinned `gpui` fork rev this
repo's `[patch]` section depends on — see `AGENTS.md`'s "Summon latency"
section for why the patch exists. Without it, `cargo build` fails with a
missing-path error.

## Licence

MIT — see `LICENSE`. Dependency licence rationale is in `AGENTS.md`.
