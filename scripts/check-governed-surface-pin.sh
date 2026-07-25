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
#   The HITL PRE-SIGNED the accumulated governed-surface delta of 0.8.20 Slices
#   5d + 10b + 15b + 15d (AC-079) — but pinned to the EXACT CONTENT of
#   src/conformance/governed-surface-allowlist.json as of commit 427d2712:
#   30 allowlist members, 5 core, and recovery_denylist unchanged at the five
#   REQ-054 names. A pre-sign keyed to specific content is worth exactly as much
#   as the mechanism that notices when that content moves. This is that
#   mechanism: the signed content is recorded in scripts/governed-surface-pin.json
#   and this gate HARD-fails the moment the file diverges from it, routing the
#   reader back to the HITL for a fresh sign-off.
#
# TRIPPING THIS GATE IS CORRECT BEHAVIOUR, NOT A BUG. It is precisely what lets
# 0.8.20 Slices 20 / 25 / 30 proceed WITHOUT stopping for a per-slice sign-off:
# the pre-sign covers the pinned content, and anything else re-opens it. Slice 20
# is EXPECTED to trip this. Re-pinning to make the gate green without a fresh
# HITL sign-off is the failure mode the gate exists to prevent.
#
# PREDICATE — the pinned file must match the pin on ALL of:
#   (a) CONTENT HASH: sha256 and git blob sha1 of the file's raw bytes equal the
#       values recorded in the pin. Both are recorded so a reviewer can
#       reproduce the pin either way (`git rev-parse 427d2712:<file>` gives the
#       blob sha1 directly).
#   (b) MEMBER LISTS: `allowlist`, `core` and `recovery_denylist` are compared
#       element-by-element against the copies stored in the pin, so the failure
#       can name WHICH member appeared or vanished — and so that updating only
#       the hash in the pin (a lazy re-pin, to silence the gate) still fails.
#   (c) COUNTS: 30 / 5 / 5, asserted separately from (b) against the pin's own
#       `counts` block, so a pin whose lists and counts disagree is caught as an
#       internally inconsistent (botched) re-pin rather than being trusted.
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
#             2 = usage error, unreadable/unparseable pin file, or python3 absent
#                 (the gate could not run — never reported as a pass).
set -euo pipefail

SELF="$(basename "${BASH_SOURCE[0]}")"

usage() {
  cat <<EOF
Usage: scripts/$SELF [--file <path>] [--pin <path>]

Fails when src/conformance/governed-surface-allowlist.json diverges from the
AC-079 pre-signed pin recorded in scripts/governed-surface-pin.json (pinned to
the file's content as of commit 427d2712). See the header of this script for the
full predicate and for why re-pinning without a fresh HITL sign-off is forbidden.

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
if not isinstance(pin["counts"], dict):
    die_env(f"the pin {PIN}: 'counts' is not an object")

WHERE = f"pinned at {pin.get('pinned_at_commit_short', '?')} under {pin.get('ac', 'the pre-sign')}"

# (d) on the PIN: a re-pin may not widen or rename the recovery denylist.
if pin["recovery_denylist"] != REQ_054:
    fail(
        f"{PIN} itself declares recovery_denylist {pin['recovery_denylist']!r}, which is not the "
        f"five REQ-054 names {REQ_054!r}. That rule (AC-041) is independently load-bearing and is "
        "not something a re-pin can change: the recovery denylist is five names."
    )

# (c) on the PIN: internally inconsistent pin (lists vs its own counts).
for key in LIST_KEYS:
    declared = pin["counts"].get(key)
    if declared is not None and declared != len(pin[key]):
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
        declared = pin["counts"].get(key)
        got = data.get(key)
        if declared is None or not isinstance(got, list):
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
    f"({pin['counts'].get('allowlist')} allowlist / {pin['counts'].get('core')} core / "
    f"{pin['counts'].get('recovery_denylist')} recovery_denylist, {WHERE})"
)
PY
RC=$?
set -e

exit "$RC"
