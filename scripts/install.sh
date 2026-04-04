#!/usr/bin/env bash
# Install the `conduit` binary into ~/.local/bin (or CONDUIT_INSTALL_DIR).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Team-Deepiri/deepiri-conduit/main/scripts/install.sh | bash
#
# From a git clone (installs release build from source):
#   ./scripts/install.sh --from-source
#
# Options:
#   --from-source     Build with cargo from the parent of this script (repo root)
#   --version TAG     Use a specific release tag (default: latest GitHub release)
#
# Environment:
#   CONDUIT_INSTALL_DIR   Install directory (default: ~/.local/bin)
#   GITHUB_REPO           owner/repo (default: Team-Deepiri/deepiri-conduit)

set -euo pipefail

REPO="${GITHUB_REPO:-Team-Deepiri/deepiri-conduit}"
INSTALL_DIR="${CONDUIT_INSTALL_DIR:-$HOME/.local/bin}"
FROM_SOURCE=0
VERSION_ARG=""

# Repo root: directory containing this repo's Cargo.toml (works when cloned; also if you `cd` into the clone).
resolve_repo_root() {
  local here="${BASH_SOURCE[0]:-}"
  if [[ -n "$here" ]]; then
    local d
    d="$(cd "$(dirname "$here")" 2>/dev/null && pwd || true)"
    if [[ -n "$d" && -f "$d/../Cargo.toml" ]]; then
      (cd "$d/.." && pwd)
      return
    fi
  fi
  if [[ -f "${PWD}/Cargo.toml" ]] && grep -q '^name = "deepiri-conduit"' "${PWD}/Cargo.toml" 2>/dev/null; then
    echo "$PWD"
    return
  fi
  echo ""
}

REPO_ROOT="$(resolve_repo_root)"

usage() {
  cat <<'EOF'
Install the `conduit` binary into ~/.local/bin (or CONDUIT_INSTALL_DIR).

Usage:
  curl -fsSL https://raw.githubusercontent.com/Team-Deepiri/deepiri-conduit/main/scripts/install.sh | bash
  ./scripts/install.sh [--from-source] [--version TAG]

Options:
  --from-source     Build with cargo from the repository (clone required)
  --version TAG     Release tag to download (default: latest)

Environment:
  CONDUIT_INSTALL_DIR   Install directory (default: ~/.local/bin)
  GITHUB_REPO           owner/repo (default: Team-Deepiri/deepiri-conduit)
EOF
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --from-source) FROM_SOURCE=1; shift ;;
    --version)
      VERSION_ARG="${2:?}"
      shift 2
      ;;
    *) echo "Unknown option: $1" >&2; usage 1 ;;
  esac
done

ensure_dir() {
  if [[ ! -d "$INSTALL_DIR" ]]; then
    mkdir -p "$INSTALL_DIR"
  fi
}

install_binary() {
  local src="$1"
  local dest="$INSTALL_DIR/conduit"
  cp -f "$src" "$dest"
  chmod +x "$dest"
  echo "Installed: $dest"
}

from_source() {
  command -v cargo >/dev/null 2>&1 || {
    echo "cargo not found. Install Rust: https://rustup.rs" >&2
    exit 1
  }
  if [[ -z "$REPO_ROOT" || ! -f "$REPO_ROOT/Cargo.toml" ]]; then
    echo "Could not find the deepiri-conduit repo. Clone it, cd into it, and run:" >&2
    echo "  ./scripts/install.sh --from-source" >&2
    exit 1
  fi
  echo "Building release binary from $REPO_ROOT ..."
  (cd "$REPO_ROOT" && cargo build --release)
  install_binary "$REPO_ROOT/target/release/conduit"
}

latest_tag() {
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -m1 '"tag_name":' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
}

download_release() {
  local tag="${VERSION_ARG:-$(latest_tag)}"
  [[ -n "$tag" ]] || { echo "Could not determine release tag." >&2; exit 1; }

  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) arch="unknown" ;;
  esac

  if [[ "$os" != linux ]] || [[ "$arch" != x86_64 ]]; then
    echo "Prebuilt binary is only published for Linux x86_64 right now (got $os $arch)." >&2
    echo "Use --from-source from a clone, or build with:" >&2
    echo "  cargo install --git https://github.com/$REPO.git $tag" >&2
    exit 1
  fi

  local asset="conduit-x86_64-unknown-linux-gnu"
  local base="https://github.com/$REPO/releases/download/$tag"
  local tmp
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  echo "Downloading $tag ($asset) ..."
  curl -fsSL "$base/$asset" -o "$tmp/conduit"
  curl -fsSL "$base/$asset.sha256" -o "$tmp/checksum.sha256"

  (cd "$tmp" && sha256sum -c checksum.sha256) || {
    echo "Checksum verification failed." >&2
    exit 1
  }

  install_binary "$tmp/conduit"
}

main() {
  ensure_dir
  if [[ "$FROM_SOURCE" -eq 1 ]]; then
    from_source
  else
    download_release
  fi

  echo
  if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "Add to your PATH (e.g. in ~/.bashrc or ~/.zshrc):"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo
  fi
  echo "Quick start:"
  echo "  conduit doctor"
  echo "  conduit ui       # local dashboard (http://127.0.0.1:9842)"
  echo "  conduit up       # in your compose project"
}

main "$@"
