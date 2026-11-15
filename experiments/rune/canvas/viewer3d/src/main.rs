//! A glTF viewer inside Rune. The model is drawn with Rune's own wgpu device from a `Compositor`:
//! offscreen (4x MSAA + depth) and then sampled by the UI pass at an `El::effect` slot, in the same
//! command buffer, so no frame crosses the CPU. `VIEWER3D_MODE=pixels` instead reads the frame back
//! and hands it to `El::pixels`, to measure what that route costs.
//!
//! `VIEWER3D_MODEL=assets/knot.glb viewer3d --snap out.png --scene wire`

mod gpu;
mod model;

use gpu::{Renderer, m4};
use rune::tok::{self, radius, space, type_};
use rune::{AppSpec, Color, Cx, El, Frame, Phase, Pixels, col, el, row, text};
use std::sync::OnceLock;
use std::time::Instant;

const FOV_Y: f32 = 40.0 * std::f32::consts::PI / 180.0;
const DEFAULT_YAW: f32 = 0.7;
const DEFAULT_PITCH: f32 = 0.45;

static PIXELS: OnceLock<Pixels> = OnceLock::new();
static MODEL_INFO: OnceLock<(usize, usize, usize)> = OnceLock::new();

#[derive(Clone, Copy)]
struct Cam {
    yaw: f32,
    pitch: f32,
    /// 1.0 = fitted: the model's bounding sphere just fills the narrower side of the viewport.
    zoom: f32,
    drag_from: (f32, f32),
}

struct State {
    cam: Cam,
    wire: bool,
    pixels_mode: bool,
}

impl Default for Cam {
    fn default() -> Self {
        Cam { yaw: DEFAULT_YAW, pitch: DEFAULT_PITCH, zoom: 1.0, drag_from: (0.0, 0.0) }
    }
}

fn init() -> State {
    State { cam: Cam::default(), wire: false, pixels_mode: std::env::var("VIEWER3D_MODE").is_ok_and(|m| m == "pixels") }
}

fn button(label: &'static str, key: &'static str, on: bool, f: impl Fn(&mut State) + 'static) -> El {
    let t = tok::theme(true);
    row().key(key).role("button").label(label).focusable(true).px(space::S3).py(space::S1).radius(radius::MD)
        .fill(if on { tok::accent(0, true) } else { t.control_off })
        .child(text(label).font(type_::BODY).color(t.text))
        .on_press(move |s: &mut State| f(s))
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let (verts, tris, edges) = MODEL_INFO.get().copied().unwrap_or((0, 0, 0));
    let stage = if s.pixels_mode {
        el().key("viewport").grow(1.0).w_full().radius(radius::LG).pixels(PIXELS.get().expect("pixels"))
    } else {
        el().key("viewport").grow(1.0).w_full().radius(radius::LG).effect(1)
    };
    let stage = stage
        .focusable(true)
        .on_drag(|s: &mut State, d, tx| {
            match d.phase {
                Phase::Down => s.cam.drag_from = (s.cam.yaw, s.cam.pitch),
                _ => {
                    s.cam.yaw = s.cam.drag_from.0 - d.dx * 0.01;
                    s.cam.pitch = (s.cam.drag_from.1 + d.dy * 0.01).clamp(-1.5, 1.5);
                }
            }
            tx.live_only();
        })
        .on_wheel(|s: &mut State, e, tx| {
            s.cam.zoom = (s.cam.zoom * (e.dy * 0.004).exp()).clamp(0.2, 30.0);
            tx.live_only();
            true
        })
        .on_key(|s: &mut State, k, _| match &k.key {
            rune::Key::Text(c) if c == "f" => { s.cam = Cam::default(); true }
            rune::Key::Text(c) if c == "w" => { s.wire = !s.wire; true }
            _ => false,
        });
    col().w_full().h_full().padding(space::S3).gap(space::S3).fill(t.bg)
        .child(row().gap(space::S2).items_center()
            .child(button("Fit to view", "fit", false, |s| s.cam = Cam::default()))
            .child(button("Wireframe", "wire", s.wire, |s| s.wire = !s.wire))
            .child(text(format!("{tris} triangles, {verts} vertices, {edges} edges. Drag to orbit, scroll to zoom. {}", if s.pixels_mode { "mode: pixels (readback)" } else { "mode: shared device" }))
                .font(type_::BODY).color(t.text_secondary)))
        .child(stage)
}

struct Viewer {
    renderer: Option<Renderer>,
    model: model::Model,
    device_key: Option<rune::compose::DeviceKey>,
    cpu_ms: Vec<f64>,
    readback_ms: Vec<f64>,
}

impl Viewer {
    fn mvp(&self, cam: &Cam, aspect: f32) -> m4::M {
        let m = &self.model;
        let half_x = (FOV_Y / 2.0).tan() * aspect;
        let half = ((FOV_Y / 2.0).tan()).min(half_x);
        let fit = m.radius / half.atan().sin();
        let dist = fit * 1.08 / cam.zoom;
        let (cy, sy, cp, sp) = (cam.yaw.cos(), cam.yaw.sin(), cam.pitch.cos(), cam.pitch.sin());
        let eye = [m.center[0] + dist * cp * cy, m.center[1] + dist * cp * sy, m.center[2] + dist * sp];
        let view = m4::look_at(eye, m.center, [0.0, 0.0, 1.0]);
        let near = (dist - m.radius * 1.5).max(dist * 0.01).max(0.01);
        let proj = m4::perspective(FOV_Y, aspect, near, dist + m.radius * 2.0);
        m4::mul(&proj, &view)
    }
}

impl rune::compose::Compositor<State> for Viewer {
    fn features(&self) -> wgpu::Features {
        wgpu::Features::TIMESTAMP_QUERY
    }

    fn render(&mut self, f: &mut Frame<'_, State>) {
        let started = Instant::now();
        if self.device_key != Some(f.device_key()) {
            self.renderer = Some(Renderer::new(f.device, f.queue, &self.model));
            self.device_key = Some(f.device_key());
        }
        let scale = f.rt.env.scale;
        let rect = f.rt.presented("viewport").map(|(r, _)| r);
        let size = rect.map_or((0, 0), |r| ((r[2] * scale).round() as u32, (r[3] * scale).round() as u32));
        let (cam, wire, pixels_mode) = (f.rt.state.cam, f.rt.state.wire, f.rt.state.pixels_mode);
        let r = self.renderer.as_mut().expect("renderer");
        if size.0 < 8 || size.1 < 8 {
            f.draw_ui();
            return;
        }
        r.ensure_target(f.device, size);
        let mvp = self.mvp(&cam, size.0 as f32 / size.1 as f32);
        let r = self.renderer.as_mut().expect("renderer");
        if pixels_mode {
            // The copying route: render on our own submit, read the frame back, hand it to El::pixels.
            let t0 = Instant::now();
            let mut enc = f.device.create_command_encoder(&Default::default());
            r.draw_model(f.queue, &mut enc, mvp, wire);
            let padded = (size.0 * 4).div_ceil(256) * 256;
            let buf = f.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: (padded * size.1) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            let tgt = r.target.as_ref().expect("target");
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &tgt.resolved, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(size.1) } },
                wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            );
            f.queue.submit([enc.finish()]);
            let slice = buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = f.device.poll(wgpu::PollType::wait_indefinitely());
            let data = slice.get_mapped_range().expect("mapped");
            let mut rgba = Vec::with_capacity((size.0 * size.1 * 4) as usize);
            for y in 0..size.1 as usize {
                rgba.extend_from_slice(&data[y * padded as usize..y * padded as usize + size.0 as usize * 4]);
            }
            drop(data);
            buf.unmap();
            PIXELS.get().expect("pixels").set(size.0, size.1, rgba);
            self.readback_ms.push(t0.elapsed().as_secs_f64() * 1e3);
            f.draw_ui();
        } else {
            r.resolve_timestamps(f.encoder);
            r.draw_model(f.queue, f.encoder, mvp, wire);
            f.ui.prepare(f.device, f.queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
            let clear = f.rt.clear_color();
            let (device, queue) = (f.device, f.queue);
            let srgb = f.ui.srgb_blend();
            f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
                r.composite(device, queue, pass, site.format, srgb, site.instance.rect, site.size);
            });
        }
        self.cpu_ms.push(started.elapsed().as_secs_f64() * 1e3);
    }

    fn summary(&mut self, device: &wgpu::Device) -> Option<String> {
        let stat = |name: &str, v: &[f64]| {
            if v.is_empty() {
                return format!("{name}: no samples");
            }
            let mut s = v.to_vec();
            s.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
            let mean = s.iter().sum::<f64>() / s.len() as f64;
            format!("{name}: n={} mean {:.3} ms, p50 {:.3}, p95 {:.3}, max {:.3}", s.len(), mean, s[s.len() / 2], s[(s.len() * 95 / 100).min(s.len() - 1)], s[s.len() - 1])
        };
        let gpu = self.renderer.as_mut().map(|r| r.gpu_times(device)).unwrap_or_default();
        let mut out = vec![stat("viewer3d compositor CPU (encode, excl. Runtime::frame)", &self.cpu_ms), stat("viewer3d 3D pass GPU", &gpu)];
        if !self.readback_ms.is_empty() {
            out.push(stat("viewer3d pixels route: render+readback+Pixels::set (CPU wall, blocking)", &self.readback_ms));
        }
        Some(out.join("\n"))
    }
}

fn main() {
    let path = std::env::var("VIEWER3D_MODEL").unwrap_or_else(|_| "assets/knot.glb".into());
    let loaded = Instant::now();
    let model = match model::load(&path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("viewer3d: cannot load {path}: {e}");
            std::process::exit(2);
        }
    };
    eprintln!("viewer3d: loaded {path}: {} triangles, {} vertices, {} edges in {:.1} ms", model.tris.len() / 3, model.verts.len(), model.edges.len() / 2, loaded.elapsed().as_secs_f64() * 1e3);
    let _ = MODEL_INFO.set((model.verts.len(), model.tris.len() / 3, model.edges.len() / 2));
    let _ = PIXELS.set(Pixels::new());
    let spec = AppSpec::<State> {
        scenes: &[
            ("solid", |_| {}),
            ("wire", |s| s.wire = true),
            ("side", |s| { s.cam.yaw = 0.0; s.cam.pitch = 0.0; }),
            ("zoomed", |s| s.cam.zoom = 3.0),
        ],
        ..AppSpec::new("Viewer3D", (900.0, 640.0), init, view)
    };
    // Rune's default macOS backend is its own Metal one, which has no depth buffer or MSAA
    // (it panics on a depth-stencil pipeline), so a compositor that draws 3D has to opt out.
    rune::App::new(spec).gpu_backend(rune::GpuBackend::Wgpu).compositor(Viewer { renderer: None, model, device_key: None, cpu_ms: vec![], readback_ms: vec![] }).run();
}

#[allow(dead_code)]
const _UNUSED: Color = Color::WHITE;
