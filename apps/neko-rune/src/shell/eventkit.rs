//! What the two EventKit bridges (`calendar.rs`, `reminders.rs`) share: how access is reported, the bundle check, and the few EventKit
//! calls both make (`CalendarBridge.swift` and `PhoneBridge.swift` each own an `EKEventStore`; Rune does the same, one per bridge, on the
//! bridge's own thread).
//!
//! **Unbundled runs.** EventKit shows its permission prompt only for an app whose Info.plist carries the usage string, and a process
//! without one is stopped by the system. So EventKit is touched only when the running executable is inside a `.app` whose Info.plist
//! has the key (`usage_ready`, read through `NSBundle.mainBundle`); otherwise one log line and nothing else, never a crash. Swift's rule
//! (`Bundle.main.bundleURL.pathExtension == "app"`) is the same, with the key itself checked as well. The bundle comes with P5b.

use neko_protocol::calendar::CalendarAccess;
use neko_protocol::phone::RemindersAccess;

/// `EKAuthorizationStatus` as the daemon is told (`CalendarPayload.access`, `PhonePayload.access`): full access is `Granted`, never decided is
/// `NotAsked`, everything else (denied, restricted, write only) is `Denied`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Granted,
    NotAsked,
    Denied,
}

impl Access {
    /// From `EKAuthorizationStatus`'s raw value: 0 not determined, 1 restricted, 2 denied, 3 full access, 4 write only.
    pub fn of_status(raw: isize) -> Access {
        match raw {
            3 => Access::Granted,
            0 => Access::NotAsked,
            _ => Access::Denied,
        }
    }

    pub fn calendar(self) -> CalendarAccess {
        match self {
            Access::Granted => CalendarAccess::Granted,
            Access::NotAsked => CalendarAccess::NotAsked,
            Access::Denied => CalendarAccess::Denied,
        }
    }

    pub fn reminders(self) -> RemindersAccess {
        match self {
            Access::Granted => RemindersAccess::Granted,
            Access::NotAsked => RemindersAccess::NotAsked,
            Access::Denied => RemindersAccess::Denied,
        }
    }
}

/// The Info.plist keys, one per bridge.
pub const CALENDAR_USAGE_KEY: &str = "NSCalendarsFullAccessUsageDescription";
pub const REMINDERS_USAGE_KEY: &str = "NSRemindersFullAccessUsageDescription";

/// Whether EventKit may be touched: the bundle is an `.app` and carries a non-empty usage string. Err says why not, in a line that fits the log.
pub fn usage_ready(bundle_path: &str, usage_text: Option<&str>, key: &str) -> Result<(), String> {
    if !bundle_path.trim_end_matches('/').ends_with(".app") {
        return Err("not an installed app bundle".to_owned());
    }
    match usage_text {
        Some(text) if !text.trim().is_empty() => Ok(()),
        _ => Err(format!("the app's Info.plist has no {key}")),
    }
}

#[cfg(target_os = "macos")]
pub mod native {
    use std::ptr::NonNull;
    use std::sync::mpsc::{Receiver, Sender};
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool, ProtocolObject};
    use objc2_event_kit::{EKEntityType, EKEventStore, EKEventStoreChangedNotification};
    use objc2_foundation::{NSBundle, NSError, NSNotification, NSNotificationCenter, NSObjectProtocol, NSString};

    use super::{usage_ready, Access};

    /// `usage_ready` for this process's own bundle.
    pub fn bundle_check(key: &str) -> Result<(), String> {
        let bundle = NSBundle::mainBundle();
        let path = bundle.bundlePath().to_string();
        let text = bundle.objectForInfoDictionaryKey(&NSString::from_str(key)).and_then(|o| o.downcast::<NSString>().ok()).map(|s| s.to_string());
        usage_ready(&path, text.as_deref(), key)
    }

    pub fn authorization(entity: EKEntityType) -> Access {
        Access::of_status(unsafe { EKEventStore::authorizationStatusForEntityType(entity) }.0)
    }

    /// Asks for full access (EventKit shows its prompt only while the answer was never decided) and waits for the answer, at most ten minutes.
    pub fn request_access(store: &EKEventStore, reminders: bool) -> bool {
        let (tx, rx): (Sender<bool>, Receiver<bool>) = std::sync::mpsc::channel();
        let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            let _ = tx.send(granted.as_bool());
        });
        let ptr = (&*block as *const block2::DynBlock<dyn Fn(Bool, *mut NSError)>).cast_mut();
        unsafe {
            if reminders {
                store.requestFullAccessToRemindersWithCompletion(ptr);
            } else {
                store.requestFullAccessToEventsWithCompletion(ptr);
            }
        }
        rx.recv_timeout(Duration::from_secs(600)).unwrap_or(false)
    }

    /// `EKEventStoreChanged` for `store`: one unit on `tx` each time (the bridge settles bursts itself). Keep the returned observer alive.
    pub fn observe(store: &EKEventStore, tx: Sender<()>) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
        let block = RcBlock::new(move |_note: NonNull<NSNotification>| {
            let _ = tx.send(());
        });
        let object: &AnyObject = store;
        unsafe { NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(Some(EKEventStoreChangedNotification), Some(object), None, &block) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_full_access_is_granted_and_everything_but_undecided_is_denied() {
        assert_eq!(Access::of_status(3), Access::Granted);
        assert_eq!(Access::of_status(0), Access::NotAsked);
        for raw in [1, 2, 4, 9] {
            assert_eq!(Access::of_status(raw), Access::Denied, "status {raw}: restricted, denied and write-only are all `Denied`");
        }
        assert_eq!((Access::Granted.calendar(), Access::NotAsked.calendar(), Access::Denied.calendar()), (CalendarAccess::Granted, CalendarAccess::NotAsked, CalendarAccess::Denied));
        assert_eq!((Access::Granted.reminders(), Access::NotAsked.reminders(), Access::Denied.reminders()), (RemindersAccess::Granted, RemindersAccess::NotAsked, RemindersAccess::Denied));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn this_test_process_is_not_an_app_bundle_so_eventkit_would_be_skipped() {
        // The real `NSBundle.mainBundle` of the test binary: a folder under `target/`, not an `.app`. Nothing of EventKit is called.
        for key in [CALENDAR_USAGE_KEY, REMINDERS_USAGE_KEY] {
            assert_eq!(native::bundle_check(key), Err("not an installed app bundle".to_owned()));
        }
    }

    #[test]
    fn eventkit_is_touched_only_from_an_app_bundle_with_the_usage_string() {
        let key = CALENDAR_USAGE_KEY;
        assert!(usage_ready("/Applications/Neko.app", Some("Neko shows your events."), key).is_ok());
        assert!(usage_ready("/Applications/Neko.app/", Some("x"), key).is_ok());
        assert_eq!(usage_ready("/Users/example/target/debug", Some("x"), key), Err("not an installed app bundle".to_owned()), "a development run");
        assert!(usage_ready("/Applications/Neko.app", None, key).unwrap_err().contains(key), "a bundle without the usage string");
        assert!(usage_ready("/Applications/Neko.app", Some("  "), key).is_err(), "an empty string is no string");
    }
}
