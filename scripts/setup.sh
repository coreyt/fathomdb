#!/usr/bin/env bash
# Setup script for users who want to build and use fathomdb.
# For developer tooling (testing, linting, Go, etc.), use setup_dev.sh instead.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"

# --- Utility functions ---

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

detect_os() {
    local kernel
    kernel="$(uname -s)"
    case "$kernel" in
        Darwin) printf 'darwin\n' ;;
        Linux) printf 'linux\n' ;;
        *) die "unsupported operating system: $kernel" ;;
    esac
}

append_once() {
    local file="$1"
    local line="$2"
    [[ ! -f "$file" ]] && touch "$file"
    if ! grep -Fqx "$line" "$file"; then
        printf '\n%s\n' "$line" >>"$file"
    fi
}

ensure_cargo_path() {
    local cargo_line='export PATH="$HOME/.cargo/bin:$PATH"'
    local profile_targets=()
    [[ -f "$HOME/.bashrc" ]] && profile_targets+=("$HOME/.bashrc")
    [[ -f "$HOME/.zshrc" ]] && profile_targets+=("$HOME/.zshrc")
    [[ ${#profile_targets[@]} -eq 0 ]] && profile_targets+=("$HOME/.profile")

    for profile in "${profile_targets[@]}"; do
        append_once "$profile" "$cargo_line"
    done

    export PATH="$HOME/.cargo/bin:$PATH"
}

# --- System packages ---

install_linux_packages() {
    if have_cmd apt-get; then
        log "installing system packages with apt-get"
        sudo apt-get update
        sudo apt-get install -y \
            build-essential \
            ca-certificates \
            curl \
            git \
            pkg-config \
            python3 \
            python3-dev \
            python3-pip \
            python3-venv
        return
    fi

    if have_cmd dnf; then
        log "installing system packages with dnf"
        sudo dnf install -y \
            ca-certificates \
            curl \
            gcc \
            gcc-c++ \
            git \
            make \
            pkgconf-pkg-config \
            python3 \
            python3-devel \
            python3-pip
        return
    fi

    if have_cmd yum; then
        log "installing system packages with yum"
        sudo yum install -y \
            ca-certificates \
            curl \
            gcc \
            gcc-c++ \
            git \
            make \
            pkgconfig \
            python3 \
            python3-devel \
            python3-pip
        return
    fi

    warn "no supported package manager found; skipping system package installation"
}

install_macos_packages() {
    if ! have_cmd brew; then
        die "Homebrew is required on macOS. Install it first, then rerun this script."
    fi

    log "installing system packages with Homebrew"
    brew update
    brew install \
        ca-certificates \
        curl \
        git \
        pkg-config \
        python@3.11
}

# --- Rust ---

install_rust() {
    if ! have_cmd rustup; then
        log "installing rustup"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile default
    fi

    export PATH="$HOME/.cargo/bin:$PATH"

    log "installing Rust toolchain: $RUST_TOOLCHAIN"
    rustup toolchain install "$RUST_TOOLCHAIN"
    rustup default "$RUST_TOOLCHAIN"
}

# --- Python build tools ---

install_python_build_tools() {
    log "installing maturin (Python-Rust build tool)"
    python3 -m pip install --upgrade pip maturin
}

# --- Verification ---

verify_setup() {
    log "verifying installation"
    local ok=true

    if ! have_cmd rustc; then
        warn "rustc not found"; ok=false
    fi
    if ! have_cmd cargo; then
        warn "cargo not found"; ok=false
    fi
    if ! have_cmd python3; then
        warn "python3 not found"; ok=false
    fi
    if ! python3 -c "import sys; assert sys.version_info >= (3, 11)" 2>/dev/null; then
        warn "Python >= 3.11 is required"; ok=false
    fi
    if ! have_cmd maturin; then
        warn "maturin not found"; ok=false
    fi
    if ! have_cmd pkg-config; then
        warn "pkg-config not found"; ok=false
    fi

    if [[ "$ok" == "false" ]]; then
        die "some required tools are missing — see warnings above"
    fi
}

print_summary() {
    printf '\n'
    log "setup complete"
    printf '  rustc:    %s\n' "$(rustc --version)"
    printf '  cargo:    %s\n' "$(cargo --version)"
    printf '  python3:  %s\n' "$(python3 --version)"
    printf '  maturin:  %s\n' "$(maturin --version)"
    printf '\n'
    printf 'To build the Python package:\n'
    printf '  cd python && pip install -e . --no-build-isolation\n'
}

# --- Main ---

setup_main() {
    local os
    os="$(detect_os)"

    log "starting setup for $os..."

    case "$os" in
        darwin) install_macos_packages ;;
        linux) install_linux_packages ;;
    esac

    install_rust
    ensure_cargo_path
    install_python_build_tools
    verify_setup
    print_summary
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    setup_main "$@"
fi
