#!/usr/bin/env bash
#
# render-mermaid.sh — render Mermaid diagrams to SVG/PNG/PDF.
#
# Wrapper around @mermaid-js/mermaid-cli (mmdc), tuned for agentic use the same
# way dev/agent-tools/ledgerwatch and ledgerwrite are: the load-bearing signal
# is the EXIT CODE, not prose. A caller branches on $? without capturing stdout;
# stdout carries only payload (the output path[s]); stderr carries only an
# advisory when there is one. That keeps an agent's context — and token use —
# small.
#
# It also, deliberately:
#   * NEVER auto-installs. Dependencies are a ~170MB Chromium download, so a
#     missing-deps run exits EX_UNAVAILABLE (69) with an explicit instruction to
#     get user approval and run `install`. An agent surfaces that to the human.
#   * has a cheap, no-launch `check` (no download, no browser start) whose whole
#     answer is its exit code.
#   * supplies a --no-sandbox Puppeteer config, required where unprivileged user
#     namespaces are disabled (Ubuntu 23.10+ / AppArmor), else Chromium dies with
#     "No usable sandbox!".
#
# Commands:
#   check                 Report install status via exit code. Silent on stdout.
#   install               Install mermaid-cli + Chromium (~170MB download).
#   render <args…>        Render (default; the word "render" is optional).
#
# Render usage:
#   render-mermaid.sh -i diagram.mmd -o diagram.svg   # explicit; silent on ok
#   render-mermaid.sh diagram.mmd                      # -> diagram.svg; prints path
#   render-mermaid.sh -i diagram.mmd -o out.png -t dark -b transparent
#   render-mermaid.sh -i README.md -o README.md        # rewrites ```mermaid blocks
#   render-mermaid.sh --all docs/                      # every *.mmd -> *.svg; prints paths
#
# Extra flags pass through to mmdc (mmdc --help): -t/--theme, -b/--backgroundColor,
# -w/--width, -H/--height, -s/--scale, -f/--pdfFit.
#
# Exit status (BSD sysexits.h — standard, so callers branch on $? alone):
#   0   EX_OK           success
#   64  EX_USAGE        bad invocation / no *.mmd found / nothing to do
#   66  EX_NOINPUT      input mermaid file does not exist
#   69  EX_UNAVAILABLE  dependencies not installed → get approval, run `install`
#   70  EX_SOFTWARE     mmdc/render failed (bad diagram, write error)
#   73  EX_CANTCREAT    output directory could not be created
set -euo pipefail

readonly EX_OK=0 EX_USAGE=64 EX_NOINPUT=66 EX_UNAVAILABLE=69 EX_SOFTWARE=70 EX_CANTCREAT=73

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PUPPETEER_CFG="$SCRIPT_DIR/puppeteer.json"
MMDC="$SCRIPT_DIR/node_modules/.bin/mmdc"
SELF="$SCRIPT_DIR/render-mermaid.sh"

# --- dependency status -------------------------------------------------------
# is_installed: cheap and silent. Confirms the mmdc binary exists AND Puppeteer
# can resolve an on-disk Chromium — WITHOUT launching a browser or touching the
# network. Its result is a return code only; no output. ~one short node start.
is_installed() {
  [[ -x "$MMDC" ]] || return 1
  ( cd "$SCRIPT_DIR" && node -e '
      try { var p = require("puppeteer"); } catch (e) { process.exit(1); }
      Promise.resolve(p.executablePath())
        .then(function (x) { process.exit(require("fs").existsSync(String(x)) ? 0 : 1); })
        .catch(function () { process.exit(1); });
    ' ) >/dev/null 2>&1
}

# The one advisory an agent needs when deps are missing. stderr only, terse.
not_installed_notice() {
  cat >&2 <<EOF
render-mermaid: NOT INSTALLED — mermaid-cli + headless Chromium (~170MB download).
Agent: do NOT install unattended. Ask the user for approval, then run:
    $SELF install
EOF
}

require_installed() {
  is_installed && return 0
  not_installed_notice
  exit "$EX_UNAVAILABLE"
}

# --- commands ----------------------------------------------------------------
# check: the answer is the exit code (0 ready / 69 not installed). Silent stdout
# so a poller can `if "$SELF" check; then …` without reading any output.
cmd_check() {
  is_installed && exit "$EX_OK"
  not_installed_notice
  exit "$EX_UNAVAILABLE"
}

cmd_install() {
  echo "render-mermaid: installing mermaid-cli + Chromium (~170MB)…" >&2
  ( cd "$SCRIPT_DIR" && npm install --no-audit --no-fund --loglevel=error )
  # Puppeteer's postinstall normally fetches Chromium; make sure it's there.
  if ! is_installed; then
    ( cd "$SCRIPT_DIR" && npx --no-install puppeteer browsers install chrome )
  fi
  if is_installed; then
    echo "render-mermaid: installed" >&2
    exit "$EX_OK"
  fi
  echo "render-mermaid: install FAILED — see output above" >&2
  exit "$EX_UNAVAILABLE"
}

# run_mmdc: quiet (-q) so only real errors reach stderr; a render failure is
# normalized to EX_SOFTWARE regardless of mmdc's own exit code.
run_mmdc() { "$MMDC" -q -p "$PUPPETEER_CFG" "$@" || exit "$EX_SOFTWARE"; }

# Pull the value following a flag out of an mmdc arg list, if the flag is present.
value_after_flag() {
  local want1="$1" want2="$2"; shift 2
  local prev=""
  for a in "$@"; do
    [[ "$prev" == "$want1" || "$prev" == "$want2" ]] && { printf '%s' "$a"; return 0; }
    prev="$a"
  done
  return 1
}
input_from_args()  { value_after_flag -i --input  "$@"; }
output_from_args() { value_after_flag -o --output "$@"; }

# Ensure the parent directory of an output path exists, so a render never fails
# with a raw mmdc crash just because the target dir is missing.
ensure_output_dir() {
  local out="$1" dir
  [[ "$out" == "-" ]] && return 0            # stdout target has no dir
  dir="$(dirname -- "$out")"
  [[ -d "$dir" ]] && return 0
  mkdir -p -- "$dir" 2>/dev/null && return 0
  echo "render-mermaid: cannot create output directory: $dir" >&2
  exit "$EX_CANTCREAT"
}

require_input_exists() {
  local in="$1"
  [[ "$in" == "-" || -f "$in" ]] && return 0   # "-" is stdin
  echo "render-mermaid: input file not found: $in" >&2
  exit "$EX_NOINPUT"
}

cmd_render() {
  require_installed

  # --all <dir>: batch every *.mmd under <dir> to a sibling *.svg.
  if [[ "${1:-}" == "--all" ]]; then
    local dir="${2:-.}"
    shift 2 || true
    shopt -s globstar nullglob
    local found=0 f out
    for f in "$dir"/**/*.mmd; do
      found=1
      out="${f%.mmd}.svg"
      run_mmdc -i "$f" -o "$out" "$@"
      printf '%s\n' "$out"          # payload: the path we produced
    done
    if [[ "$found" != 1 ]]; then
      echo "render-mermaid: no *.mmd found under $dir" >&2
      exit "$EX_USAGE"
    fi
    exit "$EX_OK"
  fi

  # A single bare positional arg is the input; output inferred as .svg.
  if [[ $# -eq 1 && "$1" != -* ]]; then
    require_input_exists "$1"
    local out="${1%.*}.svg"
    run_mmdc -i "$1" -o "$out"
    printf '%s\n' "$out"            # payload: the inferred path the caller lacks
    exit "$EX_OK"
  fi

  # Passthrough: caller supplied their own flags (incl. -o), so the output path
  # is already theirs — stay silent on success; the exit code is the signal.
  local in out
  if in="$(input_from_args "$@")"; then require_input_exists "$in"; fi
  if out="$(output_from_args "$@")"; then ensure_output_dir "$out"; fi
  run_mmdc "$@"
  exit "$EX_OK"
}

# --- dispatch ----------------------------------------------------------------
case "${1:-}" in
  check)            shift; cmd_check "$@" ;;
  install)          shift; cmd_install "$@" ;;
  render)           shift; cmd_render "$@" ;;
  -h|--help|help)   awk 'NR>1 && /^#/{sub(/^# ?/,"");print;next} NR>1{exit}' "${BASH_SOURCE[0]}" ;;
  "")               echo "render-mermaid: nothing to do. Try: $SELF --help" >&2; exit "$EX_USAGE" ;;
  *)                cmd_render "$@" ;;   # default: treat args as a render request
esac
