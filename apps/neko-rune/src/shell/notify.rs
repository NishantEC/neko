//! Routine notifications: a port of `RoutineNotifier.swift`. Needs-you items that routines raise become macOS notifications.
//!
//! * Polls `RoutineNotifications { after }` every 30 s, at most 5 notifications a poll; more than that is one more, "N more things need
//!   you. Open the Inbox." Same words: the title is `<scope> · <item title>`, the body the item's detail.
//! * The first poll on a Mac only records where the list stands, so installing Neko never replays old items. The place is kept in this
//!   client's saved state (`app::load_notify_seq`), as Swift keeps it in `UserDefaults` (`neko.routineNotificationSeq`); a daemon whose list
//!   starts over (a new data folder) is followed down.
//! * A click opens Rune on the Inbox (the Routines page). No actions on the notification.
//! * Only an installed app bundle may use the notification centre (`rune_notify::available`), as Swift's `canNotify`. `rune-notify`
//!   posts without a sound; Swift's carries the default one.
//!
//! The decisions are in [`Poller`] with the daemon and the store passed in, so they are tested without either.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use neko_protocol::routines::RoutineNotice;
use rune::AppHandle;

use crate::app::App;
use crate::link::shell::{routine_notifications, Notices};

pub const POLL_EVERY: Duration = Duration::from_secs(30);
/// `RoutineNotifier.perPoll`.
pub const PER_POLL: usize = 5;
/// Where a click goes (`WebHomeRoute.open("inbox")`).
pub const INBOX_ROUTE: &str = "inbox";

/// One notification to post.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// The notifications for the items after what was already shown: up to [`PER_POLL`], then one for the rest.
pub fn posts(items: &[RoutineNotice], latest: u64) -> Vec<Post> {
    let mut out: Vec<Post> = items.iter().take(PER_POLL).map(|n| Post { id: format!("routine-{}", n.item.seq), title: format!("{} · {}", n.scope, n.item.title), body: n.item.detail.clone() }).collect();
    if items.len() > PER_POLL {
        out.push(Post { id: format!("routine-more-{latest}"), title: "Neko".to_owned(), body: format!("{} more things need you. Open the Inbox.", items.len() - PER_POLL) });
    }
    out
}

/// `RoutineNotifier.poll`'s rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Poller {
    /// Where the list stood at the last poll; None before the first (nothing is posted then).
    known: Option<u64>,
}

impl Poller {
    pub fn new(known: Option<u64>) -> Poller {
        Poller { known }
    }

    /// One poll. `fetch(after)` asks the daemon (None: it did not answer with the list, so nothing changes); `save(latest)` remembers
    /// where the list now stands, always, even on the first poll and even if it moved down.
    pub fn poll(&mut self, fetch: impl FnOnce(u64) -> Option<Notices>, save: impl FnOnce(u64)) -> Vec<Post> {
        let Some(Notices { items, latest }) = fetch(self.known.unwrap_or(0)) else { return Vec::new() };
        let first = self.known.is_none();
        self.known = Some(latest);
        save(latest);
        if first { Vec::new() } else { posts(&items, latest) }
    }
}

impl Poller {
    /// One turn of the loop. `active` is whether Rune holds the notification duty now. Handed back (the Swift app posts them again), it asks
    /// nothing and posts nothing, and forgets where it stood: when the duty comes back the first poll only records where the list stands, so
    /// what the Swift app already showed is never shown twice.
    pub fn turn(&mut self, active: bool, fetch: impl FnOnce(u64) -> Option<Notices>, save: impl FnOnce(u64)) -> Vec<Post> {
        if !active {
            self.known = None;
            return Vec::new();
        }
        self.poll(fetch, save)
    }
}

/// Where a click on a notification goes: the route it carries (a known one), else the Inbox.
pub fn click_target(route: Option<&str>) -> String {
    route.and_then(crate::deeplink::clean_route).unwrap_or_else(|| INBOX_ROUTE.to_owned())
}

/// Starts the notifier: the click handler, and a thread that polls. Does nothing outside an installed app.
pub fn spawn(handle: AppHandle<App>, socket: PathBuf, active: Arc<AtomicBool>) {
    if !rune_notify::available() {
        eprintln!("neko-rune: --app: not an installed app bundle, so routine notifications are off");
        return;
    }
    let opener = std::sync::Mutex::new(handle);
    rune_notify::on_click(move |click| {
        let route = click_target(click.get("route"));
        let handle = opener.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        handle.update_if(move |s, tx| {
            s.open_route(&route);
            tx.instant();
            true
        });
        handle.open();
    });
    let _ = std::thread::Builder::new().name("neko-notify".into()).spawn(move || {
        let mut poller = Poller::new(crate::app::load_notify_seq());
        loop {
            for post in poller.turn(active.load(Ordering::SeqCst), |after| routine_notifications(&socket, after), crate::app::write_notify_seq) {
                rune_notify::post(&rune_notify::Notification::new(post.id, post.title).body(post.body).info("route", INBOX_ROUTE));
            }
            std::thread::sleep(POLL_EVERY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::routines::NeedsYou;

    fn notice(seq: u64, scope: &str, title: &str, detail: &str) -> RoutineNotice {
        RoutineNotice { scope: scope.into(), routine: "r".into(), item: NeedsYou { seq, title: title.into(), detail: detail.into(), link: None, done: false, created_ms: 0, touched_ms: 0, ..serde_json::from_str("{\"seq\":0,\"title\":\"\",\"detail\":\"\"}").unwrap() } }
    }

    fn many(n: u64) -> Vec<RoutineNotice> {
        (1..=n).map(|i| notice(i, "Acme", &format!("Item {i}"), "Needs a look")).collect()
    }

    #[test]
    fn the_title_and_body_are_swifts() {
        let p = posts(&[notice(7, "Acme", "Review the PR", "Two checks failed")], 7);
        assert_eq!(p, [Post { id: "routine-7".into(), title: "Acme · Review the PR".into(), body: "Two checks failed".into() }]);
    }

    #[test]
    fn at_most_five_then_one_more_that_says_how_many() {
        assert_eq!(posts(&many(5), 5).len(), 5);
        let p = posts(&many(8), 8);
        assert_eq!(p.len(), 6);
        assert_eq!(p[4].id, "routine-5");
        assert_eq!(p[5], Post { id: "routine-more-8".into(), title: "Neko".into(), body: "3 more things need you. Open the Inbox.".into() });
        assert!(posts(&[], 3).is_empty());
    }

    #[test]
    fn the_first_poll_only_records_where_the_list_stands() {
        let mut poller = Poller::new(None);
        let (mut asked, mut saved) = (None, None);
        let out = poller.poll(
            |after| {
                asked = Some(after);
                Some(Notices { items: many(3), latest: 3 })
            },
            |l| saved = Some(l),
        );
        assert!(out.is_empty(), "installing Neko never replays old items");
        assert_eq!((asked, saved), (Some(0), Some(3)));
    }

    #[test]
    fn after_advances_each_poll_and_only_new_items_are_posted() {
        let mut poller = Poller::new(Some(3));
        let mut saved = Vec::new();
        let out = poller.poll(|after| { assert_eq!(after, 3); Some(Notices { items: many(2), latest: 5 }) }, |l| saved.push(l));
        assert_eq!(out.len(), 2);
        let out = poller.poll(|after| { assert_eq!(after, 5, "the next poll asks after the last latest"); Some(Notices { items: vec![], latest: 5 }) }, |l| saved.push(l));
        assert!(out.is_empty());
        assert_eq!(saved, [5, 5]);
    }

    #[test]
    fn a_daemon_that_does_not_answer_changes_nothing_and_a_list_that_starts_over_is_followed_down() {
        let mut poller = Poller::new(Some(9));
        let mut saved = Vec::new();
        assert!(poller.poll(|_| None, |l| saved.push(l)).is_empty());
        assert!(saved.is_empty(), "nothing is remembered without an answer");
        let out = poller.poll(|after| { assert_eq!(after, 9); Some(Notices { items: vec![], latest: 2 }) }, |l| saved.push(l));
        assert!(out.is_empty());
        assert_eq!(saved, [2], "a new data folder: the place moves down");
        poller.poll(|after| { assert_eq!(after, 2); None }, |_| {});
    }

    #[test]
    fn handed_back_the_poller_is_silent_and_on_return_only_records_where_the_list_stands() {
        let mut poller = Poller::new(Some(3));
        let out = poller.turn(false, |_| panic!("asks nothing while handed back"), |_| panic!("saves nothing while handed back"));
        assert!(out.is_empty());
        let mut saved = None;
        let out = poller.turn(true, |after| { assert_eq!(after, 0); Some(Notices { items: many(4), latest: 7 }) }, |l| saved = Some(l));
        assert!(out.is_empty(), "what the Swift app showed meanwhile is not shown again");
        assert_eq!(saved, Some(7));
        let out = poller.turn(true, |after| { assert_eq!(after, 7); Some(Notices { items: many(1), latest: 8 }) }, |_| {});
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_click_opens_the_inbox_unless_it_carries_a_known_route() {
        assert_eq!(click_target(None), "inbox");
        assert_eq!(click_target(Some("inbox")), "inbox");
        assert_eq!(click_target(Some("tasks?task=t1")), "tasks?task=t1");
        assert_eq!(click_target(Some("https://example.com")), "inbox");
        assert_eq!(click_target(Some("../etc")), "inbox");
    }
}
