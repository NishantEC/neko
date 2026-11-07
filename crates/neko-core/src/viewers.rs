//! What can show an artifact, and what the model is told about it
//! (`docs/superpowers/specs/2026-11-08-everything-renders-design.md`, A4).
//!
//! The web app registers viewers (`host.registerViewer`) and reply fences
//! (`host.registerFence`); built-in ones and plugin ones go into the same
//! registry. The daemon cannot run that JavaScript, so a plugin also
//! *declares* what it adds in its `manifest.json`:
//!
//! ```json
//! { "viewers": [{ "id": "stl", "title": "STL viewer", "kinds": ["stl"] }],
//!   "fences": ["stl"],
//!   "prompt": "Save a 3D print as .stl; it opens in the STL viewer." }
//! ```
//!
//! [`capabilities`] reads and bounds those fields (the plugin listing passes
//! them to the page); [`installed_section`] turns the built-ins plus every
//! enabled plugin into the **Installed viewers** section that is appended to
//! the full-power chat instruction and routine prompts and printed by
//! `neko-daemon --plugin-guide`, so a model never meets a viewer it cannot
//! learn about. [`BUILTIN`] must stay in step with `web/src/canvas/builtin-viewers.tsx`.
use std::path::{Path, PathBuf};

use serde::Serialize;

/// A viewer Neko ships.
pub struct Builtin {
    pub id: &'static str,
    pub title: &'static str,
    pub kinds: &'static [&'static str],
    /// One line for the model: what it shows and how.
    pub prompt: &'static str,
}

/// The built-in viewers, in the order the section lists them.
pub const BUILTIN: &[Builtin] = &[
    Builtin { id: "html", title: "Page", kinds: &["html"], prompt: "a live, sandboxed page (inline CSS and scripts, no network); the person can point at elements to comment" },
    Builtin { id: "image", title: "Image", kinds: &["png", "jpg", "gif", "webp", "avif", "bmp", "ico", "svg"], prompt: "the image itself (svg is drawn live, sandboxed)" },
    Builtin { id: "pdf", title: "PDF", kinds: &["pdf"], prompt: "the browser's PDF viewer" },
    Builtin { id: "markdown", title: "Document", kinds: &["md"], prompt: "rendered Markdown (headings, lists, tables, code)" },
    Builtin { id: "csv", title: "Table", kinds: &["csv", "tsv"], prompt: "a table; the first row is the header" },
    Builtin { id: "3d", title: "3D model", kinds: &["glb", "gltf", "stl", "obj"], prompt: "a three.js view the person can orbit, zoom, fit and see as wireframe; a .gltf must embed its buffers (data: URIs), so prefer .glb" },
    Builtin { id: "excalidraw", title: "Sketch", kinds: &["excalidraw"], prompt: "an Excalidraw board the person can edit and save as the next version (the file is Excalidraw's JSON: {\"type\":\"excalidraw\",\"version\":2,\"elements\":[…],\"appState\":{},\"files\":{}})" },
    Builtin { id: "code", title: "Code", kinds: &["json", "txt", "log", "xml", "yaml", "toml", "ini", "js", "mjs", "ts", "tsx", "jsx", "css", "py", "rb", "rs", "go", "java", "kt", "swift", "c", "h", "cpp", "sh", "sql", "diff", "patch"], prompt: "highlighted, read-only text" },
];

/// Fences a chat reply may use that Neko draws itself.
pub const BUILTIN_FENCES: &[(&str, &str)] = &[
    ("csv", "a small table written inline (comma-separated, first row the header)"),
    ("tsv", "the same, tab-separated"),
    ("3d", "an artifact id (or `artifact <id> v<n>`) of a saved 3D model, drawn in the reply"),
];

pub const MAX_PROMPT_CHARS: usize = 400;
const MAX_VIEWERS: usize = 8;
const MAX_KINDS: usize = 32;
const MAX_FENCES: usize = 8;

/// One viewer a plugin's manifest declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclaredViewer {
    pub id: String,
    pub title: String,
    pub kinds: Vec<String>,
}

/// What a plugin's manifest says it adds for the model to know about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub viewers: Vec<DeclaredViewer>,
    pub fences: Vec<String>,
    pub prompt: Option<String>,
}

fn plain_id(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A kind as the artifact store spells it: lowercase, 1–16 letters or digits.
fn kind(value: &serde_json::Value) -> Option<String> {
    let text = value.as_str()?.trim().trim_start_matches('.').to_ascii_lowercase();
    crate::artifacts::kind_of(Path::new(&format!("x.{text}")))
}

fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(max).collect()
}

/// Reads `viewers`, `fences` and `prompt` from a manifest, dropping what is
/// malformed rather than failing: a plugin with a bad declaration still loads.
pub fn capabilities(manifest: &serde_json::Value) -> Capabilities {
    let viewers = manifest.get("viewers").and_then(|v| v.as_array()).map_or_else(Vec::new, |list| {
        list.iter()
            .filter_map(|v| {
                let id = v.get("id")?.as_str()?.to_string();
                if !plain_id(&id, 64) { return None; }
                let mut kinds: Vec<String> = Vec::new();
                for k in v.get("kinds")?.as_array()?.iter().filter_map(kind) {
                    if !kinds.contains(&k) && kinds.len() < MAX_KINDS { kinds.push(k); }
                }
                if kinds.is_empty() { return None; }
                let title = v.get("title").and_then(|t| t.as_str()).map(|t| one_line(t, 80)).filter(|t| !t.is_empty()).unwrap_or_else(|| id.clone());
                Some(DeclaredViewer { id, title, kinds })
            })
            .take(MAX_VIEWERS)
            .collect()
    });
    let fences = manifest.get("fences").and_then(|v| v.as_array()).map_or_else(Vec::new, |list| {
        let mut out: Vec<String> = Vec::new();
        for f in list.iter().filter_map(|f| f.as_str()).map(|f| f.trim().to_ascii_lowercase()).filter(|f| plain_id(f, 32)) {
            if !out.contains(&f) && out.len() < MAX_FENCES { out.push(f); }
        }
        out
    });
    let prompt = manifest.get("prompt").and_then(|p| p.as_str()).map(|p| one_line(p, MAX_PROMPT_CHARS)).filter(|p| !p.is_empty());
    Capabilities { viewers, fences, prompt }
}

/// The route an `OpenTab` may open, as the web app's hash spells it after
/// `#/`: `tasks`, `tasks?task=<id>&view=full`, `artifact?id=<id>`, `my-plugin:board`.
/// A leading `#`/`#/`/`/` is dropped; empty is Home. Anything that could name
/// another site (a scheme, `//`, a backslash, spaces) is refused.
pub fn tab_route(route: &str) -> Option<String> {
    let route = route.trim();
    let route = route.strip_prefix('#').unwrap_or(route);
    let route = route.strip_prefix('/').unwrap_or(route);
    let path = route.split('?').next().unwrap_or_default();
    let allowed = |c: char| c.is_ascii_alphanumeric() || "-_:/?=&.%".contains(c);
    let ok = route.len() <= 300 && route.chars().all(allowed) && !route.contains("//") && !route.starts_with('/') && !path.contains("..")
        && !path.split(':').next().is_some_and(|scheme| path.contains(':') && matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https" | "javascript" | "data" | "file" | "blob"));
    ok.then(|| route.to_string())
}

/// `<data dir>/plugins`.
pub fn plugins_dir() -> PathBuf {
    neko_protocol::support_dir().join("plugins")
}

/// A plugin as the section names it.
struct Installed {
    id: String,
    title: String,
    capabilities: Capabilities,
}

/// Every enabled plugin with a readable manifest that declares something,
/// sorted by folder name. Never cached: a plugin written a second ago counts.
fn installed(dir: &Path) -> Vec<Installed> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Installed> = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_str()?.to_string();
            if !plain_id(&id, 64) || !entry.path().is_dir() || entry.path().join(".disabled").exists() { return None; }
            let text = std::fs::read_to_string(entry.path().join("manifest.json")).ok().filter(|t| t.len() <= 64 * 1024)?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let capabilities = capabilities(&json);
            if capabilities == Capabilities::default() { return None; }
            let title = json.get("title").and_then(|t| t.as_str()).map(|t| one_line(t, 80)).unwrap_or_else(|| id.clone());
            Some(Installed { id, title, capabilities })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The **Installed viewers** section: how to save an artifact so it opens,
/// each viewer with its kinds (built-ins, then plugins, labelled), the reply
/// fences Neko draws, and each plugin's own prompt line. `neko` is the daemon
/// executable as the prompt should spell it.
pub fn installed_section(neko: &str, plugins: &Path) -> String {
    let mut out = String::from("Installed viewers. Anything you make for the person to look at is an artifact: write one file of its natural kind (a 3D model as .glb, a sketch or diagram as .excalidraw, a table as .csv, a document as .md, a screen design as .html, an image as .png or .svg) and run ");
    out.push_str(&format!("`'{neko}' --request '{{\"SaveArtifact\":{{\"title\":\"…\",\"file\":\"/absolute/path.glb\",\"workspace_id\":null,\"artifact_id\":null,\"note\":\"what this version is\",\"show\":true}}}}'`"));
    out.push_str(" (5 MB at most; \"show\" opens it on the person's screen, and is the default while you answer in Neko's chat). `{\"ShowArtifact\":{\"id\":\"…\",\"version\":null}}` shows one again and `{\"OpenTab\":{\"route\":\"tasks\"}}` opens a Neko page in a tab. The kind is the file's extension; each opens in its viewer, and any other kind is kept as a download:\n");
    for viewer in BUILTIN {
        out.push_str(&format!("- {} ({}, built in): {}.\n", viewer.title, viewer.kinds.iter().map(|k| format!(".{k}")).collect::<Vec<_>>().join(" "), viewer.prompt));
    }
    let plugins = installed(plugins);
    for plugin in &plugins {
        for viewer in &plugin.capabilities.viewers {
            out.push_str(&format!("- {} ({}, from the \"{}\" plugin).\n", viewer.title, viewer.kinds.iter().map(|k| format!(".{k}")).collect::<Vec<_>>().join(" "), plugin.id));
        }
    }
    out.push_str("Fenced blocks Neko draws inside a chat reply (use them only for content that small; anything worth keeping is an artifact):");
    let mut fences: Vec<String> = BUILTIN_FENCES.iter().map(|(lang, what)| format!(" ```{lang} for {what}")).collect();
    for plugin in &plugins {
        fences.extend(plugin.capabilities.fences.iter().map(|lang| format!(" ```{lang} (the \"{}\" plugin)", plugin.id)));
    }
    out.push_str(&fences.join(";"));
    out.push_str(". Any other fence is shown as code.");
    for plugin in plugins.iter().filter(|p| p.capabilities.prompt.is_some()) {
        out.push_str(&format!("\n{} (plugin \"{}\"): {}", plugin.title, plugin.id, plugin.capabilities.prompt.as_deref().unwrap_or_default()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn a_manifest_declares_viewers_fences_and_a_prompt_within_bounds() {
        let caps = capabilities(&json!({
            "viewers": [
                { "id": "stl", "title": "STL viewer", "kinds": ["STL", ".obj", "stl", "bad kind", 3] },
                { "id": "../x", "kinds": ["stl"] },
                { "id": "nokinds", "kinds": [] },
                { "id": "untitled", "kinds": ["ply"] },
            ],
            "fences": ["STL", "bad fence", "", "x".repeat(40)],
            "prompt": format!("Save prints as .stl.\n\n  {}", "y".repeat(600)),
        }));
        assert_eq!(caps.viewers, vec![
            DeclaredViewer { id: "stl".into(), title: "STL viewer".into(), kinds: vec!["stl".into(), "obj".into()] },
            DeclaredViewer { id: "untitled".into(), title: "untitled".into(), kinds: vec!["ply".into()] },
        ]);
        assert_eq!(caps.fences, vec!["stl".to_string()]);
        let prompt = caps.prompt.unwrap();
        assert!(prompt.starts_with("Save prints as .stl. y") && prompt.chars().count() == MAX_PROMPT_CHARS && !prompt.contains('\n'));
        assert_eq!(capabilities(&json!({ "title": "Plain page" })), Capabilities::default());
        assert_eq!(capabilities(&json!({ "viewers": "nope", "fences": {}, "prompt": 3 })), Capabilities::default());
    }

    #[test]
    fn the_section_lists_built_ins_then_enabled_plugins_only() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins = tmp.path();
        let write = |id: &str, manifest: serde_json::Value| {
            fs::create_dir_all(plugins.join(id)).unwrap();
            fs::write(plugins.join(id).join("manifest.json"), manifest.to_string()).unwrap();
        };
        write("stl", json!({ "title": "STL viewer", "viewers": [{ "id": "stl", "title": "STL viewer", "kinds": ["stl"] }], "fences": ["stl"], "prompt": "Save 3D prints as .stl." }));
        write("off", json!({ "title": "Off", "viewers": [{ "id": "off", "kinds": ["off"] }], "prompt": "never shown" }));
        fs::write(plugins.join("off/.disabled"), "").unwrap();
        write("page-only", json!({ "title": "Board" }));
        fs::create_dir_all(plugins.join("broken")).unwrap();
        fs::write(plugins.join("broken/manifest.json"), "{nope").unwrap();
        let text = installed_section("/Applications/Neko.app/Contents/MacOS/neko-daemon", plugins);
        assert!(text.starts_with("Installed viewers."));
        assert!(text.contains("'/Applications/Neko.app/Contents/MacOS/neko-daemon' --request '{\"SaveArtifact\":{\"title\":\"…\",\"file\":\"/absolute/path.glb\""));
        assert!(text.contains("\"show\":true") && text.contains("ShowArtifact") && text.contains("OpenTab"));
        for builtin in ["3D model (.glb .gltf .stl .obj, built in)", "Sketch (.excalidraw, built in)", "Table (.csv .tsv, built in)", "Document (.md, built in)", "Page (.html, built in)"] {
            assert!(text.contains(builtin), "{builtin}");
        }
        assert!(text.contains("- STL viewer (.stl, from the \"stl\" plugin)."));
        assert!(text.contains("```csv for") && text.contains("```3d for") && text.contains("```stl (the \"stl\" plugin)"));
        assert!(text.ends_with("STL viewer (plugin \"stl\"): Save 3D prints as .stl."));
        assert!(!text.contains("never shown") && !text.contains("Board"), "disabled and page-only plugins add nothing");
        assert!(!text.contains("mermaid"), "Neko does not draw mermaid");
        // No plugins folder at all: the built-ins alone.
        let bare = installed_section("neko-daemon", &tmp.path().join("missing"));
        assert!(bare.contains("3D model") && !bare.contains("plugin \""));
    }

    #[test]
    fn open_tab_takes_neko_routes_only() {
        for (given, route) in [
            ("tasks", "tasks"),
            ("#/tasks?task=abc&view=full", "tasks?task=abc&view=full"),
            ("/artifact?id=a1b2c3d4e5f6&v=2", "artifact?id=a1b2c3d4e5f6&v=2"),
            ("my-plugin:board", "my-plugin:board"),
            ("", ""),
            ("#/", ""),
        ] {
            assert_eq!(tab_route(given).as_deref(), Some(route), "{given}");
        }
        for evil in ["https://evil.example", "//evil.example", "javascript:alert(1)", "data:text/html,x", "a b", "../x", "x\\y", "tasks\"><script>", &"a".repeat(301)] {
            assert_eq!(tab_route(evil), None, "{evil}");
        }
    }

    #[test]
    fn built_in_kinds_are_store_kinds() {
        for viewer in BUILTIN {
            for kind in viewer.kinds {
                assert_eq!(crate::artifacts::kind_of(Path::new(&format!("x.{kind}"))).as_deref(), Some(*kind), "{}", viewer.id);
            }
        }
    }
}
