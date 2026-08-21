# Changelog

All notable changes to **deepiri-conduit** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `conduit completions <bash|zsh|fish>` — generate shell completions via `clap_complete`.
- `conduit doctor --fix` — back up and reset a corrupt state file.
- `conduit top` — per-container CPU, memory, network and block I/O columns (like `docker stats`).
- Unit tests for compose types, global config, state serialization, TCP tunnel helpers,
  emit (generated YAML), and a Docker-free CLI suite (`tests/cli.rs`) that also guards
  every `conduit <sub> --help` against clap flag collisions.
- Shell-completion regression and subcommand help-parsing tests in CI.

### Changed

- Default proxy image is now `traefik:v3.6` (Docker Engine 29+ requires Docker API ≥ 1.44,
  which older Traefik images do not negotiate). Configurable via `proxy.image` in
  `~/.config/conduit/config.toml`.
- The compose validator command was renamed `config-validate` (the old `config` clap
  name collided with the config display command and crashed on every invocation).
- `conduit ps` and `conduit doctor` report the configured `proxy.http_port` instead of
  hardcoding `:80/:443`.
- `conduit route` reports the real `http` scheme instead of claiming TLS on every route.
- `conduit down --volumes` and `conduit bench --concurrency` are long-only flags
  (their short `-v`/`-c` collided with the global `-v`/`bench -c`).

### Fixed

- Generated compose no longer emits `name: null` / `version: null`, which `docker compose`
  rejected with `version must be a string`.
- Project containers now join the **same external network** the proxy is attached to;
  previously compose created a project-prefixed duplicate network and every route
  returned `504`.
- Connecting the proxy to a project network is idempotent — a repeated `conduit up`
  after a partial failure no longer errors with `403 endpoint already exists`.
- The Docker client connects over the unix socket (`bollard` `pipe` feature) — without it
  every Docker command failed with `URI scheme is not supported`.
- `normalize_repo_name` handles `.git` URLs correctly in submodule resolution.
- Removed a crate-wide `allow(dead_code)` and the dead code it masked.
- `config-validate` no longer crashes every invocation (clap name collision).

## [0.1.0] - 2026-04

### Added

- Initial release: `conduit up` / `down` / `ps` / `logs` / `db` / `doctor` / `ui`.
- Compose parsing via `docker compose config`, in-memory rewrite (strip ports, project
  network, Traefik labels) and generated `.conduit/cache/docker-compose.conduit.yml`.
- Shared Traefik proxy (`conduit-proxy`) with the Docker provider.
- `.conduit.yml` project config (routes, groups, `expose`, databases).
- `/etc/hosts` DNS sync with markers, multi-project safe.
- `conduit db` TCP tunnels with per-database connection-string formatting.
- CI (fmt, clippy, test, release build) and Linux x86_64 release workflow.
- Library crate + integration tests (no Docker required).
