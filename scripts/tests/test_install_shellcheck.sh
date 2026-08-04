#!/usr/bin/env bash
# Regression tests for the pinned ShellCheck installer. These use only shims:
# no test downloads a release or relaxes the production SHA verification.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/shellcheck-version.sh
. "$REPO_ROOT/scripts/lib/shellcheck-version.sh"

FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

# This is the Linux/x86_64 release digest in the production installer. It is
# intentionally repeated as a test oracle: changing a platform release is a
# deliberate pin change and must update this fixture rather than bypass SHA.
LINUX_X86_64_SHA='8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198'

fail() { printf 'FAIL  %s\n' "$1" >&2; exit 1; }
pass() { printf 'PASS  %s\n' "$1"; }

make_shims() {
  local dir="$1"
  mkdir -p "$dir"
  cat >"$dir/curl" <<'CURL'
#!/usr/bin/env bash
printf 'curl invoked\n' >>"$SHELLCHECK_TEST_MARKERS"
printf 'offline test transport\n' >&2
exit 28
CURL
  cat >"$dir/sha256sum" <<'SHA'
#!/usr/bin/env bash
if [ "$1" != '-c' ] || [ "$2" != '-' ]; then
  printf 'unexpected sha256sum invocation: %s\n' "$*" >&2
  exit 2
fi
input="$(cat)"
printf 'sha256sum invoked: %s\n' "$input" >>"$SHELLCHECK_TEST_MARKERS"
case "${SHELLCHECK_TEST_SHA_MODE:-pass}" in
  pass)
    printf '%s\n' "$input" | grep -Fq "$SHELLCHECK_TEST_SHA" || exit 1
    exit 0
    ;;
  fail) exit 1 ;;
  *) exit 2 ;;
esac
SHA
  cat >"$dir/tar" <<'TAR'
#!/usr/bin/env bash
dest=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    -C) shift; dest="$1" ;;
  esac
  shift
done
[ -n "$dest" ] || exit 2
mkdir -p "$dest/shellcheck-v$SHELLCHECK_VERSION"
cat >"$dest/shellcheck-v$SHELLCHECK_VERSION/shellcheck" <<BIN
#!/usr/bin/env bash
printf 'ShellCheck - shell script analysis tool\nversion: $SHELLCHECK_VERSION\n'
BIN
chmod +x "$dest/shellcheck-v$SHELLCHECK_VERSION/shellcheck"
printf 'tar invoked\n' >>"$SHELLCHECK_TEST_MARKERS"
TAR
  chmod +x "$dir/curl" "$dir/sha256sum" "$dir/tar"
}

run_installer() {
  local home="$1" cache="$2" markers="$3" mode="$4"
  set +e
  OUT="$(HOME="$home" SHELLCHECK_CACHE_DIR="$cache" \
    SHELLCHECK_TEST_MARKERS="$markers" SHELLCHECK_TEST_SHA="$LINUX_X86_64_SHA" \
    SHELLCHECK_TEST_SHA_MODE="$mode" SHELLCHECK_VERSION="$SHELLCHECK_VERSION" \
    PATH="$FIX/bin:/usr/bin:/bin" bash "$REPO_ROOT/scripts/install-shellcheck.sh" 2>&1)"
  RC=$?
  set -e
}

make_shims "$FIX/bin"

# Arm A: a cache hit does not touch the network, but it STILL proves the cache
# archive against the production SHA before extraction and uses the pinned
# installed binary afterward.
HOME_A="$FIX/home-a"
CACHE_A="$FIX/cache-a"
MARKERS_A="$FIX/markers-a"
ARCHIVE_A="$CACHE_A/v$SHELLCHECK_VERSION/linux.x86_64-$LINUX_X86_64_SHA/shellcheck.tar.xz"
mkdir -p "$(dirname "$ARCHIVE_A")" "$HOME_A"
printf 'cached fixture; the SHA shim asserts the production digest is supplied\n' >"$ARCHIVE_A"
run_installer "$HOME_A" "$CACHE_A" "$MARKERS_A" pass
printf -- '---- cache-hit output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -eq 0 ] || fail "cache hit did not install the pinned ShellCheck: $OUT"
grep -Fq 'Using verified shellcheck archive cache' <<<"$OUT" \
  || fail 'cache hit was not reported'
if [ -e "$MARKERS_A" ] && grep -Fq 'curl invoked' "$MARKERS_A"; then
  fail 'cache hit invoked curl instead of using the local archive'
fi
grep -Fq 'sha256sum invoked' "$MARKERS_A" \
  || fail 'cache hit did not re-verify the archive SHA-256'
[ -x "$HOME_A/.local/bin/shellcheck" ] || fail 'cache hit did not install shellcheck'
installed_version="$(HOME="$HOME_A" "$HOME_A/.local/bin/shellcheck" --version | sed -n 's/^version: //p')"
[ "$installed_version" = "$SHELLCHECK_VERSION" ] || fail "cache hit installed $installed_version, not $SHELLCHECK_VERSION"
pass 'cached archive avoids curl but is SHA-256-verified and installs the pin'

# Arm B: an offline cache miss is a loud failure. It must not become a skipped
# shell gate or a fake successful install.
HOME_B="$FIX/home-b"
CACHE_B="$FIX/cache-b"
MARKERS_B="$FIX/markers-b"
mkdir -p "$HOME_B"
run_installer "$HOME_B" "$CACHE_B" "$MARKERS_B" pass
printf -- '---- offline-miss output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail 'offline cache miss exited 0'
grep -Fq 'download failed' <<<"$OUT" || fail 'offline cache miss did not name its download failure'
grep -Fq 'curl invoked' "$MARKERS_B" || fail 'offline cache miss did not attempt the bounded download'
[ ! -e "$HOME_B/.local/bin/shellcheck" ] || fail 'offline cache miss left an installed binary behind'
pass 'offline cache miss fails loudly rather than weakening the shell gate'

# Arm C: a corrupt cache is NOT silently redownloaded or accepted. Rechecking
# the digest on every hit is what makes the action cache an acceleration, not a
# trust boundary.
HOME_C="$FIX/home-c"
CACHE_C="$FIX/cache-c"
MARKERS_C="$FIX/markers-c"
ARCHIVE_C="$CACHE_C/v$SHELLCHECK_VERSION/linux.x86_64-$LINUX_X86_64_SHA/shellcheck.tar.xz"
mkdir -p "$(dirname "$ARCHIVE_C")" "$HOME_C"
printf 'corrupt cache fixture\n' >"$ARCHIVE_C"
run_installer "$HOME_C" "$CACHE_C" "$MARKERS_C" fail
printf -- '---- corrupt-cache output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail 'corrupt cache exited 0'
grep -Fq 'SHA-256 check' <<<"$OUT" || fail 'corrupt cache did not name SHA-256 verification'
if [ -e "$MARKERS_C" ] && grep -Fq 'curl invoked' "$MARKERS_C"; then
  fail 'corrupt cache redownloaded instead of failing its checksum loudly'
fi
grep -Fq 'sha256sum invoked' "$MARKERS_C" \
  || fail 'corrupt cache did not attempt SHA-256 verification'
pass 'corrupt cached archive fails its SHA-256 check without network fallback'

printf 'ShellCheck installer cache tests passed\n'
