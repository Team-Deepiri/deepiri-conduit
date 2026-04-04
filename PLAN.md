# deepiri-conduit

**Local development orchestrator for multi-service Docker Compose projects.**

Single binary. Zero port conflicts. Automatic HTTP routing. On-demand database access.

```
┌──────────────────────────────────────────────────────────┐
│  Before Conduit          │  After Conduit                │
│                          │                               │
│  localhost:5173  frontend│  frontend.deepiri.local       │
│  localhost:5100  gateway │  api.deepiri.local            │
│  localhost:8000  cyrex   │  cyrex.deepiri.local          │
│  localhost:5432  postgres│  conduit db deepiri postgres   │
│  localhost:8002  synapse │  synapse.deepiri.local        │
│  ... 25 more ports ...   │  ... zero port conflicts ...  │
│                          │                               │
│  Can't run two projects  │  conduit up project-a         │
│  at the same time        │  conduit up project-b         │
│                          │  (both run, zero conflicts)   │
└──────────────────────────────────────────────────────────┘
```

---

## Table of Contents

1. [Repo Decision](#1-repo-decision)
2. [The Problem](#2-the-problem)
3. [System Architecture](#3-system-architecture)
4. [CLI Reference](#4-cli-reference)
5. [Configuration](#5-configuration)
6. [Technical Deep Dive](#6-technical-deep-dive)
7. [Code Sketches](#7-code-sketches)
8. [Error Handling](#8-error-handling)
9. [Testing Strategy](#9-testing-strategy)
10. [Security](#10-security)
11. [Platform-Specific Notes](#11-platform-specific-notes)
12. [Comparison with Alternatives](#12-comparison-with-alternatives)
13. [Installation & Distribution](#13-installation--distribution)
14. [CI/CD Pipeline](#14-cicd-pipeline)
15. [Phased Roadmap](#15-phased-roadmap)
16. [v2+ Future Roadmap](#16-v2-future-roadmap)
17. [What Conduit is NOT](#17-what-conduit-is-not)
18. [Open Questions](#18-open-questions)
19. [Glossary](#19-glossary)

---

## 1. Repo Decision

### Standalone — `github.com/Team-Deepiri/deepiri-conduit`

Not a submodule of `deepiri-platform`. Not a workspace package. Its own repo.

**Why:**

| Factor | Submodule | Standalone |
|--------|-----------|------------|
| Installable via `cargo install` | No (submodules aren't crates) | Yes |
| Own release cycle | Tied to platform releases | Independent |
| Usable by non-Deepiri projects | No | Yes |
| CI/CD complexity | Inherits platform's CI | Own lightweight CI |
| Code dependency on platform | Tempting to couple | Physically impossible |
| Open-source story | Awkward (buried in monorepo) | Clean (own repo, own README) |

**How Conduit knows about Deepiri:**

Conduit is generic. It reads Docker Compose files. Deepiri-specific configuration lives in a `.conduit.yml` file committed to `deepiri-platform`. When a developer runs `conduit up` inside the platform directory, Conduit finds that config and uses it for domain names, service groups, and routing rules. Conduit's source code has zero references to "Deepiri."

**Repo structure in the org:**

```
github.com/Team-Deepiri/
├── deepiri-platform         (has .conduit.yml committed)
├── deepiri-conduit          (this tool)
├── deepiri-core-api
├── diri-cyrex
├── diri-helox
├── ... other repos ...
└── homebrew-tap             (Homebrew formula for conduit)
```

---

## 2. The Problem

### Deepiri's port allocation today

`docker-compose.dev.yml` binds **30+ host ports** across 26 services:

```
PORT   SERVICE                     PORT   SERVICE
────   ───────                     ────   ───────
5432   postgres                    5100   api-gateway
5050   pgadmin                     5001   auth-service
8080   adminer                     5002   task-orchestrator
6380   redis                       5003   engagement-service
9092   kafka                       5004   platform-analytics
8086   influxdb                    5005   notification-service
19530  milvus                      5006   external-bridge-service
9091   milvus-health               5007   challenge-service
9000   minio-api                   5008   realtime-gateway
9001   minio-console               5009   language-intelligence
11435  ollama                      5010   messaging-service
8000   cyrex                       5011   prismpipe
5175   cyrex-interface             5500   mlflow
8010   persola                     8002   synapse
3000   persola-ui                  5173   frontend-dev
```

### What goes wrong

1. **Port collisions.** Run another project with Postgres on 5432 — instant conflict. Even running `docker-compose.yml` alongside `docker-compose.dev.yml` in the same repo collides (both bind 5432).

2. **Port number memorization.** Which port is the API gateway? 5100? 5000? Depends which compose file. Developers waste time looking this up.

3. **No simultaneous projects.** A developer working on Deepiri and a side project can't have both running. Manual port remapping is tedious and error-prone.

4. **Database exposure.** Postgres is always listening on the host. Even in dev, this is unnecessary — services talk to it over the Docker network. Host binding exists only for `psql` or pgAdmin access, which is needed maybe 10% of the time.

5. **`docker ps` is unreadable.** 26 containers with no grouping. Figuring out "is everything healthy?" requires scrolling through a wall of text.

### What Conduit fixes

- **Zero host ports by default.** Services talk over Docker networks. Nothing binds to the host unless explicitly configured.
- **Named domains.** `frontend.deepiri.local` instead of `localhost:5173`.
- **On-demand database access.** `conduit db deepiri postgres` opens a tunnel when you need it, closes when you're done.
- **Multi-project.** Two projects run simultaneously with zero configuration changes.
- **Grouped services.** `conduit up --group core` starts only infrastructure + core services.

---

## 3. System Architecture

### High-level overview

```
                         Developer's machine
┌────────────────────────────────────────────────────────────┐
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │                    conduit CLI                        │  │
│  │            (single Rust binary, ~5-10MB)              │  │
│  └──────┬────────────┬───────────────┬──────────────────┘  │
│         │            │               │                     │
│         ▼            │               ▼                     │
│  ┌─────────────┐     │     ┌──────────────────────────┐    │
│  │ /etc/hosts  │     │     │    State file             │    │
│  │ DNS entries │     │     │    ~/.local/share/conduit │    │
│  └─────────────┘     │     └──────────────────────────┘    │
│                      │                                     │
│         ┌────────────┘                                     │
│         ▼                                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Docker Engine (unix socket)              │   │
│  └──────┬──────────────┬───────────────┬───────────────┘   │
│         │              │               │                   │
│         ▼              ▼               ▼                   │
│  ┌────────────┐ ┌────────────┐ ┌────────────────────────┐  │
│  │  conduit-  │ │  Project A │ │  Project B             │  │
│  │  proxy     │ │  containers│ │  containers            │  │
│  │ (Traefik)  │ │            │ │                        │  │
│  │            │ │ ┌────────┐ │ │ ┌────────┐             │  │
│  │  :80/:443  │ │ │postgres│ │ │ │postgres│             │  │
│  │  ──────►   │ │ │ :5432  │ │ │ │ :5432  │             │  │
│  │  HTTP only │ │ └────────┘ │ │ └────────┘             │  │
│  │            │ │ ┌────────┐ │ │ ┌────────┐             │  │
│  │ routes to  │ │ │frontend│ │ │ │web-app │             │  │
│  │ containers │ │ │ :5173  │ │ │ │ :3000  │             │  │
│  │ by domain  │ │ └────────┘ │ │ └────────┘             │  │
│  │            │ │    ...     │ │    ...                  │  │
│  └──────┬─────┘ └─────┬──────┘ └──────┬────────────────┘  │
│         │             │                │                   │
│         ▼             ▼                ▼                   │
│  ┌────────────────────────────────────────────────────┐    │
│  │           Docker Networks (isolated)                │    │
│  │                                                     │    │
│  │  conduit-proxy-net ◄──── shared, proxy connects     │    │
│  │  conduit-project-a ◄──── project A only             │    │
│  │  conduit-project-b ◄──── project B only             │    │
│  └─────────────────────────────────────────────────────┘   │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  TCP Tunnels (on-demand, Rust tokio)                  │  │
│  │                                                       │  │
│  │  localhost:54329 ──► conduit-project-a ──► postgres    │  │
│  │  localhost:54330 ──► conduit-project-b ──► postgres    │  │
│  │  localhost:27018 ──► conduit-project-a ──► mongodb     │  │
│  │                                                       │  │
│  │  (only open while `conduit db` is running)            │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

### Component responsibilities

| Component | What it does | Implementation |
|-----------|-------------|----------------|
| **CLI** | Parse commands, orchestrate everything | `clap` derive macros |
| **Compose Parser** | Read docker-compose YAML, resolve variables | `serde_yaml` + custom interpolation |
| **Compose Rewriter** | Strip ports, inject Traefik labels, swap networks | In-memory YAML transform |
| **Docker Client** | Create/start/stop containers, manage networks | `bollard` (Docker Engine API) |
| **Proxy Manager** | Ensure Traefik container runs, configure routes | Traefik API + file provider |
| **TCP Tunnel** | Bidirectional TCP forwarding for DB access | `tokio::net` |
| **DNS Manager** | Write /etc/hosts entries for *.local domains | File I/O + sudo escalation |
| **State Registry** | Track running projects, routes, tunnels | JSON file in `~/.local/share/conduit/` |
| **Config Loader** | Parse `.conduit.yml` + global config | `serde_yaml` + `toml` |

### Network topology (detailed)

```
Before conduit up:
  Host network: ports 5432, 5173, 8000, 5100, ... all bound
  Docker:       one flat network (deepiri-dev-network)

After conduit up:
  Host network: only ports 80, 443 bound (Traefik proxy)
  Docker:
    conduit-proxy-net:     Traefik connects here (always)
    conduit-deepiri-dev:   All Deepiri services + Traefik
    conduit-other-proj:    Other project services + Traefik

  Traefik is the ONLY container connected to multiple networks.
  Project containers only see their own network.
```

### Data flow: HTTP request

```
Browser: GET http://frontend.deepiri.local/
    │
    ▼
/etc/hosts: frontend.deepiri.local → 127.0.0.1
    │
    ▼
Traefik (port 80): match Host(`frontend.deepiri.local`)
    │
    ▼
Route to container deepiri-frontend-dev:5173 on network conduit-deepiri-dev
    │
    ▼
Vite dev server responds
```

### Data flow: TCP tunnel (database)

```
Developer runs: conduit db deepiri postgres
    │
    ▼
Conduit finds container deepiri-postgres-dev
Conduit finds free port 54329
Conduit starts tokio TCP listener on 0.0.0.0:54329
    │
    ▼
Developer runs: psql -h localhost -p 54329 -U deepiri
    │
    ▼
TCP connection accepted by Conduit
Conduit opens connection to deepiri-postgres-dev:5432 via Docker network
Bidirectional byte copying (tokio::io::copy_bidirectional)
    │
    ▼
Postgres responds through the tunnel
    │
    ▼
Developer presses Ctrl+C → tunnel closes, port freed
```

---

## 4. CLI Reference

### `conduit up`

Start a project. Parses compose, rewrites, starts containers, configures proxy.

```
conduit up [OPTIONS]

OPTIONS:
  --file, -f <path>         Compose file (default: auto-detect)
  --group, -g <name>        Start only this group + its dependencies
  --profile <name>          Docker Compose profile to activate
  --no-proxy                Skip proxy setup (expose raw ports instead)
  --build                   Force rebuild images before starting
  --detach, -d              Don't follow logs after startup (default: detach)
  --timeout <seconds>       Startup timeout per service (default: 120)

EXAMPLES:
  conduit up                             # Start all services in current dir
  conduit up --group core                # Start infra + core only
  conduit up --file docker-compose.yml   # Use specific compose file
  conduit up --no-proxy                  # Skip proxy, use original ports
```

### `conduit down`

Stop a project. Removes containers, cleans network, removes DNS entries.

```
conduit down [OPTIONS]

OPTIONS:
  --volumes, -v             Also remove named volumes
  --all                     Stop ALL conduit-managed projects
  --timeout <seconds>       Graceful shutdown timeout (default: 10)

EXAMPLES:
  conduit down              # Stop project in current directory
  conduit down --volumes    # Stop and wipe data volumes
  conduit down --all        # Stop everything conduit manages
```

### `conduit ps`

List running projects and their services.

```
conduit ps [OPTIONS]

OPTIONS:
  --all, -a                 Show stopped services too
  --json                    JSON output
  --wide, -w                Show ports, networks, health details

EXAMPLES:
  conduit ps                # Overview of all projects
  conduit ps --wide         # Detailed view with ports and health
  conduit ps --json         # Machine-readable output
```

Output:

```
PROJECT    SERVICES   HEALTHY   NETWORK               UPTIME
deepiri    26/26      24/26     conduit-deepiri-dev    2h 15m
sideproj   5/5        5/5       conduit-sideproj       45m

PROXY: running (traefik:v3.3) on :80/:443
TUNNELS: 1 active (deepiri/postgres → localhost:54329)
```

Wide output:

```
PROJECT: deepiri (docker-compose.dev.yml)
NETWORK: conduit-deepiri-dev
UPTIME:  2h 15m

SERVICE                          STATUS    HEALTH     DOMAIN
────────────────────────────────────────────────────────────────
postgres                         running   healthy    —
redis                            running   healthy    —
kafka                            running   healthy    —
influxdb                         running   healthy    —
milvus                           running   healthy    —
minio                            running   healthy    minio.deepiri.local
ollama                           running   healthy    —
synapse                          running   healthy    synapse.deepiri.local
api-gateway                      running   healthy    api.deepiri.local
auth-service                     running   healthy    —
task-orchestrator                running   healthy    —
engagement-service               running   healthy    —
platform-analytics-service       running   unhealthy  —
notification-service             running   healthy    —
external-bridge-service          running   healthy    —
challenge-service                running   healthy    —
language-intelligence-service    running   starting   —
messaging-service                running   healthy    —
realtime-gateway                 running   healthy    —
cyrex                            running   healthy    cyrex.deepiri.local
cyrex-interface                  running   healthy    cyrex-ui.deepiri.local
persola                          running   healthy    persola.deepiri.local
persola-ui                       running   healthy    —
mlflow                           running   healthy    mlflow.deepiri.local
frontend-dev                     running   healthy    frontend.deepiri.local
deepiri-prismpipe                running   healthy    —
```

### `conduit db`

Open a temporary TCP tunnel to a database service.

```
conduit db <service> [OPTIONS]

OPTIONS:
  --port, -p <port>         Use specific host port (default: auto-assign)
  --project <name>          Target project (default: current directory)
  --print-only              Print connection string without opening tunnel
  --background, -b          Run tunnel in background (returns PID)

EXAMPLES:
  conduit db postgres                    # Tunnel to postgres, auto port
  conduit db postgres -p 5432            # Tunnel on specific port
  conduit db redis                       # Tunnel to redis
  conduit db mongo --project sideproj    # Tunnel to other project's mongo
```

Output:

```
  ✓ Detected: PostgreSQL (postgres:16-alpine)
  ✓ Tunnel: localhost:54329 → deepiri-postgres-dev:5432
  ✓ Credentials extracted from compose environment

  ┌─────────────────────────────────────────────────────────┐
  │  Connection String:                                     │
  │  postgresql://deepiri:deepiripassword@localhost:54329/deepiri  │
  │                                                         │
  │  psql:                                                  │
  │  psql -h localhost -p 54329 -U deepiri -d deepiri       │
  │                                                         │
  │  GUI (DBeaver, pgAdmin, DataGrip):                      │
  │  Host: localhost  Port: 54329  User: deepiri            │
  └─────────────────────────────────────────────────────────┘

  Tunnel active. Press Ctrl+C to close.
  Connections: 0 active
```

### `conduit logs`

Tail logs from one or more services, color-coded.

```
conduit logs [service...] [OPTIONS]

OPTIONS:
  --follow, -f              Follow log output (default: true)
  --tail <n>                Number of lines from end (default: 50)
  --since <duration>        Show logs since duration (e.g., 5m, 1h)
  --timestamps, -t          Show timestamps
  --group <name>            Show logs for a service group
  --no-color                Disable color output

EXAMPLES:
  conduit logs                           # All services
  conduit logs api-gateway auth-service  # Specific services
  conduit logs --group core              # Core group only
  conduit logs cyrex --since 5m          # Cyrex logs from last 5 minutes
```

### `conduit route`

Display the full routing table.

```
conduit route [OPTIONS]

OPTIONS:
  --json                    JSON output
  --project <name>          Filter by project

EXAMPLES:
  conduit route             # Show all routes
  conduit route --json      # Machine-readable
```

Output:

```
DOMAIN                          TARGET                              TLS
───────────────────────────────────────────────────────────────────────
frontend.deepiri.local          deepiri-frontend-dev:5173           ✓
api.deepiri.local               deepiri-api-gateway-dev:5000        ✓
cyrex.deepiri.local             deepiri-cyrex-dev:8000              ✓
cyrex-ui.deepiri.local          deepiri-cyrex-interface-dev:5175    ✓
persola.deepiri.local           deepiri-persola-dev:8002            ✓
synapse.deepiri.local           deepiri-synapse-dev:8002            ✓
mlflow.deepiri.local            deepiri-mlflow-dev:5000             ✓
minio.deepiri.local             deepiri-minio-dev:9001              ✓
pgadmin.deepiri.local           deepiri-pgadmin-dev:80              ✓

ACTIVE TUNNELS:
localhost:54329                 deepiri-postgres-dev:5432           TCP
```

### `conduit link` / `conduit unlink`

Connect or disconnect two project networks so their services can communicate.

```
conduit link <project-a> <project-b>
conduit unlink <project-a> <project-b>

EXAMPLES:
  conduit link deepiri sideproj     # sideproj services can reach deepiri services
  conduit unlink deepiri sideproj   # Disconnect them
```

### `conduit doctor`

Check system requirements and diagnose issues.

```
conduit doctor

OUTPUT:
  ✓ Docker Engine: v27.5.1 (minimum: v20.10)
  ✓ Docker Compose: v2.32.4 (used for config normalization)
  ✓ Port 80: available
  ✓ Port 443: available
  ✗ /etc/hosts: conduit entries stale (run `conduit up` to refresh)
  ✓ Conduit proxy: running (traefik:v3.3)
  ✓ State file: valid (2 projects tracked)
  ✓ Disk space: 45GB free
  ✓ Docker socket: /var/run/docker.sock accessible

  WSL2 detected:
  ✓ Docker Desktop integration: connected
  ✓ Windows hosts file: synced (conduit manages via wsl.exe)
```

### `conduit init`

Generate a `.conduit.yml` from an existing compose file.

```
conduit init [OPTIONS]

OPTIONS:
  --file, -f <path>         Source compose file
  --domain <domain>         Base domain (default: <project>.local)

EXAMPLES:
  conduit init                           # Auto-detect compose, generate config
  conduit init --domain myapp.local      # Custom domain
```

### `conduit config`

Show resolved configuration.

```
conduit config [OPTIONS]

OPTIONS:
  --global                  Show global config (~/.config/conduit/config.toml)
  --project                 Show project config (.conduit.yml)
  --merged                  Show merged effective config (default)
```

---

## 5. Configuration

### Project config: `.conduit.yml`

Lives in the project root. Optional — Conduit works without it.

```yaml
# .conduit.yml
project: deepiri
compose_file: docker-compose.dev.yml

# Base domain for auto-generated routes
domain: deepiri.local

# Explicit route overrides (service → domain)
# Services not listed here get auto-generated domains:
# <service-name>.<domain> (e.g., auth-service.deepiri.local)
routes:
  frontend-dev:
    domain: frontend.deepiri.local
    # WebSocket support (needed for Vite HMR)
    websocket: true
  api-gateway:
    domain: api.deepiri.local
  cyrex:
    domain: cyrex.deepiri.local
  cyrex-interface:
    domain: cyrex-ui.deepiri.local
    websocket: true
  persola:
    domain: persola.deepiri.local
  synapse:
    domain: synapse.deepiri.local
    websocket: true
  mlflow:
    domain: mlflow.deepiri.local
  minio:
    # Route to the console port, not the API port
    domain: minio.deepiri.local
    port: 9001
  pgadmin:
    domain: pgadmin.deepiri.local

# Service groups for selective startup
# Dependencies are resolved automatically:
#   --group core starts infra (because core depends on it) + core
#   --group ai starts infra + core + ai
groups:
  infra:
    services:
      - postgres
      - redis
      - kafka
      - influxdb
      - etcd
      - minio
      - milvus
      - ollama
  core:
    depends_on: [infra]
    services:
      - synapse
      - api-gateway
      - auth-service
      - task-orchestrator
      - realtime-gateway
      - notification-service
      - engagement-service
      - platform-analytics-service
      - challenge-service
      - external-bridge-service
      - messaging-service
      - prismpipe
  ai:
    depends_on: [core]
    services:
      - cyrex
      - persola
      - mlflow
      - cyrex-interface
  frontend:
    depends_on: [core]
    services:
      - frontend-dev
      - persola-ui

# Services that keep host port bindings (escape hatch)
# Use sparingly — defeats the purpose of conduit
expose:
  ollama: 11435    # External tools (Cursor, CLI) need direct access

# Environment variable overrides applied during conduit up
# These override values in the compose file
env:
  NODE_ENV: development
  LOG_LEVEL: debug

# Health check timeouts (override compose healthcheck start_period)
health:
  timeout: 180    # Max seconds to wait for all services to be healthy
  interval: 5     # Check interval during startup

# Database connection hints (for `conduit db` credential extraction)
databases:
  postgres:
    type: postgresql
    user_env: POSTGRES_USER
    password_env: POSTGRES_PASSWORD
    database_env: POSTGRES_DB
  redis:
    type: redis
    password_env: REDIS_PASSWORD
  mongodb:
    type: mongodb
    user_env: MONGO_ROOT_USER
    password_env: MONGO_ROOT_PASSWORD
```

### Global config: `~/.config/conduit/config.toml`

```toml
# Global Conduit configuration

[proxy]
# Traefik image to use
image = "traefik:v3.3"
# Host ports for proxy (change if 80/443 conflict with something)
http_port = 80
https_port = 443
# Dashboard (for debugging proxy issues)
dashboard = false
dashboard_port = 8080

[dns]
# How to manage DNS resolution for *.local domains
# Options: "hosts" (edit /etc/hosts), "dnsmasq", "systemd-resolved", "none"
strategy = "hosts"
# If using hosts strategy, ask for sudo (true) or skip if no permission (false)
sudo = true

[tunnels]
# Port ranges for auto-assigned tunnel ports
# Each database type gets its own range for predictability
postgres_range = [54320, 54399]
mongodb_range = [27020, 27099]
redis_range = [63800, 63899]
mysql_range = [33060, 33099]
default_range = [49200, 49299]

[logging]
# Log level: error, warn, info, debug, trace
level = "info"
# Log to file (useful for debugging)
file = ""    # Empty = no file logging

[state]
# Where conduit stores its state (running projects, routes, etc.)
dir = "~/.local/share/conduit"
```

### State file: `~/.local/share/conduit/state.json`

Conduit tracks all running projects in a JSON state file:

```json
{
  "version": 1,
  "proxy": {
    "container_id": "abc123def456",
    "image": "traefik:v3.3",
    "status": "running",
    "ports": { "http": 80, "https": 443 },
    "started_at": "2026-04-03T14:22:00Z"
  },
  "projects": {
    "deepiri": {
      "directory": "/home/josep/projects/Deepiri/deepiri-platform",
      "compose_file": "docker-compose.dev.yml",
      "config_file": ".conduit.yml",
      "network": "conduit-deepiri-dev",
      "started_at": "2026-04-03T14:22:05Z",
      "services": {
        "postgres": {
          "container_id": "def789...",
          "container_name": "deepiri-postgres-dev",
          "status": "running",
          "health": "healthy",
          "internal_ports": [5432],
          "image": "postgres:16-alpine"
        },
        "api-gateway": {
          "container_id": "ghi012...",
          "container_name": "deepiri-api-gateway-dev",
          "status": "running",
          "health": "healthy",
          "internal_ports": [5000],
          "image": "deepiri-dev-api-gateway:latest",
          "domain": "api.deepiri.local"
        }
      },
      "routes": {
        "frontend.deepiri.local": "deepiri-frontend-dev:5173",
        "api.deepiri.local": "deepiri-api-gateway-dev:5000"
      }
    }
  },
  "tunnels": {
    "deepiri/postgres": {
      "host_port": 54329,
      "container": "deepiri-postgres-dev",
      "container_port": 5432,
      "pid": 12345,
      "opened_at": "2026-04-03T15:00:00Z"
    }
  },
  "hosts_entries": [
    "127.0.0.1 frontend.deepiri.local",
    "127.0.0.1 api.deepiri.local",
    "127.0.0.1 cyrex.deepiri.local"
  ]
}
```

---

## 6. Technical Deep Dive

### 6.1 Compose parsing strategy

Docker Compose files are complex: env interpolation (`${VAR:-default}`), `extends`, `profiles`, `include`, anchors/aliases. Reimplementing all of this is a multi-month effort and a moving target.

**Strategy: hybrid approach.**

1. **First pass:** Shell out to `docker compose config` to get the fully resolved, normalized YAML. This handles all interpolation, extends, profiles, includes.
2. **Parse the normalized output** with `serde_yaml` into our typed structs.
3. **Fallback:** If `docker compose` isn't installed, parse raw YAML and handle basic `${VAR:-default}` interpolation ourselves. Accept that advanced features won't work without the CLI.

```
Input:  docker-compose.dev.yml (with anchors, env vars, extends)
          │
          ▼
        docker compose -f docker-compose.dev.yml config
          │
          ▼
        Normalized YAML (all resolved)
          │
          ▼
        serde_yaml::from_str::<ComposeFile>()
          │
          ▼
        Typed ComposeFile struct
```

### 6.2 Compose rewriting

The rewriter transforms the parsed compose in-memory before passing to Docker:

| What | Before | After |
|------|--------|-------|
| `ports` | `- "5432:5432"` | *removed* (unless in `expose:` config) |
| `networks` | `deepiri-dev-network` | `conduit-deepiri-dev` |
| `labels` | *(none)* | Traefik routing labels + conduit metadata |
| `container_name` | `deepiri-postgres-dev` | *preserved* (for stable references) |

Labels injected on routable services:

```yaml
labels:
  conduit.managed: "true"
  conduit.project: "deepiri"
  conduit.service: "api-gateway"
  traefik.enable: "true"
  traefik.http.routers.deepiri-api-gateway.rule: "Host(`api.deepiri.local`)"
  traefik.http.routers.deepiri-api-gateway.tls: "true"
  traefik.http.services.deepiri-api-gateway.loadbalancer.server.port: "5000"
```

For WebSocket services (frontend-dev, synapse):

```yaml
labels:
  traefik.http.middlewares.deepiri-frontend-ws.headers.customrequestheaders.Connection: "keep-alive, Upgrade"
  traefik.http.middlewares.deepiri-frontend-ws.headers.customrequestheaders.Upgrade: "websocket"
```

### 6.3 Docker Engine API via bollard

Conduit uses `bollard` to talk directly to Docker's Unix socket (`/var/run/docker.sock`). This avoids:
- Requiring `docker compose` CLI to be installed
- Parsing CLI text output
- Shell escaping issues

Key operations:

| Operation | bollard API |
|-----------|------------|
| Create network | `docker.create_network(CreateNetworkOptions { ... })` |
| Create container | `docker.create_container(Some(opts), config)` |
| Start container | `docker.start_container(id, None)` |
| Stop container | `docker.stop_container(id, Some(StopContainerOptions { t: 10 }))` |
| Remove container | `docker.remove_container(id, Some(RemoveContainerOptions { force: true, .. }))` |
| Inspect container | `docker.inspect_container(id, None)` |
| List containers | `docker.list_containers(Some(ListContainersOptions { filters, .. }))` |
| Connect to network | `docker.connect_network(net_id, ConnectNetworkOptions { container: id, .. })` |
| Container logs | `docker.logs(id, Some(LogsOptions { follow: true, stdout: true, stderr: true, .. }))` |
| Build image | `docker.build_image(BuildImageOptions { .. }, None, Some(tar_body))` |
| Pull image | `docker.create_image(Some(CreateImageOptions { from_image: "traefik", tag: "v3.3" }), None, None)` |

**Exception:** For `conduit up --build`, we may shell out to `docker compose build` since bollard's build API doesn't handle multi-stage builds with the same ease as the compose CLI's build context handling.

### 6.4 Proxy management (Traefik)

Conduit manages a single Traefik container (`conduit-proxy`) that persists across project lifecycles.

**Traefik configuration method:** File provider (not Docker provider).

Why not Docker provider (where Traefik auto-discovers containers by labels)?
- Docker provider requires Traefik to have access to the Docker socket (security concern).
- Docker provider scans ALL containers, not just conduit-managed ones.
- File provider gives us explicit control over routing.

**How it works:**

1. Conduit creates a Docker volume `conduit-proxy-config`.
2. Traefik mounts this volume at `/etc/traefik/dynamic/`.
3. When `conduit up` runs, Conduit writes a file per project:

```yaml
# /etc/traefik/dynamic/deepiri.yml
http:
  routers:
    deepiri-frontend:
      rule: "Host(`frontend.deepiri.local`)"
      service: deepiri-frontend
      tls: {}
    deepiri-api:
      rule: "Host(`api.deepiri.local`)"
      service: deepiri-api
      tls: {}
  services:
    deepiri-frontend:
      loadBalancer:
        servers:
          - url: "http://deepiri-frontend-dev:5173"
    deepiri-api:
      loadBalancer:
        servers:
          - url: "http://deepiri-api-gateway-dev:5000"
```

4. Traefik watches the directory and hot-reloads on file changes. No restart needed.
5. On `conduit down`, Conduit deletes the project's config file from the volume.

**Traefik static config** (baked into container creation):

```yaml
entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
  websecure:
    address: ":443"
providers:
  file:
    directory: /etc/traefik/dynamic/
    watch: true
tls:
  certificates:
    - certFile: /etc/traefik/certs/conduit.crt
      keyFile: /etc/traefik/certs/conduit.key
api:
  dashboard: false
log:
  level: WARN
```

### 6.5 TCP tunnel implementation

The TCP tunnel is a pure Rust async proxy. No external binaries (no socat, no ssh).

**Architecture:**

```
Host:
  TcpListener bound to 0.0.0.0:<free_port>
    │
    │  accept()
    ▼
  For each connection:
    ┌────────────────────────────────────────┐
    │  tokio::spawn async task               │
    │                                        │
    │  1. Connect to Docker container via    │
    │     container IP on Docker network     │
    │     (looked up via bollard inspect)    │
    │                                        │
    │  2. tokio::io::copy_bidirectional      │
    │     (client_stream, container_stream)  │
    │                                        │
    │  3. On either side close → task ends   │
    └────────────────────────────────────────┘
```

**Container IP resolution:**

Bollard's `inspect_container` returns the container's IP on each attached network. Conduit uses the IP on the conduit-managed network to connect.

**Connection tracking:**

An `AtomicUsize` counter tracks active connections. Displayed in the tunnel output:

```
Tunnel active. Press Ctrl+C to close.
Connections: 3 active, 47 total
```

**Graceful shutdown:**

1. Ctrl+C signal caught via `tokio::signal::ctrl_c()`
2. Stop accepting new connections (drop TcpListener)
3. Wait up to 5 seconds for active connections to drain
4. Force-close remaining connections
5. Exit

### 6.6 DNS management

**Strategy 1: /etc/hosts (default)**

Conduit adds entries to `/etc/hosts` with markers:

```
# >>> CONDUIT START (do not edit) <<<
127.0.0.1 frontend.deepiri.local
127.0.0.1 api.deepiri.local
127.0.0.1 cyrex.deepiri.local
127.0.0.1 synapse.deepiri.local
# ... more ...
# >>> CONDUIT END <<<
```

Requires sudo on Linux/macOS. Conduit prompts once and caches the escalation.

On `conduit down`, entries between the markers are removed.

**Strategy 2: systemd-resolved (Linux)**

For systems using systemd-resolved (Ubuntu 18.04+):

```bash
resolvectl domain conduit-proxy-net ~local
resolvectl dns conduit-proxy-net 127.0.0.1
```

No sudo needed after initial setup. All `*.local` queries resolve to 127.0.0.1.

**Strategy 3: dnsmasq**

A dnsmasq container managed by Conduit:

```
address=/.local/127.0.0.1
```

All `*.local` resolves to localhost. No /etc/hosts editing.

**Phase 1 ships /etc/hosts.** Others added based on user feedback.

### 6.7 Startup ordering

Docker Compose `depends_on` with `condition: service_healthy` is the gold standard. Conduit respects this:

1. Parse the dependency graph from `depends_on` fields.
2. Topological sort to determine startup order.
3. Start services in waves (parallel within each wave).
4. Wait for health checks to pass before starting dependents.
5. If a service has no healthcheck, wait for `running` state + a brief delay.

```
Wave 1 (parallel): postgres, redis, kafka, influxdb, etcd, minio, ollama
    │ wait for healthy
Wave 2 (parallel): milvus (depends: etcd, minio), synapse (depends: redis)
    │ wait for healthy
Wave 3 (parallel): auth-service, mlflow (depend on postgres)
    │ wait for healthy
Wave 4 (parallel): all remaining services
    │ wait for healthy
Wave 5: frontend-dev (depends on synapse)
```

Progress display during startup:

```
  Starting infrastructure...
  [████████████████████░░░░] 7/9  postgres ✓  redis ✓  kafka ✓  milvus ⏳  ollama ⏳

  Starting core services...
  [████████░░░░░░░░░░░░░░░░] 3/12  synapse ✓  auth-service ✓  api-gateway ⏳

  Starting AI services...
  [░░░░░░░░░░░░░░░░░░░░░░░░] 0/4  waiting for core...
```

---

## 7. Code Sketches

### 7.1 Main entry point

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "conduit", version, about = "Local dev orchestrator for Docker Compose")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(long, global = true)]
    json: bool,

    #[arg(long, global = true)]
    project_dir: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    Up(cli::up::UpArgs),
    Down(cli::down::DownArgs),
    Ps(cli::ps::PsArgs),
    Logs(cli::logs::LogsArgs),
    Db(cli::db::DbArgs),
    Route(cli::route::RouteArgs),
    Link(cli::link::LinkArgs),
    Unlink(cli::link::UnlinkArgs),
    Doctor,
    Init(cli::init::InitArgs),
    Config(cli::config::ConfigArgs),
    Proxy(cli::proxy::ProxyArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(if cli.verbose { "debug" } else { "info" })
        .init();

    match cli.command {
        Command::Up(args) => cli::up::run(args, &cli).await,
        Command::Down(args) => cli::down::run(args, &cli).await,
        Command::Ps(args) => cli::ps::run(args, &cli).await,
        Command::Db(args) => cli::db::run(args, &cli).await,
        Command::Doctor => cli::doctor::run(&cli).await,
        // ...
    }
}
```

### 7.2 Compose types

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct ComposeFile {
    pub name: Option<String>,
    pub version: Option<String>,
    pub services: BTreeMap<String, Service>,
    pub volumes: Option<BTreeMap<String, Option<VolumeConfig>>>,
    pub networks: Option<BTreeMap<String, Option<NetworkConfig>>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Service {
    pub image: Option<String>,
    pub build: Option<BuildConfig>,
    pub container_name: Option<String>,
    pub ports: Option<Vec<PortMapping>>,
    pub environment: Option<Environment>,
    pub env_file: Option<EnvFile>,
    pub volumes: Option<Vec<String>>,
    pub networks: Option<Vec<String>>,
    pub depends_on: Option<DependsOn>,
    pub healthcheck: Option<HealthCheck>,
    pub labels: Option<BTreeMap<String, String>>,
    pub command: Option<CommandVariant>,
    pub restart: Option<String>,
    pub profiles: Option<Vec<String>>,
    pub deploy: Option<serde_yaml::Value>,
    pub logging: Option<serde_yaml::Value>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum PortMapping {
    Short(String),
    Long {
        target: u16,
        published: Option<u16>,
        host_ip: Option<String>,
        protocol: Option<String>,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum DependsOn {
    List(Vec<String>),
    Map(BTreeMap<String, DependsOnCondition>),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DependsOnCondition {
    pub condition: Option<String>,
    pub restart: Option<bool>,
}
```

### 7.3 TCP tunnel core

```rust
use bollard::Docker;
use tokio::net::TcpListener;
use tokio::io::copy_bidirectional;
use tokio::signal;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct Tunnel {
    host_port: u16,
    container_ip: String,
    container_port: u16,
    active_connections: Arc<AtomicUsize>,
    total_connections: Arc<AtomicUsize>,
}

impl Tunnel {
    pub async fn start(
        docker: &Docker,
        container_id: &str,
        container_port: u16,
        network: &str,
        preferred_port: Option<u16>,
    ) -> anyhow::Result<Self> {
        let info = docker.inspect_container(container_id, None).await?;
        let container_ip = info
            .network_settings
            .and_then(|ns| ns.networks)
            .and_then(|nets| nets.get(network).cloned())
            .and_then(|net| net.ip_address)
            .ok_or_else(|| anyhow::anyhow!("Container not connected to network {}", network))?;

        let host_port = match preferred_port {
            Some(p) => p,
            None => find_free_port(container_port).await?,
        };

        let active = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));

        let tunnel = Tunnel {
            host_port,
            container_ip,
            container_port,
            active_connections: active.clone(),
            total_connections: total.clone(),
        };

        let listener = TcpListener::bind(format!("0.0.0.0:{}", host_port)).await?;
        let target_addr = format!("{}:{}", tunnel.container_ip, tunnel.container_port);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((client_stream, _)) => {
                                let target = target_addr.clone();
                                let active_clone = active.clone();
                                let total_clone = total.clone();

                                tokio::spawn(async move {
                                    active_clone.fetch_add(1, Ordering::Relaxed);
                                    total_clone.fetch_add(1, Ordering::Relaxed);

                                    if let Ok(mut server_stream) =
                                        tokio::net::TcpStream::connect(&target).await
                                    {
                                        let mut client = client_stream;
                                        let _ = copy_bidirectional(&mut client, &mut server_stream).await;
                                    }

                                    active_clone.fetch_sub(1, Ordering::Relaxed);
                                });
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                                break;
                            }
                        }
                    }
                    _ = signal::ctrl_c() => {
                        break;
                    }
                }
            }
        });

        Ok(tunnel)
    }
}
```

### 7.4 Compose rewriter

```rust
pub fn rewrite(
    compose: &mut ComposeFile,
    config: &ConduitConfig,
    project_name: &str,
) -> anyhow::Result<RewriteResult> {
    let mut routes = Vec::new();

    for (name, service) in &mut compose.services {
        let should_expose = config.expose.as_ref()
            .map(|e| e.contains_key(name))
            .unwrap_or(false);

        if !should_expose {
            service.ports = None;
        }

        let mut labels = service.labels.clone().unwrap_or_default();
        labels.insert("conduit.managed".into(), "true".into());
        labels.insert("conduit.project".into(), project_name.into());
        labels.insert("conduit.service".into(), name.clone());

        if let Some(route_config) = config.routes.as_ref().and_then(|r| r.get(name)) {
            let domain = &route_config.domain;
            let port = route_config.port.unwrap_or_else(|| guess_http_port(service));
            let router_name = format!("{}-{}", project_name, name);

            labels.insert("traefik.enable".into(), "true".into());
            labels.insert(
                format!("traefik.http.routers.{}.rule", router_name),
                format!("Host(`{}`)", domain),
            );
            labels.insert(
                format!("traefik.http.services.{}.loadbalancer.server.port", router_name),
                port.to_string(),
            );

            routes.push(Route {
                domain: domain.clone(),
                service: name.clone(),
                container_port: port,
            });
        }

        service.labels = Some(labels);
    }

    let network_name = format!("conduit-{}", project_name);
    compose.networks = Some(BTreeMap::from([(
        network_name.clone(),
        Some(NetworkConfig { driver: Some("bridge".into()), ..Default::default() }),
    )]));

    for service in compose.services.values_mut() {
        service.networks = Some(vec![network_name.clone()]);
    }

    Ok(RewriteResult { routes, network: network_name })
}
```

---

## 8. Error Handling

### Error categories and user messages

| Category | Example | User sees |
|----------|---------|-----------|
| **Docker not running** | Socket connection refused | `Error: Docker is not running. Start Docker Desktop or dockerd.` |
| **Port conflict** | Port 80 already in use | `Error: Port 80 is in use (pid 1234: nginx). Stop it or change proxy port in ~/.config/conduit/config.toml` |
| **Compose parse error** | Invalid YAML | `Error: Failed to parse docker-compose.dev.yml:42 — expected string, got integer` |
| **Build failure** | Dockerfile error | `Error: Build failed for service 'cyrex'. Run with --verbose for full output.` |
| **Health check timeout** | Service won't start | `Warning: Service 'milvus' unhealthy after 120s. Other services continued.` |
| **DNS permission** | Can't write /etc/hosts | `Warning: No permission to update /etc/hosts. Run with sudo or use --no-proxy. Routes won't resolve in browser.` |
| **Network conflict** | Network name exists | `Warning: Network 'conduit-deepiri-dev' already exists (stale?). Removing and recreating.` |
| **Container name conflict** | Container already running | `Error: Container 'deepiri-postgres-dev' already exists. Run 'conduit down' first or use a different project name.` |

### Recovery behavior

- **Partial startup failure:** If some services fail to start, Conduit continues starting independent services and reports failures at the end. The user isn't blocked from working on services that did start.
- **Stale state:** If state file says a project is running but containers are gone (manual `docker rm`), `conduit ps` detects this and marks the project as stale. `conduit down` cleans up state.
- **Proxy crash:** If the Traefik container dies, `conduit up` and `conduit proxy status` detect it and offer to restart.

---

## 9. Testing Strategy

### Unit tests

| Module | What's tested | How |
|--------|--------------|-----|
| `compose::parser` | Parse real compose files (Deepiri's, simple ones, edge cases) | Snapshot tests with `insta` crate |
| `compose::rewriter` | Port stripping, label injection, network replacement | Input/output YAML comparison |
| `compose::types` | Serde round-trip (parse → serialize → parse) | Property-based with `proptest` |
| `config::conduit_yml` | Parse various config files, defaults, validation | Fixture files |
| `dns::hosts` | /etc/hosts marker insertion/removal | Temp file I/O |
| `tunnel::tcp` | Port allocation, range checking | Unit tests |
| `registry::state` | State serialization, conflict detection | JSON round-trip |

### Integration tests (require Docker)

| Test | What it verifies |
|------|-----------------|
| `up_down_basic` | Start a minimal compose (1 service), verify container running, stop, verify removed |
| `up_strips_ports` | Start compose with ports, verify no host bindings |
| `up_creates_network` | Verify conduit-managed network exists |
| `up_labels` | Verify conduit labels on containers |
| `tunnel_basic` | Start compose, open tunnel, verify TCP connectivity |
| `tunnel_cleanup` | Open tunnel, Ctrl+C, verify port freed |
| `multi_project` | Start two projects, verify isolation, verify no conflicts |
| `link_projects` | Link two projects, verify cross-network connectivity |
| `proxy_routes` | Start proxy + project, verify HTTP routing works |

Integration tests use a `tests/fixtures/` directory with minimal compose files:

```
tests/
├── fixtures/
│   ├── simple/
│   │   └── docker-compose.yml      # 1 nginx service
│   ├── multi-port/
│   │   └── docker-compose.yml      # 3 services, conflicting ports
│   ├── with-db/
│   │   └── docker-compose.yml      # postgres + app
│   ├── with-healthcheck/
│   │   └── docker-compose.yml      # service with healthcheck
│   └── deepiri-minimal/
│       ├── docker-compose.yml      # subset of real Deepiri compose
│       └── .conduit.yml
└── integration/
    ├── up_down_test.rs
    ├── tunnel_test.rs
    ├── multi_project_test.rs
    └── proxy_test.rs
```

### E2E test (manual, documented)

A script that:
1. `conduit up` with deepiri-platform's real compose
2. Verifies all domains resolve
3. Curls each routed service
4. Opens a DB tunnel, runs a query
5. Starts a second project
6. `conduit ps` shows both
7. `conduit down --all`

---

## 10. Security

### Docker socket access

Conduit needs access to `/var/run/docker.sock`. This is the same privilege as running `docker` commands. Conduit does NOT:
- Mount the socket into the Traefik container (uses file provider instead)
- Execute arbitrary commands inside containers
- Access container filesystems

### /etc/hosts modification

Requires elevated privileges. Conduit:
- Prompts for sudo explicitly (no silent escalation)
- Only modifies lines between `CONDUIT START` and `CONDUIT END` markers
- Never touches lines outside its markers
- Cleans up on `conduit down`

### TLS certificates

Self-signed certs generated by `rcgen` for `*.local` domains. Stored in `~/.local/share/conduit/certs/`. Browsers will show a warning on first visit — users need to trust the CA once.

Future: Add a `conduit trust` command that installs the CA into the system trust store.

### Credential handling

`conduit db` extracts database credentials from compose environment variables to format connection strings. Credentials are:
- **Never stored on disk** (only held in memory during tunnel lifetime)
- **Never logged** (redacted in verbose output)
- **Never sent anywhere** (displayed to terminal only)

---

## 11. Platform-Specific Notes

### WSL2 (your setup)

- Docker socket at `/var/run/docker.sock` — works if Docker Desktop WSL integration is enabled.
- `/etc/hosts` in WSL maps to the WSL instance, NOT Windows. For browser access from Windows, Conduit also needs to update `C:\Windows\System32\drivers\etc\hosts` via `wsl.exe` or `powershell.exe`.
- `conduit doctor` detects WSL2 and checks both hosts files.
- Port forwarding: WSL2 automatically forwards ports to Windows for `0.0.0.0` bindings. Traefik on port 80 in WSL2 is accessible from Windows browsers at `localhost:80`.

### macOS

- Docker Desktop provides `/var/run/docker.sock`.
- `/etc/hosts` requires `sudo`. Same marker strategy.
- Alternative: use `dnsmasq` via Homebrew for `*.local` without sudo.
- Note: macOS reserves `.local` for mDNS (Bonjour). Using `dnsmasq` or `/etc/hosts` overrides this, but it can cause 5-second DNS lookup delays in some cases. Alternative: use `.localhost` (reserved by RFC 6761) or `.test` (reserved by RFC 2606) instead of `.local`.

### Linux (native Docker)

- Straightforward. Docker socket at default path.
- `/etc/hosts` with sudo, or `systemd-resolved` for sudoless setup.
- No `.local` mDNS conflict if Avahi/mDNS is not running.

### Windows (native, no WSL)

- Lower priority. Docker Desktop provides named pipe `//./pipe/docker_engine`.
- `bollard` supports Windows named pipes.
- Hosts file at `C:\Windows\System32\drivers\etc\hosts`.
- Requires Administrator for hosts file modification.

---

## 12. Comparison with Alternatives

### vs. Docker Compose (alone)

| | Docker Compose | Conduit |
|-|---------------|---------|
| Port conflicts | Your problem | Eliminated |
| Service URLs | `localhost:<port>` | Named domains |
| Multi-project | Manual coordination | Automatic isolation |
| DB access | Always exposed | On-demand tunnels |
| Service grouping | `docker compose up svc1 svc2...` | `conduit up --group core` |
| Status overview | `docker ps` (flat list) | `conduit ps` (grouped by project) |
| Dependency | None | Requires Conduit binary |

### vs. Traefik (standalone)

Traefik solves HTTP routing. It doesn't solve port conflicts, database tunneling, multi-project isolation, compose rewriting, or service grouping. You'd still need to manually configure labels, manage networks, and handle DNS. Conduit automates all of that and uses Traefik under the hood.

### vs. Tilt

| | Tilt | Conduit |
|-|------|---------|
| Target | Kubernetes (local) | Docker Compose |
| Language | Starlark (Python-like) | YAML config |
| Scope | Build + deploy + watch | Orchestrate + route + tunnel |
| Complexity | High (K8s concepts) | Low (compose concepts) |
| File watching | Built-in (live update) | Not a goal (compose handles restarts) |
| Install | Go binary | Rust binary |

Tilt is for teams already on Kubernetes. Conduit is for teams on Docker Compose who don't want Kubernetes.

### vs. Lando / DDEV

| | Lando / DDEV | Conduit |
|-|-------------|---------|
| Target audience | PHP/CMS developers | Microservice developers |
| Compose integration | Generates compose files | Reads existing compose files |
| Configuration | `.lando.yml` defines stack | `.conduit.yml` configures routing |
| Service definitions | Lando-specific (recipes) | Standard Docker Compose |
| Language | Node.js | Rust |
| Lock-in | High (Lando-specific format) | Low (your compose files unchanged) |

Conduit doesn't replace your compose files. It wraps them. Remove Conduit and your compose files still work exactly as before.

### vs. Devspace / Garden / Skaffold

All three target Kubernetes development workflows. If you're not on K8s for local dev, they're not applicable. Conduit targets the Docker Compose workflow specifically.

### vs. Nginx Proxy Manager

GUI-based reverse proxy. No compose integration, no port management, no tunneling, no multi-project isolation. Different tool for a different audience (self-hosters, not developers).

---

## 13. Installation & Distribution

### Method 1: Cargo (Rust toolchain required)

```bash
cargo install deepiri-conduit
```

Installs `conduit` to `~/.cargo/bin/`. Requires Rust toolchain.

### Method 2: Homebrew

```bash
brew tap deepiri/tap
brew install conduit
```

Requires `Team-Deepiri/homebrew-tap` repo with formula:

```ruby
class Conduit < Formula
  desc "Local dev orchestrator for Docker Compose"
  homepage "https://github.com/Team-Deepiri/deepiri-conduit"
  version "0.1.0"

  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/Team-Deepiri/deepiri-conduit/releases/download/v0.1.0/conduit-aarch64-apple-darwin.tar.gz"
    sha256 "..."
  elsif OS.mac?
    url "https://github.com/Team-Deepiri/deepiri-conduit/releases/download/v0.1.0/conduit-x86_64-apple-darwin.tar.gz"
    sha256 "..."
  elsif OS.linux? && Hardware::CPU.arm?
    url "https://github.com/Team-Deepiri/deepiri-conduit/releases/download/v0.1.0/conduit-aarch64-unknown-linux-gnu.tar.gz"
    sha256 "..."
  else
    url "https://github.com/Team-Deepiri/deepiri-conduit/releases/download/v0.1.0/conduit-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "..."
  end

  def install
    bin.install "conduit"
  end

  test do
    system "#{bin}/conduit", "--version"
  end
end
```

### Method 3: Curl script

```bash
curl -fsSL https://raw.githubusercontent.com/Team-Deepiri/deepiri-conduit/main/install.sh | sh
```

Script logic:

```bash
#!/bin/sh
set -eu

REPO="Team-Deepiri/deepiri-conduit"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

detect_platform() {
  OS=$(uname -s | tr '[:upper:]' '[:lower:]')
  ARCH=$(uname -m)
  case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
  esac
  case "$OS" in
    linux)  TARGET="${ARCH}-unknown-linux-gnu" ;;
    darwin) TARGET="${ARCH}-apple-darwin" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
  esac
}

get_latest_version() {
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"v//' | sed 's/".*//'
}

main() {
  detect_platform
  VERSION=$(get_latest_version)
  URL="https://github.com/$REPO/releases/download/v${VERSION}/conduit-${TARGET}.tar.gz"
  CHECKSUM_URL="${URL}.sha256"

  echo "Installing conduit v${VERSION} for ${TARGET}..."

  TMP=$(mktemp -d)
  curl -fsSL "$URL" -o "$TMP/conduit.tar.gz"
  curl -fsSL "$CHECKSUM_URL" -o "$TMP/conduit.sha256"

  cd "$TMP"
  sha256sum -c conduit.sha256

  tar xzf conduit.tar.gz
  mkdir -p "$INSTALL_DIR"
  mv conduit "$INSTALL_DIR/conduit"
  chmod +x "$INSTALL_DIR/conduit"

  echo "Installed conduit to $INSTALL_DIR/conduit"

  if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    echo "Add to PATH: export PATH=\"$INSTALL_DIR:\$PATH\""
  fi

  rm -rf "$TMP"
}

main
```

### Method 4: GitHub Releases (manual)

Download binary from releases page, extract, put in PATH.

### Method 5: AUR (Arch Linux, future)

```bash
yay -S conduit-bin
```

### Method 6: Nix (future)

```bash
nix run github:Team-Deepiri/deepiri-conduit
```

---

## 14. CI/CD Pipeline

### `.github/workflows/ci.yml` — on every push and PR

```yaml
name: CI
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy -- -D warnings
      - run: cargo test

  build:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build

  integration:
    runs-on: ubuntu-latest
    services:
      docker:
        image: docker:dind
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --test '*' -- --ignored  # Integration tests marked #[ignore]
```

### `.github/workflows/release.yml` — on tag push

```yaml
name: Release
on:
  push:
    tags: ['v*']
jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Install cross (Linux ARM)
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: cargo install cross
      - name: Build
        run: |
          if [[ "${{ matrix.target }}" == "aarch64-unknown-linux-gnu" ]]; then
            cross build --release --target ${{ matrix.target }}
          else
            cargo build --release --target ${{ matrix.target }}
          fi
      - name: Package
        run: |
          cd target/${{ matrix.target }}/release
          tar czf conduit-${{ matrix.target }}.tar.gz conduit
          sha256sum conduit-${{ matrix.target }}.tar.gz > conduit-${{ matrix.target }}.tar.gz.sha256
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            target/${{ matrix.target }}/release/conduit-${{ matrix.target }}.tar.gz
            target/${{ matrix.target }}/release/conduit-${{ matrix.target }}.tar.gz.sha256

  publish-crate:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo publish --token ${{ secrets.CARGO_REGISTRY_TOKEN }}

  update-homebrew:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          repository: Team-Deepiri/homebrew-tap
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
      - name: Update formula
        run: |
          # Script to update SHA256 hashes and version in formula
          ./update-formula.sh ${{ github.ref_name }}
      - uses: peter-evans/create-pull-request@v7
        with:
          title: "Update conduit to ${{ github.ref_name }}"
```

---

## 15. Phased Roadmap

### Phase 0: Project Bootstrap (days 1-2)

- [ ] `cargo init` with workspace structure
- [ ] `Cargo.toml` with all dependencies (version-pinned)
- [ ] `.github/workflows/ci.yml` (fmt + clippy + test)
- [ ] `README.md` (with architecture diagram, install instructions)
- [ ] `LICENSE` (MIT)
- [ ] `.gitignore`
- [ ] Module skeleton (all `mod.rs` files with `todo!()` stubs)
- [ ] `conduit --version` and `conduit --help` work

**Deliverable:** Repo compiles, CI passes, help text renders.

### Phase 1: Compose Parse + Container Lifecycle (weeks 1-2)

- [ ] `compose::parser` — parse normalized YAML from `docker compose config`
- [ ] `compose::types` — full type definitions for Compose v3.x
- [ ] `compose::rewriter` — strip ports, replace networks, inject labels
- [ ] `docker::client` — bollard wrapper with connection retry
- [ ] `docker::network` — create, destroy, connect, list
- [ ] `docker::container` — create, start, stop, remove, inspect, list
- [ ] `registry::state` — read/write state JSON file
- [ ] `cli::up` — full implementation (parse → rewrite → create network → start containers → save state)
- [ ] `cli::down` — stop containers, remove network, clean state
- [ ] `cli::ps` — list projects from state file, verify against Docker
- [ ] Startup ordering via topological sort of `depends_on`
- [ ] Progress display during startup (indicatif)
- [ ] Unit tests for parser with Deepiri's real compose files as fixtures
- [ ] Unit tests for rewriter
- [ ] Integration test: up + ps + down with a simple compose

**Deliverable:** `conduit up` starts Deepiri's 26 services without host port bindings. `conduit down` stops them. `conduit ps` shows status.

### Phase 2: HTTP Routing + DNS (weeks 3-4)

- [ ] `proxy::manager` — create/start/stop Traefik container
- [ ] `proxy::traefik` — generate Traefik static config, dynamic config per project
- [ ] `proxy::tls` — generate self-signed CA + wildcard cert via `rcgen`
- [ ] Traefik file provider: write dynamic config to Docker volume
- [ ] Connect proxy container to project network
- [ ] `dns::hosts` — add/remove entries in `/etc/hosts` with markers
- [ ] sudo escalation for hosts file (prompt user, handle failure gracefully)
- [ ] `config::conduit_yml` — parse `.conduit.yml`
- [ ] `config::global` — parse `~/.config/conduit/config.toml`
- [ ] Merge project config + global config
- [ ] Route auto-generation: services without explicit routes get `<service>.<domain>`
- [ ] WebSocket support via Traefik middleware labels
- [ ] `cli::route` — display routing table
- [ ] `cli::init` — generate `.conduit.yml` from existing compose
- [ ] `cli::proxy` — `conduit proxy status`, `conduit proxy restart`
- [ ] Write `.conduit.yml` for deepiri-platform
- [ ] Integration test: start project, curl domain through proxy
- [ ] Integration test: two projects, different domains, both work

**Deliverable:** `frontend.deepiri.local` loads in browser after `conduit up`. Self-signed HTTPS works.

### Phase 3: TCP Tunnels (weeks 5-6)

- [ ] `tunnel::tcp` — async TCP listener + bidirectional copy
- [ ] Container IP resolution via bollard inspect
- [ ] Free port allocation with preferred ranges per DB type
- [ ] DB type detection from image name (`postgres:*` → PostgreSQL, `mongo:*` → MongoDB, etc.)
- [ ] Credential extraction from compose `environment:` block
- [ ] Connection string formatting for: PostgreSQL, MongoDB, Redis, MySQL, ClickHouse
- [ ] Connection counter (active + total)
- [ ] Graceful shutdown (Ctrl+C → drain → close)
- [ ] Background tunnel mode (`conduit db --background`)
- [ ] Tunnel state tracking in state file
- [ ] `conduit ps` shows active tunnels
- [ ] `cli::db` — full implementation
- [ ] Integration test: start postgres, open tunnel, run query via `psql`
- [ ] Integration test: multiple simultaneous tunnels
- [ ] Integration test: tunnel cleanup on Ctrl+C

**Deliverable:** `conduit db deepiri postgres` prints a working `psql` command. Tunnel survives heavy query load.

### Phase 4: Multi-Project + Groups + Polish (weeks 7-8)

- [ ] Service groups from `.conduit.yml` with dependency resolution
- [ ] `conduit up --group core` starts infra + core only
- [ ] Multi-project state tracking (multiple entries in state file)
- [ ] `conduit link` / `conduit unlink` — connect/disconnect project networks
- [ ] `cli::logs` — multiplexed, color-coded log tailing via bollard
- [ ] Log filtering by service name and group
- [ ] `cli::doctor` — comprehensive system checks:
  - Docker version and connectivity
  - Docker Compose CLI availability
  - Port 80/443 availability
  - /etc/hosts permissions and stale entries
  - WSL2 detection and Windows hosts file sync
  - Proxy container health
  - State file validity
  - Disk space
- [ ] `--json` output for all commands (for scripting / CI)
- [ ] `--no-proxy` mode (keep original port bindings)
- [ ] `conduit down --volumes` (remove data volumes)
- [ ] `conduit down --all` (stop everything)
- [ ] Shell completions via `clap_complete` (bash, zsh, fish)
- [ ] Stale state detection and cleanup
- [ ] Improved error messages for common failures
- [ ] Man page generation

**Deliverable:** Full feature-complete v1.0.

### Phase 5: Distribution + Launch (week 9)

- [ ] `.github/workflows/release.yml` — cross-compile 6 targets
- [ ] `install.sh` curl script
- [ ] Homebrew tap (`Team-Deepiri/homebrew-tap`)
- [ ] `cargo publish` to crates.io
- [ ] `README.md` with:
  - Architecture diagram
  - Install instructions (all methods)
  - Quick start guide
  - Full CLI reference
  - Configuration reference
  - FAQ
- [ ] Demo GIF/video (asciinema recording)
- [ ] `CHANGELOG.md`
- [ ] `.conduit.yml` committed to `deepiri-platform`
- [ ] Announce to team

**Deliverable:** Anyone can `cargo install deepiri-conduit` or `brew install conduit` and use it.

---

## 16. v2+ Future Roadmap

These are post-1.0 ideas. Not committed to, but tracked.

### v1.1: File watching + auto-rebuild

- Watch `src/` directories for changes
- Auto-rebuild and restart affected containers
- Similar to Tilt's live update but for Compose

### v1.2: `conduit exec`

- `conduit exec deepiri cyrex bash` — exec into a container without looking up the container name
- `conduit exec deepiri postgres psql` — auto-connect to DB with credentials

### v1.3: Dashboard

- `conduit dashboard` opens a local web UI
- Shows all projects, services, health, logs, routes
- Built with htmx or a minimal Rust web framework (axum)

### v1.4: Remote tunnels

- `conduit tunnel deepiri --remote user@server` — expose local services to a remote machine
- Useful for mobile testing, team demos

### v1.5: Plugin system

- `conduit plugin install <name>` — extend Conduit with custom providers
- Plugins for: Caddy (instead of Traefik), Consul, custom DNS providers

### v1.6: Resource monitoring

- `conduit top` — per-container CPU/memory usage
- `conduit stats` — aggregate resource usage per project
- Alert if a container exceeds memory limit

### v2.0: conduit.dev cloud

- Share local dev environments with teammates
- `conduit share deepiri` → generates a public URL (like ngrok but for entire projects)
- Team-wide `.conduit.yml` with shared presets

---

## 17. What Conduit is NOT

| Conduit is NOT... | Because... |
|-------------------|------------|
| A Docker Compose replacement | It reads your existing compose files. You keep writing compose. Remove Conduit and everything still works. |
| Kubernetes | No pods, no deployments, no ingress controllers. It's for local dev with Docker Compose. |
| Tilt / Garden / Devspace | Those target K8s. Conduit targets raw Docker Compose. |
| Lando / DDEV | Those are CMS-oriented (WordPress, Drupal). Conduit is for microservice platforms. They generate compose files; Conduit reads yours. |
| A process manager | It delegates to Docker. It doesn't run or supervise processes itself. |
| A build tool | It can trigger `docker compose build`, but it doesn't replace your Dockerfiles or build pipeline. |
| A CI/CD tool | It's for local development. Use it in CI only for integration testing. |
| A production deployment tool | It manages local routing via Traefik and /etc/hosts. Not suitable for production. |

---

## 18. Open Questions

### Resolved

| # | Question | Decision | Rationale |
|---|----------|----------|-----------|
| 1 | Compose parsing: reimplement or shell out? | Shell out to `docker compose config` first, parse result | Avoids reimplementing interpolation. Fallback to basic parsing if CLI unavailable. |
| 2 | Proxy: Traefik or Caddy? | Traefik | Better Docker integration, file provider, widely used. Caddy as future plugin option. |
| 3 | DNS: /etc/hosts vs dnsmasq? | /etc/hosts for v1 | Simplest, works everywhere. dnsmasq/systemd-resolved as config options later. |
| 4 | Docker interaction: CLI or API? | API (bollard) | No dependency on docker-compose binary for container ops. CLI fallback for build. |
| 5 | Submodule or standalone? | Standalone | Installable, independent release cycle, open-sourceable. |

### Unresolved (resolve during Phase 1)

| # | Question | Options | Leaning |
|---|----------|---------|---------|
| 6 | `.local` vs `.test` vs `.localhost` domain suffix | `.local` (macOS mDNS conflict risk), `.test` (RFC reserved), `.localhost` (RFC 6761) | `.localhost` (no mDNS conflict, RFC-compliant, resolves to 127.0.0.1 by default in many systems) |
| 7 | WSL2 Windows hosts file sync | Auto-sync via `wsl.exe`, or require manual step | Auto-sync with user prompt |
| 8 | Image builds: bollard or docker CLI? | bollard (complex for multi-stage), CLI (simple, reliable) | CLI for builds, bollard for everything else |
| 9 | State file locking | flock (Unix), advisory locks | flock — prevents two `conduit up` from racing |
| 10 | Minimum Docker version | 20.10 (2020), 23.0 (2023), 24.0 (2024) | 20.10 (broad compat, supports all APIs we need) |

---

## 19. Glossary

| Term | Meaning |
|------|---------|
| **Project** | A directory containing a `docker-compose.yml` (and optionally `.conduit.yml`). Conduit manages projects. |
| **Service** | A single container definition in a compose file (e.g., `postgres`, `api-gateway`). |
| **Group** | A named set of services in `.conduit.yml` for selective startup. |
| **Route** | A domain → container mapping managed by the proxy (e.g., `api.deepiri.local → api-gateway:5000`). |
| **Tunnel** | A temporary TCP proxy from a host port to a container port (e.g., `localhost:54329 → postgres:5432`). |
| **Proxy** | The Traefik container that handles HTTP routing for all projects. |
| **State** | The JSON file tracking all running projects, services, routes, and tunnels. |
| **Rewriter** | The module that transforms compose files in-memory (strips ports, injects labels, swaps networks). |
