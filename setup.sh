#!/usr/bin/env bash
# conduit — full system setup script
# Installs all deps (Docker, Docker Compose, Rust) + conduit itself.
# Run: curl -fsSL https://raw.githubusercontent.com/Team-Deepiri/deepiri-conduit/main/setup.sh | bash
# Or:  ./setup.sh

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; MAGENTA='\033[0;35m'; CYAN='\033[0;36m'
BOLD='\033[1m'; RESET='\033[0m'

INSTALL_DIR="${CONDUIT_INSTALL_DIR:-$HOME/.local/bin}"
FORCE=0
SKIP_DOCKER=0
SKIP_RUST=0
USE_RELEASE=0

banner() {
  cat <<'EOF'
  ██████╗ ██████╗       ██╗    ██╗ █████╗ ██████╗ ███████╗
 ██╔════╝ ██╔══██╗      ██║    ██║██╔══██╗██╔══██╗██╔════╝
 ██║  ███╗██████╔╝      ██║ █╗ ██║███████║██████║ █████╗
 ██║   ██║██╔══██╗      ██║███╗██║██╔══██║██╔══██╗██╔══╝
 ╚██████╔╝██║  ██║      ╚███╔███╔╝██║  ██║██║  ██║███████╗
  ╚═════╝ ╚═╝  ╚═╝       ╚══╝╚══╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝
  ══ SETUP ═══════════════════════════════════════════════════
EOF
  echo -e "${CYAN}  Local dev orchestrator for multi-service Docker Compose projects${RESET}"
  echo ""
}

info()  { echo -e "${CYAN}➜${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET} $*"; }
warn()  { echo -e "${YELLOW}⚠${RESET} $*"; }
error() { echo -e "${RED}✗${RESET} $*"; }
bold()  { echo -e "${BOLD}$*${RESET}"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) FORCE=1; shift ;;
    --skip-docker) SKIP_DOCKER=1; shift ;;
    --skip-rust) SKIP_RUST=1; shift ;;
    --release) USE_RELEASE=1; shift ;;
    -h|--help)
      echo "Usage: $0 [options]"
      echo ""
      echo "Options:"
      echo "  --force         Reinstall conduit if already installed"
      echo "  --skip-docker   Skip Docker installation"
      echo "  --skip-rust     Skip Rust installation"
      echo "  --release       Download prebuilt binary instead of building"
      exit 0
      ;;
    *) echo "Unknown: $1" >&2; exit 1 ;;
  esac
done

detect_os() {
  if [[ "$OSTYPE" == linux-gnu* ]]; then
    if command -v apt-get &>/dev/null; then echo "debian"
    elif command -v dnf &>/dev/null; then echo "fedora"
    elif command -v yum &>/dev/null; then echo "rhel"
    elif command -v pacman &>/dev/null; then echo "arch"
    elif command -v zypper &>/dev/null; then echo "suse"
    else echo "linux"
    fi
  elif [[ "$OSTYPE" == darwin* ]]; then echo "macos"
  else echo "$OSTYPE"
  fi
}

OS=$(detect_os)

install_system_deps() {
  info "Installing system build dependencies..."
  case "$OS" in
    debian)
      sudo apt-get update -qq
      sudo apt-get install -y -qq curl git build-essential pkg-config libssl-dev >/dev/null
      ;;
    fedora|rhel)
      sudo dnf install -y curl git gcc pkg-config openssl-devel >/dev/null 2>&1 || \
      sudo yum install -y curl git gcc pkg-config openssl-devel >/dev/null 2>&1
      ;;
    arch)
      sudo pacman -S --noconfirm curl git base-devel pkg-config openssl >/dev/null 2>&1
      ;;
    suse)
      sudo zypper install -y curl git gcc pkg-config libopenssl-devel >/dev/null 2>&1
      ;;
    macos)
      if ! command -v xcode-select &>/dev/null; then
        xcode-select --install 2>/dev/null || true
      fi
      ;;
  esac
  success "System dependencies ready"
}

install_docker() {
  if command -v docker &>/dev/null && docker info &>/dev/null 2>&1; then
    success "Docker already installed and running"
    return
  fi

  if [[ "$SKIP_DOCKER" -eq 1 ]]; then
    warn "Skipping Docker installation (--skip-docker)"
    return
  fi

  info "Installing Docker..."

  case "$OS" in
    debian)
      curl -fsSL https://get.docker.com | sudo sh
      sudo usermod -aG docker "$USER" || true
      ;;
    fedora)
      sudo dnf install -y docker docker-compose-plugin
      sudo systemctl enable --now docker
      sudo usermod -aG docker "$USER" || true
      ;;
    arch)
      sudo pacman -S --noconfirm docker docker-compose
      sudo systemctl enable --now docker
      sudo usermod -aG docker "$USER" || true
      ;;
    macos)
      warn "Docker Desktop for macOS: https://docs.docker.com/desktop/install/mac/"
      warn "Install manually, then re-run this script"
      exit 1
      ;;
    *)
      curl -fsSL https://get.docker.com | sudo sh
      sudo usermod -aG docker "$USER" || true
      ;;
  esac

  if command -v docker &>/dev/null; then
    success "Docker installed"
    warn "You may need to log out and back in for group changes to take effect"
    warn "Or run: newgrp docker"
  else
    error "Docker installation failed"
    exit 1
  fi
}

install_rust() {
  if command -v cargo &>/dev/null; then
    success "Rust already installed (cargo $(cargo --version | cut -d' ' -f2))"
    return
  fi

  if [[ "$SKIP_RUST" -eq 1 ]]; then
    warn "Skipping Rust installation (--skip-rust)"
    return
  fi

  info "Installing Rust via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path

  export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
  if [[ -f "$CARGO_HOME/env" ]]; then
    source "$CARGO_HOME/env"
  fi

  if command -v cargo &>/dev/null; then
    success "Rust $(rustc --version) installed"
  else
    error "Rust installation failed"
    exit 1
  fi
}

ensure_dir() {
  if [[ ! -d "$INSTALL_DIR" ]]; then
    mkdir -p "$INSTALL_DIR"
    info "Created $INSTALL_DIR"
  fi
}

add_to_path() {
  local rc_files=()
  if [[ "$SHELL" == *zsh* ]]; then
    rc_files+=("$HOME/.zshrc")
  elif [[ "$SHELL" == *bash* ]]; then
    rc_files+=("$HOME/.bashrc")
    if [[ -f "$HOME/.bash_profile" ]]; then
      rc_files+=("$HOME/.bash_profile")
    fi
  fi

  local path_line="export PATH=\"$INSTALL_DIR:\$PATH\""
  local added=0

  for rc in "${rc_files[@]}"; do
    if [[ -f "$rc" ]] && ! grep -qF "$INSTALL_DIR" "$rc" 2>/dev/null; then
      echo "" >> "$rc"
      echo "# conduit" >> "$rc"
      echo "$path_line" >> "$rc"
      success "Added $INSTALL_DIR to PATH in $rc"
      added=1
    fi
  done

  if [[ "$added" -eq 0 ]] && [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    warn "Add to your PATH manually:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
}

install_conduit() {
  local target="$INSTALL_DIR/conduit"

  if [[ -f "$target" && "$FORCE" -eq 0 ]]; then
    local current_ver
    current_ver=$("$target" --version 2>/dev/null || echo "unknown")
    warn "conduit already installed at $target ($current_ver)"
    echo "  Use --force to reinstall"
    return
  fi

  if [[ "$USE_RELEASE" -eq 1 ]]; then
    info "Downloading latest release..."
    local tag
    tag="$(curl -fsSL 'https://api.github.com/repos/Team-Deepiri/deepiri-conduit/releases/latest' \
      | grep -m1 '"tag_name":' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
    local url="https://github.com/Team-Deepiri/deepiri-conduit/releases/download/$tag/conduit-x86_64-unknown-linux-gnu"
    curl -fsSL "$url" -o "$target"
    chmod +x "$target"
  else
    local repo_root
    repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

    if [[ ! -f "$repo_root/Cargo.toml" ]]; then
      warn "No Cargo.toml found — cloning repo..."
      repo_root=$(mktemp -d)
      git clone --depth 1 https://github.com/Team-Deepiri/deepiri-conduit.git "$repo_root"
    fi

    info "Building conduit from source (this may take a few minutes)..."
    (cd "$repo_root" && cargo build --release 2>&1 | tail -5)
    cp -f "$repo_root/target/release/conduit" "$target"
    chmod +x "$target"
  fi

  if [[ -f "$target" ]]; then
    local ver
    ver=$("$target" --version 2>/dev/null || echo "unknown")
    success "conduit $ver installed at $target"
  else
    error "Installation failed"
    exit 1
  fi
}

run_doctor() {
  echo ""
  info "Running conduit doctor to verify installation..."
  echo ""
  if "$INSTALL_DIR/conduit" doctor 2>&1; then
    echo ""
    success "All systems ready!"
  else
    echo ""
    warn "Some checks failed — see above for details"
    warn "Common fixes:"
    warn "  - Log out and back in for Docker group changes"
    warn "  - Start Docker: sudo systemctl start docker"
    warn "  - Run: newgrp docker"
  fi
}

show_next_steps() {
  echo ""
  echo -e "${BOLD}═══════════════════════════════════════════════════════${RESET}"
  echo -e "${BOLD}  conduit is ready!${RESET}"
  echo ""
  echo -e "  ${CYAN}conduit doctor${RESET}              check system health"
  echo -e "  ${CYAN}conduit ui${RESET}                 launch web dashboard"
  echo -e "  ${CYAN}conduit up${RESET}                 start a project"
  echo -e "  ${CYAN}conduit exec <svc> bash${RESET}    execute into a container"
  echo -e "  ${CYAN}conduit top${RESET}                resource monitor"
  echo -e "  ${CYAN}conduit connect ssh user@host${RESET}  remote tunnel"
  echo ""
  echo -e "  Documentation: ${BLUE}https://github.com/Team-Deepiri/deepiri-conduit${RESET}"
  echo ""
  echo -e "  ${YELLOW}Tip:${RESET} Restart your shell or run:"
  echo -e "    export PATH=\"$INSTALL_DIR:\$PATH\""
  echo -e "${BOLD}═══════════════════════════════════════════════════════${RESET}"
  echo ""
}

main() {
  banner

  if [[ "$(id -u)" -eq 0 ]]; then
    warn "Running as root — not recommended for Rust/Cargo"
    echo ""
  fi

  install_system_deps
  install_docker
  install_rust
  ensure_dir
  add_to_path
  install_conduit
  run_doctor
  show_next_steps
}

main
