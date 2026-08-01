# Steward session hand-off — 2026-08-01-A

**Supersedes `STEWARD-SESSION-HANDOFF-2026-07-31-B`.**

## ★ IMMEDIATE NEXT STEP

**Goal: bring Slice 40 to publish-readiness, not publish.** On the private
Linux-first BASE branch, first obtain a genuinely green supported-platform local
gate; then push and open one PR, and use ordinary GitHub CI to prove B5, B9, and
the Linux TC-91 acceptance. Do **not** tag, dispatch non-dry-run, publish, or
edit release state as if Slice 40 had landed.

The combined BASE exists at private head **`5e6cade2`** on
`slice-40-e1-base`, cut from `origin/main` **`23413eb7`**. `origin/main` has
**no Slice-40 source change**. It integrates B1, B2, B3, B5, B6, B7, B8+B8b,
B9, B10, and Linux-first scope; B4 remains cancelled by HITL `seq-234`.

## 1. Verified position

- Combined integration review: **PASS**, first at `e262dceb`; the two findings
  were repaired and re-reviewed **PASS** at `3ea8354c`:
  - Linux-first actionlint fixture requires `linux-x64-gnu` and rejects the
    three deferred macOS/Windows labels.
  - B3's active transaction comments now correctly state that
    `commit_projection_outcomes` takes `BEGIN IMMEDIATE` before reads.
- A later real regression in `steward-orient` was found by the aggregate test:
  current worktree topology exceeded the approved 5120-byte cap. `5e6cade2`
  compresses only redundant worktree path presentation and reduces already
  truncated ledger previews from 96 to 80 characters. Independent review:
  **PASS**; `test_steward_orient.sh` passes at **5119/5120** bytes.
- Verified green in the BASE worktree:
  - `scripts/agent-lint.sh` rc=0
  - `scripts/agent-typecheck.sh` rc=0
  - `cargo clippy --workspace --all-targets -- -D warnings -A missing-docs`
    rc=0 (only existing Cargo license/license-file warnings)
  - `cargo check --workspace --all-targets` rc=0
  - focused Linux-first/release workflow tests, `actionlint`, B5's focused
    fixture, and `test_steward_orient.sh` rc=0.
- **Not yet green:** the full local `agent-verify`/`agent-test` result. The
  sandboxed aggregate observed `ptrace` denial, loopback fixture-bind denial,
  npm/pip home-cache write denial, and DNS denial; it was stopped after finding
  and fixing the real orient regression. Those environmental outcomes are not
  evidence of a green supported-platform local gate.

## 2. Actual remaining work — in required order

1. **Finish local supported-Linux verification.** Run the complete local gate
   with the normal permissions it requires (including security `ptrace`,
   loopback fixtures, and package caches). Capture every suite's exit code. Fix
   any real red; do not classify an infrastructure denial as green.
2. **E1 receipt and pre-push review.** Record the combined commit set, exact
   local results, the two post-integration fixes, and the independent PASS
   reviews. The Steward accepts this receipt only when item 1 is green.
3. **One BASE PR, not a branch-only push.** Push `slice-40-e1-base`, open a PR,
   and observe its GitHub CI. `ci.yml` does not run on arbitrary branch pushes.
   This PR is the only valid start for the CI-only evidence below.
4. **Make supported-platform CI green.** On Linux, require:
   - B9: `commission-manifest` green; this is its only proof because its
     shallow-clone defect cannot be reproduced locally.
   - B5: 60 consecutive isolated-node greens and 3 consecutive whole-file
     greens, each with the immediate pytest `rc` recorded; `rc=135` fails.
     The rejected bespoke B5 CI/source guard remains forbidden.
   - B3/TC-91: five consecutive relevant Linux `verify` greens that reach the
     test stage and execute the non-ignored TC-91 gate. Any relevant non-flake
     commit resets this accrual.
   - all other executed Linux jobs green. macOS/Windows N-API jobs and artifacts
     are intentionally out of 0.8.20 scope and defer to 0.8.22 (`seq-234`).
5. **Absorb CI findings in BASE.** Re-run local gates and independent review for
   each correction; re-open/re-observe the PR and reset the TC-91 sequence when
   required. Do not advance to a release phase on a CI bootstrap-only green.
6. **After BASE CI proof, finish checkpoints A–F.** Execute the already ruled
   Phase 1–6 requirements (manifests, local dry-run, parity/smokes, workspace
   gate, AC-079/080 mint, cut obligations), land by the approved route, then
   observe the landing CI and one `dry_run=true` rehearsal. Stop before any tag.

## 3. Boundaries and decisions

- The release is **Linux x86_64 native only**. Cargo source crates are not
  platform-excluded. B4's macOS/Windows patch must not be integrated.
- Strong normal CI evidence is sufficient for B5 (HITL `seq-237`). The former
  appliance policy is superseded; the failed bespoke B5 CI/source route is
  permanently out of scope for this release.
- The only unruled, run-halting decision remains **PUBLISH**. Publish-readiness
  does not authorize a tag, registry action, or non-dry-run release dispatch.
- `preflight.sh --expect-closed 30` currently false-negatives against the
  generated plan roll-up despite Slice 30 being `LANDED` in release state and
  the board. Do not bypass that guard for a new implementer; repair its witness
  recognition under a separately reviewed, RED-first change if a new spawn is
  needed.

## 4. Cold-start checks

1. Run `scripts/steward-orient.sh`; this file must be the newest handoff.
2. Verify `origin/main` and `slice-40-e1-base` heads from git; do not trust this
   narration over the commits.
3. Read `dev/plans/runs/0.8.20-slice-40-commission-brief.md` §§4.1–4.9 and §11
   before claiming any BASE or CI condition complete.
4. Preserve the shared checkout's user-owned dirt. Work only in isolated
   worktrees; never install or rebuild the Python binding from one.
