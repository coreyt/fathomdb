---
title: 0.6.0 HITL Resolution Queue (errors.md / recovery.md / engine.md)
date: 2026-04-30
target_release: 0.6.0
status: open
desc: Sequenced HITL queue for Phase 3d design files. Each item has an agent research prompt + structured deliverable contract. Resolve top-down; some downstream items depend on upstream resolutions.
---

# HITL Resolution Queue — Phase 3d (errors / recovery / engine)

## Purpose

Phase 3d design files (`errors.md`, `recovery.md`, `engine.md`) carry HITL touchpoints that must be resolved before draft. This queue sequences resolution + provides per-item agent prompts. Bindings.md is locked; this queue does NOT re-open it.

## Resolution rules

- **Order is load-bearing.** Top items unblock downstream items. Resolve sequentially. Each resolution may invalidate downstream pro/con sets — re-run downstream agents if upstream choice diverges from their assumed precondition.
- **One agent run per HITL item** unless interaction-flag invalidates output.
- **Agent type:** prefer `general-purpose` for research synthesis; `architecture-inspector` for ADR-impact + cross-doc consistency checks; `Explore` for fast codebase grep when the question hinges on existing 0.5.x behavior.
- **HITL gate** lives with main thread (orchestrator). Agent produces structured analysis + recommendation; main thread + user makes the call.
- **Output contract** (every agent MUST return exactly these 7 fields):
  1. **ID** — verbatim from this queue (e.g. `E2`).
  2. **Title** — short noun phrase.
  3. **Description** — exactly 3 sentences. What the question is, why it matters, where the answer lands.
  4. **Pro/Con list** — each option in scope; ≤4 pros + ≤4 cons per option. No padding.
  5. **Interactions** — ≤3 sentences. Names other HITL IDs whose decisions are coupled to this one + how.
  6. **Recommendation** — single chosen option + one-sentence why.
  7. **Confidence %** — integer 0–100. Calibration: 90%+ = strong evidence; 70–89% = clear lean; 50–69% = preference under uncertainty; <50% = punt to human.

- **Cross-doc artifacts.** When a resolution affects >1 file, the agent MUST list which files need a citation/edit (e.g. "errors.md § 2 + bindings.md § 3 + recovery.md § 1").

---

## Sequence

### Tier 1 — Foundation (resolves canonical taxonomy + cadence + step order)

#### `E2` — `EngineOpenError` variant set

**Agent prompt:**
> Research and recommend the full variant set for the Rust `EngineOpenError` enum introduced by ADR-0.6.0-corruption-open-behavior. The candidate variants are: `Corruption`, `Locked`, `Migration`, `EmbedderIdentityMismatch`, `EmbedderWarmupFailed`, `PragmaApplicationFailed`. For each candidate, decide: (a) live in `EngineOpenError` (open-only), (b) live in `EngineError` (runtime, also surfaced at open via `From`), or (c) merge into another variant.
>
> Constraints to honor:
> - ADR-0.6.0-error-taxonomy: per-module errors compose via `#[from]` into `EngineError`; `#[non_exhaustive]`; sanitize foreign causes.
> - ADR-0.6.0-corruption-open-behavior § 2: `OpenStage` MUST NOT include `LockAcquisition` (lock failure is `Locked`, not corruption).
> - bindings.md § 3: bindings flatten as Python `EngineError` base; TS may keep two roots. Variant placement should not force binding-side gymnastics.
> - Reliability principle (memory): no escape hatches; one variant per distinct user remediation.
>
> Read: ADR-0.6.0-error-taxonomy, ADR-0.6.0-corruption-open-behavior, ADR-0.6.0-database-lock-mechanism, ADR-0.6.0-embedder-protocol, design/bindings.md § 3 + § 7 + § 11.
>
> Return the 7-field structured output. Cover EACH candidate variant in pro/con (treat each as a sub-decision). Recommendation = the canonical variant set for `EngineOpenError` and the canonical variant set for `EngineError` open-time surface. Flag any variant that is genuinely ambiguous (confidence <70%) for human decision.

**Blocks:** E3, E5, E6, E7, X1.

---

#### `R1` — Always-on corruption detection set

**Agent prompt:**
> Recommend the always-on corruption detection set that runs at every `Engine.open`, plus the cheap-only and opt-in tiers, for 0.6.0. Constraint (anti-regression): once frozen, reducing the always-on set requires an ADR amendment per ADR-0.6.0-corruption-open-behavior § 4.
>
> Candidate detection steps to evaluate:
> - WAL replay error (free; surfaces from rusqlite open).
> - Schema-version mismatch / missing migration table (free; first SQL).
> - Embedder identity check vs stored profile (free-ish; one row read).
> - sqlite-vec `vec0` shadow-table consistency (cost: depends on row count + check shape).
> - `PRAGMA quick_check` (O(N) but cheaper than integrity_check; partial coverage).
> - `PRAGMA integrity_check` (O(N), thorough; almost certainly opt-in for any non-trivial DB).
> - Header magic / page-size sanity check.
>
> For each: tier assignment (always / cheap / opt-in), cost class (O(1) / O(rows) / O(pages)), what it detects that nothing else does, opt-in surface (env var, config flag, CLI subcommand, or N/A).
>
> Read: ADR-0.6.0-corruption-open-behavior § 4, requirements.md REQ-031b/c/d + REQ-024 + REQ-025a/b/c, acceptance.md AC-035*. Use Explore agent if needed to check 0.5.x behavior in `crates/fathomdb-engine/src/runtime.rs` and `crates/fathomdb-engine/src/recovery/*`.
>
> Return the 7-field structured output. Pro/con per **tier composition** (alternative always-on sets), not per individual check. Recommendation = a specific 3-tier list. Confidence calibrated to: how confident that this is the minimum set we can defend long-term without amendment.

**Blocks:** R3, R4, R6, R7, ENG1, X1, X6.

---

#### `R3` — `fathomdb recover` + `fathomdb doctor` verb set

**Agent prompt:**
> Recommend the canonical CLI verb set for recovery + diagnostics in 0.6.0. Two binaries-or-subcommand-roots are in scope per ADR-0.6.0-cli-scope and architecture.md § 1: `fathomdb recover` (lossy mutation paths) and `fathomdb doctor <verb>` (read-only + bit-preserving diagnostics).
>
> Candidate verbs to evaluate (assign each to `recover` / `doctor` / out-of-scope-for-0.6.0):
> - `recover --accept-data-loss` (top-level lossy: WAL truncate + projection rebuild).
> - `doctor check-integrity` (aggregator: runs always-on + cheap-only checks; produces report).
> - `doctor safe-export <out>` (REQ-039 SHA-256 manifest export; bit-preserving).
> - `doctor rebuild-vec0` (lossy: drops + rebuilds vec0 shadow tables from canonical).
> - `doctor truncate-wal` (lossy: drops uncommitted WAL).
> - `doctor verify-embedder` (read-only: identity check vs stored profile).
> - `doctor migrate` (apply pending migrations without opening engine — needed?).
>
> For each: subcommand path, bit-preserving y/n, requires `--accept-data-loss` y/n, exit-code class, JSON-output `--json` requirement (REQ-024).
>
> Read: ADR-0.6.0-cli-scope, ADR-0.6.0-corruption-open-behavior § 3, requirements.md REQ-012 + REQ-024 + REQ-025a/b/c + REQ-026 + REQ-035 + REQ-036 + REQ-037 + REQ-038 + REQ-039 + REQ-054, design/bindings.md § 1 + § 10. Use Explore for 0.5.x CLI surface in `crates/fathomdb-cli/`.
>
> Return the 7-field structured output. Pro/con per **verb-set shape** (e.g. minimal vs full vs split-binary). Recommendation = explicit verb table.

**Depends on:** R1 (always-on set determines what `doctor check-integrity` runs).
**Blocks:** R4, R6, R7.

---

#### `ENG1` — `Engine.open` step ordering

**Agent prompt:**
> Recommend canonical step ordering for `Engine.open`. Proposed 10-step order (from main-thread analysis):
>
> 1. canonicalize path
> 2. acquire sidecar `{path}.lock` flock (fail-fast pre-IO)
> 3. open SQLite writer; `PRAGMA locking_mode=EXCLUSIVE`; first read (acquires SQLite native lock)
> 4. apply remaining PRAGMAs (synchronous=NORMAL, journal_mode=WAL)
> 5. always-on corruption detection set (per R1)
> 6. schema migration loop (per-step events, REQ-042)
> 7. embedder identity check
> 8. embedder eager warmup (Invariant D)
> 9. spawn writer thread + scheduler runtime
> 10. return `Engine` handle
>
> Validate the order. Specifically check: must journal_mode=WAL precede locking_mode=EXCLUSIVE? Where do the always-on detection checks fire relative to migrations (BEFORE migrations to avoid running migrations on corrupt DB? AFTER to allow migration to repair some classes? SPLIT — some before, some after)? Does embedder identity check require an applied schema (i.e. must follow migrations)? When is the writer thread safe to spawn — pre-warmup or post-warmup?
>
> Failure semantics (ADR-0.6.0-corruption-open-behavior § 5): on failure at any step, release lock + close conn + no engine handle returned. Must this hold even after step 9 (writer thread already spawned)?
>
> Read: ADR-0.6.0-corruption-open-behavior, ADR-0.6.0-database-lock-mechanism, ADR-0.6.0-async-surface (Invariant D), ADR-0.6.0-embedder-protocol, ADR-0.6.0-vector-identity-embedder-owned, architecture.md § 5, design/bindings.md § 7, requirements.md REQ-042. Check 0.5.x `Engine::open` order via Explore.
>
> Return the 7-field structured output. Pro/con per **alternative orderings** considered. Recommendation = numbered final order. Flag any step pair where order is genuinely ambiguous.

**Depends on:** E2 (variant placement), R1 (always-on set).
**Blocks:** ENG3, ENG4, ENG6, X3.

---

#### `X1` — Single canonical `(stage, kind, locator, code, doc_anchor)` table

**Agent prompt:**
> Recommend whether `errors.md` should ship a single canonical table joining `OpenStage`, `CorruptionKind`, `CorruptionLocator`, `recovery_hint.code`, and `recovery_hint.doc_anchor`, with engine.md + recovery.md citing rows by stable code rather than re-listing.
>
> Alternative shapes:
> - **Single table in errors.md, others cite by code.** Strong consistency; one source of truth; easy diff review.
> - **Per-file fragmented tables.** Each file owns its column; coupling-by-prose. More flexibility, more drift risk.
> - **Hybrid: errors.md owns enum membership; recovery.md owns code→action mapping; engine.md owns stage→detection wiring.** Boundaries align with file responsibilities; risks consistency without explicit cross-file invariant.
>
> The decision is structural — does the design tree carry a join table or three siloed tables. Test: which shape makes round-trip validation (every code has a doc_anchor that resolves; every locator has a producing detection step) easiest to enforce in CI?
>
> Read: ADR-0.6.0-corruption-open-behavior § 2 + § 4, design/bindings.md § 3, architecture.md § 2 module assignments. No code grep needed.
>
> Return the 7-field structured output. Pro/con per **shape**. Recommendation = chosen shape + which file owns the canonical table + cite-by-code protocol.

**Depends on:** E2, R1, R3 (need to know the variant + detection + verb sets to size the table).
**Blocks:** X4, X6.

---

### Tier 2 — Cross-cutting policy

#### `X4` — Foreign-error sanitization recipe ownership + content

**Agent prompt:**
> Recommend the canonical foreign-error sanitization recipe per foreign cause type, owned by `errors.md` and cited by every module-error wrapper in engine.md + recovery.md + op-store.md. ADR-0.6.0-error-taxonomy commits the policy; this HITL produces the per-type recipe.
>
> Foreign causes in scope:
> - `rusqlite::Error` (carries SQL fragments + extended codes; PII risk via parameter values).
> - `std::io::Error` (carries absolute paths via `OsStr`; PII risk).
> - `serde_json::Error` (carries byte offsets + sometimes payload fragments).
> - `tokio::time::error::Elapsed` (no PII; bare timeout).
> - `napi::Error` / `pyo3::PyErr` (binding-layer; should never reach engine errors but defense-in-depth).
>
> For each: Display-string redaction recipe, what to keep (extended codes, error class names), what to drop (SQL text, absolute paths, byte offsets, parameter values), how `Error::source` chain is preserved for engine-internal logging.
>
> Read: ADR-0.6.0-error-taxonomy § Foreign-error wrapping policy, design/bindings.md § 3 (typed-attrs-not-stringified-payloads).
>
> Return the 7-field structured output. Pro/con per **per-type recipe** (e.g. for rusqlite: "sanitize fully" vs "sanitize SQL only, keep extended code in Display"). Recommendation = a per-type recipe table.

**Depends on:** none beyond ADR.
**Blocks:** none directly; informs errors.md draft.

---

#### `X6` — Round-trip validation contract for `recovery_hint`

**Agent prompt:**
> Recommend a CI/test contract that enforces round-trip integrity of `recovery_hint`:
> - Every `recovery_hint.code` produced by the engine has a `doc_anchor` that resolves to an existing heading in `design/recovery.md` (or the operator-facing recovery doc derived from it).
> - Every always-on detection step in recovery.md produces a code that exists in errors.md's enum.
> - Every `CorruptionLocator` variant in errors.md is produced by at least one detection step in recovery.md (no orphan locators).
>
> Alternative enforcement mechanisms:
> - Compile-time: `const` table in Rust + `static_assertions`; each ADR/design doc embeds the corresponding generated frontmatter.
> - Test-time: integration test parses errors.md + recovery.md as markdown, enforces the three round-trip invariants.
> - Doc-build-time: docs build script with hard-fail on broken anchors.
> - None: discipline only, reviewed by humans.
>
> Read: ADR-0.6.0-corruption-open-behavior § 2 + § 4, acceptance.md AC-035*, design/bindings.md § 3.
>
> Return the 7-field structured output. Pro/con per **enforcement mechanism**. Recommendation = chosen mechanism + where its setup lives (CI pipeline, test crate, docs build hook).

**Depends on:** X1 (table shape), R1 (detection set), R3 (verb set).
**Blocks:** none; informs acceptance.md AC update + test-plan.

---

### Tier 3 — Remaining errors.md HITL

#### `E3` — `CorruptionKind` / `OpenStage` / `CorruptionLocator` enum membership

**Agent prompt:**
> Enumerate the variants of `CorruptionKind`, `OpenStage`, `CorruptionLocator` (per ADR-0.6.0-corruption-open-behavior § 2 delegation to errors.md). Justify every variant; ADR § 2 anti-regression: "design/errors.md MUST justify every locator variant."
>
> Hard constraints:
> - `OpenStage` MUST NOT include `LockAcquisition`.
> - `CorruptionLocator` MUST NOT have free-form `Unspecified`; opaque SQLite errors → `OpaqueSqliteError { sqlite_extended_code: i32 }`.
>
> Cross-references: `OpenStage` aligns 1:1 with the `Engine.open` steps that can detect corruption per ENG1. `CorruptionKind` aligns with the always-on detection set per R1. `CorruptionLocator` aligns with what each detection produces (file offset, page id, table+rowid, vec0-shadow-row, sqlite extended code, etc.).
>
> Return the 7-field structured output. Pro/con per **enum scope** (minimal vs full enumeration). Recommendation = full variant list per enum with one-sentence justification each.

**Depends on:** ENG1, R1, X1.

---

#### `E5` — Migration error home

**Agent prompt:**
> Recommend whether migration failure surfaces as `EngineOpenError::Migration { step, cause }` or `EngineError::Storage(StorageError::Migration {...})`. This is partially decided by E2 but enumerated here for thoroughness.
>
> Pro/con per home. Interaction with REQ-042 (per-step structured progress events): does the variant carry the step list up-to-failure or just the failing step?
>
> Return the 7-field structured output.

**Depends on:** E2.

---

#### `E6` — Embedder warmup failure home

**Agent prompt:**
> Recommend whether eager-warmup failure (Invariant D) at `Engine.open` surfaces as `EngineOpenError::EmbedderWarmupFailed` or `EngineError::Embedder(EmbedderError::Warmup{..})`.
>
> Constraint: Invariant D timeout default 30s. Failure may be caller-controllable (user-supplied embedder bug) vs default-embedder-internal. Does the variant carry that distinction?
>
> Return the 7-field structured output.

**Depends on:** E2.

---

#### `E7` — `DatabaseLocked` variant shape

**Agent prompt:**
> Confirm shape of `EngineOpenError::Locked` per ADR-0.6.0-database-lock-mechanism. Specifically: is `holder_pid` required (PID written to sidecar by lock acquirer) or optional (sidecar may be stale / unreadable)? Should the variant also carry `holder_started_at: SystemTime` for staleness detection?
>
> Read: ADR-0.6.0-database-lock-mechanism, design/bindings.md § 7. Check `crates/fathomdb-engine/src/database_lock.rs` via Explore for current sidecar payload schema.
>
> Return the 7-field structured output.

**Depends on:** none.

---

#### `E8` — Foreign-error sanitization concrete recipe placement

Subsumed by X4. Skip if X4 resolved.

---

### Tier 4 — Remaining recovery.md HITL

#### `R2` — Cheap-only vs opt-in tier knob spelling

**Agent prompt:**
> Recommend the env / config knob surface for opt-in detection (e.g. `PRAGMA integrity_check`). Candidates: `FATHOMDB_INTEGRITY_CHECK=on`, `EngineConfig.integrity_check_at_open: bool`, CLI-only via `fathomdb doctor check-integrity --deep`. Honor bindings.md § 6 config-knob symmetry: if SDK exposes it, all bindings expose it.
>
> Return the 7-field structured output.

**Depends on:** R1, R3.

---

#### `R4` — `--accept-data-loss` scope

**Agent prompt:**
> For each verb in the R3 verb set, decide bit-preserving y/n + whether `--accept-data-loss` is required. Constraint per ADR-0.6.0-corruption-open-behavior § 3: "any path not bit-preserving" requires the flag.
>
> Return the 7-field structured output. Output = annotated verb table.

**Depends on:** R3.

---

#### `R5` — `safe_export` manifest format

**Agent prompt:**
> Recommend the manifest format for `doctor safe-export`. Constraints: REQ-039 (SHA-256 per file); REQ-040 latency target — cite P-NNN from acceptance.md, do NOT duplicate. Format candidates: NDJSON (one record per file), single JSON object with file array, plain `sha256sum`-compatible text.
>
> Return the 7-field structured output.

**Depends on:** R3.

---

#### `R6` — Detection-step ↔ `CorruptionLocator` 1:1 alignment

Subsumed by X1 + X6. Skip if both resolved.

---

#### `R7` — Detection-step ↔ `recovery_hint.code` ↔ recovery action

Subsumed by X1 + X6 + R3. Skip if all resolved.

---

#### `R8` — Lock semantics during recovery

**Agent prompt:**
> Confirm: does `fathomdb recover` (and `fathomdb doctor` mutating verbs) acquire the same hybrid lock as `Engine.open` (sidecar flock + SQLite EXCLUSIVE writer per ADR-0.6.0-database-lock-mechanism)? What about read-only `doctor` verbs — do they acquire ANY lock, or attach as a normal-mode reader?
>
> Read: ADR-0.6.0-database-lock-mechanism, design/bindings.md § 7. Check `crates/fathomdb-cli/src/recovery.rs` (or equivalent) via Explore.
>
> Return the 7-field structured output.

**Depends on:** R3.

---

#### `R9` — `check-integrity` aggregator output schema

**Agent prompt:**
> Recommend output schema for `doctor check-integrity`. REQ-024 mandates JSON output (machine-readable). Decide: schema shape (versioned), human pretty-print fallback (yes/no), per-check pass/fail granularity (boolean vs structured detail), exit-code semantics on partial failure.
>
> Return the 7-field structured output.

**Depends on:** R1 (which checks run).

---

### Tier 5 — Remaining engine.md HITL

#### `ENG2` — Reader-pool default sizing + knob name

**Agent prompt:**
> Confirm reader-pool sizing default (`num_cpus / 2` per Phase 3c handoff). Recommend config knob name (`reader_pool_size`?). Honor bindings.md § 6: every binding exposes it.
>
> Read: requirements.md REQ-018 (multi-reader concurrency), architecture.md § 2 reader row, design/bindings.md § 6.
>
> Return the 7-field structured output.

**Depends on:** none.

---

#### `ENG3` — Writer thread lifetime + Engine.close shutdown order

**Agent prompt:**
> Confirm writer thread spawn point (per ENG1 step 9) + shutdown order at `Engine.close` (cite ADR-0.6.0-scheduler-shape § Engine.close shutdown protocol). Specifically: scheduler quiesce → writer drain → writer thread join → reader pool close → SQLite conn close → release sidecar lock fd. Validate order against AC-022a (close releases lock).
>
> Read: ADR-0.6.0-scheduler-shape, ADR-0.6.0-database-lock-mechanism, requirements.md REQ-020a, acceptance.md AC-022a.
>
> Return the 7-field structured output.

**Depends on:** ENG1.

---

#### `ENG4` — PRAGMA application order

**Agent prompt:**
> Confirm PRAGMA order at open: `journal_mode=WAL` → `locking_mode=EXCLUSIVE` (writer only) → `synchronous=NORMAL`. Reader connections use NORMAL locking_mode (per bindings.md § 7). Validate with rusqlite + sqlite-vec docs that journal_mode must precede locking_mode.
>
> Use general-purpose agent with web research on SQLite PRAGMA ordering constraints. Read ADR-0.6.0-durability-fsync-policy + ADR-0.6.0-database-lock-mechanism.
>
> Return the 7-field structured output.

**Depends on:** ENG1.

---

#### `ENG5` — Migration progress event payload owner

**Agent prompt:**
> Decide: does engine.md own the migration-progress event payload schema, or design/lifecycle.md (which owns the AC-001 phase-tag enum)?
>
> Read: requirements.md REQ-042, acceptance.md AC-001, architecture.md § 2 lifecycle row + migrations row.
>
> Return the 7-field structured output.

**Depends on:** none.

---

#### `ENG6` — `Engine.open` structured result shape

**Agent prompt:**
> Define `OpenReport` struct returned from `Engine.open` (per bindings.md § 2 Invariant D citation). Fields: migration step list with durations, embedder warmup duration, lock acquisition timestamp, detection results from R1 always-on set. Field names + Rust types + binding marshalling.
>
> Read: design/bindings.md § 2, requirements.md REQ-042, ADR-0.6.0-async-surface § Decision (eager warmup reporting).
>
> Return the 7-field structured output.

**Depends on:** ENG1, R1.

---

#### `ENG7` — `embedder_pool_size` default vs reader pool default

**Agent prompt:**
> Confirm `embedder_pool_size` default = `num_cpus::get()` (per architecture.md § 6 + ADR-0.6.0-default-embedder) is intentional vs reader-pool default = `num_cpus / 2`. Justify divergence or recommend alignment.
>
> Read: architecture.md § 6, ADR-0.6.0-default-embedder, ADR-0.6.0-async-surface (Invariant B + D).
>
> Return the 7-field structured output.

**Depends on:** ENG2.

---

#### `ENG8` — Per-call embedder timeout default citation

Trivial: cite P-NNN from acceptance.md Parameter table for 30s default. No HITL needed; mechanical during draft. Skip.

---

## Resolution sign-off

When all Tier 1 + Tier 2 items resolved, main thread:

1. Updates this file's `status: open` → `status: tier-1-resolved` (then per-tier).
2. Edits affected ADRs / requirements / acceptance / bindings.md ONLY if a resolution invalidates an existing commitment (loop-back per plan.md).
3. Begins drafts in this order: `errors.md` → `recovery.md` → `engine.md`.
4. Each draft → critic (architecture-inspector) → HITL flip → commit.

Skipped items (E8, R6, R7, ENG8) are subsumed by Tier 1/2 resolutions or trivial.

## Out of scope here

- Bindings.md re-litigation (locked).
- Architecture.md amendments (only loop-back if Tier 1 resolution forces it).
- Design files beyond errors / recovery / engine (separate queue).
