//! The web UI's appearance: which theme shows in light, which in dark, and
//! whether the system decides. Plus the themes a person (or Neko) adds as
//! files in `<data dir>/themes/<id>.json`.
//!
//! **The daemon stores a choice, never a palette.** The built-in themes are
//! data files shipped with the web app (`web/src/themes/builtin/*.json`); the
//! daemon does not know their colours and does not need to. It keeps three
//! settings (`appearance.mode`, `appearance.light`, `appearance.dark`) so every
//! browser and the native window agree, and it reads the user's theme files,
//! checks each against the schema below and hands the good ones to the page.
//! A bad file is reported (`problems`) and skipped; it never fails a listing.
//!
//! The schema is documented in `docs/design/themes.md` and repeated, in the
//! same terms, by the web's own validator (`web/src/themes/schema.ts`) and by
//! the prompt that tells Neko to write a theme. Change all three together.

use std::fs;
use std::io;
use std::path::Path;

use neko_protocol::{ThemeProblem, UserTheme};

use crate::Db;

const MODE_KEY: &str = "appearance.mode";
const LIGHT_KEY: &str = "appearance.light";
const DARK_KEY: &str = "appearance.dark";

pub const DEFAULT_MODE: &str = "system";
pub const DEFAULT_LIGHT: &str = "neutral-light";
pub const DEFAULT_DARK: &str = "neutral";

/// The stored choice. `stored` is false until anything was ever saved, which
/// is how the first browser to connect knows to seed it from its own old
/// `neko-theme` value instead of overwriting that with the defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Appearance {
    pub mode: String,
    pub light: String,
    pub dark: String,
    pub stored: bool,
}

/// A theme id: the file's stem and the key the pickers store. Lowercase
/// letters, digits and hyphens, 1 to 48 characters, not starting with a hyphen.
pub fn valid_theme_id(id: &str) -> bool {
    (1..=48).contains(&id.len())
        && !id.starts_with('-')
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_mode(mode: &str) -> bool {
    matches!(mode, "system" | "light" | "dark")
}

pub fn get_appearance(db: &Db) -> rusqlite::Result<Appearance> {
    let mode = db.get_setting(MODE_KEY)?;
    let light = db.get_setting(LIGHT_KEY)?;
    let dark = db.get_setting(DARK_KEY)?;
    let stored = mode.is_some() || light.is_some() || dark.is_some();
    // A stored value that stopped being valid (a hand edit, a downgrade) reads
    // as the default rather than as an error: this is a cosmetic setting.
    Ok(Appearance {
        mode: mode.filter(|m| valid_mode(m)).unwrap_or_else(|| DEFAULT_MODE.into()),
        light: light.filter(|id| valid_theme_id(id)).unwrap_or_else(|| DEFAULT_LIGHT.into()),
        dark: dark.filter(|id| valid_theme_id(id)).unwrap_or_else(|| DEFAULT_DARK.into()),
        stored,
    })
}

/// Saves whichever of the three are given, after checking each one's shape
/// (the daemon cannot know the built-in themes, so whether an id names a real
/// theme is the page's call: an unknown id falls back to the default there).
pub fn set_appearance(db: &Db, mode: Option<&str>, light: Option<&str>, dark: Option<&str>) -> Result<Appearance, String> {
    if let Some(mode) = mode {
        if !valid_mode(mode) {
            return Err(format!("mode must be system, light or dark, not “{mode}”"));
        }
    }
    for (slot, id) in [("light", light), ("dark", dark)] {
        if let Some(id) = id {
            if !valid_theme_id(id) {
                return Err(format!("{slot} must be a theme id (lowercase letters, digits and hyphens), not “{id}”"));
            }
        }
    }
    let write = |key: &str, value: Option<&str>| match value {
        Some(value) => db.set_setting(key, value).map_err(|e| e.to_string()),
        None => Ok(()),
    };
    write(MODE_KEY, mode)?;
    write(LIGHT_KEY, light)?;
    write(DARK_KEY, dark)?;
    get_appearance(db).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// User theme files
// ---------------------------------------------------------------------------

/// Bigger than any sensible theme; a bound so a stray file cannot be read whole.
pub const MAX_THEME_BYTES: u64 = 32 * 1024;
/// More files than this are not listed (the rest are reported once).
pub const MAX_THEMES: usize = 64;

/// The colours a theme may set. `background`, `foreground` and `primary` are
/// required; the rest are derived when absent.
pub const COLOR_KEYS: &[&str] = &[
    "background", "card", "popover", "sidebar", "foreground", "strong", "mutedForeground", "border", "primary", "primaryForeground",
    "destructive", "success", "warning", "info", "bubble", "bubbleForeground",
];
const REQUIRED_COLORS: &[&str] = &["background", "foreground", "primary"];

fn valid_hex(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('#') else { return false };
    matches!(digits.len(), 3 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Checks one theme document. `expected_id` is the file's stem: a file named
/// `ocean.json` must say `"id": "ocean"`, so a rename cannot leave two names
/// for one theme. Returns the first problem in plain words.
pub fn validate_theme(text: &str, expected_id: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON ({e})"))?;
    let object = value.as_object().ok_or("the file must hold one JSON object")?;
    const TOP: &[&str] = &["id", "name", "appearance", "family", "colors", "backdrop"];
    if let Some(key) = object.keys().find(|k| !TOP.contains(&k.as_str())) {
        return Err(format!("unknown key “{key}” (allowed: {})", TOP.join(", ")));
    }
    match object.get("id").and_then(|v| v.as_str()) {
        Some(id) if id == expected_id => {}
        Some(id) => return Err(format!("“id” is “{id}” but the file is named {expected_id}.json")),
        None => return Err("“id” is missing".into()),
    }
    match object.get("name").and_then(|v| v.as_str()) {
        Some(name) if (1..=60).contains(&name.trim().chars().count()) => {}
        _ => return Err("“name” must be 1 to 60 characters".into()),
    }
    match object.get("appearance").and_then(|v| v.as_str()) {
        Some("light" | "dark") => {}
        _ => return Err("“appearance” must be \"light\" or \"dark\"".into()),
    }
    if let Some(family) = object.get("family") {
        if !family.as_str().is_some_and(|f| (1..=40).contains(&f.chars().count())) {
            return Err("“family” must be 1 to 40 characters".into());
        }
    }
    let colors = object.get("colors").and_then(|v| v.as_object()).ok_or("“colors” must be an object")?;
    for key in REQUIRED_COLORS {
        if !colors.contains_key(*key) {
            return Err(format!("colors.{key} is required"));
        }
    }
    for (key, value) in colors {
        if !COLOR_KEYS.contains(&key.as_str()) {
            return Err(format!("unknown colour “{key}” (allowed: {})", COLOR_KEYS.join(", ")));
        }
        if !value.as_str().is_some_and(valid_hex) {
            return Err(format!("colors.{key} must be a hex colour like #1a2b3c (3, 6 or 8 digits)"));
        }
    }
    if let Some(backdrop) = object.get("backdrop") {
        validate_backdrop(backdrop)?;
    }
    Ok(())
}

fn number_in(value: Option<&serde_json::Value>, low: f64, high: f64) -> bool {
    value.and_then(|v| v.as_f64()).is_some_and(|n| n >= low && n <= high)
}

fn validate_backdrop(value: &serde_json::Value) -> Result<(), String> {
    let object = value.as_object().ok_or("“backdrop” must be an object")?;
    if let Some(key) = object.keys().find(|k| !["blur", "base", "glows"].contains(&k.as_str())) {
        return Err(format!("unknown backdrop key “{key}” (allowed: blur, base, glows)"));
    }
    if !number_in(object.get("blur"), 0.0, 60.0) {
        return Err("backdrop.blur must be a number from 0 to 60".into());
    }
    let base = object.get("base").and_then(|v| v.as_array()).ok_or("backdrop.base must be a list of 1 to 4 hex colours")?;
    if !(1..=4).contains(&base.len()) || !base.iter().all(|c| c.as_str().is_some_and(valid_hex)) {
        return Err("backdrop.base must be a list of 1 to 4 hex colours".into());
    }
    if let Some(glows) = object.get("glows") {
        let glows = glows.as_array().filter(|g| g.len() <= 4).ok_or("backdrop.glows must be a list of at most 4 glows")?;
        for glow in glows {
            let glow = glow.as_object().ok_or("each glow must be an object")?;
            if let Some(key) = glow.keys().find(|k| !["color", "alpha", "x", "y", "width", "height"].contains(&k.as_str())) {
                return Err(format!("unknown glow key “{key}”"));
            }
            if !glow.get("color").and_then(|c| c.as_str()).is_some_and(valid_hex) {
                return Err("a glow needs a hex \"color\"".into());
            }
            if !number_in(glow.get("alpha"), 0.0, 1.0)
                || !number_in(glow.get("x"), -50.0, 150.0)
                || !number_in(glow.get("y"), -50.0, 150.0)
                || !number_in(glow.get("width"), 1.0, 200.0)
                || !number_in(glow.get("height"), 1.0, 200.0)
            {
                return Err("a glow needs alpha 0 to 1, x and y in percent, width and height 1 to 200".into());
            }
        }
    }
    Ok(())
}

/// What a listing found: the themes that passed, and a note for each file that
/// did not.
#[derive(Debug, Default)]
pub struct Listing {
    pub themes: Vec<UserTheme>,
    pub problems: Vec<ThemeProblem>,
}

fn problem(file: &str, message: impl Into<String>) -> ThemeProblem {
    ThemeProblem { file: file.to_string(), message: message.into() }
}

/// Reads `<dir>/*.json`. A symlink, a folder, a file over the size bound, a
/// file whose stem is not a theme id, unreadable text or a document that fails
/// [`validate_theme`] becomes a problem, never an error. Sorted by id, so the
/// listing is stable and a page can tell by comparing it whether anything changed.
pub fn list_user_themes(dir: &Path) -> Listing {
    let mut listing = Listing::default();
    let Ok(entries) = fs::read_dir(dir) else { return listing };
    let mut names: Vec<String> = entries.flatten().filter_map(|e| e.file_name().into_string().ok()).collect();
    names.sort();
    for name in names {
        let Some(stem) = name.strip_suffix(".json") else { continue };
        if name.starts_with('.') {
            continue;
        }
        if listing.themes.len() + listing.problems.len() >= MAX_THEMES {
            listing.problems.push(problem(&name, format!("more than {MAX_THEMES} theme files; this and the rest are not read")));
            break;
        }
        if !valid_theme_id(stem) {
            listing.problems.push(problem(&name, "the file name must be a theme id: lowercase letters, digits and hyphens, then .json"));
            continue;
        }
        let path = dir.join(&name);
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) => {
                listing.problems.push(problem(&name, format!("cannot read it ({e})")));
                continue;
            }
        };
        if !meta.is_file() {
            listing.problems.push(problem(&name, "not a plain file (links and folders are not followed)"));
            continue;
        }
        if meta.len() > MAX_THEME_BYTES {
            listing.problems.push(problem(&name, format!("larger than {} KiB", MAX_THEME_BYTES / 1024)));
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                listing.problems.push(problem(&name, format!("cannot read it as text ({e})")));
                continue;
            }
        };
        match validate_theme(&text, stem) {
            Ok(()) => match serde_json::from_str(&text) {
                Ok(theme) => listing.themes.push(UserTheme { id: stem.to_string(), file: name, theme }),
                Err(e) => listing.problems.push(problem(&name, format!("not valid JSON ({e})"))),
            },
            Err(message) => listing.problems.push(problem(&name, message)),
        }
    }
    listing
}

/// Deletes `<dir>/<id>.json`: the only way a theme leaves. A built-in has no
/// file here, so asking for one is "no such theme", never a deletion of
/// anything shipped. Only a plain file is removed (not a link, not a folder).
pub fn remove_user_theme(dir: &Path, id: &str) -> Result<(), String> {
    if !valid_theme_id(id) {
        return Err(format!("“{id}” is not a theme id"));
    }
    let path = dir.join(format!("{id}.json"));
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => fs::remove_file(&path).map_err(|e| format!("cannot remove {id}.json ({e})")),
        Ok(_) => Err(format!("{id}.json is not a plain file, so Neko leaves it alone")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(format!("there is no theme file for “{id}” (built-in themes cannot be removed)")),
        Err(e) => Err(format!("cannot read {id}.json ({e})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OCEAN: &str = r##"{
        "id": "ocean", "name": "Ocean", "appearance": "dark", "family": "Mine",
        "colors": { "background": "#0b1f2a", "foreground": "#d7e9f0", "primary": "#3ab0d0", "card": "#102a37" }
    }"##;

    fn dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            fs::write(dir.path().join(name), text).unwrap();
        }
        dir
    }

    #[test]
    fn a_valid_theme_passes_and_is_listed_with_its_text() {
        let dir = dir_with(&[("ocean.json", OCEAN)]);
        let listing = list_user_themes(dir.path());
        assert!(listing.problems.is_empty(), "{:?}", listing.problems);
        assert_eq!(listing.themes.len(), 1);
        assert_eq!(listing.themes[0].id, "ocean");
        assert_eq!(listing.themes[0].file, "ocean.json");
        assert_eq!(listing.themes[0].theme["colors"]["primary"], "#3ab0d0");
    }

    #[test]
    fn each_kind_of_mistake_is_named_and_none_of_them_stops_the_listing() {
        let bad: &[(&str, String, &str)] = &[
            ("not-json.json", "{ nope".into(), "not valid JSON"),
            ("wrong-id.json", OCEAN.into(), "“id” is “ocean” but the file is named wrong-id.json"),
            ("hex.json", OCEAN.replace("#3ab0d0", "teal").replace("\"ocean\"", "\"hex\""), "must be a hex colour"),
            ("extra.json", OCEAN.replace("\"family\": \"Mine\",", "\"family\": \"Mine\", \"css\": \"body{}\",").replace("\"ocean\"", "\"extra\""), "unknown key “css”"),
            ("missing.json", r##"{"id":"missing","name":"M","appearance":"dark","colors":{"background":"#000"}}"##.into(), "colors.foreground is required"),
            ("appearance.json", OCEAN.replace("\"dark\"", "\"medium\"").replace("\"ocean\"", "\"appearance\""), "“appearance”"),
            ("unknown-colour.json", OCEAN.replace("\"card\"", "\"cardinal\"").replace("\"ocean\"", "\"unknown-colour\""), "unknown colour “cardinal”"),
            ("Bad Name.json", OCEAN.into(), "the file name must be a theme id"),
        ];
        let mut files: Vec<(&str, &str)> = bad.iter().map(|(n, t, _)| (*n, t.as_str())).collect();
        files.push(("ocean.json", OCEAN));
        let dir = dir_with(&files);
        let listing = list_user_themes(dir.path());
        assert_eq!(listing.themes.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["ocean"]);
        for (name, _, expected) in bad {
            let found = listing.problems.iter().find(|p| p.file == *name).unwrap_or_else(|| panic!("no problem for {name}"));
            assert!(found.message.contains(expected), "{name}: {}", found.message);
        }
    }

    #[test]
    fn a_backdrop_is_checked_too() {
        let with = |backdrop: &str| OCEAN.replace("\"family\": \"Mine\",", &format!("\"family\": \"Mine\", \"backdrop\": {backdrop},"));
        let good = with(r##"{"blur": 20, "base": ["#000", "#112233"], "glows": [{"color":"#6b3fd0","alpha":0.5,"x":10,"y":0,"width":70,"height":60}]}"##);
        assert!(validate_theme(&good, "ocean").is_ok());
        for bad in [
            r##"{"blur": 90, "base": ["#000"]}"##,
            r##"{"blur": 5, "base": []}"##,
            r##"{"blur": 5, "base": ["#000"], "glows": [{"color":"red","alpha":1,"x":0,"y":0,"width":1,"height":1}]}"##,
            r##"{"blur": 5, "base": ["#000"], "image": "url(x)"}"##,
        ] {
            assert!(validate_theme(&with(bad), "ocean").is_err(), "{bad}");
        }
    }

    #[test]
    fn links_folders_and_oversize_files_are_reported_not_followed() {
        let dir = dir_with(&[("big.json", &"x".repeat(MAX_THEME_BYTES as usize + 1))]);
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.json"), OCEAN.replace("\"ocean\"", "\"linked\"")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.json"), dir.path().join("linked.json")).unwrap();
        fs::create_dir(dir.path().join("folder.json")).unwrap();
        let listing = list_user_themes(dir.path());
        assert!(listing.themes.is_empty());
        let message = |file: &str| listing.problems.iter().find(|p| p.file == file).unwrap().message.clone();
        assert!(message("big.json").contains("larger than"));
        assert!(message("linked.json").contains("not a plain file"));
        assert!(message("folder.json").contains("not a plain file"));
    }

    #[test]
    fn a_missing_folder_is_an_empty_listing() {
        let listing = list_user_themes(Path::new("/nonexistent/neko/themes"));
        assert!(listing.themes.is_empty() && listing.problems.is_empty());
    }

    #[test]
    fn removing_deletes_the_file_and_only_a_plain_file() {
        let dir = dir_with(&[("ocean.json", OCEAN)]);
        remove_user_theme(dir.path(), "ocean").unwrap();
        assert!(!dir.path().join("ocean.json").exists());
        // A built-in (or anything else with no file) is refused, not "already gone".
        let err = remove_user_theme(dir.path(), "neutral").unwrap_err();
        assert!(err.contains("built-in themes cannot be removed"), "{err}");
        // Nothing can climb out of the folder.
        assert!(remove_user_theme(dir.path(), "../etc/passwd").is_err());
        assert!(remove_user_theme(dir.path(), "a/b").is_err());
        // A link is left in place.
        let target = dir.path().join("target.txt");
        fs::write(&target, "keep").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("linked.json")).unwrap();
        assert!(remove_user_theme(dir.path(), "linked").is_err());
        assert!(dir.path().join("linked.json").exists() && target.exists());
    }

    #[test]
    fn the_appearance_defaults_until_something_is_saved_and_then_remembers_each_part() {
        let db = Db::open_in_memory().unwrap();
        let first = get_appearance(&db).unwrap();
        assert_eq!((first.mode.as_str(), first.light.as_str(), first.dark.as_str(), first.stored), ("system", "neutral-light", "neutral", false));
        let saved = set_appearance(&db, Some("dark"), None, Some("gruvbox-dark")).unwrap();
        assert_eq!((saved.mode.as_str(), saved.light.as_str(), saved.dark.as_str(), saved.stored), ("dark", "neutral-light", "gruvbox-dark", true));
        let later = set_appearance(&db, None, Some("catppuccin-latte"), None).unwrap();
        assert_eq!((later.mode.as_str(), later.light.as_str(), later.dark.as_str()), ("dark", "catppuccin-latte", "gruvbox-dark"));
    }

    #[test]
    fn a_bad_mode_or_id_is_refused_and_nothing_is_written() {
        let db = Db::open_in_memory().unwrap();
        assert!(set_appearance(&db, Some("sepia"), None, None).is_err());
        assert!(set_appearance(&db, Some("dark"), Some("Not An Id"), None).is_err());
        assert!(set_appearance(&db, None, None, Some("../x")).is_err());
        assert!(!get_appearance(&db).unwrap().stored);
    }

    #[test]
    fn a_stored_value_that_went_bad_reads_as_the_default() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting(MODE_KEY, "sepia").unwrap();
        db.set_setting(DARK_KEY, "Has Spaces").unwrap();
        let read = get_appearance(&db).unwrap();
        assert_eq!((read.mode.as_str(), read.dark.as_str(), read.stored), ("system", "neutral", true));
    }
}
