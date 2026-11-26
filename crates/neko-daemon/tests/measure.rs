//! Measurements, not checks: ignored by default (they take minutes). Run with
//! `cargo test -p neko-daemon --test measure -- --ignored --nocapture`. The fake CLI streams; the real one never runs.

mod support;
use std::time::{Duration, Instant};

use neko_protocol::{Change, Reply, Request, ServerMessage, TaskStatus};
use support::*;

/// CPU time (user + system, ms), bytes written to disk, and resident memory (KB) of a process.
fn usage(pid: i32) -> (u64, u64, u64) {
    // SAFETY: a zeroed rusage_info_v4 filled by the kernel for a process this test started.
    let mut ri: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, (&raw mut ri).cast()) };
    assert_eq!(r, 0, "proc_pid_rusage");
    // Mach absolute time units: nanoseconds on Apple silicon only after the timebase; read it.
    let mut tb = libc::mach_timebase_info { numer: 0, denom: 0 };
    unsafe { libc::mach_timebase_info(&mut tb) };
    let ns = (ri.ri_user_time + ri.ri_system_time) as u128 * tb.numer as u128 / tb.denom as u128;
    ((ns / 1_000_000) as u64, ri.ri_diskio_byteswritten, ri.ri_resident_size / 1024)
}

#[test]
#[ignore = "a measurement: about six minutes"]
fn idle_and_a_five_minute_run() {
    let d = Daemon::start("measure");
    let pid = d.child.id() as i32;
    let mut c = d.client();
    c.request(Request::Subscribe).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let (cpu0, _, rss0) = usage(pid);
    std::thread::sleep(Duration::from_secs(60));
    let (cpu1, _, rss1) = usage(pid);
    println!("idle, 60 s, one screen subscribed: {} ms CPU ({:.3}%), resident {} KB -> {} KB", cpu1 - cpu0, (cpu1 - cpu0) as f64 / 600.0, rss0, rss1);

    let (cpu_a, w_a, _) = usage(pid);
    let Reply::Task { task } = c.request(Request::CreateTask { words: "Write a lot [fake:long:300]".into(), workspace: None, cat: None }).unwrap() else { panic!("create") };
    c.request(Request::WatchTask { task: Some(task.id.clone()) }).unwrap();
    let t0 = Instant::now();
    let mut deltas = 0u64;
    loop {
        assert!(t0.elapsed() < Duration::from_secs(400), "the run did not end");
        match c.next_event(Duration::from_millis(500)).unwrap() {
            Some(ServerMessage::Event { change: Change::PartDelta { .. }, .. }) => deltas += 1,
            Some(ServerMessage::Event { change: Change::TaskUpdated { task: t }, .. }) if t.id == task.id && t.status == TaskStatus::Done => break,
            _ => {}
        }
    }
    let (cpu_b, w_b, rss) = usage(pid);
    let secs = t0.elapsed().as_secs_f64();
    let store: u64 = std::fs::read_dir(&d.dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("neko.sqlite3")).map(|e| e.metadata().unwrap().len()).sum();
    println!(
        "a {secs:.0} s fake run, {deltas} streamed pieces: daemon {} ms CPU ({:.2}%), {} KB written to disk ({:.1} KB/s), store files now {} KB, resident {} KB",
        cpu_b - cpu_a,
        (cpu_b - cpu_a) as f64 / (secs * 10.0),
        (w_b - w_a) / 1024,
        (w_b - w_a) as f64 / 1024.0 / secs,
        store / 1024,
        rss
    );
}
