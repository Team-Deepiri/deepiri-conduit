use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmoduleConflict {
    pub path: String,
    pub left_commit: Option<String>,
    pub right_commit: Option<String>,
    pub left_branch: String,
    pub right_branch: String,
    pub resolution: Option<SubmoduleResolution>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubmoduleResolution {
    UseLeft,
    UseRight,
    UseHigher,
}

impl SubmoduleConflict {
    pub fn new(
        path: String,
        left_commit: Option<String>,
        right_commit: Option<String>,
        left_branch: String,
        right_branch: String,
    ) -> Self {
        Self {
            path,
            left_commit,
            right_commit,
            left_branch,
            right_branch,
            resolution: None,
        }
    }

    pub fn resolve(&mut self, resolution: SubmoduleResolution) {
        self.resolution = Some(resolution);
    }

    pub fn resolved(&self) -> bool {
        self.resolution.is_some()
    }
}
