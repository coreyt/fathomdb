#!/usr/bin/env bash
# check-ledgers.sh — ledger integrity gate (sidecar agreement + seq contiguity).
#
# Shared by two callers (DOC-HYGIENE-2 T1b), exactly like its sibling
# scripts/check-board-currency.sh:
#   * scripts/preflight.sh --landing        (PREVENT, land-time gate)
#   * .github/workflows/ci.yml ledger-integrity job
#                                           (DETECT, always-on CI backstop)
# Reuse, not reimplementation: both callers invoke THIS script so the predicate
# cannot diverge between the two homes.
#
# THE MEASURED FAILURE THIS CLOSES
#   19 consecutive commits (f22e4947 -> 3264114a, four days) shipped
#   `dev/steward/steward-ledger.jsonl.seq` frozen at 80 while max(seq) inside
#   the ledger had already reached 98. The sidecar is a TRACKED file and is the
#   only thing an appender reads to pick the next seq, so any clone taken in
#   that window would have minted 81, 82, ... — colliding with 18 entries that
#   already existed. Nothing in the repo checked that the sidecar agreed with
#   the file it summarizes.
#
# PREDICATE — EXACTLY TWO CHECKS, per discovered ledger. This is deliberate and
# is not a starting point to grow: the two below are the ones with a measured
# failure behind them and a mechanical, vocabulary-free answer.
#   (a) SIDECAR AGREEMENT: the integer in <ledger>.jsonl.seq equals max(seq)
#       over the entries in <ledger>.jsonl.
#   (b) CONTIGUITY: the seq values have no gaps and no duplicates.
# Explicitly NOT checked (each was considered and ruled out): a status
# vocabulary; a --summary length cap; an `id` field requirement (the steward
# ledger is a decision trail — 107/107 entries carry no `id`, so demanding one
# would fail every entry); and the ORDER seqs appear in the file (that would be
# a third check — (b) is stated over the multiset, not the file order).
#   Note (b) deliberately does NOT require min(seq)==1: a ledger whose head was
# ever archived legitimately starts above 1, and pinning the floor would be a
# third check with no measured failure behind it.
#
# SCOPE / DISCOVERY: every `*.jsonl.seq` sidecar found under the root, with
# .git/, node_modules/, target/ and .venv/ pruned. Discovery is driven by the
# SIDECAR, never by a hardcoded list, so a ledger added tomorrow is covered the
# day it gets a sidecar — and a sidecar-less `*.jsonl` (dev/experiments/** run
# output, test fixtures, golden corpora) is out of scope by construction.
# Three ledgers qualify today:
#   dev/steward/steward-ledger.jsonl
#   dev/todos-and-considerations-ledger.jsonl
#   dev/design/record-lifecycle-protocol/OPP-12-sub-ledger.jsonl
#
# VACUOUS-PASS GUARD (TC-37, this repo's named failure class — the same shape as
# check-board-currency.sh's fix-1 guard): if discovery finds ZERO sidecars, the
# per-ledger loop body never runs and the gate would pass green having vouched
# for nothing. A gate that silently checks nothing is worse than no gate: it is
# an active false assurance. Zero discovered ledgers is therefore a HARD
# failure, never exit 0. This can only convert a silent pass into a loud
# failure; it never fires once >=1 ledger is discovered.
#
# DETERMINISTIC EDGE-CASE HANDLING (documented so it is a contract, not an
# accident of implementation):
#   blank / whitespace-only line  -> IGNORED (never written by a well-behaved
#       appender, harmless if present; mirrors ledgerwatch --validate).
#   line that is not valid JSON   -> HARD fail naming <file>:<line>. The seq set
#       cannot be trusted past a torn line, so skipping it would be a vacuous
#       pass over the rest of the file.
#   entry with no integer `seq`   -> HARD fail naming <file>:<line> (same
#       reason; `true`/`false` are not integers here).
#   EMPTY ledger (zero entries)   -> max(seq) is defined as 0, so the sidecar
#       must read 0; any other value is an unbacked high-water mark and HARD
#       fails. Contiguity is vacuously satisfied over zero entries — note this
#       is NOT the vacuous-pass hole, which is about zero LEDGERS, not zero
#       entries: the sidecar's existence is what puts the file in scope.
#   sidecar not a non-negative integer (blank, "eighty", multi-line) -> HARD
#       fail. A trailing newline IS tolerated (the three real sidecars have
#       none, but an editor may add one).
#   sidecar with no .jsonl beside it -> HARD fail (dangling sidecar).
#
# Usage:
#   scripts/check-ledgers.sh [--root <dir>]
#
# --root exists so the test fixtures can be plain directories rather than
# throwaway git repos; without it the script operates on the enclosing repo's
# toplevel, exactly as both real callers invoke it.
#
# Requires python3 for JSON parsing. If it is absent this script exits 2 (env
# error) LOUDLY rather than skipping — a skip here would be the TC-37 hole.
#
# Exit codes: 0 = every discovered ledger satisfies (a) and (b);
#             1 = at least one integrity failure, OR zero ledgers discovered
#                 (vacuous-pass guard); 2 = usage/environment error.
set -euo pipefail

ROOT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="${2:?--root needs a dir}"; shift 2 ;;
    *) printf 'check-ledgers: unknown arg %q\n' "$1" >&2; exit 2 ;;
  esac
done

if [ -n "$ROOT" ]; then
  if [ ! -d "$ROOT" ]; then
    printf 'check-ledgers: --root %q is not a directory\n' "$ROOT" >&2
    exit 2
  fi
  cd "$ROOT"
else
  if ! TOPLEVEL="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    printf 'check-ledgers: not inside a git repository (and no --root given)\n' >&2
    exit 2
  fi
  cd "$TOPLEVEL"
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf 'check-ledgers: python3 is required to parse the ledgers and is not on PATH — refusing to report a pass it did not verify\n' >&2
  exit 2
fi

# Discovery. `-print | sort` rather than `-print0 | sort -z` because `sort -z`
# is GNU-only and this repo's scripts run on macOS too; ledger paths never
# contain newlines (a path that did would be pathological, and `read -r` with a
# cleared IFS still handles every other whitespace character).
SIDECARS=()
while IFS= read -r s; do
  [ -n "$s" ] || continue
  SIDECARS+=("${s#./}")
done < <(
  find . \
    \( -name .git -o -name node_modules -o -name target -o -name .venv \) -prune -o \
    -name '*.jsonl.seq' -type f -print | LC_ALL=C sort
)

if [ "${#SIDECARS[@]}" -eq 0 ]; then
  printf 'BROKEN  discovery found no *.jsonl.seq ledger sidecars under %s — this gate cannot vouch for anything, so it fails loudly instead of reporting a vacuous ok (TC-37)\n' \
    "$(pwd)" >&2
  exit 1
fi

# The two checks. One python3 pass over all discovered ledgers, so the JSON is
# genuinely parsed rather than regex-scraped for `"seq"`.
set +e
python3 - "${SIDECARS[@]}" >&2 <<'PY'
import json
import os
import sys

CAP = 5  # per-ledger cap on enumerated bad lines / gaps / duplicates
failed = False


def broken(msg):
    global failed
    failed = True
    print("BROKEN  " + msg)


for sidecar in sys.argv[1:]:
    ledger = sidecar[: -len(".seq")]

    # --- sidecar must be a single non-negative integer -----------------------
    try:
        with open(sidecar, "r", encoding="utf-8") as fh:
            raw = fh.read()
    except OSError as exc:
        broken(f"{sidecar}: cannot read sidecar: {exc}")
        continue
    text = raw.strip()
    if not text.isdigit():
        broken(
            f"{sidecar}: sidecar is not a non-negative integer (read {raw!r}) — "
            "the next seq cannot be minted from it"
        )
        continue
    claimed = int(text)

    # --- the ledger must exist beside its sidecar ----------------------------
    if not os.path.isfile(ledger):
        broken(f"{sidecar}: dangling sidecar — no {os.path.basename(ledger)} beside it")
        continue

    # --- parse entries -------------------------------------------------------
    seqs = []
    bad_lines = []
    with open(ledger, "r", encoding="utf-8", errors="replace") as fh:
        for lineno, line in enumerate(fh, 1):
            if not line.strip():
                continue  # blank / whitespace-only: ignored, by contract
            try:
                obj = json.loads(line)
            except Exception as exc:
                bad_lines.append((lineno, f"not valid JSON: {exc}"))
                continue
            if not isinstance(obj, dict):
                bad_lines.append((lineno, "entry is not a JSON object"))
                continue
            val = obj.get("seq")
            if isinstance(val, bool) or not isinstance(val, int):
                bad_lines.append((lineno, 'entry has no integer "seq" field'))
                continue
            seqs.append(val)

    if bad_lines:
        for lineno, why in bad_lines[:CAP]:
            broken(f"{ledger}:{lineno}: {why}")
        if len(bad_lines) > CAP:
            broken(f"{ledger}: ...and {len(bad_lines) - CAP} more unparseable line(s)")
        # The seq set is untrustworthy past a torn line, so (a) and (b) are not
        # evaluated for this ledger — it is already a HARD failure.
        continue

    # --- check (a): sidecar agreement ---------------------------------------
    observed_max = max(seqs) if seqs else 0
    agrees = claimed == observed_max
    if not agrees:
        broken(
            f"{sidecar}: sidecar says {claimed} but max(seq) is {observed_max} "
            f"in {ledger} ({len(seqs)} entr{'y' if len(seqs) == 1 else 'ies'}) — "
            "an appender trusting this sidecar would mint colliding seq numbers"
        )

    # --- check (b): contiguity (no gaps, no duplicates) ----------------------
    if not seqs:
        if agrees:
            print(f"ok    {ledger}: empty ledger, sidecar reads 0 (consistent)")
        continue

    counts = {}
    for v in seqs:
        counts[v] = counts.get(v, 0) + 1
    dups = sorted(v for v, n in counts.items() if n > 1)
    if dups:
        shown = ", ".join(f"{v} (appears {counts[v]}x)" for v in dups[:CAP])
        more = f" ...and {len(dups) - CAP} more" if len(dups) > CAP else ""
        broken(f"{ledger}: seq is not contiguous — duplicate seq: {shown}{more}")
    lo, hi = min(seqs), max(seqs)
    missing = sorted(set(range(lo, hi + 1)) - set(counts))
    if missing:
        shown = ", ".join(str(v) for v in missing[:CAP])
        more = f" ...and {len(missing) - CAP} more" if len(missing) > CAP else ""
        broken(f"{ledger}: seq is not contiguous — missing seq: {shown}{more}")

    if agrees and not dups and not missing:
        print(
            f"ok    {ledger}: seq {lo}..{hi} contiguous ({len(seqs)} entries), "
            "sidecar agrees"
        )

sys.exit(1 if failed else 0)
PY
RC=$?
set -e

if [ "$RC" -eq 0 ]; then
  printf 'ok    ledgers: %d checked, sidecars agree with max(seq) and every seq run is contiguous\n' \
    "${#SIDECARS[@]}" >&2
fi

exit "$RC"
