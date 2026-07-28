#!/usr/bin/env bash
# preflight.sh — orchestrator-side gate, run BEFORE spawning an implementer.
#
# Codifies the checks whose absence has cost real slices:
#   * agent-worktree-stale-base-trap — a worktree cut from a stale base (main had
#     advanced ~206 commits) silently lost two slices. The --worktree check below
#     fails loudly when a worktree's HEAD is neither current main nor a descendant
#     of it.
#   * dependency-not-actually-CLOSED — spawning a slice whose declared dependency
#     never closed. The --expect-closed check greps the plan for the CLOSED block.
#   * landing-in-the-primary-checkout — TC-RUBRIC-5 requires release orchestration
#     and all landing git-writes to run in a dedicated linked worktree. --landing
#     HARD-fails when invoked from the primary checkout.
#
# Witness-first (orchestration.md §1.5): every check derives from the repo on disk,
# never from belief. Emits a one-line JSON summary; exits non-zero on any HARD fail.
#
# Usage:
#   scripts/preflight.sh                              # general repo health (run anytime)
#   scripts/preflight.sh --worktree /tmp/fdb-slice-5-... # + stale-base guard on a freshly cut WT
#   scripts/preflight.sh --expect-closed 5 --plan dev/plans/plan-0.8.6.md
#   scripts/preflight.sh --landing                    # TC-RUBRIC-5: refuse to land in the primary checkout
#   scripts/preflight.sh --worktree <wt> --expect-closed 5 --plan <plan> --min-disk-gb 15
#
# --landing composes with every other flag and is inert unless passed: without it
# the script behaves exactly as it did before the flag existed.
#
# Exit codes: 0 = all HARD checks pass (WARN/INFO may print); 1 = a HARD check failed.

set -euo pipefail

# Resolve this script's own directory BEFORE the toplevel cd below, so the
# board-currency check (§7) can find its sibling script regardless of which
# repo/worktree `cd` below lands us in (BASH_SOURCE is a file path, not
# affected by cwd, but resolving it AFTER a cd elsewhere would be needless
# fragility for no benefit).
SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cd "$(git rev-parse --show-toplevel)"
CANON="$(pwd)"

WT=""
EXPECT_CLOSED=""
PLAN=""
MIN_DISK_GB=10
LANDING=0

while [ $# -gt 0 ]; do
  case "$1" in
    --landing)       LANDING=1; shift ;;
    --worktree)      WT="${2:?--worktree needs a path}"; shift 2 ;;
    --expect-closed) EXPECT_CLOSED="${2:?--expect-closed needs a slice id}"; shift 2 ;;
    --plan)          PLAN="${2:?--plan needs a file}"; shift 2 ;;
    --min-disk-gb)   MIN_DISK_GB="${2:?--min-disk-gb needs a number}"; shift 2 ;;
    *) printf 'preflight: unknown arg %q\n' "$1" >&2; exit 2 ;;
  esac
done

HARD_FAILS=()
WARNS=()

hard() { HARD_FAILS+=("$1"); printf 'HARD  %s\n' "$1" >&2; }
warn() { WARNS+=("$1");      printf 'WARN  %s\n' "$1" >&2; }
info() { printf 'INFO  %s\n' "$1" >&2; }
ok()   { printf 'ok    %s\n' "$1" >&2; }

# Resolve an existing directory to its absolute, symlink-resolved path. Uses
# cd+pwd -P rather than `readlink -f` so this works on macOS too. Prints nothing
# (and returns non-zero) if the path is not a reachable directory.
abs_dir() { ( cd "$1" 2>/dev/null && pwd -P ); }

MAIN_SHA="$(git rev-parse main)"

# --- 1. Canonical repo is not mid-operation -------------------------------------
GITDIR="$(git rev-parse --git-dir)"
if [ -d "$GITDIR/rebase-merge" ] || [ -d "$GITDIR/rebase-apply" ]; then
  hard "canonical repo is mid-rebase — finish or abort before spawning"
elif [ -f "$GITDIR/MERGE_HEAD" ]; then
  hard "canonical repo is mid-merge — finish or abort before spawning"
elif [ -f "$GITDIR/CHERRY_PICK_HEAD" ]; then
  hard "canonical repo is mid-cherry-pick — finish or abort before spawning"
else
  ok "canonical repo is not mid-merge/rebase/cherry-pick"
fi

# --- 2. Tracked source is clean (dev/ docs churn is expected, so only gate src/) -
DIRTY_SRC="$(git status --porcelain -- src/ scripts/ mkdocs.yml 2>/dev/null || true)"
if [ -n "$DIRTY_SRC" ]; then
  warn "tracked source dirty on canonical main (commit/stash before a worktree inherits a stale base):"
  printf '%s\n' "$DIRTY_SRC" | sed 's/^/        /' >&2
else
  ok "tracked source (src/ scripts/ mkdocs.yml) is clean"
fi

# --- 3. Disk headroom (worktrees + per-worktree target/ are not free) ------------
WT_PARENT="${WT:+$(dirname "$WT")}"; WT_PARENT="${WT_PARENT:-/tmp}"
FREE_GB="$(df -BG --output=avail "$WT_PARENT" 2>/dev/null | tail -1 | tr -dc '0-9')"
if [ -n "$FREE_GB" ] && [ "$FREE_GB" -lt "$MIN_DISK_GB" ]; then
  hard "only ${FREE_GB}G free on $WT_PARENT (need >= ${MIN_DISK_GB}G; prune stale worktrees)"
else
  ok "disk headroom on $WT_PARENT: ${FREE_GB:-?}G free (>= ${MIN_DISK_GB}G)"
fi

# --- 4. Worktree stale-base guard (the load-bearing check) -----------------------
if [ -n "$WT" ]; then
  if [ ! -d "$WT" ]; then
    hard "worktree path does not exist: $WT"
  elif [ "$(git -C "$WT" rev-parse --show-toplevel 2>/dev/null || echo MISSING)" != "$WT" ]; then
    hard "not a git worktree rooted at: $WT"
  else
    WT_HEAD="$(git -C "$WT" rev-parse HEAD)"
    if [ "$WT_HEAD" = "$MAIN_SHA" ]; then
      ok "worktree HEAD == current main ($MAIN_SHA) — freshly cut, no stale base"
    elif [ "$(git merge-base "$MAIN_SHA" "$WT_HEAD")" = "$MAIN_SHA" ]; then
      warn "worktree has advanced past main (main is an ancestor) — OK if it carries this slice's commits"
    else
      hard "STALE BASE: worktree HEAD ($WT_HEAD) is not current main and main is not its ancestor."
      hard "  -> re-create the worktree off \$(git rev-parse main). See agent-worktree-stale-base-trap."
    fi
    # maturin/pip -e from a worktree rebinds the shared .venv to the worktree tree.
    info "reminder: do NOT 'maturin develop' / 'pip install -e' from a worktree — build on the MAIN tree only."
  fi
fi

# --- 5. Dependency-CLOSED gate ---------------------------------------------------
if [ -n "$EXPECT_CLOSED" ]; then
  if [ -z "$PLAN" ]; then
    hard "--expect-closed $EXPECT_CLOSED given without --plan <file>"
  elif [ ! -f "$PLAN" ]; then
    hard "plan file not found: $PLAN"
  elif grep -qiE "(Slice|Phase)[^A-Za-z0-9]*${EXPECT_CLOSED}[^0-9].*CLOSED|CLOSED.*(Slice|Phase)[^A-Za-z0-9]*${EXPECT_CLOSED}([^0-9]|$)" "$PLAN"; then
    ok "dependency Slice/Phase $EXPECT_CLOSED has a CLOSED witness in $PLAN"
  else
    hard "dependency Slice/Phase $EXPECT_CLOSED has NO 'CLOSED' block in $PLAN — do not spawn dependents"
  fi
fi

# --- 6. Landing-mode guard (TC-RUBRIC-5) -----------------------------------------
# TC-RUBRIC-5 (HITL-ADOPTED 2026-07-11): release orchestration and ALL landing
# git-writes run in a dedicated linked worktree, never in the primary checkout.
#
# primary checkout <=> --git-dir and --git-common-dir name the SAME directory
# (a linked worktree's git-dir is <common>/worktrees/<name>).
#
# Both paths are canonicalized before comparing. git returns these relative or
# absolute depending on cwd and version: from a subdirectory, git 2.43 returns an
# ABSOLUTE --git-dir but a RELATIVE '../.git' --git-common-dir. Today the `cd` to
# the toplevel above normalizes that away (both read '.git' there), so a raw
# string compare would happen to work — but only by accident of that cd. Measured:
# with the toplevel cd removed, a raw compare run from a subdirectory of the
# primary checkout FAILS OPEN (exit 0, "cleared for landing"), while the
# canonicalized compare below still correctly HARD-fails. Canonicalizing makes
# this check self-sufficient rather than dependent on cwd, git version, and the
# ordering of an unrelated line.
if [ "$LANDING" -eq 1 ]; then
  GITDIR_ABS="$(abs_dir "$(git rev-parse --git-dir)" || true)"
  COMMONDIR_ABS="$(abs_dir "$(git rev-parse --git-common-dir)" || true)"
  if [ -z "$GITDIR_ABS" ] || [ -z "$COMMONDIR_ABS" ]; then
    hard "--landing: cannot resolve git-dir/git-common-dir — refusing to certify this tree for landing"
  elif [ "$GITDIR_ABS" = "$COMMONDIR_ABS" ]; then
    hard "TC-RUBRIC-5: --landing invoked in the PRIMARY checkout ($CANON) — landing git-writes are forbidden here."
    hard "  -> re-run from a dedicated linked worktree: git worktree add <path> <branch>, then run this from <path>."
  else
    ok "TC-RUBRIC-5: running in a linked worktree ($CANON) — cleared for landing git-writes"
  fi
fi

# --- 7. Board-currency gate (status-board-currency-enforcement design, item 2) --
# Refuse a land that would leave dev/plans/runs/STATUS-0.8.z.md stale. The
# incident: a slice's merge commit reached origin/main and the board kept saying
# "not landed" for four days — nobody's explicit job to update it at land time
# (an ownership seam between the orchestrator, who owns in-flight rows, and the
# Steward, who lands). See dev/design/status-board-currency-enforcement.md and
# orchestration.md §12.5 for the seam-ownership contract this gate enforces
# mechanically.
#
# The predicate lives in scripts/check-board-currency.sh (see that file's header
# for the full statement) so preflight and the CI drift job (item 3) share ONE
# implementation and cannot diverge. --landing-only: mid-build the board
# legitimately reads "in flight", so this must never run outside a land (that is
# precommit's false-positive trap the design doc rejects).
if [ "$LANDING" -eq 1 ]; then
  BOARD_CHECK_OUT="$(bash "$SELF_DIR/check-board-currency.sh" --tip HEAD 2>&1)" || BOARD_CHECK_RC=$?
  BOARD_CHECK_RC="${BOARD_CHECK_RC:-0}"
  if [ "$BOARD_CHECK_RC" -ne 0 ]; then
    while IFS= read -r line; do
      case "$line" in
        STALE*) hard "board-currency: $line" ;;
        *)      info "board-currency: $line" ;;
      esac
    done <<<"$BOARD_CHECK_OUT"
  else
    ok "board-currency: live STATUS-0.8.z.md board(s) match git ancestry for this tip"
  fi
fi

# --- 8. Ledger-integrity gate (DOC-HYGIENE-2 T1b) --------------------------------
# Refuse a land that would ship a `*.jsonl.seq` sidecar disagreeing with its
# ledger. The incident: 19 consecutive commits (f22e4947 -> 3264114a, four days)
# shipped dev/steward/steward-ledger.jsonl.seq frozen at 80 while max(seq) in
# the ledger had reached 98. The sidecar is TRACKED and is the only thing an
# appender reads to pick the next seq, so every clone taken in that window would
# have minted colliding seq numbers.
#
# The predicate lives in scripts/check-ledgers.sh (see that file's header for
# the full statement — exactly two checks: sidecar == max(seq), and seq
# contiguous) so preflight and the always-on CI job share ONE implementation and
# cannot diverge, exactly as §7 does for board-currency.
#
# --landing-only, mirroring §7's structure. The ledger invariant is in fact true
# at all times (unlike a board, which legitimately reads "in flight" mid-build),
# so an unconditional run would also be sound; it is kept behind --landing so
# preflight's non-landing path stays exactly as lean as it was, and the CI job —
# which is deliberately NOT docs_only-gated — is what covers every non-landing
# push.
if [ "$LANDING" -eq 1 ]; then
  LEDGER_CHECK_OUT="$(bash "$SELF_DIR/check-ledgers.sh" 2>&1)" || LEDGER_CHECK_RC=$?
  LEDGER_CHECK_RC="${LEDGER_CHECK_RC:-0}"
  if [ "$LEDGER_CHECK_RC" -ne 0 ]; then
    LEDGER_SAW_BROKEN=0
    while IFS= read -r line; do
      case "$line" in
        BROKEN*) hard "ledger-integrity: $line"; LEDGER_SAW_BROKEN=1 ;;
        *)       info "ledger-integrity: $line" ;;
      esac
    done <<<"$LEDGER_CHECK_OUT"
    # Anti-fail-open: a non-zero rc with no BROKEN line means the checker itself
    # could not run (exit 2 = usage/env, e.g. python3 absent) — that must still
    # block the land, never degrade into INFO lines and a green summary.
    if [ "$LEDGER_SAW_BROKEN" -eq 0 ]; then
      hard "ledger-integrity: check-ledgers.sh exited $LEDGER_CHECK_RC without reporting a specific defect — refusing to certify this tree for landing"
    fi
  else
    ok "ledger-integrity: every *.jsonl.seq sidecar agrees with its ledger and every seq run is contiguous"
  fi
fi

# --- 9. Governed-surface pin gate (DOC-HYGIENE-2 T1e) ----------------------------
# Refuse a land that would ship a governed surface the HITL has not signed. The
# HITL PRE-SIGNED the accumulated delta of 0.8.20 Slices 5d+10b+15b+15d (AC-079)
# pinned to the exact content of src/conformance/governed-surface-allowlist.json
# as of 427d2712 — 30 allowlist members, recovery_denylist unchanged at the five
# REQ-054 names. This gate makes that pin mechanical.
#
# The predicate lives in scripts/check-governed-surface-pin.sh (see that file's
# header) so preflight and the always-on CI job share ONE implementation and
# cannot diverge, exactly as §7 and §8 do. --landing-only, mirroring their
# structure: the CI job — deliberately NOT docs_only-gated — covers every
# non-landing push.
#
# Tripping this is CORRECT BEHAVIOUR, not a bug: Slices 20/25/30 are expected to
# trip it, and that is exactly what routes a changed surface back to the HITL
# instead of letting it land under a pre-sign that never covered it.
if [ "$LANDING" -eq 1 ]; then
  SURFACE_CHECK_OUT="$(bash "$SELF_DIR/check-governed-surface-pin.sh" 2>&1)" || SURFACE_CHECK_RC=$?
  SURFACE_CHECK_RC="${SURFACE_CHECK_RC:-0}"
  if [ "$SURFACE_CHECK_RC" -ne 0 ]; then
    SURFACE_SAW_FAIL=0
    while IFS= read -r line; do
      case "$line" in
        FAIL*) hard "governed-surface-pin: $line"; SURFACE_SAW_FAIL=1 ;;
        *)     info "governed-surface-pin: $line" ;;
      esac
    done <<<"$SURFACE_CHECK_OUT"
    # Anti-fail-open, as in §8: a non-zero rc with no FAIL line means the checker
    # itself could not run (exit 2 = usage/env, e.g. python3 absent or an
    # unreadable pin) — that must still block the land.
    if [ "$SURFACE_SAW_FAIL" -eq 0 ]; then
      hard "governed-surface-pin: check-governed-surface-pin.sh exited $SURFACE_CHECK_RC without reporting a specific defect — refusing to certify this tree for landing"
    fi
  else
    ok "governed-surface-pin: the governed surface matches the AC-079 pre-signed pin"
  fi
fi

# --- 10. C-1 contract-conformance gate (R-20-H7, RUBRIC-H7 can-i-deploy) --------
# Refuse a land whose code no longer satisfies the ratified cross-repo design
# contract dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md.
# R-20-H7 is a PUBLISH PRECONDITION: "Gate exists and is GREEN. An
# absent-or-failing gate HOLDS the breaking pair" (plan-0.8.20.md §3,
# HITL-directed 2026-07-10).
#
# The predicate lives in scripts/check-c1-conformance.sh (see that file's header
# for the full statement — and, just as importantly, for what it deliberately
# does NOT claim to check) so preflight and the always-on CI job share ONE
# implementation and cannot diverge, exactly as §7/§8/§9 do. --landing-only,
# mirroring their structure: the CI job — deliberately NOT docs_only-gated —
# covers every non-landing push.
#
# Tripping this is CORRECT BEHAVIOUR, not a bug. It means either as-built code
# drifted from a RATIFIED cross-repo contract, or that contract moved without
# its clause registry being re-derived — and both must route to the Steward
# rather than land.
if [ "$LANDING" -eq 1 ]; then
  C1_CHECK_OUT="$(bash "$SELF_DIR/check-c1-conformance.sh" 2>&1)" || C1_CHECK_RC=$?
  C1_CHECK_RC="${C1_CHECK_RC:-0}"
  if [ "$C1_CHECK_RC" -ne 0 ]; then
    C1_SAW_FAIL=0
    while IFS= read -r line; do
      case "$line" in
        FAIL*) hard "c1-contract-conformance: $line"; C1_SAW_FAIL=1 ;;
        *)     info "c1-contract-conformance: $line" ;;
      esac
    done <<<"$C1_CHECK_OUT"
    # Anti-fail-open, as in §8/§9: a non-zero rc with no FAIL line means the
    # checker itself could not run (exit 2 = python3 absent, a missing contract,
    # a malformed pin, an unreadable source file, or an evaporated check set).
    # That must still block the land — a gate that could not see its subject is
    # exactly the case where landing on its silence is worst.
    if [ "$C1_SAW_FAIL" -eq 0 ]; then
      hard "c1-contract-conformance: check-c1-conformance.sh exited $C1_CHECK_RC without reporting a specific defect — refusing to certify this tree for landing"
    fi
  else
    ok "c1-contract-conformance: as-built code still satisfies the pinned OPP-12 C-1 contract (R-20-H7)"
  fi
fi

# --- Summary (JSON, last line) ---------------------------------------------------
json_arr() { local out="" x; for x in "$@"; do out="${out:+$out,}\"$(printf '%s' "$x" | sed 's/\\/\\\\/g; s/"/\\"/g')\""; done; printf '[%s]' "$out"; }

STATUS=$([ ${#HARD_FAILS[@]} -eq 0 ] && echo pass || echo fail)
printf '{"preflight":"%s","main_sha":"%s","worktree":"%s","hard_fails":%s,"warnings":%s}\n' \
  "$STATUS" "$MAIN_SHA" "${WT:-}" "$(json_arr "${HARD_FAILS[@]+"${HARD_FAILS[@]}"}")" "$(json_arr "${WARNS[@]+"${WARNS[@]}"}")"

[ "$STATUS" = pass ]
