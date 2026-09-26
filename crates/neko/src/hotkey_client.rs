//! Live OS-level hotkey capture. This is a client-only concern (see
//! `neko-core::hotkey`'s module doc): the daemon owns and persists the
//! *setting*, but only a process with a run loop — this one — can actually
//! ask macOS to reserve a key combination, and only that attempt can prove
//! whether it collides with something already holding it.
//!
//! `HotkeyRegistrar` exists so the rebind state machine below (`rebind`) is
//! unit-testable without touching the real OS hotkey service.

use std::cell::RefCell;
use std::rc::Rc;

use global_hotkey::GlobalHotKeyManager;
use global_hotkey::hotkey::HotKey;
use neko_protocol::{HotkeyCombo, HotkeyConfig, Modifier};

pub trait HotkeyRegistrar {
    fn register(&self, hotkey: HotKey) -> Result<(), String>;
    fn unregister(&self, hotkey: HotKey) -> Result<(), String>;
}

pub struct SystemRegistrar {
    manager: GlobalHotKeyManager,
}

impl SystemRegistrar {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            manager: GlobalHotKeyManager::new().map_err(|e| e.to_string())?,
        })
    }
}

impl HotkeyRegistrar for SystemRegistrar {
    fn register(&self, hotkey: HotKey) -> Result<(), String> {
        self.manager.register(hotkey).map_err(|e| e.to_string())
    }

    fn unregister(&self, hotkey: HotKey) -> Result<(), String> {
        self.manager.unregister(hotkey).map_err(|e| e.to_string())
    }
}

/// `global-hotkey`'s own parser accepts exactly this shape
/// (`"Alt+Space"`, `"CMD+SHIFT+3"`, ...) case-insensitively on both
/// modifier and key tokens, which is why `neko-protocol::HotkeyCombo::key`
/// is documented to use `Code`'s own names — no separate mapping table to
/// keep in sync with `global-hotkey`'s.
fn to_hotkey(combo: &HotkeyCombo) -> Result<HotKey, String> {
    let mut parts: Vec<&str> = combo
        .modifiers
        .iter()
        .map(|m| match m {
            Modifier::Cmd => "Cmd",
            Modifier::Alt => "Alt",
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
        })
        .collect();
    parts.push(&combo.key);
    let spec = parts.join("+");
    HotKey::try_from(spec.as_str()).map_err(|e| e.to_string())
}

#[derive(Debug)]
pub enum RebindError {
    /// A live OS registration attempt failed — the strongest signal
    /// available that something else already holds this combo. Callers
    /// (a future onboarding flow) should run
    /// `neko_core::hotkey::check_known_conflict` *before* calling
    /// `rebind`, to give a specific reason for the well-known cases
    /// (Spotlight, ...); this variant is what's left once that heuristic
    /// check passed but the OS itself still said no.
    OsRejected(String),
    /// Not a hotkey-syntax `HotkeyCombo` at all (shouldn't happen for
    /// anything the daemon's `HotkeyCombo` type can express, but a bad key
    /// name would surface here rather than panic).
    Unparseable(String),
}

impl std::fmt::Display for RebindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RebindError::OsRejected(reason) => {
                write!(f, "macOS rejected this combination: {reason}")
            }
            RebindError::Unparseable(reason) => write!(f, "not a valid key combination: {reason}"),
        }
    }
}

impl std::error::Error for RebindError {}

/// Owns exactly one live OS-level registration at a time and the seam that
/// swaps it for another one — the mechanism onboarding's "use a different
/// combination" (design report §3, step 09) will drive once it exists.
/// Building onboarding itself is out of scope for this task; this is the
/// real capability it plugs into (see `AGENTS.md`).
pub struct HotkeyController<R: HotkeyRegistrar> {
    registrar: R,
    current: Option<(HotKey, HotkeyCombo)>,
}

impl<R: HotkeyRegistrar> HotkeyController<R> {
    pub fn new(registrar: R) -> Self {
        Self {
            registrar,
            current: None,
        }
    }

    pub fn current_hotkey_id(&self) -> Option<u32> {
        self.current.as_ref().map(|(hk, _)| hk.id)
    }

    /// Register the daemon's current setting for the first time this
    /// process has come up. Fails loudly (unlike `rebind`, there's nothing
    /// sensible to roll back to on process start).
    pub fn apply_initial(&mut self, config: &HotkeyConfig) -> Result<(), RebindError> {
        let hotkey = to_hotkey(&config.combo).map_err(RebindError::Unparseable)?;
        self.registrar
            .register(hotkey)
            .map_err(RebindError::OsRejected)?;
        self.current = Some((hotkey, config.combo.clone()));
        Ok(())
    }

    /// Attempt to move the live registration to `candidate`. On success the
    /// old combo is unregistered and the new one is live; on failure the
    /// old registration is left untouched — the caller has not lost its
    /// working hotkey over a failed rebind attempt. Does not talk to the
    /// daemon; the caller persists via `CommitHotkey` only after this
    /// returns `Ok`, so a candidate never gets written to storage without
    /// having been proven live-registerable first.
    pub fn rebind(&mut self, candidate: HotkeyCombo) -> Result<(), RebindError> {
        let new_hotkey = to_hotkey(&candidate).map_err(RebindError::Unparseable)?;
        self.registrar
            .register(new_hotkey)
            .map_err(RebindError::OsRejected)?;

        if let Some((old_hotkey, _)) = self.current.take() {
            // Best-effort: if this fails the old combo may still be
            // reserved at the OS level (a rare double-registration, not a
            // resource leak we can recover from here), but the new one is
            // definitely live, which is the correctness property that
            // matters for "the client re-registers cleanly on change".
            let _ = self.registrar.unregister(old_hotkey);
        }
        self.current = Some((new_hotkey, candidate));
        Ok(())
    }

    /// React to `Event::HotkeyChanged` pushed by the daemon — e.g. another
    /// connected client (or a future onboarding flow) committed a change.
    /// Idempotent: a no-op if it matches what's already registered.
    pub fn apply_remote_change(&mut self, config: &HotkeyConfig) -> Result<(), RebindError> {
        if self.current.as_ref().map(|(_, c)| c) == Some(&config.combo) {
            return Ok(());
        }
        self.rebind(config.combo.clone())
    }
}

/// Performs a live OS hotkey re-registration on behalf of the Summon
/// Hotkey screen.
///
/// **Injected rather than called directly**, for the same two reasons
/// `AppearanceSetter` is: it keeps the screen headlessly testable, and the
/// real implementation lives in `main.rs`, which owns the one
/// `HotkeyController` the summon loop itself uses — a rebind proven live
/// here must be *that* registration, not a second one.
///
/// It is a deferred slot rather than a plain value because of ordering:
/// the controller cannot exist until the daemon has answered with the
/// current combo, and the window (and this `Root`) are created before that
/// round-trip completes. `None` means "not ready yet", which the screen
/// reports rather than silently doing nothing.
pub trait HotkeyRebinder {
    fn rebind(&self, candidate: HotkeyCombo) -> Result<(), String>;
}

pub type SharedRebinder = Rc<RefCell<Option<Rc<dyn HotkeyRebinder>>>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[derive(Default)]
    struct FakeRegistrar {
        registered: RefCell<HashSet<u32>>,
        refuse: RefCell<HashSet<u32>>,
    }

    impl FakeRegistrar {
        fn refusing(ids: &[HotKey]) -> Self {
            let refuse = ids.iter().map(|h| h.id).collect();
            Self {
                registered: RefCell::new(HashSet::new()),
                refuse: RefCell::new(refuse),
            }
        }
    }

    impl HotkeyRegistrar for FakeRegistrar {
        fn register(&self, hotkey: HotKey) -> Result<(), String> {
            if self.refuse.borrow().contains(&hotkey.id) {
                return Err("already reserved by another application".into());
            }
            self.registered.borrow_mut().insert(hotkey.id);
            Ok(())
        }

        fn unregister(&self, hotkey: HotKey) -> Result<(), String> {
            self.registered.borrow_mut().remove(&hotkey.id);
            Ok(())
        }
    }

    fn combo(mods: Vec<Modifier>, key: &str) -> HotkeyCombo {
        HotkeyCombo::new(mods, key)
    }

    #[test]
    fn initial_apply_registers_the_configured_combo() {
        let mut controller = HotkeyController::new(FakeRegistrar::default());
        let config = HotkeyConfig {
            combo: HotkeyCombo::default_summon(),
            updated_at_unix_ms: 0,
        };
        assert!(controller.apply_initial(&config).is_ok());
        assert!(controller.current_hotkey_id().is_some());
    }

    #[test]
    fn rebind_swaps_the_live_registration_on_success() {
        let mut controller = HotkeyController::new(FakeRegistrar::default());
        controller
            .apply_initial(&HotkeyConfig {
                combo: HotkeyCombo::default_summon(),
                updated_at_unix_ms: 0,
            })
            .unwrap();
        let old_id = controller.current_hotkey_id().unwrap();

        let candidate = combo(vec![Modifier::Cmd, Modifier::Shift], "Space");
        controller.rebind(candidate.clone()).unwrap();

        let new_id = controller.current_hotkey_id().unwrap();
        assert_ne!(old_id, new_id);
        assert_eq!(controller.current.as_ref().unwrap().1, candidate);
    }

    #[test]
    fn rebind_leaves_the_old_hotkey_registered_when_the_os_rejects_the_candidate() {
        let candidate = combo(vec![Modifier::Cmd], "Space");
        let candidate_hotkey = to_hotkey(&candidate).unwrap();
        let mut controller = HotkeyController::new(FakeRegistrar::refusing(&[candidate_hotkey]));
        controller
            .apply_initial(&HotkeyConfig {
                combo: HotkeyCombo::default_summon(),
                updated_at_unix_ms: 0,
            })
            .unwrap();
        let old_id = controller.current_hotkey_id().unwrap();

        let result = controller.rebind(candidate);
        assert!(matches!(result, Err(RebindError::OsRejected(_))));
        assert_eq!(controller.current_hotkey_id(), Some(old_id));
    }

    #[test]
    fn applying_the_same_remote_config_twice_is_a_no_op() {
        let mut controller = HotkeyController::new(FakeRegistrar::default());
        let config = HotkeyConfig {
            combo: HotkeyCombo::default_summon(),
            updated_at_unix_ms: 0,
        };
        controller.apply_initial(&config).unwrap();
        let id_before = controller.current_hotkey_id();
        controller.apply_remote_change(&config).unwrap();
        assert_eq!(controller.current_hotkey_id(), id_before);
    }

    #[test]
    fn hotkey_combo_names_round_trip_through_global_hotkeys_own_parser() {
        assert!(to_hotkey(&HotkeyCombo::default_summon()).is_ok());
        assert!(to_hotkey(&combo(vec![Modifier::Cmd, Modifier::Shift], "3")).is_ok());
        assert!(to_hotkey(&combo(vec![Modifier::Ctrl, Modifier::Alt], "KeyK")).is_ok());
    }
}
