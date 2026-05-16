# Phase 11c-fix-1 — Reviewer remediation pass

Targeted fix for the three codex `gpt-5.4` findings on Phase 11c
(verdict `BLOCK`, see `dev/plans/runs/11c-review-20260516T223403Z.md`).

Operates in the **existing 11c worktree**
`/tmp/fdb-11c-set-version-20260516T221655Z` on branch
`phase-11c-set-version-20260516T221655Z`. Builds new commits on top
of `d408361`.

## Model + effort

Opus 4.7, intent: medium. Spawn from main thread:

```bash
PHASE=11c-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-11c-set-version-20260516T221655Z
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Use
--disallowedTools Task Agent as a hard guard. Write code, run tests,
commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/11c-fix-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/runs/11c-review-20260516T223403Z.md` — reviewer verdict.
- `dev/plans/prompts/11c-set-version.md` — original spec, esp. § 4
  (skew fixtures) and § 2 (`--check-files` diagnostic contract).
- `dev/acceptance.md` AC-051a (line 823) — the assertion text you
  must satisfy in shape, not just spirit.
- `dev/release/fixtures/pip-skew/` (already landed) — read this
  whole fixture. Finding 1's solution is the **cargo analogue** of
  what pip-skew did. Use the same naming pattern (`mock-skew-*`).

## Scope — three findings, one commit per finding is fine

### Finding 1 (`high`) — Real sibling-skew shape for AC-051a

The current cargo-skew fixture
(`dev/release/fixtures/cargo-skew/Cargo.toml`) directly pins
`fathomdb-embedder-api = "=99.99.99"`. That proves direct-pin
rejection, NOT what AC-051a actually requires.

**What AC-051a actually requires (from `dev/acceptance.md:823`):**

> A Cargo.toml requesting `fathomdb = X` and `fathomdb-embedder = Y`
> whose `fathomdb-embedder-api` ranges do not overlap fails
> `cargo update` with a resolver error.

The skew is **transitive** between two sibling consumer crates
through their shared dep on `fathomdb-embedder-api`. The fixture
needs three synthetic crates that mirror the real release shape.

**Required structure:**

`dev/release/fixtures/cargo-skew/` (rewrite):

```text
cargo-skew/
├── Cargo.toml             # workspace + probe consumer
├── mock-skew-api/         # analogue of fathomdb-embedder-api
│   ├── Cargo.toml         # name = "mock-skew-api", version = "1.0.0"
│   └── src/lib.rs         # empty pub trait MockApi {}
├── mock-skew-consumer-a/  # analogue of fathomdb
│   ├── Cargo.toml         # name = "mock-skew-consumer-a"
│   │                      # version = "1.0.0"
│   │                      # [dependencies] mock-skew-api = { path = "../mock-skew-api", version = "^1" }
│   └── src/lib.rs         # empty
├── mock-skew-consumer-b/  # analogue of fathomdb-embedder
│   ├── Cargo.toml         # name = "mock-skew-consumer-b"
│   │                      # version = "1.0.0"
│   │                      # [dependencies] mock-skew-api = { path = "../mock-skew-api-v2", version = "^2" }
│   └── src/lib.rs         # empty
└── mock-skew-api-v2/      # second copy of mock-skew-api at version 2.0.0
    ├── Cargo.toml         # name = "mock-skew-api", version = "2.0.0"
    └── src/lib.rs         # empty pub trait MockApi {}
```

`cargo-skew/Cargo.toml` (the probe consumer at the root) depends on
both consumer crates by path:

```toml
[package]
name = "cargo-skew-probe"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
mock-skew-consumer-a = { path = "./mock-skew-consumer-a" }
mock-skew-consumer-b = { path = "./mock-skew-consumer-b" }
```

Resolver outcome: probe transitively requires `mock-skew-api^1` (via
consumer-a) AND `mock-skew-api^2` (via consumer-b). With both
crates declared as path deps to the same package name at
incompatible major versions, `cargo update` (or `cargo check`) MUST
fail with a resolver error citing `mock-skew-api`. That is the
sibling-package transitive-skew shape AC-051a wants.

**Verification path** (the cleanest one likely to actually fail at
resolve time):

Some cargo versions treat two path-deps to same-name different-version
crates as "different sources" rather than as a resolver conflict.
If the path-dep approach does not produce a resolver error in your
local cargo, fall back to the canonical approach: use `[patch.crates-io]`
to inject conflicting sources, OR publish the three mock crates to a
local registry (e.g. `cargo-local-registry` or `cargo-publish` to a
tempdir) and depend on them by version. Whichever path you take,
the SUCCESS CRITERION is: a non-zero `cargo update` exit with stderr
naming the `mock-skew-api` version conflict.

If none of these work without substrate not in this slice (e.g.
installing a registry tool that bootstrap doesn't have), STOP and
write a blocker report. Do NOT silently ship a fixture that doesn't
actually exercise the resolver.

`dev/release/tests/cargo_skew.sh` (rewrite):

- Run `cargo update` (or `cargo check`) inside
  `dev/release/fixtures/cargo-skew/`.
- Assert exit non-zero.
- Assert stderr contains `mock-skew-api` (the conflicting package
  name) — proving the resolver named the actual conflict, not some
  unrelated error.
- Print `PASS: AC-051a — cargo resolver detected mock-skew-api
sibling skew`.

Add a one-line note in
`dev/release/fixtures/cargo-skew/Cargo.toml` (or a sibling README):
"Synthetic mock-skew-\* names mirror the AC-051a shape
(`fathomdb` + `fathomdb-embedder` skewing on `fathomdb-embedder-api`).
Real-name swap deferred until REQ-048 publishing lands."

### Finding 2 (`medium`) — `--check-files` diagnostic format

`scripts/set-version.sh` (lines 202, 207, 213, 243, 247): every
drift-emit site must produce a structured one-line error of the form:

```text
<path>:<line>: version drift — observed "<observed>", expected "<expected>"
```

For files where a line number is not naturally available (e.g.
inferred from a grep match), include the line number of the version
declaration that was checked. Example output:

```text
src/python/pyproject.toml:7: version drift — observed "0.6.1", expected "0.6.0"
src/ts/package.json:3: version drift — observed "0.6.1", expected "0.6.0"
src/rust/crates/fathomdb-engine/Cargo.toml:7: version drift — observed inheritance, expected `version.workspace = true`
```

Approach:

- For TOML manifests (Cargo.toml, pyproject.toml): `grep -n
'^version' <file>` returns `<line>:<content>`; parse the line
  number from that.
- For JSON (package.json): `grep -n '"version"' <file>` works
  identically since the field is on its own line in our package.json.
- Centralize a `_drift_line(file, observed, expected)` helper so all
  drift sites emit the same shape.

`scripts/tests/test_set_version.sh` (line 136 and any other
drift-coverage cases): tighten the assertions. Each drift-detection
test must assert:

1. The output contains `<file>:<line>:` (the file:line prefix).
2. The output contains `observed "<observed-value>"`.
3. The output contains `expected "<expected-value>"`.

Add at least one new test case per drift type (Axis W version
mismatch, Axis E inheritance-regression, missing Axis W
`version.workspace = true` on a non-embedder-api crate) so each
diagnostic shape is independently exercised.

### Finding 3 (`low`) — Regenerate `output.json`

`dev/plans/runs/11c-set-version-output.json` currently records
`head_sha: "3cf95ca…"` and omits the final docs commit. After this
remediation pass:

- Update `head_sha` to the HEAD of this fix-1 work (your final
  commit on the branch).
- Update `commits` to list the original four + your new fix-1
  commits in chronological order.
- Add a `remediation_findings_addressed` array describing the three
  fixes by short label.

## Required commands

```bash
cd /tmp/fdb-11c-set-version-20260516T221655Z
# Workspace still resolves with the new fixture present (fixture is
# outside the workspace; cargo check at the repo root must remain green).
cargo check --workspace
# Set-version unit tests with the new diagnostic assertions.
bash scripts/tests/test_set_version.sh
# AC-051a synthetic-skew fixture must produce a resolver failure.
bash dev/release/tests/cargo_skew.sh
# Pip-skew fixture (unchanged from 11c) still passes.
bash dev/release/tests/pip_skew.sh
# Canonical local gate.
./scripts/agent-verify.sh
```

All must pass. If `agent-verify.sh` flakes on
`ac_029_canonical_writes_complete_under_projection_stall`,
`ac_017_vector_projection_freshness_p99_le_five_seconds`, or
`t_safe_export_engine_error_exits_export_failure_66`, rerun once —
all are pre-existing host-load timing flakes unrelated to 11c.

## Discipline

- TDD: each new diagnostic-shape assertion lands as a failing test
  before the diagnostic shape is implemented.
- The fixture rewrite is NOT a one-line tweak; it's a structural
  change. Read the pip-skew fixture in its entirety before designing
  the cargo analogue — they should look structurally similar.
- No scope creep into 11d (`release.yml`).
- Comment policy: no WHAT comments, only non-obvious WHY. No
  "fixed in 11c-fix-1" markers. The mock-skew-api real-name swap
  note IS non-obvious WHY and is intentional.

## Blockers — surface before writing code

If the synthetic-skew fixture cannot produce a real `cargo update`
resolver failure without installing a tool not in the current
bootstrap (`cargo-local-registry`, custom registry server, etc.),
STOP and write a blocker report. Document:

- Each approach tried (path deps, `[patch.crates-io]`, local
  registry) and the specific failure mode of each.
- The substrate that would be required to make AC-051a's real
  shape work locally.
- A recommendation: defer AC-051a real-shape coverage to the same
  follow-up that REQ-048 publishing lands, with the fixture
  rewritten as scaffolding that documents the intent.

Blocker report shape: same as 10b-B
(`dev/plans/runs/10b-B-purge-restore-output.json`).

## Output

After all commands pass, write
`dev/plans/runs/11c-fix-1-output.json`:

```json
{
  "phase": "11c-fix-1",
  "baseline_sha": "d408361",
  "branch": "phase-11c-set-version-20260516T221655Z",
  "head_sha": "<HEAD after final commit>",
  "findings_addressed": [
    "1 [high]: cargo-skew fixture rewritten as mock-skew-api + mock-skew-consumer-a/b shape; cargo_skew.sh asserts resolver names mock-skew-api conflict",
    "2 [medium]: set-version.sh --check-files emits <file>:<line>: version drift — observed \"X\", expected \"Y\" at every drift site; test suite asserts the structured shape",
    "3 [low]: output.json regenerated with correct head_sha + post-fix commit list"
  ],
  "cargo_skew_fixture_approach": "<path-deps | patch.crates-io | local-registry>",
  "tests_tightened": ["<test names>"],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for clean PASS"
}
```

Then stop. Do not advance to 11d. Do not run the reviewer yourself.
