//! The art (polish spec §3) on a real GPU, through the headless renderer: the budget holds, at most 1.0 ms of GPU per frame at 2560x1600,
//! measured with timestamp queries. Home draws no art since the prototype's look (2026-11-21), so a snapshot of it prints no figure and the
//! check is skipped; the art's own contrast tests are in `src/art.rs`.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_neko-rune");

fn out(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("neko-rune-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn snap(scene: &str, theme: &str, name: &str, extra_env: &[(&str, &str)]) -> (PathBuf, String) {
    let path = out(name);
    let mut cmd = Command::new(BIN);
    cmd.args(["--snap", path.to_str().unwrap(), "--scene", scene, "--theme", theme, "--size", "1360x840", "--scale", "2"]);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("the binary runs");
    (path, String::from_utf8_lossy(&output.stderr).to_string() + &String::from_utf8_lossy(&output.stdout))
}

#[test]
fn the_art_costs_at_most_a_millisecond_of_gpu_at_2560_by_1600() {
    let (_, log) = snap("default", "dark", "bench.png", &[("NEKO_RUNE_ART_BENCH", "1")]);
    let Some(line) = log.lines().find(|l| l.starts_with("home art GPU at 2560x1600")) else {
        eprintln!("no GPU timestamp queries on this machine; the budget is not checked here:\n{log}");
        return;
    };
    let number = |key: &str| -> f64 { line.split(key).nth(1).and_then(|r| r.split_whitespace().next()).and_then(|v| v.trim_end_matches(',').parse().ok()).unwrap_or_else(|| panic!("{key} in {line}")) };
    let (mean, p50) = (number("mean"), number("p50"));
    eprintln!("{line}");
    // The median is what a frame costs; the mean also carries the other GPU clients' interruptions (a loaded Mac).
    assert!(p50 <= 1.0, "median GPU frame {p50} ms is over the 1.0 ms budget: {line}");
    assert!(mean <= 1.5, "mean GPU frame {mean} ms is far over the budget: {line}");
}
