//! Integration tests: parse → rewrite → emit round-trip (no Docker daemon required).

use deepiri_conduit::compose::{emit, parser, rewriter};
use deepiri_conduit::config::conduit_yml::ConduitConfig;
use deepiri_conduit::project_id;
use std::fs;
use tempfile::tempdir;

const FIXTURE: &str = include_str!("fixtures/docker-compose.fixture.yml");

#[test]
fn parse_rewrite_emit_roundtrip() {
    let mut compose = parser::parse_str(FIXTURE).expect("parse fixture");

    let config = ConduitConfig {
        project: Some("fixtureproj".into()),
        compose_file: Some("docker-compose.fixture.yml".into()),
        domain: Some("fixture.localhost".into()),
        routes: None,
        groups: None,
        expose: None,
        env: None,
        health: None,
        databases: None,
    };

    let result = rewriter::rewrite(&mut compose, &config, "fixtureproj");
    assert!(
        result.network_name.contains("fixtureproj"),
        "network: {}",
        result.network_name
    );

    assert!(compose.services["web"].ports.is_none());
    assert!(compose.services["postgres"].ports.is_none());

    let web_labels = compose.services["web"].labels.as_ref().unwrap().as_map();
    assert_eq!(web_labels.get("conduit.managed"), Some(&"true".to_string()));
    assert_eq!(
        web_labels.get("conduit.project"),
        Some(&"fixtureproj".to_string())
    );
    assert!(web_labels.contains_key("traefik.enable"));

    let dir = tempdir().expect("tempdir");
    let path = emit::write_generated(dir.path(), &compose).expect("emit");
    assert!(path.exists());

    let roundtrip = fs::read_to_string(&path).expect("read emitted");
    let compose2 = parser::parse_str(&roundtrip).expect("re-parse emitted");
    assert_eq!(compose2.services.len(), compose.services.len());
}

#[test]
fn rewrite_network_name() {
    let mut compose = parser::parse_str(FIXTURE).expect("parse");
    let config = ConduitConfig {
        project: Some("ab".into()),
        compose_file: None,
        domain: Some("ab.localhost".into()),
        routes: None,
        groups: None,
        expose: None,
        env: None,
        health: None,
        databases: None,
    };
    let r = rewriter::rewrite(&mut compose, &config, "ab");
    assert_eq!(r.network_name, "conduit-ab");
}

#[test]
fn sanitize_project_id() {
    assert_eq!(
        project_id::sanitize_compose_project("My_Project!"),
        "my_project"
    );
}
