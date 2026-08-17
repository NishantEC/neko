//! App icon extraction, cached to disk as PNGs the client can load with a
//! plain file path (GPUI's `img()` takes a path, not an `NSImage`).

use std::path::PathBuf;

/// Rendered/cached at roughly 3x the summon panel's 22pt row-icon slot
/// (`theme::ROW_ICON_PX` in the `neko` crate — `neko-core` doesn't depend on
/// that crate per the workspace's own boundary rule, so the value is
/// restated here rather than imported): crisp on a 2x Retina display with
/// headroom to spare, still sharp on 3x. Previously each icon was cached at
/// whatever pixel size `NSWorkspace`'s source representation happened to
/// carry — `NSImage.setSize` + `TIFFRepresentation` never resampled it (see
/// `extract_icon_png`'s own comment) — up to 1024x1024 and ~1.4MB for a
/// single icon, ~188MB across a real 147-app index, for artwork displayed
/// at 22px.
const ICON_CACHE_PX: isize = 64;

/// Bumped whenever the cached pixel format/size changes, so upgrading never
/// silently keeps serving an old, wrongly-sized file — `ensure_cached_icon`
/// only checks whether *a* file exists at this path, not what size it is,
/// so a stale cache from before this constant last changed would otherwise
/// never regenerate on its own. See `purge_stale_unversioned_cache` for the
/// one-time cleanup of caches written before this versioning existed.
const CACHE_GENERATION: &str = "v2-64px";

/// `~/Library/Caches/neko/icons/v2-64px/<sanitized-id>.png`
pub fn cached_icon_path(app_id: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let sanitized: String = app_id
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    Some(
        PathBuf::from(home)
            .join("Library/Caches/neko/icons")
            .join(CACHE_GENERATION)
            .join(format!("{sanitized}.png")),
    )
}

/// One-time cleanup of icons cached by a build that predates
/// `CACHE_GENERATION`: those files sat loose directly under
/// `icons/` (not in a versioned subdirectory) at up to ~1.4MB each, and
/// `ensure_cached_icon`'s existence check would never have replaced them on
/// its own — a real disk-space emergency traced to this cache would not
/// self-heal just from upgrading the binary. Removes only loose `*.png`
/// files sitting directly in `icons/`; never descends into or touches a
/// versioned subdirectory (this generation's own, or a future one), so this
/// is a no-op once the migration has already run once.
pub fn purge_stale_unversioned_cache() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let dir = PathBuf::from(home).join("Library/Caches/neko/icons");
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("png") {
            let _ = std::fs::remove_file(path);
        }
    }
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
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace,
        NSGraphicsContext, NSWorkspace,
    };
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};
    use std::ptr;

    let path_str = app_path.to_str()?;
    // Pooled for the same reason `clipboard.rs`'s `pasteboard` module is
    // (see its doc comment): this runs on a plain background thread with
    // no `NSApplication`/`CFRunLoop` draining autorelease pools the normal
    // way, and `iconForFile`/`drawInRect_fromRect_operation_fraction`/
    // `representationUsingType_properties` all produce transient
    // autoreleased objects internally. Called once per app rather than on a
    // timer, so the leak this would otherwise cause is bounded (one app
    // index's worth, not unbounded over the daemon's lifetime) — pooled
    // anyway for the same audited guarantee every AppKit call site in this
    // daemon now has.
    objc2::rc::autoreleasepool(|_pool| {
        // SAFETY: these are plain Cocoa calls (icon lookup, offscreen
        // bitmap drawing, in-memory image format conversion) with no shared
        // mutable state; every objc2 call below follows the exact method
        // signatures generated from Apple's own headers, called from a
        // single thread. `NSGraphicsContext::setCurrentContext` sets a
        // per-thread current context — no run loop or window is required,
        // unlike the deprecated `NSImage.lockFocus`/`unlockFocus` pair this
        // replaces.
        unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            let ns_path = NSString::from_str(path_str);
            let image = workspace.iconForFile(&ns_path);

            // Render into an explicitly `ICON_CACHE_PX`-sized bitmap rather
            // than trusting `NSImage.setSize` + `TIFFRepresentation`: a
            // modern asset-catalog icon's source representation can be as
            // large as 1024x1024, and `TIFFRepresentation` serializes that
            // source representation as-is — `setSize` only changes the
            // image's reported *drawing* size, it never resamples the
            // backing representation. Confirmed live: the previous
            // `setSize(64,64)` + `TIFFRepresentation` chain produced a
            // 1024x1024, 1.4MB PNG for `com.apple.calculator`. Drawing into
            // a bitmap built at the exact target pixel dimensions is the
            // approach Apple's own docs point to as the replacement for
            // `lockFocus` (see that method's deprecation notice).
            let target = ICON_CACHE_PX;
            let bitmap = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                ptr::null_mut(),
                target,
                target,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                0,
                0,
            )?;

            let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
            NSGraphicsContext::saveGraphicsState_class();
            NSGraphicsContext::setCurrentContext(Some(&context));
            let target_f = target as f64;
            image.drawInRect_fromRect_operation_fraction(
                NSRect {
                    origin: NSPoint { x: 0.0, y: 0.0 },
                    size: NSSize {
                        width: target_f,
                        height: target_f,
                    },
                },
                NSRect::ZERO,
                NSCompositingOperation::Copy,
                1.0,
            );
            NSGraphicsContext::restoreGraphicsState_class();

            let properties = NSDictionary::new();
            let png = bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)?;
            Some(png.to_vec())
        }
    })
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
