//! Rune as the installed Mac app (P5b): what makes a run "bundled" and what `--check-bundle` asks of the bundle.
//!
//! `scripts/build-rune-app.sh` assembles `Neko.app` with this executable as `Contents/MacOS/neko`, the daemon beside it
//! (`neko-daemon`), the web UI in `Contents/Resources/web` and an `Info.plist` with the usage strings Calendar and Reminders wait
//! for. Two things follow from the bundle:
//!
//! * a run from inside a `.app` whose bundle id is `dev.neko.launcher` is `--app` mode without the flag ([`running_bundled`]);
//! * `neko --check-bundle` prints `ok` and exits 0 when the sibling daemon, the web UI and the usage strings are all there, else
//!   prints what is missing, one line each, and exits 1. `scripts/install-native.sh` runs it on the built, staged and installed
//!   bundle in place of the Swift app's `otool` check.
//!
//! This is the only file besides the confined readers that opens files for a check; it reads a handful of paths inside the
//! bundle and nothing else, and writes nothing.

use std::path::{Path, PathBuf};

use super::eventkit::{CALENDAR_USAGE_KEY, REMINDERS_USAGE_KEY};

/// The flag that checks the bundle and exits.
pub const CHECK_FLAG: &str = "--check-bundle";
/// The one bundle id both Mac apps carry (`scripts/Neko-Info.plist`).
pub const BUNDLE_ID: &str = "dev.neko.launcher";
/// What `--check-bundle` prints when nothing is missing.
pub const OK: &str = "ok";

/// `<name>.app` for an executable at `<name>.app/Contents/MacOS/<exe>`; `None` for any other place (a `target/` folder).
pub fn bundle_root(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let root = contents.parent()?;
    let named = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && root.extension().is_some_and(|e| e == "app");
    named.then(|| root.to_path_buf())
}

/// The text of the bundle's `Info.plist` (XML; a binary one is converted by `plutil`).
fn info_text(root: &Path) -> Result<String, String> {
    let path = root.join("Contents/Info.plist");
    match std::fs::read_to_string(&path) {
        Ok(text) if text.contains("<key>") => Ok(text),
        _ if path.is_file() => {
            let out = std::process::Command::new("/usr/bin/plutil").args(["-convert", "xml1", "-o", "-"]).arg(&path).output().map_err(|e| e.to_string())?;
            if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).into_owned()) } else { Err("Contents/Info.plist cannot be read".to_owned()) }
        }
        _ => Err("Contents/Info.plist is missing".to_owned()),
    }
}

/// The string value of a top-level key in an XML property list: the `<string>` that directly follows `<key>name</key>`.
pub fn plist_string(text: &str, key: &str) -> Option<String> {
    let marker = format!("<key>{key}</key>");
    let rest = text[text.find(&marker)? + marker.len()..].trim_start();
    let value = rest.strip_prefix("<string>")?;
    Some(value[..value.find("</string>")?].trim().to_owned())
}

/// Whether `exe` runs from inside a Neko app bundle (an `.app` with bundle id `dev.neko.launcher`).
pub fn is_neko_bundle(exe: &Path) -> bool {
    bundle_root(exe).and_then(|root| info_text(&root).ok()).and_then(|text| plist_string(&text, "CFBundleIdentifier")).is_some_and(|id| id == BUNDLE_ID)
}

/// This process runs from inside the installed Neko app: `--app` mode is implied.
pub fn running_bundled() -> bool {
    std::env::current_exe().is_ok_and(|exe| is_neko_bundle(&exe))
}

/// What is missing from the bundle `exe` belongs to; empty means whole.
pub fn problems(exe: &Path) -> Vec<String> {
    let Some(root) = bundle_root(exe) else {
        return vec!["this executable is not inside a .app bundle (Contents/MacOS/)".to_owned()];
    };
    let mut missing = Vec::new();
    let daemon = root.join("Contents/MacOS/neko-daemon");
    match std::fs::metadata(&daemon) {
        Ok(meta) if meta.is_file() && std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o111 != 0 => {}
        Ok(_) => missing.push("Contents/MacOS/neko-daemon is not an executable file".to_owned()),
        Err(_) => missing.push("Contents/MacOS/neko-daemon is missing".to_owned()),
    }
    if !root.join("Contents/Resources/web/index.html").is_file() {
        missing.push("Contents/Resources/web (the web UI, with index.html) is missing".to_owned());
    }
    match info_text(&root) {
        Err(why) => missing.push(why),
        Ok(text) => {
            if plist_string(&text, "CFBundleIdentifier").as_deref() != Some(BUNDLE_ID) {
                missing.push(format!("Info.plist CFBundleIdentifier is not {BUNDLE_ID}"));
            }
            for key in [CALENDAR_USAGE_KEY, REMINDERS_USAGE_KEY] {
                if plist_string(&text, key).is_none_or(|v| v.is_empty()) {
                    missing.push(format!("Info.plist has no {key}"));
                }
            }
        }
    }
    missing
}

/// `--check-bundle` among `args`: prints the answer and gives the exit code; `None` when the flag is not there.
pub fn check_if_asked(args: &[String], exe: &Path) -> Option<i32> {
    if !args.iter().any(|a| a == CHECK_FLAG) {
        return None;
    }
    let missing = problems(exe);
    if missing.is_empty() {
        println!("{OK}");
        Some(0)
    } else {
        for line in &missing {
            println!("{line}");
        }
        Some(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key>
<string>dev.neko.launcher</string>
<key>NSCalendarsFullAccessUsageDescription</key>
<string>Shows your meetings.</string>
<key>NSRemindersFullAccessUsageDescription</key>
<string>Puts reminders in its own list.</string>
</dict></plist>"#;

    fn fake(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("neko-bundle-{tag}-{}", std::process::id())).join("Neko.app");
        let _ = fs::remove_dir_all(root.parent().unwrap());
        fs::create_dir_all(root.join("Contents/MacOS")).unwrap();
        fs::create_dir_all(root.join("Contents/Resources/web")).unwrap();
        fs::write(root.join("Contents/Resources/web/index.html"), "<html></html>").unwrap();
        fs::write(root.join("Contents/Info.plist"), PLIST).unwrap();
        let exe = root.join("Contents/MacOS/neko");
        fs::write(&exe, "").unwrap();
        let daemon = root.join("Contents/MacOS/neko-daemon");
        fs::write(&daemon, "").unwrap();
        fs::set_permissions(&daemon, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        (root, exe)
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_whole_bundle_prints_ok_and_is_the_neko_app() {
        let (root, exe) = fake("whole");
        assert!(problems(&exe).is_empty(), "{:?}", problems(&exe));
        assert_eq!(check_if_asked(&args(&["--check-bundle"]), &exe), Some(0));
        assert!(is_neko_bundle(&exe), "bundle id dev.neko.launcher inside an .app is the app: --app is implied");
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn the_flag_is_only_acted_on_when_given() {
        let (root, exe) = fake("flag");
        assert_eq!(check_if_asked(&args(&["--app"]), &exe), None);
        assert_eq!(check_if_asked(&[], &exe), None);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn each_missing_piece_is_named_and_fails_the_check() {
        let (root, exe) = fake("missing");
        fs::remove_file(root.join("Contents/MacOS/neko-daemon")).unwrap();
        fs::remove_dir_all(root.join("Contents/Resources/web")).unwrap();
        fs::write(root.join("Contents/Info.plist"), PLIST.replace("NSRemindersFullAccessUsageDescription", "NSSomethingElse")).unwrap();
        let missing = problems(&exe);
        assert_eq!(missing.len(), 3, "{missing:?}");
        assert!(missing[0].contains("neko-daemon") && missing[1].contains("Resources/web") && missing[2].contains("NSRemindersFullAccessUsageDescription"), "{missing:?}");
        assert_eq!(check_if_asked(&args(&["--check-bundle"]), &exe), Some(1));
        // A daemon that cannot run, and an empty usage string, are as bad as a missing one.
        let (root2, exe2) = fake("weak");
        fs::set_permissions(root2.join("Contents/MacOS/neko-daemon"), std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
        fs::write(root2.join("Contents/Info.plist"), PLIST.replace("Shows your meetings.", "")).unwrap();
        let weak = problems(&exe2);
        assert!(weak.iter().any(|l| l.contains("not an executable")) && weak.iter().any(|l| l.contains("NSCalendarsFullAccessUsageDescription")), "{weak:?}");
        let _ = fs::remove_dir_all(root.parent().unwrap());
        let _ = fs::remove_dir_all(root2.parent().unwrap());
    }

    #[test]
    fn a_run_outside_an_app_or_with_another_id_is_not_the_bundled_app() {
        let (root, exe) = fake("other");
        fs::write(root.join("Contents/Info.plist"), PLIST.replace("dev.neko.launcher", "dev.neko.native-preview")).unwrap();
        assert!(!is_neko_bundle(&exe), "another bundle id never implies --app");
        assert!(problems(&exe).iter().any(|l| l.contains("CFBundleIdentifier")));
        // A development run: the executable is in a `target/` folder.
        let dev = Path::new("/tmp/project/target/debug/neko-rune");
        assert_eq!(bundle_root(dev), None);
        assert!(!is_neko_bundle(dev));
        assert_eq!(problems(dev).len(), 1);
        // Right folder names but not an `.app`.
        assert_eq!(bundle_root(Path::new("/tmp/Neko/Contents/MacOS/neko")), None);
        assert_eq!(bundle_root(Path::new("/Applications/Neko.app/Contents/MacOS/neko")), Some(PathBuf::from("/Applications/Neko.app")));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn plist_strings_are_read_by_key() {
        assert_eq!(plist_string(PLIST, "CFBundleIdentifier").as_deref(), Some("dev.neko.launcher"));
        assert_eq!(plist_string(PLIST, "Nope"), None);
        // A key whose value is not a string is not read as the next key's string.
        assert_eq!(plist_string("<key>A</key><true/><key>B</key><string>x</string>", "A"), None);
    }
}
