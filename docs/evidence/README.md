# Evidence

The written reports in this directory are the durable record: what was
measured, how, what the numbers were, and what was *not* verified. They are
the reason this project's claims can be checked rather than taken on trust.

**The screenshots they cite are not published.** Several of these reports link
`docs/evidence/*.png`, and those links will not resolve here. That is
deliberate, not rot.

Every capture was a window-scoped screenshot of a real working machine, taken
under the safety rules in `AGENTS.md` — never a full-screen or region grab.
Even so, reviewing them before this repository was first published found real
client repository names, branch names that named unreleased features, a pull
request number, and fragments of real prompts. The agent grid and the Agents
mode render exactly that at rest, so any capture wide enough to include them
carried it.

Removing the images cost little: each report already states in prose what its
screenshot proved, including the measurements. Keeping them would have
published somebody else's private work to make a point about a rounded corner.

Captures are still taken during development and still live on disk; they are
gitignored. If you are working on this repository and need one, the harness and
the hooks that produce them are all still here — see `AGENTS.md`'s sections on
`verify_harness`, `crates/neko/src/evidence.rs`, and the standing rule that a
window-scoped capture is not automatically private: check what is in the frame
before saving one.
