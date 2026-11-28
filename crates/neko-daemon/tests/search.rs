//! Search in the real daemon (docs/knowledge/search.md): the request and its reply over the wire, and that it stays fast on a large
//! store. The store is made before the daemon starts, from generated threads and messages (no run, no model), in one transaction.
//! Every wait has a deadline.

mod support;
use std::time::{Duration, Instant};

use neko_core::Store;
use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

/// The spec's size: ten thousand threads, four messages each.
const THREADS: u64 = 10_000;
const PARTS: u64 = 4;
/// What one search may take, asked of the daemon over the socket. The spec says 50 ms; a development build is slower than the shipped
/// one (the bundled SQLite is built without optimisation), so the figure that is held here is the spec's, with the measured figures
/// printed by `--nocapture`.
const BUDGET: Duration = Duration::from_millis(50);

/// A small made-up vocabulary, the same every run.
fn vocabulary() -> Vec<String> {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    (0..400)
        .map(|_| {
            let mut w = String::new();
            for _ in 0..(3 + (x >> 60) % 5) {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                w.push((b'a' + (x % 26) as u8) as char);
            }
            w
        })
        .collect()
}

/// Three days ago, so nothing is old enough for the week's housekeeping to settle (it would put the whole store away).
fn base() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64 - 3 * 24 * 3_600_000
}

fn seed(dir: &std::path::Path) {
    let t0 = base();
    // The store as the version before search left it: the daemon migrates it (builds both indexes) when it starts, as it does for a
    // person who has used Neko for a year.
    let store = Store::open_at(&dir.join("neko.sqlite3"), 13).unwrap();
    store.ensure_default_workspace(t0).unwrap();
    let words = vocabulary();
    let mut x: u64 = 88_172_645_463_325_252;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut sql = String::from("BEGIN;\n");
    for i in 0..THREADS {
        let title = format!("{} {} {}", words[(next() % 400) as usize], words[(next() % 400) as usize], words[(next() % 400) as usize]);
        // Newest threads have the highest numbers; every tenth is a thread that works.
        sql += &format!("INSERT INTO tasks (id, workspace_id, words, title, status, created_ms, last_read_ms, activity_ms) VALUES ('task_{i:06}', 'default', '{title}', '{title}', 'done', {0}, {0}, {0});\n", t0 + i as i64 * 1000);
        for p in 0..PARTS {
            let text: Vec<&str> = (0..60).map(|_| words[(next() % 400) as usize].as_str()).collect();
            let mut text = text.join(" ");
            // One thread in all of them said the needle; every thread that is a multiple of 7 said the common word.
            if i == 6_173 && p == 2 {
                text += " and the pomegranate harvest was late";
            }
            if i % 7 == 0 && p == 1 {
                text += " refactor";
            }
            let (kind, body) = if p % 2 == 0 { ("person", format!(r#"{{"kind":"person","text":"{text}"}}"#)) } else { ("agent", format!(r#"{{"kind":"agent","text":"{text}","done":true}}"#)) };
            sql += &format!("INSERT INTO parts (id, task_id, seq, kind, body, at_ms) VALUES ('part_{i:06}_{p}', 'task_{i:06}', {}, '{kind}', '{}', {});\n", p + 1, body.replace('\'', "''"), t0 + i as i64 * 1000 + p as i64);
        }
    }
    sql += "COMMIT;";
    store.connection().execute_batch(&sql).unwrap();
}

fn found(c: &mut Client, query: &str, limit: Option<u32>) -> (Vec<Found>, Option<String>, Duration, u32) {
    let at = Instant::now();
    match c.request(Request::Search { query: query.into(), limit }).unwrap() {
        Reply::Found { hits, problem, took_ms, .. } => (hits, problem, at.elapsed(), took_ms),
        r => panic!("search: {r:?}"),
    }
}

#[test]
fn a_search_over_ten_thousand_threads_answers_within_the_budget() {
    let dir = temp_dir("search-speed");
    let t = Instant::now();
    seed(&dir);
    println!("seeded {THREADS} threads and {} messages in {:?}", THREADS * PARTS, t.elapsed());
    let t = Instant::now();
    let d = Daemon::start_in(dir);
    println!("the daemon migrated and indexed it in {:?}", t.elapsed());
    let mut c = d.client();
    // One search first: the first read of a file the daemon has not touched is the cold one, as it is for the person's first ⌘K.
    let _ = found(&mut c, "warm", None);
    let queries = ["pomegranate", "refactor", "pom", "re", "status:needs", "", "refactor project:none since:4d", "\"pomegranate harvest\"", "zzzzzz"];
    let mut slowest = Duration::ZERO;
    for q in queries {
        // The best of three, so one stalled scheduler tick does not decide a test about the typical case.
        let best = (0..3).map(|_| found(&mut c, q, None)).min_by_key(|r| r.2).unwrap();
        println!("search {q:?}: {} hits, {:?} over the socket ({} ms in the daemon)", best.0.len(), best.2, best.3);
        slowest = slowest.max(best.2);
        assert!(best.0.len() <= 12, "the limit holds");
        assert!(best.2 <= BUDGET, "{q:?} took {:?}, over the {BUDGET:?} budget", best.2);
    }
    println!("slowest: {slowest:?}");
    let (hits, _, _, _) = found(&mut c, "pomegranate", None);
    assert_eq!(hits.len(), 1, "the needle is in one message of one thread");
    assert_eq!((hits[0].task.0.as_str(), hits[0].by, hits[0].seq), ("task_006173", FoundBy::Message, 3));
    assert!(hits[0].snippet.contains("pomegranate harvest"), "{:?}", hits[0].snippet);
    let (hits, _, _, _) = found(&mut c, "refactor", None);
    assert_eq!(hits.len(), 12, "a common word fills the list, and no thread is listed twice");
    let (recent, _, _, _) = found(&mut c, "", Some(5));
    assert_eq!(recent.iter().map(|f| f.task.0.as_str()).collect::<Vec<_>>(), ["task_009999", "task_009998", "task_009997", "task_009996", "task_009995"], "nothing typed: the newest");
}

#[test]
fn the_request_is_answered_while_a_thread_runs_and_a_bad_filter_says_why() {
    let d = Daemon::start("search-live");
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    let Reply::Task { task } = c.request(Request::CreateTask { crew: None, words: "Tidy the garden shed [fake:long:2]".into(), workspace: None, in_folder: false, from_branch: None }).unwrap() else { panic!("create") };
    // The thread's own words (the person's message is stored with the run's first part) and its title are found at once, and again once
    // the agent has spoken, with the run still going.
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let (hits, problem, _, _) = found(&mut c, "garden", None);
        assert_eq!(problem, None);
        if hits.iter().any(|h| h.task == task.id && h.by == FoundBy::Title) {
            break;
        }
        assert!(Instant::now() < end, "the thread's title was never found");
        std::thread::sleep(Duration::from_millis(50));
    }
    let (hits, problem, _, _) = found(&mut c, "garden status:sleepy", None);
    assert!(hits.is_empty() && problem.is_some_and(|p| p.contains("not a status")));
    let (hits, _, _, _) = found(&mut c, "garden status:working|idle|needs|failed|waiting", None);
    assert_eq!(hits.len(), 1);
    let (hits, _, _, _) = found(&mut c, "garden project:nowhere", None);
    assert!(hits.is_empty());
}
