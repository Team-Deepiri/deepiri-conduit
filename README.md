# conduit

**Local dev orchestrator for multi-service Docker Compose projects.**

Single binary. No compose port clashes (by default). HTTP routing via **Traefik** + **Docker labels**. On-demand DB tunnels.

## How it works

1. **Parse** your compose file (`docker compose config` when available).
2. **Rewrite** in memory: strip published ports (optional escape hatch via `.conduit.yml` `expose:`), attach a per-project Docker network, inject `conduit.*` and `traefik.*` **labels**.
3. **Emit** a generated file: `.conduit/cache/docker-compose.conduit.yml` — this is what `docker compose` actually runs (`-f` + `-p <sanitized-project>`).
4. **Proxy** — one shared **Traefik** container (`conduit-proxy`) with the **Docker provider** (reads labels; bind-mounts `/var/run/docker.sock` + static config under `~/.local/share/conduit/proxy/traefik.yml`).
5. **DNS** — merges route hostnames into `/etc/hosts` (sudo may be required). Safe with multiple projects: host block is rebuilt from state.

Use **`conduit up --no-proxy`** to skip Traefik/network rewrite and only add `conduit.*` labels (keeps your published ports).

## Install

```bash
cargo build --release
cp target/release/conduit ~/.local/bin/   # or cargo install --path .
```

## Quick start

```bash
conduit init          # optional: scaffold .conduit.yml from compose
conduit up
conduit ps
conduit logs <service> --follow
conduit db postgres   # after up — TCP tunnel to DB
conduit down
```

## Commands

| Command | Description |
|---------|-------------|
| `conduit up` | Emit generated compose, create network, start Traefik if needed, `compose up` |
| `conduit down` | `compose down` with same `-f`/`-p`, cleanup network, sync `/etc/hosts` |
| `conduit ps` | Projects + services from state / Docker |
| `conduit logs` | Uses generated compose + project name when present |
| `conduit db <svc>` | Ephemeral localhost → container TCP forward |
| `conduit doctor` | Docker, compose CLI, ports, hosts, proxy |

## Configuration (`.conduit.yml`)

See inline docs in `src/config/conduit_yml.rs` — `project`, `compose_file`, `domain`, `routes`, `groups`, `expose`, `databases`.

### Example (Deepiri-style monorepo)

```yaml
project: deepiri
compose_file: docker-compose.dev.yml
domain: deepiri.localhost

routes:
  frontend-dev:
    domain: frontend.deepiri.localhost
    websocket: true
  api-gateway:
    domain: api.deepiri.localhost

groups:
  infra:
    services: [postgres, redis]
  core:
    depends_on: [infra]
    services: [api-gateway, auth-service]

databases:
  postgres:
    type: postgresql
    user_env: POSTGRES_USER
    password_env: POSTGRES_PASSWORD
    database_env: POSTGRES_DB
```

## Troubleshooting

| Symptom | What to check |
|--------|----------------|
| `Docker Engine: not available` | Start Docker Desktop / `dockerd`. On WSL2, enable Docker Desktop **WSL integration** for your distro. |
| `docker compose` missing | Install Docker Compose v2 plugin (`docker compose version`). |
| Port 80 in use | Traefik needs **80** for HTTP routing. Stop nginx/apache or change `proxy.http_port` in `~/.config/conduit/config.toml` (advanced). |
| Routes don’t resolve | Run `conduit doctor`. Hosts sync may need **sudo** for `/etc/hosts`; on WSL2 you may also sync Windows hosts (see `dns/hosts.rs`). |
| `conduit up` fails on compose | Run `docker compose -f <your-file> config` in the same directory to see compose errors. |
| Stale state | `conduit down` then remove `.conduit/cache/` if needed; state lives under `~/.local/share/conduit/state.json`. |

Run **`conduit doctor`** before reporting issues.

## Requirements

- Docker Engine + **`docker compose`** CLI (for `compose config` / `up` / `down`).
- Traefik image pull on first proxy start (e.g. `traefik:v3.3` — configurable in `~/.config/conduit/config.toml`).

## License

Apache License 2.0 — Copyright 2026 Deepiri. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

## Roadmap

Product milestones and priorities: [**ROADMAP.md**](ROADMAP.md). Full technical design and historical plan: [**PLAN.md**](PLAN.md).

## CI & releases

- **CI** — `fmt`, `clippy`, tests, release build on push/PR ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)).
- **Releases** — push a tag `v*` → Linux x86_64 binary + checksum attached to a GitHub Release ([`.github/workflows/release.yml`](.github/workflows/release.yml)).
