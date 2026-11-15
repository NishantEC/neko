//! Frame and memory measurement for the bench runs: fed by `App::on_frame_encoded` and `poll`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Default)]
pub struct Phase {
    pub intervals_ms: Vec<f64>,
    pub update_ms: Vec<f64>,
    pub encode_ms: Vec<f64>,
    pub drawn: u32,
    pub unchanged: u32,
    pub skipped: u32,
    last_started: Option<Instant>,
}

#[derive(Default)]
pub struct Stats {
    pub phase: &'static str,
    pub phases: BTreeMap<&'static str, Phase>,
    /// (label, footprint bytes, resident bytes)
    pub memory: Vec<(String, u64, u64)>,
}

thread_local! {
    pub static STATS: RefCell<Stats> = RefCell::new(Stats::default());
}

pub fn set_phase(name: &'static str) {
    STATS.with(|s| {
        let mut s = s.borrow_mut();
        if s.phase == name {
            return;
        }
        s.phase = name;
        // A gap between phases is not a frame interval.
        if let Some(p) = s.phases.get_mut(name) {
            p.last_started = None;
        }
    });
}

pub fn frame(f: &rune::FrameEncoded) {
    STATS.with(|s| {
        let mut s = s.borrow_mut();
        let name = s.phase;
        if name.is_empty() {
            return;
        }
        let p = s.phases.entry(name).or_default();
        match f.outcome() {
            rune::FrameOutcome::Drawn => {
                p.drawn += 1;
                if let Some(u) = f.update_ms() {
                    p.update_ms.push(u);
                }
                if let Some(e) = f.encode_ms() {
                    p.encode_ms.push(e);
                }
                // `at()`: when encoding finished. `started()` is only set for display-paced frames.
                let st = f.at();
                if let Some(prev) = p.last_started {
                    p.intervals_ms.push(st.duration_since(prev).as_secs_f64() * 1000.0);
                }
                p.last_started = Some(st);
            }
            rune::FrameOutcome::Unchanged => p.unchanged += 1,
            _ => p.skipped += 1,
        }
    });
}

pub fn memory(label: &str) {
    let u = procwatch::usage();
    STATS.with(|s| s.borrow_mut().memory.push((label.to_owned(), u.footprint.unwrap_or(0), u.resident.unwrap_or(0))));
}

fn pct(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f64 * p).round() as usize]
}

fn summary(v: &[f64]) -> String {
    let mean = if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    format!(
        "{{\"n\":{},\"mean\":{:.3},\"p50\":{:.3},\"p95\":{:.3},\"p99\":{:.3},\"max\":{:.3}}}",
        v.len(),
        mean,
        pct(v, 0.5),
        pct(v, 0.95),
        pct(v, 0.99),
        pct(v, 1.0)
    )
}

pub fn json(label: &str) -> String {
    STATS.with(|s| {
        let s = s.borrow();
        let mut out = format!("{{\"run\":\"{label}\",\"phases\":{{");
        for (i, (name, p)) in s.phases.iter().enumerate() {
            let over_8 = p.intervals_ms.iter().filter(|x| **x > 8.4 * 1.5).count();
            let over_16 = p.intervals_ms.iter().filter(|x| **x > 16.7 * 1.5).count();
            out.push_str(&format!(
                "{}\"{}\":{{\"drawn\":{},\"unchanged\":{},\"skipped\":{},\"interval_ms\":{},\"update_ms\":{},\"encode_ms\":{},\"intervals_over_1.5x_120hz\":{},\"intervals_over_1.5x_60hz\":{}}}",
                if i > 0 { "," } else { "" },
                name,
                p.drawn,
                p.unchanged,
                p.skipped,
                summary(&p.intervals_ms),
                summary(&p.update_ms),
                summary(&p.encode_ms),
                over_8,
                over_16
            ));
        }
        out.push_str("},\"memory_mb\":[");
        for (i, (l, f, r)) in s.memory.iter().enumerate() {
            out.push_str(&format!("{}{{\"at\":\"{}\",\"footprint\":{:.1},\"resident\":{:.1}}}", if i > 0 { "," } else { "" }, l, *f as f64 / 1048576.0, *r as f64 / 1048576.0));
        }
        out.push_str("]}");
        out
    })
}
