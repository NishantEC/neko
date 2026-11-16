//! The 3D viewer: `glb` and `gltf` (`gltf`), `stl` (`stl_io`) and `obj` (`tobj`) load into one mesh
//! (positions, normals, a base colour, and a de-duplicated edge list for the wireframe). It is
//! drawn on Rune's own wgpu device by `model3d_gpu`, inside the frame, at the element's effect
//! slot; this file builds that element and holds the model, the camera maths and the camera's
//! gestures: drag orbits, scroll zooms, a double press fits the model, W toggles the wireframe.
//!
//! The canvas spike's compositor pass is the base (docs/knowledge/rune-p3a-the-context-panel.md).
//! The world is Z-up: glTF and OBJ are Y-up files, turned on import; STL is Z-up already.

use std::collections::HashSet;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Mat4, Vec3};
use rune::{El, Phase, col, el, text};

use super::{Camera, ViewerInput, ViewerKey, ViewerState};
use crate::app::App;
use crate::theme::theme;

/// The vertical field of view, in radians.
pub const FOV_Y: f32 = 40.0 * std::f32::consts::PI / 180.0;
/// The key of the element the model is drawn into.
pub const VIEW_KEY: &str = "model3d-view";

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nor: [f32; 3],
    pub col: [f32; 3],
}

/// A model ready to draw.
#[derive(Debug)]
pub struct Mesh {
    /// Unique per mesh made: the renderer knows a new model by it.
    pub id: u64,
    pub verts: Vec<Vertex>,
    pub tris: Vec<u32>,
    pub edges: Vec<u32>,
    pub center: [f32; 3],
    /// Half the bounding box's diagonal.
    pub radius: f32,
    /// The lowest point: the grid floor sits here.
    pub floor: f32,
}

impl Mesh {
    pub fn triangles(&self) -> usize {
        self.tris.len() / 3
    }

    /// How much memory the mesh holds, for a measurement.
    pub fn held(&self) -> usize {
        self.verts.len() * std::mem::size_of::<Vertex>() + (self.tris.len() + self.edges.len()) * 4
    }
}

/// A neutral base colour for formats that carry none.
const BASE: [f32; 3] = [0.72, 0.74, 0.78];

fn y_up_to_z_up(v: [f32; 3]) -> [f32; 3] {
    [v[0], -v[2], v[1]]
}

fn finish(mut verts: Vec<Vertex>, tris: Vec<u32>) -> Result<Mesh, String> {
    if verts.is_empty() || tris.len() < 3 {
        return Err("This model has no triangles.".to_owned());
    }
    // Indices that point past the vertices would draw garbage: refuse the file.
    if tris.iter().any(|i| *i as usize >= verts.len()) {
        return Err("This model has triangles that point past its points.".to_owned());
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &verts {
        for k in 0..3 {
            lo[k] = lo[k].min(v.pos[k]);
            hi[k] = hi[k].max(v.pos[k]);
        }
    }
    if lo.iter().chain(&hi).any(|c| !c.is_finite()) {
        return Err("This model has points that are not numbers.".to_owned());
    }
    let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let radius = (Vec3::from(hi) - Vec3::from(lo)).length() / 2.0;
    for v in &mut verts {
        if !v.nor.iter().all(|c| c.is_finite()) || Vec3::from(v.nor).length_squared() < 1e-12 {
            v.nor = [0.0, 0.0, 1.0];
        }
    }
    // An edge is a pair of places, not of vertices: a flat-shaded model has a vertex per corner of each
    // triangle, and its neighbours' shared edge is one line.
    let place = |i: u32| verts[i as usize].pos.map(f32::to_bits);
    let mut seen = HashSet::with_capacity(tris.len());
    let mut edges = Vec::new();
    for t in tris.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (pa, pb) = (place(a), place(b));
            if seen.insert(if pa <= pb { (pa, pb) } else { (pb, pa) }) {
                edges.extend([a, b]);
            }
        }
    }
    static NEXT: AtomicU64 = AtomicU64::new(1);
    Ok(Mesh { id: NEXT.fetch_add(1, Ordering::Relaxed), verts, tris, edges, center, radius: radius.max(1e-6), floor: lo[2] })
}

/// Normals for a mesh that came without: each triangle's face normal added to its points.
fn smooth_normals(verts: &mut [Vertex], tris: &[u32]) {
    let mut sum = vec![Vec3::ZERO; verts.len()];
    for t in tris.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if a.max(b).max(c) >= verts.len() {
            continue;
        }
        let n = (Vec3::from(verts[b].pos) - Vec3::from(verts[a].pos)).cross(Vec3::from(verts[c].pos) - Vec3::from(verts[a].pos));
        sum[a] += n;
        sum[b] += n;
        sum[c] += n;
    }
    for (v, n) in verts.iter_mut().zip(sum) {
        v.nor = n.normalize_or_zero().to_array();
    }
}

fn walk(node: gltf::Node, parent: Mat4, buffers: &[gltf::buffer::Data], verts: &mut Vec<Vertex>, tris: &mut Vec<u32>) {
    let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(pos) = reader.read_positions() else { continue };
            let pos: Vec<[f32; 3]> = pos.collect();
            let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(|n| n.collect());
            let rgb = prim.material().pbr_metallic_roughness().base_color_factor();
            let base = verts.len() as u32;
            let normal_matrix = world.inverse().transpose();
            let start = verts.len();
            for (i, p) in pos.iter().enumerate() {
                let p = y_up_to_z_up(world.transform_point3(Vec3::from(*p)).to_array());
                let n = normals.as_ref().and_then(|n| n.get(i)).map_or([0.0; 3], |n| y_up_to_z_up(normal_matrix.transform_vector3(Vec3::from(*n)).normalize_or_zero().to_array()));
                verts.push(Vertex { pos: p, nor: n, col: [rgb[0], rgb[1], rgb[2]] });
            }
            let first = tris.len();
            match reader.read_indices() {
                Some(ix) => tris.extend(ix.into_u32().map(|i| base + i)),
                None => tris.extend((0..pos.len() as u32).map(|i| base + i)),
            }
            if normals.is_none() {
                // Only this primitive's triangles and points: indices are global, so shift to local.
                let local: Vec<u32> = tris[first..].iter().map(|i| i - base).collect();
                smooth_normals(&mut verts[start..], &local);
            }
        }
    }
    for child in node.children() {
        walk(child, world, buffers, verts, tris);
    }
}

pub fn load_gltf(bytes: &[u8]) -> Result<Mesh, String> {
    let (doc, buffers, _images) = gltf::import_slice(bytes).map_err(|e| format!("This model couldn’t be read ({e})."))?;
    let (mut verts, mut tris) = (Vec::new(), Vec::new());
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("This model has no scene.")?;
    for node in scene.nodes() {
        walk(node, Mat4::IDENTITY, &buffers, &mut verts, &mut tris);
    }
    finish(verts, tris)
}

pub fn load_stl(bytes: &[u8]) -> Result<Mesh, String> {
    let stl = stl_io::read_stl(&mut Cursor::new(bytes)).map_err(|e| format!("This model couldn’t be read ({e})."))?;
    let (mut verts, mut tris) = (Vec::with_capacity(stl.faces.len() * 3), Vec::with_capacity(stl.faces.len() * 3));
    for face in &stl.faces {
        let p: Vec<[f32; 3]> = face.vertices.iter().map(|i| stl.vertices.get(*i).map_or([0.0; 3], |v| [v[0], v[1], v[2]])).collect();
        let n = Vec3::from(p[1]) - Vec3::from(p[0]);
        let n = n.cross(Vec3::from(p[2]) - Vec3::from(p[0])).normalize_or_zero().to_array();
        for pos in p {
            tris.push(verts.len() as u32);
            verts.push(Vertex { pos, nor: n, col: BASE });
        }
    }
    finish(verts, tris)
}

pub fn load_obj(bytes: &[u8]) -> Result<Mesh, String> {
    let options = tobj::LoadOptions { triangulate: true, single_index: true, ..tobj::LoadOptions::default() };
    // No material files: a model's own colours are not read, only its shape.
    let (models, _) = tobj::load_obj_buf(&mut Cursor::new(bytes), &options, |_| Err(tobj::LoadError::OpenFileFailed)).map_err(|e| format!("This model couldn’t be read ({e})."))?;
    let (mut verts, mut tris) = (Vec::new(), Vec::new());
    for model in models {
        let m = model.mesh;
        let base = verts.len() as u32;
        let start = verts.len();
        for (i, p) in m.positions.chunks_exact(3).enumerate() {
            let n = m.normals.get(i * 3..i * 3 + 3).map_or([0.0; 3], |n| y_up_to_z_up([n[0], n[1], n[2]]));
            verts.push(Vertex { pos: y_up_to_z_up([p[0], p[1], p[2]]), nor: n, col: BASE });
        }
        let first = tris.len();
        tris.extend(m.indices.iter().map(|i| base + i));
        if m.normals.is_empty() {
            let local: Vec<u32> = tris[first..].iter().map(|i| i - base).collect();
            smooth_normals(&mut verts[start..], &local);
        }
    }
    finish(verts, tris)
}

/// Loads a model by its kind.
pub fn load(kind: &str, bytes: &[u8]) -> Result<Mesh, String> {
    match kind.to_ascii_lowercase().as_str() {
        "glb" | "gltf" => load_gltf(bytes),
        "stl" => load_stl(bytes),
        "obj" => load_obj(bytes),
        other => Err(format!("{other} isn’t a 3D format.")),
    }
}

/// What the viewer keeps of a model: the mesh, or why it did not load.
pub enum Loaded {
    Mesh(Rc<Mesh>),
    Failed(String),
}

/// The model-view-projection matrix for a camera looking at `mesh` in a viewport of `aspect`.
pub fn mvp(mesh: &Mesh, cam: &Camera, aspect: f32) -> ([[f32; 4]; 4], [[f32; 4]; 4]) {
    let half_x = (FOV_Y / 2.0).tan() * aspect;
    let half = (FOV_Y / 2.0).tan().min(half_x);
    let fit = mesh.radius / half.atan().sin();
    let dist = fit * 1.08 / cam.zoom.max(0.01);
    let (cy, sy, cp, sp) = (cam.yaw.cos(), cam.yaw.sin(), cam.pitch.cos(), cam.pitch.sin());
    let center = Vec3::from(mesh.center);
    let eye = center + Vec3::new(dist * cp * cy, dist * cp * sy, dist * sp);
    let view = Mat4::look_at_rh(eye, center, Vec3::Z);
    let near = (dist - mesh.radius * 1.5).max(dist * 0.01).max(0.01);
    let proj = Mat4::perspective_rh(FOV_Y, aspect, near, dist + mesh.radius * 3.0);
    ((proj * view).to_cols_array_2d(), view.to_cols_array_2d())
}

/// Drag orbits: from where the drag began, `dx` and `dy` points turn the model.
pub fn orbit(cam: &mut Camera, dx: f32, dy: f32) {
    cam.yaw = cam.drag_from.0 - dx * 0.01;
    cam.pitch = (cam.drag_from.1 + dy * 0.01).clamp(-1.5, 1.5);
}

/// Scroll zooms: `dy` points of wheel, smoothly, within 0.2x and 30x.
pub fn zoom(cam: &mut Camera, dy: f32) {
    cam.zoom = (cam.zoom * (dy * 0.004).exp()).clamp(0.2, 30.0);
}

/// Double press: back to the fitted view, keeping the wireframe switch.
pub fn fit(cam: &mut Camera) {
    *cam = Camera { wire: cam.wire, ..Camera::default() };
}

pub fn build(input: &ViewerInput, state: &mut ViewerState) -> El {
    let t = theme(input.theme_dark);
    let (kind, bytes) = (input.kind.to_owned(), input.bytes.unwrap_or(&[]));
    let loaded = state.data(|| match load(&kind, bytes) {
        Ok(mesh) => Loaded::Mesh(Rc::new(mesh)),
        Err(e) => Loaded::Failed(e),
    });
    let mesh = match loaded {
        Loaded::Mesh(m) => m.clone(),
        Loaded::Failed(e) => return col().key("viewer-model3d").w_full().grow(1.0).min_h(0.0).center().px(24.0).child(text(e.clone()).font(t.ty.meta).color(t.text.muted).wrap()),
    };
    let key: ViewerKey = state.key.clone();
    let (k1, k2, k3) = (key.clone(), key.clone(), key.clone());
    let wire = state.camera.wire;
    let view = el()
        .key(VIEW_KEY)
        .role("img")
        .label(format!("{}: a 3D model. Drag to turn it, scroll to zoom, W for the wireframe.", input.title))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .radius(t.radius.row)
        .effect(1)
        .focusable(true)
        .on_drag(move |s: &mut App, d, tx| {
            s.panel.states.with(&k1, |st| match d.phase {
                Phase::Down => st.camera.drag_from = (st.camera.yaw, st.camera.pitch),
                _ => orbit(&mut st.camera, d.dx, d.dy),
            });
            tx.live_only();
        })
        .on_wheel(move |s: &mut App, e, tx| {
            s.panel.states.with(&k2, |st| zoom(&mut st.camera, e.dy));
            tx.live_only();
            true
        })
        .on_double_press(move |s: &mut App| s.panel.states.with(&k3, |st| fit(&mut st.camera)));
    let hint = format!("{} triangles · drag to turn, scroll to zoom, double-click to fit, W for {}", super::table::group_digits(mesh.triangles()), if wire { "solid" } else { "the wireframe" });
    col().key("viewer-model3d").w_full().grow(1.0).min_h(0.0).padding(8.0).gap(6.0).child(view).child(text(hint).key("model3d-hint").font(t.ty.id).color(t.text.faint).truncate().px(4.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_stl_cube_loads_with_flat_normals_a_bounding_sphere_and_a_floor() {
        let mesh = load("stl", &crate::samples::stl()).unwrap();
        assert_eq!(mesh.triangles(), 12);
        assert_eq!(mesh.center, [0.5, 0.5, 0.5]);
        assert!((mesh.radius - 3f32.sqrt() / 2.0).abs() < 1e-5);
        assert_eq!(mesh.floor, 0.0);
        assert!(mesh.verts.iter().all(|v| (Vec3::from(v.nor).length() - 1.0).abs() < 1e-4), "unit normals");
        assert_eq!(mesh.edges.len() / 2, 18, "a triangulated cube has 18 distinct edges");
    }

    #[test]
    fn an_obj_loads_triangulated_with_computed_normals_and_turns_y_up_to_z_up() {
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        let mesh = load("obj", obj).unwrap();
        assert_eq!(mesh.triangles(), 2, "a quad is two triangles");
        // The quad lies in z = 0 (Y-up file): after the turn it stands up, y = 0 and z from 0 to 1.
        assert!(mesh.verts.iter().all(|v| v.pos[1].abs() < 1e-6));
        assert_eq!((mesh.floor, mesh.center[2]), (0.0, 0.5));
        assert!(mesh.verts.iter().all(|v| (Vec3::from(v.nor).length() - 1.0).abs() < 1e-4));
    }

    #[test]
    fn files_that_are_not_models_are_refused_with_a_line() {
        assert!(load("stl", b"garbage").is_err());
        assert!(load("obj", b"f 1 2 3\n").is_err(), "faces without points");
        assert!(load("glb", b"glTFnot really").is_err());
        assert!(load("xyz", b"").unwrap_err().contains("isn’t a 3D format"));
        assert!(load("obj", b"# nothing\n").unwrap_err().contains("no triangles"));
    }

    #[test]
    fn the_camera_orbits_zooms_and_fits() {
        let mut cam = Camera::default();
        cam.drag_from = (cam.yaw, cam.pitch);
        orbit(&mut cam, 100.0, -50.0);
        assert!((cam.yaw - (0.7 - 1.0)).abs() < 1e-5 && (cam.pitch - (0.45 - 0.5)).abs() < 1e-5);
        orbit(&mut cam, 0.0, 10_000.0);
        assert_eq!(cam.pitch, 1.5, "the pitch stops at the poles");
        zoom(&mut cam, 100.0);
        assert!(cam.zoom > 1.4 && cam.zoom < 1.6);
        zoom(&mut cam, -100_000.0);
        assert_eq!(cam.zoom, 0.2);
        cam.wire = true;
        fit(&mut cam);
        assert_eq!((cam.yaw, cam.pitch, cam.zoom, cam.wire), (0.7, 0.45, 1.0, true), "fitted, the wireframe stays");
    }

    #[test]
    fn a_fitted_camera_keeps_the_model_inside_the_view() {
        let mesh = load("stl", &crate::samples::stl()).unwrap();
        let (m, _) = mvp(&mesh, &Camera::default(), 1.5);
        let proj = Mat4::from_cols_array_2d(&m);
        for v in &mesh.verts {
            let p = proj * Vec3::from(v.pos).extend(1.0);
            let (x, y) = (p.x / p.w, p.y / p.w);
            assert!(x.abs() <= 1.0 && y.abs() <= 1.0 && p.z / p.w <= 1.0, "{x} {y}");
        }
    }
}
