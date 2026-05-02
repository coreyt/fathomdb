# AGENTS.md — FathomDB

Operating manual for AI coding agents (Claude Code, Codex, Cursor, Aider, Copilot) working in this repo. Cross-vendor convention. This is the canonical agent-instruction file; **no `CLAUDE.md` is maintained** — Claude Code reads `AGENTS.md` natively.

Bullet form, prescriptive, ≤300 lines. Link out, do not inline.

---

## 1. Invariants — read these first

- **Memory first.** Read `MEMORY.md` and the `feedback_*.md` files it points to before planning any change. They encode prior corrections that override default behavior.
- **ADRs are authoritative.** Decisions live in `dev/adr/`. Index: `dev/adr/ADR-0.6.0-decision-index.md`. Do not contradict an accepted ADR; propose a successor instead.
- **TDD is mandatory.** Failing test first; red → green → refactor. Mechanical version bumps and renames are the only exception. (`feedback_tdd.md`)
- **Stale > missing.** A wrong comment, doc, or ADR is more harmful than its absence. If you cannot maintain something, delete it.
- **Public surface is contract.** Anything in `dev/interfaces/` or `pub` Rust APIs is a contract; changes need an ADR or interface-doc update in the same PR.

## 2. Repo shape

- **Rust workspace** under `src/rust/crates/` — 7 crates: `fathomdb`, `fathomdb-cli`, `fathomdb-engine`, `fathomdb-query`, `fathomdb-schema`, `fathomdb-embedder`, `fathomdb-embedder-api`.
- **Python bindings** under `src/python/` (package: `fathomdb`).
- **TypeScript bindings** under `src/ts/`.
- **Public docs** under `docs/` (MkDocs-built).
- **Internal engineering docs** under `dev/`: `adr/`, `interfaces/`, `progress/`, `plans/`, `tmp/`.

## 3. Build / test / lint commands

Use the typed dev-loop verbs (Phase 2). Each emits **concise output on pass, structured diagnostic on fail**, with full output spilled to `/tmp/fathomdb-agent-<verb>-<pid>.log` when capped.

| Verb      | Script                         | Purpose                                                           |
| --------- | ------------------------------ | ----------------------------------------------------------------- |
| build     | `./scripts/agent-build.sh`     | Compile workspace (Rust + Python install + TS build if installed) |
| lint      | `./scripts/agent-lint.sh`      | clippy + ruff + markdownlint + prettier --check + lychee          |
| typecheck | `./scripts/agent-typecheck.sh` | cargo check + pyright + tsc --noEmit                              |
| test      | `./scripts/agent-test.sh`      | cargo test + pytest                                               |
| verify    | `./scripts/agent-verify.sh`    | lint → typecheck → test (short-circuits on first fail)            |

Markdown lint covers `AGENTS.md`, `dev/plans/`, `dev/progress/`, README files, and root metadata. Pre-existing legacy under `dev/adr/`, `dev/design/`, `dev/agents/`, `dev/deps/`, etc. is excluded — clean up incrementally when touching those files. Auto-fix: `npm run format:md` (prettier --write) + `./node_modules/.bin/markdownlint-cli2 --fix`.

Run `./scripts/agent-verify.sh` after every meaningful edit. Do not ship a PR with verify failing.

The broader CI gate is `./scripts/check.sh` (adds mkdocs build); the agent-loop gate is `agent-verify.sh`.

### One-time setup

- Rust toolchain: stable per `rust-version` in `Cargo.toml`. clippy + rustfmt come with rustup defaults.
- Python dev tooling: `pip install -e 'src/python[dev]'` — installs `pytest`, `hypothesis`, `ruff`, `pyright`. Without this, the Python lint/typecheck/test steps emit a skip notice and pass without exercising.
- TypeScript: `cd src/ts && npm install` if you intend to touch TS. Without this, TS verbs skip.
- Markdown tooling: `npm install` at repo root — installs `markdownlint-cli2` + `prettier`. `cargo install --locked lychee` for link checking (one-time). All wired up by `./scripts/bootstrap.sh`.

## 4. Verification ordering

Run in latency order; short-circuit on first failure:

1. **lint** (clippy / ruff) — fastest signal, catches most style + correctness issues
2. **typecheck** (cargo check / pyright / tsc) — catches type errors before tests run them
3. **unit tests** (`agent-test.sh`)
4. **integration tests** — opt-in, gated by feature/env flag; not part of `agent-verify`

Do not paraphrase, summarize, or shorten compiler diagnostics — pass them through unaltered. Rust diagnostics in particular are best-in-class; Anthropic's RustAssistant numbers depend on them being unaltered.

## 5. Test discipline

- **Failing test first.** Write the failing test, commit it (or stage it visible to the reviewer), then implement.
- **Test files are read-only during fix-to-spec.** Do not edit a test to make a failing build pass; fix the code.
- **Property-based tests required** for codec / projection / recovery / round-trip layers. Rust: `proptest` (dev-dep on `fathomdb-schema`, `fathomdb-engine`, `fathomdb-query`). Python: `hypothesis` (in `[dev]` and `[test]` optional deps; install via `pip install -e 'src/python[dev]'`). Scaffolds: `src/rust/crates/<crate>/tests/property_template.rs` and `src/python/tests/test_property_template.py` — replace the trivial property with real round-trip / invariant assertions when domain types land.
- **No agent-generated oracles.** Tests must encode human intent; do not regenerate snapshot or golden tests autonomously.
- **Retry budget.** If you hit the same failure mode twice, stop. Re-read the failing test, the relevant ADR, and the relevant `feedback_*.md`. Do not loop a third time without re-thinking; if necessary, `/clear` and reset.

## 6. Comment policy

- **Public-API docstrings:** required for `pub` Rust items, top-level Python functions/classes, exported TS symbols. Document contract: inputs, outputs, errors, panics, invariants.
- **Internal helpers:** no docstrings unless behavior is non-obvious. Names should carry the meaning.
- **Inline comments:** why / invariants / hazards only. Never restate what code does. Never reference the current task or PR. (Per CLAUDE-default rules and `feedback_reliability_principles.md`.)
- **Stale comment > delete.** If a comment no longer matches the code, delete it; do not "update later."

## 7. Subagent rules

- **Main thread orchestrates.** Do not spawn an "orchestrator" subagent — the main thread _is_ the orchestrator. (`feedback_orchestrator_thread.md`)
- **Releases:** main thread plans; delegate coding to `implementer` (in worktree); request diff review from `code-reviewer`. (`feedback_orchestrate_releases.md`)
- **Subagents win for fan-out.** Parallel research, format-strict review, output isolation. Examples: searching across crates, auditing a diff, generating an ADR draft.
- **Subagents lose for shared-state edits.** A multi-agent edit pipeline drops tacit context at every handoff. Single-agent for any edit on shared mutable state.
- **Worktrees** are the unit of isolation. Implementer subagents always operate in a fresh worktree; main thread never edits in a subagent's worktree.

## 8. Iteration discipline

- **Cap retry-budget at ~2 same-issue corrections.** Beyond that, stop, externalize plan to `dev/progress/0.6.0.md`, `/clear`, restart with the plan in front of you.
- **Compact-aware.** Anything that must survive compaction goes on disk: ADRs, progress logs, plan files, MEMORY entries. Do not rely on chat to remember.
- **Front-load invariants, end-load tasks.** When prompting yourself or constructing context, put rules near the top, the immediate task near the bottom (lost-in-the-middle).

## 9. Pointers

- **Decisions:** `dev/adr/ADR-0.6.0-decision-index.md`
- **Interfaces:** `dev/interfaces/{rust,python,typescript,cli,wire}.md`
- **Progress log:** `dev/progress/0.6.0.md`
- **Plans:** `dev/plans/`
- **Research:** `dev/tmp/context-research-agentic-best-practices.md` (the best-practices synthesis this file operationalizes)
- **Memory:** `MEMORY.md` (auto-loaded into Claude Code session start)

## 10. Forbidden patterns

Pulled from MEMORY `feedback_*.md`. Violations are correctness bugs, not style preferences.

- **No mocking the database.** Integration tests hit a real database. (`feedback_tdd.md`-adjacent; prior incident.)
- **No skipping hooks.** Never `--no-verify`, never `--no-gpg-sign` unless explicitly approved.
- **No backwards-compatibility shims** in pre-1.0 rewrites.
- **No data migrations** in managed-vec projection releases. (`feedback_no_data_migration.md`)
- **No `c_char` hardcoded as `i8` or `u8`** at C interop boundaries. (`feedback_cross_platform_rust.md`)
- **No `pip install` + manual `cargo build` + `cp`** for Python native modules — use `pip install -e python/`. (`feedback_python_native_build.md`)
- **No `yaml.safe_load` as workflow validator** — use `actionlint`. (`feedback_workflow_validation.md`)
- **No "fix in 0.7"** for reliability bugs. Net-negative LoC on reliability releases. (`feedback_reliability_principles.md`)
- **No "green CI = done"** for releases — install the published wheel and run end-to-end open/close/exit before declaring done. (`feedback_release_verification.md`)
- **Vector identity belongs to the embedder.** (`project_vector_identity_invariant.md`)

## 11. Permission model (for the Claude Code harness)

`.claude/settings.json` enforces a 3-tier model:

- **read-only** (auto-allowed): `Read`, `Grep`, `Glob`, plus `Bash` for the agent-loop verbs and read-only git/cargo/uv operations.
- **workspace-write** (auto-allowed): `Edit`, `Write` within the repo; `git add`, `git commit`, `git restore`.
- **escalated** (denied or asked): destructive ops (`rm`, `mv`, `git reset --hard`, `git clean`); network beyond the package registries (`curl`, `wget`); `git push` requires confirmation.

If the agent needs an op outside this model, surface it to the user; do not look for workarounds.

## 12. Working with this file

- This file should stay ≤300 lines. If it grows, link out to a scoped doc and reference it.
- Per-crate `AGENTS.md` files are **not maintained** until each crate has non-scaffold content. Stale > missing.
- Update this file in the same PR as any change that invalidates one of its rules.
