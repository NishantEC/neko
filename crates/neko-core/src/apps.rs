//! Installed-application discovery: walk the standard macOS application
//! directories, read each bundle's `Info.plist` for a display name and
//! bundle id.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// The bundle id when the app has one, otherwise the `.app` path as a
    /// string — either way, stable across a single scan and usable as the
    /// protocol `SearchItem::id` / `launches.app_id`.
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

/// The standard places macOS keeps application bundles, in priority order
/// (earlier entries win on a name collision).
pub fn application_directories() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Applications"));
    }
    dirs
}

/// Scan the standard application directories (non-recursive into nested
/// `.app` bundles, one level of subfolder inside e.g. `/Applications/Utilities`).
pub fn scan_applications() -> Vec<AppEntry> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut entries = Vec::new();
    for dir in application_directories() {
        scan_dir(&dir, 0, &mut seen_ids, &mut entries);
    }
    entries
}

fn scan_dir(
    dir: &Path,
    depth: u8,
    seen_ids: &mut std::collections::HashSet<String>,
    out: &mut Vec<AppEntry>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "app") {
            if let Some(app) = read_app_bundle(&path)
                && seen_ids.insert(app.id.clone())
            {
                out.push(app);
            }
        } else if depth < 1 && path.is_dir() {
            scan_dir(&path, depth + 1, seen_ids, out);
        }
    }
}

fn read_app_bundle(path: &Path) -> Option<AppEntry> {
    let plist_path = path.join("Contents/Info.plist");
    let value: plist::Value = plist::from_file(&plist_path).ok()?;
    let dict = value.as_dictionary()?;

    let name = dict
        .get("CFBundleDisplayName")
        .or_else(|| dict.get("CFBundleName"))
        .and_then(|v| v.as_string())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });

    let id = dict
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .map(str::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    Some(AppEntry {
        id,
        name,
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_directories_include_the_standard_macos_locations() {
        let dirs = application_directories();
        assert!(dirs.contains(&PathBuf::from("/Applications")));
        assert!(dirs.contains(&PathBuf::from("/System/Applications")));
    }

    #[test]
    fn scanning_the_real_machine_finds_at_least_finder_or_safari() {
        // A real, non-mocked scan — this is the acceptance criterion
        // ("real applications, ranked sensibly"), so the smoke test reads
        // the real filesystem rather than a fixture.
        let apps = scan_applications();
        assert!(
            apps.iter().any(|a| a.name.contains("Safari")
                || a.name.contains("Finder")
                || a.name.contains("System Settings")),
            "expected to find at least one well-known app on this machine, found: {:?}",
            apps.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
    }
}
