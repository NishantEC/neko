//! Oversized output Neko feeds back into a prompt is written to a file, and the
//! prompt gets its head, its tail and the file's path instead of all of it.
//! Applies where Neko itself builds the text (a goal's check output, the open
//! items a routine is shown, a crew `task_status` answer), never to an agent's
//! own tool calls. Files live under `<data>/spill` and go after a week.
//! See docs/knowledge/neko-maintains-itself.md.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

/// Output at or under this many bytes goes into the prompt whole.
pub const LIMIT: usize = 8 * 1024;
/// What the prompt keeps of a longer one: the first and last of it.
pub const HEAD: usize = 2 * 1024;
pub const TAIL: usize = 2 * 1024;
/// Spill files older than this are deleted.
pub const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);

/// Where spill files go in the real data directory.
pub fn dir() -> PathBuf {
    neko_protocol::support_dir().join("spill")
}

/// `text` as the prompt should carry it: whole when small, otherwise its head,
/// its tail and the path of the file holding all of it. A file that cannot be
/// written still gets the head and tail, without a path.
pub fn bounded(label: &str, text: &str) -> String {
    bounded_in(&dir(), label, text)
}

pub fn bounded_in(directory: &Path, label: &str, text: &str) -> String {
    if text.len() <= LIMIT {
        return text.to_owned();
    }
    let path = write(directory, label, text);
    let head = cut_head(text, HEAD);
    let tail = cut_tail(text, TAIL);
    let omitted = text.len().saturating_sub(head.len() + tail.len());
    let place = match &path {
        Some(path) => format!("the full output ({} bytes) is in {}", text.len(), path.display()),
        None => format!("the full output ({} bytes) could not be saved", text.len()),
    };
    format!("{head}\n[... {omitted} bytes left out; {place} ...]\n{tail}")
}

fn cut_head(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn cut_tail(text: &str, max: usize) -> &str {
    let mut start = text.len().saturating_sub(max);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// The file name is the label and the content's hash, so the same output
/// spilled twice is one file.
fn write(directory: &Path, label: &str, text: &str) -> Option<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let label: String = label.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(24).collect();
    let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
    let path = directory.join(format!("{}-{}.txt", if label.is_empty() { "output" } else { &label }, &digest[..16]));
    if path.is_file() {
        // Touch it so a repeated spill is not swept while still in use.
        let _ = fs::OpenOptions::new().append(true).open(&path).and_then(|f| f.set_modified(std::time::SystemTime::now()));
        return Some(path);
    }
    fs::DirBuilder::new().recursive(true).mode(0o700).create(directory).ok()?;
    let staging = directory.join(format!(".{}.{}", std::process::id(), &digest[..8]));
    let written = (|| {
        let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&staging)?;
        file.write_all(text.as_bytes())?;
        fs::rename(&staging, &path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&staging);
        return None;
    }
    Some(path)
}

/// Deletes spill files not touched for `older_than`. Returns how many went.
pub fn cleanup(directory: &Path, now: std::time::SystemTime, older_than: Duration) -> usize {
    let Ok(entries) = fs::read_dir(directory) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let old = meta.modified().ok().and_then(|m| now.duration_since(m).ok()).is_some_and(|age| age >= older_than);
        if old && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_output_is_kept_whole() {
        let dir = tempfile::tempdir().unwrap();
        let text = "x".repeat(LIMIT);
        assert_eq!(bounded_in(dir.path(), "check", &text), text);
        assert!(fs::read_dir(dir.path()).unwrap().next().is_none(), "nothing is written");
    }

    #[test]
    fn large_output_keeps_head_tail_and_the_path_of_the_full_file() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("{}{}{}", "HEAD".repeat(1000), "m".repeat(20_000), "TAIL".repeat(1000));
        let shown = bounded_in(dir.path(), "check", &text);
        assert!(shown.starts_with(&text[..HEAD]));
        assert!(shown.ends_with(&text[text.len() - TAIL..]));
        assert!(shown.len() < HEAD + TAIL + 300, "{}", shown.len());
        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        let path = files[0].path();
        assert!(shown.contains(path.to_str().unwrap()), "the path is in the prompt");
        assert_eq!(fs::read_to_string(&path).unwrap(), text, "the file holds everything");
        // The same output twice is one file.
        bounded_in(dir.path(), "check", &text);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn cuts_fall_on_character_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let text = "é".repeat(10_000);
        let shown = bounded_in(dir.path(), "x", &text);
        assert!(shown.is_char_boundary(shown.len()));
        assert!(shown.contains("bytes left out"));
    }

    #[test]
    fn files_older_than_a_week_are_cleaned_and_newer_ones_stay() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.txt");
        let new = dir.path().join("new.txt");
        fs::write(&old, "a").unwrap();
        fs::write(&new, "b").unwrap();
        let eight_days_ago = std::time::SystemTime::now() - Duration::from_secs(8 * 24 * 3600);
        fs::OpenOptions::new().append(true).open(&old).unwrap().set_modified(eight_days_ago).unwrap();
        assert_eq!(cleanup(dir.path(), std::time::SystemTime::now(), KEEP), 1);
        assert!(!old.exists() && new.exists());
        assert_eq!(cleanup(&dir.path().join("missing"), std::time::SystemTime::now(), KEEP), 0);
    }
}
