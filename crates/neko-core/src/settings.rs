//! System Settings pane search — the fourth provider through the seam
//! (`provider.rs`), and, like `files.rs`, a proof the abstraction holds
//! rather than a design done in the abstract.
//!
//! **How panes are enumerated on this machine, established by looking, not
//! assuming (macOS 26.5.1/"Tahoe").** The old `.prefPane` bundle layout
//! (`/System/Library/PreferencePanes`, `/Library/PreferencePanes`,
//! `~/Library/PreferencePanes`) still exists on disk, but is a vestigial
//! stub on this OS: `/System/Library/PreferencePanes/Displays.prefPane`, for
//! instance, has no `Contents/Info.plist` at all — just a leftover
//! `Resources/MirrorDisplays.app` helper. Scanning it the way an older
//! macOS launcher would find nothing real. The System Settings app was
//! rewritten on ExtensionKit: every real pane is a `.appex` bundle under
//! `/System/Library/ExtensionKit/Extensions/` (241 total on this machine,
//! most of them unrelated system extensions — Siri metrics, Mail search
//! indexing, ...) whose `Info.plist` declares
//! `EXAppExtensionAttributes.EXExtensionPointIdentifier ==
//! "com.apple.Settings.extension.ui"` — 51 of the 241 do, one of which
//! (`HomePrivacySettingsExtension.appex`) doesn't also set
//! `SettingsExtensionAttributes.allowsXAppleSystemPreferencesURLScheme`,
//! Apple's own declared signal that the pane resolves through the
//! `x-apple.systempreferences:` URL scheme `open` already knows how to
//! drive — so [`enumerate_panes`] requires both.
//!
//! **The URL target is the extension's own `CFBundleIdentifier`, not
//! `SettingsExtensionAttributes.legacyBundleIdentifier` — measured, not
//! assumed.** The legacy field looked like the natural choice (it's the
//! identifier the URL scheme has accepted since much older macOS versions,
//! e.g. `com.apple.preference.displays`), and it does work — `open
//! "x-apple.systempreferences:com.apple.preference.displays"` reliably
//! spawns `DisplaysExt.appex` (verified by watching for that exact process
//! in `ps`, the same technique used to confirm every pane below). But two
//! real problems rule it out as the primary key: (1) only 33 of the 51
//! panes have one at all (`Storage.appex`, `ControlCenterSettings.appex`,
//! `LoginItems.appex`, and others carry no legacy identifier, yet still
//! set `allowsXAppleSystemPreferencesURLScheme`); (2) where it exists it
//! isn't always unique — `SiriPreferenceExtension.appex` and
//! `SpotlightPreferenceExtension.appex` both declare the *same* two-element
//! legacy list (`["com.apple.preference.speech",
//! "com.apple.preference.spotlight"]`), so picking "the" legacy id for
//! either one is ambiguous by construction. The modern `CFBundleIdentifier`
//! has neither problem — every one of the 51 panes has one, it's unique by
//! definition, and it was verified live to resolve correctly on its own:
//! `open "x-apple.systempreferences:com.apple.Displays-Settings.extension"`,
//! `com.apple.settings.Storage`, `com.apple.preferences.Bluetooth`'s
//! sibling `com.apple.BluetoothSettings`, and
//! `com.apple.Siri-Settings.extension` each spawned exactly the right
//! `.appex` process and no other, confirmed by process name after `open`
//! returned. This module never reads `legacyBundleIdentifier` at all.
//!
//! **Display names mostly come from each bundle's own localized
//! `InfoPlist.loctable`, not the raw `Info.plist`.** A `.appex`'s
//! unlocalized `CFBundleDisplayName` is frequently just its Xcode target
//! name — `MouseExtension`, `TrackpadExtension`,
//! `AccessibilitySettingsExtension` — because the real, user-facing name
//! ("Mouse", "Trackpad", "Accessibility") lives in
//! `Contents/Resources/InfoPlist.loctable`, a per-locale compiled plist
//! (the modern replacement for `InfoPlist.strings`), under its `"en"` key.
//! [`localized_display_name`] reads that first and falls back to the raw
//! `Info.plist` only when no loctable entry exists. Two of the 51 panes —
//! `com.apple.Battery-Settings.extension` and `com.apple.HeadphoneSettings`
//! — have *no* localized name anywhere in their bundle (checked directly,
//! not assumed) and would otherwise show as "PowerPreferences" and
//! "HeadphoneSettingsExtension"; [`DISPLAY_NAME_OVERRIDES`] is a two-entry
//! table giving them the same name System Settings' own sidebar shows
//! ("Battery", "Headphones"). Every other pane's name comes straight from
//! the bundle, unedited.
//!
//! **Enumeration is a one-time scan, not a live watch — same reasoning
//! `apps.rs` already established for the sealed system volume.** `mdfind`
//! only indexes user-writable locations; `/System/Library/ExtensionKit/
//! Extensions/` sits on the sealed, read-only system volume, which "can't
//! change without an OS update, which restarts the daemon anyway" (see
//! `apps.rs`'s own doc comment for the identical argument about
//! `/System/Applications`). [`SettingsProvider::new`] scans once, at daemon
//! construction, and caches the result for the daemon's whole lifetime —
//! no live watcher, no work on any later `Request::Search`. Measured on
//! this machine: well under the time `apps::scan_applications` itself
//! already takes, and, unlike `FileProvider`, entirely in-process (no
//! `mdfind` subprocess) since ExtensionKit bundles aren't Spotlight
//! metadata queries — 51 small `plist::from_file` calls over a fixed,
//! bounded 241-entry directory.
//!
//! **Icon: the System Settings app's own icon, not a per-pane one — a
//! deliberate scope cut, not an oversight.** Each pane's `Info.plist`
//! declares an `ISGraphicIconConfiguration.ISTypeIdentifier` (e.g.
//! `"com.apple.graphic-icon.display"`), which is how System Settings
//! itself renders a distinct colored glyph per row in its own sidebar —
//! but resolving that identifier to actual pixels goes through Apple's
//! private `ISIconRenderer`/`CoreMaterial` iconography stack, not any
//! public API this daemon can call the way `icons.rs` calls the public
//! `NSWorkspace.iconForFile`. Per the launch brief's own instruction ("use
//! the System Settings icon rather than an empty socket"), every pane row
//! instead shares one cached extraction of `/System/Applications/System
//! Settings.app`'s own icon via the existing, provider-agnostic
//! `icons::ensure_cached_icon` — the same self-healing `Icon::Placeholder`
//! → `Icon::Image` path apps already use (`neko-daemon/src/main.rs`
//! extracts it once, first, in the same background pass that extracts
//! every app icon) rather than a second icon pipeline.

use std::path::Path;

use neko_protocol::{Icon, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

/// Where every real System Settings pane lives on this OS — see this
/// module's doc comment for why the old `.prefPane` locations are dead
/// ends here.
const EXTENSIONS_DIR: &str = "/System/Library/ExtensionKit/Extensions";

/// The `EXAppExtensionAttributes.EXExtensionPointIdentifier` every real
/// settings pane extension declares, out of the many unrelated extension
/// points (widgets, Siri intents, thumbnail providers, ...) that also live
/// under [`EXTENSIONS_DIR`].
const SETTINGS_EXTENSION_POINT: &str = "com.apple.Settings.extension.ui";

/// The bundle whose own icon stands in for every pane row — see this
/// module's doc comment, "Icon" section.
pub const SETTINGS_APP_PATH: &str = "/System/Applications/System Settings.app";
/// The cache key `icons::ensure_cached_icon`/`cached_icon_path` use for
/// [`SETTINGS_APP_PATH`]'s icon — an arbitrary but stable string, namespaced
/// like a bundle identifier so it can never collide with a real app's own
/// `CFBundleIdentifier` in the shared icon cache directory.
pub const SETTINGS_APP_ICON_ID: &str = "com.apple.systempreferences";

/// Below this, a query is too short to be a meaningful pane-name prefix and
/// would otherwise match a large fraction of all ~50 panes at once (every
/// `fuzzy_score` for a 1-character query is a same-tier idx==0 hit) —
/// mirrors `files::MIN_QUERY_LEN`'s identical reasoning for the same shape
/// of problem.
const MIN_QUERY_LEN: usize = 2;

/// The final number of candidates one search can hand back — mirrors
/// `files::MAX_CANDIDATES`; the panel only ever renders a handful of any
/// one section.
const MAX_CANDIDATES: usize = 10;

/// See this module's doc comment, "Display names" section — the two real,
/// verified exceptions where no localized name exists anywhere in the
/// bundle.
const DISPLAY_NAME_OVERRIDES: &[(&str, &str)] =
    &[("com.apple.Battery-Settings.extension", "Battery"), ("com.apple.HeadphoneSettings", "Headphones")];

/// One enumerated pane: a stable identifier (also the `x-apple.
/// systempreferences:` URL target — see this module's doc comment) and its
/// on-screen name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsPane {
    pub bundle_id: String,
    pub title: String,
}

fn read_plist_dict(path: &Path) -> Option<plist::Dictionary> {
    plist::Value::from_file(path).ok()?.into_dictionary()
}

/// See this module's doc comment, "Display names" section.
fn localized_display_name(appex_dir: &Path) -> Option<String> {
    let loctable = read_plist_dict(&appex_dir.join("Contents/Resources/InfoPlist.loctable"))?;
    let name = loctable.get("en")?.as_dictionary()?.get("CFBundleDisplayName")?.as_string()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn display_name(bundle_id: &str, appex_dir: &Path, info: &plist::Dictionary) -> Option<String> {
    if let Some((_, name)) = DISPLAY_NAME_OVERRIDES.iter().find(|(id, _)| *id == bundle_id) {
        return Some((*name).to_string());
    }
    let from_key = |key: &str| info.get(key).and_then(|v| v.as_string()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    localized_display_name(appex_dir).or_else(|| from_key("CFBundleDisplayName")).or_else(|| from_key("CFBundleName"))
}

/// Parses one `.appex` bundle into a [`SettingsPane`], or `None` if it
/// isn't a real, URL-scheme-openable settings pane — see this module's doc
/// comment for exactly what's checked and why.
fn pane_from_appex(appex_dir: &Path) -> Option<SettingsPane> {
    let info = read_plist_dict(&appex_dir.join("Contents/Info.plist"))?;
    let ext_attrs = info.get("EXAppExtensionAttributes")?.as_dictionary()?;
    if ext_attrs.get("EXExtensionPointIdentifier")?.as_string()? != SETTINGS_EXTENSION_POINT {
        return None;
    }
    let settings_attrs = ext_attrs.get("SettingsExtensionAttributes")?.as_dictionary()?;
    if !settings_attrs.get("allowsXAppleSystemPreferencesURLScheme").and_then(|v| v.as_boolean()).unwrap_or(false) {
        return None;
    }
    let bundle_id = info.get("CFBundleIdentifier")?.as_string()?.to_string();
    let title = display_name(&bundle_id, appex_dir, &info)?;
    Some(SettingsPane { bundle_id, title })
}

/// The real enumeration — see this module's doc comment for the on-disk
/// mechanism and why a one-time scan is the right shape.
pub fn enumerate_panes() -> Vec<SettingsPane> {
    enumerate_panes_in(Path::new(EXTENSIONS_DIR))
}

/// The pure part of [`enumerate_panes`], parameterized on the directory to
/// scan — testable against a fabricated fixture directory rather than the
/// real, OS-version-dependent `/System/Library/ExtensionKit/Extensions`.
fn enumerate_panes_in(dir: &Path) -> Vec<SettingsPane> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut panes: Vec<SettingsPane> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "appex"))
        .filter_map(|path| pane_from_appex(&path))
        .collect();
    panes.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.bundle_id.cmp(&b.bundle_id)));
    panes.dedup_by(|a, b| a.bundle_id == b.bundle_id);
    panes
}

fn build_candidate(score: f32, pane: &SettingsPane, icon: Icon) -> Candidate {
    Candidate {
        score,
        item: SearchItem {
            id: pane.bundle_id.clone(),
            kind: "settings".to_string(),
            title: pane.title.clone(),
            subtitle: None,
            icon,
            section_label: "System Settings".to_string(),
            action_label: "Open  ↵".to_string(),
            badge: None,
            accessory: None,
            enters_mode: None,
            group_label: None,
            actions: Vec::new(),
            source: None,
            meter: None,
        },
    }
}

/// The System Settings pane provider. Holds the one-time [`enumerate_panes`]
/// result for the daemon's whole lifetime — see this module's doc comment
/// for why no live watcher exists here, unlike `apps::AppsProvider`.
pub struct SettingsProvider {
    panes: Vec<SettingsPane>,
}

impl SettingsProvider {
    pub fn new() -> Self {
        Self { panes: enumerate_panes() }
    }

    /// A provider over an explicit, fabricated pane list — for daemon-level
    /// integration tests (`neko-daemon/src/server.rs`) that need a real
    /// `AppState`-shaped provider list without depending on the test
    /// machine's own installed panes, exactly `FileProvider::empty()`'s
    /// reasoning.
    pub fn with_panes(panes: Vec<SettingsPane>) -> Self {
        Self { panes }
    }
}

impl Default for SettingsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for SettingsProvider {
    fn id(&self) -> &'static str {
        "settings"
    }

    fn section_label(&self) -> &'static str {
        "System Settings"
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let query = query.trim();
        if query.chars().count() < MIN_QUERY_LEN {
            return Vec::new();
        }
        // This provider's own candidates return plain, unboosted
        // `fuzzy_score` here — the category bonus that makes an
        // exact/near-exact pane match beat an incidental file/clipboard hit
        // (`search::settings_category_score`) is applied by `allocate`,
        // gated to the "settings" provider, exactly where
        // `search::app_category_score` already sits for "app". Kept
        // deliberately smaller than the app bonus and gated on the same
        // real-prefix-match condition, so a genuine application match for
        // an app-shaped query ("Bluetooth File Exchange" for "bluetooth")
        // still outranks the pane — see `search.rs`'s doc comment on
        // `settings_category_score` for the full reasoning and
        // `docs/evidence/settings-and-clipboard-ranking.md` for the real
        // numbers this was tuned against. An earlier version of this
        // provider left this fully unboosted, reasoning that plain
        // `fuzzy_score` already satisfied "must not crowd out application
        // matches" — true, but it also meant the pane lost to *everything
        // else*, not just apps (a captain-reported live defect: "sound"
        // ranked the Sound pane tenth, behind unrelated files and clipboard
        // entries).
        let icon = crate::icons::cached_icon_path(SETTINGS_APP_ICON_ID)
            .filter(|p| p.exists())
            .map(|p| Icon::Image(p.to_string_lossy().into_owned()))
            .unwrap_or(Icon::Placeholder);
        let mut candidates: Vec<Candidate> = self
            .panes
            .iter()
            .filter_map(|pane| Some(build_candidate(fuzzy_score(query, &pane.title)?, pane, icon.clone())))
            .collect();
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.item.title.cmp(&b.item.title)));
        candidates.truncate(MAX_CANDIDATES);
        candidates
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        if !self.panes.iter().any(|p| p.bundle_id == id) {
            return Err(ProviderError(format!("no such settings pane: {id}")));
        }
        crate::launch::open_url(&format!("x-apple.systempreferences:{id}")).map_err(|e| ProviderError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "neko-settings-test-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
    }

    fn write_appex(dir: &Path, name: &str, info_plist_body: &str, loctable_en_body: Option<&str>) {
        let appex_dir = dir.join(name);
        std::fs::create_dir_all(appex_dir.join("Contents/Resources")).unwrap();
        std::fs::write(
            appex_dir.join("Contents/Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>{info_plist_body}</dict></plist>"#
            ),
        )
        .unwrap();
        if let Some(en_body) = loctable_en_body {
            std::fs::write(
                appex_dir.join("Contents/Resources/InfoPlist.loctable"),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>en</key><dict>{en_body}</dict></dict></plist>"#
                ),
            )
            .unwrap();
        }
    }

    const SETTINGS_UI_HEADER: &str = r#"
        <key>EXAppExtensionAttributes</key>
        <dict>
            <key>EXExtensionPointIdentifier</key>
            <string>com.apple.Settings.extension.ui</string>
            <key>SettingsExtensionAttributes</key>
            <dict>
                <key>allowsXAppleSystemPreferencesURLScheme</key>
                <true/>
            </dict>
        </dict>
    "#;

    #[test]
    fn a_real_settings_extension_is_enumerated_with_its_localized_name() {
        let dir = temp_dir("basic");
        write_appex(
            &dir,
            "MouseExtension.appex",
            &format!(
                r#"<key>CFBundleIdentifier</key><string>com.apple.Mouse-Settings.extension</string>
                   <key>CFBundleDisplayName</key><string>MouseExtension</string>
                   {SETTINGS_UI_HEADER}"#
            ),
            Some("<key>CFBundleDisplayName</key><string>Mouse</string>"),
        );

        let panes = enumerate_panes_in(&dir);
        assert_eq!(panes, vec![SettingsPane { bundle_id: "com.apple.Mouse-Settings.extension".to_string(), title: "Mouse".to_string() }]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_non_settings_extension_point_is_excluded() {
        let dir = temp_dir("wrong-point");
        write_appex(
            &dir,
            "SomeWidget.appex",
            r#"<key>CFBundleIdentifier</key><string>com.apple.SomeWidget</string>
               <key>EXAppExtensionAttributes</key>
               <dict>
                   <key>EXExtensionPointIdentifier</key>
                   <string>com.apple.widgetkit-extension</string>
               </dict>"#,
            None,
        );
        assert_eq!(enumerate_panes_in(&dir), Vec::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_settings_extension_without_the_url_scheme_flag_is_excluded() {
        let dir = temp_dir("no-url-scheme");
        write_appex(
            &dir,
            "HomePrivacy.appex",
            r#"<key>CFBundleIdentifier</key><string>com.apple.Home.PrivacySettingsExtension</string>
               <key>EXAppExtensionAttributes</key>
               <dict>
                   <key>EXExtensionPointIdentifier</key>
                   <string>com.apple.Settings.extension.ui</string>
                   <key>SettingsExtensionAttributes</key>
                   <dict></dict>
               </dict>"#,
            None,
        );
        assert_eq!(enumerate_panes_in(&dir), Vec::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_directory_that_is_not_an_appex_bundle_is_ignored() {
        let dir = temp_dir("not-appex");
        std::fs::create_dir_all(dir.join("SomeOtherThing.bundle")).unwrap();
        assert_eq!(enumerate_panes_in(&dir), Vec::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_loctable_falls_back_to_the_raw_info_plist_display_name() {
        let dir = temp_dir("no-loctable");
        write_appex(
            &dir,
            "Storage.appex",
            &format!(
                r#"<key>CFBundleIdentifier</key><string>com.apple.settings.Storage</string>
                   <key>CFBundleDisplayName</key><string>Storage</string>
                   {SETTINGS_UI_HEADER}"#
            ),
            None,
        );
        let panes = enumerate_panes_in(&dir);
        assert_eq!(panes[0].title, "Storage");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn known_unnamed_panes_get_their_documented_override_name() {
        let dir = temp_dir("overrides");
        write_appex(
            &dir,
            "PowerPreferences.appex",
            &format!(
                r#"<key>CFBundleIdentifier</key><string>com.apple.Battery-Settings.extension</string>
                   <key>CFBundleDisplayName</key><string>PowerPreferences</string>
                   {SETTINGS_UI_HEADER}"#
            ),
            None,
        );
        let panes = enumerate_panes_in(&dir);
        assert_eq!(panes[0].title, "Battery");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn enumerating_the_real_machine_finds_at_least_displays_or_sound() {
        // A real, non-mocked scan against this machine's actual
        // `/System/Library/ExtensionKit/Extensions` — mirrors
        // `apps.rs`'s own `scanning_the_real_machine_finds_at_least_finder_or_safari`.
        let panes = enumerate_panes();
        assert!(
            panes.iter().any(|p| p.title == "Displays" || p.title == "Sound"),
            "expected at least one of Displays/Sound among {} enumerated panes",
            panes.len()
        );
    }

    #[test]
    fn query_below_the_minimum_length_returns_no_candidates() {
        let provider = SettingsProvider::with_panes(vec![SettingsPane {
            bundle_id: "com.apple.preference.displays".to_string(),
            title: "Displays".to_string(),
        }]);
        assert_eq!(provider.search("d", 0).len(), 0);
        assert_eq!(provider.search("", 0).len(), 0);
    }

    #[test]
    fn a_query_matching_a_pane_name_ranks_it() {
        let provider = SettingsProvider::with_panes(vec![
            SettingsPane { bundle_id: "com.apple.preference.displays".to_string(), title: "Displays".to_string() },
            SettingsPane { bundle_id: "com.apple.preference.sound".to_string(), title: "Sound".to_string() },
        ]);
        let items = provider.search("displays", 0);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item.title, "Displays");
        assert_eq!(items[0].item.id, "com.apple.preference.displays");
        assert_eq!(items[0].item.kind, "settings");
        assert_eq!(items[0].item.section_label, "System Settings");
        assert_eq!(items[0].item.action_label, "Open  ↵");
    }

    #[test]
    fn activate_with_an_unknown_pane_id_errors_without_shelling_out() {
        let provider = SettingsProvider::with_panes(vec![SettingsPane {
            bundle_id: "com.apple.preference.displays".to_string(),
            title: "Displays".to_string(),
        }]);
        let result = provider.activate("com.apple.does-not-exist");
        assert!(result.is_err());
        assert!(result.unwrap_err().0.contains("no such settings pane"));
    }
}
