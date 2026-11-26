//! Finding an installed CLI the way a person's Terminal would (docs/knowledge/runtimes.md).
//!
//! An app opened from the Finder gets a short PATH (`/usr/bin:/bin:/usr/sbin:/sbin`), so a CLI installed with npm, Homebrew or its
//! own installer is not on it. Neko looks in this process's PATH, then in the folders such installers use, and, when the CLI is
//! still not found or is a script that needs its interpreter, in the PATH a login shell sets up (read once, with a time limit).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Whether `p` is a file this user can run.
pub fn executable(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The folders installers put CLIs in, after this process's own PATH.
pub fn common_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).filter(|d| d.is_absolute()).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for d in [".local/bin", ".claude/local", ".npm-global/bin", ".bun/bin", ".volta/bin", "Library/pnpm", ".cargo/bin"] {
            dirs.push(home.join(d));
        }
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

/// The PATH a login shell sets up, read once per process (at most five seconds; None if the shell says nothing).
pub fn login_shell_path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var("SHELL").ok().filter(|s| s.starts_with('/')).unwrap_or_else(|| "/bin/zsh".to_owned());
        // Interactive and login, as Terminal starts it, so the PATH from .zshrc counts too. The markers keep anything the shell's
        // own files print out of the answer.
        let mut cmd = Command::new(shell);
        cmd.args(["-ilc", "printf '__NEKO_PATH__%s__NEKO_END__' \"$PATH\""]).stdin(Stdio::null()).stderr(Stdio::null());
        let out = run_for(cmd, Duration::from_secs(5))?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let start = text.find("__NEKO_PATH__")? + "__NEKO_PATH__".len();
        let end = text[start..].find("__NEKO_END__")? + start;
        Some(text[start..end].to_owned()).filter(|p| !p.is_empty())
    })
    .as_deref()
}

/// Finds `name`: `override_path` if given, else this process's PATH and the common folders, else the login shell's PATH.
pub fn find(name: &str, override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = override_path {
        return executable(p).then(|| p.to_path_buf());
    }
    common_dirs().into_iter().map(|d| d.join(name)).find(|p| executable(p)).or_else(|| {
        login_shell_path().and_then(|path| std::env::split_paths(path).map(|d| d.join(name)).find(|p| p.is_absolute() && executable(p)))
    })
}

/// Whether `p` is a script (`#!`), which needs its interpreter on the PATH.
fn is_script(p: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 2];
    std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"#!"
}

/// The PATH a CLI at `bin` is started with: its own folder, the login shell's PATH when it is a script, this process's PATH, and
/// the common folders. Nothing else of the environment changes.
pub fn child_path(bin: &Path) -> String {
    let mut parts: Vec<PathBuf> = Vec::new();
    if let Some(d) = bin.parent() {
        parts.push(d.to_path_buf());
    }
    if is_script(bin)
        && let Some(p) = login_shell_path()
    {
        parts.extend(std::env::split_paths(p));
    }
    parts.extend(common_dirs());
    let mut seen = std::collections::HashSet::new();
    parts.retain(|p| p.is_absolute() && seen.insert(p.clone()));
    std::env::join_paths(parts).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Runs `cmd` to its end, or kills it (by the pid it was given) after `limit`. None if it could not start or ran out of time.
pub fn run_for(mut cmd: Command, limit: Duration) -> Option<Output> {
    cmd.stdout(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        buf
    });
    let until = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = reader.join().unwrap_or_default();
    status.map(|status| Output { status, stdout, stderr: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_override_is_not_found_and_sh_is() {
        assert_eq!(find("x", Some(Path::new("/nonexistent/claude"))), None);
        assert_eq!(find("sh", Some(Path::new("/bin/sh"))), Some(PathBuf::from("/bin/sh")));
        assert!(find("sh", None).is_some());
    }

    #[test]
    fn a_slow_command_is_cut_off() {
        let mut c = Command::new("/bin/sleep");
        c.arg("5");
        let t = Instant::now();
        assert!(run_for(c, Duration::from_millis(200)).is_none());
        assert!(t.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn the_childs_path_starts_with_its_own_folder() {
        assert!(child_path(Path::new("/opt/x/bin/claude")).starts_with("/opt/x/bin:"));
    }
}
