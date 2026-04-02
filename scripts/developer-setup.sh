#!/usr/bin/env bash

set -euo pipefail

# --- 1. UTILITY FUNCTIONS (Defined first so they are available) ---

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

have_cmd() {
    command -v "$1" >/dev/null 2>&1
}

# --- 2. CONFIGURATION ---

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
GO_VERSION="${GO_VERSION:-1.26.1}"
GO_INSTALL_DIR="${GO_INSTALL_DIR:-$HOME/.local/go}"
SQLITE_POLICY_FILE="$REPO_ROOT/tooling/sqlite.env"

if [[ -f "$SQLITE_POLICY_FILE" ]]; then
    source "$SQLITE_POLICY_FILE"
fi

SQLITE_MIN_VERSION="${SQLITE_MIN_VERSION:-3.41.0}"
SQLITE_VERSION="${SQLITE_VERSION:-3.46.0}"
SQLITE_INSTALL_DIR="${SQLITE_INSTALL_DIR:-$REPO_ROOT/.local/sqlite-$SQLITE_VERSION}"

# --- 3. INSTALLATION LOGIC ---

append_once() {
    local file="$1"
    local line="$2"
    [[ ! -f "$file" ]] && touch "$file"
    if ! grep -Fqx "$line" "$file"; then
        printf '\n%s\n' "$line" >>"$file"
    fi
}

ensure_path_exports() {
    local profile_targets=()
    [[ -f "$HOME/.bashrc" ]] && profile_targets+=("$HOME/.bashrc")
    [[ -f "$HOME/.zshrc" ]] && profile_targets+=("$HOME/.zshrc")
    [[ ${#profile_targets[@]} -eq 0 ]] && profile_targets+=("$HOME/.profile")

    local cargo_line='export PATH="$HOME/.cargo/bin:$PATH"'
    local go_line="export PATH=\"$GO_INSTALL_DIR/bin:\$PATH\""
    local sqlite_line="export PATH=\"$SQLITE_INSTALL_DIR/bin:\$PATH\""

    for profile in "${profile_targets[@]}"; do
        append_once "$profile" "$sqlite_line"
        append_once "$profile" "$cargo_line"
        append_once "$profile" "$go_line"
    done

    export PATH="$SQLITE_INSTALL_DIR/bin:$HOME/.cargo/bin:$GO_INSTALL_DIR/bin:$PATH"
}

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

go_sha256() {
    local version="$1" arch="$2"
    case "${version}-${arch}" in
        1.26.1-amd64) printf '5e8f4951119c82b773077e5e18a098e6e7da9768a3b68ab0cb3e295476a3dc3f\n' ;;
        1.26.1-arm64) printf '8df5750ffc0281017fb6070fba450f5d22b600a02081dceef47966ffaf36a3af\n' ;;
        *) warn "no known SHA-256 for Go ${version}/${arch}; skipping verification"; return 1 ;;
    esac
}

install_go_linux() {
    local arch archive url tmp_dir
    arch="$(detect_arch)"
    archive="go${GO_VERSION}.linux-${arch}.tar.gz"
    url="https://go.dev/dl/${archive}"
    tmp_dir="$(mktemp -d)"

    log "installing Go ${GO_VERSION} to ${GO_INSTALL_DIR}"
    curl -fsSL "$url" -o "${tmp_dir}/${archive}"
    
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
            install_go_linux
        fi
    else
        install_go_linux
    fi
}

install_rust() {
    if ! have_cmd rustup; then
        log "installing rustup"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile default
    fi
    export PATH="$HOME/.cargo/bin:$PATH"
    rustup toolchain install "$RUST_TOOLCHAIN"
    rustup default "$RUST_TOOLCHAIN"
}

# --- 4. MAIN EXECUTION ---

main() {
    local os
    os="$(detect_os)"
    
    log "Starting setup for $os..."
    
    # Ensure system dependencies are present
    if [[ "$os" == "linux" ]]; then
        sudo apt-get update && sudo apt-get install -y build-essential curl git
    fi

    install_rust
    install_go
    ensure_path_exports

    log "Developer environment setup complete."
    log "Go version: $(go version)"
    log "Rust version: $(rustc --version)"
    log "Please run: source ~/.bashrc"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
