# AGENTS.md — FathomDB

Operating manual for AI coding agents (Claude Code, Codex, Cursor, Aider, Copilot) working in this repo. Cross-vendor convention. This is the canonical agent-instruction file; **no `CLAUDE.md` is maintained** — Claude Code reads `AGENTS.md` natively.

Bullet form, prescriptive, ≤300 lines. Link out, do not inline.

## 0. System layers — where each rule fits

- **Invariants layer** — this file (front-loaded, cached, ≤300, bullets).
- **Index layer** — `dev/adr/ADR-0.6.0-decision-index.md`, `dev/interfaces/*.md`. Linked, never inlined.
- **Retrieval layer** — grep / glob / read / LSP / tree-sitter map on demand. No local vector index.
- **Execution layer** — typed dev-loop verbs under `scripts/agent-*.sh` emit structured JSON. Tests are oracle.
- **Persistence layer** — the per-release board `dev/plans/runs/STATUS-<version>.md`, its single writer `dev/plans/release-state-<version>.json`, the append-only JSONL ledgers, and ADR supersession. Compaction-safe. Concrete files: § 9. `dev/progress/` is **historical** — frozen at 0.6.x; read for history, never write.
- **Permission layer** — three-tier sandbox + worktree-per-task + egress allowlist (`.claude/settings.json`).
- **Topology layer** — main thread orchestrates; implementer (worktree) + the codex review gate fan out at clean seams; single-agent for shared-state edits; **one writer per checkout** — concurrent file-mutating sessions/agents each get their own worktree, or are serialized.

---

## 1. Invariants — read these first

- **Memory first.** The store is **outside the repo**, under `~/.claude/projects/<repo-path-slug>/memory/` (the slug is this repo's absolute path with `/` replaced by `-`) — kebab-case topic files (e.g. `orchestration-execution-traps.md`, `release-publish-gotchas.md`) indexed by `MEMORY.md` in that same directory, auto-loaded at Claude Code session start. Read the index, then the entries bearing on your change, before planning. They encode prior corrections that override default behavior. Cite entries by their real filename — **verify the file exists before citing it** (this file once carried ten citations to files that never existed).
- **ADRs are authoritative.** Decisions live in `dev/adr/`. Index: `dev/adr/ADR-0.6.0-decision-index.md`. Do not contradict an accepted ADR; propose a successor instead.
- **TDD is mandatory.** Failing test first; red → green → refactor. Mechanical version bumps and renames are the only exception. Discipline: § 5.
- **Stale > missing.** A wrong comment, doc, or ADR is more harmful than its absence. If you cannot maintain something, delete it.
- **Public surface is contract.** Anything in `dev/interfaces/` or `pub` Rust APIs is a contract; changes need an ADR or interface-doc update in the same PR.

## 2. Repo shape

- **Rust workspace** under `src/rust/crates/` — 7 crates: `fathomdb`, `fathomdb-cli`, `fathomdb-engine`, `fathomdb-query`, `fathomdb-schema`, `fathomdb-embedder`, `fathomdb-embedder-api`.
- **Python bindings** under `src/python/` (package: `fathomdb`).
- **TypeScript bindings** under `src/ts/`.
- **Public docs** under `docs/` (MkDocs-built).
- **Internal engineering docs** under `dev/` — load-bearing: `dev/adr/`, `dev/interfaces/`, `dev/design/`, `dev/plans/` (boards, plans, prompts), `dev/steward/`. List `dev/` for the rest. `dev/progress/` is **historical** (0.6.x only).

## 3. Build / test / lint commands

Use the typed dev-loop verbs (Phase 2). Each emits **concise output on pass, structured diagnostic on fail**, with full output spilled to `/tmp/fathomdb-agent-<verb>-<pid>.log` when capped.

| Verb      | Script                         | Purpose                                                           |
| --------- | ------------------------------ | ----------------------------------------------------------------- |
| build     | `./scripts/agent-build.sh`     | Compile workspace (Rust + Python install + TS build if installed) |
| lint      | `./scripts/agent-lint.sh`      | clippy + rustfmt + migration policy + ruff + actionlint + md + lychee |
| typecheck | `./scripts/agent-typecheck.sh` | cargo check + pyright + tsc --noEmit                              |
| test      | `./scripts/agent-test.sh`      | cargo test + pytest                                               |
| verify    | `./scripts/agent-verify.sh`    | lint → typecheck → test (short-circuits on first fail)            |

Markdown lint covers **every `**/*.md`** except the ignore list in `.markdownlint-cli2.jsonc` (build output, `dev/archive/`, `dev/plans/runs/`, `dev/plans/prompts/`, `dev/experiments/`, `.claude/`, `docs/`). `docs/` is linted separately by `scripts/agent-lint-docs.sh`. `scripts/agent-lint-md.sh` also runs the plans/design/findings/anchor linters and `scripts/check-release-state-views.sh`. Auto-fix: **`npm run format:md` only** — it wraps `markdownlint-cli2 --fix` in the CommonMark-AST neutrality guard (`dev/tools/md_neutrality_guard.py`). ⛔ **Never run `prettier` on markdown, and never run `markdownlint-cli2 --fix` unguarded** — both are documented corruptors (prettier rewrites `*` → `_`; raw `--fix` mangles `#`-prefixed prose and schemeless hosts). See `dev/tools/md-fix-corruption-ledger.md`.

Run `./scripts/agent-verify.sh` after every meaningful edit. Do not ship a PR with verify failing.

The broader CI gate is `./scripts/check.sh` (adds mkdocs build); the agent-loop gate is `scripts/agent-verify.sh`. Long-run test variants (e.g. AC-021 60 s window, AC-059b ~1000-iteration cursor-race fixture) are exercised only via `scripts/check.sh` with `AGENT_LONG=1`; `scripts/agent-verify.sh` skips them for runtime budget.

### One-time setup

- Rust toolchain: stable per `rust-version` in `Cargo.toml`. clippy + rustfmt come with rustup defaults.
- Python dev tooling: `pip install -e 'src/python[dev]'` — installs `pytest`, `hypothesis`, `ruff`, `pyright`. Without this, the Python lint/typecheck/test steps emit a skip notice and pass without exercising.
- TypeScript: `cd src/ts && npm install` if you intend to touch TS. Without this, TS verbs skip.
- Markdown tooling: `npm install` at repo root — installs `markdownlint-cli2`, and `prettier` which is retained as a devDep for non-markdown use only (§ 3 bans it on `.md`). `cargo install --locked lychee` for link checking (one-time). All wired up by `./scripts/bootstrap.sh`.

## 4. Verification ordering

Run in latency order; short-circuit on first failure:

1. **lint** (clippy / ruff) — fastest signal, catches most style + correctness issues
2. **typecheck** (cargo check / pyright / tsc) — catches type errors before tests run them
3. **unit tests** (`scripts/agent-test.sh`)
4. **integration tests** — opt-in, gated by feature/env flag; not part of `agent-verify`

Do not paraphrase, summarize, or shorten compiler diagnostics — pass them through unaltered. Rust diagnostics in particular are best-in-class; Anthropic's RustAssistant numbers depend on them being unaltered.

## 5. Test discipline

- **Failing test first.** Write the failing test, commit it (or stage it visible to the reviewer), then implement.
- **Test files are read-only during fix-to-spec.** Do not edit a test to make a failing build pass; fix the code.
- **Property-based tests required** for codec / projection / recovery / round-trip layers. Rust: `proptest` (dev-dep on `fathomdb-schema`, `fathomdb-engine`, `fathomdb-query`). Python: `hypothesis` (in `[dev]` and `[test]` optional deps; install via `pip install -e 'src/python[dev]'`). Scaffolds: `src/rust/crates/<crate>/tests/property_template.rs` and `src/python/tests/test_property_template.py` — replace the trivial property with real round-trip / invariant assertions when domain types land.
- **No agent-generated oracles.** Tests must encode human intent; do not regenerate snapshot or golden tests autonomously.
- **Retry budget.** If you hit the same failure mode twice, stop. Re-read the failing test, the relevant ADR, and the relevant memory entry (§ 1). Do not loop a third time without re-thinking; if necessary, `/clear` and reset.

## 6. Comment policy

- **Public-API docstrings:** required for `pub` Rust items, top-level Python functions/classes, exported TS symbols. Document contract: inputs, outputs, errors, panics, invariants.
- **Internal helpers:** no docstrings unless behavior is non-obvious. Names should carry the meaning.
- **Inline comments:** why / invariants / hazards only. Never restate what code does. Never reference the current task or PR.
- **Stale comment > delete.** If a comment no longer matches the code, delete it; do not "update later."

## 7. Subagent rules

- **Main thread orchestrates.** Do not spawn an "orchestrator" subagent _from inside an orchestrator or an implementer_ — no nesting; the main thread of a session _is_ that session's orchestrator. "Main thread" is **role-indexed, not globally unique**, and this line is silent on (not prohibitive of) a Steward-commissioned orchestrator — see `dev/design/orchestration.md` § 1.1 for the definition and the chronology.
- **Releases:** main thread plans; delegate coding to `implementer` (in worktree); gate the merge on the **codex review** (the repo calls this "codex §9" after `dev/design/orchestration.md`'s own numbering — not §9 of this file) of the implementer's worktree branch — `dev/design/orchestration.md` §3 (spawn) and §4 (verdict promotion). **No `code-reviewer` agent is defined in `.claude/agents/`**; codex is the reviewer of record. When codex is rate-limited or out of budget, fall back to an independent adversarial review subagent rather than stalling (memory `orchestration-execution-traps.md`, `codex-unavailable-use-claude-code-review.md`). Delegate rather than hand-do (memory `steward-delegate-dont-hand-do.md`).
- **Subagents win for fan-out.** Parallel research, format-strict review, output isolation. Examples: searching across crates, auditing a diff, generating an ADR draft.
- **Subagents lose for shared-state edits.** A multi-agent edit pipeline drops tacit context at every handoff. Single-agent for any edit on shared mutable state.
- **Worktrees** are the unit of isolation. Implementer subagents always operate in a fresh worktree; main thread never edits in a subagent's worktree.
- **One writer per checkout — NEVER share a checkout across concurrent file-mutating sessions or subagents.** Two writers in one working tree race on `HEAD`/branch-switch/uncommitted edits — commits land on the wrong branch and one writer's edits surface on another's. Each concurrent writer MUST have its own git worktree (cut off `origin/main`); if you can't isolate, **serialize** (run one, let it finish + verify, then the next). Read-only subagents (audit, search, review) MAY share a checkout and run concurrently. After any concurrent run, verify-from-git and push **explicit branch refs** (HEAD-independent), never relying on current HEAD. (incident 2026-06-29)
- **Valid agent types (Claude Code `subagent_type`): enumerate, do not recall.** Project-defined types are exactly the basenames in `.claude/agents/` — list that directory. The harness enumerates its own built-ins in-session. **No hardcoded list is kept on this line, by design:** the list that used to live here was written at `b52b50c3` (2026-06-28) and went stale twice — at `31a73401`, which added `orchestrator` and `steward`, and again at `999d3f4a`, which added `sealed-orchestrator`. A bullet warning readers to "re-check this snapshot" did not prevent the second recurrence, so the snapshot is gone instead of re-warned. Never guess or invent a type — if none fits, omit `subagent_type` (defaults to `claude`). A wrong type is a hard error that cascades to every child.
- **`steward` is main-thread-only — do NOT spawn it.** A spawned steward structurally cannot perform its defining duty as the propose-first interface to the HITL, because it has no channel to the human — its output returns to its spawner.
- **Canonical mechanics:** `dev/design/orchestration.md` owns Claude-implementer + Codex-reviewer spawn discipline (agent-def anti-chaining, main-thread-owned worktrees, cherry-pick + fix-N + override patterns, worktree cleanup). This file owns the principles; that file owns the spawn discipline.

## 8. Iteration discipline

- **Cap retry-budget at ~2 same-issue corrections.** Beyond that, stop, externalize the plan to the live release board (`dev/plans/runs/STATUS-<version>.md`) or a `dev/plans/` plan file, `/clear`, restart with the plan in front of you.
- **Compact-aware.** Anything that must survive compaction goes on disk: ADRs, the release board, `dev/plans/release-state-*.json`, the ledgers, plan files, memory entries (§ 9). Do not rely on chat to remember.
- **Front-load invariants, end-load tasks.** When prompting yourself or constructing context, put rules near the top, the immediate task near the bottom (lost-in-the-middle).

## 9. Pointers

- **Decisions:** `dev/adr/ADR-0.6.0-decision-index.md`
- **Interfaces:** `dev/interfaces/{rust,python,typescript,cli,wire}.md`
- **Method on-ramp:** `dev/agent-harness-bootstrap-prompt.md` (portable distillation — the plan-vs-runbook split, 7 principles, the operating loop)
- **Orchestration mechanics:** `dev/design/orchestration.md` (cross-release runbook: Claude+Codex spawn discipline, § 1.6 preflight gate, § 13 failure catalog)
- **Preflight gate:** `scripts/preflight.sh` (run before every worktree spawn — stale-base + dep-CLOSED + disk)
- **Plan / slice templates:** `dev/plans/prompts/PLAN-TEMPLATE.md` (per-release plan + authoring checklist) · `dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md` (per-slice prompt)
- **Plans:** `dev/plans/`
- **Research:** `dev/notes/context-research-agentic-best-practices.md` (the best-practices synthesis this file operationalizes)
- **Memory:** `MEMORY.md` in the out-of-repo memory store (§ 1) — index of the kebab-case topic entries; auto-loaded at Claude Code session start.

### Release state — where the current release actually lives

- **Board of record:** `dev/plans/runs/STATUS-<version>.md` — one per release; slice ladder, what landed at which sha, current state, immediate next action. **Find the live one via the single writer below** — `ls dev/plans/release-state-*.json` resolves to exactly one file; its `board` key names the live board. ⛔ Do not `ls dev/plans/runs/`: it holds 24 `STATUS-*.md` and nothing marks which is current. Update at every slice close; verify state from git, never from narration.
- **Single writer:** `dev/plans/release-state-<version>.json` — machine-readable ladder, `next_slice`, acceptance + publish gate, ruled/unruled decisions. It owns generated regions in the board and the master plan — HTML comments beginning `BEGIN GENERATED release-state:` — enforced by `scripts/check-release-state-views.sh`. **Edit the JSON, never the generated blocks.** ⛔ That marker is written INCOMPLETE on purpose: the checker hard-fails on any orphan occurrence in any tracked `.md`, so spelling it out here would red the gate. Do not "complete" it, and never paste a real marker into a file the state file does not declare.
- **Ledgers (append-only JSONL):** `dev/steward/steward-ledger.jsonl` (program decisions, `seq-N`) · `dev/todos-and-considerations-ledger.jsonl`. Read/write only via the `ledgerwatch` / `ledgerwrite` tools under `dev/agent-tools/` — never hand-edit.
- **Role contracts (durable):** `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` · `dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`
- **Session hand-offs (dated, newest wins):** `dev/plans/runs/STEWARD-SESSION-HANDOFF-<YYYY-MM-DD>-<A|B|…>.md` — the trailing letter is a per-day sequence. `scripts/steward-orient.sh` prints the newest; read that one, not the series.
- **Workflow entry points:** Claude Code loads the launchers in `.claude/commands/`. Codex discovers the shared skill wrappers in `.codex/skills/`; use natural-language requests — “act as the FathomDB Program Steward”, “orchestrate this FathomDB release plan”, or “enumerate the open FathomDB HITL decisions”. ⛔ Codex TUI slash commands are reserved and cannot be repurposed, so `/steward`, `/orchestrate`, `/orch`, and `/decisions` are Claude-only. Each Codex skill loads the matching launcher and does not duplicate its workflow rules.
- **Historical:** `dev/progress/` — frozen at 0.6.x. Read for history; never write. Superseded by the board + `dev/plans/release-state-*.json` above.

## 10. Forbidden patterns

Hard-won from prior incidents. Violations are correctness bugs, not style preferences.

- **No mocking the database.** Integration tests hit a real database. (prior incident)
- **No skipping hooks.** Never `--no-verify`, never `--no-gpg-sign` unless explicitly approved.
- **No backwards-compatibility shims** in pre-1.0 rewrites.
- **No data migrations** in managed-vec projection releases — schema additions are additive ALTERs, never `INSERT…SELECT` across legacy tables. Enforced by `scripts/agent-lint-migrations.sh` in the lint verb; applied form: `dev/design/slice-G0-design.md`.
- **No `c_char` hardcoded as `i8` or `u8`** at C interop boundaries — always `std::os::raw::c_char`. It is `i8` on x86_64/Darwin and `u8` on aarch64 Linux; hardcoding breaks one target, and only the cross-platform CI matrix catches it.
- **No `pip install` + manual `cargo build` + `cp`** for Python native modules — use `pip install -e src/python/`. Packaging contract: `dev/design/bindings.md`. (Also never `pip install -e` or `maturin develop` from a worktree — memory `agent-worktree-stale-base-trap.md`.)
- **No `yaml.safe_load` as workflow validator** — use `actionlint`; it is wired as the canonical validator in `scripts/agent-lint.sh` and `.github/workflows/release.yml`. `yaml.safe_load` passes schema-invalid syntax GitHub silently rejects.
- **No "fix in 0.7"** for reliability bugs. Net-negative LoC on reliability releases. (`dev/learnings.md`)
- **No "green CI = done"** for releases — install the published wheel from the registry and run end-to-end open/close/exit before declaring done. `dev/design/release.md` § "Post-publish smoke"; memory `release-publish-gotchas.md` (pushing a `v*` tag auto-fires the real multi-registry publish) and `release-dod-requires-full-workspace-gate.md` (full-workspace clippy + check before any green claim).
- **No shared checkout across concurrent writers.** Never run more than one file-mutating session/subagent in the same working tree at once — worktree-isolate each writer (off `origin/main`) or serialize them (§ 7). (incident 2026-06-29; memory `shared-checkout-branch-can-be-stale-vs-session-env.md`)
- **Vector identity belongs to the embedder** — vector configs never carry identity strings. `dev/adr/ADR-0.6.0-vector-identity-embedder-owned.md` (accepted; decision-index row 11).

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
- **Every path this file cites must resolve.** Before committing a change here, confirm each backticked path/filename exists — in the repo, or in the memory store for memory entries (§ 1). Unverified citations are how this file came to cite ten `feedback_*.md` / `project_*.md` files that never existed anywhere, plus `dev/progress/<release>.md` two releases after that directory froze. Prefer pointing at a **directory to enumerate** over transcribing a list that will rot (§ 7, agent types).
