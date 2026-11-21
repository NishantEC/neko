//! Polish spec §4, "everything else, made consistent": checked by reading the sources and by laying out
//! the Neko column, so a later screen cannot quietly go its own way.

use std::fs;
use std::path::{Path, PathBuf};

use neko_rune::app::App;
use neko_rune::scenes;
use neko_rune::view::view;
use rune::{Env, Runtime};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn all_sources() -> Vec<(PathBuf, String)> {
    let mut paths = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut paths);
    paths.into_iter().map(|p| (p.clone(), fs::read_to_string(p).unwrap())).collect()
}

/// The part of a source file before its test module.
fn production(text: &str) -> &str {
    text.split("#[cfg(test)]").next().unwrap_or(text)
}

#[test]
fn every_focus_ring_is_two_points_at_an_offset_of_two() {
    let mut seen = 0;
    for (path, text) in all_sources() {
        for call in production(&text).split(".focus_ring(").skip(1) {
            seen += 1;
            assert!(call.starts_with("2.0, 2.0, "), "{}: a focus ring that is not 2 points at offset 2: `.focus_ring({}`", path.display(), call.chars().take(40).collect::<String>());
        }
    }
    assert!(seen > 30, "found {seen} focus rings");
}

#[test]
fn only_the_hover_fill_the_panel_and_the_ring_animate() {
    // Anything that moves on its own is named here, in the files that own it: the hover curve (motion.rs, set once on
    // the root in view.rs), the context panel's spring (view.rs), the progress ring's turn (view/sessions.rs, view/look.rs) and
    // Home's art (art.rs, art_gpu.rs). A new `transition`, `looping`, `.motion(` or tween elsewhere fails here.
    // presence.rs (P4, the activity pill, `--overlays`): the glow's breathing and the working ring's turn, both stopped under Reduce Motion
    // (the owner's brief: "respect Reduce Motion (no pulse) as Swift does"); the pill is its own window and never in the main one.
    // toasts.rs only declares that a toast stands still (`Transition::none()`), as the one that came before it did in view.rs.
    let owners = ["motion.rs", "view.rs", "sessions.rs", "look.rs", "art.rs", "art_gpu.rs", "model3d_gpu.rs", "toasts.rs", "presence.rs"];
    for (path, text) in all_sources() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        for word in [".transition(", ".looping(", ".motion(", ".token_motion(", ".offset_motion(", ".opacity_motion(", ".scale_motion(", ".sequence(", ".fold_glide(", "Curve::tween", "Spring::new"] {
            assert!(owners.contains(&name.as_str()) || !production(&text).contains(word), "{}: `{word}` animates something nobody owns (polish spec §4: nothing else animates)", path.display());
        }
    }
}

#[test]
fn the_neko_column_uses_the_sidebars_row_metrics() {
    let (_, scene) = scenes::SCENES.iter().find(|(n, _)| *n == "live").unwrap();
    let mut app = App::headless_neko();
    scene(&mut app);
    let mut rt = Runtime::new(app, view, (1440.0, 900.0), Env { system_dark: true, scale: 2.0 });
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let rect = |key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect;
    for key in ["crew-row", "inbox-toggle", "digest-toggle"] {
        assert_eq!(rect(key)[3], 32.0, "{key} is a 32 point row");
    }
    // 12 of padding: the rows start 12 in from the column's left edge, and the ask box does too.
    let neko = rect("neko");
    assert_eq!(rect("inbox-toggle")[0], neko[0] + 12.0);
    assert_eq!(rect("ask-card")[0], neko[0] + 12.0);
    // The Inbox's items are single 32 point lines unless opened.
    let first = rt.elements().into_iter().find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("inbox-") && k[6..].chars().all(|c| c.is_ascii_digit()) && k.len() > 6)).expect("an inbox row");
    assert_eq!(first.rect[3], 32.0, "an Inbox item is one 32 point row");
    // The session view's header row is the sidebar header's: 32 high, centred on the same line.
    let top = rect("top-title");
    assert!((top[1] + top[3] / 2.0 - 24.0).abs() <= 2.0, "the session header's centre is on the bar's line (24): {top:?}");
}
