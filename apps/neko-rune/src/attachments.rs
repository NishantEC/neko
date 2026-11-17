//! A pasted image kept as a file so a message can point at it, as the web's composer does
//! (`![Image](file://…)`). The web UI posts the bytes to the daemon, which keeps them in its
//! `attachments` folder. This client keeps them in its own cache folder instead
//! (`~/Library/Caches/neko/composer-attachments`, private to the person, or `NEKO_RUNE_ATTACHMENTS`),
//! so it asks the daemon for nothing and the daemon's data folder stays the daemon's.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The largest image kept (Rune hands over at most 32 MiB of PNG).
pub const MAX_BYTES: usize = 32 * 1024 * 1024;

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
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G']) || bytes.len() > MAX_BYTES {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a PNG, or too large"));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join(name_of(bytes));
    if !path.exists() {
        // Written beside it and renamed, so a crash never leaves a half-written image under its name.
        let partial = dir.join(format!("{}.partial", name_of(bytes)));
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&partial)?;
        if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()).and_then(|()| std::fs::rename(&partial, &path)) {
            let _ = std::fs::remove_file(&partial);
            return Err(e);
        }
    }
    Ok(path)
}

/// How big a file is, when it is a file we may read.
pub fn size_of(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// A file's bytes for the daemon's `/api/attachments` (at most 32 MB, never empty).
pub fn read_for_upload(path: &Path) -> Result<Vec<u8>, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "That file".to_owned());
    let size = size_of(path).ok_or_else(|| format!("Couldn’t read {name}."))?;
    if size == 0 {
        return Err(format!("{name} is empty."));
    }
    if size as usize > crate::link::http::MAX_ATTACHMENT_BYTES {
        return Err("Choose a file under 32 MB.".to_owned());
    }
    std::fs::read(path).map_err(|_| format!("Couldn’t read {name}."))
}

/// A downloaded artifact saved under its own name in the person's Downloads folder (`NEKO_RUNE_DOWNLOADS` in tests), never over a file
/// that is there: `report.csv` becomes `report 2.csv`. Returns where it went.
pub fn save_download(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = std::env::var_os("NEKO_RUNE_DOWNLOADS").map(PathBuf::from).or_else(crate::web_panel::downloads_dir).ok_or_else(|| "There is no Downloads folder.".to_owned())?;
    let dest = crate::web_panel::downloads_destination(Path::new(name), &dir);
    std::fs::write(&dest, bytes).map_err(|_| "The download could not be saved.".to_owned())?;
    Ok(dest)
}

/// The Markdown the message carries for a kept image.
pub fn reference(path: &Path) -> String {
    // A space or a bracket in the folder's name would end the Markdown link early.
    let encoded: String = path.to_string_lossy().chars().map(|c| if matches!(c, ' ' | '(' | ')' | '[' | ']') { format!("%{:02X}", c as u32) } else { c.to_string() }).collect();
    format!("![Image](file://{encoded})")
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
        assert_eq!(reference(Path::new("/Users/a b/(x)/i.png")), "![Image](file:///Users/a%20b/%28x%29/i.png)");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn something_that_is_not_a_png_is_refused() {
        let dir = std::env::temp_dir().join(format!("neko-rune-att-bad-{}", std::process::id()));
        assert!(save_png_in(&dir, b"<html>").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
