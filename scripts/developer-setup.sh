#!/usr/bin/env bash

set -euo pipefail

RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
GO_VERSION="${GO_VERSION:-1.24.1}"
GO_INSTALL_DIR="${GO_INSTALL_DIR:-$HOME/.local/go}"
PROFILE_TARGETS=()

if [[ -f "$HOME/.bashrc" ]]; then
  PROFILE_TARGETS+=("$HOME/.bashrc")
fi

if [[ -f "$HOME/.zshrc" ]]; then
  PROFILE_TARGETS+=("$HOME/.zshrc")
fi

if [[ ${#PROFILE_TARGETS[@]} -eq 0 ]]; then
  PROFILE_TARGETS+=("$HOME/.profile")
fi

log() {
  printf '[setup] %s\n' "$*"
}

warn() {
  printf '[setup] warning: %s\n' "$*" >&2
}

die() {
  printf '[setup] error: %s\n' "$*" >&2
  exit 1
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

append_once() {
  local file="$1"
  local line="$2"

  if [[ ! -f "$file" ]]; then
    touch "$file"
  fi

  if ! grep -Fqx "$line" "$file"; then
    printf '\n%s\n' "$line" >>"$file"
  fi
}

ensure_path_exports() {
  local cargo_line='export PATH="$HOME/.cargo/bin:$PATH"'
  local go_line
  go_line="export PATH=\"$GO_INSTALL_DIR/bin:\$PATH\""

  for profile in "${PROFILE_TARGETS[@]}"; do
    append_once "$profile" "$cargo_line"
    append_once "$profile" "$go_line"
  done

  export PATH="$HOME/.cargo/bin:$GO_INSTALL_DIR/bin:$PATH"
}

detect_os() {
  local kernel
  kernel="$(uname -s)"
  case "$kernel" in
    Darwin) printf 'darwin\n' ;;
    Linux) printf 'linux\n' ;;
    *) die "unsupported operating system: $kernel" ;;
  esac
}

detect_arch() {
  local arch
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) printf 'amd64\n' ;;
    arm64|aarch64) printf 'arm64\n' ;;
    *) die "unsupported architecture: $arch" ;;
  esac
}

run_with_privilege() {
  if have_cmd sudo; then
    sudo "$@"
  else
    "$@"
  fi
}

install_macos_packages() {
  if ! have_cmd brew; then
    die "Homebrew is required on macOS. Install it first, then rerun this script."
  fi

  log "installing macOS packages with Homebrew"
  brew update
  brew install \
    bash \
    ca-certificates \
    coreutils \
    curl \
    git \
    go \
    pkg-config \
    sqlite
}

install_linux_packages() {
  if have_cmd apt-get; then
    log "installing Linux packages with apt-get"
    run_with_privilege apt-get update
    run_with_privilege apt-get install -y \
      build-essential \
      ca-certificates \
      clang \
      cmake \
      curl \
      git \
      pkg-config \
      sqlite3 \
      unzip
    return
  fi

  if have_cmd dnf; then
    log "installing Linux packages with dnf"
    run_with_privilege dnf install -y \
      clang \
      cmake \
      curl \
      gcc \
      gcc-c++ \
      git \
      make \
      pkgconf-pkg-config \
      sqlite \
      unzip
    return
  fi

  if have_cmd yum; then
    log "installing Linux packages with yum"
    run_with_privilege yum install -y \
      clang \
      cmake \
      curl \
      gcc \
      gcc-c++ \
      git \
      make \
      pkgconfig \
      sqlite \
      unzip
    return
  fi

  warn "no supported package manager found; skipping system package installation"
}

install_rust() {
  if have_cmd rustup; then
    log "rustup already installed"
  else
    log "installing rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile default
  fi

  export PATH="$HOME/.cargo/bin:$PATH"

  log "installing Rust toolchain: $RUST_TOOLCHAIN"
  rustup toolchain install "$RUST_TOOLCHAIN"
  rustup default "$RUST_TOOLCHAIN"
  rustup component add rustfmt clippy
}

install_go_linux() {
  local os arch archive url tmp_dir
  os='linux'
  arch="$(detect_arch)"
  archive="go${GO_VERSION}.${os}-${arch}.tar.gz"
  url="https://go.dev/dl/${archive}"
  tmp_dir="$(mktemp -d)"

  log "installing Go ${GO_VERSION} to ${GO_INSTALL_DIR}"
  curl -fsSL "$url" -o "${tmp_dir}/${archive}"
  rm -rf "$GO_INSTALL_DIR"
  mkdir -p "$GO_INSTALL_DIR"
  tar -C "$GO_INSTALL_DIR" --strip-components=1 -xzf "${tmp_dir}/${archive}"
  rm -rf "$tmp_dir"
}

install_go() {
  local os
  os="$(detect_os)"

  if have_cmd go; then
    local current_version
    current_version="$(go version | awk '{print $3}' | sed 's/^go//')"
    log "Go already installed: ${current_version}"

    if [[ "$os" == "linux" && "$current_version" != "$GO_VERSION" ]]; then
      log "upgrading Go to ${GO_VERSION} in ${GO_INSTALL_DIR}"
      install_go_linux
    fi
  elif [[ "$os" == "darwin" ]]; then
    log "Go is expected to come from Homebrew on macOS"
  else
    install_go_linux
  fi

  if [[ "$os" == "darwin" ]] && ! have_cmd go; then
    die "Go is still unavailable after Homebrew install"
  fi
}

install_cargo_tools() {
  local os
  os="$(detect_os)"

  if ! have_cmd cargo; then
    die "cargo is required after rustup installation"
  fi

  if cargo nextest --version >/dev/null 2>&1; then
    log "cargo-nextest already installed"
  else
    log "installing cargo-nextest"
    if [[ "$os" == "linux" ]] && have_cmd clang && have_cmd clang++; then
      log "using clang/clang++ for cargo-nextest installation"
      CC=clang CXX=clang++ cargo install cargo-nextest --locked
    else
      cargo install cargo-nextest --locked
    fi
  fi
}

print_summary() {
  log "developer environment setup complete"
  printf '\n'
  printf 'Installed/verified:\n'
  printf '  rustup: %s\n' "$(rustup --version | head -n 1)"
  printf '  rustc: %s\n' "$(rustc --version)"
  printf '  cargo: %s\n' "$(cargo --version)"
  printf '  cargo-nextest: %s\n' "$(cargo nextest --version)"
  printf '  go: %s\n' "$(go version)"
  printf '\n'
  printf 'Suggested next commands:\n'
  printf '  cargo build --workspace\n'
  printf '  cargo nextest run --workspace\n'
  printf '  (cd go/fathom-integrity && go test ./...)\n'
}

main() {
  local os
  os="$(detect_os)"

  ensure_path_exports

  case "$os" in
    darwin) install_macos_packages ;;
    linux) install_linux_packages ;;
  esac

  install_rust
  ensure_path_exports
  install_go
  ensure_path_exports
  install_cargo_tools
  print_summary
}

main "$@"
