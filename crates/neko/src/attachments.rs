//! Images pasted into the composer.
//!
//! **The mechanism is Paseo's own, read off a real transcript rather than
//! guessed.** `send_agent_prompt` takes exactly one thing — `prompt: String`
//! — with no attachment field anywhere in its schema, so an image cannot be
//! handed to an agent through the tool call. What Paseo does instead is
//! visible in the transcript it writes: it saves the attachment to a temp
//! directory and puts a markdown image reference to it in the prompt text,
//! `![Image](file:///var/folders/.../paseo-attachments-XXXX/<hash>.png`.
//! The agent's harness reads the path. This file does the same thing, which
//! is why an image pasted here reaches an agent at all.
//!
//! **Written where the agent can still read it later.** Not `/tmp`, which is
//! swept, and not a per-run temp directory that vanishes with the process:
//! a prompt referencing a file that has been deleted is worse than one that
//! never had the image, because it looks like it worked. These live beside
//! neko's other caches and are pruned by count, oldest first, the same rule
//! `conversation.rs` applies to the images it decodes *out* of transcripts.

use std::path::PathBuf;

/// Versioned, like every other cache directory here — the only staleness
/// check is a file's existence, so a change to what gets written has to
/// change the directory.
const GENERATION: &str = "v1";

/// How many pasted attachments to keep.
///
/// Generous: these are referenced by prompts that live in transcripts, and a
/// reference outliving its file is the failure this bound is trading against.
const MAX_ATTACHMENTS: usize = 200;

fn attachments_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/Caches/neko/composer-attachments")
            .join(GENERATION),
    )
}

/// Saves image bytes and returns the markdown reference to put in the prompt.
///
/// Keyed by a hash of the bytes, so pasting the same screenshot twice writes
/// one file — and so a paste is idempotent, which matters because a person
/// who pastes, deletes the text, and pastes again should not leave litter.
pub fn save_pasted_image(bytes: &[u8]) -> Option<String> {
    use std::hash::{Hash, Hasher};

    let dir = attachments_dir()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let extension = if bytes.starts_with(b"\x89PNG") { "png" } else { "tiff" };
    let path = dir.join(format!("{:016x}.{extension}", hasher.finish()));
    if !path.exists() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, bytes).ok()?;
        prune(&dir);
    }
    Some(markdown_reference(&path))
}

/// `![Image](file:///…)` — the exact shape Paseo writes, so an agent's
/// harness sees the same thing from either app.
pub fn markdown_reference(path: &std::path::Path) -> String {
    format!("![Image](file://{})", path.to_string_lossy())
}

fn prune(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= MAX_ATTACHMENTS {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    for (_, path) in files.iter().take(files.len() - MAX_ATTACHMENTS) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_is_the_shape_paseo_writes() {
        // Read off a real Paseo transcript, not invented: the agent harness
        // resolves this, so the format is a contract with something else.
        let path = PathBuf::from("/tmp/x/abc.png");
        assert_eq!(markdown_reference(&path), "![Image](file:///tmp/x/abc.png)");
    }

    #[test]
    fn the_same_image_pasted_twice_is_one_file() {
        let bytes = b"\x89PNG\r\n\x1a\nfixture-bytes";
        let Some(first) = save_pasted_image(bytes) else { return };
        let second = save_pasted_image(bytes).expect("second paste");
        assert_eq!(first, second, "keyed by content, so a repeat writes nothing new");
        if let Some(path) = first.strip_prefix("![Image](file://").and_then(|s| s.strip_suffix(")")) {
            assert!(std::path::Path::new(path).exists());
            assert!(path.ends_with(".png"), "the extension follows the bytes' own magic");
            let _ = std::fs::remove_file(path);
        }
    }
}
