//! Home's frosted glass (polish spec §2): the composer card and the floating "Show sidebar" button are a blur of the art under a
//! tint, with the words above at full strength; Reduce Transparency draws the opaque card. The structure is read through the real
//! layout; the pixels are read from headless captures of the real binary (the GPU draws the art and the blur there).

use std::path::{Path, PathBuf};
use std::process::Command;

use neko_rune::app::App;
use neko_rune::scenes::SCENES;
use neko_rune::theme::{contrast, theme};
use neko_rune::view::view;
use rune::{Color, Env, Runtime};

const BIN: &str = env!("CARGO_BIN_EXE_neko-rune");
const SIZE: (f32, f32) = (1360.0, 840.0);

fn runtime(scene: &str, dark: bool) -> Runtime<App> {
    let (_, f) = SCENES.iter().find(|(n, _)| *n == scene).unwrap_or_else(|| panic!("no scene {scene}"));
    let mut app = App::headless();
    f(&mut app);
    app.outbox.clear();
    let mut rt = Runtime::new(app, view, SIZE, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn elements(rt: &Runtime<App>) -> Vec<rune::ElementInfo> {
    rt.elements().into_iter().filter(|e| !e.exiting).collect()
}

fn index(els: &[rune::ElementInfo], key: &str) -> Option<usize> {
    els.iter().position(|e| e.key.as_deref() == Some(key))
}

#[test]
fn home_draws_the_card_on_glass_with_its_words_above_it() {
    for dark in [true, false] {
        let rt = runtime("home", dark);
        let els = elements(&rt);
        let glass = index(&els, "composer-glass").unwrap_or_else(|| panic!("no composer-glass, dark={dark}"));
        assert!(index(&els, "composer-opaque").is_none());
        let g = &els[glass];
        assert_eq!(g.radius, 16.0, "radius 16");
        assert_eq!(g.fill.a, if dark { 0.55 } else { 0.60 }, "the tint's strength, dark={dark}");
        let card = els.iter().find(|e| e.key.as_deref() == Some("home-composer")).expect("the card");
        assert_eq!(g.rect, card.rect, "the glass is the card's own rect");
        assert_eq!(card.fill.a, 0.0, "the card itself has no fill: the glass is its backing");
        assert!(index(&els, "composer-glass-highlight").is_some(), "the 1 px inner top highlight");
        // Painted in tree order: the text area, the Send button and the words under the card's bar are all after the glass.
        for key in ["new-request", "new-start", "new-mode-plan"] {
            let at = index(&els, key).unwrap_or_else(|| panic!("no {key}"));
            assert!(at > glass, "{key} is drawn above the glass (dark={dark})");
        }
        // The greeting and the status line stay crisp: a shadow copy under each, the real words after it.
        let title = index(&els, "home-title").expect("the greeting");
        assert!(els[title].text.as_deref().is_some_and(|t| t.starts_with("Good ")));
    }
}

#[test]
fn the_floating_show_sidebar_button_is_glass_too_on_home() {
    let rt = runtime("home-sessions-collapsed", true);
    let els = elements(&rt);
    let at = index(&els, "sidebar-show-glass").expect("glass behind the button");
    assert_eq!(els[at].radius, 8.0);
    let button = els.iter().find(|e| e.key.as_deref() == Some("sidebar-show")).expect("the button");
    assert!((button.rect[0] - 80.0).abs() <= 1.0);
    assert!(index(&els, "sidebar-show").is_some_and(|b| b < at), "the glass is inside the button, above its own fill, below its glyph");
    // Elsewhere the button is the quiet one.
    let rt = runtime("sessions-collapsed", true);
    assert!(index(&elements(&rt), "sidebar-show-glass").is_none());
}

#[test]
fn reduce_transparency_draws_the_opaque_card() {
    for dark in [true, false] {
        let rt = runtime("home-opaque", dark);
        let els = elements(&rt);
        assert!(index(&els, "composer-glass").is_none() && index(&els, "composer-opaque").is_some(), "dark={dark}");
        let o = els.iter().find(|e| e.key.as_deref() == Some("composer-opaque")).expect("the fallback");
        let t = theme(dark);
        assert_eq!(o.fill.a, 1.0, "opaque");
        let want = t.surface.window.mix(t.text.base, 0.04);
        assert!((o.fill.r - want.r).abs() < 0.002 && (o.fill.g - want.g).abs() < 0.002 && (o.fill.b - want.b).abs() < 0.002, "the window's colour with the foreground at 4%");
        assert_eq!(o.radius, 16.0);
    }
}

// ---- pixels -------------------------------------------------------------------------------------------------------------

struct Image {
    w: usize,
    rgba: Vec<u8>,
}

impl Image {
    fn read(path: &Path) -> Image {
        let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        buf.truncate(info.buffer_size());
        Image { w: info.width as usize, rgba: buf }
    }
    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.w + x) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
    /// The mean absolute difference between horizontal neighbours over a patch (device pixels): how much fine detail it holds.
    fn detail(&self, r: [usize; 4]) -> f64 {
        let mut sum = 0.0;
        let mut n = 0.0;
        for y in r[1]..r[1] + r[3] {
            for x in r[0]..r[0] + r[2] - 1 {
                let (a, b) = (self.at(x, y), self.at(x + 1, y));
                sum += a.iter().zip(b).map(|(a, b)| (f64::from(*a) - f64::from(b)).abs()).sum::<f64>();
                n += 3.0;
            }
        }
        sum / n
    }
    fn mean_abs_difference(&self, other: &Image, r: [usize; 4]) -> f64 {
        let mut sum = 0.0;
        let mut n = 0.0;
        for y in r[1]..r[1] + r[3] {
            for x in r[0]..r[0] + r[2] {
                sum += self.at(x, y).iter().zip(other.at(x, y)).map(|(a, b)| (f64::from(*a) - f64::from(b)).abs()).sum::<f64>();
                n += 3.0;
            }
        }
        sum / n
    }
}

fn snap(scene: &str, theme: &str, name: &str, env: &[(&str, &str)]) -> Image {
    let dir = std::env::temp_dir().join(format!("neko-rune-glass-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join(name);
    let mut cmd = Command::new(BIN);
    cmd.args(["--snap", path.to_str().unwrap(), "--scene", scene, "--theme", theme, "--size", "1360x840", "--scale", "2"]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("the binary runs");
    assert!(path.exists(), "no capture: {}", String::from_utf8_lossy(&out.stderr));
    Image::read(&path)
}

fn device_rect(scene: &str, key: &str, dark: bool) -> [usize; 4] {
    let rt = runtime(scene, dark);
    let r = elements(&rt).into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect;
    [(r[0] * 2.0) as usize, (r[1] * 2.0) as usize, (r[2] * 2.0) as usize, (r[3] * 2.0) as usize]
}

fn color(p: [u8; 3]) -> Color {
    let d = |v: u8| rune::color::srgb_decode(f32::from(v) / 255.0);
    Color { r: d(p[0]), g: d(p[1]), b: d(p[2]), a: 1.0 }
}

fn luma(p: [u8; 3]) -> f32 {
    color(p).luminance()
}

/// The empty right half of the card's text area: no words there, only what is under the card.
fn empty_patch(card: [usize; 4]) -> [usize; 4] {
    [card[0] + card[2] / 2, card[1] + 40, card[2] / 2 - 80, 40]
}

#[test]
fn the_pixels_inside_the_card_are_the_art_blurred_not_the_art() {
    for theme_name in ["dark", "light"] {
        let dark = theme_name == "dark";
        let glass = snap("home", theme_name, &format!("glass-{theme_name}.png"), &[]);
        let raw = snap("home-art-probe", theme_name, &format!("raw-{theme_name}.png"), &[]);
        let card = device_rect("home", "home-composer", dark);
        let patch = empty_patch(card);
        let changed = glass.mean_abs_difference(&raw, patch);
        let (g, r) = (glass.detail(patch), raw.detail(patch));
        eprintln!("{theme_name}: card {card:?} changed {changed:.2}, fine detail glass {g:.3} vs raw art {r:.3}");
        assert!(changed > 1.0, "{theme_name}: the card's pixels differ from the raw art under it: {changed:.2}");
        assert!(r > 0.2, "{theme_name}: the raw art has fine detail where the card sits (else this proves nothing): {r:.3}");
        assert!(g < r * 0.5, "{theme_name}: the art under the card is blurred (detail {g:.3} against {r:.3})");
        // Outside the card the art is untouched by the glass: the same pixels with and without it.
        let outside = [card[0] - 60, card[1] + card[3] + 60, 40, 40];
        assert!(glass.mean_abs_difference(&raw, outside) < 0.5, "{theme_name}: the glass stays inside the card");
    }
}

#[test]
fn the_words_in_the_card_keep_contrast_over_the_glass_in_both_themes() {
    for theme_name in ["dark", "light"] {
        let dark = theme_name == "dark";
        let img = snap("home-typed", theme_name, &format!("typed-{theme_name}.png"), &[]);
        let text = device_rect("home-typed", "new-request", dark);
        // The ground is the text area's most common luminance; the ink is the pixel furthest from it.
        let mut lumas: Vec<(f32, [u8; 3])> = Vec::new();
        for y in text[1]..text[1] + text[3] {
            for x in text[0]..text[0] + text[2] {
                let p = img.at(x, y);
                lumas.push((luma(p), p));
            }
        }
        lumas.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let ground = lumas[lumas.len() / 2];
        let ink = if dark { lumas[lumas.len() - 1] } else { lumas[0] };
        let c = contrast(color(ink.1), color(ground.1));
        eprintln!("{theme_name}: ink {:?} on {:?}: {c:.2}:1", ink.1, ground.1);
        assert!(c >= 4.5, "{theme_name}: the typed words over the glass: {c:.2}:1");
    }
}

#[test]
fn the_opaque_fallback_is_flat_and_the_expected_colour() {
    let img = snap("home-opaque", "dark", "opaque.png", &[]);
    let card = device_rect("home-opaque", "home-composer", true);
    let patch = empty_patch(card);
    assert!(img.detail(patch) < 0.05, "no art shows through the opaque card: {}", img.detail(patch));
    let t = theme(true);
    let want = t.surface.window.mix(t.text.base, 0.04);
    let p = img.at(patch[0] + 10, patch[1] + 10);
    let enc = |v: f32| (rune::color::srgb_encode(v) * 255.0).round() as i32;
    assert!((i32::from(p[0]) - enc(want.r)).abs() <= 2 && (i32::from(p[2]) - enc(want.b)).abs() <= 2, "{p:?} against {:?}", [enc(want.r), enc(want.g), enc(want.b)]);
    // The system setting does the same as the scene (`NEKO_RUNE_REDUCE_TRANSPARENCY=1` stands for it).
    let by_setting = snap("home", "dark", "opaque-setting.png", &[("NEKO_RUNE_REDUCE_TRANSPARENCY", "1")]);
    assert!(by_setting.detail(patch) < 0.05, "Reduce Transparency draws the card flat");
}
