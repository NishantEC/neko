//! The data folder's own files: the folder (0700), the token (0600) and `daemon.json` (0600, written whole).

use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use neko_protocol::{DaemonInfo, files};

/// Makes the folder if needed, readable by its owner only.
pub fn ensure_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, Permissions::from_mode(0o700))
}

pub fn random_hex(bytes: usize) -> io::Result<String> {
    let mut buf = vec![0u8; bytes];
    fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// The folder's token: read if it is there (its mode put back to 0600 if it drifted), else made.
pub fn token(dir: &Path) -> io::Result<String> {
    let path = dir.join(files::TOKEN);
    match fs::read_to_string(&path) {
        Ok(t) if t.trim().len() >= 32 => {
            if fs::metadata(&path)?.permissions().mode() & 0o077 != 0 {
                fs::set_permissions(&path, Permissions::from_mode(0o600))?;
            }
            Ok(t.trim().to_owned())
        }
        Ok(_) | Err(_) => {
            let t = random_hex(32)?;
            let _ = fs::remove_file(&path);
            let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
            f.write_all(t.as_bytes())?;
            f.sync_all()?;
            Ok(t)
        }
    }
}

/// Writes `daemon.json` through a temporary file and a rename, so a reader never sees half of it.
pub fn write_info(dir: &Path, info: &DaemonInfo) -> io::Result<()> {
    let tmp = dir.join(format!("{}.tmp", files::DAEMON));
    let _ = fs::remove_file(&tmp);
    let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    f.write_all(serde_json::to_string(info).map_err(io::Error::other)?.as_bytes())?;
    f.sync_all()?;
    fs::rename(&tmp, dir.join(files::DAEMON))
}

/// Removes `daemon.json` if it still describes this process.
pub fn remove_info(dir: &Path) {
    let path = dir.join(files::DAEMON);
    let ours = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<DaemonInfo>(&b).ok()).is_some_and(|i| i.pid == std::process::id());
    if ours {
        let _ = fs::remove_file(path);
    }
}

/// Compares two tokens in time that does not depend on where they differ.
pub fn same_token(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_whole() {
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abc", "abd"));
        assert!(!same_token("abc", "abcd"));
        assert!(!same_token("", "a"));
    }
}
