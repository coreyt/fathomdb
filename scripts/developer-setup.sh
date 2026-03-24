#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
GO_VERSION="${GO_VERSION:-1.24.1}"
GO_INSTALL_DIR="${GO_INSTALL_DIR:-$HOME/.local/go}"

# Security fix H-6: Hardcode the policy file path to the repository's own
# tooling/sqlite.env. Previously SQLITE_POLICY_FILE could be overridden via
# environment variable, allowing an attacker to source arbitrary bash code.
SQLITE_POLICY_FILE="$REPO_ROOT/tooling/sqlite.env"

if [[ -f "$SQLITE_POLICY_FILE" ]]; then
  # shellcheck disable=SC1090
  source "$SQLITE_POLICY_FILE"
fi

SQLITE_MIN_VERSION="${SQLITE_MIN_VERSION:-3.41.0}"
SQLITE_VERSION="${SQLITE_VERSION:-3.46.0}"
SQLITE_INSTALL_DIR="${SQLITE_INSTALL_DIR:-$REPO_ROOT/.local/sqlite-$SQLITE_VERSION}"
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
  local sqlite_line
  go_line="export PATH=\"$GO_INSTALL_DIR/bin:\$PATH\""
  sqlite_line="export PATH=\"$SQLITE_INSTALL_DIR/bin:\$PATH\""

  for profile in "${PROFILE_TARGETS[@]}"; do
    append_once "$profile" "$sqlite_line"
    append_once "$profile" "$cargo_line"
    append_once "$profile" "$go_line"
  done

  export PATH="$SQLITE_INSTALL_DIR/bin:$HOME/.cargo/bin:$GO_INSTALL_DIR/bin:$PATH"
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

sqlite_numeric_version() {
  local version="$1"
  local major minor patch
  IFS=. read -r major minor patch <<<"$version"
  printf '%d%02d%02d00\n' "$major" "$minor" "$patch"
}

sqlite_version_at_least() {
  local actual="$1"
  local minimum="$2"
  [[ "$(sqlite_numeric_version "$actual")" -ge "$(sqlite_numeric_version "$minimum")" ]]
}

sqlite_version_supported() {
  local version="$1"
  sqlite_version_at_least "$version" "$SQLITE_MIN_VERSION"
}

sqlite_project_install_needed() {
  local installed_version="${1:-}"
  [[ -z "$installed_version" || "$installed_version" != "$SQLITE_VERSION" ]]
}

sqlite_release_year() {
  local version="$1"
  case "$version" in
    3.41.*|3.42.*|3.43.*|3.44.*) printf '2023\n' ;;
    3.45.*|3.46.*) printf '2024\n' ;;
    *) die "unsupported SQLite release year lookup for version: $version" ;;
  esac
}

sqlite_download_url() {
  local version="${1:-$SQLITE_VERSION}"
  printf 'https://sqlite.org/%s/sqlite-autoconf-%s.tar.gz\n' \
    "$(sqlite_release_year "$version")" \
    "$(sqlite_numeric_version "$version")"
}

sqlite_installed_version() {
  local sqlite_bin="${1:-$SQLITE_INSTALL_DIR/bin/sqlite3}"
  if [[ ! -x "$sqlite_bin" ]]; then
    return 1
  fi

  "$sqlite_bin" --version | awk '{print $1}'
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

# Security fix M-7: Verify downloaded archive integrity with SHA-256 checksums
# before extraction. Prevents MITM or mirror-compromise attacks.
verify_sha256() {
  local file="$1"
  local expected="$2"
  local actual
  actual="$(sha256sum "$file" | awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    die "SHA-256 mismatch for $(basename "$file"): expected $expected, got $actual"
  fi
  log "SHA-256 verified for $(basename "$file")"
}

# Known SHA-256 checksums for Go releases.
go_sha256() {
  local version="$1" arch="$2"
  case "${version}-${arch}" in
    1.24.1-amd64) printf 'cb2396bae64183cdccf75c33a271e5e3a8bce8b3e53e52af4e5f64c39aef8596\n' ;;
    1.24.1-arm64) printf '8e298f34519c82b773077e5e18a098e6e7da9768a3b68ab0cb3e295476a3dc3f\n' ;;
    *) warn "no known SHA-256 for Go ${version}/${arch}; skipping verification"; return 1 ;;
  esac
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
  # Security fix M-7: Verify download integrity before extracting.
  local expected_sha
  if expected_sha="$(go_sha256 "$GO_VERSION" "$arch")"; then
    verify_sha256 "${tmp_dir}/${archive}" "$expected_sha"
  fi
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

install_project_sqlite() {
  local archive_url archive_name tmp_dir source_dir jobs current_version
  current_version="$(sqlite_installed_version 2>/dev/null || true)"

  if ! sqlite_project_install_needed "$current_version"; then
    log "project-local SQLite already installed: ${current_version}"
    return
  fi

  tmp_dir="$(mktemp -d)"
  archive_url="$(sqlite_download_url "$SQLITE_VERSION")"
  archive_name="$(basename "$archive_url")"
  source_dir="${tmp_dir}/sqlite-src"
  jobs="${JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '2\n')}"

  log "installing project-local SQLite ${SQLITE_VERSION} to ${SQLITE_INSTALL_DIR}"
  curl -fsSL "$archive_url" -o "${tmp_dir}/${archive_name}"
  # Security fix M-7: Verify SQLite archive checksum when available.
  local sqlite_sha_url
  sqlite_sha_url="$(printf 'https://sqlite.org/%s/sqlite-autoconf-%s.tar.gz.sha256' \
    "$(sqlite_release_year "$SQLITE_VERSION")" \
    "$(sqlite_numeric_version "$SQLITE_VERSION")")"
  if curl -fsSL "$sqlite_sha_url" -o "${tmp_dir}/sha256.txt" 2>/dev/null; then
    local expected_sha
    expected_sha="$(awk '{print $1}' "${tmp_dir}/sha256.txt")"
    verify_sha256 "${tmp_dir}/${archive_name}" "$expected_sha"
  else
    warn "could not fetch SHA-256 checksum for SQLite ${SQLITE_VERSION}; skipping verification"
  fi
  mkdir -p "$source_dir"
  tar -C "$source_dir" --strip-components=1 -xzf "${tmp_dir}/${archive_name}"

  rm -rf "$SQLITE_INSTALL_DIR"
  mkdir -p "$SQLITE_INSTALL_DIR"

  (
    cd "$source_dir"
    ./configure --prefix="$SQLITE_INSTALL_DIR"
    make -j "$jobs"
    make install
  )

  rm -rf "$tmp_dir"
}

verify_sqlite() {
  local project_sqlite_bin project_version
  project_sqlite_bin="$SQLITE_INSTALL_DIR/bin/sqlite3"

  if [[ ! -x "$project_sqlite_bin" ]]; then
    die "project-local sqlite3 not found at $project_sqlite_bin"
  fi

  project_version="$(sqlite_installed_version "$project_sqlite_bin")"
  if ! sqlite_version_supported "$project_version"; then
    die "project-local sqlite3 version ${project_version} is below the supported minimum ${SQLITE_MIN_VERSION}"
  fi

  if [[ "$project_version" != "$SQLITE_VERSION" ]]; then
    die "project-local sqlite3 version ${project_version} does not match the development target ${SQLITE_VERSION}"
  fi

  export PATH="$SQLITE_INSTALL_DIR/bin:$PATH"
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
  printf '  sqlite3: %s\n' "$(sqlite3 --version)"
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
  install_project_sqlite
  ensure_path_exports
  verify_sqlite
  install_cargo_tools
  print_summary
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
