# Phase 12-RC1-WF-FIX-1 — RESUME 3 (code-reviewer fixups)

Resume from HEAD `4d91653` in worktree
`/tmp/fdb-12-RC1-WF-FIX-1-20260518T031153Z`. Three small fixups
from code-reviewer pass. Single commit on top.

## Fixup 1 — CHANGELOG.md:19 factual error

File `CHANGELOG.md` line ~19 currently reads:

```text
Axis W bumped to `0.6.0-rc.1`; Axis E (`fathomdb-embedder-api`)
unchanged at `0.6.0` (trait surface stable since lock).
```

This is wrong post-`4d91653`. Replace with text matching the new
reality:

- Axis E joined Axis W lockstep at `0.6.0-rc.1` for the RC1
  bootstrap-publish (`dev/design/release.md` § RC1 bootstrap
  publish).
- Trait surface remains stable; the bump is a registry-seeding
  mechanic, not an API change.
- Axis-E independence resumes at/after 0.6.0 GA.

Keep prose tight — 1-2 sentences. Match surrounding CHANGELOG
style.

## Fixup 2 — bootstrap script idempotency comment tightening

File `scripts/release/publish-rc1-bootstrap.sh:22-23`.

Comment says "Idempotent: skip if already on crates.io at
RC_VERSION." This is only true on first re-run. If a later
version (e.g. rc.2) lands on crates.io, `cargo search --limit 1`
returns the newer version and the grep misses — the script then
attempts `cargo publish` of rc.1, which hard-fails "already
uploaded".

Two options — pick the simpler:

(a) **Tighten the check.** Replace `cargo search` (latest-only) with
something that enumerates all published versions and greps for
RC_VERSION exactly. Use crates.io's index API or `cargo info`
(cargo 1.78+) — pick whichever is available in the workflow's
toolchain pinned by `rust-toolchain.toml`. If neither is viable,
fall back to a `curl` against the sparse index
(`https://index.crates.io/<prefix>/<crate>`) and grep the JSONL
for `"vers":"0.6.0-rc.1"`.

(b) **Weaken the comment.** Change to: "Skip if rc.1 is still the
latest version on crates.io. After a later version lands, this
check no longer catches rc.1 as published and re-run will hard-fail
at cargo publish — by design, since re-running this script
post-rc.2 is operator error."

Recommend **(a)** with sparse-index curl — it's ~3 lines of bash
and is precise. The bootstrap script is operator-critical; "by
design hard-fail" is footgunny.

Sparse-index URL convention:
`https://index.crates.io/<lo>/<wn>/<crate>` where `<lo>/<wn>` is
the standard cargo registry path prefix (for short names: 3/
prefix; for longer: first-2/second-2; for `fathomdb-embedder-api`
use `fa/th/fathomdb-embedder-api`). Each line is a JSON object;
grep for `"vers":"0.6.0-rc.1"`.

## Fixup 3 — stale comment

File `scripts/tests/test_assert_co_tagging.sh:72-73`.

Comment says "0.6.0 (Axis E read from ... Cargo.toml)". Axis-E
is now `0.6.0-rc.1` in the manifest. Update the comment literal
to match. Test logic is correct, only the comment is stale.

## Required commands

```bash
cd /tmp/fdb-12-RC1-WF-FIX-1-20260518T031153Z
bash scripts/tests/test_actionlint_fixture.sh
bash scripts/tests/test_assert_co_tagging.sh
bash scripts/tests/test_verify_release_gates.sh
```

If you change the bootstrap script (Fixup 2 option a), also:

```bash
bash -n scripts/release/publish-rc1-bootstrap.sh
shellcheck scripts/release/publish-rc1-bootstrap.sh   # if installed
```

Full `agent-verify.sh` not required — none of these touch
build/test surface.

## Commit

Single commit:

```text
fix(release): code-reviewer fixups for bootstrap-publish slice

- CHANGELOG: correct Axis-E version after rc.1 bootstrap join
- bootstrap script: precise sparse-index idempotency check
  (was latest-only via cargo search)
- test_assert_co_tagging.sh: update stale Axis-E comment literal
```

(Adjust bullet 2 if you picked option (b).)

## Output

Append to `dev/plans/runs/12-RC1-WF-FIX-1-resume3-output.json`:

```json
{
  "phase": "12-RC1-WF-FIX-1-resume3",
  "parent_commit": "4d91653",
  "head_sha": "<new HEAD>",
  "commits": ["<sha>: <subject>"],
  "fixups_landed": ["1 CHANGELOG", "2 bootstrap idempotency (option a|b)", "3 stale comment"],
  "commands_run": {...},
  "blockers_encountered": []
}
```

Hard constraints unchanged: no remote push, no workflow_dispatch,
no tag push, no cargo publish, no script execution, no agent
spawn, stay in this worktree.

Stop after output.json written.
