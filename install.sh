#!/usr/bin/env bash
# Install `conduit` CLI tool and optionally Rust
#
# Usage:
#   ./scripts/install.sh              # install from source (installs Rust if needed)
#   ./scripts/install.sh --release     # download latest release
#   ./scripts/install.sh --rust     # just install Rust
#
# Options:
#   --release    Download prebuilt binary (Linux x86_64)
#   --rust      Install Rust first
#   --force     Reinstall even if exists

set -euo pipefail

FORCE=0
USE_RELEASE=0
INSTALL_RUST=0
INSTALL_DIR="${CONDUIT_INSTALL_DIR:-$HOME/.local/bin}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) FORCE=1; shift ;;
    --release) USE_RELEASE=1; shift ;;
    --rust) INSTALL_RUST=1; shift ;;
    -h|--help)
      echo "Usage: $0 [--release] [--rust] [--force]"
      echo ""
      echo "Options:"
      echo "  --release    Download prebuilt binary"
      echo "  --rust      Install Rust first"
      echo "  --force      Reinstall over existing"
      exit 0
      ;;
    *) echo "Unknown: $1" >&2; exit 1 ;;
  esac
done

ensure_dir() {
  if [[ ! -d "$INSTALL_DIR" ]]; then
    mkdir -p "$INSTALL_DIR"
    echo "Created: $INSTALL_DIR"
  fi
}

install_rust() {
  if command -v cargo >/dev/null 2>&1; then
    echo "Rust already installed"
    return
  fi

  echo "Installing Rust..."
  if command -v rustup >/dev/null 2>&1; then
    rustup update stable
  else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
  fi

  echo "✓ Rust installed"
}

check_rust() {
  if ! command -v cargo >/dev/null 2>&1; then
    echo "Installing Rust..." >&2
    install_rust
  fi
}

find_repo_root() {
  local here="${BASH_SOURCE[0]}"
  local d
  d="$(cd "$(dirname "$here")" && pwd)"
  if [[ -f "$d/../Cargo.toml" ]]; then
    echo "$(cd "$d/.." && pwd)"
  elif [[ -f "${PWD}/Cargo.toml" ]]; then
    echo "$PWD"
  else
    echo ""
  fi
}

install_from_source() {
  local repo_root
  repo_root="$(find_repo_root)"

  if [[ -z "$repo_root" || ! -f "$repo_root/Cargo.toml" ]]; then
    echo "Error: Could not find deepiri-conduit repo" >&2
    echo "Run from the repo directory or clone it first" >&2
    exit 1
  fi

  echo "Building from source..."
  (cd "$repo_root" && cargo build --release)

  local dest="$INSTALL_DIR/conduit"
  if [[ -f "$dest" && "$FORCE" -eq 0 ]]; then
    echo "Already installed: $dest"
    echo "Use --force to reinstall"
    return
  fi

  cp -f "$repo_root/target/release/conduit" "$dest"
  chmod +x "$dest"
  echo "Installed: $dest"
}

download_release() {
  local tag
  tag="$(curl -fsSL 'https://api.github.com/repos/Team-Deepiri/deepiri-conduit/releases/latest' | grep -m1 '"tag_name":' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

  local os arch url
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
  esac

  if [[ "$os" != linux ]] || [[ "$arch" != x86_64 ]]; then
    echo "No prebuilt for $os $arch. Use source install." >&2
    exit 1
  fi

  url="https://github.com/Team-Deepiri/deepiri-conduit/releases/download/$tag/conduit-x86_64-unknown-linux-gnu"

  echo "Downloading release $tag..."
  curl -fsSL "$url" -o "$INSTALL_DIR/conduit"
  chmod +x "$INSTALL_DIR/conduit"
  echo "Installed: $INSTALL_DIR/conduit"
}

add_to_path() {
  if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "Add to your PATH (add to ~/.bashrc or ~/.zshrc):"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
}

main() {
  ensure_dir

  if [[ "$INSTALL_RUST" -eq 1 ]]; then
    install_rust
    return
  fi

  if [[ "$USE_RELEASE" -eq 1 ]]; then
    download_release
  else
    check_rust
    install_from_source
  fi

  echo ""
  echo "✓ Conduit installed!"
  echo ""
  echo "Quick start:"
  echo "  conduit doctor              # check system"
  echo "  conduit ui                 # dashboard"
  echo "  conduit submod             # resolve submodule conflicts"
  echo "  conduit submod --interactive"
}

main