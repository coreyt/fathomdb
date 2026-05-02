---
title: Agentic context engineering — Phase 1–3 implementation plan
date: 2026-05-01
source: dev/tmp/context-research-agentic-best-practices.md (Conclusion items 1–8)
phases-in-scope: 1, 2, 3 (foundations + dev-loop + property-test scaffolding)
phases-deferred: 4 (repo-map.sh — discoverability), 5 (prompt caching — gated on embedder)
---

# Phases 1–3 implementation plan

Critical path before substantive 0.6.0 implementation. Total ~3–4 days. One PR per phase.

## Decisions resolved up-front

- **No `CLAUDE.md`.** AGENTS.md is the cross-vendor convention (F7); Claude Code, Codex, Cursor all read it natively. Symlink is unreliable on Windows (Git's `core.symlinks` defaults to `false` on Windows clones, producing a text file containing the target path). Per user direction: use AGENTS.md only, do not copy.
- **Egress allowlist** mostly satisfied by existing `.claude/settings.json` (no curl/wget allowed; cargo/npm/pip allowed for build). Add explicit denylist on `cargo install` is already present. Phase 1 only needs to add the workflow-level "approve specific egress on demand" pattern in AGENTS.md, not new config.
- **Output discipline (per user 1a):** every script produces:
  - **Pass:** single-line summary or empty stdout. No tool-version banners, no "build succeeded" verbosity. Exit 0.
  - **Fail:** structured diagnostic with file:line, error code, suggested next action. No truncation of the failure context. Exit non-zero.
  - **Truncation rule:** capped at 200 lines stdout, full output spilled to `/tmp/fathomdb-agent-<verb>-<pid>.log`; the cap message identifies the spill path.
- **Per-crate AGENTS.md:** deferred per F4 (stale > missing). Wait until each crate has non-scaffold content.

---

## Phase 1 — AGENTS.md + permissions + progress log

**Items: 1, 4 (subagent rules folded into AGENTS.md), 6, 8.**

### Files

| Path | Action | Notes |
|---|---|---|
| `AGENTS.md` | overwrite (currently empty) | ≤300 lines, bullet form (F15) |
| `.claude/settings.json` | edit | add explicit allow for `./scripts/agent-*.sh`; document tier intent |
| `dev/progress/0.6.0.md` | create | template + first entry |
| `dev/progress/README.md` | create | one-line: "per-release multi-session work logs" |

### AGENTS.md contents (sections, top-down for prompt-position discipline F9)

1. **Invariants (front-loaded)** — read MEMORY first; ADR decisions in `dev/adr/`; never bypass `feedback_*.md` rules
2. **Repo shape** — three language surfaces, paths
3. **Build/test/lint commands** — `scripts/agent-{build,lint,typecheck,test,verify}.sh` (Phase 2 will land these)
4. **Verification ordering** — lint → typecheck → unit → integration; short-circuit on first fail
5. **Test discipline** — TDD (failing test first); proptest/hypothesis for codec/projection/recovery (Phase 3); no agent-generated oracles
6. **Comment policy** — public-API docstrings yes; internal helpers no; why/invariants only; stale > missing
7. **Subagent rules** — main thread orchestrates; implementer-in-worktree + code-reviewer; single-agent for shared-state edits
8. **Iteration discipline** — cap retry-budget ~2 same-issue corrections then `/clear`
9. **Pointers** — `dev/adr/ADR-0.6.0-decision-index.md`, `dev/interfaces/`, `dev/progress/0.6.0.md`, `dev/tmp/context-research-agentic-best-practices.md`
10. **Forbidden patterns** — pulled from MEMORY `feedback_*.md`

### .claude/settings.json edits

Add to allow list:
- `Bash(./scripts/agent-build.sh:*)`
- `Bash(./scripts/agent-lint.sh:*)`
- `Bash(./scripts/agent-typecheck.sh:*)`
- `Bash(./scripts/agent-test.sh:*)`
- `Bash(./scripts/agent-verify.sh:*)`

Existing tiers (allow/deny/ask) already match the 3-tier model; no structural change.

### dev/progress/0.6.0.md template

- Date-stamped entries
- Sections: **Done**, **In progress**, **Blocked**, **Decisions**, **Next**
- First entry: Phase 1 landed; AGENTS.md filled; permissions extended

### Verification

- Open AGENTS.md, confirm ≤300 lines.
- Confirm new permission lines parse (`.claude/settings.json` valid JSON).
- Spawn fresh Claude session, ask "what's the build command" → cited from AGENTS.md.

### Exit criteria

- AGENTS.md ≤300 lines, committed.
- `.claude/settings.json` updated, committed.
- `dev/progress/0.6.0.md` created, first entry committed.

---

## Phase 2 — Typed dev-loop verbs

**Item: 2.** Output discipline per user 1a.

### Files

| Path | Action |
|---|---|
| `scripts/lib/agent-output.sh` | new — shared truncation + spill helper |
| `scripts/agent-build.sh` | new |
| `scripts/agent-lint.sh` | new |
| `scripts/agent-typecheck.sh` | new |
| `scripts/agent-test.sh` | new |
| `scripts/agent-verify.sh` | new — runs lint → typecheck → unit-test, short-circuit |

### Output contract (every script)

```
PASS path:  exit 0, single-line "OK <verb> <duration>" to stderr (or silent)
FAIL path:  exit non-zero, structured diagnostic to stdout (file:line:col + msg + suggested fix), full log spilled to /tmp/fathomdb-agent-<verb>-<pid>.log
            stdout capped at 200 lines; if capped, last line names the spill path
```

### Per-script behavior

**agent-build.sh**
- Rust: `cargo build --workspace --message-format=human` (humanize for terminal; switch via `--json` flag)
- Python: `pip install -e src/python` if pyproject changed (skip otherwise — track via mtime sentinel `.cache/agent/python-installed`)
- TS: `(cd src/ts && [ -d node_modules ] && npm run build || echo "skipped: TS not installed")`

**agent-lint.sh**
- Rust: `cargo clippy --workspace --all-targets -- -D warnings`
- Python: `ruff check src/python` (skip if ruff not installed → emit one-line skip notice; do not fail)
- TS: ESLint not configured yet → no-op skip

**agent-typecheck.sh**
- Rust: `cargo check --workspace` (clippy already covers, but `check` is faster — used as the type-only gate)
- Python: `pyright src/python` (skip if not installed)
- TS: `(cd src/ts && [ -d node_modules ] && npm run typecheck || echo "skipped: TS not installed")`

**agent-test.sh**
- Rust: `cargo test --workspace --no-fail-fast`
- Python: `pytest -q src/python/tests` (skip if pytest not installed)
- TS: no test runner configured yet → skip

**agent-verify.sh**
- Run lint → typecheck → test in order; first failure short-circuits.
- Pass-path output: one-line `PASS verify (<total-duration>)`.
- Fail-path output: identifies which step failed, its diagnostic, and the spill path.

### Output discipline implementation

`scripts/lib/agent-output.sh` provides:
```
run_capped <verb> <command...>
  - Captures stdout+stderr to spill file
  - On exit 0: prints nothing (or "ok <verb> NNNms" if AGENT_VERBOSE=1)
  - On non-zero: prints first 200 lines of spill to stdout, plus footer pointing to spill path
```

Cap is mid-pipe (head -n 200 of failure output) so the agent always sees the *start* of the failure (where the actionable diagnostic lives).

### Verification

- Empty workspace: each script runs green, single-line or empty output.
- Inject a clippy warning into one crate → `agent-lint.sh` fails with the diagnostic visible, structured location, spill path printed.
- Inject `tsc` error → `agent-typecheck.sh` fails with TS diagnostic.
- `agent-verify.sh` short-circuits: bad lint → typecheck never runs.

### Exit criteria

- All five scripts executable, in repo, follow output contract.
- `scripts/check.sh` (existing) continues to work or gets superseded — decide during impl: likely keep `check.sh` as the broad CI gate, `agent-verify.sh` as the agent-loop gate.
- AGENTS.md "build/test/lint commands" section references the new scripts.

---

## Phase 3 — Property-based test scaffolding

**Item: 3.**

### Files

| Path | Action |
|---|---|
| `src/rust/crates/fathomdb-schema/Cargo.toml` | add `[dev-dependencies] proptest = "1"` |
| `src/rust/crates/fathomdb-engine/Cargo.toml` | same |
| `src/rust/crates/fathomdb-query/Cargo.toml` | same |
| `src/rust/crates/fathomdb-schema/tests/property_template.rs` | new — proptest scaffold |
| `src/rust/crates/fathomdb-engine/tests/property_template.rs` | new |
| `src/rust/crates/fathomdb-query/tests/property_template.rs` | new |
| `src/python/pyproject.toml` | add `[project.optional-dependencies] test = ["hypothesis", "pytest"]` |
| `src/python/tests/test_property_template.py` | new — hypothesis scaffold |
| `AGENTS.md` test section | edit — codify proptest/hypothesis requirement for codec/projection/recovery |

### Template content (Rust)

Each `property_template.rs`:
- Trivial proptest (e.g., `proptest! { fn ident_round_trip(x in any::<u64>()) { prop_assert_eq!(x, x); } }`)
- Comment block listing the real targets per ADR (codec round-trip, recovery rank correlation, etc.)

### Template content (Python)

`test_property_template.py`:
- Trivial hypothesis test (e.g., `@given(st.integers())` → `assert x == x`)
- Comment listing real targets (Python API round-trips per ADR-0.6.0-python-api-shape)

### Verification

- `cargo test --workspace` includes the proptest scaffolds (run via `agent-test.sh`).
- `pytest src/python/tests/test_property_template.py` passes (skip if hypothesis not installed in env — emit clear message in AGENTS.md install instructions).
- AGENTS.md test policy updated: codec/projection/recovery layers MUST use proptest/hypothesis.

### Exit criteria

- Three Rust crates compile with proptest dev-dep.
- Python test extras include hypothesis.
- All template tests pass.
- AGENTS.md updated.

---

## Cross-phase

### Commit shape

- One commit per phase, conventional commit prefix `chore(agents):` or `chore(scaffold):`.
- Commit message body: bullet list of files + reference to this plan.
- No `--no-verify`. Pre-commit hooks must pass.

### Verification gate per phase

- `scripts/check.sh` (existing) green.
- `scripts/agent-verify.sh` green (after Phase 2 lands).
- `dev/progress/0.6.0.md` updated.

### Open questions

- Should `scripts/check.sh` be replaced by `agent-verify.sh` or kept as the broader CI gate? Keep both for now: `check.sh` includes mkdocs build, `agent-verify.sh` does not.
- ESLint setup for TS — defer until TS gains real source.

---

## After Phases 1–3

Implementation of 0.6.0 features can begin. Phase 4 (repo-map.sh) and Phase 5 (prompt caching) deferred per main best-practices doc.
