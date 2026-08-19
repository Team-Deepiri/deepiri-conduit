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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_conflict_is_unresolved() {
        let conflict = SubmoduleConflict::new(
            "libs/shared".into(),
            Some("abc123".into()),
            Some("def456".into()),
            "main".into(),
            "feature/x".into(),
        );
        assert_eq!(conflict.path, "libs/shared");
        assert_eq!(conflict.left_commit.as_deref(), Some("abc123"));
        assert_eq!(conflict.right_commit.as_deref(), Some("def456"));
        assert!(!conflict.resolved());
        assert!(conflict.resolution.is_none());
    }

    #[test]
    fn resolve_marks_conflict_resolved() {
        let mut conflict = SubmoduleConflict::new(
            "libs/shared".into(),
            None,
            Some("def456".into()),
            "main".into(),
            "feature/x".into(),
        );
        conflict.resolve(SubmoduleResolution::UseRight);
        assert!(conflict.resolved());
        assert_eq!(conflict.resolution, Some(SubmoduleResolution::UseRight));
    }

    #[test]
    fn resolution_enum_roundtrips_through_serde() {
        let json = serde_json::to_string(&SubmoduleResolution::UseHigher).unwrap();
        let parsed: SubmoduleResolution = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SubmoduleResolution::UseHigher);
    }
}
