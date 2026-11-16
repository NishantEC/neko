//! Draws the 3D viewer's model on Rune's own wgpu device, in Rune's command buffer: an offscreen
//! target (4x MSAA with depth) that Rune's UI pass samples at the element's `El::effect` slot, so the
//! frame never reaches the CPU. This is the canvas spike's compositor pass (docs/knowledge/rune-p3a
//! and `git show origin/spike/rune:experiments/rune/canvas/REPORT.md`: 0.21 ms of GPU for 48k
//! triangles), with matcap shading, a grid floor and two-sided triangles added.
//!
//! It needs the wgpu backend: Rune's default Metal one has no depth buffer and panics on a
//! depth-stencil pipeline (`gpu/metal/pipeline.rs`). The app therefore runs on `GpuBackend::Wgpu`
//! (the measurement that allowed it is in the knowledge note).
//!
//! The renderer lives only while a model is on screen: when the viewer leaves, the buffers and the
//! offscreen target are dropped the same frame.

use std::rc::Rc;
use std::time::Instant;

use rune::Frame;
use rune::compose::{Compositor, DeviceKey};
use wgpu::util::DeviceExt;

use super::Camera;
use super::model3d::{Mesh, VIEW_KEY, Vertex, mvp};
use crate::app::App;
use crate::art;
use crate::art_gpu::HomeArt;
use crate::theme::theme;

const SHADER: &str = r#"
struct U { mvp: mat4x4f, view: mat4x4f, light: vec4f, line: vec4f, grid: vec4f, flags: vec4f };
@group(0) @binding(0) var<uniform> u: U;

struct VO { @builtin(position) pos: vec4f, @location(0) nor: vec3f, @location(1) col: vec3f };

@vertex
fn vs(@location(0) p: vec3f, @location(1) n: vec3f, @location(2) c: vec3f) -> VO {
    var o: VO;
    o.pos = u.mvp * vec4f(p, 1.0);
    o.nor = n;
    o.col = c;
    return o;
}

// Matcap shading: the normal as the camera sees it picks a spot on a lit sphere, so the model reads
// the same whichever way it is turned, with a soft key light, a fill, a rim and a small highlight.
fn matcap(nv: vec3f) -> vec3f {
    let n = normalize(nv);
    let key_dir = normalize(vec3f(-0.45, 0.55, 0.70));
    let key = max(dot(n, key_dir), 0.0);
    let fill = max(dot(n, normalize(vec3f(0.6, -0.2, 0.5))), 0.0);
    let rim = pow(1.0 - max(n.z, 0.0), 2.5);
    let spec = pow(max(dot(reflect(vec3f(0.0, 0.0, -1.0), n), key_dir), 0.0), 24.0);
    return vec3f(0.30) + 0.62 * vec3f(key) + 0.18 * vec3f(fill) + 0.22 * rim * vec3f(0.7, 0.8, 1.0) + 0.35 * vec3f(spec);
}

@fragment
fn fs(i: VO, @builtin(front_facing) front: bool) -> @location(0) vec4f {
    var nv = (u.view * vec4f(normalize(i.nor), 0.0)).xyz;
    if (!front) { nv = -nv; }
    return vec4f(i.col * matcap(nv), 1.0);
}

@vertex
fn vs_line(@location(0) p: vec3f, @location(1) n: vec3f, @location(2) c: vec3f) -> VO {
    var o: VO;
    o.pos = u.mvp * vec4f(p, 1.0);
    o.pos.z = o.pos.z - 0.0004 * o.pos.w;
    o.nor = n;
    o.col = c;
    return o;
}

@fragment
fn fs_line(i: VO) -> @location(0) vec4f { return u.line; }

@vertex
fn vs_grid(@location(0) p: vec3f, @location(1) n: vec3f, @location(2) c: vec3f) -> VO {
    var o: VO;
    o.pos = u.mvp * vec4f(p, 1.0);
    o.pos.z = o.pos.z + 0.0004 * o.pos.w;
    o.nor = n;
    o.col = c;
    return o;
}

@fragment
fn fs_grid(i: VO) -> @location(0) vec4f { return u.grid; }

// Composite: the finished offscreen image into the UI pass.
@group(0) @binding(0) var img: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(0) @binding(2) var<uniform> cu: U;

struct CO { @builtin(position) pos: vec4f, @location(0) uv: vec2f };

@vertex
fn vs_blit(@builtin(vertex_index) vi: u32) -> CO {
    let p = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: CO;
    o.pos = vec4f(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2f(p.x, 1.0 - p.y);
    return o;
}

fn to_srgb(c: vec3f) -> vec3f {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3f(0.0)), vec3f(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3f(0.0031308));
}

@fragment
fn fs_blit(i: CO) -> @location(0) vec4f {
    // The offscreen image is Rgba8UnormSrgb, so this read is linear and premultiplied by alpha.
    let c = textureSampleLevel(img, smp, i.uv, 0.0);
    if (cu.flags.x > 0.5) { return vec4f(to_srgb(c.rgb), c.a); }
    return c;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    light: [f32; 4],
    line: [f32; 4],
    grid: [f32; 4],
    flags: [f32; 4],
}

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;
const TS_FRAMES: u32 = 512;
/// Lines each way in the grid floor.
const GRID_LINES: u32 = 16;

struct Target {
    size: (u32, u32),
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    resolved_view: wgpu::TextureView,
    blit_bg: wgpu::BindGroup,
}

struct Timestamps {
    qs: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period: f32,
    next: u32,
}

/// The GPU side of one model: buffers, pipelines and the offscreen target.
pub struct Renderer {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    ebuf: wgpu::Buffer,
    gbuf: wgpu::Buffer,
    idx_count: u32,
    edge_count: u32,
    grid_count: u32,
    ubuf: wgpu::Buffer,
    blit_ubuf: wgpu::Buffer,
    bg: wgpu::BindGroup,
    blit_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    fill: wgpu::RenderPipeline,
    depth_only: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    grid: wgpu::RenderPipeline,
    blit: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    shader: wgpu::ShaderModule,
    target: Option<Target>,
    ts: Option<Timestamps>,
}

/// The grid floor as line vertices at the model's lowest point, `GRID_LINES` each way across twice
/// the model's radius.
fn grid_vertices(mesh: &Mesh) -> Vec<Vertex> {
    let (cx, cy, z) = (mesh.center[0], mesh.center[1], mesh.floor);
    let ext = mesh.radius * 2.0;
    let mut v = Vec::new();
    let mut vert = |x: f32, y: f32| v.push(Vertex { pos: [x, y, z], nor: [0.0, 0.0, 1.0], col: [1.0; 3] });
    for i in 0..=GRID_LINES {
        let t = -ext + 2.0 * ext * i as f32 / GRID_LINES as f32;
        vert(cx - ext, cy + t);
        vert(cx + ext, cy + t);
        vert(cx + t, cy - ext);
        vert(cx + t, cy + ext);
    }
    v
}

impl Renderer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, mesh: &Mesh) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("model3d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let buf = |label: &str, contents: &[u8], usage| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage });
        let vbuf = buf("verts", bytemuck::cast_slice(&mesh.verts), wgpu::BufferUsages::VERTEX);
        let ibuf = buf("tris", bytemuck::cast_slice(&mesh.tris), wgpu::BufferUsages::INDEX);
        let ebuf = buf("edges", bytemuck::cast_slice(&mesh.edges), wgpu::BufferUsages::INDEX);
        let grid = grid_vertices(mesh);
        let gbuf = buf("grid", bytemuck::cast_slice(&grid), wgpu::BufferUsages::VERTEX);
        let ubuf_size = std::mem::size_of::<Uniforms>() as u64;
        let uniform = |label: &str| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: ubuf_size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let (ubuf, blit_ubuf) = (uniform("u"), uniform("blit u"));
        let ubgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("u"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("u"), layout: &ubgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }] });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("model"), bind_group_layouts: &[Some(&ubgl)], immediate_size: 0 });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];
        let vbl = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs };
        let ms = wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false };
        let mk = |label: &str, vs: &str, fs: &str, topo: wgpu::PrimitiveTopology, depth_write: bool, compare: wgpu::CompareFunction, mask: wgpu::ColorWrites| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[Some(vbl.clone())] },
                primitive: wgpu::PrimitiveState { topology: topo, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(depth_write), depth_compare: Some(compare), stencil: Default::default(), bias: Default::default() }),
                multisample: ms,
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: COLOR, blend: None, write_mask: mask })] }),
                multiview_mask: None,
                cache: None,
            })
        };
        let list = wgpu::PrimitiveTopology::TriangleList;
        let fill = mk("fill", "vs", "fs", list, true, wgpu::CompareFunction::Less, wgpu::ColorWrites::ALL);
        let depth_only = mk("depth only", "vs", "fs", list, true, wgpu::CompareFunction::Less, wgpu::ColorWrites::empty());
        let lines = mk("lines", "vs_line", "fs_line", wgpu::PrimitiveTopology::LineList, false, wgpu::CompareFunction::LessEqual, wgpu::ColorWrites::ALL);
        let grid_pipeline = mk("grid", "vs_grid", "fs_grid", wgpu::PrimitiveTopology::LineList, false, wgpu::CompareFunction::LessEqual, wgpu::ColorWrites::ALL);
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let ts = device.features().contains(wgpu::Features::TIMESTAMP_QUERY).then(|| Timestamps {
            qs: device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("model3d ts"), ty: wgpu::QueryType::Timestamp, count: TS_FRAMES * 2 }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor { label: Some("ts resolve"), size: u64::from(TS_FRAMES * 256), usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false }),
            read: device.create_buffer(&wgpu::BufferDescriptor { label: Some("ts read"), size: u64::from(TS_FRAMES * 256), usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }),
            period: queue.get_timestamp_period(),
            next: 0,
        });
        Renderer {
            vbuf, ibuf, ebuf, gbuf,
            idx_count: mesh.tris.len() as u32,
            edge_count: mesh.edges.len() as u32,
            grid_count: grid.len() as u32,
            ubuf, blit_ubuf, bg, blit_bgl, sampler, fill, depth_only, lines, grid: grid_pipeline, blit: None, shader, target: None, ts,
        }
    }

    fn ensure_target(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let tex = |label: &str, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] })
        };
        let msaa = tex("msaa", COLOR, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = tex("depth", DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolved = tex("resolved", COLOR, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let resolved_view = resolved.create_view(&Default::default());
        let blit_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.blit_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&resolved_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: self.blit_ubuf.as_entire_binding() },
            ],
        });
        self.target = Some(Target { size, msaa_view: msaa.create_view(&Default::default()), depth_view: depth.create_view(&Default::default()), resolved_view, blit_bg });
    }

    /// Records the 3D pass: the grid, then the model shaded, or (wireframe) its depth and edges.
    fn draw_model(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, u: &Uniforms, clear: wgpu::Color, wire: bool) {
        let Some(t) = self.target.as_ref() else { return };
        queue.write_buffer(&self.ubuf, 0, bytemuck::bytes_of(u));
        let stamp = self.ts.as_mut().and_then(|ts| {
            if ts.next >= TS_FRAMES {
                return None;
            }
            ts.next += 1;
            Some((ts.next - 1) * 2)
        });
        let timestamp_writes = stamp.zip(self.ts.as_ref()).map(|(k, ts)| wgpu::RenderPassTimestampWrites { query_set: &ts.qs, beginning_of_pass_write_index: Some(k), end_of_pass_write_index: Some(k + 1) });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("model3d"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &t.msaa_view, depth_slice: None, resolve_target: Some(&t.resolved_view), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Discard } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &t.depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
            timestamp_writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bg, &[]);
        pass.set_pipeline(&self.grid);
        pass.set_vertex_buffer(0, self.gbuf.slice(..));
        pass.draw(0..self.grid_count, 0..1);
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        if wire {
            pass.set_pipeline(&self.depth_only);
            pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.idx_count, 0, 0..1);
            pass.set_pipeline(&self.lines);
            pass.set_index_buffer(self.ebuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.edge_count, 0, 0..1);
        } else {
            pass.set_pipeline(&self.fill);
            pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.idx_count, 0, 0..1);
        }
    }

    /// Resolves the previous frame's timestamps; call at the start of the next frame's encoder.
    fn resolve_timestamps(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(ts) = &self.ts
            && ts.next >= 2
            && ts.next <= TS_FRAMES
        {
            let k = ts.next - 1;
            // QUERY_RESOLVE_BUFFER_ALIGNMENT is 256: one 16-byte pair per 256-byte slot.
            let at = u64::from((k - 1) * 256);
            encoder.resolve_query_set(&ts.qs, (k - 1) * 2..k * 2, &ts.resolve, at);
            encoder.copy_buffer_to_buffer(&ts.resolve, at, &ts.read, at, 16);
        }
    }

    /// GPU milliseconds per timed 3D pass (every frame recorded so far).
    fn gpu_times(&mut self, device: &wgpu::Device) -> Vec<f64> {
        let Some(ts) = &self.ts else { return vec![] };
        let done = u64::from(ts.next.saturating_sub(1));
        if done == 0 {
            return vec![];
        }
        let slice = ts.read.slice(..done * 256);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("mapped");
        let t: &[u64] = bytemuck::cast_slice(&data);
        let out = t.chunks_exact(32).map(|c| (c[0], c[1])).filter(|p| p.1 > p.0).map(|p| (p.1 - p.0) as f64 * f64::from(ts.period) / 1e6).collect();
        drop(data);
        ts.read.unmap();
        out
    }

    /// Draws the offscreen image into the already-open UI pass, inside the slot's rectangle
    /// (`rect` is `[x, y, w, h]` in the pass's own device pixels).
    #[allow(clippy::too_many_arguments)]
    fn composite(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'_>, format: wgpu::TextureFormat, srgb_blend: bool, rect: [f32; 4], pass_size: [u32; 2]) {
        let Some(t) = self.target.as_ref() else { return };
        if self.blit.as_ref().is_none_or(|(f, _)| *f != format) {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("blit"), bind_group_layouts: &[Some(&self.blit_bgl)], immediate_size: 0 });
            let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("blit"),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &self.shader, entry_point: Some("vs_blit"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &self.shader, entry_point: Some("fs_blit"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
                multiview_mask: None,
                cache: None,
            });
            self.blit = Some((format, p));
        }
        let mut u = Uniforms::zeroed();
        u.flags[0] = if srgb_blend { 1.0 } else { 0.0 };
        queue.write_buffer(&self.blit_ubuf, 0, bytemuck::bytes_of(&u));
        let x = rect[0].max(0.0);
        let y = rect[1].max(0.0);
        let w = rect[2].min(pass_size[0] as f32 - x).max(1.0);
        let h = rect[3].min(pass_size[1] as f32 - y).max(1.0);
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        pass.set_pipeline(&self.blit.as_ref().expect("pipeline").1);
        pass.set_bind_group(0, &t.blit_bg, &[]);
        pass.draw(0..3, 0..1);
    }
}

use bytemuck::Zeroable;

/// What the compositor reads of the app each frame: the model on screen, its camera and the
/// appearance, or nothing when no model is shown.
pub struct Scene {
    pub mesh: Rc<Mesh>,
    pub camera: Camera,
    pub dark: bool,
}

/// The compositor: draws the app's frame, and the 3D viewer's model at its slot when one is on screen.
#[derive(Default)]
pub struct Viewer3d {
    renderer: Option<Renderer>,
    mesh_id: u64,
    device_key: Option<DeviceKey>,
    drew: bool,
    cpu_ms: Vec<f64>,
    /// Home's art (`art_gpu.rs`): its pipeline, made on first use.
    art: Option<(HomeArt, DeviceKey)>,
    /// The art's own clock, in seconds: it only runs while the art is on screen, the window is active and Reduce Motion is off.
    art_clock: f64,
    /// The art is moving now: the window keeps its display link running only while this is true.
    art_moving: bool,
    /// GPU milliseconds per frame of the art at 2560x1600 (`NEKO_RUNE_ART_BENCH=1`).
    art_bench: Option<Vec<f64>>,
    art_floor: Vec<f64>,
}

impl Viewer3d {
    fn release(&mut self) {
        self.renderer = None;
        self.mesh_id = 0;
    }
}

impl Viewer3d {
    /// Home's art: the window's frame with the shader drawn at the art's slot. Home and a model never share the screen.
    fn render_art(&mut self, f: &mut Frame<'_, App>) {
        self.release();
        let dark = f.rt.state.panel.dark.get();
        let moving = art::moving(f.rt.state.focused, f.rt.state.reduce_motion.get());
        if moving {
            // A long stall (a dragged window, a sleeping Mac) must not jump the stars.
            self.art_clock += f.dt.min(0.1);
        }
        self.art_moving = moving;
        let key = f.device_key();
        if self.art.as_ref().is_none_or(|(_, k)| *k != key) {
            self.art = Some((HomeArt::new(f.device), key));
            self.art_bench = None;
        }
        let scale = f.rt.env.scale;
        let look = art::look(dark, self.art_clock as f32, scale);
        let (device, queue) = (f.device, f.queue);
        let Some((gpu, _)) = self.art.as_mut() else { return };
        if self.art_bench.is_none() && std::env::var_os("NEKO_RUNE_ART_BENCH").is_some() {
            // Steady state: the first 120 of 480 frames are the GPU's clock coming up, and are dropped.
            let steady = |mut v: Vec<f64>| v.split_off(v.len().min(120));
            self.art_bench = Some(gpu.bench(device, queue, (2560, 1600), 480, &art::look(dark, 1.0, 2.0)).map(steady).unwrap_or_default());
            // What an empty pass of the same size costs, to read the art's own share.
            self.art_floor = gpu.bench(device, queue, (2560, 1600), 480, &crate::art_gpu::Look { floor: true, ..art::look(dark, 1.0, 2.0) }).map(steady).unwrap_or_default();
        }
        // The scene is drawn small, one texel per dither cell, before the window's pass; the pass expands it.
        if let Some((rect, _)) = f.rt.presented(art::SLOT_KEY) {
            gpu.prepare(device, queue, f.encoder, ((rect[2] * scale).round() as u32, (rect[3] * scale).round() as u32), &look);
        }
        f.ui.prepare(f.device, f.queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
        let clear = f.rt.clear_color();
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if site.mark == art::MARK {
                gpu.draw(device, queue, pass, site, &look);
            }
        });
        self.drew = true;
    }
}

impl Compositor<App> for Viewer3d {
    fn features(&self) -> wgpu::Features {
        wgpu::Features::TIMESTAMP_QUERY
    }

    fn draws_own(&self) -> bool {
        self.drew
    }

    fn animating(&self) -> bool {
        self.art_moving
    }

    fn render(&mut self, f: &mut Frame<'_, App>) {
        let started = Instant::now();
        if f.rt.presented(art::SLOT_KEY).is_some_and(|(r, opacity)| r[2] > 8.0 && r[3] > 8.0 && opacity > 0.0) {
            self.render_art(f);
            return;
        }
        self.art_moving = false;
        let scene = f.rt.state.panel_scene();
        let rect = f.rt.presented(VIEW_KEY).map(|(r, _)| r);
        let (Some(scene), Some(rect)) = (scene, rect) else {
            // No model on screen: let go of the GPU side and draw the window as Rune would.
            self.release();
            self.drew = false;
            f.draw_ui();
            return;
        };
        let scale = f.rt.env.scale;
        let size = ((rect[2] * scale).round() as u32, (rect[3] * scale).round() as u32);
        if size.0 < 8 || size.1 < 8 {
            self.drew = false;
            f.draw_ui();
            return;
        }
        if self.device_key != Some(f.device_key()) || self.mesh_id != scene.mesh.id || self.renderer.is_none() {
            self.renderer = Some(Renderer::new(f.device, f.queue, &scene.mesh));
            self.device_key = Some(f.device_key());
            self.mesh_id = scene.mesh.id;
        }
        let t = theme(scene.dark);
        let (bg, fg) = (t.surface.window, t.text.base);
        let (grid, line) = (bg.mix(fg, 0.16), fg.mix(bg, 0.12));
        let (m, view) = mvp(&scene.mesh, &scene.camera, size.0 as f32 / size.1 as f32);
        let u = Uniforms { mvp: m, view, light: [0.4, -0.6, 0.8, 0.0], line: [line.r, line.g, line.b, 1.0], grid: [grid.r, grid.g, grid.b, 1.0], flags: [0.0; 4] };
        let clear = wgpu::Color { r: f64::from(bg.r), g: f64::from(bg.g), b: f64::from(bg.b), a: 1.0 };
        let r = self.renderer.as_mut().expect("renderer");
        r.ensure_target(f.device, size);
        r.resolve_timestamps(f.encoder);
        r.draw_model(f.queue, f.encoder, &u, clear, scene.camera.wire);
        f.ui.prepare(f.device, f.queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
        let clear_ui = f.rt.clear_color();
        let (device, queue) = (f.device, f.queue);
        let srgb = f.ui.srgb_blend();
        f.ui.draw_with(f.encoder, f.target, clear_ui, &mut |pass, site| {
            r.composite(device, queue, pass, site.format, srgb, site.instance.rect, site.size);
        });
        self.drew = true;
        if self.cpu_ms.len() < 20_000 {
            self.cpu_ms.push(started.elapsed().as_secs_f64() * 1e3);
        }
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
        let mut lines = vec![stat("model3d compositor CPU (encode, excl. Runtime::frame)", &self.cpu_ms), stat("model3d 3D pass GPU", &gpu)];
        if let Some(bench) = self.art_bench.as_ref() {
            lines.push(stat("home art GPU at 2560x1600", bench));
            lines.push(stat("empty pass GPU at 2560x1600", &self.art_floor));
        }
        Some(lines.join("\n"))
    }
}
