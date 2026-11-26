//! Mac notifications for "Needs you" (docs/knowledge/needs-you.md), as data first: which notification a change would post, with
//! which actions, and when one is taken back. Posting goes through a [`Poster`]: the real window hands in [`System`] (Rune's
//! `rune-notify`, `UNUserNotificationCenter`); everything else, tests and pictures included, keeps the [`Recorder`], so nothing
//! here ever reaches the system unless the real app is running.
//!
//! - One notification per task at a time (its id is the task's): a newer one for the same task replaces it, which is the closest
//!   `rune-notify` comes to grouping by task (it cannot set a thread identifier).
//! - Never more than one per item: an item is posted once, the first time it is seen, and never again.
//! - Taken back when its item leaves the list (answered here, in the task, or by a notification), and when the person opens the
//!   task.
//! - Posted only when it would not be noise: the app is not in front, or the person is looking at another task (the Needs you list
//!   shows every item, so it counts as looking).
//! - The morning briefing is one more notification (`neko.briefing`, title "Your briefing: 3 finished, 1 needs you"), posted when the
//!   daemon says it is the morning and the window has news, unless the person has the Briefing open in front. Its click opens
//!   `neko://briefing`; opening the page takes it back.
//! - A permission request carries Allow once (first), Always allow and Deny; a destructive one carries none and only opens the
//!   task. A question carries a reply field. A failure only opens the task.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use neko_protocol::{Decision, Need, NeedsItem, PermissionId, RunId, TaskId};

/// The actions' ids, as a click hands them back.
pub const ALLOW_ONCE: &str = "allow_once";
pub const ALWAYS_ALLOW: &str = "always_allow";
pub const DENY: &str = "deny";
pub const REPLY: &str = "reply";

/// The longest a notification's body is, in characters.
pub const BODY_CHARS: usize = 140;

/// The three switches on Settings ▸ Notifications, all on until the person turns one off. Kept in the app's prefs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Switches {
    /// A notification when a task starts needing the person.
    pub needs: bool,
    /// A notification when a run finishes.
    pub finished: bool,
    /// The count on the Dock icon.
    pub badge: bool,
}

impl Default for Switches {
    fn default() -> Self {
        Switches { needs: true, finished: true, badge: true }
    }
}

/// The prefs key the switches are kept under.
pub const SWITCHES_KEY: &str = "notifications";

/// Whether the Mac lets Neko post, as the system says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Allowed {
    /// Not read yet.
    #[default]
    Unknown,
    /// The person has not been asked: Neko asks the first time it would post.
    NotAsked,
    Granted,
    /// Turned off in System Settings.
    Denied,
    /// This copy cannot post at all: it is not running as an app bundle (`cargo run`, a test, a picture).
    Unavailable,
}

impl Allowed {
    fn code(self) -> u8 {
        match self {
            Allowed::Unknown => 0,
            Allowed::NotAsked => 1,
            Allowed::Granted => 2,
            Allowed::Denied => 3,
            Allowed::Unavailable => 4,
        }
    }

    pub fn from_code(c: u8) -> Allowed {
        match c {
            1 => Allowed::NotAsked,
            2 => Allowed::Granted,
            3 => Allowed::Denied,
            4 => Allowed::Unavailable,
            _ => Allowed::Unknown,
        }
    }
}

/// A button on a notification, or its reply field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Button {
    Action { id: &'static str, label: &'static str },
    Reply { id: &'static str, placeholder: &'static str, label: &'static str },
}

/// One notification, as data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// Posting another with the same id replaces it: one per task.
    pub id: String,
    pub title: String,
    pub body: String,
    /// What a click hands back: the task, the item, the link, and what an action needs.
    pub info: Vec<(String, String)>,
    pub buttons: Vec<Button>,
}

impl Note {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.info.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// Where notifications go.
pub trait Poster {
    fn post(&self, n: &Note);
    fn withdraw(&self, ids: &[String]);
}

/// What a [`Recorder`] saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sent {
    Post(Note),
    Withdraw(Vec<String>),
}

/// The poster tests and pictures use: it keeps what would have been posted, and posts nothing.
#[derive(Clone, Default)]
pub struct Recorder(pub Rc<RefCell<Vec<Sent>>>);

impl Recorder {
    /// What was sent since the last look.
    pub fn take(&self) -> Vec<Sent> {
        std::mem::take(&mut self.0.borrow_mut())
    }
}

impl Poster for Recorder {
    fn post(&self, n: &Note) {
        self.0.borrow_mut().push(Sent::Post(n.clone()));
    }
    fn withdraw(&self, ids: &[String]) {
        self.0.borrow_mut().push(Sent::Withdraw(ids.to_vec()));
    }
}

/// With `NEKO_NOTIFY_LOG` naming a file, each notification posted or taken back, and each Dock badge count, is appended to it as
/// one JSON line, so the QA rig can check what a real window did without reading the system's notifications (docs/knowledge/qa.md).
pub fn log_event(event: serde_json::Value) {
    use std::io::Write;
    let Some(path) = std::env::var_os("NEKO_NOTIFY_LOG").filter(|p| !p.is_empty()) else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{event}");
    }
}

/// The real thing: Rune's `rune-notify`. The first post asks the person for permission (the system does, once); a post while it is
/// refused is dropped by the system. Silent: no sound is set, so the system's own rules (Focus among them) apply untouched.
pub struct System;

impl Poster for System {
    fn post(&self, n: &Note) {
        let mut r = rune_notify::Notification::new(n.id.clone(), n.title.clone()).body(n.body.clone());
        for (k, v) in &n.info {
            r = r.info(k.clone(), v.clone());
        }
        for b in &n.buttons {
            r = match b {
                Button::Action { id, label } => r.action(*id, *label),
                Button::Reply { id, placeholder, label } => r.reply(*id, *placeholder, *label),
            };
        }
        let posted = rune_notify::post(&r);
        let buttons: Vec<String> = n
            .buttons
            .iter()
            .map(|b| match b {
                Button::Action { label, .. } => (*label).to_owned(),
                Button::Reply { label, .. } => format!("reply: {label}"),
            })
            .collect();
        log_event(serde_json::json!({"post": n.id, "title": n.title, "body": n.body, "buttons": buttons, "posted": posted}));
    }
    fn withdraw(&self, ids: &[String]) {
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        rune_notify::remove(&ids);
        log_event(serde_json::json!({"withdraw": ids}));
    }
}

/// Where the person is when something would be posted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Here {
    /// Neko is the frontmost app with its window focused.
    pub front: bool,
    /// The task in view, if one is.
    pub task: Option<TaskId>,
    /// The Needs you list is in view.
    pub needs_view: bool,
    /// The Briefing is in view.
    pub briefing_view: bool,
}

/// What a task's notification is for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Shown {
    Item(String),
    /// "Done", or "That was already answered": nothing to take back but on opening the task.
    Said,
}

/// The notification model. It decides; its [`Poster`] posts.
pub struct Notifier {
    pub switches: Switches,
    poster: Box<dyn Poster>,
    /// Each task's notification now in Notification Center, and what it is for.
    posted: HashMap<TaskId, Shown>,
    /// Every item already posted (or already there when the app first heard the list): never posted again.
    notified: HashSet<String>,
    /// The first list has been heard (what was waiting before the app started is not news).
    heard: bool,
    /// The morning briefing's notification is in Notification Center.
    briefing_posted: bool,
}

impl Default for Notifier {
    fn default() -> Self {
        Notifier::new(Box::new(Recorder::default()))
    }
}

/// A task's notification id.
pub fn note_id(task: &TaskId) -> String {
    format!("neko.task.{}", task.0)
}

/// The id of Settings' test notification.
pub const TEST_ID: &str = "neko.test";

impl Notifier {
    pub fn new(poster: Box<dyn Poster>) -> Notifier {
        Notifier { switches: Switches::default(), poster, posted: HashMap::new(), notified: HashSet::new(), heard: false, briefing_posted: false }
    }

    /// The daemon's list, as it is now. Takes back the notifications whose item is gone, and posts one for each item never seen
    /// before (oldest first), unless the switch is off or the person is already looking.
    pub fn needs(&mut self, items: &[NeedsItem], here: &Here) {
        let live: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
        let mut gone: Vec<TaskId> = self.posted.iter().filter(|(_, s)| matches!(s, Shown::Item(i) if !live.contains(i.as_str()))).map(|(t, _)| t.clone()).collect();
        gone.sort();
        if !gone.is_empty() {
            for t in &gone {
                self.posted.remove(t);
            }
            self.poster.withdraw(&gone.iter().map(note_id).collect::<Vec<_>>());
        }
        if !std::mem::replace(&mut self.heard, true) {
            self.notified.extend(items.iter().map(|i| i.id.clone()));
            return;
        }
        for item in items {
            if !self.notified.insert(item.id.clone()) {
                continue;
            }
            if !self.switches.needs || !wanted(here, &item.task, true) {
                continue;
            }
            self.poster.post(&note_for(item));
            self.posted.insert(item.task.clone(), Shown::Item(item.id.clone()));
        }
    }

    /// A run finished with nothing asked: "Done" and the first line of its answer.
    pub fn finished(&mut self, task: &TaskId, title: &str, cat: &str, answer: Option<&str>, here: &Here) {
        if !self.switches.finished || !wanted(here, task, false) {
            return;
        }
        self.poster.post(&done_note(task, title, cat, answer));
        self.posted.insert(task.clone(), Shown::Said);
    }

    /// The person opened the task: its notification has done its work.
    pub fn opened(&mut self, task: &TaskId) {
        if self.posted.remove(task).is_some() {
            self.poster.withdraw(&[note_id(task)]);
        }
    }

    /// An action or reply from a notification came too late: says so plainly, in the task's notification.
    pub fn stale(&mut self, task: &TaskId, title: &str) {
        self.poster.post(&Note { id: note_id(task), title: title.to_owned(), body: "That was already answered.".into(), info: link_info(task, None), buttons: Vec::new() });
        self.posted.insert(task.clone(), Shown::Said);
    }

    /// The morning: "Your briefing: 3 finished, 1 needs you", unless the person is looking at the Briefing already.
    pub fn briefing(&mut self, counts: &neko_protocol::BriefingCounts, body: &str, here: &Here) {
        if !counts.news || (here.front && here.briefing_view) {
            return;
        }
        self.poster.post(&briefing_note(counts, body));
        self.briefing_posted = true;
    }

    /// The person opened the Briefing (or marked it read): its notification has done its work.
    pub fn briefing_opened(&mut self) {
        if std::mem::take(&mut self.briefing_posted) {
            self.poster.withdraw(&[BRIEFING_NOTE.to_owned()]);
        }
    }

    /// Settings' "Send a test notification".
    pub fn test(&mut self) {
        self.poster.post(&Note { id: TEST_ID.into(), title: "Neko".into(), body: "Notifications from Neko look like this.".into(), info: Vec::new(), buttons: Vec::new() });
    }

    /// Whether a notification for `task` is in Notification Center now.
    pub fn showing(&self, task: &TaskId) -> bool {
        self.posted.contains_key(task)
    }
}

/// Whether a notification about `task` is worth posting: the app is not in front, or the person looks at something else (the
/// Needs you list counts as looking at every item).
pub fn wanted(here: &Here, task: &TaskId, item: bool) -> bool {
    !here.front || (here.task.as_ref() != Some(task) && !(item && here.needs_view))
}

fn link_info(task: &TaskId, item: Option<&str>) -> Vec<(String, String)> {
    let mut info = vec![("task".to_owned(), task.0.clone()), ("url".to_owned(), crate::deeplink::url(task))];
    if let Some(i) = item {
        info.push(("item".to_owned(), i.to_owned()));
    }
    info
}

/// The morning briefing's notification id.
pub const BRIEFING_NOTE: &str = "neko.briefing";

/// The morning's notification: the counts in its title, one line of what is first in its body; a click opens `neko://briefing`.
pub fn briefing_note(counts: &neko_protocol::BriefingCounts, body: &str) -> Note {
    Note {
        id: BRIEFING_NOTE.into(),
        title: format!("Your briefing: {}", counts.headline()),
        body: short(body, BODY_CHARS),
        info: vec![("briefing".to_owned(), "1".to_owned()), ("url".to_owned(), crate::deeplink::BRIEFING_URL.to_owned())],
        buttons: Vec::new(),
    }
}

/// One line, spaces collapsed, cut at a word to `max` characters.
pub fn short(s: &str, max: usize) -> String {
    let line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let cut: String = line.chars().take(max.saturating_sub(1)).collect();
    let at = cut.rfind(' ').filter(|&i| i > max / 2).unwrap_or(cut.len());
    format!("{}…", cut[..at].trim_end())
}

/// The notification for an item: the task's title, the cat and what is asked in short ("miso asks: …"), and its actions.
pub fn note_for(item: &NeedsItem) -> Note {
    let mut info = link_info(&item.task, Some(&item.id));
    let (body, buttons) = match &item.need {
        Need::Permission { permission: p } => {
            info.push(("permission".into(), p.id.0.clone()));
            let body = short(&format!("{} asks: {}: {}", item.cat_name, p.title, p.detail), BODY_CHARS);
            if p.destructive {
                // The destructive form has no default and never confirms from a banner: the notification only opens the task.
                info.push(("destructive".into(), "1".into()));
                (body, Vec::new())
            } else {
                let b = vec![
                    Button::Action { id: ALLOW_ONCE, label: "Allow once" },
                    Button::Action { id: ALWAYS_ALLOW, label: "Always allow" },
                    Button::Action { id: DENY, label: "Deny" },
                ];
                (body, b)
            }
        }
        Need::Question { run, question } => {
            info.push(("run".into(), run.0.clone()));
            (short(&format!("{} asks: {question}", item.cat_name), BODY_CHARS), vec![Button::Reply { id: REPLY, placeholder: "Your answer", label: "Send" }])
        }
        Need::Failure { text, .. } => (short(&format!("{} could not go on: {text}", item.cat_name), BODY_CHARS), Vec::new()),
    };
    Note { id: note_id(&item.task), title: item.title.clone(), body, info, buttons }
}

/// "tofu is done", with the first line of the answer when there is one.
pub fn done_note(task: &TaskId, title: &str, cat: &str, answer: Option<&str>) -> Note {
    let body = match answer.and_then(first_line) {
        Some(a) => short(&format!("{cat} is done: {a}"), BODY_CHARS),
        None => format!("{cat} is done"),
    };
    Note { id: note_id(task), title: title.to_owned(), body, info: link_info(task, None), buttons: Vec::new() }
}

/// The first line of an answer with words in it, without Markdown's marks.
pub fn first_line(words: &str) -> Option<String> {
    let line = words.lines().map(|l| l.trim().trim_start_matches(['#', '>', '-', '*']).trim()).find(|l| !l.is_empty())?;
    let plain = line.replace("**", "").replace("__", "").replace('`', "");
    Some(plain.trim().to_owned()).filter(|l| !l.is_empty())
}

/// What a click on a notification asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClickDo {
    /// Open Neko at the task (a click on the notification itself).
    Open(TaskId),
    /// An action on a permission request: the same answer the app's own control sends.
    Answer { task: TaskId, permission: PermissionId, decision: Decision },
    /// Words typed in a question's reply field: the same follow-up the app's reply box sends.
    Reply { task: TaskId, run: RunId, words: String },
    /// Open Neko at the Briefing (a click on the morning notification).
    OpenBriefing,
    /// Not one of Neko's.
    Nothing,
}

/// Reads a click: its action (None for the notification itself), the words typed, and its info. Anything odd opens the task at
/// most; an action never answers a destructive request.
pub fn read_click(action: Option<&str>, typed: Option<&str>, info: &[(String, String)]) -> ClickDo {
    let get = |k: &str| info.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
    // The morning briefing's notification has no actions: any click on it opens the Briefing.
    if get("briefing") == Some("1") && get("task").is_none() {
        return ClickDo::OpenBriefing;
    }
    let Some(task) = get("task").filter(|t| crate::deeplink::is_task_id(t)).map(|t| TaskId(t.to_owned())) else { return ClickDo::Nothing };
    let open = ClickDo::Open(task.clone());
    match action {
        None => open,
        Some(a @ (ALLOW_ONCE | ALWAYS_ALLOW | DENY)) => {
            let Some(p) = get("permission").filter(|p| !p.is_empty()) else { return open };
            if get("destructive") == Some("1") {
                return open;
            }
            let decision = match a {
                ALLOW_ONCE => Decision::AllowOnce,
                ALWAYS_ALLOW => Decision::AlwaysAllow,
                _ => Decision::Deny,
            };
            ClickDo::Answer { task, permission: PermissionId(p.to_owned()), decision }
        }
        Some(REPLY) => match (get("run").filter(|r| !r.is_empty()), typed.map(str::trim).filter(|w| !w.is_empty())) {
            (Some(run), Some(words)) => ClickDo::Reply { task, run: RunId(run.to_owned()), words: words.to_owned() },
            _ => open,
        },
        Some(_) => open,
    }
}

/// Asks the system, without asking the person, whether Neko may post, and keeps the answer in `out` (read each frame). Only a
/// bundled app can ask; anything else is [`Allowed::Unavailable`].
pub fn read_allowed(out: std::sync::Arc<std::sync::atomic::AtomicU8>) {
    use std::sync::atomic::Ordering;
    if !rune_notify::available() {
        out.store(Allowed::Unavailable.code(), Ordering::SeqCst);
        return;
    }
    #[cfg(target_os = "macos")]
    mac::read(out);
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};

    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    use super::Allowed;

    /// `getNotificationSettingsWithCompletionHandler:`: reads `authorizationStatus` (0 not determined, 1 denied, 2 authorized, 3
    /// provisional, 4 ephemeral). It never shows anything to the person.
    pub fn read(out: Arc<AtomicU8>) {
        let done = RcBlock::new(move |settings: *mut AnyObject| {
            if settings.is_null() {
                return;
            }
            // SAFETY: the center hands a live UNNotificationSettings; `authorizationStatus` is an NSInteger getter.
            let status: isize = unsafe { msg_send![settings, authorizationStatus] };
            let a = match status {
                0 => Allowed::NotAsked,
                1 => Allowed::Denied,
                _ => Allowed::Granted,
            };
            out.store(a.code(), Ordering::SeqCst);
        });
        // SAFETY: gated on `rune_notify::available()` (a bundled app, where the center exists; rune-notify links the framework);
        // the center copies the block.
        unsafe {
            let center: *mut AnyObject = msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
            let _: () = msg_send![center, getNotificationSettingsWithCompletionHandler: &*done];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::{FailureKind, Permission, PermissionState};

    const T1: &str = "task_00000000000000000000000001";
    const T2: &str = "task_00000000000000000000000002";

    fn perm(task: &str, destructive: bool) -> NeedsItem {
        let p = Permission {
            id: PermissionId(format!("perm_{task}")),
            task: TaskId(task.into()),
            run: RunId("run_1".into()),
            tool: "Bash".into(),
            title: "Run a command".into(),
            why: "Commands can change anything on this Mac.".into(),
            detail: if destructive { "rm -rf build".into() } else { "cargo test".into() },
            destructive,
            state: PermissionState::Pending,
            asked_ms: 1,
            answered_ms: None,
        };
        NeedsItem { id: format!("perm:{}", p.id.0), task: p.task.clone(), workspace: "default".into(), title: "Run the tests".into(), cat: "cat_miso".into(), cat_name: "miso".into(), since_ms: 1, need: Need::Permission { permission: p } }
    }

    fn question(task: &str) -> NeedsItem {
        NeedsItem { id: format!("question:{task}"), task: TaskId(task.into()), workspace: "default".into(), title: "Rename the button".into(), cat: "cat_mochi".into(), cat_name: "mochi".into(), since_ms: 2, need: Need::Question { run: RunId("run_2".into()), question: "Should I rename all four?".into() } }
    }

    fn failure(task: &str) -> NeedsItem {
        NeedsItem {
            id: format!("failure:{task}"),
            task: TaskId(task.into()),
            workspace: "default".into(),
            title: "Tidy the folder".into(),
            cat: "cat_tofu".into(),
            cat_name: "tofu".into(),
            since_ms: 3,
            need: Need::Failure { failure: FailureKind::NotSignedIn, text: "Claude Code is not signed in.".into(), next: None, command: Some("claude auth login".into()) },
        }
    }

    fn away() -> Here {
        Here { front: false, task: None, needs_view: false, ..Here::default() }
    }

    fn notifier() -> (Notifier, Recorder) {
        let r = Recorder::default();
        let mut n = Notifier::new(Box::new(r.clone()));
        n.needs(&[], &away());
        (n, r)
    }

    fn posted(sent: &[Sent]) -> Vec<&Note> {
        sent.iter().filter_map(|s| if let Sent::Post(n) = s { Some(n) } else { None }).collect()
    }

    #[test]
    fn each_kind_of_item_posts_one_notification_with_its_actions() {
        let (mut n, r) = notifier();
        n.needs(&[perm(T1, false)], &away());
        let sent = r.take();
        let p = posted(&sent);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].id.as_str(), p[0].title.as_str(), p[0].body.as_str()), ("neko.task.task_00000000000000000000000001", "Run the tests", "miso asks: Run a command: cargo test"));
        assert_eq!(
            p[0].buttons,
            vec![Button::Action { id: ALLOW_ONCE, label: "Allow once" }, Button::Action { id: ALWAYS_ALLOW, label: "Always allow" }, Button::Action { id: DENY, label: "Deny" }],
            "the default first"
        );
        assert_eq!(p[0].get("url"), Some("neko://task/task_00000000000000000000000001"), "a click opens the task by its link");
        n.needs(&[perm(T1, false)], &away());
        assert!(r.take().is_empty(), "never twice for one item");
        let d = note_for(&perm(T2, true));
        assert!(d.buttons.is_empty(), "a destructive request's notification only opens the task: {d:?}");
        let q = note_for(&question(T2));
        assert_eq!((q.body.as_str(), &q.buttons[..]), ("mochi asks: Should I rename all four?", &[Button::Reply { id: REPLY, placeholder: "Your answer", label: "Send" }][..]));
        let f = note_for(&failure(T2));
        assert_eq!((f.body.as_str(), f.buttons.len()), ("tofu could not go on: Claude Code is not signed in.", 0));
    }

    #[test]
    fn nothing_is_posted_for_what_was_already_waiting_or_where_the_person_looks() {
        let r = Recorder::default();
        let mut n = Notifier::new(Box::new(r.clone()));
        n.needs(&[perm(T1, false)], &away());
        assert!(r.take().is_empty(), "the first list after launch is not news");
        let front_in_task = Here { front: true, task: Some(TaskId(T2.into())), needs_view: false, ..Here::default() };
        n.needs(&[perm(T1, false), question(T2)], &front_in_task);
        assert!(r.take().is_empty(), "the person is in that task, in front");
        let front_elsewhere = Here { front: true, task: Some(TaskId(T1.into())), needs_view: false, ..Here::default() };
        n.needs(&[perm(T1, false), question(T2), failure("task_00000000000000000000000003")], &front_elsewhere);
        assert_eq!(posted(&r.take()).len(), 1, "in front, but in a different task");
        let in_list = Here { front: true, task: None, needs_view: true, ..Here::default() };
        n.needs(&[perm(T1, false), question(T2), failure("task_00000000000000000000000003"), failure("task_00000000000000000000000004")], &in_list);
        assert!(r.take().is_empty(), "the Needs you list shows it");
        let home = Here { front: true, task: None, needs_view: false, ..Here::default() };
        n.needs(&[failure("task_00000000000000000000000005")], &home);
        assert_eq!(posted(&r.take()).len(), 1, "Home does not show it");
    }

    #[test]
    fn a_notification_is_taken_back_when_its_item_is_answered_elsewhere_or_the_task_is_opened() {
        let (mut n, r) = notifier();
        n.needs(&[perm(T1, false), question(T2)], &away());
        r.take();
        n.needs(&[question(T2)], &away());
        assert_eq!(r.take(), vec![Sent::Withdraw(vec![note_id(&TaskId(T1.into()))])]);
        n.opened(&TaskId(T2.into()));
        assert_eq!(r.take(), vec![Sent::Withdraw(vec![note_id(&TaskId(T2.into()))])]);
        n.opened(&TaskId(T2.into()));
        assert!(r.take().is_empty(), "nothing to take back twice");
        n.needs(&[], &away());
        assert!(r.take().is_empty(), "already taken back");
    }

    #[test]
    fn the_switches_hold() {
        let (mut n, r) = notifier();
        n.switches.needs = false;
        n.needs(&[perm(T1, false)], &away());
        assert!(r.take().is_empty(), "needs off");
        n.finished(&TaskId(T2.into()), "Draft the notes", "tofu", Some("I drafted them.\n\nMore."), &away());
        assert_eq!(posted(&r.take())[0].body, "tofu is done: I drafted them.");
        n.switches.finished = false;
        n.finished(&TaskId(T2.into()), "Draft the notes", "tofu", None, &away());
        assert!(r.take().is_empty(), "finished off");
        n.switches.needs = true;
        n.needs(&[perm(T1, false)], &away());
        assert!(r.take().is_empty(), "an item seen while the switch was off is not posted later");
        assert_eq!(Switches::default(), Switches { needs: true, finished: true, badge: true }, "all on by default");
    }

    #[test]
    fn a_finished_run_says_done_with_its_first_line_unless_the_person_is_in_that_task() {
        let (mut n, r) = notifier();
        let here = Here { front: true, task: Some(TaskId(T1.into())), needs_view: true, ..Here::default() };
        n.finished(&TaskId(T1.into()), "Draft the notes", "miso", Some("Done."), &here);
        assert!(r.take().is_empty(), "the person is looking at it");
        n.finished(&TaskId(T2.into()), "Draft the notes", "tofu", Some("## I renamed **the button**\n\n- and more"), &here);
        let sent = r.take();
        assert_eq!(posted(&sent)[0].body, "tofu is done: I renamed the button", "the list shows needs only, so a finished task is news");
        assert_eq!(done_note(&TaskId(T2.into()), "x", "tofu", None).body, "tofu is done");
        assert_eq!(first_line("\n\n  \n- `cargo test` passes"), Some("cargo test passes".into()));
    }

    #[test]
    fn a_click_is_read_strictly() {
        let n = note_for(&perm(T1, false));
        assert_eq!(read_click(None, None, &n.info), ClickDo::Open(TaskId(T1.into())));
        assert_eq!(read_click(Some(ALWAYS_ALLOW), None, &n.info), ClickDo::Answer { task: TaskId(T1.into()), permission: PermissionId(format!("perm_{T1}")), decision: Decision::AlwaysAllow });
        assert_eq!(read_click(Some(DENY), None, &n.info), ClickDo::Answer { task: TaskId(T1.into()), permission: PermissionId(format!("perm_{T1}")), decision: Decision::Deny });
        let d = note_for(&perm(T1, true));
        assert_eq!(read_click(Some(ALLOW_ONCE), None, &d.info), ClickDo::Open(TaskId(T1.into())), "never confirms a destructive step");
        let q = note_for(&question(T2));
        assert_eq!(read_click(Some(REPLY), Some("  All four. "), &q.info), ClickDo::Reply { task: TaskId(T2.into()), run: RunId("run_2".into()), words: "All four.".into() });
        assert_eq!(read_click(Some(REPLY), Some("  "), &q.info), ClickDo::Open(TaskId(T2.into())), "nothing typed: open the task");
        assert_eq!(read_click(None, None, &[("task".into(), "../etc".into())]), ClickDo::Nothing);
        assert_eq!(read_click(Some("unknown"), None, &q.info), ClickDo::Open(TaskId(T2.into())));
    }

    #[test]
    fn stale_answers_say_so_plainly() {
        let (mut n, r) = notifier();
        n.stale(&TaskId(T1.into()), "Run the tests");
        let sent = r.take();
        let p = posted(&sent);
        assert_eq!((p[0].id.as_str(), p[0].body.as_str()), ("neko.task.task_00000000000000000000000001", "That was already answered."));
        assert!(n.showing(&TaskId(T1.into())));
    }

    #[test]
    fn the_morning_briefing_is_one_notification_with_the_counts_and_its_own_click() {
        let (mut n, r) = notifier();
        let counts = neko_protocol::BriefingCounts { needs_you: 1, finished: 3, working: 0, rounds_ran: 0, rounds_not_run: 0, news: true };
        n.briefing(&counts, "mochi asks: Run a command: cargo test", &away());
        let sent = r.take();
        let p = posted(&sent);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].id.as_str(), p[0].title.as_str(), p[0].body.as_str()), ("neko.briefing", "Your briefing: 3 finished, 1 needs you", "mochi asks: Run a command: cargo test"));
        assert!(p[0].buttons.is_empty(), "no actions: a click opens the Briefing");
        assert_eq!(read_click(None, None, &p[0].info), ClickDo::OpenBriefing);
        assert!(p[0].info.contains(&("url".to_owned(), "neko://briefing".to_owned())));
        // Opening the Briefing takes it back, once.
        n.briefing_opened();
        assert_eq!(r.take(), vec![Sent::Withdraw(vec!["neko.briefing".to_owned()])]);
        n.briefing_opened();
        assert!(r.take().is_empty(), "nothing left to take back");
    }

    #[test]
    fn the_morning_briefing_stays_quiet_when_there_is_no_news_or_the_person_is_reading_it() {
        let (mut n, r) = notifier();
        let counts = neko_protocol::BriefingCounts { finished: 1, news: true, ..Default::default() };
        n.briefing(&neko_protocol::BriefingCounts { news: false, ..counts }, "", &away());
        n.briefing(&counts, "", &Here { front: true, briefing_view: true, ..Here::default() });
        assert!(r.take().is_empty());
        n.briefing(&counts, "", &Here { front: true, task: None, needs_view: false, briefing_view: false });
        assert_eq!(posted(&r.take()).len(), 1, "in front on another page is worth one");
    }

    #[test]
    fn bodies_are_short() {
        let s = short(&"word ".repeat(100), BODY_CHARS);
        assert!(s.chars().count() <= BODY_CHARS && s.ends_with('…'), "{s}");
        assert_eq!(short("a\n  b", 10), "a b");
    }
}
