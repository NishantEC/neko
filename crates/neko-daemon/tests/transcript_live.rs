//! A screen that opens a task while its words stream (it watches only from then) gets every word so far in the transcript, though
//! the store writes them only every few seconds; with the streamed words after it, nothing is missing and nothing doubled.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::*;
use support::*;

#[test]
fn a_transcript_asked_mid_stream_holds_every_word_so_far_and_the_stream_continues_it() {
    let d = Daemon::start("transcript-live");
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    let task = match c.request(Request::CreateTask { words: "Count slowly [fake:slow]".into(), workspace: None, cat: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    };
    // Not watching yet: the first words stream to nobody, and the store has not written them.
    std::thread::sleep(Duration::from_millis(900));
    assert!(matches!(c.request(Request::WatchTask { task: Some(task.id.clone()) }).unwrap(), Reply::Ok));
    let t = match c.request(Request::GetTranscript { task: task.id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    };
    let mut words: String = t.parts.iter().filter_map(|p| if let PartBody::Agent { text, .. } = &p.body { Some(text.as_str()) } else { None }).collect();
    assert!(words.starts_with("slow0"), "the first words are in the transcript at once: {words:?}");
    // The stream carries on from there.
    let end = Instant::now() + Duration::from_secs(10);
    while !words.contains("slow8") {
        assert!(Instant::now() < end, "the stream did not continue: {words:?}");
        if let Some(ServerMessage::Event { change: Change::PartDelta { text, .. }, .. }) = c.next_event(Duration::from_millis(100)).unwrap() {
            words.push_str(&text);
        }
    }
    let n: Vec<&str> = words.split_whitespace().collect();
    for (i, w) in n.iter().enumerate() {
        assert_eq!(*w, format!("slow{i}"), "no word missing or doubled: {words:?}");
    }
}
