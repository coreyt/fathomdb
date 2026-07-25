#!/usr/bin/env bash
# scripts/tests/lib/governed-surface-fixture.sh — seeds a throwaway fixture repo
# with a governed-surface allowlist + pin that are CONSISTENT with each other, so
# `scripts/preflight.sh --landing` can be exercised inside a fixture.
#
# WHY THIS EXISTS. DOC-HYGIENE-2 T1e wired scripts/check-governed-surface-pin.sh
# into `preflight.sh --landing` (§9). Every fixture repo that runs `--landing`
# and does NOT carry a pin therefore hard-fails:
#
#   INFO  governed-surface-pin: cannot read the pin scripts/governed-surface-pin.json:
#         [Errno 2] No such file or directory — the gate cannot run, so it refuses to pass
#   HARD  governed-surface-pin: check-governed-surface-pin.sh exited 2 without reporting
#         a specific defect — refusing to certify this tree for landing
#
# THE GATE IS RIGHT AND IS NOT WEAKENED HERE. Refusing to certify a tree whose
# pin it cannot read is exactly the TC-37 anti-vacuity stance: a gate that cannot
# see its subject must never report green. What was incomplete is the FIXTURES —
# they did not model a real checkout. This is the same repair T1b made when its
# ledger-integrity gate started hard-failing the same fixtures: give each fixture
# builder a minimal, consistent instance of what the new gate reads. No bypass,
# no env escape hatch, no `--skip`, no conditional that makes the gate inert.
#
# SYNTHETIC, NOT A COPY OF THE REAL PAIR — DELIBERATE. The pin's own suite
# (scripts/tests/test_check_governed_surface_pin.sh) copies the REAL
# src/conformance/governed-surface-allowlist.json + scripts/governed-surface-pin.json
# into its fixtures, and must: the real pair IS its subject. The three suites that
# use THIS helper are about board currency, ledger integrity and TC-RUBRIC-5 — the
# governed surface is incidental to all of them. Copying the real pair would couple
# them to the real pin's SIGNING STATE, and that state is expected to diverge: the
# gate's own header records that 0.8.20 Slices 20/25/30 are EXPECTED to trip it
# before a fresh HITL sign-off. On that day a legitimate, correct trip would turn
# three unrelated suites red for a reason that has nothing to do with what they
# test. A self-consistent synthetic pair is immune to that, and still exercises the
# gate for the RIGHT reason: it runs, reads both files, and matches.
#
# WHAT THE GATE REQUIRES OF THE PAIR (see check-governed-surface-pin.sh's header):
#   * the pin carries sha256 + git_blob_sha1 of the allowlist file's RAW BYTES;
#   * the pin's allowlist/core/recovery_denylist lists equal the file's, element
#     for element and in order;
#   * counts.<list> is present, an integer, and equals len(<list>) in BOTH;
#   * recovery_denylist is EXACTLY the five REQ-054 names, in BOTH.
# All four are satisfied by construction below and then VERIFIED by running the
# real gate against the pair (see seed_governed_surface_fixture).
#
# Usage — from a fixture builder, BEFORE its `git add -A`:
#   . "$SCRIPT_DIR/lib/governed-surface-fixture.sh"
#   seed_governed_surface_fixture "$primary"
# The caller commits the files; a linked worktree added afterwards inherits them.

# seed_governed_surface_fixture <repo-dir>
# Writes <repo>/src/conformance/governed-surface-allowlist.json and
# <repo>/scripts/governed-surface-pin.json, then proves the pair passes the real
# gate. Returns non-zero (and says why) if it cannot — a fixture seeder that
# silently produced an unusable pair would just move the confusion downstream.
seed_governed_surface_fixture() {
  local repo="${1:?seed_governed_surface_fixture needs a repo dir}"
  local lib_dir repo_root
  lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  repo_root="$(cd "$lib_dir/../../.." && pwd)"

  if ! command -v python3 >/dev/null 2>&1; then
    printf 'seed_governed_surface_fixture: python3 is required to hash the fixture surface\n' >&2
    return 2
  fi

  mkdir -p "$repo/src/conformance" "$repo/scripts"

  python3 - "$repo" <<'PY' || return 1
import hashlib
import json
import os
import sys

repo = sys.argv[1]

# REQ-054 / AC-041: the recovery denylist is FIVE names, and the gate checks this
# against a constant of its own in BOTH the file and the pin. A fixture that got
# it wrong would fail for a reason that has nothing to do with the suite using it.
REQ_054 = ["recover", "restore", "repair", "fix", "rebuild"]

surface = {
    "_comment": (
        "FIXTURE governed surface — minimal and synthetic. Not the real "
        "src/conformance/governed-surface-allowlist.json and not a statement about it."
    ),
    "allowlist": ["fixture_verb_one", "fixture_verb_two"],
    "core": ["fixture_verb_one"],
    "recovery_denylist": REQ_054,
}
raw = (json.dumps(surface, indent=2) + "\n").encode("utf-8")

file_path = os.path.join(repo, "src/conformance/governed-surface-allowlist.json")
with open(file_path, "wb") as fh:
    fh.write(raw)

LIST_KEYS = ["allowlist", "core", "recovery_denylist"]
pin = {
    "_comment": (
        "FIXTURE pin over the fixture surface next to it. NOT a HITL sign-off and not "
        "a copy of scripts/governed-surface-pin.json."
    ),
    "ac": "fixture pin (no HITL sign-off is implied)",
    "pinned_at_commit_short": "fixture0",
    # Hashed over the bytes just written, so the pair cannot drift apart.
    "sha256": hashlib.sha256(raw).hexdigest(),
    "git_blob_sha1": hashlib.sha1(b"blob %d\0" % len(raw) + raw).hexdigest(),
    "counts": {k: len(surface[k]) for k in LIST_KEYS},
}
for k in LIST_KEYS:
    pin[k] = surface[k]

with open(os.path.join(repo, "scripts/governed-surface-pin.json"), "w", encoding="utf-8") as fh:
    json.dump(pin, fh, indent=2)
    fh.write("\n")
PY

  # Self-check: run the REAL gate against the pair just written. If the gate's
  # predicate ever gains a requirement this seeder does not satisfy, the failure
  # surfaces HERE, naming this helper, instead of as a baffling `--landing`
  # failure in three unrelated suites.
  local gate="$repo_root/scripts/check-governed-surface-pin.sh"
  local out
  if [ ! -f "$gate" ]; then
    printf 'seed_governed_surface_fixture: %s is missing — cannot verify the seeded pair\n' "$gate" >&2
    return 2
  fi
  if ! out="$(bash "$gate" \
        --file "$repo/src/conformance/governed-surface-allowlist.json" \
        --pin "$repo/scripts/governed-surface-pin.json" 2>&1)"; then
    printf 'seed_governed_surface_fixture: the seeded pair does NOT satisfy check-governed-surface-pin.sh:\n%s\n' "$out" >&2
    return 1
  fi
}
