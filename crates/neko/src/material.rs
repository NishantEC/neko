//! The window material seam. Captain's ruling (launch brief): target real
//! system material (`NSVisualEffectMaterial.hudWindow`), not an invented
//! one — but a parallel spike (`neko-native-material`) is the one doing the
//! `objc2` `NSVisualEffectView` bridging work under a live GPUI window.
//! This function is the one line that swap gets to change: today it's
//! GPUI's own native primitive for window-level vibrancy (design report
//! §5, "Direction 1 — System Vibrancy", confirmed real and shipping,
//! `WindowBackgroundAppearance::Blurred`); once the spike lands its
//! recommended code path, this is where it plugs in.

use gpui::WindowBackgroundAppearance;

pub fn window_background() -> WindowBackgroundAppearance {
    WindowBackgroundAppearance::Blurred
}
