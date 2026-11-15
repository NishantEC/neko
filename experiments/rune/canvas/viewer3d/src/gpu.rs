//! The 3D renderer: draws the model with Rune's own wgpu device into an offscreen target
//! (4x MSAA + depth), which the UI pass then samples at the `El::effect` slot.

use crate::model::{Model, Vertex};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct U { mvp: mat4x4f, light: vec4f, line: vec4f, flags: vec4f };
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

@fragment
fn fs(i: VO) -> @location(0) vec4f {
    let n = normalize(i.nor);
    let l = normalize(u.light.xyz);
    let diff = max(dot(n, l), 0.0);
    let back = max(dot(n, -l), 0.0) * 0.25;
    let hemi = 0.35 + 0.15 * n.z;
    let rgb = i.col * (hemi + 0.75 * diff + back);
    return vec4f(rgb, 1.0);
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
    light: [f32; 4],
    line: [f32; 4],
    flags: [f32; 4],
}

pub const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const SAMPLES: u32 = 4;
pub const TS_FRAMES: u32 = 512;

pub struct Target {
    pub size: (u32, u32),
    pub resolved: wgpu::Texture,
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    pub resolved_view: wgpu::TextureView,
    blit_bg: wgpu::BindGroup,
}

pub struct Renderer {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    ebuf: wgpu::Buffer,
    pub tri_count: u32,
    idx_count: u32,
    edge_count: u32,
    ubuf: wgpu::Buffer,
    blit_ubuf: wgpu::Buffer,
    bg: wgpu::BindGroup,
    blit_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    fill: wgpu::RenderPipeline,
    depth_only: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    blit: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    shader: wgpu::ShaderModule,
    pub target: Option<Target>,
    // GPU timestamps of the 3D pass, one slot per frame, read back in `summary`.
    ts: Option<Timestamps>,
}

struct Timestamps {
    qs: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period: f32,
    next: u32,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, model: &Model) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("viewer3d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("verts"), contents: bytemuck::cast_slice(&model.verts), usage: wgpu::BufferUsages::VERTEX });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("tris"), contents: bytemuck::cast_slice(&model.tris), usage: wgpu::BufferUsages::INDEX });
        let ebuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("edges"), contents: bytemuck::cast_slice(&model.edges), usage: wgpu::BufferUsages::INDEX });
        let ubuf_size = std::mem::size_of::<Uniforms>() as u64;
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("u"), size: ubuf_size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let blit_ubuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("blit u"), size: ubuf_size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
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
        let fill = mk("fill", "vs", "fs", wgpu::PrimitiveTopology::TriangleList, true, wgpu::CompareFunction::Less, wgpu::ColorWrites::ALL);
        let depth_only = mk("depth only", "vs", "fs", wgpu::PrimitiveTopology::TriangleList, true, wgpu::CompareFunction::Less, wgpu::ColorWrites::empty());
        let lines = mk("lines", "vs_line", "fs_line", wgpu::PrimitiveTopology::LineList, false, wgpu::CompareFunction::LessEqual, wgpu::ColorWrites::ALL);
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
            qs: device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("viewer3d ts"), ty: wgpu::QueryType::Timestamp, count: TS_FRAMES * 2 }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor { label: Some("ts resolve"), size: (TS_FRAMES * 256) as u64, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false }),
            read: device.create_buffer(&wgpu::BufferDescriptor { label: Some("ts read"), size: (TS_FRAMES * 256) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }),
            period: queue.get_timestamp_period(),
            next: 0,
        });
        Renderer {
            vbuf, ibuf, ebuf,
            tri_count: (model.tris.len() / 3) as u32,
            idx_count: model.tris.len() as u32,
            edge_count: model.edges.len() as u32,
            ubuf, blit_ubuf, bg, blit_bgl, sampler, fill, depth_only, lines, blit: None, shader, target: None, ts,
        }
    }

    pub fn ensure_target(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let tex = |label: &str, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] })
        };
        let msaa = tex("msaa", COLOR, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = tex("depth", DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolved = tex("resolved", COLOR, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC);
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
        self.target = Some(Target { size, msaa_view: msaa.create_view(&Default::default()), depth_view: depth.create_view(&Default::default()), resolved, resolved_view, blit_bg });
    }

    /// Records the 3D pass into `encoder`: depth pre-pass + lines for the wireframe, else shaded fill.
    pub fn draw_model(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, mvp: [[f32; 4]; 4], wire: bool) {
        let Some(t) = self.target.as_ref() else { return };
        let u = Uniforms { mvp, light: [0.4, -0.6, 0.8, 0.0], line: [0.93, 0.93, 0.95, 1.0], flags: [0.0; 4] };
        queue.write_buffer(&self.ubuf, 0, bytemuck::bytes_of(&u));
        let timestamp_writes = self.ts.as_mut().and_then(|ts| {
            if ts.next >= TS_FRAMES {
                return None;
            }
            let k = ts.next * 2;
            ts.next += 1;
            Some((k, ()))
        });
        let ts_ref = self.ts.as_ref();
        let tw = timestamp_writes.zip(ts_ref).map(|((k, _), ts)| wgpu::RenderPassTimestampWrites { query_set: &ts.qs, beginning_of_pass_write_index: Some(k), end_of_pass_write_index: Some(k + 1) });
        let bg_color = wgpu::Color { r: 0.012, g: 0.014, b: 0.02, a: 1.0 };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewer3d model"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &t.msaa_view, depth_slice: None, resolve_target: Some(&t.resolved_view), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(bg_color), store: wgpu::StoreOp::Discard } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &t.depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
            timestamp_writes: tw,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bg, &[]);
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
    pub fn resolve_timestamps(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(ts) = &self.ts {
            if ts.next >= 2 && ts.next <= TS_FRAMES {
                let k = ts.next - 1;
                // QUERY_RESOLVE_BUFFER_ALIGNMENT is 256: one 16-byte pair per 256-byte slot.
                let at = ((k - 1) * 256) as u64;
                encoder.resolve_query_set(&ts.qs, (k - 1) * 2..k * 2, &ts.resolve, at);
                encoder.copy_buffer_to_buffer(&ts.resolve, at, &ts.read, at, 16);
            }
        }
    }

    /// GPU milliseconds per timed 3D pass (all frames recorded so far).
    pub fn gpu_times(&mut self, device: &wgpu::Device) -> Vec<f64> {
        let Some(ts) = &self.ts else { return vec![] };
        let done = ts.next.saturating_sub(1) as u64;
        if done == 0 {
            return vec![];
        }
        let slice = ts.read.slice(..done * 256);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("mapped");
        let t: &[u64] = bytemuck::cast_slice(&data);
        let out = t.chunks_exact(32).map(|c| (c[0], c[1])).filter(|p| p.1 > p.0).map(|p| (p.1 - p.0) as f64 * ts.period as f64 / 1e6).collect();
        drop(data);
        ts.read.unmap();
        out
    }

    /// Draws the offscreen image into the already-open UI pass, inside the slot's rectangle
    /// (`rect` is `[x, y, w, h]` in the pass's own device pixels).
    pub fn composite(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'_>, format: wgpu::TextureFormat, srgb_blend: bool, rect: [f32; 4], pass_size: [u32; 2]) {
        let Some(t) = self.target.as_ref() else { return };
        if self.blit.as_ref().is_none_or(|(f, _)| *f != format) {
            let bgl = &self.blit_bgl;
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("blit"), bind_group_layouts: &[Some(bgl)], immediate_size: 0 });
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
        let mut u = Uniforms { mvp: [[0.0; 4]; 4], light: [0.0; 4], line: [0.0; 4], flags: [0.0; 4] };
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

/// Column-major 4x4 helpers (wgpu clip space: z in 0..1).
pub mod m4 {
    pub type M = [[f32; 4]; 4];
    pub fn mul(a: &M, b: &M) -> M {
        let mut o = [[0.0; 4]; 4];
        for c in 0..4 {
            for r in 0..4 {
                o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
            }
        }
        o
    }
    pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> M {
        let f = 1.0 / (fovy / 2.0).tan();
        [[f / aspect, 0.0, 0.0, 0.0], [0.0, f, 0.0, 0.0], [0.0, 0.0, far / (near - far), -1.0], [0.0, 0.0, near * far / (near - far), 0.0]]
    }
    pub fn look_at(eye: [f32; 3], at: [f32; 3], up: [f32; 3]) -> M {
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let norm = |a: [f32; 3]| {
            let l = dot(a, a).sqrt().max(1e-9);
            [a[0] / l, a[1] / l, a[2] / l]
        };
        let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let f = norm(sub(at, eye));
        let s = norm(cross(f, up));
        let u = cross(s, f);
        [[s[0], u[0], -f[0], 0.0], [s[1], u[1], -f[1], 0.0], [s[2], u[2], -f[2], 0.0], [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0]]
    }
}
