//! CLI integration tests that do not require a Docker daemon.
//! Exercises version/help output and Docker-free commands (config, init, validate).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

const FIXTURE: &str = include_str!("fixtures/docker-compose.fixture.yml");

fn fixture_project_dir() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    let proj = dir.path().join("myproj");
    fs::create_dir_all(&proj).expect("mkdir proj");
    fs::write(proj.join("docker-compose.fixture.yml"), FIXTURE).expect("write fixture");
    dir
}

#[test]
fn version_prints_semver() {
    Command::cargo_bin("conduit")
        .expect("binary exists")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

#[test]
fn completions_generates_shell_scripts() {
    for shell in ["bash", "zsh", "fish"] {
        Command::cargo_bin("conduit")
            .expect("binary exists")
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("conduit"));
    }
}

#[test]
fn help_lists_commands() {
    Command::cargo_bin("conduit")
        .expect("binary exists")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ui"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("submod"));
}

#[test]
fn every_subcommand_help_parses_without_clap_panic() {
    // Regression: clap panics at startup in debug builds when a subcommand flag
    // collides with a global short flag (e.g. `down -v` vs global `-v`).
    for sub in [
        "up",
        "down",
        "ps",
        "logs",
        "top",
        "db",
        "doctor",
        "init",
        "config",
        "config-validate",
        "route",
        "link",
        "unlink",
        "snapshot",
        "exec",
        "run",
        "cp",
        "env",
        "graph",
        "image",
        "bench",
        "ui",
        "describe",
        "submod",
        "port-forward",
        "proxy",
        "connect",
        "completions",
    ] {
        Command::cargo_bin("conduit")
            .expect("binary exists")
            .args([sub, "--help"])
            .assert()
            .success()
            .stderr(predicate::str::contains("panicked").not());
    }
}

#[test]
fn config_global_prints_config() {
    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args(["config", "--global"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Global Config"));
}

#[test]
fn config_project_prints_defaults() {
    let dir = fixture_project_dir();
    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args(["config", "--project-dir", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project Config"));
}

#[test]
fn init_generates_conduit_yml() {
    let dir = fixture_project_dir();
    let proj = dir.path().join("myproj");
    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args([
            "init",
            "--project-dir",
            proj.to_str().unwrap(),
            "--file",
            "docker-compose.fixture.yml",
        ])
        .assert()
        .success();

    let generated = proj.join(".conduit.yml");
    assert!(generated.exists(), ".conduit.yml was not generated");
    let contents = fs::read_to_string(&generated).expect("read .conduit.yml");
    assert!(contents.contains("project: myproj"), "project name missing");
    assert!(
        contents.contains("web.myproj.localhost"),
        "web route missing"
    );
    assert!(contents.contains("compose_file: docker-compose.fixture.yml"));
}

#[test]
fn init_refuses_to_overwrite() {
    let dir = fixture_project_dir();
    let proj = dir.path().join("myproj");
    fs::write(proj.join(".conduit.yml"), "project: existing\n").expect("pre-existing config");

    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args([
            "init",
            "--project-dir",
            proj.to_str().unwrap(),
            "--file",
            "docker-compose.fixture.yml",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn config_validate_accepts_good_config() {
    let dir = fixture_project_dir();
    let proj = dir.path().join("myproj");
    fs::write(proj.join(".conduit.yml"), "project: myproj\n").expect("write config");

    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args(["config-validate", "--project-dir", proj.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configuration valid"));
}

#[test]
fn config_validate_rejects_bad_config() {
    let dir = fixture_project_dir();
    let proj = dir.path().join("myproj");
    fs::write(
        proj.join(".conduit.yml"),
        "routes:\n  web:\n    domain: \"\"\n",
    )
    .expect("write config");

    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args(["config-validate", "--project-dir", proj.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error(s)"));
}

#[test]
fn config_validate_rejects_malformed_yaml() {
    let dir = fixture_project_dir();
    let proj = dir.path().join("myproj");
    fs::write(proj.join(".conduit.yml"), "project: [unclosed\n").expect("write config");

    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args(["config-validate", "--project-dir", proj.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("YAML parse error"));
}

#[test]
fn validate_detects_missing_compose_file() {
    let dir = tempdir().expect("tempdir");
    let proj = dir.path().join("nocompose");
    fs::create_dir_all(&proj).expect("mkdir");
    fs::write(
        proj.join(".conduit.yml"),
        "project: nocompose\ncompose_file: missing.yml\n",
    )
    .expect("write config");

    Command::cargo_bin("conduit")
        .expect("binary exists")
        .args(["config-validate", "--project-dir", proj.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "compose_file 'missing.yml' not found",
        ));
}
