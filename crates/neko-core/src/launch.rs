//! Launching an application. Shells out to `/usr/bin/open`, which is the
//! same mechanism Finder, Spotlight, and every third-party launcher uses —
//! it hands off to `launchd`/`LaunchServices` rather than this process
//! `fork`+`exec`ing the target directly, so the launched app is not a child
//! process of neko and survives neko exiting.

use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct LaunchError(pub String);

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for LaunchError {}

pub fn launch_app(app_path: &Path) -> Result<(), LaunchError> {
    let status = Command::new("/usr/bin/open")
        .arg(app_path)
        .status()
        .map_err(|e| LaunchError(format!("failed to run /usr/bin/open: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(LaunchError(format!("/usr/bin/open exited with {status}")))
    }
}

/// Same mechanism as [`launch_app`], for a URL rather than a file path —
/// `settings::SettingsProvider::activate` uses this to hand
/// `x-apple.systempreferences:<bundle-id>` to `open`, which is how System
/// Settings itself resolves a deep link to one specific pane rather than
/// just bringing the app to the front. `open` treats a URL-scheme argument
/// and a filesystem path identically, so this is the same call, just typed
/// on `&str` since a `systempreferences:` URL is never a valid `Path`.
/// Selects a path in Finder rather than opening it.
///
/// `open -R`, the same `/usr/bin/open` every other launch here goes through.
/// The distinction matters for both kinds of row it serves: `Open` on an
/// application *runs* it and on a file hands it to whatever owns the
/// extension, and neither is what somebody wants when they are trying to
/// find where the thing lives.
pub fn reveal_in_finder(path: &Path) -> Result<(), LaunchError> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|e| LaunchError(format!("couldn't run open: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(LaunchError(format!("couldn't reveal {}", path.display())))
    }
}

pub fn open_url(url: &str) -> Result<(), LaunchError> {
    let status = Command::new("/usr/bin/open")
        .arg(url)
        .status()
        .map_err(|e| LaunchError(format!("failed to run /usr/bin/open: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(LaunchError(format!("/usr/bin/open exited with {status}")))
    }
}
