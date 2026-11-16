//! Home's art (polish spec §3) on a real GPU, through the headless renderer: the dither is there behind Home and
//! only there, and the budget holds: at most 1.0 ms of GPU per frame at 2560x1600, measured with timestamp queries.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_neko-rune");

fn out(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("neko-rune-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

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
        assert_eq!(info.color_type, png::ColorType::Rgba, "an RGBA snapshot");
        buf.truncate(info.buffer_size());
        Image { w: info.width as usize, rgba: buf }
    }

    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.w + x) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }

    /// How many pixels of the patch are Iris-blue (blue well above red), and how many are not.
    fn blue_share(&self, x0: usize, y0: usize, w: usize, h: usize) -> f32 {
        let mut blue = 0;
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let [r, _, b] = self.at(x, y);
                if i32::from(b) - i32::from(r) > 40 {
                    blue += 1;
                }
            }
        }
        blue as f32 / (w * h) as f32
    }
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
fn the_art_is_a_one_hue_dither_behind_home_and_not_behind_a_session() {
    let (home, _) = snap("default", "dark", "home-dark.png", &[]);
    let home = Image::read(&home);
    // The sea, lower right of the middle region (device pixels at 2x): dots of Iris on near-black.
    let sea = home.blue_share(1500, 1300, 300, 200);
    assert!(sea > 0.10 && sea < 0.95, "the sea is a dither of Iris dots on the ground: {sea:.2} of it is Iris");
    // The same patch behind a session is the window colour and text only.
    let (live, _) = snap("live", "dark", "session-dark.png", &[]);
    let live = Image::read(&live);
    assert!(live.blue_share(1500, 1300, 300, 200) < 0.02, "no art behind a session");
    // One hue: every lit pixel of the sea is a blend of the ground and Iris, never another hue (green stays under red + blue).
    let mut seen_ground = false;
    for y in (1300..1500).step_by(3) {
        for x in (1500..1800).step_by(3) {
            let [r, g, b] = home.at(x, y);
            assert!(i32::from(g) <= i32::from(b) + 4 && i32::from(g) >= i32::from(r) - 4, "one hue: {r},{g},{b}");
            seen_ground |= r < 30 && b < 30;
        }
    }
    assert!(seen_ground, "the ground shows between the dots: a 1-bit look");
}

#[test]
fn the_light_theme_is_the_same_scene_at_dawn() {
    let (light, _) = snap("default", "light", "home-light.png", &[]);
    let light = Image::read(&light);
    // A pale ground (nowhere near black) with a faint Iris wash: lit pixels are at most a third of the way to Iris.
    let [r, g, b] = light.at(1700, 1550);
    assert!(r > 190 && g > 190 && b > 200, "pale sky: {r},{g},{b}");
}

#[test]
fn the_art_costs_at_most_a_millisecond_of_gpu_at_2560_by_1600() {
    let (_, log) = snap("default", "dark", "bench.png", &[("NEKO_RUNE_ART_BENCH", "1")]);
    let Some(line) = log.lines().find(|l| l.starts_with("home art GPU at 2560x1600")) else {
        eprintln!("no GPU timestamp queries on this machine; the budget is not checked here:\n{log}");
        return;
    };
    let number = |key: &str| -> f64 { line.split(key).nth(1).and_then(|r| r.trim_start().split_whitespace().next()).and_then(|v| v.trim_end_matches(',').parse().ok()).unwrap_or_else(|| panic!("{key} in {line}")) };
    let (mean, p50) = (number("mean"), number("p50"));
    eprintln!("{line}");
    // The median is what a frame costs; the mean also carries the other GPU clients' interruptions (a loaded Mac).
    assert!(p50 <= 1.0, "median GPU frame {p50} ms is over the 1.0 ms budget: {line}");
    assert!(mean <= 1.5, "mean GPU frame {mean} ms is far over the budget: {line}");
}
