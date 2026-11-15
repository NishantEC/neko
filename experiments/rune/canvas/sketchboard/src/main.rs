//! An Excalidraw-ish board on `examples/sketch`'s idea (a `canvas(Shape)` plus drag handlers):
//! rectangles, ellipses, arrows, freehand, text, select / move / delete, JSON save and load.
//! Written in about two hours to see where Rune helps and where it does not (REPORT.md).
//!
//! `sketchboard --snap out.png --script scripts/demo.json`; `SKETCH_FILE=board.json` sets the file.

use rune::tok::{self, radius, space, type_};
use rune::{AppSpec, Color, Cx, El, Key, LineCap, Phase, Shape, TextEdit, canvas, col, el, row, text, text_input};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
enum Kind {
    Rect,
    Ellipse,
    Arrow,
    Pen,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Select,
    Draw(Kind),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Elem {
    id: u32,
    kind: Kind,
    /// Rect, ellipse, text: top-left. Arrow: start. Pen: unused (points carry the position).
    x: f32,
    y: f32,
    /// Rect, ellipse: size. Arrow: end minus start (may be negative).
    w: f32,
    h: f32,
    #[serde(default)]
    text: String,
    #[serde(default)]
    pts: Vec<[f32; 2]>,
    /// Index into `INKS`.
    ink: usize,
}

const INKS: [Color; 4] = [
    Color { r: 0.12, g: 0.12, b: 0.14, a: 1.0 },
    Color { r: 0.80, g: 0.18, b: 0.18, a: 1.0 },
    Color { r: 0.10, g: 0.45, b: 0.85, a: 1.0 },
    Color { r: 0.10, g: 0.60, b: 0.30, a: 1.0 },
];
const SELECT: Color = Color { r: 0.38, g: 0.30, b: 0.95, a: 1.0 };
const TEXT_SIZE: f32 = 18.0;
const TEXT_STYLE: tok::TextStyle = tok::TextStyle::new(TEXT_SIZE, 400.0);

#[derive(Clone, Copy)]
enum Grab {
    None,
    /// Dragging element `id`, which started at (x, y).
    Move(u32, f32, f32),
    /// Drawing element `id`.
    Create(u32),
}

struct State {
    elems: Vec<Elem>,
    tool: Tool,
    ink: usize,
    selected: Option<u32>,
    grab: Grab,
    next_id: u32,
    /// The text element being typed into, and its field.
    editing: Option<u32>,
    edit: TextEdit,
    status: String,
}

impl Default for State {
    fn default() -> Self {
        State { elems: vec![], tool: Tool::Select, ink: 0, selected: None, grab: Grab::None, next_id: 1, editing: None, edit: TextEdit::new(""), status: String::new() }
    }
}

fn file() -> String {
    std::env::var("SKETCH_FILE").unwrap_or_else(|_| "board.json".into())
}

fn save(elems: &[Elem]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&serde_json::json!({ "version": 1, "elements": elems })).map_err(|e| e.to_string())?;
    std::fs::write(file(), json).map_err(|e| e.to_string())
}

fn load() -> Result<Vec<Elem>, String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    serde_json::from_value(v["elements"].clone()).map_err(|e| e.to_string())
}

fn text_size(e: &Elem) -> (f32, f32) {
    // Rune has no text-measure call usable from a handler, so this is an estimate of the Inter metrics.
    (e.text.chars().count().max(1) as f32 * TEXT_SIZE * 0.56, TEXT_SIZE * 1.3)
}

fn bounds(e: &Elem) -> [f32; 4] {
    match e.kind {
        Kind::Rect | Kind::Ellipse => [e.x.min(e.x + e.w), e.y.min(e.y + e.h), e.w.abs(), e.h.abs()],
        Kind::Arrow => [e.x.min(e.x + e.w), e.y.min(e.y + e.h), e.w.abs(), e.h.abs()],
        Kind::Text => {
            let (w, h) = text_size(e);
            [e.x, e.y, w, h]
        }
        Kind::Pen => {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in &e.pts {
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
            [x0, y0, x1 - x0, y1 - y0]
        }
    }
}

fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) };
    ((p[0] - (a[0] + t * dx)).powi(2) + (p[1] - (a[1] + t * dy)).powi(2)).sqrt()
}

fn hit(e: &Elem, p: [f32; 2]) -> bool {
    const SLOP: f32 = 6.0;
    let [x, y, w, h] = bounds(e);
    match e.kind {
        // Hollow shapes hit on their edge, as Excalidraw does, so what is inside stays reachable.
        Kind::Rect => {
            let inside = p[0] >= x - SLOP && p[0] <= x + w + SLOP && p[1] >= y - SLOP && p[1] <= y + h + SLOP;
            let deep = p[0] > x + SLOP && p[0] < x + w - SLOP && p[1] > y + SLOP && p[1] < y + h - SLOP;
            inside && !deep
        }
        Kind::Ellipse => {
            let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, (w / 2.0).max(1.0), (h / 2.0).max(1.0));
            let d = (((p[0] - cx) / rx).powi(2) + ((p[1] - cy) / ry).powi(2)).sqrt();
            (d - 1.0).abs() * rx.min(ry) < SLOP
        }
        Kind::Arrow => seg_dist(p, [e.x, e.y], [e.x + e.w, e.y + e.h]) < SLOP,
        Kind::Text => p[0] >= x && p[0] <= x + w && p[1] >= y && p[1] <= y + h,
        Kind::Pen => e.pts.windows(2).any(|s| seg_dist(p, s[0], s[1]) < SLOP) || e.pts.len() == 1,
    }
}

fn ellipse_points(x: f32, y: f32, w: f32, h: f32) -> Vec<[f32; 2]> {
    (0..=48).map(|i| {
        let a = i as f32 / 48.0 * std::f32::consts::TAU;
        [x + w / 2.0 + a.cos() * w / 2.0, y + h / 2.0 + a.sin() * h / 2.0]
    }).collect()
}

fn drawing(s: &State) -> Shape {
    let mut shape = Shape::new();
    for e in &s.elems {
        let ink = INKS[e.ink % INKS.len()];
        shape = match e.kind {
            Kind::Rect => {
                let [x, y, w, h] = bounds(e);
                shape.stroke(vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]], 2.0, ink)
            }
            Kind::Ellipse => {
                let [x, y, w, h] = bounds(e);
                shape.stroke(ellipse_points(x, y, w, h), 2.0, ink)
            }
            Kind::Arrow => {
                let (a, b) = ([e.x, e.y], [e.x + e.w, e.y + e.h]);
                let ang = (b[1] - a[1]).atan2(b[0] - a[0]);
                let head = |da: f32| [b[0] - 14.0 * (ang + da).cos(), b[1] - 14.0 * (ang + da).sin()];
                shape.line(a, b, 2.0, LineCap::Round, ink).line(b, head(0.45), 2.0, LineCap::Round, ink).line(b, head(-0.45), 2.0, LineCap::Round, ink)
            }
            Kind::Pen => match e.pts.as_slice() {
                [p] => shape.line(*p, *p, 2.5, LineCap::Round, ink),
                pts => shape.stroke(pts.to_vec(), 2.5, ink),
            },
            Kind::Text => shape,
        };
    }
    if let Some(e) = s.selected.and_then(|id| s.elems.iter().find(|e| e.id == id)) {
        let [x, y, w, h] = bounds(e);
        let (x, y, w, h) = (x - 6.0, y - 6.0, w + 12.0, h + 12.0);
        shape = shape.stroke(vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]], 1.5, SELECT);
        for c in [[x, y], [x + w, y], [x + w, y + h], [x, y + h]] {
            shape = shape.fill(vec![[c[0] - 3.0, c[1] - 3.0], [c[0] + 3.0, c[1] - 3.0], [c[0] + 3.0, c[1] + 3.0], [c[0] - 3.0, c[1] + 3.0]], SELECT);
        }
    }
    shape
}

fn commit_text(s: &mut State) {
    if let Some(id) = s.editing.take() {
        let value = s.edit.text.clone();
        if value.trim().is_empty() {
            s.elems.retain(|e| e.id != id);
            if s.selected == Some(id) {
                s.selected = None;
            }
        } else if let Some(e) = s.elems.iter_mut().find(|e| e.id == id) {
            e.text = value;
        }
    }
}

fn pointer(s: &mut State, d: &rune::Drag) {
    let p = [d.x, d.y];
    match d.phase {
        Phase::Down => {
            commit_text(s);
            match s.tool {
                Tool::Select => {
                    let top = s.elems.iter().rev().find(|e| hit(e, p)).map(|e| (e.id, e.x, e.y));
                    s.selected = top.map(|t| t.0);
                    s.grab = top.map_or(Grab::None, |(id, x, y)| Grab::Move(id, x, y));
                }
                Tool::Draw(kind) => {
                    let id = s.next_id;
                    s.next_id += 1;
                    let mut e = Elem { id, kind, x: p[0], y: p[1], w: 0.0, h: 0.0, text: String::new(), pts: vec![], ink: s.ink };
                    if kind == Kind::Pen {
                        e.pts.push(p);
                    }
                    if kind == Kind::Text {
                        s.edit = TextEdit::new("");
                        s.edit.focused = true;
                        s.editing = Some(id);
                        s.selected = Some(id);
                        s.elems.push(e);
                        s.tool = Tool::Select;
                        return;
                    }
                    s.elems.push(e);
                    s.selected = Some(id);
                    s.grab = Grab::Create(id);
                }
            }
        }
        Phase::Up => {
            if let Grab::Create(id) = s.grab {
                // A click without a drag leaves nothing worth keeping.
                if let Some(e) = s.elems.iter().find(|e| e.id == id) {
                    let tiny = match e.kind { Kind::Pen => false, _ => e.w.abs() < 3.0 && e.h.abs() < 3.0 };
                    if tiny {
                        s.elems.retain(|e| e.id != id);
                        s.selected = None;
                    }
                }
                s.tool = Tool::Select;
            }
            s.grab = Grab::None;
        }
        _ => match s.grab {
            Grab::Move(id, x0, y0) => {
                let (dx, dy) = (d.x - d.start_x, d.y - d.start_y);
                if let Some(e) = s.elems.iter_mut().find(|e| e.id == id) {
                    let (nx, ny) = (x0 + dx, y0 + dy);
                    let (mx, my) = (nx - e.x, ny - e.y);
                    e.x = nx;
                    e.y = ny;
                    for q in &mut e.pts {
                        q[0] += mx;
                        q[1] += my;
                    }
                }
            }
            Grab::Create(id) => {
                if let Some(e) = s.elems.iter_mut().find(|e| e.id == id) {
                    match e.kind {
                        Kind::Pen => e.pts.push(p),
                        _ => {
                            e.w = p[0] - e.x;
                            e.h = p[1] - e.y;
                        }
                    }
                }
            }
            Grab::None => {}
        },
    }
}

fn tool_button(label: &'static str, key: &'static str, tool: Tool, s: &State) -> El {
    action(label, key, s.tool == tool, move |s| {
        commit_text(s);
        s.tool = tool;
    })
}

fn action(label: &'static str, key: &'static str, on: bool, f: impl Fn(&mut State) + 'static) -> El {
    let t = tok::theme(false);
    row().key(key).role("button").label(label).focusable(true).px(space::S3).py(space::S1).radius(radius::MD)
        .fill(if on { tok::accent(0, false) } else { t.control_off })
        .child(text(label).font(type_::BODY).color(if on { Color::WHITE } else { t.text }))
        .on_press(move |s: &mut State| f(s))
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(false);
    let toolbar = row().gap(space::S2).items_center()
        .child(tool_button("Select", "tool-select", Tool::Select, s))
        .child(tool_button("Rectangle", "tool-rect", Tool::Draw(Kind::Rect), s))
        .child(tool_button("Ellipse", "tool-ellipse", Tool::Draw(Kind::Ellipse), s))
        .child(tool_button("Arrow", "tool-arrow", Tool::Draw(Kind::Arrow), s))
        .child(tool_button("Text", "tool-text", Tool::Draw(Kind::Text), s))
        .child(tool_button("Pen", "tool-pen", Tool::Draw(Kind::Pen), s))
        .child(action("Ink", "ink", false, |s| s.ink = (s.ink + 1) % INKS.len()))
        .child(action("Delete", "delete", false, |s| {
            if let Some(id) = s.selected.take() {
                s.elems.retain(|e| e.id != id);
            }
        }))
        .child(action("Save", "save", false, |s| s.status = match save(&s.elems) { Ok(()) => format!("saved {} elements to {}", s.elems.len(), file()), Err(e) => format!("save failed: {e}") }))
        .child(action("Load", "load", false, |s| {
            s.status = match load() {
                Ok(v) => {
                    s.next_id = v.iter().map(|e| e.id).max().unwrap_or(0) + 1;
                    s.selected = None;
                    let n = v.len();
                    s.elems = v;
                    format!("loaded {n} elements from {}", file())
                }
                Err(e) => format!("load failed: {e}"),
            }
        }))
        .child(text(s.status.clone()).font(type_::BODY).color(t.text_secondary));

    let mut paper = el().key("paper").w_full().grow(1.0).radius(radius::LG).fill(Color::WHITE).border(1.0, t.card_border).clip_rounded(radius::LG)
        .child(canvas(drawing(s)).key("surface").w_full().h_full().absolute(0.0, 0.0)
            .on_drag(|s: &mut State, d, _| pointer(s, d)));
    for e in s.elems.iter().filter(|e| e.kind == Kind::Text && Some(e.id) != s.editing) {
        paper = paper.child(text(e.text.clone()).font(TEXT_STYLE).color(INKS[e.ink % INKS.len()]).absolute(e.x, e.y));
    }
    if let Some(e) = s.editing.and_then(|id| s.elems.iter().find(|e| e.id == id)) {
        paper = paper.child(text_input(&s.edit, TEXT_STYLE).key("text-edit").single_line().min_w(160.0).absolute(e.x, e.y)
            .on_edit(|s: &mut State| &mut s.edit)
            .on_submit(|s: &mut State, tx| {
                commit_text(s);
                tx.blur();
            }));
    }
    col().w_full().h_full().padding(space::S3).gap(space::S3).fill(t.bg)
        .child(toolbar)
        .child(paper)
        .on_key(|s: &mut State, k, _| {
            if s.editing.is_some() {
                return false;
            }
            match &k.key {
                Key::Text(c) => {
                    let tool = match c.as_str() {
                        "v" => Tool::Select,
                        "r" => Tool::Draw(Kind::Rect),
                        "o" => Tool::Draw(Kind::Ellipse),
                        "a" => Tool::Draw(Kind::Arrow),
                        "t" => Tool::Draw(Kind::Text),
                        "p" => Tool::Draw(Kind::Pen),
                        _ => return false,
                    };
                    s.tool = tool;
                    true
                }
                _ => false,
            }
        })
}

fn main() {
    let spec = AppSpec::<State> {
        scenes: &[("empty", |_| {}), ("sample", sample)],
        ..AppSpec::new("Sketchboard", (900.0, 640.0), State::default, view)
    };
    rune::run(spec);
}

/// A small board for snapshots: every element kind, one selected.
fn sample(s: &mut State) {
    let mk = |id, kind, x, y, w, h, text: &str, ink| Elem { id, kind, x, y, w, h, text: text.into(), pts: vec![], ink };
    s.elems = vec![
        mk(1, Kind::Rect, 60.0, 60.0, 200.0, 110.0, "", 0),
        mk(2, Kind::Ellipse, 380.0, 60.0, 170.0, 110.0, "", 2),
        mk(3, Kind::Arrow, 262.0, 115.0, 116.0, 0.0, "", 1),
        mk(4, Kind::Text, 84.0, 100.0, 0.0, 0.0, "Neko", 0),
        mk(5, Kind::Text, 430.0, 100.0, 0.0, 0.0, "Rune?", 2),
    ];
    let wave: Vec<[f32; 2]> = (0..40).map(|i| [90.0 + i as f32 * 8.0, 260.0 + (i as f32 * 0.5).sin() * 30.0]).collect();
    s.elems.push(Elem { id: 6, kind: Kind::Pen, x: 0.0, y: 0.0, w: 0.0, h: 0.0, text: String::new(), pts: wave, ink: 3 });
    s.selected = Some(2);
    s.next_id = 7;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn rt() -> Runtime<State> {
        let mut rt = Runtime::new(State::default(), view, (900.0, 640.0), Env { system_dark: false, scale: 2.0 });
        rt.frame(0.0);
        rt
    }

    fn press(rt: &mut Runtime<State>, key: &str) {
        let (x, y) = rt.find(key).unwrap();
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
    }

    fn drag(rt: &mut Runtime<State>, from: (f32, f32), to: (f32, f32)) {
        rt.pointer_move(from.0, from.1);
        rt.pointer_down(from.0, from.1);
        rt.frame(0.0);
        rt.pointer_move((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        rt.pointer_move(to.0, to.1);
        rt.pointer_up(to.0, to.1);
        rt.frame(0.0);
    }

    #[test]
    fn draw_move_save_load_round_trip() {
        let mut rt = rt();
        let (px, py) = rt.find("paper").unwrap();
        press(&mut rt, "tool-rect");
        drag(&mut rt, (px + 40.0, py + 40.0), (px + 140.0, py + 100.0));
        press(&mut rt, "tool-arrow");
        drag(&mut rt, (px + 150.0, py + 70.0), (px + 260.0, py + 70.0));
        assert_eq!(rt.state.elems.len(), 2, "{:?}", rt.state.elems);
        assert_eq!(rt.state.tool, Tool::Select, "tool returns to select after a draw, as Excalidraw does");
        // Move the rectangle by grabbing its top edge.
        let before = rt.state.elems[0].clone();
        drag(&mut rt, (px + 90.0, py + 40.0), (px + 110.0, py + 80.0));
        let after = &rt.state.elems[0];
        assert!((after.x - before.x - 20.0).abs() < 1.0 && (after.y - before.y - 40.0).abs() < 1.0, "{before:?} -> {after:?}");
        // Save, wipe, load.
        let path = std::env::temp_dir().join("sketchboard-test.json");
        unsafe { std::env::set_var("SKETCH_FILE", &path) };
        let saved = rt.state.elems.clone();
        save(&saved).unwrap();
        rt.state.elems.clear();
        press(&mut rt, "load");
        assert_eq!(rt.state.elems, saved);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn text_tool_types_into_a_field_and_commits() {
        let mut rt = rt();
        let (px, py) = rt.find("paper").unwrap();
        press(&mut rt, "tool-text");
        rt.pointer_move(px + 50.0, py + 50.0);
        rt.pointer_down(px + 50.0, py + 50.0);
        rt.pointer_up(px + 50.0, py + 50.0);
        rt.frame(0.0);
        assert!(rt.state.editing.is_some());
    }
}
