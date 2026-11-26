//! Pictures and video of a headless runtime: each frame rendered offscreen on the GPU, PNGs for steps, raw frames streamed into
//! ffmpeg for the run's video.

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Instant;

use neko_app::state::App;
use rune::Runtime;
use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer, write_png};

pub struct Recorder {
    gpu: Gpu,
    renderer: Renderer,
    target: Offscreen,
    pub width: u32,
    pub height: u32,
    video: Option<(Child, ChildStdin)>,
    fps: f64,
    began: Instant,
    written: u64,
    pub video_error: Option<String>,
}

impl Recorder {
    /// A recorder for a window of `size` points at `scale`; with `video`, ffmpeg writes an `.mp4` there at `fps`, scaled to the
    /// window's size in points.
    pub fn new(size: (f32, f32), scale: f32, video: Option<&Path>, fps: f64, log: &Path) -> Result<(Recorder, Option<u32>), String> {
        let gpu = Gpu::headless();
        let (width, height) = ((size.0 * scale).round() as u32, (size.1 * scale).round() as u32);
        let renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let target = Offscreen::new(&gpu.device, width, height);
        let mut pid = None;
        let video = match video {
            None => None,
            Some(path) => {
                let err = std::fs::OpenOptions::new().create(true).append(true).open(log).map_err(|e| e.to_string())?;
                let mut child = Command::new("ffmpeg")
                    .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
                    .arg(format!("{width}x{height}"))
                    .arg("-framerate")
                    .arg(format!("{fps}"))
                    .args(["-i", "-", "-vf"])
                    .arg(format!("scale={}:-2", size.0.round() as u32))
                    .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "24", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
                    .arg(path)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(err)
                    .spawn()
                    .map_err(|e| format!("ffmpeg could not start: {e}"))?;
                pid = Some(child.id());
                let stdin = child.stdin.take().ok_or("ffmpeg has no stdin")?;
                Some((child, stdin))
            }
        };
        Ok((Recorder { gpu, renderer, target, width, height, video, fps, began: Instant::now(), written: 0, video_error: None }, pid))
    }

    /// The runtime's last frame as RGBA pixels.
    pub fn pixels(&mut self, rt: &mut Runtime<App>) -> Vec<u8> {
        self.renderer.render_runtime_to_offscreen(&self.gpu.device, &self.gpu.queue, &self.target, rt);
        self.target.read(&self.gpu.device)
    }

    pub fn png(&mut self, rt: &mut Runtime<App>, path: &Path) -> Result<(), String> {
        let px = self.pixels(rt);
        write_png(path, self.width, self.height, &px).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Writes the frames the wall clock owes the video (the current picture, repeated when the run fell behind; at most two
    /// seconds' worth, so a long pause shortens in the video).
    pub fn tick(&mut self, rt: &mut Runtime<App>) {
        if self.video.is_none() {
            return;
        }
        let due = (self.began.elapsed().as_secs_f64() * self.fps) as u64;
        if due < self.written + 1 {
            return;
        }
        let owed = (due - self.written).min((self.fps * 2.0) as u64).max(1);
        let px = self.pixels(rt);
        if let Some((_, stdin)) = &mut self.video {
            for _ in 0..owed {
                if let Err(e) = stdin.write_all(&px) {
                    self.video_error = Some(format!("ffmpeg stopped taking frames: {e}"));
                    self.video = None;
                    return;
                }
            }
        }
        self.written = due;
    }

    /// Closes the video and waits for ffmpeg.
    pub fn finish(&mut self) -> Result<(), String> {
        if let Some((mut child, stdin)) = self.video.take() {
            drop(stdin);
            let st = child.wait().map_err(|e| e.to_string())?;
            if !st.success() {
                return Err(format!("ffmpeg ended with {st}"));
            }
        }
        match self.video_error.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
