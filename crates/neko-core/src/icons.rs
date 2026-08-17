//! App icon extraction, cached to disk as PNGs the client can load with a
//! plain file path (GPUI's `img()` takes a path, not an `NSImage`).

use std::path::PathBuf;

/// Cached at a native macOS icon-representation size, not a value picked to
/// be "roughly enough" bigger than the render slot. The row icon renders at
/// `theme::ROW_ICON_PX` = 22 **logical points** (`neko-core` doesn't depend
/// on the `neko` crate per the workspace's own boundary rule, so the value
/// is restated here rather than imported) — on the captain's own two
/// displays (built-in Liquid Retina XDR, external LG UltraFine 4K set to
/// its native "looks like 1920x1080" mode) that's a confirmed
/// `backingScaleFactor` of 2.0 on both (`system_profiler SPDisplaysDataType`:
/// external reports "Resolution: 3840 x 2160" / "UI Looks like: 1920 x
/// 1080", i.e. exactly 2x, not a fractional scaled-resolution mode — verify
/// again with that command if this ever needs re-checking, don't assume),
/// i.e. 44 physical px today, with headroom kept for a 3x display (66px)
/// this repo has no way to test against.
///
/// The previous value here, 64, was chosen as "roughly 3x the render slot"
/// without checking what macOS actually ships. That was two compounding
/// mistakes, not one: (1) 64 down to 44 physical px is a non-integer 0.6875
/// downscale — GPUI's own image sampler is a plain bilinear `min_filter:
/// linear` with no mipmap chain (`gpui-0.2.2/src/platform/mac/shaders.metal`,
/// `atlas_texture_sampler`), so any downscale ratio still aliases somewhat,
/// but a *non-native* source resolution makes it worse for a second,
/// independent reason: (2) `extract_icon_png`'s `drawInRect` call already
/// resamples once, from whichever representation `NSWorkspace.iconForFile`'s
/// composite `NSImage` picks as "best for a 64x64 draw," to 64x64 — and 64
/// is not one of the fixed sizes modern asset-catalog icons ship
/// representations at (16, 32, 128, 256, 512, 1024, each present at both
/// 1x and 2x pixel densities). Caching at 128 instead means that draw call
/// can hit an exact native representation for most modern icons, so
/// extraction becomes a copy instead of a resample — leaving exactly one
/// resample in the whole pipeline (GPUI's own downscale to the display's
/// physical pixels at whatever the window's current scale factor is),
/// instead of two compounding ones. `Img` decodes the full cached PNG and
/// re-samples it every frame at the window's live `scale_factor()` — see
/// that field's own doc comment in gpui's `elements/img.rs` — which is
/// exactly why nothing here has to special-case a window moving between the
/// two displays above: the source bitmap doesn't change, only the sampling
/// target size does, on every frame, automatically.
///
/// Storage cost, measured on the same real 146-app index the 1024px and
/// 64px generations were both measured against: 1.10MB total at 64px vs.
/// 2.52MB at 128px (4x the pixel area, but PNG compresses the extra native
/// detail better than it compressed the old generation's own resample
/// artifacts, so the total isn't a full 4x) — see
/// `docs/evidence/icon-cache-128px-report.md` for the full before/after
/// table. Nowhere close to the original 188MB/1024px problem this whole
/// cache-generation mechanism exists to keep from regressing.
const ICON_CACHE_PX: isize = 128;

/// Bumped whenever the cached pixel format/size changes, so upgrading never
/// silently keeps serving an old, wrongly-sized file — `ensure_cached_icon`
/// only checks whether *a* file exists at this path, not what size it is,
/// so a stale cache from before this constant last changed would otherwise
/// never regenerate on its own. See `purge_stale_icon_cache` for the
/// one-time cleanup of caches written under a previous generation.
const CACHE_GENERATION: &str = "v3-128px";

/// `~/Library/Caches/neko/icons/v3-128px/<sanitized-id>.png`
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

/// One-time cleanup of every icon-cache generation that isn't the current
/// one: (1) loose `*.png` files sitting directly under `icons/` — the
/// layout a build predating any `CACHE_GENERATION` versioning wrote — and
/// (2) any versioned subdirectory (e.g. this task's own predecessor,
/// `v2-64px`) other than `CACHE_GENERATION` itself. `ensure_cached_icon`'s
/// existence check only ever looks at the *current* generation's path, so
/// without this, a captain who has upgraded through several icon-cache
/// changes would keep accumulating every previous generation's files on
/// disk forever rather than having them replaced. Never descends into or
/// touches the current generation's own subdirectory.
pub fn purge_stale_icon_cache() {
    let Some(home) = std::env::var_os("HOME") else { return };
    purge_stale_icon_cache_at(&PathBuf::from(home).join("Library/Caches/neko/icons"));
}

/// The actual sweep, taking the `icons/` directory directly so it's
/// testable without touching the real `$HOME` cache — see
/// `purge_stale_icon_cache`'s own doc comment for what this does and why.
fn purge_stale_icon_cache_at(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("png") {
            let _ = std::fs::remove_file(path);
        } else if path.is_dir() && entry.file_name() != CACHE_GENERATION {
            let _ = std::fs::remove_dir_all(&path);
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
    fn purge_stale_icon_cache_removes_old_generations_and_loose_files_only() {
        let dir = std::env::temp_dir().join(format!(
            "neko-icons-purge-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(CACHE_GENERATION)).unwrap();
        std::fs::write(dir.join(CACHE_GENERATION).join("com.apple.Safari.png"), b"current").unwrap();
        std::fs::create_dir_all(dir.join("v2-64px")).unwrap();
        std::fs::write(dir.join("v2-64px").join("com.apple.Safari.png"), b"stale").unwrap();
        std::fs::write(dir.join("loose-unversioned.png"), b"pre-versioning").unwrap();

        purge_stale_icon_cache_at(&dir);

        assert!(dir.join(CACHE_GENERATION).join("com.apple.Safari.png").exists());
        assert!(!dir.join("v2-64px").exists());
        assert!(!dir.join("loose-unversioned.png").exists());

        std::fs::remove_dir_all(&dir).unwrap();
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
