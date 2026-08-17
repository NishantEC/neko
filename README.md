# neko

A hotkey-summoned, GPU-rendered launcher for macOS, built from scratch in Rust on GPUI.

## Status: proof slice

This is the smallest possible proof that GPUI is viable for this product: a single
crate, one `⌥Space`-summoned window with a text field, escape to dismiss. No app
search, no clipboard history, no daemon, no protocol crate yet — see `AGENTS.md`
for what was deliberately cut and why.

## Running it

```sh
cargo run --release
```

Press **⌥Space** (Option+Space) anywhere to summon the window; **Escape** hides it.
The window and process stay resident between summons — see `AGENTS.md`.

## Toolchain

Rust `1.97.1` (`rustup show` / `rustc --version` to confirm). No other setup —
`cargo build` and `cargo test` are clean from a fresh clone.

## Licence

MIT — see `LICENSE`. Dependency licence rationale is in `AGENTS.md`.
