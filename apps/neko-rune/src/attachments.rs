//! A pasted image kept as a file so a message can point at it, as the web's composer does
//! (`![Image](file://…)`). The web UI posts the bytes to the daemon, which keeps them in its
//! `attachments` folder. This client keeps them in its own cache folder instead
//! (`~/Library/Caches/neko/composer-attachments`, private to the person, or `NEKO_RUNE_ATTACHMENTS`),
//! so it asks the daemon for nothing and the daemon's data folder stays the daemon's.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The folder pasted images go to.
pub fn dir() -> PathBuf {
    std::env::var_os("NEKO_RUNE_ATTACHMENTS").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        home.join("Library/Caches/neko/composer-attachments")
    })
}

/// A name from the bytes themselves: the same image is the same file (FNV-1a, 64 bits, and the length).
fn name_of(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}-{}.png", bytes.len())
}

/// Writes the PNG (mode 0600 in a 0700 folder) and returns its path. An image already kept is not
/// written again.
pub fn save_png(bytes: &[u8]) -> std::io::Result<PathBuf> {
    save_png_in(&dir(), bytes)
}

pub fn save_png_in(dir: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a PNG"));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join(name_of(bytes));
    if !path.exists() {
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?;
        if let Err(e) = file.write_all(bytes) {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
    }
    Ok(path)
}

/// The Markdown the message carries for a kept image.
pub fn reference(path: &Path) -> String {
    format!("![Image](file://{})", path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3];

    #[test]
    fn a_pasted_image_is_kept_privately_once_and_referenced_like_the_webs() {
        let dir = std::env::temp_dir().join(format!("neko-rune-att-{}", std::process::id()));
        let a = save_png_in(&dir, PNG).unwrap();
        let b = save_png_in(&dir, PNG).unwrap();
        assert_eq!(a, b, "the same image is the same file");
        assert_eq!(std::fs::read(&a).unwrap(), PNG);
        assert_eq!(std::fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(reference(&a), format!("![Image](file://{})", a.display()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn something_that_is_not_a_png_is_refused() {
        let dir = std::env::temp_dir().join(format!("neko-rune-att-bad-{}", std::process::id()));
        assert!(save_png_in(&dir, b"<html>").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
