# Phase 12-S-fix-1 — Reviewer remediation pass

Targeted fix for the three codex `gpt-5.4` findings on Phase 12-S
(verdict `BLOCK`; all three substantive, no orchestrator override
applicable). See `dev/plans/runs/12-S-review-20260517T200825Z.md`.

Operates in the existing 12-S worktree
`/tmp/fdb-12-S-security-fixtures-20260517T195735Z` on branch
`phase-12-S-security-fixtures-20260517T195735Z`. Builds new commits
on top of `d48d226`.

## Model + effort

Opus 4.7, intent: medium. Per `dev/design/orchestration.md` § 6
fix-N pattern:

```bash
PHASE=12-S-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-12-S-security-fixtures-20260517T195735Z
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Use
--disallowedTools Task Agent as a hard guard. Write code, run
tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-S-fix-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/runs/12-S-review-20260517T200825Z.md` — the verdict.
- `scripts/security/ast_scan.py` — finding 1 + 2 patch site
  (around line 31 = V05_VERBS; line 77 = scan_rust).
- `scripts/security/ast-scan-rust.sh` — wrapper.
- `scripts/tests/test_ast_scan.sh` — finding 2 self-test gap.
- `scripts/tests/fixtures/ast-shim/rust/dirty/legacy_admin.rs` —
  finding 2 fixture (need to split crate-root rule into proper
  lib.rs fixture).
- `scripts/agent-security.sh` — finding 3 STRICT path.
- `scripts/agent-verify.sh` — finding 3 wiring target.
- `scripts/bootstrap.sh` — finding 3 sudo apt-get update gap.
- `.github/workflows/ci.yml` — alternative finding 3 wiring target.

For finding 1 verb enumeration, recover 0.5.x verb names from
pre-0.6.0 source:

```bash
git show 39ee271^:python/fathomdb/__init__.py 2>/dev/null | head -100
git show 39ee271^:typescript/packages/fathomdb/src/index.ts 2>/dev/null | head -100
git log --grep="rename\|verb\|0.5" 39ee271^ --oneline -20
```

## Scope — three findings

TDD discipline: red test → green fix.

### Finding 1 (`high`) — populate V05_VERBS

The scanner's "0.5.x verb re-route stub" detection is policy
required by AC-050a. Empty `V05_VERBS` = unenforced clause.

**Required fix:**

1. Inventory 0.5.x verbs from pre-0.6.0 source (`git show
   39ee271^:`). Look at:
   - `python/fathomdb/__init__.py` (top-level Python verbs)
   - `typescript/packages/fathomdb/src/index.ts` (TS exports)
   - `rust/src/lib.rs` or equivalent (Rust facade verbs)
   - CLI verbs from `cli` crate / pre-0.6.0 binary
2. Populate `V05_VERBS` in `scripts/security/ast_scan.py` with
   every 0.5.x top-level verb name that has NO 0.6.0 equivalent
   (or whose 0.6.0 name differs). Be liberal — false positives on
   names like `open` / `close` are unlikely in 0.6.0 because
   those names ARE the 0.6.0 surface; the scanner pattern matches
   re-route stubs (`fn old_verb_name(...) { new_verb_name() }`),
   not name presence per se.
3. Re-read the scanner's verb-detection logic. If it just checks
   "does this file define a function whose name is in V05_VERBS",
   that's too aggressive (would flag legitimate same-name 0.6.0
   verbs). It should look for re-route patterns:
   - Rust: `pub fn <V05_VERB>(...) { ... <0.6.0-verb>(...); }`
     where the function body delegates to a 0.6.0 surface.
   - Python: `def <V05_VERB>(...): return <0.6.0-verb>(...)`.
   - TS: `export function <V05_VERB>(...) { return
     <0.6.0-verb>(...); }`.
   If V05_VERBS list is genuinely "all verbs that should NOT
   exist by name in 0.6.0", the simpler `function-name-matches`
   check is fine — but then the list must NOT include
   `open`/`close`/`search`/`write`/`configure` (the kept verbs).
4. Add a dirty fixture under
   `scripts/tests/fixtures/ast-shim/rust/dirty/v05_verb_reroute.rs`
   that defines a 0.5.x verb name as a function. Same for
   Python + TS.
5. Extend `scripts/tests/test_ast_scan.sh` to:
   - Assert scanner exits non-zero on each v05_verb_reroute
     fixture.
   - Assert scanner exits zero on the clean fixtures (regression
     guard).
   - Assert `V05_VERBS` is non-empty (sentinel test: `python3 -c
     "from scripts.security.ast_scan import V05_VERBS; assert
     len(V05_VERBS) > 0"` or equivalent grep-based check).

**If the 0.5.x verb inventory is genuinely empty** (i.e. no verbs
were renamed/removed in the 0.5.x→0.6.0 transition; rare but
possible if 0.6.0 is a fresh redesign), surface as blocker and
document the empty-list rationale in `V05_VERBS` comment + add a
sentinel test that the file's intentionally-empty state is
explicit not accidental.

### Finding 2 (`medium`) — Rust block-comment + crate-root

Two sub-fixes:

**Sub-2a: Block comment stripping in `scan_rust()`**

Currently `scan_rust()` only skips lines starting with `//`. AC-050a
excludes "comments and docs" — must include `/* */` block comments
and `/** */` doc comments.

1. Add block-comment state tracking to `scan_rust()`: a flag
   that tracks whether we're inside a multi-line comment;
   `*/` exits the state; `/*` enters it. Lines fully inside the
   block are skipped.
2. Add a dirty fixture variant that contains a banned name
   INSIDE a block comment — assert scanner does NOT flag it
   (negative-of-negative; the comment-skip prevents the
   false-positive).
3. Add a dirty fixture variant that contains a banned name
   OUTSIDE the block comment but with a block comment elsewhere
   in the file — assert scanner DOES flag it (block-comment
   stripping doesn't accidentally swallow real code).

**Sub-2b: Crate-root `#![allow(deprecated)]` test gap**

The negative fixture currently puts `#![allow(deprecated)]` in
`legacy_admin.rs`, but the scanner only checks `lib.rs`/`main.rs`
for the crate-root rule. The fixture's pass is from the
filename-prefix rule firing, not the crate-root rule.

1. Add a separate fixture
   `scripts/tests/fixtures/ast-shim/rust/dirty/lib.rs` containing
   only `#![allow(deprecated)]` (no other banned content). This
   exercises the crate-root rule in isolation.
2. Update `test_ast_scan.sh` to point at this fixture and assert
   the scanner fires the crate-root rule (not just the name-prefix
   rule).
3. Keep `legacy_admin.rs` as the name-prefix fixture; remove the
   `#![allow(deprecated)]` from it (it was confusingly conflating
   two rules).

### Finding 3 (`high`) — STRICT=1 wired into real gate + bootstrap hardened

**Sub-3a: Wire STRICT=1 into agent-verify or CI**

Currently `agent-security.sh` only fails on policy violations
(rc=1); blockers (rc=2) are non-fatal unless `STRICT=1` is set
externally. Nothing in the repo sets `STRICT=1`. Real CI should
gate on the blocker path too.

Cleanest approach: invoke `STRICT=1 bash scripts/agent-security.sh`
from `scripts/agent-verify.sh` (which is what CI runs via
`.github/workflows/ci.yml`). That makes the gate real without
adding a new workflow.

1. Add `STRICT=1 bash scripts/agent-security.sh` step to
   `scripts/agent-verify.sh` (after lint/typecheck, before tests
   — same tier as the existing `scripts/agent-lint.sh` call).
2. Document the STRICT=1 invocation in `agent-security.sh` header
   comment.
3. Test: run `bash scripts/agent-verify.sh` locally and confirm
   the security stage runs strict (any blocker now causes
   agent-verify to fail). On this aarch64 dev host strace is
   absent → the new STRICT step should fail with a clear
   diagnostic. That's the intended behavior; the bootstrap fix
   in Sub-3b closes the gap on CI.

**Sub-3b: bootstrap.sh `sudo apt-get update` before install**

Per GitHub Actions runner customization docs
(<https://docs.github.com/en/actions/how-tos/manage-runners/github-hosted-runners/customize-runners>),
`sudo apt-get update` must precede `sudo apt-get install` because
package indexes on hosted runners can be stale.

1. Edit `scripts/bootstrap.sh` strace-install clause: add `sudo
   apt-get update -qq` before `sudo apt-get install -y strace`.
2. Keep the OS gating (apt-get only on Linux) intact.
3. If `sudo apt-get update` would slow down local dev
   (developers running bootstrap repeatedly), gate the update on
   `CI=true` or an explicit env var. Surface tradeoff in commit
   message.

## Required commands

```bash
cd /tmp/fdb-12-S-security-fixtures-20260517T195735Z
# Finding 1 verb-reroute tests.
bash scripts/tests/test_ast_scan.sh
# Finding 2 block-comment + crate-root tests (in same suite).
# (Already covered by test_ast_scan.sh extension.)
# Finding 3a: agent-verify now runs security strict.
bash scripts/agent-verify.sh
# (On this aarch64 host, this WILL fail at the new strict step
# because strace is absent. That's expected; sub-3b's bootstrap
# fix solves it on CI. For local dev, run `sudo apt install strace`
# manually first.)
# Finding 3b: bootstrap.sh syntax check.
bash -n scripts/bootstrap.sh
```

Document in output JSON: agent-verify expected-fail status on
local aarch64 host vs expected-pass on Linux CI.

## Discipline

- TDD: every new lint rule lands red (dirty fixture flagged), then
  green (clean fixture passes).
- Net LoC bias: V05_VERBS population is a list of strings, ~10-30
  lines. Block-comment state in scan_rust is ~15-25 lines.
  agent-verify strict invocation is 1-3 lines. bootstrap apt-get
  update is 1 line. Total fix ~30-70 LoC.
- Comment policy: WHY only. The V05_VERBS file needs a brief
  WHY comment ("0.5.x verbs that were removed/renamed in
  0.6.0; do not re-introduce — see CHANGELOG `Removed`").

## Blockers — surface before writing code

1. **0.5.x verb inventory genuinely empty.** If pre-0.6.0 source
   shows the 0.6.0 verbs are identical names to 0.5.x (no
   renames/removals), the verb-reroute rule is unenforceable. In
   that case, document the empty-list rationale explicitly +
   add a sentinel test asserting V05_VERBS is intentionally
   empty (so a future renamed verb can't silently skip
   enforcement).
2. **agent-verify-strict breaks on local dev hosts without
   strace** (this aarch64 host). Surface tradeoff; recommend
   either (a) gate the STRICT=1 on CI=true env var, or (b)
   document that local devs must run bootstrap (which now
   installs strace) before agent-verify.

## Output

After all commands pass, write
`dev/plans/runs/12-S-fix-1-output.json`:

```json
{
  "phase": "12-S-fix-1",
  "baseline_sha": "d48d226",
  "branch": "phase-12-S-security-fixtures-20260517T195735Z",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "findings_addressed": [
    "1 [high]: V05_VERBS populated from pre-0.6.0 source inventory; verb-reroute fixtures + sentinel test added",
    "2 [medium]: scan_rust() now strips /* */ + /** */ block comments; crate-root rule exercised by dedicated lib.rs fixture",
    "3 [high]: agent-verify.sh now invokes STRICT=1 agent-security.sh; bootstrap.sh runs apt-get update before install"
  ],
  "v05_verbs_count": <int>,
  "v05_verbs_source": "git show 39ee271^:<file> + <file>",
  "agent_verify_local_aarch64_status": "fail | pass (strace install gating)",
  "agent_verify_ci_expected": "pass",
  "blockers_encountered": [{...}],
  "agent_verify_result": "pass | fail (+ tail) [note local-vs-CI]",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for clean PASS"
}
```

Then stop. Do not advance to next slice. Do not run the reviewer
yourself.
