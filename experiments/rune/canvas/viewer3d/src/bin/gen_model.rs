//! Writes the test model: a (3,2) torus knot (~48k triangles) with a red material and a grey
//! ground disc on its own node (translated). Generated here, so there is nothing to attribute:
//! CC0 / public domain, like the rest of this spike's own code is MIT.
//!
//! `cargo run -p viewer3d --bin gen-model -- assets/knot.glb [rings] [segments]`

use std::f32::consts::TAU;
use std::fmt::Write as _;

struct Mesh {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    idx: Vec<u32>,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn knot_point(t: f32) -> [f32; 3] {
    // (p, q) = (2, 3) torus knot.
    let (p, q) = (2.0f32, 3.0f32);
    let r = 2.0 + (q * t).cos();
    [r * (p * t).cos(), r * (p * t).sin(), -(q * t).sin()]
}

fn torus_knot(rings: u32, segs: u32, tube: f32) -> Mesh {
    let mut m = Mesh { pos: vec![], nor: vec![], idx: vec![] };
    for i in 0..=rings {
        let t = i as f32 / rings as f32 * TAU;
        let c = knot_point(t);
        let ahead = knot_point(t + 0.01);
        let tangent = norm(sub(ahead, c));
        let mut n = norm(cross(tangent, [0.0, 0.0, 1.0]));
        if n[0].is_nan() { n = [1.0, 0.0, 0.0]; }
        let b = norm(cross(tangent, n));
        for j in 0..=segs {
            let a = j as f32 / segs as f32 * TAU;
            let dir = [n[0] * a.cos() + b[0] * a.sin(), n[1] * a.cos() + b[1] * a.sin(), n[2] * a.cos() + b[2] * a.sin()];
            m.pos.push([c[0] + dir[0] * tube, c[1] + dir[1] * tube, c[2] + dir[2] * tube]);
            m.nor.push(dir);
        }
    }
    let stride = segs + 1;
    for i in 0..rings {
        for j in 0..segs {
            let a = i * stride + j;
            let (b, c, d) = (a + stride, a + 1, a + stride + 1);
            m.idx.extend([a, b, c, c, b, d]);
        }
    }
    m
}

fn disc(segs: u32) -> Mesh {
    let mut m = Mesh { pos: vec![[0.0, 0.0, 0.0]], nor: vec![[0.0, 0.0, 1.0]], idx: vec![] };
    for i in 0..segs {
        let a = i as f32 / segs as f32 * TAU;
        m.pos.push([a.cos() * 5.0, a.sin() * 5.0, 0.0]);
        m.nor.push([0.0, 0.0, 1.0]);
    }
    for i in 0..segs {
        m.idx.extend([0, 1 + i, 1 + (i + 1) % segs]);
    }
    m
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "assets/knot.glb".into());
    let rings: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(400);
    let segs: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let meshes = [torus_knot(rings, segs, 0.55), disc(64)];

    let mut bin: Vec<u8> = Vec::new();
    let mut views = String::new();
    let mut accessors = String::new();
    let mut prims = String::new();
    let mut tris = 0usize;
    for (mi, m) in meshes.iter().enumerate() {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &m.pos {
            for k in 0..3 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); }
        }
        let push = |bin: &mut Vec<u8>, bytes: &[u8]| -> (usize, usize) {
            while bin.len() % 4 != 0 { bin.push(0); }
            let at = bin.len();
            bin.extend_from_slice(bytes);
            (at, bytes.len())
        };
        let pos_bytes: Vec<u8> = m.pos.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let nor_bytes: Vec<u8> = m.nor.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let idx_bytes: Vec<u8> = m.idx.iter().flat_map(|f| f.to_le_bytes()).collect();
        let (po, pl) = push(&mut bin, &pos_bytes);
        let (no, nl) = push(&mut bin, &nor_bytes);
        let (io, il) = push(&mut bin, &idx_bytes);
        let v0 = mi * 3;
        write!(views, "{{\"buffer\":0,\"byteOffset\":{po},\"byteLength\":{pl},\"target\":34962}},{{\"buffer\":0,\"byteOffset\":{no},\"byteLength\":{nl},\"target\":34962}},{{\"buffer\":0,\"byteOffset\":{io},\"byteLength\":{il},\"target\":34963}},").unwrap();
        write!(accessors,
            "{{\"bufferView\":{},\"componentType\":5126,\"count\":{},\"type\":\"VEC3\",\"min\":[{},{},{}],\"max\":[{},{},{}]}},\
             {{\"bufferView\":{},\"componentType\":5126,\"count\":{},\"type\":\"VEC3\"}},\
             {{\"bufferView\":{},\"componentType\":5125,\"count\":{},\"type\":\"SCALAR\"}},",
            v0, m.pos.len(), lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], v0 + 1, m.nor.len(), v0 + 2, m.idx.len()).unwrap();
        write!(prims, "{{\"primitives\":[{{\"attributes\":{{\"POSITION\":{},\"NORMAL\":{}}},\"indices\":{},\"material\":{mi}}}]}},", v0, v0 + 1, v0 + 2).unwrap();
        tris += m.idx.len() / 3;
    }
    while bin.len() % 4 != 0 { bin.push(0); }
    let trim = |s: &mut String| { s.pop(); };
    trim(&mut views); trim(&mut accessors); trim(&mut prims);
    let json = format!(
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"neko rune spike gen-model\"}},\"scene\":0,\"scenes\":[{{\"nodes\":[0,1]}}],\
         \"nodes\":[{{\"mesh\":0,\"name\":\"knot\"}},{{\"mesh\":1,\"name\":\"ground\",\"translation\":[0,0,-2.2]}}],\
         \"materials\":[{{\"pbrMetallicRoughness\":{{\"baseColorFactor\":[0.86,0.24,0.2,1.0],\"metallicFactor\":0.0,\"roughnessFactor\":0.6}}}},\
         {{\"pbrMetallicRoughness\":{{\"baseColorFactor\":[0.55,0.58,0.62,1.0],\"metallicFactor\":0.0,\"roughnessFactor\":0.9}}}}],\
         \"meshes\":[{prims}],\"accessors\":[{accessors}],\"bufferViews\":[{views}],\"buffers\":[{{\"byteLength\":{}}}]}}",
        bin.len());
    let mut jb = json.into_bytes();
    while jb.len() % 4 != 0 { jb.push(b' '); }
    let total = 12 + 8 + jb.len() + 8 + bin.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend(b"glTF");
    glb.extend(2u32.to_le_bytes());
    glb.extend((total as u32).to_le_bytes());
    glb.extend((jb.len() as u32).to_le_bytes());
    glb.extend(b"JSON");
    glb.extend(&jb);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(b"BIN\0");
    glb.extend(&bin);
    std::fs::write(&out, &glb).expect("write glb");
    eprintln!("wrote {out}: {} bytes, {tris} triangles", glb.len());
}
