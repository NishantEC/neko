//! Rune's own headless runs on the real binary: `--snap` renders a PNG with the window's renderer
//! and `--tree` dumps the element tree, both without a window, a display or focus. Each scene is
//! rendered in light and dark; the test checks the picture (its size and the colours at known
//! places) and the tree (the right elements, no layout warnings). The PNGs are also kept under
//! `$NEKO_RUNE_SNAPS` when that is set, for looking at.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_neko-rune");
const SIZE: (u32, u32) = (1280, 800);
const SCALE: u32 = 2;

fn out_dir() -> PathBuf {
    let dir = std::env::var_os("NEKO_RUNE_SNAPS").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join(format!("neko-rune-snaps-{}", std::process::id())));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) {
    let out = Command::new(BIN).args(args).env_remove("NEKO_DATA_DIR").output().expect("the binary starts");
    assert!(out.status.success(), "{args:?}\n{}", String::from_utf8_lossy(&out.stderr));
}

/// Tests run in parallel, so every render gets a file of its own.
fn unique() -> usize {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn snap(scene: &str, theme: &str) -> PathBuf {
    let path = out_dir().join(format!("{scene}-{theme}-{}.png", unique()));
    run(&["--snap", path.to_str().unwrap(), "--scene", scene, "--theme", theme, "--size", &format!("{}x{}", SIZE.0, SIZE.1), "--scale", &SCALE.to_string()]);
    path
}

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Image {
    fn read(path: &Path) -> Image {
        let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        let rgba = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            other => panic!("unexpected PNG colour type {other:?}"),
        };
        Image { width: info.width, height: info.height, rgba }
    }

    /// The colour at a point given in the window's points.
    fn at(&self, x: f32, y: f32) -> [u8; 3] {
        let (px, py) = ((x * SCALE as f32) as u32, (y * SCALE as f32) as u32);
        let i = ((py * self.width + px) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
}

fn near(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 2)
}

fn tree(scene: &str, theme: &str) -> serde_json::Value {
    let path = out_dir().join(format!("{scene}-{theme}-{}.tree.json", unique()));
    run(&["--tree", path.to_str().unwrap(), "--scene", scene, "--theme", theme, "--size", &format!("{}x{}", SIZE.0, SIZE.1)]);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn element<'a>(tree: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    tree["elements"].as_array().unwrap().iter().find(|e| e["key"] == key)
}

fn texts(tree: &serde_json::Value) -> Vec<String> {
    tree["elements"].as_array().unwrap().iter().filter_map(|e| e["text"].as_str().map(str::to_owned)).collect()
}

#[test]
fn the_shell_snaps_in_light_and_dark_with_the_neko_surfaces() {
    // The sessions and Neko columns are the sidebar surface, the window background plus 2% lightness
    // (#141414 dark, #ffffff light: polish spec §4); the session is the window surface (#0f0f0f,
    // #fafafa); the context panel is the card surface (#151515, #ffffff).
    let cases = [("dark", [0x14, 0x14, 0x14], [0x0f, 0x0f, 0x0f], [0x15, 0x15, 0x15]), ("light", [0xff, 0xff, 0xff], [0xfa, 0xfa, 0xfa], [0xff, 0xff, 0xff])];
    for (theme, column, session, card) in cases {
        let image = Image::read(&snap("live", theme));
        assert_eq!((image.width, image.height), (SIZE.0 * SCALE, SIZE.1 * SCALE), "{theme}");
        let t = tree("live", theme);
        let r = rect_of(&t, "session-region");
        assert!(near(image.at(3.0, 400.0), column), "{theme}: the sessions column edge is {column:?}, got {:?}", image.at(3.0, 400.0));
        assert!(near(image.at(r[0] + r[2] / 2.0, r[1] + 200.0), session), "{theme}: the session is {session:?}, got {:?}", image.at(r[0] + r[2] / 2.0, r[1] + 200.0));
        let n = rect_of(&t, "neko");
        assert!(near(image.at(n[0] + n[2] - 3.0, 400.0), column), "{theme}: the Neko column is {column:?}");
        let ctx = Image::read(&snap("context", theme));
        let c = rect_of(&tree("context", theme), "context");
        assert!(near(ctx.at(c[0] + c[2] / 2.0, c[1] + 400.0), card), "{theme}: the context panel is {card:?}, got {:?}", ctx.at(c[0] + c[2] / 2.0, c[1] + 400.0));
    }
}

fn rect_of(tree: &serde_json::Value, key: &str) -> Vec<f32> {
    element(tree, key).unwrap_or_else(|| panic!("no element keyed {key}"))["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect()
}

#[test]
fn the_tree_has_the_four_regions_and_no_layout_warnings() {
    for theme in ["dark", "light"] {
        let t = tree("live", theme);
        assert_eq!(t["title"], "Neko (Rune)");
        assert_eq!(t["warnings"].as_array().unwrap().len(), 0, "{theme}: {}", t["warnings"]);
        for key in ["sessions", "session-region", "neko", "group-0", "group-1", "group-2", "session-t0", "composer-input", "steer-trigger", "crew", "inbox", "ask", "grip-sessions", "grip-neko", "view-calendar", "view-inbox", "view-setup", "account", "sessions-search", "new-session"] {
            assert!(element(&t, key).is_some(), "{theme}: {key}");
        }
        assert!(element(&t, "context").is_none(), "the context panel starts closed");
        let words = texts(&t);
        assert!(words.contains(&"5 agents working · 6 need you".to_owned()), "{words:?}");
        assert!(words.contains(&"Connected · 22 tasks".to_owned()), "{words:?}");
        // Everything that does something has a name for assistive technology.
        for key in ["session-t0", "context-toggle", "neko-collapse", "view-inbox", "account", "sessions-search", "new-session", "steer-trigger", "grip-sessions"] {
            let label = element(&t, key).unwrap()["label"].as_str().unwrap_or("");
            assert!(!label.is_empty(), "{theme}: {key} has no label");
        }
        // No tabs, no rail.
        assert!(element(&t, "tabs").is_none() && element(&t, "rail").is_none());
    }
}

#[test]
fn the_glyph_lab_shows_all_seven_status_glyphs_in_their_colours() {
    let t = tree("lab", "dark");
    let words = texts(&t);
    for label in ["Backlog", "To do", "In progress", "In review", "Done", "Failed", "Cancelled"] {
        assert!(words.contains(&label.to_owned()), "{label}");
    }
    let image = Image::read(&snap("lab", "dark"));
    // The glyph is 14 points wide and sits 8 before its label. A point inside the disc, off the
    // mark in the middle, shows the glyph's own colour: Done is Mint, Failed is Coral.
    let disc = |label: &str| {
        let e = t["elements"].as_array().unwrap().iter().find(|e| e["text"] == label).unwrap();
        let r: Vec<f32> = e["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
        image.at(r[0] - 8.0 - 12.5, r[1] + r[3] / 2.0)
    };
    assert!(near(disc("Done"), [0x4c, 0xc3, 0x8a]), "Done is Mint, got {:?}", disc("Done"));
    assert!(near(disc("Failed"), [0xe5, 0x48, 0x4d]), "Failed is Coral, got {:?}", disc("Failed"));
}

#[test]
fn offline_and_notice_scenes_say_what_they_should() {
    assert!(texts(&tree("offline", "dark")).contains(&"Neko isn't running. Trying again in 4 s.".to_owned()));
    assert!(texts(&tree("notice", "dark")).contains(&"Neko isn't running, so there's no web UI to open yet.".to_owned()));
    // The context panel, the collapsed Neko, the open menus and a full view each render something.
    assert!(element(&tree("context", "dark"), "context-picker").is_some());
    assert!(element(&tree("collapsed", "dark"), "neko-expand").is_some());
    assert!(texts(&tree("steer", "dark")).contains(&"Interrupt and restart".to_owned()));
    let more = texts(&tree("more", "dark"));
    assert!(["Settings", "Usage", "Plugins", "Open the web UI", "Quit"].iter().all(|w| more.contains(&(*w).to_owned())), "{more:?}");
    let settings = texts(&tree("full-view", "dark"));
    assert!(settings.contains(&"AI & agents".to_owned()), "a full view is its own page now, not a placeholder");
}

#[test]
fn every_scene_renders_in_both_appearances() {
    let list = Command::new(BIN).arg("--list-scenes").output().unwrap();
    let scenes: Vec<String> = String::from_utf8(list.stdout).unwrap().lines().map(str::to_owned).collect();
    assert!(scenes.len() >= 10, "{scenes:?}");
    // Each render is its own process; four at a time keep the gate near a minute (nothing is shared).
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(scene) = scenes.get(i) else { break };
                    for theme in ["light", "dark"] {
                        let image = Image::read(&snap(scene, theme));
                        assert_eq!(image.width, SIZE.0 * SCALE, "{scene} {theme}");
                    }
                }
            });
        }
    });
}
