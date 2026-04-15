use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::submod::conflict::{SubmoduleConflict, SubmoduleResolution};

pub struct SubmoduleResolver {
    repo_path: String,
}

pub struct RepoScanner {
    search_paths: Vec<PathBuf>,
}

impl RepoScanner {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let default_paths = vec![
            home.clone(),
            home.join("code"),
            home.join("workspace"),
            home.join("projects"),
            home.join("dev"),
            home.join("Documents/Deepiri"),
        ];
        Self {
            search_paths: default_paths,
        }
    }

    pub fn with_paths(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            search_paths: paths.into_iter().collect(),
        }
    }

    pub fn find_local_repo(&self, url_or_name: &str) -> Option<PathBuf> {
        let normalized = normalize_repo_name(url_or_name);
        for base in &self.search_paths {
            if !base.exists() {
                continue;
            }
            if let Some(path) = self.scan_tree(&base, &normalized, 3) {
                return Some(path);
            }
        }
        None
    }

    fn scan_tree(&self, base: &Path, name: &str, max_depth: usize) -> Option<PathBuf> {
        self.scan_recursive(base, name, max_depth, 0)
    }

    fn scan_recursive(&self, dir: &Path, name: &str, max_depth: usize, current_depth: usize) -> Option<PathBuf> {
        if current_depth > max_depth {
            return None;
        }
        let git_dir = dir.join(".git");
        if git_dir.is_dir() {
            if let Some(basename) = dir.file_name() {
                if basename.to_string_lossy().to_lowercase() == name.to_lowercase() {
                    return Some(dir.to_path_buf());
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(result) = self.scan_recursive(&path, name, max_depth, current_depth + 1) {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    pub fn clone_repo(&self, url: &str, target_path: &Path) -> anyhow::Result<()> {
        if target_path.exists() {
            return Ok(());
        }
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let output = Command::new("git")
            .args(["clone", url, target_path.to_str().unwrap()])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("clone failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(())
    }
}

impl Default for RepoScanner {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_repo_name(input: &str) -> String {
    let input = input.trim();
    if input.ends_with(".git") {
        input.trim_end_matches(".git").to_string()
    } else {
        input.split('/').last().unwrap_or(input).to_string()
    }
}

fn normalize_branch(branch: &str) -> String {
    let b = branch.trim();
    if b.starts_with("origin/") || b.starts_with("remotes/") {
        b.to_string()
    } else if b.contains('/') && !b.starts_with("HEAD") {
        format!("origin/{}", b)
    } else {
        b.to_string()
    }
}

pub fn fetch_if_needed(repo_path: &str) -> anyhow::Result<()> {
    let output = Command::new("git")
        .args(["-C", repo_path, "fetch", "origin", "--prune"])
        .output()?;
    if !output.status.success() {
        eprintln!("Fetch warning: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

pub fn get_branch_commitdate(repo_path: &str, branch: &str) -> anyhow::Result<i64> {
    let normalized = normalize_branch(branch);
    let output = Command::new("git")
        .args(["-C", repo_path, "log", "-1", "--format=%ct", &normalized])
        .output()?;
    if !output.status.success() {
        anyhow::bail!("failed to get commit date for {}", branch);
    }
    let date_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    date_str.parse().map_err(|_| anyhow::anyhow!("invalid date"))
}

impl SubmoduleResolver {
    pub fn new(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    pub async fn find_conflicts(
        &self,
        left_branch: &str,
        right_branch: &str,
    ) -> anyhow::Result<Vec<SubmoduleConflict>> {
        fetch_if_needed(&self.repo_path)?;

        let left_submods = self.list_submodules(left_branch)?;
        let right_submods = self.list_submodules(right_branch)?;

        let mut conflicts = Vec::new();
        for (path, left_oid) in &left_submods {
            let right_oid = right_submods.get(path);
            let is_different = match (left_oid.as_str(), right_oid.map(|s| s.as_str())) {
                (l, Some(r)) => {
                    let d = l != r;
                    d
                },
                (l, None) => true,
            };
            if is_different {
                conflicts.push(SubmoduleConflict::new(
                    path.clone(),
                    Some(left_oid.clone()),
                    right_oid.map(|s| s.clone()),
                    left_branch.to_string(),
                    right_branch.to_string(),
                ));
            }
        }
        for (path, right_oid) in &right_submods {
            if !left_submods.contains_key(path) {
                conflicts.push(SubmoduleConflict::new(
                    path.clone(),
                    None,
                    Some(right_oid.clone()),
                    left_branch.to_string(),
                    right_branch.to_string(),
                ));
            }
        }

        Ok(conflicts)
    }

    pub async fn resolve_all(&self, conflicts: &mut [SubmoduleConflict]) -> anyhow::Result<()> {
        let left_date = get_branch_commitdate(&self.repo_path, &conflicts.first().map(|c| c.left_branch.as_str()).unwrap_or("main")).unwrap_or(0);
        let right_date = get_branch_commitdate(&self.repo_path, &conflicts.first().map(|c| c.right_branch.as_str()).unwrap_or("main")).unwrap_or(0);

        let resolution = if right_date >= left_date {
            SubmoduleResolution::UseRight
        } else {
            SubmoduleResolution::UseLeft
        };

        for conflict in conflicts.iter_mut() {
            conflict.resolve(resolution);
        }

        Ok(())
    }

    pub async fn init_submodules(&self) -> anyhow::Result<()> {
        let output = Command::new("git")
            .args(["-C", &self.repo_path, "submodule", "update", "--init", "--recursive"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Permission denied") || stderr.contains("publickey") {
                anyhow::bail!("Submodule auth failed. Set up SSH deploy key or use HTTPS.");
            }
            anyhow::bail!("Failed to init submodules: {}", stderr);
        }

        Ok(())
    }

    pub async fn need_init(&self) -> bool {
        let output = Command::new("git")
            .args(["-C", &self.repo_path, "submodule", "status"])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.contains("-") || stdout.contains("+")
            }
            _ => true,
        }
    }

    pub async fn apply_resolution(&self, conflict: &SubmoduleConflict, _target_branch: &str) -> anyhow::Result<()> {
        let resolution = conflict
            .resolution
            .expect("conflict must be resolved first");

        let commit = match resolution {
            SubmoduleResolution::UseRight => conflict.right_commit.as_ref(),
            SubmoduleResolution::UseLeft => conflict.left_commit.as_ref(),
            SubmoduleResolution::UseHigher => {
                let left = conflict.left_commit.as_ref().and_then(|c| commit_score(c));
                let right = conflict.right_commit.as_ref().and_then(|c| commit_score(c));
                match (left, right) {
                    (Some(l), Some(r)) if r > l => conflict.right_commit.as_ref(),
                    _ => conflict.left_commit.as_ref(),
                }
            }
        };

        let commit = commit.ok_or_else(|| anyhow::anyhow!("no commit to apply for {}", conflict.path))?;

        let path = Path::new(&self.repo_path).join(&conflict.path);
        let output = Command::new("git")
            .args(["-C", path.to_str().unwrap(), "checkout", commit])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("failed to update submodule: {}", String::from_utf8_lossy(&output.stderr));
        }

        let output = Command::new("git")
            .args(["-C", &self.repo_path, "submodule", "update", "--init", &conflict.path])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("failed to update submodule in parent: {}", String::from_utf8_lossy(&output.stderr));
        }

        Ok(())
    }

    pub async fn commit_and_push(&self, message: &str, target_branch: Option<&str>) -> anyhow::Result<()> {
        let output = Command::new("git")
            .args(["-C", &self.repo_path, "add", "-A"])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("git add failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let output = Command::new("git")
            .args(["-C", &self.repo_path, "commit", "-m", message])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("nothing to commit") {
                println!("No changes to commit.");
                return Ok(());
            }
            anyhow::bail!("git commit failed: {}", stderr);
        }

        let branch = target_branch.unwrap_or("HEAD");
        let output = Command::new("git")
            .args(["-C", &self.repo_path, "push", "origin", branch])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("git push failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        Ok(())
    }

    fn list_submodules(&self, branch: &str) -> anyhow::Result<HashMap<String, String>> {
        let input = branch.to_string();
        let normalized = normalize_branch(&input);
        
        let output = Command::new("git")
            .args(["-C", &self.repo_path, "ls-tree", &normalized])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("failed to list submodules: {}", String::from_utf8_lossy(&output.stderr));
        }

        let mut submods = HashMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if !line.starts_with("160000 commit") {
                continue;
            }
            if let Some(tab_pos) = line.find('\t') {
                let oid = line[15..tab_pos].to_string();
                let path = line[tab_pos + 1..].to_string();
                submods.insert(path, oid);
            }
        }
        Ok(submods)
    }
}

fn commit_score(commit: &str) -> Option<u32> {
    commit.get(0..40).map(|c| {
        c.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_digit(16).unwrap_or(0))
            .take(8)
            .sum()
    })
}