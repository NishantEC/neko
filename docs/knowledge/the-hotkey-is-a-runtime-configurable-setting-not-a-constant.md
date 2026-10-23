## The hotkey is a runtime-configurable setting, not a constant

Added mid-task, on top of the original single-crate proof's hard-coded
`⌥Space`. The daemon persists the current `HotkeyCombo` and serves it over
`neko-protocol::Request::{GetHotkey, CheckHotkeyConflict, CommitHotkey}`; on a
successful `CommitHotkey` it broadcasts `Event::HotkeyChanged` to every
connected client. `neko::hotkey_client::HotkeyController::rebind` is the real
capture mechanism: it attempts a live OS registration of the candidate first,
and only unregisters the old combo once the new one is proven live — a failed
rebind never loses the working hotkey. `neko_core::hotkey::check_known_conflict`
is a fast, side-effect-free pre-check against well-known reserved combos
(Spotlight, Mission Control, screenshot shortcuts, ...) — real but a heuristic,
not a guarantee; only a live registration attempt can catch a *third-party*
app's custom binding.

`rebind` is now called from exactly one place: onboarding's step 09
(`onboarding::view::OnboardingRoot::try_rebind`) — captures the next raw
keydown while "Use a different combination" is active, validates it locally
(`onboarding::state::validate_candidate`), checks
`Request::CheckHotkeyConflict` for a human-readable reason to show if the live
attempt fails, then calls `rebind` and on success `Request::CommitHotkey`. See
`hotkey_client.rs`'s own doc comments and test suite (`FakeRegistrar`) for the
state machine `rebind` itself guarantees.
