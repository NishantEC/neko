//! What every app test shares: a headless runtime of the real app on a scene, and walking the accessibility tree.
#![allow(dead_code)]

use neko_app::state::App;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

/// The window shapes every scene is checked at: wide, and the smallest (where the sidebar has folded).
pub const SIZES: [(f32, f32); 2] = [(1200.0, 800.0), (760.0, 560.0)];

/// The real app, headless, at `size`, in light or dark, with `scene` applied and the layout settled.
pub fn runtime(size: (f32, f32), dark: bool, scene: fn(&mut App)) -> Runtime<App> {
    let mut rt = neko_app::test_app().headless_runtime(size, Env::new(dark, 2.0));
    rt.update(move |s, _| scene(s));
    settle(&mut rt);
    rt
}

pub fn settle(rt: &mut Runtime<App>) {
    for _ in 0..4 {
        rt.frame(0.0);
    }
}

pub fn scenes() -> &'static [(&'static str, fn(&mut App))] {
    neko_app::scenes::SCENES
}

pub fn press(rt: &mut Runtime<App>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    rt.frame(0.0);
    rt.frame(0.0);
}

pub fn tab(rt: &mut Runtime<App>, back: bool) {
    press(rt, Key::Tab, Mods { shift: back, ..Mods::default() });
}

pub fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

/// Every node of the accessibility tree, with its parent's index.
pub fn nodes(tree: &serde_json::Value) -> Vec<(Option<usize>, serde_json::Value)> {
    fn walk(v: &serde_json::Value, parent: Option<usize>, out: &mut Vec<(Option<usize>, serde_json::Value)>) {
        let me = out.len();
        let mut flat = v.clone();
        if let Some(o) = flat.as_object_mut() {
            o.remove("children");
        }
        out.push((parent, flat));
        for c in v.get("children").and_then(|c| c.as_array()).into_iter().flatten() {
            walk(c, Some(me), out);
        }
    }
    let mut out = Vec::new();
    walk(tree, None, &mut out);
    out
}

pub fn str_of<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

pub fn has(v: &serde_json::Value, list: &str, item: &str) -> bool {
    v.get(list).and_then(|a| a.as_array()).is_some_and(|a| a.iter().any(|x| x.as_str() == Some(item)))
}
