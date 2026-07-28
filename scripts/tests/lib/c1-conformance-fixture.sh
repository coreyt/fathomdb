#!/usr/bin/env bash
# scripts/tests/lib/c1-conformance-fixture.sh — seeds a throwaway fixture repo
# with everything scripts/check-c1-conformance.sh reads, so
# `scripts/preflight.sh --landing` can be exercised inside a fixture.
#
# WHY THIS EXISTS. 0.8.20 Slice 30 (R-20-H7) wired
# scripts/check-c1-conformance.sh into `preflight.sh --landing` (§10). Every
# fixture repo that runs `--landing` and does NOT carry the C-1 contract, its pin
# and the sources the clause assertions read therefore hard-fails:
#
#   INFO  c1-contract-conformance: cannot read the pin scripts/c1-conformance-pin.json:
#         [Errno 2] No such file or directory — the gate cannot run, so it refuses to pass
#   HARD  c1-contract-conformance: check-c1-conformance.sh exited 2 without reporting
#         a specific defect — refusing to certify this tree for landing
#
# THE GATE IS RIGHT AND IS NOT WEAKENED HERE. Refusing to certify a tree whose
# subject it cannot see is exactly the TC-37 anti-vacuity stance. What was
# incomplete is the FIXTURES — they did not model a real checkout. This is the
# same repair T1b made for ledger-integrity and T1e made for the governed-surface
# pin (see lib/governed-surface-fixture.sh): give each fixture builder a minimal,
# consistent instance of what the new gate reads. No bypass, no env escape hatch,
# no `--skip`, no conditional that makes the gate inert.
#
# ⚠ THIS SEEDER COPIES THE REAL ARTIFACTS. That is a DELIBERATE DEPARTURE from
# lib/governed-surface-fixture.sh, which builds a SYNTHETIC pair on purpose so
# three unrelated suites are not coupled to the real pin's signing state. A
# synthetic C-1 pair is not merely undesirable here, it is IMPOSSIBLE:
# check-c1-conformance.sh's implemented-assertion set is a constant inside the
# script, and its registry bijection is deliberately rigid in BOTH directions, so
# any pin that did not register exactly those 26 CHECKABLE ids would be reported
# as MALFORMED (exit 2) — which is the very failure this seeder exists to remove.
# A synthetic contract is impossible for the same reason: the pin is a content
# hash over the real document's bytes.
#
# THE COUPLING THAT CREATES IS ACCEPTABLE, AND HERE IS WHY. The governed-surface
# pin is EXPECTED to trip during 0.8.20 (its own header says so), so coupling
# unrelated suites to it would turn them red for a correct, unrelated reason. The
# C-1 pin is NOT expected to trip: it moves only when the ratified contract is
# amended or when as-built code drifts away from it — and in either case the tree
# genuinely does not satisfy R-20-H7, `--landing` is genuinely blocked for every
# slice, and a red in these suites is telling the truth rather than obscuring it.
# The self-check at the bottom makes that failure name THIS helper.
#
# Usage — from a fixture builder, BEFORE its `git add -A`:
#   . "$SCRIPT_DIR/lib/c1-conformance-fixture.sh"
#   seed_c1_conformance_fixture "$primary"
# The caller commits the files; a linked worktree added afterwards inherits them.

# seed_c1_conformance_fixture <repo-dir>
# Copies the ratified contract, its pin, and every file/tree the gate's own
# --list-sources manifest names, then proves the result passes the real gate.
# Returns non-zero (and says why) if it cannot — a fixture seeder that silently
# produced an unusable tree would just move the confusion downstream.
#
# The source list comes from `--list-sources` rather than from a hand-maintained
# path list here, so a clause added later cannot leave this seeder silently stale
# (a stale seeder would re-open the exact hard-fail it exists to prevent).
seed_c1_conformance_fixture() {
  local repo="${1:?seed_c1_conformance_fixture needs a repo dir}"
  local lib_dir repo_root gate contract pin manifest kind path
  lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  repo_root="$(cd "$lib_dir/../../.." && pwd)"
  gate="$repo_root/scripts/check-c1-conformance.sh"
  contract="dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md"
  pin="scripts/c1-conformance-pin.json"

  if [ ! -f "$gate" ]; then
    printf 'seed_c1_conformance_fixture: %s is missing — cannot seed or verify\n' "$gate" >&2
    return 2
  fi
  if ! manifest="$(bash "$gate" --list-sources 2>&1)"; then
    printf 'seed_c1_conformance_fixture: --list-sources failed:\n%s\n' "$manifest" >&2
    return 2
  fi

  mkdir -p "$repo/$(dirname "$contract")" "$repo/$(dirname "$pin")"
  cp "$repo_root/$contract" "$repo/$contract" || return 1
  cp "$repo_root/$pin" "$repo/$pin" || return 1

  while IFS=$'\t' read -r kind path; do
    [ -n "${kind:-}" ] || continue
    case "$kind" in
      file)
        mkdir -p "$repo/$(dirname "$path")"
        cp "$repo_root/$path" "$repo/$path" || return 1
        ;;
      tree)
        mkdir -p "$repo/$path"
        ;;
    esac
  done <<<"$manifest"

  # Self-check: run the REAL gate against the seeded tree. If the gate's
  # predicate ever gains a requirement this seeder does not satisfy, the failure
  # surfaces HERE, naming this helper, instead of as a baffling `--landing`
  # failure in four unrelated suites.
  local out
  if ! out="$(bash "$gate" --contract "$repo/$contract" --pin "$repo/$pin" --root "$repo" 2>&1)"; then
    printf 'seed_c1_conformance_fixture: the seeded tree does NOT satisfy check-c1-conformance.sh:\n%s\n' "$out" >&2
    return 1
  fi
}
