//! Rune's own headless runs on the real binary: `--snap` renders a PNG with the window's renderer
//! and `--tree` dumps the element tree, both without a window, a display or focus. Each scene is
//! rendered in light and dark; the test checks the picture (its size and the colours at known
//! places) and the tree (the right elements, no layout warnings). The PNGs are also kept under
//! `$NEKO_RUNE_SNAPS` when that is set, for looking at.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_neko-rune");
const SIZE: (u32, u32) = (1180, 760);
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

fn snap(scene: &str, theme: &str) -> PathBuf {
    let path = out_dir().join(format!("{scene}-{theme}.png"));
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
    let path = out_dir().join(format!("{scene}-{theme}.tree.json"));
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
fn the_view_snaps_in_light_and_dark_with_the_neko_surfaces() {
    // Dark: the window is #0f0f0f. Light: #fafafa. The primary button is Iris in both.
    for (theme, window) in [("dark", [0x0f, 0x0f, 0x0f]), ("light", [0xfa, 0xfa, 0xfa])] {
        let image = Image::read(&snap("live", theme));
        assert_eq!((image.width, image.height), (SIZE.0 * SCALE, SIZE.1 * SCALE), "{theme}");
        assert!(near(image.at(4.0, 400.0), window), "{theme}: the window is {window:?}, got {:?}", image.at(4.0, 400.0));
        assert!(near(image.at(1100.0, 600.0), window), "{theme}: empty space is the window's colour");
        let button = button_pixel(&tree("live", theme), &image);
        assert!(near(button, [0x34, 0x6b, 0xf1]), "{theme}: the primary button is Iris, got {button:?}");
    }
}

fn rect_of(tree: &serde_json::Value, key: &str) -> Vec<f32> {
    element(tree, key).unwrap_or_else(|| panic!("no element keyed {key}"))["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect()
}

/// A pixel just inside the primary button's left edge, found from the tree.
fn button_pixel(tree: &serde_json::Value, image: &Image) -> [u8; 3] {
    let r = rect_of(tree, "open-web");
    image.at(r[0] + 4.0, r[1] + r[3] / 2.0)
}

#[test]
fn the_tree_has_the_rows_and_no_layout_warnings() {
    for theme in ["dark", "light"] {
        let t = tree("live", theme);
        assert_eq!(t["title"], "Neko (Rune)");
        assert_eq!(t["warnings"].as_array().unwrap().len(), 0, "{theme}: {}", t["warnings"]);
        for key in ["window", "page", "title", "rows", "open-web", "status-text", "task-t0", "status-InProgress"] {
            assert!(element(&t, key).is_some(), "{theme}: {key}");
        }
        let words = texts(&t);
        assert!(words.contains(&"Connected · 25 tasks · 2 need you".to_owned()), "{words:?}");
        assert!(words.contains(&"Tasks".to_owned()));
        // The button has a name for assistive technology.
        assert_eq!(element(&t, "open-web").unwrap()["label"], "Open in the web UI");
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
}

#[test]
fn every_scene_renders_in_both_appearances() {
    let list = Command::new(BIN).arg("--list-scenes").output().unwrap();
    let scenes: Vec<String> = String::from_utf8(list.stdout).unwrap().lines().map(str::to_owned).collect();
    assert!(scenes.len() >= 6, "{scenes:?}");
    for scene in &scenes {
        for theme in ["light", "dark"] {
            let image = Image::read(&snap(scene, theme));
            assert_eq!(image.width, SIZE.0 * SCALE, "{scene} {theme}");
        }
    }
}
