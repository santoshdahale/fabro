//! Project domain: first-class project object.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A structurally resolved `[project]` view for consumers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectNamespace {
    pub name:        Option<String>,
    pub description: Option<String>,
    pub metadata:    HashMap<String, String>,
}
