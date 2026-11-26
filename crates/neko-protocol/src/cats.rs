//! Cats (docs/knowledge/cats.md): the named agents who do the person's work. A cat has a name, a face, a purpose line and standing
//! rules; every task has exactly one. The daemon keeps them; a screen lists and changes them through requests.

use serde::{Deserialize, Serialize};

/// The longest name a cat may have, in characters.
pub const MAX_NAME: usize = 40;
/// The longest purpose line, in characters.
pub const MAX_PURPOSE: usize = 200;
/// The longest standing rules, in bytes (16 KB).
pub const MAX_RULES: usize = 16 * 1024;
/// How many cats may be active at once.
pub const MAX_ACTIVE: usize = 16;

/// The faces a cat may have, by id. The app draws each; the ids are the contract.
pub const FACES: &[&str] = &["tabby", "ginger", "tuxedo", "calico", "siamese", "void", "snow", "smoke"];

/// The three cats a new install starts with: their fixed ids, names and faces. Ordinary cats: they can be renamed or retired.
pub const STARTERS: &[(&str, &str, &str)] = &[("cat_miso", "miso", "ginger"), ("cat_mochi", "mochi", "snow"), ("cat_tofu", "tofu", "calico")];

/// A named agent with its own standing rules.
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
    /// Plain Markdown, at most [`MAX_RULES`] bytes, exactly as the person saved it. They reach the agent on every run.
    pub rules: String,
    /// The version of the rules: a hash of their bytes. A run records the one it used.
    pub rules_hash: String,
    /// The rules before the last save, which Restore puts back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_rules: Option<String>,
    /// When the rules were last saved or restored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_saved_ms: Option<i64>,
    /// The workspace whose new tasks go to it when no cat is picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    pub created_ms: i64,
    /// When it was retired: its tasks stay readable; it takes no new ones until it is brought back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_ms: Option<i64>,
}

impl Cat {
    /// Not retired: it takes new tasks.
    pub fn is_active(&self) -> bool {
        self.retired_ms.is_none()
    }
}

/// The version of a cat's rules: FNV-1a (64 bits) of their bytes, as 16 hex digits. It only tells versions apart; it is not a
/// secret and not a check against tampering.
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
}
