//! Small sample files made by code, for the scenes, the snapshots and the tests of the context panel's
//! viewers: one of every built-in kind. Each is generated here (nothing is copied from anywhere, so
//! there is nothing to attribute and no licence to carry); they return bytes and never touch a disk.

use std::f32::consts::TAU;
use std::io::Cursor;

/// A `w` by `h` gradient PNG.
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 150, 255]));
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).expect("a PNG encodes in memory");
    out
}

/// An animated GIF of `n` frames: a square that moves across a dark ground, a tenth of a second a frame.
pub fn gif(n: u8) -> Vec<u8> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut out);
        enc.set_repeat(Repeat::Infinite).expect("repeat");
        for i in 0..n {
            let x0 = 8 + u32::from(i) * (72 / u32::from(n.max(1)));
            let rgba = image::RgbaImage::from_fn(96, 64, |x, y| if (x0..x0 + 16).contains(&x) && (24..40).contains(&y) { image::Rgba([240, 160, 60, 255]) } else { image::Rgba([30, 34, 48, 255]) });
            let frame = image::Frame::from_parts(rgba, 0, 0, image::Delay::from_numer_denom_ms(100, 1));
            enc.encode_frame(frame).expect("a GIF frame");
        }
    }
    out
}

pub fn svg() -> Vec<u8> {
    br##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="200" viewBox="0 0 320 200"><rect width="320" height="200" rx="16" fill="#1d2433"/><circle cx="110" cy="100" r="52" fill="#4cc38a"/><circle cx="190" cy="100" r="52" fill="#346bf1" fill-opacity="0.8"/><path d="M60 170 L260 170" stroke="#e0a042" stroke-width="6" stroke-linecap="round"/></svg>"##.to_vec()
}

pub fn markdown() -> String {
    "# Release notes\n\nThe **export** now takes a lock, so two runs never write the same file.\n\n- Fixed the retry loop\n- Added a `--dry-run` flag\n- Removed the old cache\n\n| step | time |\n| --- | --- |\n| build | 12 s |\n| test | 41 s |\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n".to_owned()
}

pub fn code() -> String {
    "use std::collections::HashMap;\n\n/// Counts words, case-blind.\npub fn count(text: &str) -> HashMap<String, usize> {\n    let mut out = HashMap::new();\n    for word in text.split_whitespace() {\n        *out.entry(word.to_lowercase()).or_insert(0) += 1;\n    }\n    out\n}\n\n#[test]\nfn counts() {\n    assert_eq!(count(\"A a b\")[\"a\"], 2);\n}\n".to_owned()
}

/// A CSV of `rows` rows under a header: an id, a name, an amount and a date.
pub fn csv(rows: usize) -> String {
    let mut out = String::from("id,name,amount,when\n");
    for i in 0..rows {
        out.push_str(&format!("{i},user-{i},{}.{:02},2026-11-{:02}\n", i % 977, i % 100, 1 + i % 28));
    }
    out
}

/// A small valid PDF of `n` pages, each with its number in large type and a blue block (the offsets
/// are written as the cross-reference table needs them).
pub fn pdf(n: usize) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let mut add = |out: &mut Vec<u8>, body: String| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", offsets.len()).as_bytes());
    };
    add(&mut out, "<< /Type /Catalog /Pages 2 0 R >>".to_owned());
    let kids: String = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect::<Vec<_>>().join(" ");
    add(&mut out, format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>"));
    add(&mut out, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned());
    for i in 0..n {
        let stream = format!("BT /F1 48 Tf 72 700 Td (Page {}) Tj ET\n0.2 0.4 0.9 rg 72 500 300 120 re f\nBT /F1 14 Tf 72 460 Td (A made-up page for the viewer.) Tj ET\n", i + 1);
        add(&mut out, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", 5 + 2 * i));
        add(&mut out, format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()));
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    out
}

/// A unit cube as an ASCII STL, 12 triangles.
pub fn stl() -> Vec<u8> {
    let v = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]];
    let f = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    let mut s = String::from("solid cube\n");
    for t in f {
        s.push_str("facet normal 0 0 0\n outer loop\n");
        for i in t {
            s.push_str(&format!("  vertex {} {} {}\n", v[i][0], v[i][1], v[i][2]));
        }
        s.push_str(" endloop\nendfacet\n");
    }
    s.push_str("endsolid cube\n");
    s.into_bytes()
}

/// A quad as an OBJ.
pub fn obj() -> Vec<u8> {
    b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n".to_vec()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// A (2, 3) torus knot as a binary glTF (`.glb`): `rings` by `segments` quads of two triangles each,
/// a coral material, Y-up as glTF is. `glb(160, 24)` is about 7,700 triangles.
pub fn glb(rings: u32, segs: u32) -> Vec<u8> {
    let knot = |t: f32| {
        let r = 2.0 + (3.0 * t).cos();
        [r * (2.0 * t).cos(), r * (2.0 * t).sin(), -(3.0 * t).sin()]
    };
    let (mut pos, mut nor): (Vec<[f32; 3]>, Vec<[f32; 3]>) = (Vec::new(), Vec::new());
    for i in 0..=rings {
        let t = i as f32 / rings as f32 * TAU;
        let c = knot(t);
        let tangent = norm(sub(knot(t + 0.01), c));
        let mut n = norm(cross(tangent, [0.0, 0.0, 1.0]));
        if n[0].is_nan() {
            n = [1.0, 0.0, 0.0];
        }
        let b = norm(cross(tangent, n));
        for j in 0..=segs {
            let a = j as f32 / segs as f32 * TAU;
            let dir = [n[0] * a.cos() + b[0] * a.sin(), n[1] * a.cos() + b[1] * a.sin(), n[2] * a.cos() + b[2] * a.sin()];
            // The knot is made Z-up; glTF is Y-up: (x, y, z) becomes (x, z, -y).
            let up = |p: [f32; 3]| [p[0], p[2], -p[1]];
            pos.push(up([c[0] + dir[0] * 0.45, c[1] + dir[1] * 0.45, c[2] + dir[2] * 0.45]));
            nor.push(up(dir));
        }
    }
    let stride = segs + 1;
    let mut idx: Vec<u32> = Vec::new();
    for i in 0..rings {
        for j in 0..segs {
            let a = i * stride + j;
            idx.extend([a, a + stride, a + 1, a + 1, a + stride, a + stride + 1]);
        }
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let mut bin: Vec<u8> = Vec::new();
    for p in &pos {
        bin.extend(p.iter().flat_map(|f| f.to_le_bytes()));
    }
    let nor_at = bin.len();
    for n in &nor {
        bin.extend(n.iter().flat_map(|f| f.to_le_bytes()));
    }
    let idx_at = bin.len();
    bin.extend(idx.iter().flat_map(|i| i.to_le_bytes()));
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let json = format!(
        concat!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"#,
            r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2,"material":0}}]}}],"#,
            r#""materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.92,0.38,0.32,1.0],"metallicFactor":0.0}}}}],"#,
            r#""buffers":[{{"byteLength":{blen}}}],"#,
            r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{nlen},"target":34962}},{{"buffer":0,"byteOffset":{nat},"byteLength":{nlen},"target":34962}},{{"buffer":0,"byteOffset":{iat},"byteLength":{ilen},"target":34963}}],"#,
            r#""accessors":[{{"bufferView":0,"componentType":5126,"count":{count},"type":"VEC3","min":[{lx},{ly},{lz}],"max":[{hx},{hy},{hz}]}},"#,
            r#"{{"bufferView":1,"componentType":5126,"count":{count},"type":"VEC3"}},"#,
            r#"{{"bufferView":2,"componentType":5125,"count":{icount},"type":"SCALAR"}}]}}"#
        ),
        blen = bin.len(),
        nlen = pos.len() * 12,
        nat = nor_at,
        iat = idx_at,
        ilen = idx.len() * 4,
        count = pos.len(),
        icount = idx.len(),
        lx = lo[0],
        ly = lo[1],
        lz = lo[2],
        hx = hi[0],
        hy = hi[1],
        hz = hi[2],
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(&bin);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sample_is_the_kind_it_says() {
        assert!(png(8, 8).starts_with(b"\x89PNG"));
        assert!(gif(3).starts_with(b"GIF89a"));
        assert!(svg().starts_with(b"<svg"));
        assert!(pdf(2).starts_with(b"%PDF-"));
        assert!(glb(16, 8).starts_with(b"glTF"));
        assert_eq!(csv(3).lines().count(), 4);
        assert_eq!(crate::viewers::model3d::load("glb", &glb(40, 10)).map(|m| m.triangles()), Ok(40 * 10 * 2));
        assert_eq!(crate::viewers::model3d::load("stl", &stl()).map(|m| m.triangles()), Ok(12));
        assert_eq!(crate::viewers::model3d::load("obj", &obj()).map(|m| m.triangles()), Ok(2));
    }
}
