# Roadmap — deepiri-conduit

This document is the **product roadmap** (milestones, priorities, exit criteria). For architecture, CLI details, and deep technical design, see [**PLAN.md**](PLAN.md).

---

## Vision

A **small, fast CLI** that makes multi-service Docker Compose tolerable locally: no port wars, sane hostnames, one command to bring stacks up/down, and safe DB access when you need it—without owning production orchestration.

---

## Current state (v0.1.x)

**Shipped**

- Core loop: parse compose → rewrite → emit `.conduit/cache/docker-compose.conduit.yml` → `docker compose -p … up/down`
- Traefik sidecar with **Docker provider** (labels), shared proxy, project networks
- `.conduit.yml`: routes, groups, `expose`, database hints for `conduit db`
- State under `~/.local/share/conduit/`, `/etc/hosts` sync (multi-project safe)
- `conduit db` with env + YAML credential resolution
- CI: fmt, clippy, test, release build ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)); optional tagged [releases](.github/workflows/release.yml)
- Library crate + integration tests (`tests/integration_compose.rs`, no Docker required)
- Apache-2.0, `Cargo.lock` tracked

**Known gaps (acceptable for 0.1)**

- No Homebrew/apt distribution yet (build from source / GitHub Releases TBD)
- TLS/HTTPS for local routes is minimal (HTTP-first via Traefik `web` entrypoint)
- Large compose edge cases (extends, profiles, bake) rely on `docker compose config` + serde

---

## Near term (v0.2 — polish & adoption)

| Theme | Outcomes |
|--------|----------|
| **Reliability** | ✅ Integration tests (`tests/integration_compose.rs`) — parse → rewrite → emit round-trip using a YAML fixture **without Docker**; CI runs them on every push. |
| **UX** | ✅ `conduit doctor` expanded: `DOCKER_HOST`, writable state/config dirs, clearer compose failure hints, port 80 context. |
| **Docs** | ✅ README: Deepiri-style `.conduit.yml` example + **Troubleshooting** (WSL2, ports, hosts, state). |
| **Release** | ✅ `.github/workflows/release.yml` — on `v*` tags, builds Linux x86_64 binary + `sha256` and attaches to GitHub Releases. Optional: Homebrew / crates.io later. |

**Exit criteria:** A new contributor can follow README + run the full happy path on Linux/macOS without reading PLAN.md.

---

## v1.0 — “default dev tool for our compose”

| Theme | Outcomes |
|--------|----------|
| **Stability** | Semver API for `.conduit.yml` (documented schema + migration notes on breaking changes) |
| **Distribution** | At least one binary distribution path (e.g. GitHub Releases artifacts + checksums; optional Homebrew formula) |
| **Proxy** | Documented story for HTTPS locally (mkcert, Traefik TLS store, or documented limitation) |
| **Operations** | `conduit` safe with multiple projects daily; state repair or `conduit doctor --fix` where safe |

**Exit criteria:** Team agrees `conduit up` / `conduit down` is the **documented** default for local Deepiri platform dev; rollback path is “git clean `.conduit/` + compose down”.

---

## Post–1.0 (backlog — not committed)

- **Kubernetes / remote**: out of scope unless we explicitly fork a “conduit-k8s” story; keep compose-first.
- **Podman**: compatibility pass (socket path, compose flavor).
- **Plugin / IDE**: only if CLI stabilizes.
- **Metrics / telemetry**: opt-in only, never default.

Items here are **intentionally unordered**; they move into versioned milestones when we pick them up.

---

## How we use this file

1. **Planning a release** — Move items from “Post–1.0” or “Near term” into a GitHub milestone and link PRs.
2. **Avoid drift** — If PLAN.md’s technical roadmap disagrees with this file, **ROADMAP.md wins** for priorities; update PLAN.md’s architecture sections to match decisions.
3. **Review cadence** — Revisit quarterly or at each minor release.

---

*Last updated: 2026-04*
