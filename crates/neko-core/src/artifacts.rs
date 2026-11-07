//! Artifacts: files Neko keeps for the person, shown on the canvas beside the
//! chat (`docs/superpowers/specs/2026-11-06-artifact-canvas-design.md`,
//! `docs/superpowers/specs/2026-11-08-everything-renders-design.md`).
//!
//! `<data dir>/artifacts/<id>/` holds `meta.json` (the [`Artifact`] record)
//! and one `v<n>.<kind>` per version. An agent writes the file wherever it
//! likes and sends `SaveArtifact`; the daemon copies it in here, so what the
//! canvas shows can never change under it and never points outside this
//! folder. Any file type is kept: its **kind** is its extension, normalised
//! ([`kind_of`]); kinds Neko knows must look like what their name says
//! ([`check_contents`]), the rest are kept as opaque bytes. Only html is
//! served as a page (with its sandbox and comment bridge); every other kind
//! is served as a file under its own type ([`mime`]) and never run. Ids are
//! plain characters and every path is proved to stay inside the store.
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use neko_protocol::{Artifact, ArtifactVersion};

/// The most one version may weigh.
pub const MAX_BYTES: u64 = 5 * 1024 * 1024;
/// Kinds kept as UTF-8 text: their contents must decode, and a viewer that
/// edits (Excalidraw) may save the next version of one as text.
pub const TEXT_KINDS: &[&str] = &[
    "html", "svg", "md", "csv", "tsv", "json", "excalidraw", "gltf", "obj", "txt", "log", "xml", "yaml", "toml", "ini",
    "js", "mjs", "cjs", "ts", "tsx", "jsx", "css", "scss", "py", "rb", "rs", "go", "java", "kt", "swift", "c", "h", "cc", "cpp", "hpp",
    "cs", "php", "sh", "bash", "zsh", "fish", "sql", "lua", "r", "dart", "scala", "ex", "exs", "hs", "ml", "vue", "svelte", "diff", "patch",
    "graphql", "proto", "tex", "mermaid", "dot",
];
/// Extensions that name the same kind as another.
const ALIASES: &[(&str, &str)] = &[("htm", "html"), ("markdown", "md"), ("jpeg", "jpg"), ("yml", "yaml"), ("tif", "tiff"), ("text", "txt")];
pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_NOTE_CHARS: usize = 500;
/// Versions one artifact keeps; past this, make a new artifact.
pub const MAX_VERSIONS: u32 = 200;

/// `<data dir>/artifacts`.
pub fn root() -> PathBuf {
    neko_protocol::support_dir().join("artifacts")
}

/// An id (artifact, workspace or message) is 1–64 letters, digits, `-` or `_`:
/// safe as a folder name and in a URL.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// What a `SaveArtifact` asks for, once the daemon has added the reply it
/// belongs to.
#[derive(Debug, Clone, Default)]
pub struct Save {
    pub title: String,
    pub file: PathBuf,
    pub workspace_id: Option<String>,
    pub artifact_id: Option<String>,
    pub note: String,
    pub message_id: Option<String>,
}

/// Saves run one at a time, so two versions of one artifact never get the
/// same number.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn shorten(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}

/// The kind a file name says: its extension, lowercased, with the aliases
/// folded in (`htm` is html, `jpeg` is jpg). An extension is 1–16 ASCII
/// letters or digits, so a kind is always safe as a file extension and in a
/// URL. A file without one has no kind.
pub fn kind_of(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !(1..=16).contains(&ext.len()) || !ext.bytes().all(|b| b.is_ascii_alphanumeric()) { return None; }
    Some(ALIASES.iter().find(|(from, _)| *from == ext).map_or(ext.clone(), |(_, to)| (*to).to_string()))
}

/// Whether a kind is kept as UTF-8 text.
pub fn is_text(kind: &str) -> bool {
    TEXT_KINDS.contains(&kind)
}

/// The type a kind is served as. Only html is a page (the web bridge gives
/// it its sandbox and comment bridge); svg is an image, which also runs only
/// inside the artifact sandbox. Everything else is inert: text kinds as text,
/// known media under their own type, the rest `application/octet-stream`.
pub fn mime(kind: &str) -> &'static str {
    match kind {
        "html" => "text/html; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "json" | "excalidraw" => "application/json",
        "md" => "text/markdown; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "tsv" => "text/tab-separated-values; charset=utf-8",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "stl" => "model/stl",
        "obj" => "model/obj",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        kind if is_text(kind) => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Whether the bytes are what the kind says. Text kinds are UTF-8 (svg with
/// an `<svg` element; json, excalidraw and gltf JSON that parses); images,
/// pdf and glb start with their signature. A kind Neko does not know is kept
/// as it is: it is only offered as a download, or to a viewer that claims it.
pub fn check_contents(kind: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() { return Err("The file is empty.".into()); }
    let text = || std::str::from_utf8(bytes).map_err(|_| format!("A {kind} artifact must be UTF-8 text."));
    let signature = |ok: bool, what: &str| if ok { Ok(()) } else { Err(format!("The file is not {what}.")) };
    match kind {
        "svg" => if text()?.contains("<svg") { Ok(()) } else { Err("The file has no <svg> element.".into()) },
        "json" | "excalidraw" | "gltf" => serde_json::from_str::<serde_json::Value>(text()?).map(|_| ()).map_err(|e| format!("The file is not valid JSON ({e}).")),
        "png" => signature(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "a PNG image"),
        "jpg" => signature(bytes.starts_with(b"\xff\xd8\xff"), "a JPEG image"),
        "gif" => signature(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"), "a GIF image"),
        "webp" => signature(bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP", "a WebP image"),
        "pdf" => signature(bytes.starts_with(b"%PDF-"), "a PDF"),
        "glb" => signature(bytes.len() >= 20 && bytes.starts_with(b"glTF"), "a binary glTF (.glb)"),
        kind if is_text(kind) => text().map(|_| ()),
        _ => Ok(()),
    }
}

const TOO_BIG: &str = "Keep an artifact under 5 MB.";

/// Reads the file to keep, refusing anything that is not a regular file with
/// a kind, under [`MAX_BYTES`], whose contents match it. Reads at most one
/// byte past the limit, so a file that grows while it is read is refused too.
fn read_source(file: &Path) -> Result<(String, Vec<u8>), String> {
    if !file.is_absolute() { return Err("Give the file as an absolute path.".into()); }
    let kind = kind_of(file).ok_or("Give the file an extension that says what it is (.html, .md, .csv, .glb, .excalidraw…).")?;
    let meta = fs::metadata(file).map_err(|e| format!("Cannot read {}: {e}", file.display()))?;
    if !meta.is_file() { return Err(format!("{} is not a file.", file.display())); }
    if meta.len() > MAX_BYTES { return Err(TOO_BIG.into()); }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    fs::File::open(file)
        .and_then(|f| f.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("Cannot read {}: {e}", file.display()))?;
    if bytes.len() as u64 > MAX_BYTES { return Err(TOO_BIG.into()); }
    check_contents(&kind, &bytes)?;
    Ok((kind, bytes))
}

/// Writes beside the target, then renames over it: a reader never sees half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = fs::File::create(&tmp).and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all())).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() { let _ = fs::remove_file(&tmp); }
    result.map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

fn folder(root: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id).then(|| root.join(id))
}

fn read_meta(dir: &Path) -> Option<Artifact> {
    let text = fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_meta(dir: &Path, artifact: &Artifact) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(artifact).map_err(|e| e.to_string())?;
    write_atomic(&dir.join("meta.json"), &json)
}

fn new_artifact_id(root: &Path) -> String {
    loop {
        let id: String = crate::workbench::new_id().chars().filter(char::is_ascii_alphanumeric).take(12).collect();
        if valid_id(&id) && !root.join(&id).exists() { return id; }
    }
}

/// Keeps `save.file` as a new artifact, or as the next version of
/// `save.artifact_id`. Returns the record and the version just kept.
pub fn save(root: &Path, save: Save) -> Result<(Artifact, u32), String> {
    let (kind, bytes) = read_source(&save.file)?;
    keep(root, save, &kind, &bytes)
}

/// Keeps `text` as the next version of `artifact_id`, in the kind of its
/// newest version: how a viewer that edits (Excalidraw) saves. Only text
/// kinds, and the same checks and cap as a file.
pub fn save_text(root: &Path, artifact_id: &str, text: &str, note: &str) -> Result<(Artifact, u32), String> {
    let artifact = get(root, artifact_id).ok_or_else(|| format!("There is no artifact {artifact_id}."))?;
    let kind = artifact.versions.iter().max_by_key(|v| v.n).and_then(|v| kind_of(Path::new(&v.file))).unwrap_or(artifact.kind.clone());
    if !is_text(&kind) { return Err(format!("A {kind} artifact cannot be saved as text.")); }
    if text.len() as u64 > MAX_BYTES { return Err(TOO_BIG.into()); }
    check_contents(&kind, text.as_bytes())?;
    keep(root, Save { artifact_id: Some(artifact_id.to_string()), note: note.to_string(), ..Save::default() }, &kind, text.as_bytes())
}

fn keep(root: &Path, save: Save, kind: &str, bytes: &[u8]) -> Result<(Artifact, u32), String> {
    let title = shorten(&save.title, MAX_TITLE_CHARS);
    let note = shorten(&save.note, MAX_NOTE_CHARS);
    let workspace_id = save.workspace_id.filter(|w| !w.is_empty());
    if workspace_id.as_deref().is_some_and(|w| !valid_id(w)) { return Err("The workspace id is not valid.".into()); }
    let message_id = save.message_id.filter(|m| valid_id(m));
    let _guard = lock();
    fs::create_dir_all(root).map_err(|e| format!("Cannot create {}: {e}", root.display()))?;
    let (dir, mut artifact) = match save.artifact_id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => {
            let dir = folder(root, id).ok_or("The artifact id is not valid.")?;
            let artifact = read_meta(&dir).ok_or_else(|| format!("There is no artifact {id}."))?;
            (dir, artifact)
        }
        None => {
            if title.is_empty() { return Err("Give the artifact a title.".into()); }
            let id = new_artifact_id(root);
            let dir = root.join(&id);
            fs::create_dir(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
            (dir, Artifact { id, title: title.clone(), kind: kind.into(), workspace_id: workspace_id.clone(), message_id: message_id.clone(), versions: Vec::new() })
        }
    };
    let n = artifact.versions.iter().map(|v| v.n).max().unwrap_or(0) + 1;
    if n > MAX_VERSIONS { return Err(format!("{} already has {MAX_VERSIONS} versions; save a new artifact instead.", artifact.title)); }
    let file = format!("v{n}.{kind}");
    write_atomic(&dir.join(&file), bytes)?;
    if !title.is_empty() { artifact.title = title; }
    artifact.kind = kind.into();
    if artifact.workspace_id.is_none() { artifact.workspace_id = workspace_id; }
    if artifact.message_id.is_none() { artifact.message_id = message_id.clone(); }
    artifact.versions.push(ArtifactVersion { n, file, at_ms: crate::now_unix_ms(), note, message_id });
    write_meta(&dir, &artifact)?;
    Ok((artifact, n))
}

/// When an artifact last changed (its newest version).
pub fn updated_ms(artifact: &Artifact) -> i64 {
    artifact.versions.iter().map(|v| v.at_ms).max().unwrap_or(0)
}

/// Every readable artifact, newest first; only one workspace's when asked.
pub fn list(root: &Path, workspace_id: Option<&str>) -> Vec<Artifact> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<Artifact> = entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(valid_id) && e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| read_meta(&e.path()).filter(|a| e.file_name().to_str() == Some(a.id.as_str())))
        .filter(|a| workspace_id.is_none_or(|w| a.workspace_id.as_deref() == Some(w)))
        .collect();
    out.sort_by(|a, b| updated_ms(b).cmp(&updated_ms(a)).then_with(|| a.id.cmp(&b.id)));
    out
}

pub fn get(root: &Path, id: &str) -> Option<Artifact> {
    let dir = folder(root, id)?;
    read_meta(&dir).filter(|a| a.id == id)
}

/// The file of version `n` (the latest for 0) and its kind, proved to be a
/// file inside the artifact's own folder.
pub fn version_file(root: &Path, id: &str, n: u32) -> Option<(PathBuf, String)> {
    let artifact = get(root, id)?;
    let version = if n == 0 { artifact.versions.iter().max_by_key(|v| v.n)? } else { artifact.versions.iter().find(|v| v.n == n)? };
    let name = Path::new(&version.file);
    if name.components().count() != 1 || version.file.starts_with('.') { return None; }
    let kind = kind_of(name)?;
    let dir = root.join(id).canonicalize().ok()?;
    let file = dir.join(name).canonicalize().ok()?;
    (file.starts_with(&dir) && file.is_file()).then_some((file, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(dir: &Path, name: &str, body: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    fn new(file: PathBuf, title: &str) -> Save {
        Save { title: title.into(), file, ..Save::default() }
    }

    /// The smallest valid binary glTF: a header and an empty JSON chunk.
    fn tiny_glb() -> Vec<u8> {
        let json = br#"{"asset":{"version":"2.0"}}   "#;
        let mut out = Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(12 + 8 + json.len() as u32).to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(json);
        out
    }

    #[test]
    fn a_saved_file_is_copied_in_and_versions_count_up() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("artifacts");
        let page = source(tmp.path(), "page.html", b"<html><body>one</body></html>");
        let (first, n) = save(&root, Save { workspace_id: Some("ws1".into()), message_id: Some("m1".into()), note: "first".into(), ..new(page.clone(), "Onboarding") }).unwrap();
        assert_eq!(n, 1);
        assert_eq!((first.kind.as_str(), first.workspace_id.as_deref(), first.message_id.as_deref()), ("html", Some("ws1"), Some("m1")));
        // The store keeps its own copy: changing the source changes nothing kept.
        fs::write(&page, "<html><body>two</body></html>").unwrap();
        let (file, kind) = version_file(&root, &first.id, 1).unwrap();
        assert_eq!((fs::read_to_string(&file).unwrap().as_str(), kind.as_str()), ("<html><body>one</body></html>", "html"));
        let (second, n) = save(&root, Save { artifact_id: Some(first.id.clone()), message_id: Some("m2".into()), ..new(page, "") }).unwrap();
        assert_eq!(n, 2);
        assert_eq!(second.title, "Onboarding", "an empty title keeps the old one");
        assert_eq!(second.message_id.as_deref(), Some("m1"), "the reply that made it stays");
        assert_eq!(second.versions[1].message_id.as_deref(), Some("m2"));
        assert_eq!(fs::read_to_string(version_file(&root, &first.id, 2).unwrap().0).unwrap(), "<html><body>two</body></html>");
        assert_eq!(version_file(&root, &first.id, 0).unwrap().0, version_file(&root, &first.id, 2).unwrap().0, "0 is the latest");
        assert!(version_file(&root, &first.id, 3).is_none());
        assert_eq!(get(&root, &first.id).unwrap(), second);
    }

    #[test]
    fn listing_is_newest_first_and_filters_by_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("artifacts");
        assert!(list(&root, None).is_empty(), "no folder yet is an empty list");
        let svg = source(tmp.path(), "a.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>");
        let (a, _) = save(&root, Save { workspace_id: Some("one".into()), ..new(svg.clone(), "A") }).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let (b, _) = save(&root, Save { workspace_id: Some("two".into()), ..new(svg, "B") }).unwrap();
        fs::create_dir_all(root.join("junk")).unwrap();
        fs::write(root.join("junk/meta.json"), "not json").unwrap();
        fs::write(root.join("stray.txt"), "x").unwrap();
        let all: Vec<_> = list(&root, None).into_iter().map(|x| x.id).collect();
        assert_eq!(all, vec![b.id.clone(), a.id.clone()]);
        assert_eq!(list(&root, Some("one")).into_iter().map(|x| x.id).collect::<Vec<_>>(), vec![a.id]);
    }

    #[test]
    fn any_kind_is_kept_under_its_normalised_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("artifacts");
        for (name, body, kind) in [
            ("model.glb", tiny_glb(), "glb"),
            ("scene.gltf", br#"{"asset":{"version":"2.0"}}"#.to_vec(), "gltf"),
            ("part.STL", b"solid x\nendsolid x\n".to_vec(), "stl"),
            ("sketch.excalidraw", br#"{"type":"excalidraw","version":2,"elements":[]}"#.to_vec(), "excalidraw"),
            ("table.csv", b"a,b\n1,2\n".to_vec(), "csv"),
            ("notes.markdown", b"# Notes".to_vec(), "md"),
            ("photo.JPEG", b"\xff\xd8\xff\xe0rest".to_vec(), "jpg"),
            ("run.sh", b"echo hi".to_vec(), "sh"),
            ("data.parquet", b"PAR1\x00\x01".to_vec(), "parquet"),
        ] {
            let (artifact, _) = save(&root, new(source(tmp.path(), name, &body), name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(artifact.kind, kind, "{name}");
            assert_eq!(artifact.versions[0].file, format!("v1.{kind}"));
            let (file, served) = version_file(&root, &artifact.id, 1).unwrap();
            assert_eq!((served.as_str(), fs::read(file).unwrap()), (kind, body));
        }
    }

    #[test]
    fn every_kind_has_an_inert_type_except_html() {
        assert_eq!(mime("html"), "text/html; charset=utf-8");
        assert_eq!(mime("glb"), "model/gltf-binary");
        assert_eq!(mime("csv"), "text/csv; charset=utf-8");
        assert_eq!(mime("md"), "text/markdown; charset=utf-8");
        assert_eq!(mime("excalidraw"), "application/json");
        assert_eq!(mime("jpg"), "image/jpeg");
        // Code and markup that a browser could run are served as plain text.
        for kind in ["js", "xml", "sh", "py", "mjs"] { assert_eq!(mime(kind), "text/plain; charset=utf-8", "{kind}"); }
        for kind in ["xhtml", "parquet", "exe", "zip"] { assert_eq!(mime(kind), "application/octet-stream", "{kind}"); }
        assert!(TEXT_KINDS.iter().all(|k| !mime(k).starts_with("text/html") || *k == "html"));
    }

    #[test]
    fn refuses_files_that_are_too_big_without_a_kind_or_not_what_they_claim() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("artifacts");
        let big = source(tmp.path(), "big.glb", &vec![b'a'; MAX_BYTES as usize + 1]);
        assert!(save(&root, new(big, "Big")).unwrap_err().contains("5 MB"));
        let exactly = source(tmp.path(), "max.md", &vec![b'a'; MAX_BYTES as usize]);
        assert!(save(&root, new(exactly, "Max")).is_ok(), "5 MB itself is allowed");
        for (name, body) in [
            ("noext", b"<html>".as_slice()),
            ("odd.ex-t", b"x"),
            ("long.abcdefghijklmnopq", b"x"),
            ("fake.png", b"<html>not a png"),
            ("fake.pdf", b"hello"),
            ("fake.glb", b"not a gltf binary at all"),
            ("fake.jpg", b"GIF89a"),
            ("bad.json", b"{nope"),
            ("bad.excalidraw", b"{nope"),
            ("bad.svg", b"<html></html>"),
            ("binary.html", b"\xff\xfe\x00"),
            ("binary.csv", b"\xff\xfe\x00"),
            ("empty.html", b""),
            ("empty.bin", b""),
        ] {
            assert!(save(&root, new(source(tmp.path(), name, body), "X")).is_err(), "{name} must be refused");
        }
        assert!(save(&root, new(PathBuf::from("relative.html"), "X")).is_err(), "a relative path");
        assert!(save(&root, new(tmp.path().to_path_buf().join("missing.html"), "X")).is_err());
        let dir = tmp.path().join("folder.html");
        fs::create_dir(&dir).unwrap();
        assert!(save(&root, new(dir, "X")).is_err(), "a folder is not a file");
        let ok = source(tmp.path(), "ok.png", b"\x89PNG\r\n\x1a\nrest");
        assert!(save(&root, new(ok.clone(), "")).unwrap_err().contains("title"), "a new artifact needs a title");
        assert!(save(&root, Save { artifact_id: Some("nope".into()), ..new(ok.clone(), "X") }).unwrap_err().contains("no artifact"));
        assert!(save(&root, Save { workspace_id: Some("../x".into()), ..new(ok, "X") }).is_err());
    }

    #[test]
    fn a_viewer_saves_text_as_the_next_version_of_a_text_kind_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("artifacts");
        let sketch = source(tmp.path(), "s.excalidraw", br#"{"type":"excalidraw","elements":[]}"#);
        let (artifact, _) = save(&root, new(sketch, "Sketch")).unwrap();
        let (edited, n) = save_text(&root, &artifact.id, r#"{"type":"excalidraw","elements":[{"id":"a"}]}"#, "Edited in Neko").unwrap();
        assert_eq!((n, edited.kind.as_str(), edited.versions[1].file.as_str(), edited.versions[1].note.as_str()), (2, "excalidraw", "v2.excalidraw", "Edited in Neko"));
        assert_eq!(edited.title, "Sketch");
        assert!(save_text(&root, &artifact.id, "{broken", "").is_err(), "the kind's checks still apply");
        assert!(save_text(&root, "missing", "{}", "").is_err());
        assert!(save_text(&root, "../x", "{}", "").is_err());
        let (model, _) = save(&root, new(source(tmp.path(), "m.glb", &tiny_glb()), "Model")).unwrap();
        assert!(save_text(&root, &model.id, "text", "").unwrap_err().contains("cannot be saved as text"));
        assert!(save_text(&root, &artifact.id, &"a".repeat(MAX_BYTES as usize + 1), "").unwrap_err().contains("5 MB"));
    }

    #[test]
    fn ids_and_stored_names_cannot_lead_out_of_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("artifacts");
        fs::write(tmp.path().join("secret.html"), "secret").unwrap();
        for id in ["..", "../x", "a/b", ".hidden", "", &"a".repeat(65)] {
            assert!(get(&root, id).is_none() && version_file(&root, id, 1).is_none(), "{id:?}");
            let file = source(tmp.path(), "v.html", b"<p>");
            assert!(save(&root, Save { artifact_id: Some(id.to_string()), ..new(file, "X") }).is_err() || id.is_empty(), "{id:?}");
        }
        // A meta.json edited to name a file outside the folder is not followed.
        let page = source(tmp.path(), "page.html", b"<p>ok</p>");
        let (artifact, _) = save(&root, new(page, "T")).unwrap();
        let dir = root.join(&artifact.id);
        for evil in ["../../secret.html", "/etc/hosts", ".v1.html", "v1"] {
            let mut meta = artifact.clone();
            meta.versions[0].file = evil.into();
            fs::write(dir.join("meta.json"), serde_json::to_vec(&meta).unwrap()).unwrap();
            assert!(version_file(&root, &artifact.id, 1).is_none(), "{evil}");
        }
        // Nor a symlink planted in the folder.
        let mut meta = artifact.clone();
        meta.versions[0].file = "v9.html".into();
        fs::write(dir.join("meta.json"), serde_json::to_vec(&meta).unwrap()).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("secret.html"), dir.join("v9.html")).unwrap();
        assert!(version_file(&root, &artifact.id, 1).is_none());
    }

    #[test]
    fn titles_and_notes_are_bounded() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("artifacts");
        let page = source(tmp.path(), "page.HTM", b"<p>");
        let (artifact, _) = save(&root, Save { note: "n".repeat(2000), ..new(page, &"t".repeat(500)) }).unwrap();
        assert_eq!(artifact.title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!(artifact.versions[0].note.chars().count(), MAX_NOTE_CHARS);
        assert_eq!(artifact.versions[0].file, "v1.html", "htm is kept as html");
    }
}
