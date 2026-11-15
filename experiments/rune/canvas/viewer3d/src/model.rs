//! glTF/GLB loading with the `gltf` crate (MIT or Apache-2.0): every mesh primitive of the default
//! scene, node transforms applied, base-colour factor baked into the vertices, plus a de-duplicated
//! edge list for the wireframe.

use std::collections::HashSet;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nor: [f32; 3],
    pub col: [f32; 3],
}

pub struct Model {
    pub verts: Vec<Vertex>,
    pub tris: Vec<u32>,
    pub edges: Vec<u32>,
    pub center: [f32; 3],
    pub radius: f32,
}

type M4 = [[f32; 4]; 4];

fn xform_point(m: &M4, p: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0; 3];
    for (r, out) in o.iter_mut().enumerate() {
        *out = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    }
    o
}

fn xform_dir(m: &M4, p: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0; 3];
    for (r, out) in o.iter_mut().enumerate() {
        *out = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2];
    }
    let l = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt().max(1e-9);
    [o[0] / l, o[1] / l, o[2] / l]
}

fn mul(a: &M4, b: &M4) -> M4 {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

fn walk(node: gltf::Node, parent: &M4, buffers: &[gltf::buffer::Data], out: &mut Model) {
    let world = mul(parent, &node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(|b| Some(&buffers[b.index()]));
            let Some(pos) = reader.read_positions() else { continue };
            let pos: Vec<[f32; 3]> = pos.collect();
            let nor: Vec<[f32; 3]> = match reader.read_normals() {
                Some(n) => n.collect(),
                None => vec![[0.0, 0.0, 1.0]; pos.len()],
            };
            let rgb = prim.material().pbr_metallic_roughness().base_color_factor();
            let base = out.verts.len() as u32;
            for (p, n) in pos.iter().zip(&nor) {
                out.verts.push(Vertex { pos: xform_point(&world, *p), nor: xform_dir(&world, *n), col: [rgb[0], rgb[1], rgb[2]] });
            }
            match reader.read_indices() {
                Some(ix) => out.tris.extend(ix.into_u32().map(|i| base + i)),
                None => out.tris.extend((0..pos.len() as u32).map(|i| base + i)),
            }
        }
    }
    for child in node.children() {
        walk(child, &world, buffers, out);
    }
}

pub fn load(path: &str) -> Result<Model, String> {
    let (doc, buffers, _images) = gltf::import(path).map_err(|e| e.to_string())?;
    let mut m = Model { verts: vec![], tris: vec![], edges: vec![], center: [0.0; 3], radius: 1.0 };
    let identity: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("no scene")?;
    for node in scene.nodes() {
        walk(node, &identity, &buffers, &mut m);
    }
    if m.verts.is_empty() {
        return Err("no triangles".into());
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &m.verts {
        for k in 0..3 {
            lo[k] = lo[k].min(v.pos[k]);
            hi[k] = hi[k].max(v.pos[k]);
        }
    }
    m.center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    m.radius = m.verts.iter().map(|v| {
        let d = [v.pos[0] - m.center[0], v.pos[1] - m.center[1], v.pos[2] - m.center[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }).fold(0.0, f32::max);
    let mut seen = HashSet::with_capacity(m.tris.len());
    for t in m.tris.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if seen.insert((a.min(b), a.max(b))) {
                m.edges.extend([a, b]);
            }
        }
    }
    Ok(m)
}
