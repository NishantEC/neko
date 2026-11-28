//! Cats (docs/knowledge/cats.md): named agents with a face. Since version 20 every cat is a **Cattain**: it has its own thread to talk
//! to, its own folder of four files (RULES.md, CREW.md, NOTES.md, DIGEST.md), an optional project, a model and an access mode. The
//! daemon keeps them; a screen lists and changes them through requests.

use serde::{Deserialize, Serialize};

/// The longest name a cat may have, in characters.
pub const MAX_NAME: usize = 40;
/// The longest purpose line, in characters.
pub const MAX_PURPOSE: usize = 200;
/// The longest standing rules, in bytes (16 KB): the most the person may save in RULES.md or CREW.md.
pub const MAX_RULES: usize = 16 * 1024;
/// The most of a Cattain's file a screen is sent or a run is handed, in bytes (256 KB); a longer file is cut, and says so.
pub const MAX_FILE_READ: usize = 256 * 1024;
/// How many cats may be active at once.
pub const MAX_ACTIVE: usize = 16;

/// A Cattain's own rules: they reach its own thread on every run.
pub const RULES: &str = "RULES.md";
/// What every crew thread reads on every run, verbatim.
pub const CREW: &str = "CREW.md";
/// The Cattain's own plan, which it keeps.
pub const NOTES: &str = "NOTES.md";
/// What happened, which it keeps for the person.
pub const DIGEST: &str = "DIGEST.md";
/// A Cattain's four files, in the order they are listed.
pub const FILES: &[&str] = &[RULES, CREW, NOTES, DIGEST];

/// Whether the person may save `name` from a screen (RULES.md and CREW.md). NOTES.md and DIGEST.md are the Cattain's to write; a
/// screen reads them.
pub fn editable(name: &str) -> bool {
    name == RULES || name == CREW
}

/// The models a Cattain's Profile offers, as the CLI's aliases (`claude --model`), with their words. Any other name the daemon is
/// given is kept and passed as it is.
pub const MODELS: &[(&str, &str)] = &[("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];

/// The faces a cat may have, by id. The app draws each; the ids are the contract.
pub const FACES: &[&str] = &["tabby", "ginger", "tuxedo", "calico", "siamese", "void", "snow", "smoke"];

/// The three cats a new install starts with: their fixed ids, names and faces. Ordinary cats: they can be renamed or retired.
pub const STARTERS: &[(&str, &str, &str)] = &[("cat_miso", "miso", "ginger"), ("cat_mochi", "mochi", "snow"), ("cat_tofu", "tofu", "calico")];

/// A named agent with a face, its own thread and its own files: a Cattain (version 20).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cat {
    /// `cat_` and a name for a starter cat, else `cat_` and 26 letters and digits.
    pub id: String,
    /// 1 to 40 characters, one line, unique among all cats (retired ones too), compared without case.
    pub name: String,
    /// One of [`FACES`].
    pub face: String,
    /// A short line about what it is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// Its project (a workspace's id), when it has one: its own thread may read the project's folder (`--add-dir`), and a crew
    /// thread started from it is made there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Its own thread (version 20): the conversation the person has with it. Every cat has one once the daemon has opened its store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<crate::TaskId>,
    /// The model its own thread asks for (`--model`: an alias such as `sonnet`, or a full name). None: the CLI's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The access its own thread runs with. None: the mode in Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<crate::PermissionMode>,
    pub created_ms: i64,
    /// When it was retired: its threads stay readable; it takes no new crew until it is brought back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_ms: Option<i64>,
}

impl Cat {
    /// Not retired: it takes new work.
    pub fn is_active(&self) -> bool {
        self.retired_ms.is_none()
    }
}

/// One of a Cattain's files as it is now (`get_cattain_file`, `save_cattain_file`, `restore_cattain_file`, and the
/// `cattain_file_changed` event when it changed, by the person or by the Cattain).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CattainFile {
    /// The Cattain's id.
    pub cat: String,
    /// One of [`FILES`].
    pub name: String,
    /// Its words, at most [`MAX_FILE_READ`] bytes of them.
    pub text: String,
    /// The file was longer and `text` is cut.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
    /// One earlier version is kept: Restore previous puts it back.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub previous: bool,
    /// When it last changed (saved, restored, or written by the Cattain) as Neko saw it; None: never since it was made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_ms: Option<i64>,
    /// Where it is on disk (Reveal in Finder).
    pub path: String,
    /// The person may save it ([`editable`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub editable: bool,
}

/// The version of a block's words (a Cattain's RULES.md, a crew's CREW.md): FNV-1a (64 bits) of their bytes, as 16 hex digits. It
/// only tells versions apart; it is not a secret and not a check against tampering.
pub fn rules_hash(rules: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in rules.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_tells_versions_apart_and_is_stable() {
        assert_eq!(rules_hash(""), "cbf29ce484222325");
        assert_eq!(rules_hash("a"), "af63dc4c8601ec8c");
        assert_ne!(rules_hash("Write tests first."), rules_hash("Write tests first. "));
        assert!(STARTERS.iter().all(|(_, _, f)| FACES.contains(f)));
    }

    #[test]
    fn only_rules_and_crew_are_the_persons_to_save() {
        assert!(editable(RULES) && editable(CREW));
        assert!(!editable(NOTES) && !editable(DIGEST) && !editable("reports/a.md"));
        assert_eq!(FILES, &["RULES.md", "CREW.md", "NOTES.md", "DIGEST.md"]);
    }
}
