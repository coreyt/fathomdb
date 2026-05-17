# Phase 12-S — Security fixtures (AC-036, AC-037, AC-038, AC-050a, AC-050c)

Phase 12 Wave 1 slice (parallel with 12-B). Lands the five security
acceptance gates that close the security-fixture portion of the
Path-to-Client-Ready sequence (per
`dev/plans/0.6.0-implementation.md` § Path to Client-Ready (0.6.0
GA), 12-S row).

Out of scope:

- 12-D durability harnesses (closed at `f2f21b5`).
- 12-B benchmark-and-robustness.yml (Wave 1 sibling slice).
- AC-064/065/066 op-store / schema validation (different ACs;
  prior plan refs were wrong).

## Model + effort

Opus 4.7, intent: high. Spawn per `dev/design/orchestration.md` §
2 canonical pattern:

```bash
PHASE=12-S-security-fixtures
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT
re-run that block. Use --disallowedTools Task Agent as a hard
guard. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-S-security-fixtures.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Log destination

- stdout/stderr: `dev/plans/runs/12-S-security-fixtures-<ts>.log`
- structured: `dev/plans/runs/12-S-security-fixtures-output.json`
- reviewer verdict: `dev/plans/runs/12-S-review-<ts>.md`

## Required reading

- `AGENTS.md` § 1, § 3, § 4, § 5, § 7.
- `MEMORY.md`, especially `feedback_tdd.md`,
  `feedback_reliability_principles.md`, `feedback_no_data_migration.md`,
  `feedback_file_deletion.md`.
- `dev/design/orchestration.md` § 2, § 3, § 8.
- `dev/plans/0.6.0-implementation.md` § Path to Client-Ready (0.6.0
  GA) — your slice row + the "Compatibility-shim enforcement"
  section (AC-050a/c enforcement mechanisms #4 + #5).
- `dev/acceptance.md`:
  - AC-036 (line 622) no-listen-syscall capture: bpftrace/auditd
    capture of socket()+listen() syscalls scoped to fathomdb's
    pid/threads during full open+write+search+close cycle.
  - AC-037 (line 630) netns-deny-egress: open in netns with
    default-deny egress; no `connect()` outside loopback.
  - AC-038 (line 638) FTS5-injection-safe: 100 fixture queries with
    FTS5 control syntax; result-set parity with safe-grammar parser;
    zero `SQLITE_ERROR`.
  - AC-050a (line 798) AST no-shim scanner: Rust/Python/TS AST
    analysis of `src/rust/crates/`, `src/python/`, `src/ts/`. Zero
    `legacy_*` modules, zero `compat_v0_5*` features, zero
    `#[allow(deprecated)]` at crate roots, zero re-route stubs
    from 0.5.x verb names.
  - AC-050c (line 816) removal-detect linter: release-checklist
    scans diff for removed public API symbols; each must be
    announced in the same release's `Removed` changelog section.
- Existing related substrate:
  - `scripts/agent-lint-migrations.{sh,py}` — accretion-budget
    linter; pattern for the AST scanner.
  - `scripts/agent-lint.sh` — host script to wire new lints into.
- `dev/test-plan.md` line 1184 (netns-deny-egress + bpftrace
  harnesses already cataloged for AC-036/AC-037).
- Per `feedback_workflow_validation.md`: actionlint = canonical
  workflow validator. Not relevant to this slice but cited for
  any CI yaml changes you might make.

## Scope — five sub-scopes, one commit per sub-scope is fine

TDD discipline: red test → green fix.

**Inventory before writing.** Some substrate may exist:

- search `scripts/` + `tests/` for any existing `listen` /
  `connect` / `netns` / `FTS5 injection` / `AST scanner` / `removal`
  scaffolding.
- if a partial implementation exists, extend it; do not duplicate.

### Sub-1: AC-036 no-listen-syscall capture

**Approach** — `bpftrace` not portable across all CI runners + may
need root. Alternative: `strace -f -e trace=socket,listen` is
portable, pid-scoped, runs unprivileged. Use strace.

1. Test script: `scripts/security/check-no-listen.sh` that:
   - spawns a `fathomdb` binary running an env-gated test fixture
     entry-point that performs open+write+search+close.
   - wraps the spawn under `strace -f -e trace=socket,listen
     -o <log>`.
   - parses the strace log; asserts zero `listen(` syscalls that
     succeed (returns non-negative fd or `0`).
   - exits 0/1 per AC-036.
2. Test fixture binary: extend `fathomdb-cli` with a `--test-cycle`
   hidden flag OR use an existing test binary (`error_taxonomy` or
   similar) gated on env var. Prefer reusing existing test binaries
   to avoid scope creep.
3. Wire `check-no-listen.sh` into `scripts/agent-lint.sh` OR a
   new `scripts/agent-security.sh` (the latter if security gates
   should run separately from lint).
4. Blocker note: if strace isn't installed in the CI runner
   bootstrap, surface — recommend adding to
   `scripts/bootstrap.sh` (apt install strace, ~50KB).

### Sub-2: AC-037 netns-deny-egress

**Approach** — Linux network namespace with default-deny egress.
Test scaffold: `unshare -nU` to create a netns; loopback up;
run engine open+close inside; assert no `connect()` succeeds
outside loopback via `strace -f -e trace=connect`.

1. Test script: `scripts/security/check-netns-deny-egress.sh` that:
   - `unshare -rUn` (rootless netns).
   - Inside netns: `ip link set lo up`.
   - Runs the test fixture entry-point under
     `strace -f -e trace=connect -o <log>`.
   - Parses log; asserts every `connect(...)` is to AF_LOCAL
     (`/tmp/...`) or AF_INET 127.0.0.1 or AF_INET6 ::1.
   - Exits 0/1.
2. Same wiring point as Sub-1 (`scripts/agent-security.sh` or
   `agent-lint.sh`).
3. Blocker note: `unshare -rUn` requires unprivileged userns
   enabled (most modern Linux yes; CI runner verify). If not
   available, document blocker.

### Sub-3: AC-038 FTS5-injection-safe text query

**Approach** — pure Rust integration test against the engine; no
syscall capture needed. 100 fixture queries containing FTS5 syntax
chars; each goes through `engine.search()`; assert result-set
matches the safe-grammar parser's literal-token output; assert no
`SqliteError(SQLITE_ERROR)` raised.

1. Test file:
   `src/rust/crates/fathomdb-engine/tests/fts5_injection_safety.rs`
2. Fixture corpus: 100 queries covering FTS5 syntax: `"`, `*`, `^`,
   `NEAR(`, `AND`, `OR`, `(`, `)`, `:`, `*foo*`, `"foo bar"`,
   `foo OR bar`, etc. Mix in benign tokens. Hand-curate or generate
   via a fixture builder. Per AC-038 measurement: 100 queries.
3. For each query: seed a small DB with deterministic content,
   run `engine.search(q)`, capture result set + any error. Assert:
   - No `SQLITE_ERROR` (i.e. no `EngineError` whose source is
     SQLite malformed-MATCH).
   - Result set matches safe-grammar reference (a parser that
     treats FTS5 syntax chars as literal tokens — implement OR
     reuse existing safe-grammar parser if present in engine).
4. If a safe-grammar parser doesn't exist yet, surface as blocker
   — implementing one is significant scope expansion.

### Sub-4: AC-050a AST no-shim scanner

**Approach** — three scripts, one per language. Each parses source
trees with the language's AST and asserts zero matches for the
banned patterns:

1. `scripts/security/ast-scan-rust.sh` (or `.py` using `syn` via
   subprocess) — scans `src/rust/crates/` for:
   - module names matching `legacy_*` or `compat_v0_5*`.
   - `#[allow(deprecated)]` attributes at crate roots (`lib.rs` /
     `main.rs` top-level).
   - Re-route stubs from 0.5.x verb names (define the 0.5.x verb
     name list explicitly; if empty in repo, scanner is a no-op
     but still wired).
2. `scripts/security/ast-scan-python.py` — uses Python's `ast`
   module on `src/python/`. Same pattern list.
3. `scripts/security/ast-scan-ts.sh` (uses ts-morph or
   `@typescript-eslint/parser` via npx) — `src/ts/`. Same patterns.
4. Wire all three into `scripts/agent-security.sh` (or
   `agent-lint.sh`).
5. Tests for the scanners themselves: `scripts/tests/test_ast_scan.sh`
   with positive (clean tree) + negative (synthetic fixture with
   `legacy_foo` module / `compat_v0_5_admin` feature) cases. The
   negative fixtures live under `scripts/tests/fixtures/ast-shim/`
   so they don't pollute the actual scanned paths.

Per plan § "Compatibility-shim enforcement" mechanism #4: this is
THE enforcement of the no-0.5.x-shim policy. Quality matters.

### Sub-5: AC-050c removal-detect linter

**Approach** — bash script that scans git diff for removed public
API symbols + asserts each removed symbol is named in the release's
CHANGELOG `Removed` section.

1. `scripts/security/check-removal-changelog.sh` that:
   - Takes two args: `<base-ref>` and `<head-ref>` (default
     `0.6.0-rewrite^..HEAD`).
   - Extracts removed public symbols from
     `git diff <base>..<head> -- 'src/rust/crates/**' 'src/python/**'
     'src/ts/**'`.
     "Removed public symbol" = a `-` line whose content matches
     `pub fn|pub struct|pub enum|pub trait|pub const|pub type` (Rust),
     `^def [a-zA-Z]` or `^class [A-Z]` at module top-level (Python),
     `export function|export class|export const|export type` (TS).
     A `+` line at the same path with the same symbol name = NOT
     removed (renamed/moved within the file is symbol-equivalent;
     don't false-positive).
   - For each truly-removed symbol: assert the symbol name appears
     in `CHANGELOG.md` under a `## ...` heading containing
     `Removed`.
   - Exits 0 if all removals documented, non-zero with file:line
     diagnostics otherwise.
2. Tests: `scripts/tests/test_removal_detect.sh` with positive
   (release where every removal is documented) + negative
   (release with undocumented removal) synthetic-diff fixtures.
3. Wire into `scripts/agent-security.sh`.

Per plan § "Compatibility-shim enforcement" mechanism #5.

## Required commands

```bash
cd /tmp/fdb-12-S-security-fixtures-<ts>
# AC-036/037 fixture scripts (linux only).
bash scripts/security/check-no-listen.sh
bash scripts/security/check-netns-deny-egress.sh
# AC-038 FTS5-injection-safe test.
cargo test --workspace --test fts5_injection_safety
# AC-050a AST scanner (all 3 languages).
bash scripts/tests/test_ast_scan.sh
bash scripts/security/ast-scan-rust.sh
bash scripts/security/ast-scan-python.py
bash scripts/security/ast-scan-ts.sh
# AC-050c removal-detect linter.
bash scripts/tests/test_removal_detect.sh
bash scripts/security/check-removal-changelog.sh
# Canonical local gate (existing slice regression guards still pass).
bash scripts/agent-verify.sh
```

All must pass. Known flakes (rerun once):
`ac_029_canonical_writes_complete_under_projection_stall`,
`ac_017_vector_projection_freshness_p99_le_five_seconds`,
`t_safe_export_engine_error_exits_export_failure_66`.

## Discipline

- TDD: each new lint/script lands with a failing test fixture
  (positive case clean tree exits 0; negative case synthetic-shim
  fixture exits non-zero).
- AC-050a is the no-0.5.x-shim enforcement. Quality matters; this
  is what stops future drift.
- AC-050c is the removal-discipline gate. Same.
- No new production code — security fixtures are
  scripts/tests/fixtures only. Per
  `feedback_reliability_principles` net-negative LoC bias: if a
  sub-scope grows past 200 lines of script, audit for
  over-engineering.
- Comment policy: WHY only.

## Blockers — surface before writing code

If any blocks, STOP and write a blocker report at
`dev/plans/runs/12-S-security-fixtures-output.json`:

1. **strace not in CI runner bootstrap** (Sub-1 + Sub-2 depend).
   Recommend: add `apt install strace` to `scripts/bootstrap.sh`.
2. **Unprivileged userns disabled** (Sub-2 needs it). Surface;
   document container/host requirement.
3. **No safe-grammar FTS5 parser in engine** (Sub-3). Building one
   is significant scope; surface + recommend separate slice
   12-S-FTS5-PARSER OR scope AC-038 to "no SQLITE_ERROR" portion
   only (drop the result-set-parity assertion as a follow-up).
4. **ts-morph / @typescript-eslint/parser not in npm deps**
   (Sub-4 TS scanner). Adding it expands npm dep tree; surface +
   propose minimal dep.
5. **No CHANGELOG.md in repo** (Sub-5). Already known —
   `dev/plans/runs/12-D-fix-1-review-20260517T193739Z.md` reviewer
   spot-check noted "CHANGELOG.md not found". Surface: AC-050c
   can land its scanner but cannot demonstrate green against
   real repo until CHANGELOG.md exists. Recommend: create empty
   `CHANGELOG.md` with a `## [Unreleased]` section as part of
   this slice OR defer the real-tree assertion to 12-DX.

## Output

After all commands pass (or all blockers surfaced), write
`dev/plans/runs/12-S-security-fixtures-output.json`:

```json
{
  "phase": "12-S-security-fixtures",
  "baseline_sha": "<HEAD of 0.6.0-rewrite at spawn>",
  "branch": "phase-12-S-security-fixtures-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "sub_scopes_landed": ["1 (AC-036)", "2 (AC-037)", "3 (AC-038)", "4 (AC-050a)", "5 (AC-050c)"],
  "acs_addressed": ["AC-036", "AC-037", "AC-038", "AC-050a", "AC-050c"],
  "scripts_added": ["scripts/security/check-no-listen.sh", "..."],
  "tests_added": ["..."],
  "fixtures_added": ["..."],
  "no_shim_enforcement_live": true,
  "blockers_encountered": [{...}],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for verdict"
}
```

Then stop. Do not advance to 12-P. Do not run the reviewer yourself.
