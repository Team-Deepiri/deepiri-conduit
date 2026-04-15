use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;

use crate::cli::GlobalOpts;
use crate::submod::resolver::{fetch_if_needed, RepoScanner, SubmoduleResolver};

const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[1;33m";
const BLUE: &str = "\x1b[0;34m";
const MAGENTA: &str = "\x1b[0;35m";
const CYAN: &str = "\x1b[0;36m";
const WHITE: &str = "\x1b[1;37m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

fn banner() {
  println!();
  println!("{MAGENTA}  ██████╗ ██████╗       {WHITE}██╗    ██╗ █████╗ ██████╗ ███████╗{RESET}");
  println!("{MAGENTA} ██╔════╝ ██╔══██╗      {WHITE}██║    ██║██╔══██╗██╔══██╗██╔════╝{RESET}");
  println!("{MAGENTA} ██║  ███╗██████╔╝      {WHITE}██║ █╗ ██║███████║██████║ █████╗  {RESET}");
  println!("{MAGENTA} ██║   ██║██╔══██╗      {WHITE}██║███╗██║██╔══██║██╔══██╗██╔══╝  {RESET}");
  println!("{MAGENTA} ╚██████╔╝██║  ██║      {WHITE}╚███╔███╔╝██║  ██║██║  ██║███████╗{RESET}");
  println!("{MAGENTA}  ╚═════╝ ╚═╝  ╚═╝       {WHITE}╚══╝╚══╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝{RESET}");
  println!("{CYAN}  ══ SUBMOD RESOLVER ═══════════════════════════════════{RESET}");
  println!();
}

fn info(msg: &str) {
  println!("{CYAN}➜{RESET} {}", msg);
}

fn success(msg: &str) {
  println!("{GREEN}✓{RESET} {}", msg);
}

fn warn(msg: &str) {
  println!("{YELLOW}⚠{RESET} {}", msg);
}

fn error(msg: &str) {
  eprintln!("{RED}✗{RESET} {}", msg);
}

fn prompt(label: &str) -> String {
  print!("{CYAN}{label}{RESET}");
  io::stdout().flush().ok();
  let mut input = String::new();
  io::stdin().read_line(&mut input).ok();
  input.trim().to_string()
}

pub struct Subrepo {
  pub name: String,
  pub path: PathBuf,
  pub has_submodules: bool,
}

pub fn detect_repos_with_submodules(search_paths: &[PathBuf]) -> Vec<Subrepo> {
  let mut repos = Vec::new();

  for base in search_paths {
    if !base.exists() {
      continue;
    }

    if let Ok(entries) = std::fs::read_dir(base) {
      for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
          continue;
        }

        let gitmodules = path.join(".gitmodules");
        if gitmodules.exists() {
          if let Some(name) = path.file_name() {
            repos.push(Subrepo {
              name: name.to_string_lossy().to_string(),
              path,
              has_submodules: true,
            });
          }
        }
      }
    }
  }

  repos.sort_by(|a, b| a.name.cmp(&b.name));
  repos
}

#[derive(Parser)]
#[command(before_help = None)]
pub struct SubmodArgs {
  #[arg(help = "Repo path or name (leave empty to auto-detect)")]
  pub repo: Option<String>,

  #[arg(help = "Left/base branch")]
  pub left: Option<String>,

  #[arg(help = "Right/compare branch")]
  pub right: Option<String>,

  #[arg(long, help = "Auto-resolve and push")]
  pub auto: bool,

  #[arg(long, help = "Init/update submodules before resolving")]
  pub init_submodules: bool,

  #[arg(long, help = "Force clone submodules even if they exist")]
  pub force_clone: bool,

  #[arg(long, help = "Commit message")]
  pub message: Option<String>,

  #[arg(long, help = "Interactive mode")]
  pub interactive: bool,
}

pub async fn run(args: SubmodArgs, _globals: &GlobalOpts) -> anyhow::Result<()> {
  let search_paths = vec![
    dirs::home_dir()
      .map(|h| h.join("Documents/Deepiri"))
      .unwrap_or_else(|| PathBuf::from("Documents/Deepiri")),
  ];

  banner();

  if args.repo.is_none() && args.interactive {
    return interactive_mode(&search_paths).await;
  }

  if args.repo.is_none() {
    show_detected_repos(&search_paths);
    return Ok(());
  }

  let repo_arg = args.repo.as_ref().unwrap();
  let repo_path = resolve_repo(repo_arg, &search_paths)?;

  let left = args.left.as_deref().unwrap_or("main");
  let right = args.right.as_deref().unwrap_or("origin/dev");

  run_resolve(&repo_path, left, right, args.auto, args.init_submodules, args.force_clone, args.message.as_deref()).await
}

fn show_detected_repos(search_paths: &[PathBuf]) {
  info("Scanning for repos with submodules...\n");

  let repos = detect_repos_with_submodules(search_paths);

  if repos.is_empty() {
    warn("No repos with submodules found in Documents/Deepiri");
    println!();
    println!("Run {CYAN}conduit submod --interactive{RESET} to select manually");
    return;
  }

  println!("{WHITE}Repos with submodules:{RESET}");
  println!();
  for (i, repo) in repos.iter().enumerate() {
    println!("  {BLUE}{}.{RESET} {WHITE}{}{RESET}", i + 1, repo.name);
  }
  println!();
  println!("{CYAN}Usage:{RESET}");
  println!("  conduit submod <name> <left-branch> <right-branch> [--auto]");
  println!("  conduit submod --interactive");
  println!();
}

async fn interactive_mode(search_paths: &[PathBuf]) -> anyhow::Result<()> {
  let repos = detect_repos_with_submodules(search_paths);

  if repos.is_empty() {
    error("No repos with submodules found");
    return Ok(());
  }

  println!("{WHITE}Select a repo:{RESET}");
  println!();
  for (i, repo) in repos.iter().enumerate() {
    println!("  {BLUE}{}.{RESET} {WHITE}{}{RESET}", i + 1, repo.name);
  }
  println!();

  let idx: usize = prompt("Repo [1]: ").parse().unwrap_or(1);
  let idx = if idx == 0 { 1 } else { idx };

  if idx > repos.len() {
    error("Invalid selection");
    return Ok(());
  }

  let selected = &repos[idx - 1];
  info(&format!("Selected: {}\n", selected.name));

  let branches = list_branches(&selected.path)?;
  println!("{WHITE}Available branches:{RESET} ({BLUE}Top 20{RESET})");
  println!();
  for (i, branch) in branches.iter().take(20).enumerate() {
    println!("  {BLUE}{}.{RESET} {WHITE}{}", i + 1, branch);
  }
  println!();

  let left = prompt("Left branch (base) [main]: ");
  let left = if left.is_empty() { "main".to_string() } else { left };

  let right = prompt("Right branch (compare) [origin/dev]: ");
  let right = if right.is_empty() { "origin/dev".to_string() } else { right };

  println!();

  let auto = prompt("Auto-resolve and push? [y/N]: ");
  let auto = auto.to_lowercase().starts_with('y');

  run_resolve(&selected.path, &left, &right, auto, false, false, None).await
}

async fn run_resolve(
  repo_path: &Path,
  left_branch: &str,
  right_branch: &str,
  auto: bool,
  init_submodules: bool,
  force_clone: bool,
  message: Option<&str>,
) -> anyhow::Result<()> {
  let repo_str = repo_path.to_str().unwrap();

  info(&format!("Comparing: {} → {}\n", left_branch, right_branch));

  fetch_if_needed(repo_str)?;
  let resolver = SubmoduleResolver::new(repo_str);

  if init_submodules || force_clone {
    let needs_init = resolver.need_init().await;
    if needs_init || force_clone {
      println!();
      info("Initializing submodules...");
      match resolver.init_submodules().await {
        Ok(_) => success("Submodules initialized"),
        Err(e) => {
          warn(&format!("Submodule init failed: {}", e));
          println!("{}Resolving without updating submodules...{}", YELLOW, RESET);
        }
      }
    }
  }

  let mut conflicts = resolver.find_conflicts(left_branch, right_branch).await?;

  if conflicts.is_empty() {
    success("No submodule conflicts!");
    return Ok(());
  }

  println!("{WHITE}Found {}{YELLOW} conflict(s){RESET}:", conflicts.len());
  println!();

  for (i, c) in conflicts.iter().enumerate() {
    println!("  {MAGENTA}{}.{RESET} {WHITE}{}", i + 1, c.path);
    println!("      {WHITE}{}:{RESET} {YELLOW}{:?}", left_branch, c.left_commit.as_ref().map(|h| &h[..7]));
    println!("      {WHITE}{}:{RESET} {GREEN}{:?}", right_branch, c.right_commit.as_ref().map(|h| &h[..7]));
  }
  println!();

  if !auto {
    println!("{CYAN}Run with --auto to resolve using newer branch and push{RESET}");
    return Ok(());
  }

  info("Resolving conflicts...");
  resolver.resolve_all(&mut conflicts).await?;

  for conflict in &conflicts {
    if conflict.resolution.is_none() {
      continue;
    }
    match resolver.apply_resolution(conflict, right_branch).await {
      Ok(_) => success(&format!("Resolved: {}", conflict.path)),
      Err(e) => error(&format!("{}: {}", conflict.path, e)),
    }
  }

  let msg = message.unwrap_or("chore: resolve submodule conflicts");
  match resolver.commit_and_push(msg, Some(right_branch)).await {
    Ok(_) => {
      println!();
      success(&format!("Resolved and pushed to {}", right_branch));
    }
    Err(e) => {
      println!();
      error(&format!("Push failed: {}", e));
      warn("Changes staged locally. Commit manually or fix auth.");
    }
  }

  Ok(())
}

fn resolve_repo<'a>(repo_arg: &'a str, search_paths: &'a [PathBuf]) -> anyhow::Result<PathBuf> {
  if Path::new(repo_arg).is_absolute() && Path::new(repo_arg).exists() {
    return Ok(PathBuf::from(repo_arg));
  }

  for base in search_paths {
    if !base.exists() {
      continue;
    }
    let candidate = base.join(repo_arg);
    if candidate.exists() {
      return Ok(candidate);
    }
  }

  anyhow::bail!("Repo not found: {}", repo_arg)
}

fn list_branches(repo_path: &Path) -> anyhow::Result<Vec<String>> {
  let output = Command::new("git")
    .args(["-C", repo_path.to_str().unwrap(), "branch", "-a"])
    .output()?;

  if !output.status.success() {
    anyhow::bail!("Failed to list branches");
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let branches: Vec<String> = stdout
    .lines()
    .map(|l| l.trim().trim_start_matches("* ").to_string())
    .filter(|l| !l.is_empty())
    .collect();

  Ok(branches)
}