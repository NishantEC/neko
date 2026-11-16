//! What the Rune client's embedded web view needs from the daemon: a signed-in address for a web
//! route (`SignedWebUrl`, the one `neko-daemon --open-page` opens), what the plugins' manifests
//! declare as viewers (`PluginViewers`), and the pages plugins registered (`PluginPages`, reported
//! by whichever web window loaded the plugins and kept across restarts).
//!
//! All read-only. `SignedWebUrl` carries the web token, so the HTTP bridge refuses it
//! (`web::api_request`): it is for the Unix socket, whose peer already owns the data folder.

use std::path::Path;

use neko_protocol::{PluginPage, PluginViewer};

/// The web UI's address for `route`, signed in with the token in `<dir>/web-token`:
/// `http://127.0.0.1:<port>/?token=…#/<route>`. `route` is the hash route the web app reads
/// (`artifact/<id>/<n>?embed=1`, `my-plugin:board?embed=1`); a leading `#/` is dropped and anything
/// that could name another site is refused (`neko_core::viewers::tab_route`).
pub fn signed_url(dir: &Path, port_env: Option<&str>, route: &str) -> Result<String, String> {
    let route = neko_core::viewers::tab_route(route).ok_or("That is not a route in Neko's web app.")?;
    let port = match port_env.map(str::trim).map(str::parse::<u16>) {
        Some(Ok(0)) => return Err("Neko's web interface is switched off (NEKO_WEB_PORT=0).".into()),
        Some(Ok(port)) => port,
        _ => crate::web::DEFAULT_PORT,
    };
    let token = std::fs::read_to_string(dir.join("web-token")).unwrap_or_default();
    let token = token.trim();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Neko's web interface has not published its address yet; is the daemon running?".into());
    }
    Ok(format!("http://127.0.0.1:{port}/?token={token}#/{route}"))
}

/// The viewers every enabled plugin's manifest declares, plugin by plugin.
pub fn plugin_viewers(plugins: &Path) -> Vec<PluginViewer> {
    neko_core::viewers::declared(plugins)
        .into_iter()
        .map(|(plugin, v)| PluginViewer { plugin, viewer_id: v.id, kinds: v.kinds, title: v.title })
        .collect()
}

const MAX_PAGES: usize = 64;

fn plain(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'))
}

/// What a window reports is data: bounded, and only plain ids.
pub fn clean_pages(pages: Vec<PluginPage>) -> Vec<PluginPage> {
    let mut out: Vec<PluginPage> = Vec::new();
    for page in pages {
        if out.len() >= MAX_PAGES { break; }
        let id_ok = plain(&page.id, 96) && page.plugin.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) && !page.plugin.is_empty() && page.plugin.len() <= 64;
        if !id_ok || out.iter().any(|p| p.id == page.id) { continue; }
        let title: String = page.title.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
        out.push(PluginPage { title: if title.is_empty() { page.id.clone() } else { title }, ..page });
    }
    out.sort_by(|a, b| a.plugin.cmp(&b.plugin).then_with(|| a.id.cmp(&b.id)));
    out
}

fn file(dir: &Path) -> std::path::PathBuf {
    dir.join("plugin-pages.json")
}

/// The pages last reported, minus those of a plugin that is gone or switched off now.
pub fn load_pages(dir: &Path, plugins: &Path) -> Vec<PluginPage> {
    let saved: Vec<PluginPage> = std::fs::read_to_string(file(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    clean_pages(saved).into_iter().filter(|p| plugins.join(&p.plugin).is_dir() && !plugins.join(&p.plugin).join(".disabled").exists()).collect()
}

/// Keeps what a window reported, writing the file only when it changed.
pub fn store_pages(dir: &Path, pages: Vec<PluginPage>) -> Result<(), String> {
    let pages = clean_pages(pages);
    let json = serde_json::to_string_pretty(&pages).map_err(|e| e.to_string())?;
    if std::fs::read_to_string(file(dir)).ok().as_deref() == Some(json.as_str()) { return Ok(()); }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("plugin-pages.tmp-{}", std::process::id()));
    std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, file(dir))).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("web-token"), format!("{}\n", "ab".repeat(32))).unwrap();
        dir
    }

    #[test]
    fn the_signed_address_is_the_one_open_page_builds_for_a_route() {
        let dir = token_dir();
        let token = "ab".repeat(32);
        assert_eq!(signed_url(dir.path(), None, "artifact/abc123/2?embed=1").unwrap(), format!("http://127.0.0.1:4747/?token={token}#/artifact/abc123/2?embed=1"));
        assert_eq!(signed_url(dir.path(), Some("5000"), "#/my-plugin:board?embed=1&theme=dark").unwrap(), format!("http://127.0.0.1:5000/?token={token}#/my-plugin:board?embed=1&theme=dark"));
        assert!(signed_url(dir.path(), Some("0"), "tasks").is_err(), "the web interface is off");
    }

    #[test]
    fn a_route_that_could_name_another_site_or_leave_the_app_is_refused() {
        let dir = token_dir();
        for bad in ["https://evil.example", "//evil.example", "javascript:alert(1)", "../x", "a b", "tasks\nx", ""] {
            assert!(signed_url(dir.path(), None, bad).is_err() || bad.is_empty(), "{bad:?}");
        }
        assert!(signed_url(dir.path(), None, "http:evil").is_err());
        // No token yet: it says so rather than make an address that cannot sign in.
        let empty = tempfile::tempdir().unwrap();
        assert!(signed_url(empty.path(), None, "tasks").unwrap_err().contains("has not published"));
        std::fs::write(empty.path().join("web-token"), "short").unwrap();
        assert!(signed_url(empty.path(), None, "tasks").is_err());
    }

    #[test]
    fn plugin_viewers_come_from_the_enabled_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let make = |name: &str, manifest: &str, disabled: bool| {
            let p = dir.path().join(name);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("manifest.json"), manifest).unwrap();
            if disabled { std::fs::write(p.join(".disabled"), "").unwrap(); }
        };
        make("stl", r#"{"title":"STL","viewers":[{"id":"stl","title":"STL viewer","kinds":["stl",".OBJ"]}]}"#, false);
        make("off", r#"{"viewers":[{"id":"x","kinds":["xyz"]}]}"#, true);
        make("none", r#"{"title":"No viewers"}"#, false);
        make("bad", "not json", false);
        let viewers = plugin_viewers(dir.path());
        assert_eq!(viewers, vec![PluginViewer { plugin: "stl".into(), viewer_id: "stl".into(), kinds: vec!["stl".into(), "obj".into()], title: "STL viewer".into() }]);
    }

    #[test]
    fn reported_pages_are_cleaned_kept_and_dropped_with_their_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        std::fs::create_dir_all(plugins.join("board")).unwrap();
        std::fs::create_dir_all(plugins.join("gone-soon")).unwrap();
        let page = |plugin: &str, id: &str, title: &str| PluginPage { plugin: plugin.into(), id: id.into(), title: title.into() };
        store_pages(dir.path(), vec![
            page("gone-soon", "gone-soon:one", "One"),
            page("board", "board:main", "  Board \n page "),
            page("board", "board:main", "again"),
            page("board", "../evil", "no"),
            page("bad plugin", "x", "no"),
            page("board", "board:empty", ""),
        ]).unwrap();
        let pages = load_pages(dir.path(), &plugins);
        assert_eq!(pages.iter().map(|p| (p.plugin.as_str(), p.id.as_str(), p.title.as_str())).collect::<Vec<_>>(), vec![("board", "board:empty", "board:empty"), ("board", "board:main", "Board page"), ("gone-soon", "gone-soon:one", "One")]);
        std::fs::remove_dir_all(plugins.join("gone-soon")).unwrap();
        std::fs::write(plugins.join("board").join(".disabled"), "").unwrap();
        assert!(load_pages(dir.path(), &plugins).is_empty(), "a removed or switched-off plugin has no pages");
    }
}
