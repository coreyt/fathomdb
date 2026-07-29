#!/usr/bin/env bash
# check-staged-ledger-sidecars.sh — TC-88 (DOC-HYGIENE-3): a ledger and its
# `.seq` sidecar CANNOT BE COMMITTED APART.
#
# ---------------------------------------------------------------------------
# THE MEASURED FAILURE
# ---------------------------------------------------------------------------
# `dev/agent-tools/ledgerwrite/ledgerwrite.py` appends to `<ledger>.jsonl` AND
# writes the new high-water mark to a sibling `<ledger>.jsonl.seq`. The natural
# git incantation after an append is
#     git add dev/steward/steward-ledger.jsonl
# — the sidecar is a SEPARATE PATH, is not implied by it, and nothing warned.
# Commits 41a81c17 (steward seq-131) and 3e660f95 (todos TC-87) each staged the
# `.jsonl` and stranded the `.seq`. Twenty minutes apart, same session, same
# actor: discipline did not prevent the second occurrence, which is the point.
#
# BLAST RADIUS, and why this is worse than a private mistake. The sidecar is
# TRACKED and is the only thing an appender reads to pick the next seq, so a
# stranded one mints COLLIDING seq numbers in every clone taken afterwards.
# `scripts/preflight.sh --landing` §8 hard-fails when a sidecar disagrees with
# max(seq) — so the actor who caused it SEES NOTHING WRONG while the landing gate
# on `main` is broken for every subsequent agent. It surfaced only because the
# Slice 21 orchestrator hit exit 1 at its push and correctly refused to clear a
# ledger it had been told was not its own.
#
# ---------------------------------------------------------------------------
# WHY DETECTION ALREADY EXISTED AND WAS NOT ENOUGH
# ---------------------------------------------------------------------------
# `scripts/check-ledgers.sh` reads the WORKING TREE, and preflight/CI run it
# there. On the machine that made the mistake the working tree is CONSISTENT —
# both files are correct on disk; only the COMMIT is torn. So both existing
# homes went green for the author and red for everyone downstream. The missing
# predicate is over the INDEX, at the moment of the mistake. That is this file.
#
# TC-88 rules out the other remedy explicitly: "DO NOT fix this by telling agents
# to remember the sidecar; that is the failure mode this repo has already ruled
# against." Guardrail failures are fixed in tooling.
#
# ---------------------------------------------------------------------------
# THE PREDICATE — AND IT IS NOT "BOTH PATHS ARE STAGED"
# ---------------------------------------------------------------------------
# For every tracked ledger pair, the CONTENT THAT WILL BE COMMITTED must satisfy
# the ledger invariant. "Did you also `git add` the sidecar?" would be a proxy,
# and a proxy passes the case where both are staged and still disagree — an
# unbacked high-water mark reached by a different route, which is the same defect.
#
# HOW: materialise every ledger pair FROM THE INDEX (`git show :0:<path>`, i.e.
# exactly the bytes `git commit` is about to write) into a temp tree, then run
# `scripts/check-ledgers.sh --root <tmp>` over it.
#
# REUSE, NOT REIMPLEMENTATION. This is the repo's standing pattern for shared
# gates (board-currency, ledger-integrity, governed-surface pin, C-1 conformance,
# transcript hygiene): ONE predicate, several callers, so they cannot diverge. A
# second local copy of "sidecar == max(seq)" would eventually disagree with the
# one `preflight --landing` enforces, and two gates disagreeing about the same
# invariant is worse than either alone. Contiguity (check (b)) comes along free.
#
# SCOPE / DISCOVERY is the SIDECAR, exactly as in check-ledgers.sh: a `*.jsonl`
# with no `*.jsonl.seq` beside it is not a ledger (dev/experiments/** run output,
# test fixtures, golden corpora) and is out of scope by construction.
#
# IT DOES NOT FIRE ON UNRELATED WORK. If the commit stages no ledger path at all,
# this exits 0 immediately. A gate that taxed every commit in the repo would be
# turned off, and a gate that gets turned off is worse than no gate.
#
# READ-ONLY: it never touches the index or the working tree. A commit-time gate
# that auto-staged would change what is about to be committed out from under the
# author, and an author who did not intend the sidecar change would ship it
# unseen.
#
# Usage:
#   scripts/check-staged-ledger-sidecars.sh
#
# Exit: 0 = no ledger path staged, or every ledger pair is consistent as
#           committed; 1 = a pair would be torn or inconsistent; 2 = the gate
#           could not run (not a repo, python3 absent, an unmerged path).
set -euo pipefail

if ! TOPLEVEL="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  printf 'check-staged-ledger-sidecars: not inside a git repository\n' >&2
  exit 2
fi
cd "$TOPLEVEL"

SELF_DIR="$TOPLEVEL/scripts"
CHECK_LEDGERS="$SELF_DIR/check-ledgers.sh"
if [ ! -f "$CHECK_LEDGERS" ]; then
  printf 'check-staged-ledger-sidecars: %s is missing — the shared ledger predicate\n' \
    "$CHECK_LEDGERS" >&2
  printf '  cannot run, and a gate that cannot run must not report a pass (TC-37).\n' >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# 1. Discover every tracked ledger pair (driven by the SIDECAR).
# ---------------------------------------------------------------------------
SIDECARS=()
while IFS= read -r s; do
  [ -n "$s" ] || continue
  SIDECARS+=("$s")
done < <(git ls-files -- '*.jsonl.seq' | LC_ALL=C sort)

if [ "${#SIDECARS[@]}" -eq 0 ]; then
  # No ledgers tracked at all. Nothing to tear; this is not the TC-37 hole,
  # because the hole is about a gate that CLAIMS to have checked. Say what was
  # actually true and exit 0 — a repo with no ledgers must still be committable.
  exit 0
fi

# ---------------------------------------------------------------------------
# 2. Is any ledger path in this commit? If not, get out of the way.
# ---------------------------------------------------------------------------
STAGED="$(git diff --cached --name-only)"
TOUCHED=()
for sc in "${SIDECARS[@]}"; do
  lg="${sc%.seq}"
  if grep -qxF "$sc" <<<"$STAGED" || grep -qxF "$lg" <<<"$STAGED"; then
    TOUCHED+=("$sc")
  fi
done
if [ "${#TOUCHED[@]}" -eq 0 ]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# 3. Materialise the pairs AS THEY WILL BE COMMITTED.
# ---------------------------------------------------------------------------
# `:0:<path>` is the index at stage 0 — the exact bytes `git commit` writes. For
# a tracked file that is NOT staged, the index still carries HEAD's content, so
# this correctly models "the .jsonl moved and the .seq did not".
STAGE_DIR="$(mktemp -d)"
cleanup() {
  case "$STAGE_DIR" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$STAGE_DIR" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$STAGE_DIR" >&2 ;;
  esac
}
trap cleanup EXIT

materialise() {
  # $1 = repo-relative path. Absent from the index (a staged deletion) is not an
  # error here: check-ledgers.sh reports a dangling sidecar or a missing ledger
  # with its own wording, and duplicating that judgement would be a second
  # predicate.
  local p="$1"
  local blob
  if ! blob="$(git show ":0:$p" 2>/dev/null)"; then
    return 1
  fi
  mkdir -p "$STAGE_DIR/$(dirname "$p")"
  printf '%s' "$blob" >"$STAGE_DIR/$p"
  return 0
}

MATERIALISED=0
for sc in "${TOUCHED[@]}"; do
  lg="${sc%.seq}"
  # An UNMERGED path has no stage 0. Refusing loudly is correct: mid-conflict is
  # exactly when a ledger is most likely to be resolved wrongly, and clearing a
  # commit whose content the gate could not read is the fail-open this repo's
  # other gates all guard against.
  if git ls-files --unmerged -- "$sc" "$lg" | grep -q .; then
    printf 'FAIL check-staged-ledger-sidecars: `%s` and/or `%s` are UNMERGED, so the\n' "$lg" "$sc" >&2
    printf '  content that would be committed cannot be read. Resolve the conflict and\n' >&2
    printf '  `git add` BOTH files, then commit.\n' >&2
    exit 2
  fi
  materialise "$sc" || true
  materialise "$lg" || true
  MATERIALISED=$((MATERIALISED + 1))
done

if [ "$MATERIALISED" -eq 0 ]; then
  printf 'FAIL check-staged-ledger-sidecars: a ledger path is staged but ZERO pairs were\n' >&2
  printf '  materialised from the index, so nothing was checked. That is a vacuous pass,\n' >&2
  printf '  not a pass (TC-37).\n' >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 4. Run the SHARED predicate over the staged tree.
# ---------------------------------------------------------------------------
set +e
LEDGER_OUT="$(bash "$CHECK_LEDGERS" --root "$STAGE_DIR" 2>&1)"
LEDGER_RC=$?
set -e

if [ "$LEDGER_RC" -eq 0 ]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# 5. Refuse, and say EXACTLY what to run.
# ---------------------------------------------------------------------------
# A gate that says "wrong" without saying "run this" makes the fix a guess, and a
# guess at commit time is how `--no-verify` gets reached for — which AGENTS.md
# forbids and which would put the repo straight back into the incident.
printf 'FAIL check-staged-ledger-sidecars: this commit would leave a ledger and its\n' >&2
printf '  `.seq` sidecar INCONSISTENT once committed (TC-88).\n' >&2
printf '\n' >&2
printf '%s\n' "$LEDGER_OUT" | sed 's/^/  /' >&2
printf '\n' >&2
printf '  The working tree on THIS machine may look fine — check-ledgers.sh reads the\n' >&2
printf '  worktree, and this gate reads the INDEX. That gap is the whole defect: a torn\n' >&2
printf '  commit is invisible to its author and breaks `preflight --landing` on main for\n' >&2
printf '  the NEXT agent.\n' >&2
printf '\n' >&2
printf '  Staged in this commit:\n' >&2
for sc in "${TOUCHED[@]}"; do
  lg="${sc%.seq}"
  if grep -qxF "$lg" <<<"$STAGED"; then lg_s="STAGED    "; else lg_s="not staged"; fi
  if grep -qxF "$sc" <<<"$STAGED"; then sc_s="STAGED    "; else sc_s="not staged"; fi
  printf '    %s  %s\n' "$lg_s" "$lg" >&2
  printf '    %s  %s\n' "$sc_s" "$sc" >&2
done
printf '\n' >&2
printf '  Fix — stage the PAIR, never one half:\n' >&2
printf '    git add' >&2
for sc in "${TOUCHED[@]}"; do
  printf ' %s %s' "${sc%.seq}" "$sc" >&2
done
printf '\n' >&2
printf '  If the pair is genuinely inconsistent on disk (the sidecar does not equal\n' >&2
printf '  max(seq), or a seq is duplicated/skipped), repair the LEDGER first — never\n' >&2
printf '  hand-edit the sidecar to silence this.\n' >&2
exit 1
