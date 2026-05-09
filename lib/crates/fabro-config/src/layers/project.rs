//! Sparse `[project]` settings layer definitions.

use serde::{Deserialize, Serialize};

use super::maps::ReplaceMap;

/// A sparse `[project]` layer as it appears in a single settings file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, fabro_macros::Combine)]
#[serde(deny_unknown_fields)]
pub struct ProjectLayer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name:        Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Deprecated parse-only field. Project workflows always live under the
    /// `.fabro` directory that contains `project.toml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory:   Option<String>,
    #[serde(default, skip_serializing_if = "ReplaceMap::is_empty")]
    pub metadata:    ReplaceMap<String>,
}
