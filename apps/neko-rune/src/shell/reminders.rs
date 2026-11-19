//! Reminders push and replies from EventKit: a port of `PhoneBridge.swift`. Puts a reminder in Neko's own "Neko" Reminders list for each
//! thing that needs the person, so their phone tells them (iCloud delivers the alarm), and reports back when one is completed or answered
//! in words. The daemon decides what to push and when (`docs/knowledge/phone-push.md`); this only owns EventKit.
//!
//! * Every 20 s, and 2 s after Reminders reports a change, `PhoneRemindersSync { access, present, completed, notes }` goes to the daemon
//!   and the plan it answers with (`create` / `delete` / `finish`) is carried out. Only reminders that carry a reply line (one that starts
//!   with `>`) send their notes; the daemon never sees the rest of anyone's Reminders text.
//! * Access is asked only when the plan says the person turned the channel on, and only while it was never decided.
//! * Creating: the "Neko" list is found by title and never made just to look; if it has to be made it goes on the source Reminders already
//!   uses for new lists when that is iCloud, else any iCloud source, else that default, else a local one (`pick_source`). A reminder has the
//!   item's title, its detail and link in the notes, a due time and an alarm 5 s ahead (so iCloud notifies the phone), and **a last notes line
//!   `neko:<seq>`**.
//! * Deleting removes the reminders the plan names. Finishing writes the daemon's echo into the notes and completes the reminder (it stays
//!   in the list, completed, so the echo can be read); `keep_open` writes the notes and leaves it open.
//! * Everything done is reported with `PhoneRemindersDone { created, deleted, finished }`, only when something was planned.
//!
//! **Which reminder is which, across restarts.** The bridge keeps nothing on disk and holds nothing in memory that a restart would lose.
//! Two things carry it: the daemon's `phone_state_v1` (item seq to the reminder's `calendarItemIdentifier`, written from `PhoneRemindersDone`),
//! and the `neko:<seq>` marker on the last notes line of every reminder, which finds a reminder again when the report was lost (the app quit
//! between saving and reporting): before creating, the list's open reminders are read and one with the same marker is reused, not made twice.
//! The marker is the last line with the prefix, so an echo written after it does not hide it. A summary has no marker (seq 0) and is
//! de-duplicated by its `key` instead, which the daemon echoes back.
//!
//! A duty (`shell::Duties::reminders`): only in `--app` mode, never while the Swift app runs, so there is never a second writer to the list.
//! Only from an app bundle that carries `NSRemindersFullAccessUsageDescription`; otherwise one log line and no thread.
//!
//! EventKit is behind [`ReminderStore`] and the daemon behind [`PhoneDaemon`], so every rule is tested with fakes; [`native`] is thin.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use neko_protocol::phone::{ReminderCreated, ReminderNote, ReminderPlan, RemindersAccess};

use super::eventkit::Access;
use super::schedule::{self, Job};

/// How often the bridge cycles (`PhoneBridge.interval`).
pub const INTERVAL: Duration = Duration::from_secs(20);
/// The list's title.
pub const LIST_TITLE: &str = "Neko";
/// Written on the last line of a reminder's notes, so a reminder can be matched to its item without trusting an identifier alone.
pub const MARKER_PREFIX: &str = "neko:";

/// A reminder found in Neko's list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReminderInfo {
    pub id: String,
    pub completed: bool,
    pub notes: String,
}

/// A reminder the plan asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewReminder {
    pub seq: u64,
    pub title: String,
    pub notes: String,
    pub url: Option<String>,
    pub key: String,
}

/// A reminder the person answered in words: set these notes (the "Got it" echo) and complete it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Finish {
    pub id: String,
    pub notes: String,
    pub keep_open: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub enabled: bool,
    pub create: Vec<NewReminder>,
    pub delete: Vec<String>,
    pub finish: Vec<Finish>,
}

impl Plan {
    /// `PhonePayload.plan(from:)`: an empty `url` is none, and a `finish` with no id is dropped.
    pub fn of(reply: ReminderPlan) -> Plan {
        Plan {
            enabled: reply.enabled,
            create: reply.create.into_iter().map(|n| NewReminder { seq: n.seq, title: n.title, notes: n.notes, url: n.url.filter(|u| !u.is_empty()), key: n.key }).collect(),
            delete: reply.delete,
            finish: reply.finish.into_iter().filter(|f| !f.id.is_empty()).map(|f| Finish { id: f.id, notes: f.notes, keep_open: f.keep_open }).collect(),
        }
    }

    fn is_empty_work(&self) -> bool {
        self.delete.is_empty() && self.create.is_empty() && self.finish.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Local,
    /// A CalDAV account (iCloud among them).
    OtherAccount,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceInfo {
    pub id: String,
    pub title: String,
    pub kind: SourceKind,
}

/// Where Neko's list goes (`PhonePayload.pickSource`): the source Reminders already uses for new lists when it is iCloud, else any iCloud
/// source, else that default, else a local list. iCloud is what delivers the alarm to a phone.
pub fn pick_source(sources: &[SourceInfo], preferred: Option<&str>) -> Option<String> {
    let is_cloud = |s: &SourceInfo| s.title.to_lowercase().contains("icloud") && s.kind != SourceKind::Local;
    if let Some(source) = preferred.and_then(|p| sources.iter().find(|s| s.id == p)).filter(|s| is_cloud(s)) {
        return Some(source.id.clone());
    }
    if let Some(cloud) = sources.iter().find(|s| is_cloud(s)) {
        return Some(cloud.id.clone());
    }
    if let Some(preferred) = preferred.filter(|p| sources.iter().any(|s| s.id == *p)) {
        return Some(preferred.to_owned());
    }
    sources.iter().find(|s| s.kind == SourceKind::Local).or_else(|| sources.first()).map(|s| s.id.clone())
}

/// Swift's `Character.isNewline`.
fn is_newline(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn lines(notes: &str) -> impl DoubleEndedIterator<Item = &str> {
    notes.split(is_newline).filter(|l| !l.is_empty())
}

/// The item number in a reminder's notes, if it has the marker line. The marker is the last line that has the prefix: after an echo the
/// notes end with "Got it: ...", not with the marker.
pub fn seq_from_notes(notes: &str) -> Option<u64> {
    let last = lines(notes).map(|l| l.trim_matches(|c: char| c == '\t' || (c.is_whitespace() && !is_newline(c)))).rfind(|l| l.starts_with(MARKER_PREFIX))?;
    last[MARKER_PREFIX.len()..].parse::<i64>().ok().and_then(|n| u64::try_from(n).ok())
}

/// Whether the person wrote a reply: a line whose first character (after spaces) is `>` and has something after it. Nothing Neko writes
/// into a reminder starts that way.
pub fn has_reply(notes: &str) -> bool {
    lines(notes).any(|line| {
        let trimmed = line.trim_start_matches([' ', '\t']);
        trimmed.strip_prefix('>').is_some_and(|rest| !rest.chars().all(|c| c == ' ' || c == '\t'))
    })
}

/// What `PhoneRemindersSync` carries from what the list holds (`PhonePayload.syncRequest`): the ids present, the ids completed, and the
/// notes of only the reminders with a reply line.
pub fn sync_parts(reminders: &[ReminderInfo]) -> (Vec<String>, Vec<String>, Vec<ReminderNote>) {
    (
        reminders.iter().map(|r| r.id.clone()).collect(),
        reminders.iter().filter(|r| r.completed).map(|r| r.id.clone()).collect(),
        reminders.iter().filter(|r| has_reply(&r.notes)).map(|r| ReminderNote { id: r.id.clone(), notes: r.notes.clone() }).collect(),
    )
}

/// What the bridge needs of EventKit. Every call but `read` and `request_access` is quick; none of them is run by a test against the real thing.
pub trait ReminderStore {
    fn access(&mut self) -> Access;
    /// Shows the system prompt (only called while access is `NotAsked`) and waits for the answer.
    fn request_access(&mut self);
    /// Drops what the store cached (access was granted later in System Settings).
    fn reset(&mut self);
    /// Neko's list, if it exists. Never created just to look.
    fn list(&mut self) -> Option<String>;
    /// Makes Neko's list on the best source; None when that failed.
    fn make_list(&mut self) -> Option<String>;
    /// The reminders in the list (completed ones too).
    fn read(&mut self, list: &str) -> Vec<ReminderInfo>;
    /// Removes a reminder, not committed.
    fn remove(&mut self, id: &str);
    /// Makes a reminder in the list (due and alarmed a few seconds ahead), not committed. The identifier, or None when it could not be saved.
    fn create(&mut self, list: &str, item: &NewReminder) -> Option<String>;
    /// Writes the notes and, unless `keep_open`, completes the reminder; not committed. False when there is no such reminder or it did not save.
    fn finish(&mut self, id: &str, notes: &str, keep_open: bool) -> bool;
    fn commit(&mut self);
}

/// What the bridge needs of the daemon.
pub trait PhoneDaemon {
    /// `PhoneRemindersSync`; None when the daemon did not answer with a plan (an empty plan).
    fn sync(&mut self, access: RemindersAccess, present: Vec<String>, completed: Vec<String>, notes: Vec<ReminderNote>) -> Option<ReminderPlan>;
    fn done(&mut self, created: Vec<ReminderCreated>, deleted: Vec<String>, finished: Vec<String>);
}

/// `PhoneBridge`'s methods.
pub struct Bridge<S, D> {
    pub store: S,
    pub daemon: D,
    last_access: Option<Access>,
}

impl<S: ReminderStore, D: PhoneDaemon> Bridge<S, D> {
    pub fn new(store: S, daemon: D) -> Bridge<S, D> {
        Bridge { store, daemon, last_access: None }
    }

    fn sync(&mut self, access: Access) -> Plan {
        let mut found = Vec::new();
        if access == Access::Granted
            && let Some(list) = self.store.list()
        {
            found = self.store.read(&list);
        }
        let (present, completed, notes) = sync_parts(&found);
        match self.daemon.sync(access.reminders(), present, completed, notes) {
            Some(plan) => Plan::of(plan),
            None => Plan::default(),
        }
    }

    /// One cycle (`runCycle`).
    pub fn run_cycle(&mut self) {
        let mut access = self.store.access();
        // Access granted later in System Settings: the store must re-read.
        if access == Access::Granted && self.last_access.is_some_and(|last| last != Access::Granted) {
            self.store.reset();
        }
        self.last_access = Some(access);
        let mut plan = self.sync(access);
        if plan.enabled && access == Access::NotAsked {
            // EventKit shows its prompt only while access was never decided, and only now that the person turned the channel on.
            self.store.request_access();
            access = self.store.access();
            self.last_access = Some(access);
            plan = self.sync(access);
        }
        if access != Access::Granted || plan.is_empty_work() {
            return;
        }
        let created = self.apply(&plan);
        let finished = self.finish(&plan.finish);
        self.daemon.done(created, plan.delete.clone(), finished);
    }

    /// Writes the daemon's echo into each answered reminder's notes and completes it. Returns the ids that were saved.
    fn finish(&mut self, items: &[Finish]) -> Vec<String> {
        let done = items.iter().filter(|i| self.store.finish(&i.id, &i.notes, i.keep_open)).map(|i| i.id.clone()).collect();
        self.store.commit();
        done
    }

    /// Carries out the deletes and the creates and returns what was created.
    fn apply(&mut self, plan: &Plan) -> Vec<ReminderCreated> {
        for id in &plan.delete {
            self.store.remove(id);
        }
        let mut created = Vec::new();
        if !plan.create.is_empty() {
            let list = match self.store.list() {
                Some(list) => Some(list),
                None => self.store.make_list(),
            };
            if let Some(list) = list {
                // A reminder made before an earlier report was lost is reused.
                let known = self.known_markers(&list);
                for item in &plan.create {
                    if item.seq != 0
                        && let Some(id) = known.get(&item.seq)
                    {
                        created.push(ReminderCreated { seq: item.seq, id: id.clone(), key: item.key.clone() });
                        continue;
                    }
                    let id = self.store.create(&list, item).unwrap_or_default();
                    created.push(ReminderCreated { seq: item.seq, id, key: item.key.clone() });
                }
            }
        }
        self.store.commit();
        created
    }

    /// Item number to identifier for the reminders already in the list, found by the marker line in their notes (open ones only).
    fn known_markers(&mut self, list: &str) -> HashMap<u64, String> {
        let mut out = HashMap::new();
        for info in self.store.read(list).into_iter().filter(|r| !r.completed) {
            if let Some(seq) = seq_from_notes(&info.notes) {
                out.insert(seq, info.id);
            }
        }
        out
    }
}

impl<S: ReminderStore, D: PhoneDaemon> Job for Bridge<S, D> {
    fn cycle(&mut self, _now_ms: i64) -> Duration {
        self.run_cycle();
        INTERVAL
    }

    fn settled(&mut self, _now_ms: i64) {
        self.run_cycle();
    }
}

/// The real daemon, over its socket.
pub struct SocketDaemon(pub PathBuf);

impl PhoneDaemon for SocketDaemon {
    fn sync(&mut self, access: RemindersAccess, present: Vec<String>, completed: Vec<String>, notes: Vec<ReminderNote>) -> Option<ReminderPlan> {
        crate::link::shell::phone_sync(&self.0, access, present, completed, notes)
    }

    fn done(&mut self, created: Vec<ReminderCreated>, deleted: Vec<String>, finished: Vec<String>) {
        let _ = crate::link::shell::phone_done(&self.0, created, deleted, finished);
    }
}

/// Starts the bridge on a thread of its own (EventKit's store is made there and never leaves it). `active` says whether Rune holds the duty
/// now. Does nothing outside an app bundle with the usage string: one line on stderr.
#[cfg(target_os = "macos")]
pub fn spawn(socket: PathBuf, active: Arc<AtomicBool>) {
    if let Err(why) = super::eventkit::native::bundle_check(super::eventkit::REMINDERS_USAGE_KEY) {
        eprintln!("neko-rune: --app: {why}, so Reminders is off");
        return;
    }
    let _ = std::thread::Builder::new().name("neko-reminders".into()).spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let store = native::EkReminders::new(tx);
        schedule::drive(Bridge::new(store, SocketDaemon(socket)), &active, &rx);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn spawn(_socket: PathBuf, _active: Arc<AtomicBool>) {}

#[cfg(target_os = "macos")]
pub mod native {
    //! EventKit's side of [`ReminderStore`]. Thin on purpose: it reads, writes and converts, it decides nothing.

    use std::sync::mpsc::{channel, Sender};
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_event_kit::{EKAlarm, EKCalendar, EKEntityType, EKEventStore, EKReminder, EKSourceType};
    use objc2_foundation::{NSArray, NSCalendar, NSCalendarUnit, NSDate, NSObjectProtocol, NSString, NSURL};

    use super::{pick_source, NewReminder, ReminderInfo, ReminderStore, SourceInfo, SourceKind, LIST_TITLE};
    use crate::shell::eventkit::{native as ek, Access};

    pub struct EkReminders {
        store: Retained<EKEventStore>,
        _observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    }

    impl EkReminders {
        pub fn new(changed: Sender<()>) -> EkReminders {
            let store = unsafe { EKEventStore::new() };
            let observer = ek::observe(&store, changed);
            EkReminders { store, _observer: observer }
        }

        fn calendar(&self, id: &str) -> Option<Retained<EKCalendar>> {
            unsafe { self.store.calendarWithIdentifier(&NSString::from_str(id)) }
        }

        fn reminder(&self, id: &str) -> Option<Retained<EKReminder>> {
            unsafe { self.store.calendarItemWithIdentifier(&NSString::from_str(id)) }.and_then(|item| item.downcast::<EKReminder>().ok())
        }
    }

    impl ReminderStore for EkReminders {
        fn access(&mut self) -> Access {
            ek::authorization(EKEntityType::Reminder)
        }

        fn request_access(&mut self) {
            ek::request_access(&self.store, true);
        }

        fn reset(&mut self) {
            unsafe { self.store.reset() }
        }

        fn list(&mut self) -> Option<String> {
            unsafe { self.store.calendarsForEntityType(EKEntityType::Reminder).iter().find(|c| c.title().to_string() == LIST_TITLE).map(|c| c.calendarIdentifier().to_string()) }
        }

        fn make_list(&mut self) -> Option<String> {
            unsafe {
                let list = EKCalendar::calendarForEntityType_eventStore(EKEntityType::Reminder, &self.store);
                list.setTitle(&NSString::from_str(LIST_TITLE));
                let sources: Vec<SourceInfo> = self
                    .store
                    .sources()
                    .iter()
                    .map(|s| SourceInfo {
                        id: s.sourceIdentifier().to_string(),
                        title: s.title().to_string(),
                        kind: match s.sourceType() {
                            EKSourceType::CalDAV => SourceKind::OtherAccount,
                            EKSourceType::Local => SourceKind::Local,
                            _ => SourceKind::Other,
                        },
                    })
                    .collect();
                let preferred = self.store.defaultCalendarForNewReminders().and_then(|c| c.source()).map(|s| s.sourceIdentifier().to_string());
                let chosen = pick_source(&sources, preferred.as_deref())?;
                let source = self.store.sourceWithIdentifier(&NSString::from_str(&chosen))?;
                list.setSource(Some(&source));
                self.store.saveCalendar_commit_error(&list, true).ok()?;
                Some(list.calendarIdentifier().to_string())
            }
        }

        fn read(&mut self, list: &str) -> Vec<ReminderInfo> {
            let Some(calendar) = self.calendar(list) else { return Vec::new() };
            let (tx, rx) = channel();
            let block = RcBlock::new(move |found: *mut NSArray<EKReminder>| {
                let infos: Vec<ReminderInfo> = unsafe { found.as_ref() }
                    .map(|array| unsafe { array.iter().map(|r| ReminderInfo { id: r.calendarItemIdentifier().to_string(), completed: r.isCompleted(), notes: r.notes().map(|n| n.to_string()).unwrap_or_default() }).collect() })
                    .unwrap_or_default();
                let _ = tx.send(infos);
            });
            unsafe {
                let calendars = NSArray::from_retained_slice(&[calendar]);
                let predicate = self.store.predicateForRemindersInCalendars(Some(&calendars));
                let _token = self.store.fetchRemindersMatchingPredicate_completion(&predicate, &block);
            }
            rx.recv_timeout(Duration::from_secs(60)).unwrap_or_default()
        }

        fn remove(&mut self, id: &str) {
            if let Some(reminder) = self.reminder(id) {
                let _ = unsafe { self.store.removeReminder_commit_error(&reminder, false) };
            }
        }

        fn create(&mut self, list: &str, item: &NewReminder) -> Option<String> {
            let calendar = self.calendar(list)?;
            unsafe {
                let reminder = EKReminder::reminderWithEventStore(&self.store);
                reminder.setCalendar(Some(&calendar));
                reminder.setTitle(Some(&NSString::from_str(&item.title)));
                reminder.setNotes(Some(&NSString::from_str(&item.notes)));
                if let Some(url) = item.url.as_deref().and_then(|u| NSURL::URLWithString(&NSString::from_str(u))) {
                    reminder.setURL(Some(&url));
                }
                // Due and alarmed 5 s ahead, so iCloud notifies the phone.
                let due = NSDate::dateWithTimeIntervalSinceNow(5.0);
                let units = NSCalendarUnit::Year | NSCalendarUnit::Month | NSCalendarUnit::Day | NSCalendarUnit::Hour | NSCalendarUnit::Minute | NSCalendarUnit::Second;
                reminder.setDueDateComponents(Some(&NSCalendar::currentCalendar().components_fromDate(units, &due)));
                reminder.addAlarm(&EKAlarm::alarmWithAbsoluteDate(&due));
                self.store.saveReminder_commit_error(&reminder, false).ok()?;
                Some(reminder.calendarItemIdentifier().to_string())
            }
        }

        fn finish(&mut self, id: &str, notes: &str, keep_open: bool) -> bool {
            let Some(reminder) = self.reminder(id) else { return false };
            unsafe {
                reminder.setNotes(Some(&NSString::from_str(notes)));
                // An answer that could not be used keeps its reminder open, with the reason in the notes.
                if !keep_open {
                    reminder.setCompleted(true);
                }
                self.store.saveReminder_commit_error(&reminder, false).is_ok()
            }
        }

        fn commit(&mut self) {
            let _ = unsafe { self.store.commit() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::phone::{ReminderFinish, ReminderNew};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// One `sync` the daemon saw, as the fake records it.
    type Seen = (RemindersAccess, Vec<String>, Vec<String>, Vec<ReminderNote>);

    /// The Reminders list as a person's phone would see it, and what was done to it.
    #[derive(Default)]
    struct World {
        access: Option<Access>,
        /// The reminders of Neko's list; None when the list does not exist.
        list: Option<Vec<ReminderInfo>>,
        next_id: usize,
        made_lists: usize,
        created: Vec<NewReminder>,
        removed: Vec<String>,
        asked: usize,
        resets: usize,
        commits: usize,
        fail_create: bool,
        /// Each `sync` the daemon saw, and the plans it answers with, in order (the last one repeats).
        seen: Vec<Seen>,
        plans: Vec<Option<ReminderPlan>>,
        done: Vec<(Vec<ReminderCreated>, Vec<String>, Vec<String>)>,
        /// What the person does on their phone between cycles is done by editing `list` directly.
        plan_after_permission: Option<ReminderPlan>,
    }

    #[derive(Clone, Default)]
    struct Fake(Rc<RefCell<World>>);

    impl ReminderStore for Fake {
        fn access(&mut self) -> Access {
            self.0.borrow().access.unwrap_or(Access::NotAsked)
        }
        fn request_access(&mut self) {
            let mut w = self.0.borrow_mut();
            w.asked += 1;
            w.access = Some(Access::Granted);
        }
        fn reset(&mut self) {
            self.0.borrow_mut().resets += 1;
        }
        fn list(&mut self) -> Option<String> {
            self.0.borrow().list.as_ref().map(|_| "LIST".to_owned())
        }
        fn make_list(&mut self) -> Option<String> {
            let mut w = self.0.borrow_mut();
            w.made_lists += 1;
            w.list = Some(Vec::new());
            Some("LIST".to_owned())
        }
        fn read(&mut self, _list: &str) -> Vec<ReminderInfo> {
            self.0.borrow().list.clone().unwrap_or_default()
        }
        fn remove(&mut self, id: &str) {
            let mut w = self.0.borrow_mut();
            w.removed.push(id.to_owned());
            if let Some(list) = w.list.as_mut() {
                list.retain(|r| r.id != id);
            }
        }
        fn create(&mut self, _list: &str, item: &NewReminder) -> Option<String> {
            let mut w = self.0.borrow_mut();
            if w.fail_create {
                return None;
            }
            w.next_id += 1;
            let id = format!("R{}", w.next_id);
            w.created.push(item.clone());
            let info = ReminderInfo { id: id.clone(), completed: false, notes: item.notes.clone() };
            w.list.get_or_insert_with(Vec::new).push(info);
            Some(id)
        }
        fn finish(&mut self, id: &str, notes: &str, keep_open: bool) -> bool {
            let mut w = self.0.borrow_mut();
            let Some(r) = w.list.as_mut().and_then(|l| l.iter_mut().find(|r| r.id == id)) else { return false };
            r.notes = notes.to_owned();
            if !keep_open {
                r.completed = true;
            }
            true
        }
        fn commit(&mut self) {
            self.0.borrow_mut().commits += 1;
        }
    }

    impl PhoneDaemon for Fake {
        fn sync(&mut self, access: RemindersAccess, present: Vec<String>, completed: Vec<String>, notes: Vec<ReminderNote>) -> Option<ReminderPlan> {
            let mut w = self.0.borrow_mut();
            w.seen.push((access, present, completed, notes));
            let at = (w.seen.len() - 1).min(w.plans.len().saturating_sub(1));
            // The plan after the permission prompt is the one that has the work.
            if access == RemindersAccess::Granted && w.plan_after_permission.is_some() {
                return w.plan_after_permission.clone();
            }
            w.plans.get(at).cloned().flatten()
        }
        fn done(&mut self, created: Vec<ReminderCreated>, deleted: Vec<String>, finished: Vec<String>) {
            self.0.borrow_mut().done.push((created, deleted, finished));
        }
    }

    fn bridge(w: &Fake) -> Bridge<Fake, Fake> {
        Bridge::new(w.clone(), w.clone())
    }

    fn granted_with_list(w: &Fake, reminders: Vec<ReminderInfo>) {
        let mut world = w.0.borrow_mut();
        world.access = Some(Access::Granted);
        world.list = Some(reminders);
    }

    fn new(seq: u64, title: &str) -> ReminderNew {
        ReminderNew { seq, title: title.into(), notes: format!("Detail of {title}\nneko:{seq}"), url: Some("neko://inbox".into()), key: format!("k{seq}") }
    }

    fn plan(create: Vec<ReminderNew>, delete: Vec<&str>, finish: Vec<ReminderFinish>) -> ReminderPlan {
        ReminderPlan { enabled: true, create, delete: delete.into_iter().map(str::to_owned).collect(), finish }
    }

    fn info(id: &str, completed: bool, notes: &str) -> ReminderInfo {
        ReminderInfo { id: id.into(), completed, notes: notes.into() }
    }

    #[test]
    fn a_new_item_makes_one_reminder_in_a_list_made_for_it_and_is_reported_with_its_identifier() {
        let w = Fake::default();
        {
            let mut world = w.0.borrow_mut();
            world.access = Some(Access::Granted);
            world.plans = vec![Some(plan(vec![new(7, "Review the PR")], vec![], vec![]))];
        }
        bridge(&w).run_cycle();
        let world = w.0.borrow();
        assert_eq!(world.made_lists, 1, "Neko's list did not exist: it is made, because there is something to put in it");
        assert_eq!(world.created.len(), 1);
        let made = &world.created[0];
        assert_eq!((made.seq, made.title.as_str(), made.url.as_deref(), made.key.as_str()), (7, "Review the PR", Some("neko://inbox"), "k7"));
        assert!(made.notes.ends_with("neko:7"), "the marker is the last line");
        assert_eq!(world.done, vec![(vec![ReminderCreated { seq: 7, id: "R1".into(), key: "k7".into() }], vec![], vec![])]);
        assert!(world.commits >= 1, "committed once, at the end of the batch");
    }

    #[test]
    fn the_list_is_never_made_just_to_look() {
        let w = Fake::default();
        {
            let mut world = w.0.borrow_mut();
            world.access = Some(Access::Granted);
            world.plans = vec![None];
        }
        bridge(&w).run_cycle();
        let world = w.0.borrow();
        assert_eq!(world.made_lists, 0);
        assert!(world.done.is_empty(), "nothing planned, nothing reported");
        assert_eq!(world.seen[0].0, RemindersAccess::Granted);
    }

    #[test]
    fn a_resolved_item_deletes_its_reminder_and_the_deletion_is_reported() {
        let w = Fake::default();
        granted_with_list(&w, vec![info("A", false, "x\nneko:1"), info("B", false, "y\nneko:2")]);
        w.0.borrow_mut().plans = vec![Some(plan(vec![], vec!["A", "GONE"], vec![]))];
        bridge(&w).run_cycle();
        let world = w.0.borrow();
        assert_eq!(world.removed, ["A", "GONE"]);
        assert_eq!(world.list.as_ref().unwrap().iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["B"]);
        assert_eq!(world.done, vec![(vec![], vec!["A".to_owned(), "GONE".to_owned()], vec![])], "every planned deletion is reported, even of a reminder already gone");
    }

    #[test]
    fn a_plain_completion_is_reported_as_completed_and_its_notes_are_not_sent() {
        let w = Fake::default();
        granted_with_list(&w, vec![info("A", true, "Detail\nneko:1"), info("B", false, "Detail\nneko:2")]);
        w.0.borrow_mut().plans = vec![None];
        bridge(&w).run_cycle();
        let (access, present, completed, notes) = w.0.borrow().seen[0].clone();
        assert_eq!(access, RemindersAccess::Granted);
        assert_eq!(present, ["A", "B"]);
        assert_eq!(completed, ["A"], "completing is the default: the daemon is told which ones were completed");
        assert!(notes.is_empty(), "the daemon never sees the notes of a reminder with no reply line");
    }

    #[test]
    fn a_greater_than_line_is_the_answer_and_only_those_notes_are_sent() {
        let w = Fake::default();
        granted_with_list(&w, vec![info("A", false, "Extend?\nneko:3\n> 20"), info("B", false, "Detail\nneko:4"), info("C", false, "Only an arrow\n>   \nneko:5")]);
        w.0.borrow_mut().plans = vec![None];
        bridge(&w).run_cycle();
        let notes = w.0.borrow().seen[0].3.clone();
        assert_eq!(notes, vec![ReminderNote { id: "A".into(), notes: "Extend?\nneko:3\n> 20".into() }], "an empty arrow is no reply");
    }

    #[test]
    fn the_daemons_echo_is_written_and_the_reminder_completed_or_kept_open_with_the_reason() {
        let w = Fake::default();
        granted_with_list(&w, vec![info("A", false, "Q\nneko:3\n> 20"), info("B", false, "Q\nneko:4\n> lots")]);
        let finish = vec![
            ReminderFinish { id: "A".into(), notes: "Q\nneko:3\nGot it: extend by 20 turns".into(), keep_open: false },
            ReminderFinish { id: "B".into(), notes: "Q\nneko:4\n> lots\nNeeds a number from 1 to 100".into(), keep_open: true },
            ReminderFinish { id: "".into(), notes: "dropped".into(), keep_open: false },
            ReminderFinish { id: "MISSING".into(), notes: "x".into(), keep_open: false },
        ];
        w.0.borrow_mut().plans = vec![Some(plan(vec![], vec![], finish))];
        bridge(&w).run_cycle();
        let world = w.0.borrow();
        let list = world.list.as_ref().unwrap();
        assert_eq!((list[0].completed, list[0].notes.as_str()), (true, "Q\nneko:3\nGot it: extend by 20 turns"));
        assert_eq!((list[1].completed, list[1].notes.ends_with("Needs a number from 1 to 100")), (false, true), "an unusable answer stays open with the reason");
        assert_eq!(world.done, vec![(vec![], vec![], vec!["A".to_owned(), "B".to_owned()])], "both were saved; the empty id and the missing reminder were not");
    }

    #[test]
    fn the_same_item_arriving_twice_makes_one_reminder() {
        let w = Fake::default();
        w.0.borrow_mut().access = Some(Access::Granted);
        w.0.borrow_mut().plans = vec![Some(plan(vec![new(9, "Approve the plan")], vec![], vec![]))];
        let mut b = bridge(&w);
        b.run_cycle();
        b.run_cycle();
        let world = w.0.borrow();
        assert_eq!(world.created.len(), 1, "the second plan names an item that already has its reminder: nothing is made");
        assert_eq!(world.done.len(), 2);
        assert_eq!(world.done[1].0, vec![ReminderCreated { seq: 9, id: "R1".into(), key: "k9".into() }], "and the existing one is what is reported");
    }

    #[test]
    fn a_restart_between_saving_and_reporting_makes_no_duplicate_because_the_marker_in_the_notes_finds_it() {
        let w = Fake::default();
        // The reminder was saved, then the app quit before the daemon was told: the list has it, the daemon has no identifier for it.
        granted_with_list(&w, vec![info("OLD", false, "Detail\nhttps://example.com\nneko:12")]);
        w.0.borrow_mut().plans = vec![Some(plan(vec![new(12, "Merge the release")], vec![], vec![]))];
        // A new bridge, as after a restart: it remembers nothing.
        bridge(&w).run_cycle();
        let world = w.0.borrow();
        assert!(world.created.is_empty(), "the reminder that is already there is reused");
        assert_eq!(world.done[0].0, vec![ReminderCreated { seq: 12, id: "OLD".into(), key: "k12".into() }]);
        assert_eq!(world.made_lists, 0);
    }

    #[test]
    fn the_marker_survives_an_echo_and_only_open_reminders_count() {
        assert_eq!(seq_from_notes("Detail\nneko:5"), Some(5));
        assert_eq!(seq_from_notes("Detail\nneko:5\nGot it: done"), Some(5), "an echo after the marker does not hide it");
        assert_eq!(seq_from_notes("neko:1\nneko:2"), Some(2), "the last marker line wins");
        assert_eq!(seq_from_notes("  neko:8  \r\n"), Some(8));
        assert_eq!(seq_from_notes("no marker"), None);
        assert_eq!(seq_from_notes("neko: 5"), None, "no space is allowed after the colon");
        assert_eq!(seq_from_notes("neko:five"), None);
        let w = Fake::default();
        // A completed reminder with the marker is not reused: a finished item that comes back gets a new reminder.
        granted_with_list(&w, vec![info("DONE", true, "x\nneko:4")]);
        w.0.borrow_mut().plans = vec![Some(plan(vec![new(4, "Again")], vec![], vec![]))];
        bridge(&w).run_cycle();
        assert_eq!(w.0.borrow().created.len(), 1);
    }

    #[test]
    fn a_summary_has_no_marker_and_every_one_is_made() {
        let w = Fake::default();
        w.0.borrow_mut().access = Some(Access::Granted);
        let mut a = new(0, "3 more things need you");
        a.notes = "Open Neko.".into();
        a.key = "sum-1".into();
        w.0.borrow_mut().plans = vec![Some(plan(vec![a], vec![], vec![]))];
        let mut b = bridge(&w);
        b.run_cycle();
        assert_eq!(w.0.borrow().created.len(), 1);
        assert_eq!(w.0.borrow().done[0].0, vec![ReminderCreated { seq: 0, id: "R1".into(), key: "sum-1".into() }]);
    }

    #[test]
    fn a_failed_save_is_reported_with_an_empty_id_so_the_daemon_can_drop_it() {
        let w = Fake::default();
        w.0.borrow_mut().access = Some(Access::Granted);
        w.0.borrow_mut().fail_create = true;
        w.0.borrow_mut().plans = vec![Some(plan(vec![new(2, "x")], vec![], vec![]))];
        bridge(&w).run_cycle();
        assert_eq!(w.0.borrow().done[0].0, vec![ReminderCreated { seq: 2, id: String::new(), key: "k2".into() }]);
    }

    #[test]
    fn access_is_asked_only_when_the_plan_says_the_channel_is_on_and_only_while_undecided() {
        // Off: the plan is empty and not enabled. macOS is not asked.
        let off = Fake::default();
        off.0.borrow_mut().plans = vec![Some(ReminderPlan::default())];
        bridge(&off).run_cycle();
        assert_eq!(off.0.borrow().asked, 0);
        assert_eq!(off.0.borrow().seen[0].0, RemindersAccess::NotAsked, "the daemon is told it was never asked");
        assert!(off.0.borrow().done.is_empty());
        // On: asked once, the plan is fetched again with the new access and carried out.
        let on = Fake::default();
        on.0.borrow_mut().plans = vec![Some(ReminderPlan { enabled: true, ..ReminderPlan::default() })];
        on.0.borrow_mut().plan_after_permission = Some(plan(vec![new(1, "First")], vec![], vec![]));
        let mut b = bridge(&on);
        b.run_cycle();
        b.run_cycle();
        let world = on.0.borrow();
        assert_eq!(world.asked, 1, "the prompt is asked for once");
        assert_eq!(world.seen[0].0, RemindersAccess::NotAsked);
        assert_eq!(world.seen[1].0, RemindersAccess::Granted, "synced again after the prompt");
        assert_eq!(world.created.len(), 1);
        // Denied: nothing is ever written, and the daemon hears it.
        let denied = Fake::default();
        denied.0.borrow_mut().access = Some(Access::Denied);
        denied.0.borrow_mut().plans = vec![Some(plan(vec![new(1, "x")], vec!["A"], vec![]))];
        bridge(&denied).run_cycle();
        assert_eq!((denied.0.borrow().asked, denied.0.borrow().created.len(), denied.0.borrow().removed.len()), (0, 0, 0));
        assert_eq!(denied.0.borrow().seen[0].0, RemindersAccess::Denied);
        assert!(denied.0.borrow().done.is_empty());
    }

    #[test]
    fn access_granted_later_resets_the_store_once() {
        let w = Fake::default();
        w.0.borrow_mut().access = Some(Access::Denied);
        w.0.borrow_mut().plans = vec![None];
        let mut b = bridge(&w);
        b.run_cycle();
        w.0.borrow_mut().access = Some(Access::Granted);
        b.run_cycle();
        b.run_cycle();
        assert_eq!(w.0.borrow().resets, 1);
    }

    #[test]
    fn a_daemon_that_does_not_answer_is_an_empty_plan() {
        let w = Fake::default();
        granted_with_list(&w, vec![info("A", false, "x\nneko:1")]);
        w.0.borrow_mut().plans = vec![None];
        bridge(&w).run_cycle();
        assert!(w.0.borrow().done.is_empty() && w.0.borrow().created.is_empty() && w.0.borrow().removed.is_empty());
    }

    #[test]
    fn the_plan_drops_empty_urls_and_ids_as_swift_does() {
        let reply = ReminderPlan {
            enabled: true,
            create: vec![ReminderNew { seq: 1, title: "t".into(), notes: "n".into(), url: Some(String::new()), key: "k".into() }, ReminderNew { seq: 2, title: "t".into(), notes: "n".into(), url: Some("neko://x".into()), key: "k2".into() }],
            delete: vec!["D".into()],
            finish: vec![ReminderFinish { id: String::new(), notes: "x".into(), keep_open: false }, ReminderFinish { id: "F".into(), notes: "y".into(), keep_open: true }],
        };
        let p = Plan::of(reply);
        assert_eq!((p.create[0].url.clone(), p.create[1].url.clone()), (None, Some("neko://x".into())));
        assert_eq!(p.finish, vec![Finish { id: "F".into(), notes: "y".into(), keep_open: true }]);
        assert!(p.enabled && p.delete == ["D"]);
    }

    #[test]
    fn the_list_goes_on_icloud_when_there_is_one() {
        let s = |id: &str, title: &str, kind| SourceInfo { id: id.into(), title: title.into(), kind };
        let sources = [s("loc", "On My Mac", SourceKind::Local), s("ex", "Work Exchange", SourceKind::Other), s("ic", "iCloud", SourceKind::OtherAccount), s("g", "Google", SourceKind::OtherAccount)];
        assert_eq!(pick_source(&sources, Some("g")).as_deref(), Some("ic"), "iCloud delivers the alarm to a phone, even when another account is the default");
        assert_eq!(pick_source(&sources, Some("ic")).as_deref(), Some("ic"));
        assert_eq!(pick_source(&sources[..2], Some("ex")).as_deref(), Some("ex"), "no iCloud: the default");
        assert_eq!(pick_source(&sources[..2], None).as_deref(), Some("loc"), "no default: a local list");
        assert_eq!(pick_source(&sources[1..2], None).as_deref(), Some("ex"), "else the first there is");
        assert_eq!(pick_source(&[s("loc", "iCloud", SourceKind::Local)], None).as_deref(), Some("loc"), "a local source called iCloud is not iCloud");
        assert_eq!(pick_source(&[], None), None);
    }

    #[test]
    fn a_reply_line_starts_with_an_arrow_and_has_words() {
        assert!(has_reply("Q\n> yes"));
        assert!(has_reply("Q\n  \t>no"));
        assert!(has_reply("Q\r\n>x\r\n"));
        assert!(!has_reply("Q\n>"));
        assert!(!has_reply("Q\n>  \t"));
        assert!(!has_reply("Q > not at the start"));
        assert!(!has_reply(""));
    }

    #[test]
    fn the_loop_cycles_at_once_every_twenty_seconds_and_two_seconds_after_a_change() {
        use crate::shell::schedule::{Runner, SETTLE};
        let w = Fake::default();
        granted_with_list(&w, vec![]);
        w.0.borrow_mut().plans = vec![None];
        let mut r = Runner::new(bridge(&w));
        r.step(0, true, false);
        assert_eq!(w.0.borrow().seen.len(), 1);
        r.step(19_000, true, false);
        assert_eq!(w.0.borrow().seen.len(), 1);
        r.step(20_000, true, false);
        assert_eq!(w.0.borrow().seen.len(), 2);
        assert_eq!(r.step(21_000, true, true), SETTLE);
        r.step(23_000, true, false);
        assert_eq!(w.0.borrow().seen.len(), 3, "a change cycles once, two seconds later");
        // Handed back: nothing is read, nothing is asked of the daemon.
        for t in [24_000, 40_000, 60_000] {
            r.step(t, false, true);
        }
        assert_eq!(w.0.borrow().seen.len(), 3);
        r.step(61_000, true, false);
        assert_eq!(w.0.borrow().seen.len(), 4, "taken again: a cycle at once");
    }
}
