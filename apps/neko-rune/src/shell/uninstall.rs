//! Uninstall: a port of `Uninstaller.swift`. Everything goes to the Trash, so it can be put back; nothing is erased outright.
//! Offered only from an installed `.app`.
//!
//! What it does, in Swift's order: stop all work; unload and trash the login item (`~/Library/LaunchAgents/com.neko.launcher.plist`);
//! `pkill` the bundle's own daemon (`<bundle>/Contents/MacOS/neko-daemon`, matched by full path so another daemon is never touched);
//! reset the app's Accessibility permission (`tccutil`); trash the app bundle and, only if asked, Neko's data folder
//! (`~/Library/Application Support/neko`); then quit. It returns what it could not do, in plain words.
//!
//! The question is the window's own confirmation (`task_actions::Ask`, a coral button) with Swift's words and its checkbox. The steps run
//! through two small traits ([`Runner`] for the three tools, [`Trasher`] for the Trash), so the tests run all of it against a temp
//! folder with fakes and never touch a real path, a real daemon or the real Trash.

use std::path::{Path, PathBuf};

pub const LAUNCH_AGENT_LABEL: &str = "com.neko.launcher";

/// The dialog's words (`Uninstaller.confirmAndRun`).
pub const TITLE: &str = "Uninstall Neko?";
pub const DETAIL: &str = "Neko stops all work, removes its login item and shortcut, and moves the app to the Trash. Your repositories are not touched. You can restore anything from the Trash.";
pub const OK: &str = "Uninstall";
pub const CANCEL: &str = "Cancel";
pub const DATA_CHECKBOX: &str = "Also move Neko’s data to the Trash (memory, tasks, settings and task worktrees)";
/// The report after a partly failed run.
pub const PARTLY_TITLE: &str = "Neko was mostly removed";
/// What the menu says when this is not an installed app.
pub const UNAVAILABLE: &str = "Uninstall is available from the installed Neko app.";

/// What will be done (`Uninstaller.Plan`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub trash: Vec<PathBuf>,
    pub launch_agent: PathBuf,
    pub daemon_path: String,
    pub bundle_id: Option<String>,
}

pub fn plan(bundle: &Path, home: &Path, bundle_id: Option<&str>, include_data: bool) -> Plan {
    let mut trash = vec![bundle.to_path_buf()];
    if include_data {
        trash.push(home.join("Library/Application Support/neko"));
    }
    Plan {
        trash,
        launch_agent: home.join(format!("Library/LaunchAgents/{LAUNCH_AGENT_LABEL}.plist")),
        daemon_path: bundle.join("Contents/MacOS/neko-daemon").to_string_lossy().into_owned(),
        bundle_id: bundle_id.map(str::to_owned),
    }
}

/// The `.app` folder this executable runs from, if any: Swift's `Bundle.main.bundleURL` when it ends in `.app`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

/// `Uninstaller.available`.
pub fn available() -> bool {
    std::env::current_exe().ok().and_then(|e| bundle_of(&e)).is_some()
}

/// Runs a tool and says whether it started (its exit status is not looked at, as Swift's).
pub trait Runner {
    fn run(&mut self, tool: &str, args: &[String]);
}

/// Moves a path to the Trash.
pub trait Trasher {
    fn trash(&mut self, path: &Path) -> Result<(), String>;
}

/// `NSRegularExpression.escapedPattern` for `pkill -f`'s extended expression.
pub fn escape_pattern(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if "\\^$.|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `Uninstaller.run`: returns what could not be done, in plain words.
pub fn run(plan: &Plan, runner: &mut impl Runner, trasher: &mut impl Trasher) -> Vec<String> {
    let mut problems = Vec::new();
    if plan.launch_agent.exists() {
        runner.run("/bin/launchctl", &["unload".into(), "-w".into(), plan.launch_agent.to_string_lossy().into_owned()]);
        if trasher.trash(&plan.launch_agent).is_err() {
            problems.push(format!("Couldn’t remove the login item at {}.", plan.launch_agent.display()));
        }
    }
    runner.run("/usr/bin/pkill", &["-f".into(), format!("^{}( |$)", escape_pattern(&plan.daemon_path))]);
    if let Some(id) = &plan.bundle_id {
        runner.run("/usr/bin/tccutil", &["reset".into(), "Accessibility".into(), id.clone()]);
    }
    for path in plan.trash.iter().filter(|p| p.exists()) {
        if let Err(why) = trasher.trash(path) {
            problems.push(format!("Couldn’t move {} to the Trash: {why}", path.display()));
        }
    }
    problems
}

/// The real tools.
pub struct RealRunner;

impl Runner for RealRunner {
    fn run(&mut self, tool: &str, args: &[String]) {
        let _ = std::process::Command::new(tool).args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
}

/// The real Trash (`NSFileManager.trashItemAtURL`).
pub struct RealTrash;

#[cfg(target_os = "macos")]
impl Trasher for RealTrash {
    fn trash(&mut self, path: &Path) -> Result<(), String> {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        NSFileManager::defaultManager().trashItemAtURL_resultingItemURL_error(&url, None).map_err(|e| e.localizedDescription().to_string())
    }
}

#[cfg(not(target_os = "macos"))]
impl Trasher for RealTrash {
    fn trash(&mut self, _path: &Path) -> Result<(), String> {
        Err("no Trash here".to_owned())
    }
}

/// The installed app's real plan (Swift's `confirmAndRun`): None when this is not an installed app.
pub fn real_plan(include_data: bool) -> Option<Plan> {
    let bundle = bundle_of(&std::env::current_exe().ok()?)?;
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(plan(&bundle, &home, bundle_id().as_deref(), include_data))
}

#[cfg(target_os = "macos")]
fn bundle_id() -> Option<String> {
    objc2_foundation::NSBundle::mainBundle().bundleIdentifier().map(|s| s.to_string())
}

#[cfg(not(target_os = "macos"))]
fn bundle_id() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Tools(Vec<(String, Vec<String>)>);

    impl Runner for Tools {
        fn run(&mut self, tool: &str, args: &[String]) {
            self.0.push((tool.to_owned(), args.to_vec()));
        }
    }

    /// Moves into a temp "Trash" folder, or refuses the paths it is told to.
    struct FakeTrash {
        bin: PathBuf,
        refuse: Vec<PathBuf>,
    }

    impl Trasher for FakeTrash {
        fn trash(&mut self, path: &Path) -> Result<(), String> {
            if self.refuse.iter().any(|r| r == path) {
                return Err("Permission denied".to_owned());
            }
            let name = path.file_name().unwrap();
            std::fs::rename(path, self.bin.join(name)).map_err(|e| e.to_string())
        }
    }

    /// A pretend home with a bundle, a login item and a data folder, all under one temp folder.
    fn world(tag: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("neko-uninstall-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let bundle = root.join("Applications/Neko.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/neko-daemon"), b"x").unwrap();
        std::fs::create_dir_all(home.join("Library/LaunchAgents")).unwrap();
        std::fs::write(home.join("Library/LaunchAgents/com.neko.launcher.plist"), b"x").unwrap();
        std::fs::create_dir_all(home.join("Library/Application Support/neko")).unwrap();
        std::fs::write(home.join("Library/Application Support/neko/neko.db"), b"x").unwrap();
        let bin = root.join("Trash");
        std::fs::create_dir_all(&bin).unwrap();
        (root, home, bundle, bin)
    }

    #[test]
    fn the_plan_trashes_the_bundle_and_the_data_only_when_asked() {
        let home = Path::new("/Users/example");
        let bundle = Path::new("/Applications/Neko.app");
        let p = plan(bundle, home, Some("dev.neko.launcher"), false);
        assert_eq!(p.trash, [PathBuf::from("/Applications/Neko.app")]);
        assert_eq!(p.launch_agent, PathBuf::from("/Users/example/Library/LaunchAgents/com.neko.launcher.plist"));
        assert_eq!(p.daemon_path, "/Applications/Neko.app/Contents/MacOS/neko-daemon");
        let with = plan(bundle, home, None, true);
        assert_eq!(with.trash, [PathBuf::from("/Applications/Neko.app"), PathBuf::from("/Users/example/Library/Application Support/neko")]);
        assert_eq!(with.bundle_id, None);
    }

    #[test]
    fn only_an_installed_app_can_be_uninstalled() {
        assert_eq!(bundle_of(Path::new("/Applications/Neko.app/Contents/MacOS/neko-rune")), Some(PathBuf::from("/Applications/Neko.app")));
        assert_eq!(bundle_of(Path::new("/Users/example/work/target/debug/neko-rune")), None, "a development binary is not an app");
    }

    #[test]
    fn the_pattern_matches_only_the_bundles_own_daemon() {
        assert_eq!(escape_pattern("/Applications/Neko.app/Contents/MacOS/neko-daemon"), "/Applications/Neko\\.app/Contents/MacOS/neko-daemon");
        assert_eq!(escape_pattern("a(b)+c"), "a\\(b\\)\\+c");
    }

    #[test]
    fn it_runs_swifts_steps_in_swifts_order_against_a_temp_folder() {
        let (root, home, bundle, bin) = world("all");
        let p = plan(&bundle, &home, Some("dev.neko.launcher"), true);
        let (mut tools, mut trash) = (Tools::default(), FakeTrash { bin: bin.clone(), refuse: vec![] });
        let problems = run(&p, &mut tools, &mut trash);
        assert!(problems.is_empty(), "{problems:?}");
        let names: Vec<&str> = tools.0.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(names, ["/bin/launchctl", "/usr/bin/pkill", "/usr/bin/tccutil"]);
        assert_eq!(tools.0[0].1, ["unload", "-w", p.launch_agent.to_str().unwrap()]);
        assert_eq!(tools.0[1].1[0], "-f");
        assert!(tools.0[1].1[1].starts_with('^') && tools.0[1].1[1].ends_with("( |$)") && tools.0[1].1[1].contains("neko-daemon"));
        assert_eq!(tools.0[2].1, ["reset", "Accessibility", "dev.neko.launcher"]);
        assert!(!bundle.exists() && !home.join("Library/LaunchAgents/com.neko.launcher.plist").exists() && !home.join("Library/Application Support/neko").exists());
        assert!(bin.join("Neko.app").exists() && bin.join("neko").exists() && bin.join("com.neko.launcher.plist").exists(), "all of it is in the Trash, none erased");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn data_stays_unless_asked_and_a_missing_login_item_is_skipped() {
        let (root, home, bundle, bin) = world("keep");
        std::fs::remove_file(home.join("Library/LaunchAgents/com.neko.launcher.plist")).unwrap();
        let p = plan(&bundle, &home, None, false);
        let (mut tools, mut trash) = (Tools::default(), FakeTrash { bin, refuse: vec![] });
        assert!(run(&p, &mut tools, &mut trash).is_empty());
        let names: Vec<&str> = tools.0.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(names, ["/usr/bin/pkill"], "no login item to unload, no bundle id to reset");
        assert!(!bundle.exists());
        assert!(home.join("Library/Application Support/neko/neko.db").exists(), "the data folder is untouched");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn what_could_not_be_done_is_said_in_plain_words() {
        let (root, home, bundle, bin) = world("fail");
        let agent = home.join("Library/LaunchAgents/com.neko.launcher.plist");
        let p = plan(&bundle, &home, None, false);
        let (mut tools, mut trash) = (Tools::default(), FakeTrash { bin, refuse: vec![agent.clone(), bundle.clone()] });
        let problems = run(&p, &mut tools, &mut trash);
        assert_eq!(problems, [format!("Couldn’t remove the login item at {}.", agent.display()), format!("Couldn’t move {} to the Trash: Permission denied", bundle.display())]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_words_are_swifts() {
        assert_eq!(TITLE, "Uninstall Neko?");
        assert!(DETAIL.starts_with("Neko stops all work, removes its login item and shortcut, and moves the app to the Trash."));
        assert_eq!(PARTLY_TITLE, "Neko was mostly removed");
        assert!(DATA_CHECKBOX.starts_with("Also move Neko’s data to the Trash"));
    }
}
