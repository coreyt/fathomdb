#!/usr/bin/env bash
# Developer setup script for fathomdb contributors.
# Installs everything from setup.sh plus: Go, project-local SQLite,
# testing tools (cargo-nextest, pytest, golangci-lint), and linters
# (clippy, rustfmt, ruff).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Source the base setup script for shared functions and user-level deps.
source "$REPO_ROOT/scripts/setup.sh"

GO_VERSION="${GO_VERSION:-1.26.1}"
GO_INSTALL_DIR="${GO_INSTALL_DIR:-$HOME/.local/go}"
GOLANGCI_LINT_VERSION="${GOLANGCI_LINT_VERSION:-1.64.8}"

SQLITE_POLICY_FILE="$REPO_ROOT/tooling/sqlite.env"
if [[ -f "$SQLITE_POLICY_FILE" ]]; then
    # shellcheck disable=SC1090
    source "$SQLITE_POLICY_FILE"
fi
SQLITE_MIN_VERSION="${SQLITE_MIN_VERSION:-3.41.0}"
SQLITE_VERSION="${SQLITE_VERSION:-3.46.0}"
SQLITE_INSTALL_DIR="${SQLITE_INSTALL_DIR:-$REPO_ROOT/.local/sqlite-$SQLITE_VERSION}"

# --- Path management ---

ensure_dev_path_exports() {
    local profile_targets=()
    [[ -f "$HOME/.bashrc" ]] && profile_targets+=("$HOME/.bashrc")
    [[ -f "$HOME/.zshrc" ]] && profile_targets+=("$HOME/.zshrc")
    [[ ${#profile_targets[@]} -eq 0 ]] && profile_targets+=("$HOME/.profile")

    local cargo_line='export PATH="$HOME/.cargo/bin:$PATH"'
    local go_line="export PATH=\"$GO_INSTALL_DIR/bin:\$PATH\""
    local sqlite_line="export PATH=\"$SQLITE_INSTALL_DIR/bin:\$PATH\""
    local gopath_line='export PATH="$(go env GOPATH 2>/dev/null)/bin:$PATH"'

    for profile in "${profile_targets[@]}"; do
        append_once "$profile" "$sqlite_line"
        append_once "$profile" "$cargo_line"
        append_once "$profile" "$go_line"
        append_once "$profile" "$gopath_line"
    done

    export PATH="$SQLITE_INSTALL_DIR/bin:$HOME/.cargo/bin:$GO_INSTALL_DIR/bin:$PATH"
    if have_cmd go; then
        export PATH="$(go env GOPATH)/bin:$PATH"
    fi
}

# --- Additional system packages for development ---

install_linux_dev_packages() {
    if have_cmd apt-get; then
        log "installing dev system packages with apt-get"
        sudo apt-get install -y \
            clang \
            cmake \
            sqlite3 \
            unzip
        return
    fi

    if have_cmd dnf; then
        log "installing dev system packages with dnf"
        sudo dnf install -y \
            clang \
            cmake \
            sqlite \
            unzip
        return
    fi

    if have_cmd yum; then
        log "installing dev system packages with yum"
        sudo yum install -y \
            clang \
            cmake \
            sqlite \
            unzip
        return
    fi
}

install_macos_dev_packages() {
    if have_cmd brew; then
        log "installing dev packages with Homebrew"
        brew install \
            cmake \
            go \
            sqlite
    fi
}

# --- Go ---

detect_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64|amd64) printf 'amd64\n' ;;
        arm64|aarch64) printf 'arm64\n' ;;
        *) die "unsupported architecture: $arch" ;;
    esac
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
            log "upgrading Go to ${GO_VERSION}"
            install_go_linux
        fi
    elif [[ "$os" == "darwin" ]]; then
        log "Go is expected from Homebrew on macOS"
    else
        install_go_linux
    fi
}

# --- Project-local SQLite ---

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

install_project_sqlite() {
    local current_version
    current_version="$(sqlite_installed_version 2>/dev/null || true)"

    if ! sqlite_project_install_needed "$current_version"; then
        log "project-local SQLite already installed: ${current_version}"
        return
    fi

    local archive_url archive_name tmp_dir source_dir jobs
    tmp_dir="$(mktemp -d)"
    archive_url="$(sqlite_download_url "$SQLITE_VERSION")"
    archive_name="$(basename "$archive_url")"
    source_dir="${tmp_dir}/sqlite-src"
    jobs="${JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '2\n')}"

    log "installing project-local SQLite ${SQLITE_VERSION} to ${SQLITE_INSTALL_DIR}"
    curl -fsSL "$archive_url" -o "${tmp_dir}/${archive_name}"

    # Verify checksum when available.
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
        ./configure --prefix="$SQLITE_INSTALL_DIR" --disable-shared --quiet
        make -j "$jobs" --quiet
        make install --quiet
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
        die "project-local sqlite3 version ${project_version} is below minimum ${SQLITE_MIN_VERSION}"
    fi

    if [[ "$project_version" != "$SQLITE_VERSION" ]]; then
        die "project-local sqlite3 version ${project_version} does not match target ${SQLITE_VERSION}"
    fi

    export PATH="$SQLITE_INSTALL_DIR/bin:$PATH"
}

# --- Rust dev components ---

install_rust_dev_components() {
    log "installing Rust dev components: clippy, rustfmt"
    rustup component add clippy rustfmt
}

# --- Cargo tools ---

install_cargo_tools() {
    if cargo nextest --version >/dev/null 2>&1; then
        log "cargo-nextest already installed"
    else
        log "installing cargo-nextest"
        if have_cmd clang && have_cmd clang++; then
            CC=clang CXX=clang++ cargo install cargo-nextest --locked
        else
            cargo install cargo-nextest --locked
        fi
    fi
}

# --- Go linting ---

install_golangci_lint() {
    if have_cmd golangci-lint; then
        local current
        current="$(golangci-lint version 2>&1 | grep -oP '\d+\.\d+\.\d+' | head -1 || true)"
        if [[ "$current" == "$GOLANGCI_LINT_VERSION" ]]; then
            log "golangci-lint ${GOLANGCI_LINT_VERSION} already installed"
            return
        fi
    fi

    log "installing golangci-lint ${GOLANGCI_LINT_VERSION}"
    curl -sSfL https://raw.githubusercontent.com/golangci/golangci-lint/v${GOLANGCI_LINT_VERSION}/install.sh \
        | sh -s -- -b "$(go env GOPATH)/bin" "v${GOLANGCI_LINT_VERSION}"
}

# --- Python dev tools ---

install_python_dev_tools() {
    log "installing Python dev tools: pytest, pytest-timeout, ruff"
    python3 -m pip install --upgrade pytest pytest-timeout ruff
}

# --- Verification ---

verify_dev_setup() {
    log "verifying dev installation"
    local ok=true

    if ! have_cmd rustc; then
        warn "rustc not found"; ok=false
    fi
    if ! have_cmd cargo; then
        warn "cargo not found"; ok=false
    fi
    if ! rustup component list --installed 2>/dev/null | grep -q clippy; then
        warn "clippy not installed"; ok=false
    fi
    if ! rustup component list --installed 2>/dev/null | grep -q rustfmt; then
        warn "rustfmt not installed"; ok=false
    fi
    if ! cargo nextest --version >/dev/null 2>&1; then
        warn "cargo-nextest not found"; ok=false
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
    if ! have_cmd pytest; then
        warn "pytest not found"; ok=false
    fi
    if ! have_cmd ruff; then
        warn "ruff not found"; ok=false
    fi
    if ! have_cmd go; then
        warn "go not found"; ok=false
    fi
    if ! have_cmd golangci-lint; then
        warn "golangci-lint not found"; ok=false
    fi
    if ! have_cmd pkg-config; then
        warn "pkg-config not found"; ok=false
    fi
    if [[ ! -x "$SQLITE_INSTALL_DIR/bin/sqlite3" ]]; then
        warn "project-local sqlite3 not found at $SQLITE_INSTALL_DIR/bin/sqlite3"; ok=false
    fi

    if [[ "$ok" == "false" ]]; then
        die "some required tools are missing — see warnings above"
    fi
}

print_dev_summary() {
    printf '\n'
    log "developer environment setup complete"
    printf '  rustc:          %s\n' "$(rustc --version)"
    printf '  cargo:          %s\n' "$(cargo --version)"
    printf '  cargo-nextest:  %s\n' "$(cargo nextest --version 2>&1 | head -1)"
    printf '  go:             %s\n' "$(go version)"
    printf '  golangci-lint:  %s\n' "$(golangci-lint version 2>&1 | head -1)"
    printf '  python3:        %s\n' "$(python3 --version)"
    printf '  maturin:        %s\n' "$(maturin --version)"
    printf '  pytest:         %s\n' "$(pytest --version 2>&1 | head -1)"
    printf '  ruff:           %s\n' "$(ruff version)"
    printf '  sqlite3:        %s\n' "$("$SQLITE_INSTALL_DIR/bin/sqlite3" --version)"
    printf '\n'
    printf 'Suggested next commands:\n'
    printf '  cargo build --workspace\n'
    printf '  cargo nextest run --workspace\n'
    printf '  cd python && pip install -e . --no-build-isolation\n'
    printf '  (cd go/fathom-integrity && go test ./...)\n'
}

# --- Main ---

dev_main() {
    local os
    os="$(detect_os)"

    log "starting developer setup for $os..."

    # Base user-level setup (system packages, Rust, Python, maturin).
    case "$os" in
        darwin) install_macos_packages ;;
        linux) install_linux_packages ;;
    esac
    install_rust
    ensure_cargo_path
    install_python_build_tools

    # Dev-only system packages.
    case "$os" in
        darwin) install_macos_dev_packages ;;
        linux) install_linux_dev_packages ;;
    esac

    # Dev tools.
    install_rust_dev_components
    install_go
    ensure_dev_path_exports
    install_project_sqlite
    verify_sqlite
    ensure_dev_path_exports
    install_cargo_tools
    install_golangci_lint
    install_python_dev_tools

    verify_dev_setup
    print_dev_summary
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    dev_main "$@"
fi
