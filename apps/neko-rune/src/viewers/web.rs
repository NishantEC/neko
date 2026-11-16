//! The web-backed viewers (P3b): HTML artifacts, Excalidraw sketches and whatever a JS plugin's viewer
//! draws. Rune does not draw these: the web UI already does, in its embed mode, and the web panel
//! (`web_panel.rs`) shows that page in a web view. Registering them is how the panel learns which kinds
//! go there.
//!
//! | id | kinds | priority |
//! | --- | --- | --- |
//! | `web-artifact` | html htm | 5 |
//! | `web-excalidraw` | excalidraw | 5 |
//! | `web-plugin:<plugin>:<viewer>` | each kind the plugin's manifest declares for the viewer | 10 |
//!
//! A plugin's viewer outranks Rune's own at 0, as in the web UI (a plugin that declares `stl` or `glb`
//! gets that kind). The page chooses the viewer itself, so every one of these asks for the same thing:
//! `artifact/<id>/<n>?embed=1`.

use std::sync::Mutex;

use neko_protocol::PluginViewer;
use rune::{El, el, text};

use super::{ViewerInput, ViewerSpec, ViewerState, replace_group};
use crate::app::App;
use crate::theme::Neko;
use crate::web_panel::Rect;

/// Whether a viewer is one of these (the panel puts the web view in its place).
pub fn is_web(viewer_id: &str) -> bool {
    viewer_id.starts_with("web-")
}

/// The registry's `build` for every web viewer. The panel never calls it (it draws [`slot`] itself, which
/// knows whether the view is up); it is what a caller that only has the registry gets: the bare slot.
pub fn build(input: &ViewerInput, _state: &mut ViewerState) -> El {
    slot(&crate::theme::theme(input.theme_dark), "")
}

/// What the slot says: nothing once the view is up, why when it could not start, else that it is on its way.
pub fn line(s: &App) -> String {
    if s.web_view.exists() { String::new() } else { s.web_view.error().unwrap_or_else(|| "Opening…".to_owned()) }
}

/// The place the web view sits over: the panel's own surface colour (what shows when the view is hidden
/// under a menu), and a quiet line while the view is still on its way. The slot reports its rect every
/// layout pass and the view follows it.
pub fn slot(t: &Neko, line: &str) -> El {
    // The line is always there, and empty once the view is up: the same tree every build.
    el().key("viewer-web")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(t.surface.card)
        .center()
        .on_layout_tx(|s: &mut App, r: Rect, tx: &mut rune::Tx| {
            s.web_view.set_slot(r);
            tx.live_only();
        })
        .child(text(line.to_owned()).key("web-opening").font(t.ty.meta).color(t.text.faint).a11y_hidden())
}

/// Marks the region the web view is cut to (the panel's body, or a plugin page's whole region).
pub fn region(el: El) -> El {
    el.on_layout_tx(|s: &mut App, r: Rect, tx: &mut rune::Tx| {
        s.web_view.set_region(r);
        tx.live_only();
    })
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn spec(id: &'static str, title: &'static str, kinds: &'static [&'static str], priority: i32) -> ViewerSpec {
    ViewerSpec { id, title, kinds, priority, build }
}

/// Neko's own web-backed viewers, at 5.
pub fn builtins() -> Vec<ViewerSpec> {
    vec![spec("web-artifact", "Page", &["html", "htm"], 5), spec("web-excalidraw", "Sketch", &["excalidraw"], 5)]
}

/// What was registered from the plugins' manifests last, so the same list is not registered again.
fn last() -> &'static Mutex<String> {
    static LAST: std::sync::OnceLock<Mutex<String>> = std::sync::OnceLock::new();
    LAST.get_or_init(Default::default)
}

/// Registers a `web-plugin:<plugin>:<viewer>` viewer at priority 10 for each viewer the plugins declare,
/// replacing the ones registered before (a plugin that was removed or switched off stops claiming kinds).
pub fn register_plugin_viewers(viewers: &[PluginViewer]) {
    let signature = viewers.iter().map(|v| format!("{}:{}:{}:{}", v.plugin, v.viewer_id, v.kinds.join(","), v.title)).collect::<Vec<_>>().join("|");
    let mut last = last().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if *last == signature {
        return;
    }
    *last = signature;
    let specs = viewers
        .iter()
        .filter(|v| !v.kinds.is_empty())
        .map(|v| {
            let kinds: &'static [&'static str] = Box::leak(v.kinds.iter().map(|k| leak(k.trim_start_matches('.').to_ascii_lowercase())).collect::<Vec<_>>().into_boxed_slice());
            spec(leak(format!("web-plugin:{}:{}", v.plugin, v.viewer_id)), leak(v.title.clone()), kinds, 10)
        })
        .collect();
    replace_group("web-plugin", specs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewers::resolve;

    #[test]
    fn html_and_excalidraw_resolve_to_the_web_and_a_plugins_kind_beats_a_builtin() {
        assert_eq!(resolve("html").map(|s| s.id), Some("web-artifact"));
        assert_eq!(resolve("htm").map(|s| s.id), Some("web-artifact"));
        assert_eq!(resolve("excalidraw").map(|s| s.id), Some("web-excalidraw"));
        assert_eq!(resolve("png").map(|s| s.id), Some("image"), "Rune's own stay Rune's");
        crate::viewers::register(ViewerSpec { id: "zz-own", title: "own", kinds: &["zzbuiltin"], priority: 0, build });
        register_plugin_viewers(&[
            PluginViewer { plugin: "stl-tools".into(), viewer_id: "stl".into(), kinds: vec!["stlx".into(), ".WRL".into()], title: "STL viewer".into() },
            PluginViewer { plugin: "nobody".into(), viewer_id: "none".into(), kinds: vec![], title: "No kinds".into() },
            PluginViewer { plugin: "zz-plus".into(), viewer_id: "better".into(), kinds: vec!["zzbuiltin".into()], title: "Better".into() },
        ]);
        assert_eq!(resolve("stlx").map(|s| (s.id, s.priority)), Some(("web-plugin:stl-tools:stl", 10)));
        assert_eq!(resolve("wrl").map(|s| s.id), Some("web-plugin:stl-tools:stl"), "kinds are lowercase without the dot");
        assert_eq!(resolve("zzbuiltin").map(|s| s.id), Some("web-plugin:zz-plus:better"), "priority 10 beats a built-in at 0, as in the web UI");
        assert!(resolve("none").is_none());
        // Replaced when the list changes: the plugin that went no longer claims anything.
        register_plugin_viewers(&[]);
        assert_eq!(resolve("zzbuiltin").map(|s| s.id), Some("zz-own"));
        assert!(resolve("stlx").is_none());
        assert!(is_web("web-plugin:a:b") && is_web("web-artifact") && !is_web("image"));
    }
}
