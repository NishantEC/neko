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
fn the_shell_snaps_in_light_and_dark_with_the_neko_surfaces() {
    // Dark: the rail and titlebar are the sidebar surface #121212, the page is the window
    // surface #0f0f0f. Light: #f3f3f3 and #fafafa.
    for (theme, rail, page) in [("dark", [0x12, 0x12, 0x12], [0x0f, 0x0f, 0x0f]), ("light", [0xf3, 0xf3, 0xf3], [0xfa, 0xfa, 0xfa])] {
        let image = Image::read(&snap("live", theme));
        assert_eq!((image.width, image.height), (SIZE.0 * SCALE, SIZE.1 * SCALE), "{theme}");
        assert!(near(image.at(4.0, 400.0), rail), "{theme}: the rail's edge is {rail:?}, got {:?}", image.at(4.0, 400.0));
        assert!(near(image.at(900.0, 500.0), page), "{theme}: the page is {page:?}, got {:?}", image.at(900.0, 500.0));
        // The primary button is Iris, #346bf1, in both appearances.
        let button = image.at(212.0, 174.0);
        assert!(near(button, [0x34, 0x6b, 0xf1]), "{theme}: the primary button is Iris, got {button:?}");
    }
}

#[test]
fn the_tree_has_the_shell_and_no_layout_warnings() {
    for theme in ["dark", "light"] {
        let t = tree("live", theme);
        assert_eq!(t["title"], "Neko (Rune)");
        assert_eq!(t["warnings"].as_array().unwrap().len(), 0, "{theme}: {}", t["warnings"]);
        for key in ["rail-home", "rail-inbox", "rail-tasks", "rail-calendar", "rail-workspaces", "rail-settings", "rail-more", "back", "forward", "new-tab", "tabs", "open-web", "card"] {
            assert!(element(&t, key).is_some(), "{theme}: {key}");
        }
        let words = texts(&t);
        assert!(words.contains(&"Coming in P1: open in the web UI".to_owned()), "{words:?}");
        assert!(words.contains(&"Connected · 25 tasks · 2 need you".to_owned()), "{words:?}");
        // Everything that does something has a name for assistive technology.
        for key in ["rail-home", "rail-tasks", "rail-more", "open-web", "back", "forward", "new-tab"] {
            let label = element(&t, key).unwrap()["label"].as_str().unwrap_or("");
            assert!(!label.is_empty(), "{theme}: {key} has no label");
        }
    }
}

#[test]
fn the_rail_places_stay_inside_the_rail_and_the_page_clear_of_it() {
    let t = tree("live", "dark");
    let rect = |key: &str| -> Vec<f64> { element(&t, key).unwrap()["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect() };
    let (home, settings, card) = (rect("rail-home"), rect("rail-settings"), rect("card"));
    assert_eq!(home[2], 36.0);
    assert!(home[0] + home[2] <= 56.0, "the rail is 56 points wide");
    assert!(settings[1] > home[1], "Settings sits at the foot");
    assert!(card[0] >= 56.0, "the page starts after the rail");
}

#[test]
fn offline_and_palette_scenes_say_what_they_should() {
    let offline = tree("offline", "dark");
    assert!(texts(&offline).contains(&"Neko isn't running. Trying again in 4 s.".to_owned()));
    let palette = tree("palette", "dark");
    assert!(element(&palette, "palette-input").is_some());
    assert!(texts(&palette).contains(&"Pull requests".to_owned()));
    let image = Image::read(&snap("palette", "dark"));
    // A scrim dims the page: the colour at the page's empty corner is darker than the page.
    assert!(image.at(900.0, 700.0)[0] < 0x0f, "the scrim darkens the page");
}

#[test]
fn every_scene_renders_in_both_appearances() {
    let list = Command::new(BIN).arg("--list-scenes").output().unwrap();
    let scenes: Vec<String> = String::from_utf8(list.stdout).unwrap().lines().map(str::to_owned).collect();
    assert!(scenes.len() >= 8, "{scenes:?}");
    for scene in &scenes {
        for theme in ["light", "dark"] {
            let image = Image::read(&snap(scene, theme));
            assert_eq!(image.width, SIZE.0 * SCALE, "{scene} {theme}");
        }
    }
}
