#!/usr/bin/env bash
# check-governed-surface-pin.sh — governed-surface pin gate (DOC-HYGIENE-2 T1e).
#
# Shared by two callers, exactly like its siblings scripts/check-ledgers.sh and
# scripts/check-board-currency.sh:
#   * scripts/preflight.sh --landing                 (PREVENT, land-time gate)
#   * .github/workflows/ci.yml governed-surface-pin job
#                                                    (DETECT, always-on backstop)
# Reuse, not reimplementation: both callers invoke THIS script so the predicate
# cannot diverge between the two homes.
#
# WHAT THIS ENFORCES
#   The HITL signed the accumulated governed-surface delta of 0.8.20 Slices
#   5d + 10b + 15b + 15d (AC-079), then 0.8.21 Slice 60's projected-text search
#   delta — pinned to the EXACT CONTENT of src/conformance/governed-surface-
#   allowlist.json at the commit recorded in scripts/governed-surface-pin.json:
#   32 allowlist members, 5 core, and recovery_denylist unchanged at the five
#   REQ-054 names. A pre-sign keyed to specific content is worth exactly as much
#   as the mechanism that notices when that content moves. This is that
#   mechanism: the signed content is recorded in scripts/governed-surface-pin.json
#   and this gate HARD-fails the moment the file diverges from it, routing the
#   reader back to the HITL for a fresh sign-off.
#
# TRIPPING THIS GATE IS CORRECT BEHAVIOUR, NOT A BUG: the pre-sign covers the
# pinned content, and anything else re-opens it. Re-pinning to make the gate
# green without a fresh HITL sign-off is the failure mode the gate exists to
# prevent.
#
# ⚠ CORRECTED 2026-07-26 (HITL, steward ledger seq-113). An earlier revision of
# this header claimed tripping the gate "is precisely what lets Slices 20/25/30
# proceed WITHOUT stopping for a per-slice sign-off". THAT CLAIM WAS FALSE, and
# the mechanism it described does not exist. This gate hashes the file's RAW
# BYTES (predicate (a) below) and `scripts/preflight.sh` treats a FAIL as `hard`,
# refusing to certify the tree for landing. So the procedure that claim implied —
# record the delta as a `_comment` proposal and land — is NOT executable: the
# `_comment` edit IS a byte diff, so writing the proposal blocks the very land it
# was meant to permit. A trip is a genuine HALT, not a soft signal.
#
# THE RULED STRATEGY IS ONE RE-PIN AT THE BATCHED DECISION (TC-59, option (b)):
# the `_comment` prose correction (TC-52) and the signing of any accumulated
# delta happen together as a SINGLE ceremony at the Slice 30 -> Slice 40
# boundary, where AC-079 mints anyway. No `pending_delta` mechanism is built —
# with TC-55 ruled as instrumentation, Slice 20c adds zero governed commands,
# R-20-SUR is a write-time minting rule with no new verb, and R-20-H7 is a gate
# rather than SDK surface, so the pin is NOT expected to trip again in 0.8.20.
#
# IF IT DOES TRIP: HALT and escalate to the Steward. Do not work around it, do
# not re-pin, do not edit the pinned file to make it pass.
#
# PREDICATE — the pinned file must match the pin on ALL of:
#   (a) CONTENT HASH: sha256 and git blob sha1 of the file's raw bytes equal the
#       values recorded in the pin. Both are recorded so a reviewer can
#       reproduce the pin either way (`git rev-parse <pinned_at_commit>:<file>`
#       gives the blob sha1 directly).
#   (b) MEMBER LISTS: `allowlist`, `core` and `recovery_denylist` are compared
#       element-by-element against the copies stored in the pin, so the failure
#       can name WHICH member appeared or vanished — and so that updating only
#       the hash in the pin (a lazy re-pin, to silence the gate) still fails.
#   (c) COUNTS: 32 / 5 / 5, asserted separately from (b) against the pin's own
#       `counts` block, so a pin whose lists and counts disagree is caught as an
#       internally inconsistent (botched) re-pin rather than being trusted.
#       EVERY one of the three counts must be PRESENT and an integer: a count the
#       gate cannot read is a MALFORMED PIN (exit 2), never a skipped check. An
#       absent count used to read back as None and silently disable both halves
#       of (c) for that list, so a re-pin that DELETED a count, added a member
#       and recomputed the hashes exited 0 — disarming the very backstop that
#       makes a lazy re-pin (b) impossible to hide.
#   (d) REQ-054: `recovery_denylist` is EXACTLY {recover, restore, repair, fix,
#       rebuild}, checked against a constant HARDCODED BELOW — in the FILE and in
#       the PIN. This rule (AC-041) is independently load-bearing: the recovery
#       denylist is five names, and it must not silently widen even by way of an
#       otherwise well-formed re-pin. This is the one check a re-pin cannot buy
#       its way past.
#
# WHITESPACE / FORMATTING-ONLY CHANGES FAIL. DELIBERATE, DOCUMENTED CHOICE:
#   (a) is a CONTENT hash over raw bytes, so re-indenting the file, reordering
#   keys, or adding a trailing newline HARD-fails even though the parsed members
#   are unchanged. The pin is a statement about a file's content at a commit, not
#   about its abstract meaning — and a "harmless reformat" is the ideal cover for
#   a member slipped in on an adjacent line. When only formatting moved, the
#   failure says so explicitly (the parsed members are reported as identical), so
#   the reader is never left guessing whether the surface actually changed.
#
# VACUOUS-PASS GUARD (TC-37, this repo's named failure class): if the pinned file
# is MISSING or UNREADABLE, this gate HARD-fails — it never exits 0. A gate that
# cannot see its subject and reports green is worse than no gate: it is an active
# false assurance. A vanished governed-surface allowlist is also, on its face, the
# largest possible change to the governed surface.
#
# Usage:
#   scripts/check-governed-surface-pin.sh [--file <path>] [--pin <path>] [--help]
#
# --file/--pin exist so the test fixtures can point at COPIES under mktemp -d; the
# real src/conformance/governed-surface-allowlist.json is never written by the
# tests (mutating it is the exact thing this gate exists to catch). Both callers
# invoke the script with no arguments. (--help mirrors scripts/set-version.sh, the
# one sibling that offers it.)
#
# Requires python3 for JSON parsing and hashing. If it is absent this script exits
# 2 (env error) LOUDLY rather than skipping — a skip here would be the TC-37 hole.
#
# Exit codes: 0 = the pinned file matches the pin on (a)-(d);
#             1 = divergence from the pin, OR the pinned file is missing /
#                 unreadable / unparseable (vacuous-pass guard), OR the pin
#                 itself breaches REQ-054 or is internally inconsistent;
#             2 = usage error, unreadable/unparseable/MALFORMED pin file, or
#                 python3 absent (the gate could not run — never reported as a
#                 pass). "Malformed" is structural: a missing/mistyped field the
#                 predicate reads. That is deliberately NOT exit 1: exit 1 says
#                 "the surface diverged, take it back to the HITL", whereas a
#                 malformed pin means the gate has no trustworthy statement of
#                 what was signed at all.
set -euo pipefail

SELF="$(basename "${BASH_SOURCE[0]}")"

usage() {
  cat <<EOF
Usage: scripts/$SELF [--file <path>] [--pin <path>]

Fails when src/conformance/governed-surface-allowlist.json diverges from the
AC-079 pre-signed pin recorded in scripts/governed-surface-pin.json. The pin
itself records the signed content's provenance. See the header of this script
for the full predicate and for why re-pinning without a fresh HITL sign-off is
forbidden.

  --file <path>  the governed-surface allowlist to check
                 (default: src/conformance/governed-surface-allowlist.json)
  --pin <path>   the pin record to check it against
                 (default: scripts/governed-surface-pin.json)
  --help         show this text

Exit codes: 0 = matches the pin; 1 = divergence (or missing/unreadable file);
            2 = usage/environment error.
EOF
}

FILE="src/conformance/governed-surface-allowlist.json"
PIN="scripts/governed-surface-pin.json"

while [ $# -gt 0 ]; do
  case "$1" in
    --file)    FILE="${2:?--file needs a path}"; shift 2 ;;
    --pin)     PIN="${2:?--pin needs a path}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) printf '%s: unknown arg %q\n' "${SELF%.sh}" "$1" >&2; usage >&2; exit 2 ;;
  esac
done

# Both callers run from anywhere in the repo; defaults are repo-relative. An
# absolute --file/--pin (what the fixtures pass) is unaffected by the cd.
if TOPLEVEL="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$TOPLEVEL"
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf 'check-governed-surface-pin: python3 is required to parse and hash the governed surface and is not on PATH — refusing to report a pass it did not verify\n' >&2
  exit 2
fi

set +e
python3 - "$FILE" "$PIN" >&2 <<'PY'
import hashlib
import json
import sys

FILE, PIN = sys.argv[1], sys.argv[2]

# REQ-054 / AC-041: the recovery denylist is FIVE names. Hardcoded here on
# purpose — checked against both the file and the pin, so a re-pin cannot widen
# it. See the "recovery denylist is five names" rule.
REQ_054 = ["recover", "restore", "repair", "fix", "rebuild"]

LIST_KEYS = ["allowlist", "core", "recovery_denylist"]
CAP = 8  # cap on enumerated member names per difference line

failures = []


def fail(msg):
    failures.append(msg)
    print("FAIL  governed-surface-pin: " + msg)


def die_env(msg):
    print("check-governed-surface-pin: " + msg)
    sys.exit(2)


def show(names):
    shown = ", ".join(sorted(names)[:CAP])
    if len(names) > CAP:
        shown += f" ...and {len(names) - CAP} more"
    return shown


# ---------------------------------------------------------------- the pin ----
# An unusable pin is a broken GATE (exit 2), not a governed-surface divergence.
try:
    with open(PIN, "rb") as fh:
        pin = json.loads(fh.read().decode("utf-8"))
except OSError as exc:
    die_env(f"cannot read the pin {PIN}: {exc} — the gate cannot run, so it refuses to pass")
except Exception as exc:
    die_env(f"the pin {PIN} is not valid JSON: {exc} — the gate cannot run, so it refuses to pass")

if not isinstance(pin, dict):
    die_env(f"the pin {PIN} is not a JSON object")
for key in ["sha256", "git_blob_sha1", "counts"] + LIST_KEYS:
    if key not in pin:
        die_env(f"the pin {PIN} has no {key!r} field — it cannot vouch for anything")
for key in LIST_KEYS:
    if not isinstance(pin[key], list) or not all(isinstance(v, str) for v in pin[key]):
        die_env(f"the pin {PIN}: {key!r} is not a list of strings")
for key in ["sha256", "git_blob_sha1"]:
    if not isinstance(pin[key], str) or not pin[key]:
        die_env(
            f"the pin {PIN}: {key!r} is {pin[key]!r}, not a non-empty string — the pin is MALFORMED "
            "and cannot vouch for any content. Reported as a broken gate (exit 2), not as a surface "
            "divergence (exit 1): a hash that is not a hash never equals the file's, so exit 1 would "
            "send the reader to the HITL to re-sign a surface that may not have moved at all."
        )
if not isinstance(pin["counts"], dict):
    die_env(f"the pin {PIN}: 'counts' is not an object")

# EVERY pinned count must be PRESENT and an integer — a missing one is a hard
# failure, never an implicit skip. Read permissively (dict.get) an absent entry
# came back as None and BOTH count checks below skipped that list, so a re-pin
# could delete a count, add a member to the surface, update the signed member
# list and hashes to match, and still exit 0 — the count backstop that catches a
# lazy re-pin, deleted. bool is excluded explicitly because isinstance(True, int)
# is True in Python and `True == 1` would let a count of `true` masquerade as 1.
for key in LIST_KEYS:
    if key not in pin["counts"]:
        die_env(
            f"the pin {PIN}: 'counts' has no {key!r} entry — the pin is MALFORMED and cannot be "
            f"trusted. counts.{key} is the backstop that catches a re-pin which updates the hashes "
            "and the signed member list but not the counts; if it is absent the gate would silently "
            "stop checking the size of that list altogether, which is how a pass gets bought rather "
            "than earned. DO NOT 'fix' this by regenerating the pin: a pin is only regenerated from "
            "a governed surface the HITL has SIGNED. Restore the pin from git instead."
        )
    declared = pin["counts"][key]
    if isinstance(declared, bool) or not isinstance(declared, int):
        die_env(
            f"the pin {PIN}: counts.{key} is {declared!r}, which is not an integer — the pin is "
            "MALFORMED and cannot be trusted. A non-integer count can be compared neither against "
            "the pinned member list nor against the file, so it would silently weaken this gate to "
            "the same degree as deleting it. DO NOT 'fix' this by regenerating the pin unless the "
            "HITL has signed the surface the new pin would record; restore it from git instead."
        )

# Deliberately permissive, and the ONLY permissive reads left in this file:
# 'pinned_at_commit_short' and 'ac' are prose for the failure message and are not
# part of the predicate, so a pin missing them still gates correctly (it just
# says "pinned at ?"). Every field the predicate actually READS is validated
# above and then indexed strictly, so no check can be skipped by omission.
WHERE = f"pinned at {pin.get('pinned_at_commit_short', '?')} under {pin.get('ac', 'the pre-sign')}"

# (d) on the PIN: a re-pin may not widen or rename the recovery denylist.
if pin["recovery_denylist"] != REQ_054:
    fail(
        f"{PIN} itself declares recovery_denylist {pin['recovery_denylist']!r}, which is not the "
        f"five REQ-054 names {REQ_054!r}. That rule (AC-041) is independently load-bearing and is "
        "not something a re-pin can change: the recovery denylist is five names."
    )

# (c) on the PIN: internally inconsistent pin (lists vs its own counts). Every
# count is present and an int by the validation above, so this never skips.
for key in LIST_KEYS:
    declared = pin["counts"][key]
    if declared != len(pin[key]):
        fail(
            f"{PIN} is internally inconsistent: counts.{key} says {declared} but its {key!r} list "
            f"holds {len(pin[key])} member(s) — a botched re-pin, not a usable signature record."
        )

# --------------------------------------------------------- the pinned file ----
# TC-37 vacuous-pass guard: missing / unreadable is a HARD failure, never a pass.
try:
    with open(FILE, "rb") as fh:
        raw = fh.read()
except OSError as exc:
    fail(
        f"cannot read {FILE}: {exc}. The gate cannot see the surface it vouches for, so it fails "
        "loudly instead of reporting a vacuous ok (TC-37) — and a governed-surface allowlist that "
        "has moved or vanished is itself the largest possible change to the governed surface."
    )
    raw = None

data = None
if raw is not None:
    try:
        data = json.loads(raw.decode("utf-8"))
    except Exception as exc:
        fail(f"{FILE} is not valid UTF-8 JSON: {exc} — it cannot be compared against the pin.")
    else:
        if not isinstance(data, dict):
            fail(f"{FILE} is not a JSON object.")
            data = None

hash_ok = False
if raw is not None:
    # (a) content hash. Both forms: sha256 over the raw bytes, and git's blob
    # sha1 so `git rev-parse <commit>:<path>` reproduces the pin directly.
    got_sha256 = hashlib.sha256(raw).hexdigest()
    got_blob = hashlib.sha1(b"blob %d\0" % len(raw) + raw).hexdigest()
    hash_ok = got_sha256 == pin["sha256"] and got_blob == pin["git_blob_sha1"]
    if not hash_ok:
        fail(
            f"{FILE} content differs from the pin ({WHERE}).\n"
            f"        pinned   sha256 {pin['sha256']}  git-blob {pin['git_blob_sha1']}\n"
            f"        on disk  sha256 {got_sha256}  git-blob {got_blob}"
        )

members_identical = None
if data is not None:
    members_identical = True
    # (b) member lists, element-by-element, so the failure can NAME the change.
    for key in LIST_KEYS:
        got = data.get(key)
        if not isinstance(got, list) or not all(isinstance(v, str) for v in got):
            members_identical = False
            fail(f"{FILE}: {key!r} is missing or is not a list of strings.")
            continue
        want = pin[key]
        if got == want:
            continue
        members_identical = False
        added = [v for v in got if v not in want]
        removed = [v for v in want if v not in got]
        detail = []
        if added:
            detail.append(f"ADDED {show(added)}")
        if removed:
            detail.append(f"REMOVED {show(removed)}")
        if not added and not removed:
            detail.append("REORDERED (same members, different order)")
        fail(
            f"{FILE}: {key!r} diverges from the pin ({WHERE}): "
            + "; ".join(detail)
            + f". Pinned {len(want)} member(s), on disk {len(got)}."
        )

    # (c) counts, asserted against the pin's own counts block as well as (b), so
    # a same-hash-different-meaning or partially-updated pin is still caught.
    for key in LIST_KEYS:
        declared = pin["counts"][key]
        got = data.get(key)
        if not isinstance(got, list):
            # (b) above has ALREADY failed this key by name; counting a non-list
            # would add nothing. This is the one remaining `continue`, and it
            # cannot hide anything: it is unreachable without a recorded failure.
            continue
        if len(got) != declared:
            members_identical = False
            fail(
                f"{FILE}: {key!r} holds {len(got)} member(s) but the pin's counts block says "
                f"{declared} ({WHERE})."
            )

    # (d) on the FILE: REQ-054, independent of what the pin happens to say.
    got_deny = data.get("recovery_denylist")
    if isinstance(got_deny, list) and got_deny != REQ_054:
        members_identical = False
        extra = [v for v in got_deny if v not in REQ_054]
        gone = [v for v in REQ_054 if v not in got_deny]
        what = []
        if extra:
            what.append(f"WIDENED by {show(extra)}")
        if gone:
            what.append(f"DROPPED {show(gone)}")
        if not extra and not gone:
            what.append("REORDERED")
        fail(
            "recovery_denylist is no longer exactly the five REQ-054 names "
            f"{REQ_054!r}: " + "; ".join(what) + ". This rule (AC-041) is independently "
            "load-bearing — the recovery denylist is five names and must not silently widen."
        )

if failures and not hash_ok and members_identical is True:
    print(
        "NOTE  governed-surface-pin: the PARSED members are identical to the pin — this is a "
        "formatting/whitespace-only change. It still fails, deliberately: the pin is a CONTENT "
        "hash over the file's bytes (see this gate's header), because a 'harmless reformat' is "
        "the ideal cover for a member slipped in on an adjacent line."
    )

if failures:
    print(
        "\n"
        "  The governed surface has changed relative to the AC-079 pre-signed pin; this re-opens\n"
        "  the HITL sign-off. The pre-sign covers exactly the pinned content and nothing else, so\n"
        "  a changed surface is by definition unsigned.\n"
        "\n"
        "  DO NOT update the pin to make this pass unless the HITL has signed the new surface.\n"
        f"  Once it is signed — and only then — update {PIN} in the SAME change that carries the\n"
        "  new surface, so the signed member list and the shipped member list land together and a\n"
        "  reviewer can diff them.\n"
        "\n"
        "  Silently re-pinning to make this gate green is the failure mode this gate exists to\n"
        "  prevent.\n"
        "\n"
        "  (0.8.20 Slices 20/25/30 are EXPECTED to trip this. Tripping is CORRECT BEHAVIOUR, not a\n"
        "  bug: this gate is what lets them proceed without a per-slice sign-off, by guaranteeing\n"
        "  that any surface change routes back to the HITL.)"
    )
    sys.exit(1)

print(
    f"ok    governed-surface-pin: {FILE} matches the {pin.get('ac', 'pre-signed')} pin "
    f"({pin['counts']['allowlist']} allowlist / {pin['counts']['core']} core / "
    f"{pin['counts']['recovery_denylist']} recovery_denylist, {WHERE})"
)
PY
RC=$?
set -e

exit "$RC"
