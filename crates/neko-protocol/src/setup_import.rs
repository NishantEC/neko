//! Secret-free previews of configurations discovered on this Mac.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportConnection {
    pub id: String,
    pub name: String,
    pub source: String,
    /// None denotes a global definition; importing it does not grant access.
    pub repository: Option<String>,
    pub has_credentials: bool,
    pub enabled_at_source: bool,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportPreview {
    pub connections: Vec<ImportConnection>,
    pub repositories: Vec<String>,
    pub warnings: Vec<String>,
}
