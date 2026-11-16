//! Home's art on the GPU (polish spec §3): an original night scene drawn procedurally in one fragment
//! shader at an effect slot (`El::effect(art::MARK)`), inside Rune's own UI pass, no texture and no raster
//! image. A sky with sparse twinkling stars, a large soft moon top-right, a calm sea in the lower third
//! with a moon glade, and on a small rock at the horizon the silhouette of a sitting cat facing the moon.
//!
//! The look is ordered dither: a smooth value field (0 to 1) is thresholded by a Bayer 8x8 matrix, so every
//! pixel is either the theme's Iris or the window's colour, the 1-bit look made with halftone. The dither
//! cell is one logical point, so the pattern is the same size on every display. In light it is the same scene
//! as dawn: a pale ground, Iris at 35% strength and a darker cat.
//!
//! Motion: each star twinkles on its own period (3 to 7 s) and phase, the sea's pattern drifts 6 points a
//! second, the moon glade shimmers. All of it is a function of one time uniform that only advances while
//! the art is on screen, the window is active and Reduce Motion is off; otherwise the time stands still.
//!
//! Budget: at most 1.0 ms of GPU per frame at 2560x1600; [`HomeArt::bench`] measures it with timestamp
//! queries (`NEKO_RUNE_ART_BENCH=1` prints it after a headless run; `tests/art.rs` checks it).

use rune::render::EffectSite;
use wgpu::util::DeviceExt;

/// Matches `art::MARK`'s payload: [dark, reduce motion, 0, 0].
pub const STRIDE_VEC4: usize = 6;

const SHADER: &str = r#"
struct U {
  rect: vec4f,   // slot x, y, w, h in device pixels
  a: vec4f,      // time (s), dark (1 or 0), dither cell in device pixels, srgb blend (1 or 0)
  bg: vec4f,     // the window colour, linear
  ink: vec4f,    // Iris, linear
  cat: vec4f,    // the cat's colour, linear
  b: vec4f,      // dot strength, density, 0, 0
};
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var img: texture_2d<f32>;

struct VO { @builtin(position) pos: vec4f };

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VO {
    let p = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: VO;
    o.pos = vec4f(p * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

fn hash(p: vec2u) -> u32 {
    var h = p.x * 374761393u + p.y * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    return h ^ (h >> 16u);
}
fn rnd(p: vec2u, salt: u32) -> f32 {
    return f32(hash(p + vec2u(salt * 7919u, salt * 104729u)) & 0xffffffu) / 16777216.0;
}

// Bayer 8x8: the bit-reversed interleave of x ^ y and y, in 6 bits.
fn bayer8(g: vec2u) -> f32 {
    let x = g.x & 7u;
    let y = g.y & 7u;
    var v = 0u;
    for (var k = 0u; k < 3u; k = k + 1u) {
        let xb = (x >> k) & 1u;
        let yb = (y >> k) & 1u;
        v = (v << 2u) | ((xb ^ yb) << 1u) | yb;
    }
    return (f32(v) + 0.5) / 64.0;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}
fn sd_ellipse(p: vec2f, c: vec2f, r: vec2f) -> f32 {
    let q = (p - c) / r;
    return (length(q) - 1.0) * min(r.x, r.y);
}
fn sd_capsule(p: vec2f, a: vec2f, b: vec2f, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - r;
}
fn sd_tri(p: vec2f, a: vec2f, b: vec2f, c: vec2f) -> f32 {
    let e0 = b - a; let e1 = c - b; let e2 = a - c;
    let v0 = p - a; let v1 = p - b; let v2 = p - c;
    let p0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    let p1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    let p2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d0 = vec2f(dot(p0, p0), s * (v0.x * e0.y - v0.y * e0.x));
    let d1 = vec2f(dot(p1, p1), s * (v1.x * e1.y - v1.y * e1.x));
    let d2 = vec2f(dot(p2, p2), s * (v2.x * e2.y - v2.y * e2.x));
    let d = min(min(d0, d1), d2);
    return -sqrt(d.x) * sign(d.y);
}

// The sitting cat, facing right, in its own units (feet on the rock at y = 0.08, y up). Negative inside.
fn cat_sd(q: vec2f) -> f32 {
    if (abs(q.x) > 1.05 || q.y < -0.35 || q.y > 1.55) { return 1.0; }
    var d = sd_ellipse(q, vec2f(0.02, -0.06), vec2f(0.60, 0.17));                       // the rock
    let body = sd_ellipse(q, vec2f(-0.02, 0.50), vec2f(0.29, 0.42));
    let haunch = sd_ellipse(q, vec2f(-0.10, 0.28), vec2f(0.33, 0.22));
    let neck = sd_ellipse(q, vec2f(0.05, 0.86), vec2f(0.19, 0.20));
    let head = sd_ellipse(q, vec2f(0.11, 1.04), vec2f(0.21, 0.19));
    let ear_a = sd_tri(q, vec2f(-0.07, 1.09), vec2f(0.07, 1.19), vec2f(-0.03, 1.42));
    let ear_b = sd_tri(q, vec2f(0.15, 1.19), vec2f(0.29, 1.09), vec2f(0.27, 1.42));
    let tail = min(sd_capsule(q, vec2f(-0.28, 0.14), vec2f(-0.56, 0.10), 0.05),
               min(sd_capsule(q, vec2f(-0.56, 0.10), vec2f(-0.69, 0.30), 0.045),
                   sd_capsule(q, vec2f(-0.69, 0.30), vec2f(-0.66, 0.56), 0.04)));
    var c = smin(body, haunch, 0.12);
    c = smin(c, neck, 0.10);
    c = smin(c, head, 0.10);
    c = min(c, min(ear_a, ear_b));
    c = min(c, tail);
    return min(d, c);
}

fn to_srgb(c: vec3f) -> vec3f {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3f(0.0)), vec3f(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3f(0.0031308));
}

// The scene pass: one texel per dither cell (the target is the slot divided by the cell), so the shader runs
// once per cell, not once per device pixel. The blit below expands each texel to its cell.
@fragment
fn fs(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    if (u.b.z > 0.5) { return vec4f(u.bg.rgb, 1.0); }   // the benchmark's floor: the pass with nothing drawn
    let size = u.rect.zw;
    let cell = max(u.a.z, 1.0);
    let g = floor(frag.xy);
    let uv = (g + 0.5) * cell / size;
    let asp = size.x / size.y;
    let p = vec2f(uv.x * asp, uv.y);          // height units, y down
    let t = u.a.x;
    let px = cell / size.y;                   // one dither cell in height units
    let hz = 0.70;
    let moon = vec2f(asp * 0.80, 0.22);

    var v = 0.0;
    if (p.y < hz) {
        let k = p.y / hz;
        v = 0.04 + 0.24 * k * k * sqrt(k);
        let d = length(p - moon);
        let R = 0.085;
        if (d < 0.9) { v = v + 0.34 * exp(-d * 5.0); }
        // the moon: a disc with two soft maria
        var disc = 1.0 - smoothstep(R * 0.93, R, d);
        let m1 = 1.0 - smoothstep(0.016, 0.026, length(p - moon - vec2f(-0.022, -0.012)));
        let m2 = 1.0 - smoothstep(0.010, 0.020, length(p - moon - vec2f(0.026, 0.024)));
        disc = disc * (1.0 - 0.30 * max(m1, m2));
        v = max(v, 0.97 * disc);
        // stars: one per cell at most, jittered, each on its own period (3 to 7 s) and phase
        let S = 0.030;
        let id = vec2u(vec2i(floor(p / S) + vec2f(4096.0)));
        let local = fract(p / S);
        if (rnd(id, 1u) < 0.10 && p.y < hz - 0.03 && d > R * 1.7) {
            let centre = vec2f(0.28 + 0.44 * rnd(id, 2u), 0.28 + 0.44 * rnd(id, 3u));
            let rad = (0.9 + 1.4 * rnd(id, 4u)) * px;
            let ds = length((local - centre) * S);
            let period = 3.0 + 4.0 * rnd(id, 5u);
            let tw = 0.55 + 0.45 * sin(6.2831853 * t / period + 6.2831853 * rnd(id, 6u));
            v = max(v, (1.0 - smoothstep(0.0, rad, ds)) * tw * 1.15);
        }
    } else {
        let y = p.y - hz;
        let depth = clamp(y / 0.30, 0.0, 1.0);
        let z = 1.0 / (y + 0.035);
        // the sea's pattern drifts 6 points a second
        let x = p.x + 6.0 * t * cell / size.y;
        let wv = sin(z * 4.6 + sin(x * 8.0 + z * 0.55) * 0.9);
        v = 0.09 + 0.07 * (0.5 + 0.5 * wv) + 0.12 * (1.0 - depth);
        v = v + smoothstep(0.80, 1.0, wv) * 0.14 * (1.0 - 0.5 * depth);
        // the moon glade: a column of shimmer under the moon, widening toward the viewer
        let w = 0.012 + 0.17 * y;
        let gx = (p.x - moon.x) / w;
        // Past three widths from the middle the glade is nothing: skip its three sines and its exponential.
        if (abs(gx) < 3.0) {
            let glade = exp(-gx * gx);
            let shimmer = 0.5 + 0.5 * sin(z * 6.2 - t * 1.6 + sin(x * 26.0 + t * 0.8) * 1.4);
            v = v + glade * (0.20 + 0.75 * shimmer) * (1.0 - 0.40 * depth);
        }
    }
    v = v * u.b.y;

    let on = select(0.0, 1.0, v > bayer8(vec2u(g)));
    var col = mix(u.bg.rgb, u.ink.rgb, on * u.b.x);

    // the cat on its rock, on the horizon, facing the moon: a flat silhouette, edges one cell soft
    let s = 0.066;
    let q = vec2f(p.x - asp * 0.14, -(p.y - (hz - 0.004))) / s;
    let dc = cat_sd(q) * s;
    let a = 1.0 - smoothstep(-px * 0.6, px * 0.6, dc);
    col = mix(col, u.cat.rgb, a);

    return vec4f(col, 1.0);
}

@fragment
fn fs_blit(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let g = vec2i(floor((frag.xy - u.rect.xy) / max(u.a.z, 1.0)));
    let dims = vec2i(textureDimensions(img));
    var col = textureLoad(img, clamp(g, vec2i(0), dims - vec2i(1)), 0).rgb;
    if (u.a.w > 0.5) { col = to_srgb(col); }
    // The slot's corners are the card's: outside a rounded rectangle of radius b.w (device pixels) nothing is drawn.
    let half = u.rect.zw * 0.5;
    let q = abs(frag.xy - (u.rect.xy + half)) - (half - vec2f(u.b.w));
    let d = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - u.b.w;
    let a = clamp(0.5 - d, 0.0, 1.0);
    return vec4f(col * a, a);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    rect: [f32; 4],
    a: [f32; 4],
    bg: [f32; 4],
    ink: [f32; 4],
    cat: [f32; 4],
    b: [f32; 4],
}

/// What a frame of the art is drawn with.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// Seconds on the art's own clock (it stops while the art is paused).
    pub time: f32,
    pub dark: bool,
    /// Device pixels per point: the dither cell is one point.
    pub scale: f32,
    /// The window's colour and Iris, linear.
    pub bg: [f32; 3],
    pub ink: [f32; 3],
    /// The cat's colour, linear.
    pub cat: [f32; 3],
    /// Benchmark only: draw nothing, to measure what an empty pass of this size costs.
    pub floor: bool,
}

impl Look {
    fn uniforms(&self, rect: [f32; 4], srgb_blend: bool) -> Uniforms {
        // Dark: Iris dots at 80% on the near-black window. Light (dawn): Iris at 35% on the pale ground, and
        // fewer dots, so the page stays quiet.
        let (strength, density) = if self.dark { (0.80, 1.0) } else { (0.35, 0.85) };
        Uniforms {
            rect,
            a: [self.time, if self.dark { 1.0 } else { 0.0 }, self.scale.round().max(1.0), if srgb_blend { 1.0 } else { 0.0 }],
            bg: [self.bg[0], self.bg[1], self.bg[2], 1.0],
            ink: [self.ink[0], self.ink[1], self.ink[2], 1.0],
            cat: [self.cat[0], self.cat[1], self.cat[2], 1.0],
            b: [strength, density, if self.floor { 1.0 } else { 0.0 }, CORNER_POINTS * self.scale],
        }
    }
}

/// The card's corner radius in points (the window radius): the art is clipped to it.
pub const CORNER_POINTS: f32 = 20.0;

/// The dot strength the shader uses over the window's colour, so the legibility test can name the worst case.
pub fn dot_strength(dark: bool) -> f32 {
    if dark { 0.80 } else { 0.35 }
}

const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Target {
    size: (u32, u32),
    view: wgpu::TextureView,
    blit_bg: wgpu::BindGroup,
}

/// The art's GPU side: the scene pass into a small target (one texel per dither cell), and the blit that
/// expands it into the slot of the window's own pass.
pub struct HomeArt {
    shader: wgpu::ShaderModule,
    blit_bgl: wgpu::BindGroupLayout,
    scene_u: wgpu::Buffer,
    blit_u: wgpu::Buffer,
    scene_bg: wgpu::BindGroup,
    scene_pipeline: wgpu::RenderPipeline,
    blit_pipeline: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    target: Option<Target>,
}

impl HomeArt {
    pub fn new(device: &wgpu::Device) -> HomeArt {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("home art"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("home art scene"), entries: &[uniform_entry] });
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("home art blit"),
            entries: &[
                uniform_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let buffer = |label: &str| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: bytemuck::bytes_of(&Uniforms::zeroed()), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });
        let (scene_u, blit_u) = (buffer("home art scene u"), buffer("home art blit u"));
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("home art scene"), layout: &scene_bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: scene_u.as_entire_binding() }] });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("home art scene"), bind_group_layouts: &[Some(&scene_bgl)], immediate_size: 0 });
        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("home art scene"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: SCENE_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            multiview_mask: None,
            cache: None,
        });
        HomeArt { shader, blit_bgl, scene_u, blit_u, scene_bg, scene_pipeline, blit_pipeline: None, target: None }
    }

    fn blit_pipeline_for(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if self.blit_pipeline.as_ref().is_none_or(|(f, _)| *f != format) {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("home art blit"), bind_group_layouts: &[Some(&self.blit_bgl)], immediate_size: 0 });
            let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("home art blit"),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &self.shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &self.shader, entry_point: Some("fs_blit"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
                multiview_mask: None,
                cache: None,
            });
            self.blit_pipeline = Some((format, p));
        }
        self.blit_pipeline.as_ref().expect("pipeline").1.clone()
    }

    fn ensure_target(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("home art"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let blit_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("home art blit"),
            layout: &self.blit_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: self.blit_u.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        self.target = Some(Target { size, view, blit_bg });
    }

    /// The scene pass: draws one texel per dither cell of a `slot`-sized (device pixels) picture into the small
    /// target, in the frame's encoder, before the window's own pass.
    fn scene_pass(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, slot: (u32, u32), look: &Look, timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>) {
        let cell = look.scale.round().max(1.0);
        let dims = ((((slot.0 as f32) / cell).ceil() as u32).max(1), (((slot.1 as f32) / cell).ceil() as u32).max(1));
        self.ensure_target(device, dims);
        queue.write_buffer(&self.scene_u, 0, bytemuck::bytes_of(&look.uniforms([0.0, 0.0, slot.0 as f32, slot.1 as f32], false)));
        let Some(t) = self.target.as_ref() else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("home art scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &t.view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.scene_pipeline);
        pass.set_bind_group(0, &self.scene_bg, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Draws the scene into its small target (call before the window's pass).
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, slot: (u32, u32), look: &Look) {
        self.scene_pass(device, queue, encoder, slot, look, None);
    }

    /// Expands the scene into the slot's rectangle of the open UI pass.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'_>, site: &EffectSite<'_>, look: &Look) {
        let Some(bg) = self.target.as_ref().map(|t| t.blit_bg.clone()) else { return };
        let r = site.instance.rect;
        let (x, y) = (r[0].max(0.0), r[1].max(0.0));
        let w = r[2].min(site.size[0] as f32 - x).max(1.0);
        let h = r[3].min(site.size[1] as f32 - y).max(1.0);
        // The texel under a pixel counts from the slot's own corner, which a clipped viewport may have moved.
        queue.write_buffer(&self.blit_u, 0, bytemuck::bytes_of(&look.uniforms([r[0], r[1], r[2], r[3]], site.srgb_blend)));
        let pipeline = self.blit_pipeline_for(device, site.format);
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    /// GPU milliseconds per frame of the art over a `size` target: the scene pass and the blit, as the window
    /// runs them, timed together with timestamp queries over `frames` frames. None without timestamp queries.
    pub fn bench(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32), frames: u32, look: &Look) -> Option<Vec<f64>> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("home art bench"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let qs = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("home art ts"), ty: wgpu::QueryType::Timestamp, count: frames * 2 });
        let bytes = u64::from(frames) * 2 * 8;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: Some("home art resolve"), size: bytes, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let read = device.create_buffer(&wgpu::BufferDescriptor { label: Some("home art read"), size: bytes, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let blit = self.blit_pipeline_for(device, format);
        for i in 0..frames {
            // Every frame its own time, as the window's would be; the writes land before this frame's submit.
            let look = Look { time: look.time + i as f32 / 60.0, ..*look };
            let mut encoder = device.create_command_encoder(&Default::default());
            self.scene_pass(device, queue, &mut encoder, size, &look, Some(wgpu::RenderPassTimestampWrites { query_set: &qs, beginning_of_pass_write_index: Some(i * 2), end_of_pass_write_index: None }));
            queue.write_buffer(&self.blit_u, 0, bytemuck::bytes_of(&look.uniforms([0.0, 0.0, size.0 as f32, size.1 as f32], false)));
            if let Some(t) = self.target.as_ref() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("home art bench blit"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: None,
                    timestamp_writes: Some(wgpu::RenderPassTimestampWrites { query_set: &qs, beginning_of_pass_write_index: None, end_of_pass_write_index: Some(i * 2 + 1) }),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&blit);
                pass.set_bind_group(0, &t.blit_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            queue.submit(Some(encoder.finish()));
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.resolve_query_set(&qs, 0..frames * 2, &resolve, 0);
        encoder.copy_buffer_to_buffer(&resolve, 0, &read, 0, bytes);
        queue.submit(Some(encoder.finish()));
        let slice = read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("mapped");
        let ticks: &[u64] = bytemuck::cast_slice(&data);
        let period = f64::from(queue.get_timestamp_period());
        let out = ticks.chunks_exact(2).map(|c| (c[0], c[1])).filter(|p| p.1 > p.0).map(|p| (p.1 - p.0) as f64 * period / 1e6).collect();
        drop(data);
        read.unmap();
        Some(out)
    }
}

use bytemuck::Zeroable;
