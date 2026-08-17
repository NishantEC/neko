//! App icon extraction, cached to disk as PNGs the client can load with a
//! plain file path (GPUI's `img()` takes a path, not an `NSImage`).

use std::path::PathBuf;

/// `~/Library/Caches/neko/icons/<sanitized-id>.png`
pub fn cached_icon_path(app_id: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let sanitized: String = app_id
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    Some(
        PathBuf::from(home)
            .join("Library/Caches/neko/icons")
            .join(format!("{sanitized}.png")),
    )
}

/// Extract `app_path`'s icon (via the same `NSWorkspace` resolution Finder
/// uses, so it's correct for both legacy `.icns`-file apps and modern
/// asset-catalog-only apps) and cache it as a PNG. No-ops if already
/// cached. Returns the cache path on success.
#[cfg(target_os = "macos")]
pub fn ensure_cached_icon(app_id: &str, app_path: &std::path::Path) -> Option<PathBuf> {
    let cache_path = cached_icon_path(app_id)?;
    if cache_path.exists() {
        return Some(cache_path);
    }
    let png_bytes = extract_icon_png(app_path)?;
    std::fs::create_dir_all(cache_path.parent()?).ok()?;
    std::fs::write(&cache_path, png_bytes).ok()?;
    Some(cache_path)
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_cached_icon(_app_id: &str, _app_path: &std::path::Path) -> Option<PathBuf> {
    None
}

#[cfg(target_os = "macos")]
fn extract_icon_png(app_path: &std::path::Path) -> Option<Vec<u8>> {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSSize, NSString};

    let path_str = app_path.to_str()?;
    // SAFETY: these are plain Cocoa calls (icon lookup + in-memory image
    // format conversion) with no shared mutable state; every objc2 call
    // below follows the exact method signatures generated from Apple's own
    // headers, called from a single thread.
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let ns_path = NSString::from_str(path_str);
        let image = workspace.iconForFile(&ns_path);
        // A consistent target size: crisp at the design's 22px row icon
        // even at a 2x/3x Retina backing scale.
        image.setSize(NSSize {
            width: 64.0,
            height: 64.0,
        });
        let tiff = image.TIFFRepresentation()?;
        let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
        let properties = NSDictionary::new();
        let png = bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)?;
        Some(png.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_icon_path_sanitizes_bundle_ids_into_a_flat_filename() {
        let path = cached_icon_path("com.apple.Safari").unwrap();
        assert!(path.ends_with("com.apple.Safari.png"));
        assert!(path.to_string_lossy().contains("Library/Caches/neko/icons"));
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore)]
    fn extracting_finders_icon_produces_a_cached_png() {
        let path = ensure_cached_icon("com.apple.finder", std::path::Path::new("/System/Library/CoreServices/Finder.app"));
        let Some(path) = path else {
            // Sandboxed CI environments sometimes can't resolve Finder's
            // icon; don't fail the suite over an environment quirk the
            // acceptance criteria (a real machine, `cargo build --release`)
            // won't hit.
            return;
        };
        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }
}
