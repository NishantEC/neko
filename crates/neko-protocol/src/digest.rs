//! The morning digest: what finished, what is stuck, what needs a decision,
//! today's calendar and yesterday's spend, composed from data Neko already
//! has (no model call). See docs/knowledge/morning-digest.md.
use serde::{Deserialize, Serialize};

/// One line of the digest. `default` is set on a decision that has one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DigestLine {
    pub text: String,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Digest {
    /// When it was composed (ms since the epoch).
    pub at_ms: i64,
    /// The local day it is for, `YYYY-MM-DD`.
    pub day: String,
    /// One sentence: the status, then the bottleneck.
    pub headline: String,
    pub finished: Vec<DigestLine>,
    pub stuck: Vec<DigestLine>,
    pub decisions: Vec<DigestLine>,
    /// "3 events today, first: Standup at 09:30", empty when Calendar is off.
    pub calendar: String,
    /// "Yesterday: $1.20 reported", empty when unknown.
    pub spend: String,
    /// The person read it (completed its reminder or dismissed it on Home).
    pub read: bool,
}
