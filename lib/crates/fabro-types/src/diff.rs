use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffStats {
    pub additions: i64,
    pub deletions: i64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub files_changed: i64,
    pub additions:     i64,
    pub deletions:     i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDiff {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch:   Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<DiffSummary>,
}
