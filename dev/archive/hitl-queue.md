---
title: 0.6.0 HITL Resolution Queue (errors.md / recovery.md / engine.md)
date: 2026-04-30
target_release: 0.6.0
status: tier-4-resolved
desc: Sequenced HITL queue for Phase 3d design files. Each item has an agent research prompt + structured deliverable contract. Resolve top-down; some downstream items depend on upstream resolutions.
progress:
  - 2026-04-30 E2 resolved (conf 82%); E6 initially left for human, then resolved later the same day.
  - 2026-04-30 R1 resolved (conf 78%); composition A — minimal always-on.
  - 2026-04-30 R3 resolved (conf 74%); shape A — two roots, lossy concentrated under `recover`.
  - 2026-04-30 ENG1 resolved (conf 88%); 11-step order, ADR violation fixed (WAL before EXCLUSIVE), R1 split pre/post-migration.
  - 2026-04-30 X1 resolved (conf 88%); Shape A — canonical table in `errors.md`, cite-by-code from engine.md + recovery.md.
  - 2026-04-30 Tier 1 complete.
  - 2026-04-30 E6 resolved by HITL (runtime-rooted `EmbedderError::Warmup`; E2 unchanged); ENG1 corrected order ADOPTED. Downstream annotations + test plan added below.
  - 2026-04-30 Tier 2 dispatched (X4 + X6 in parallel).
  - 2026-04-30 X4 resolved (conf 84%); per-foreign-type recipe table.
  - 2026-04-30 X6 resolved (conf 78%); test-time integration test + compile-time backstop.
  - 2026-04-30 Tier 3 dispatched (E3 + E7 in parallel).
  - 2026-04-30 E3 resolved (conf 84%); 5 OpenStage + 5 CorruptionKind + 6 CorruptionLocator variants.
  - 2026-04-30 E7 resolved (conf 95%); confirms E2 pre-commit `Locked{holder_pid: Option<u32>}`; rejects holder_started_at + hostname.
  - 2026-04-30 ENG3 resolved (conf 80%); explicit fallible `close()` + best-effort `Drop`; 30s drain timeout; detach + lock-release on timeout. Test plan extended (#27-30). Tracking issue #60 for AsyncDrop migration.
  - 2026-04-30 ENG6 resolved (conf 88%); idiomatic-only marshalling for OpenReport/CloseReport (ms sufficient — SLI surfaces are separate Report types not yet scoped). Over-design avoided after FathomDB sub-ms-use-case enumeration showed no SLI consumer reads these fields.
  - 2026-04-30 R9 resolved (conf 80%); single JSON object (3 sections per AC-043a/b), per-finding {code, stage, locator, doc_anchor, detail}, single exit code from R3 doctor-check class, `--pretty` fallback. No schema_version, no manifest envelope, no NDJSON (deferred non-breaking).
  - 2026-04-30 Tier 4 dispatched in parallel (R2, R4, R5, R8, R9).
  - 2026-04-30 R2 initially resolved (conf 78%) to env knob `FATHOMDB_OPEN_INTEGRITY_CHECK=off|quick|full` + R3 CLI verb, with SDK config deferred.
  - 2026-04-30 R4 resolved (conf 86%); root-level `--accept-data-loss` on `recover` only. Per-sub-flag opt-in REJECTED (over-design falsified). `--restore-logical-id` adjudicated lossy.
  - 2026-04-30 R5 resolved (conf 88%); plain `sha256sum`-compatible text manifest (`<hex>  <relpath>\n`). Schema versioning + JSON envelope REJECTED (over-design). Prompt errata: REQ-035 (not 039) is correct cite; no P-NNN exists for safe-export latency.
  - 2026-04-30 R8 resolved (conf 86%); strategy β tiered — `recover` acquires hybrid lock per ENG1; read-only `doctor` skips sidecar + uses NORMAL reader. Strategy γ ("read-lock sidecar") REJECTED (over-design).
  - 2026-04-30 R2 reconsidered after over-design audit found AC-035a `IntegrityCheckFailed` fixture as the only concrete 0.6.0 consumer of opt-in detection at `Engine.open`.
  - 2026-04-30 R2 amended (conf 86%); final choice = no opt-in detection surface at `Engine.open` in 0.6.0. Keep opt-in integrity checks on `doctor check-integrity` only; do not ship env or SDK-config open-time knob.
  - 2026-04-30 E3 amended by R2; open-path enums shrink to 4 `OpenStage` + 4 `CorruptionKind` variants. `doctor check-integrity --full` RETAINS dedicated page-damage finding code `E_CORRUPT_INTEGRITY_CHECK`, but that code is doctor-only report surface, not `Engine.open` corruption enum surface.
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
>
> - ADR-0.6.0-error-taxonomy: per-module errors compose via `#[from]` into `EngineError`; `#[non_exhaustive]`; sanitize foreign causes.
> - ADR-0.6.0-corruption-open-behavior § 2: `OpenStage` MUST NOT include `LockAcquisition` (lock failure is `Locked`, not corruption).
> - bindings.md § 3: bindings flatten as Python `EngineError` base; TS may keep two roots. Variant placement should not force binding-side gymnastics.
> - Reliability principle (memory): no escape hatches; one variant per distinct user remediation.
>
> Read: ADR-0.6.0-error-taxonomy, ADR-0.6.0-corruption-open-behavior, ADR-0.6.0-database-lock-mechanism, ADR-0.6.0-embedder-protocol, design/bindings.md § 3 + § 7 + § 11.
>
> Return the 7-field structured output. Cover EACH candidate variant in pro/con (treat each as a sub-decision). Recommendation = the canonical variant set for `EngineOpenError` and the canonical variant set for `EngineError` open-time surface. Flag any variant that is genuinely ambiguous (confidence <70%) for human decision.

**Blocks:** E3, E5, E6, E7, X1.

**Status:** RESOLVED 2026-04-30 (confidence 82%).

**Resolution:**

`EngineOpenError` (open-only, `#[non_exhaustive]`):

- `Io { source: std::io::Error }` — path canonicalize, sidecar create, non-PRAGMA IO.
- `Locked { holder_pid: Option<u32> }` — sidecar flock contention; full shape per E7.
- `Corruption(CorruptionDetail)` — per ADR-0.6.0-corruption-open-behavior § 2.
- `Migration { step: MigrationStep, cause: SanitizedCause }` — REQ-042-shaped.
- `Engine(#[from] EngineError)` — composes runtime errors that may surface at open.

`EngineError` variants surfacing at open via `#[from]`:

- `EngineError::EmbedderIdentityMismatch(#[from] EmbedderIdentityMismatchError)` — locked by ADR-0.6.0-error-taxonomy.
- `EngineError::Embedder(#[from] EmbedderError)` with new `EmbedderError::Warmup` sub-variant — CONFIRMED by E6 (2026-04-30, conf 78%).

REJECTED: `PragmaApplicationFailed` as standalone variant. PRAGMA failures wrap into `EngineOpenError::Io` (sanitized rusqlite cause); `locking_mode=EXCLUSIVE` busy is `Locked`.

**ENG1 step→variant mapping (locked 2026-04-30):**
| Step | Action | EngineOpenError variant on failure |
|---|---|---|
| 1 | canonicalize path | `Io` |
| 2 | sidecar flock | `Locked{holder_pid}` or `Io` |
| 3 | open SQLite writer conn | `Io` |
| 4 | `PRAGMA journal_mode=WAL` | `Io` (rusqlite cause sanitized) |
| 5 | EXCLUSIVE + sync + fk + first read | `Io` or `Locked` |
| 6 | R1 pre-migration detection | `Corruption(WalReplay/HeaderProbe/SchemaProbe)` |
| 7 | migration loop (REQ-042) | `Migration{step, cause}` or `Corruption` |
| 8 | embedder identity probe | `Engine(EmbedderIdentityMismatch)` or `Corruption(EmbedderIdentity)` |
| 9 | embedder eager warmup | `Engine(Embedder(Warmup))` per E6 |
| 10 | spawn writer + scheduler + reader pool | `Engine(...)` after reverse-ENG3 teardown |

**Cross-doc artifacts:** `design/errors.md` (canonical variant table — owner); `design/engine.md` (step→OpenStage mapping); `design/bindings.md` § 3 + § 11 (verify "more than one top-level error type" wording); `interfaces/python.md` + `interfaces/typescript.md` (per-language class enumeration, downstream); `adr/ADR-0.6.0-error-taxonomy.md` (no edit; already `#[non_exhaustive]`).

---

#### `R1` — Always-on corruption detection set

**Agent prompt:**

> Recommend the always-on corruption detection set that runs at every `Engine.open`, plus the cheap-only and opt-in tiers, for 0.6.0. Constraint (anti-regression): once frozen, reducing the always-on set requires an ADR amendment per ADR-0.6.0-corruption-open-behavior § 4.
>
> Candidate detection steps to evaluate:
>
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

**Status:** RESOLVED 2026-04-30 (confidence 78%). Composition A — minimal always-on.

**Resolution:**

Always-on (frozen by ADR § 4):

- WAL replay error (rusqlite open) → `OpenStage::WalReplay`. O(1).
- Header magic / page-size sanity (one read of page 1) → `OpenStage::HeaderProbe`. O(1).
- Schema-version / migration-table probe (`PRAGMA user_version` + `_fathomdb_migrations` existence) → `OpenStage::SchemaProbe`. O(1).
- Embedder identity check vs stored profile (one row read) → `OpenStage::EmbedderIdentity`. O(1).

Cheap-only (via `fathomdb doctor check-integrity`; NOT at open):

- sqlite-vec `vec0` shadow-table consistency — metadata + row-count parity (NOT per-vector). O(partitions).
- Canonical-table referential sanity (orphan ID scan). O(rows) single pass.
- Provenance / FTS shadow-table existence + row-count parity per declared profile. O(profiles).

Opt-in (explicit operator request only):

- `PRAGMA quick_check` — `FATHOMDB_OPEN_INTEGRITY_CHECK=quick` or `doctor check-integrity --quick`. O(pages).
- `PRAGMA integrity_check` — `=full` or `--full`. O(pages) thorough.
- Vector round-trip / re-embed validation — `doctor check-integrity --round-trip`. O(rows × embedder).

Rationale: composition A is the minimum defensible long-term without amendment — every item is already a precondition for `Engine.open` to function. vec0 metadata check tempting (composition B) but locks fathomdb to current sqlite-vec internals + risks REQ-031c on growth; safer in cheap tier.

**Pre/post-migration split (forced by ENG1 adoption):**

- Pre-migration (ENG1 step 6): WAL replay verdict, header probe, schema probe. Run before migrations to honor "no partial state on corrupt DB" (ADR-corruption § 5).
- Post-migration (ENG1 step 8): embedder identity probe. Reads vector-profile row from migrated schema; cannot run pre-migration.
- `OpenReport` (ENG6) carries TWO `DetectionReport` fields, not one.
- R2 opt-in knob spelling MUST distinguish pre vs post-migration if `quick_check` / `integrity_check` could fire at either (recommendation: opt-in checks ONLY fire post-migration since they require a stable schema).

**Cross-doc artifacts:** `design/recovery.md` (primary owner); `design/engine.md` (ENG1 stage ordering); `design/errors.md` (`OpenStage` variants: `WalReplay`, `HeaderProbe`, `SchemaProbe`, `EmbedderIdentity`, opt-in `IntegrityCheck`); `requirements.md` (verify REQ-031d alignment); `acceptance.md` (AC-035a/b fixtures, P-RECOV-N=10 budget re-confirm); `adr/ADR-0.6.0-corruption-open-behavior.md` § 4 anti-regression now binds these four items.

---

#### `R3` — `fathomdb recover` + `fathomdb doctor` verb set

**Agent prompt:**

> Recommend the canonical CLI verb set for recovery + diagnostics in 0.6.0. Two binaries-or-subcommand-roots are in scope per ADR-0.6.0-cli-scope and architecture.md § 1: `fathomdb recover` (lossy mutation paths) and `fathomdb doctor <verb>` (read-only + bit-preserving diagnostics).
>
> Candidate verbs to evaluate (assign each to `recover` / `doctor` / out-of-scope-for-0.6.0):
>
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

**Status:** RESOLVED 2026-04-30 (confidence 74%). Shape A — two roots; lossy under `recover`.

**Resolution:**

| Verb                             | Path                                                                                                                                                                         | Bit-preserving | Requires `--accept-data-loss` | Exit class                 | `--json`                    |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ----------------------------- | -------------------------- | --------------------------- |
| recover (lossy gate)             | `fathomdb recover --accept-data-loss [--truncate-wal] [--rebuild-vec0] [--rebuild-projections] [--excise-source <id>] [--purge-logical-id <id>] [--restore-logical-id <id>]` | N              | Y mandatory                   | recover-\* (0/64/70/71)    | Y (NDJSON events + summary) |
| check-integrity                  | `fathomdb doctor check-integrity [--quick] [--full] [--round-trip]`                                                                                                          | Y              | N                             | doctor-check-\* (0/65/70)  | Y mandatory                 |
| safe-export                      | `fathomdb doctor safe-export <out> [--manifest <path>]`                                                                                                                      | Y              | N                             | doctor-export-\* (0/66/71) | Y                           |
| verify-embedder                  | `fathomdb doctor verify-embedder`                                                                                                                                            | Y              | N                             | doctor-check-\* (0/65)     | Y                           |
| trace                            | `fathomdb doctor trace --source-ref <id>`                                                                                                                                    | Y              | N                             | doctor-check-\*            | Y                           |
| dump-{schema,row-counts,profile} | `fathomdb doctor dump-…`                                                                                                                                                     | Y              | N                             | doctor-check-\*            | Y                           |

REJECTED `doctor migrate`: would re-introduce open-without-engine path forbidden by ADR-corruption § 5; deferred to 0.6.x if demand surfaces.

Exit codes (numbers fixed in `interfaces/cli.md`): 0 success / 64 partial-recover / 65 issues-found / 66 export-IO / 70 unrecoverable / 71 lock-held.

`--json` mandatory on every verb (REQ-024). Pretty-print is optional fallback (R9 owns for check-integrity).

**Cross-doc artifacts:** `design/recovery.md` (primary owner — new file); `requirements.md` REQ-036 (rewrite to two-root shape); `interfaces/cli.md` (concrete flag spelling + exit numbers); `acceptance.md` AC-035d (broaden to cover doctor non-mutation invariant); `adr/ADR-0.6.0-cli-scope.md` § Decision (cite recovery.md, no amendment needed); `design/errors.md` (variant→exit-code mapping). 0.5.x has no CLI binary — greenfield.

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

**Status:** RESOLVED 2026-04-30 (confidence 88%). ADR violation in proposed order fixed.

**Resolution — corrected canonical 11-step order:**

1. Canonicalize path. Failure → `Io`.
2. Acquire sidecar `{path}.lock` flock (pre-IO, Inv-Lock-1). Failure → `Locked{holder_pid}` or `Io`.
3. Open SQLite writer connection (no read yet). Failure → `Io`.
4. Apply `PRAGMA journal_mode=WAL` (Inv-Lock-2: WAL MUST precede EXCLUSIVE so `-shm` is never created).
5. Apply `PRAGMA locking_mode=EXCLUSIVE` + `synchronous=NORMAL` + `foreign_keys` + first read (Inv-Lock-3). Failure → `Io`/`Locked`.
6. **R1 always-on detection — pre-migration subset:** WAL replay verdict + header/page-1 probe + schema/migration-table probe. Failure → `Corruption(OpenStage::{WalReplay,HeaderProbe,SchemaProbe})`.
7. Schema migration loop (REQ-042 per-step events). Failure → `Migration` or `Corruption`.
8. **R1 always-on — post-migration subset:** embedder identity check vs stored profile. Failure → `Engine(EmbedderIdentityMismatch)` or `Corruption(OpenStage::EmbedderIdentity)`.
9. Embedder eager warmup (Invariant D). Failure → `Engine(EmbedderError)`.
10. Spawn writer thread + scheduler runtime + reader pool + admin handle. On failure: run reverse-ENG3 teardown (drain+join writer, close pools, close conn) BEFORE releasing lock and surfacing error.
11. Return `Engine` handle.

**ADR violation caught:** Original proposed step 3 (`locking_mode=EXCLUSIVE` + first read) BEFORE step 4 (`journal_mode=WAL`) violates ADR-database-lock-mechanism Inv-Lock-2. Steps split + reordered.

**R1 split rationale:** Embedder identity probe needs migrated schema (reads vector-profile row), so cannot be in pre-migration subset. WAL/header/schema probes MUST precede migrations to honor "no partial state on corrupt DB" (ADR-corruption § 5).

**Failure semantics post-step-10:** Even after writer thread spawned, ENG3 close protocol runs in reverse before lock release if any later check fails (Inv-Lock-4 + ADR-corruption § 5).

**Ambiguous flags:**

- Step 8 vs 9 order: identity-then-warmup recommended (fail-fast no warmup cost) but defensible alternative if a binding's `Embedder` impl computes `identity()` lazily from loaded model. ADR-embedder-protocol treats `identity()` as cheap; recommendation stands. Flag for ENG6.
- Step 6 WAL-replay sub-step: implicit in SQLite first read at step 5; "detection" is interpreting the verdict, not invoking a separate API. Document in design/engine.md.

**Cross-doc artifacts:** `design/engine.md` (NEW — primary owner; does not yet exist); `design/errors.md` (step→OpenStage feeds E3); `design/recovery.md` (R1 pre/post split alignment); `architecture.md` § 5 (cite); `adr/ADR-0.6.0-database-lock-mechanism.md` § 3 (Inv-Lock-1..4 cite); `requirements.md` REQ-042 + REQ-031c; `crates/fathomdb-engine/src/runtime.rs` `EngineRuntime::open` (current 0.5.x order needs PRAGMA-EXCLUSIVE add, R1 insert, identity check, warmup).

---

#### `X1` — Single canonical `(stage, kind, locator, code, doc_anchor)` table

**Agent prompt:**

> Recommend whether `errors.md` should ship a single canonical table joining `OpenStage`, `CorruptionKind`, `CorruptionLocator`, `recovery_hint.code`, and `recovery_hint.doc_anchor`, with engine.md + recovery.md citing rows by stable code rather than re-listing.
>
> Alternative shapes:
>
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

**Status:** RESOLVED 2026-04-30 (confidence 88%). Shape A.

**Resolution:** `design/errors.md` hosts the canonical `(stage, kind, locator, code, doc_anchor)` table as the single materialized join. `design/engine.md` and `design/recovery.md` cite rows by stable `code` (e.g. `E_CORRUPT_WAL_REPLAY`); MUST NOT redeclare columns. May add prose for wiring/action context but variant identity lives once.

Cite-by-code protocol:

- engine.md / recovery.md MUST use literal `code` string when referencing a row.
- CI fails if a cited code is absent from errors.md.
- CI fails if errors.md has a code uncited by both engine.md and recovery.md (orphan codes).
- X6 round-trip CI check becomes a single-table walk.

Rationale: ADR-corruption § 2 already centralizes enum-membership in errors.md; bindings.md § 3 commits `code` as stable dispatch key; architecture.md § 2 assigns `errors` as the cross-cutting module. Shape B/C force tri-file reconciliation — equivalent CI work with three input dialects + drift as default failure mode (memory `feedback_reliability_principles`).

**Cross-doc artifacts:** `design/errors.md` (host canonical table — new section); `design/engine.md` (cite by code); `design/recovery.md` (cite by code; doc_anchors resolve into this file); `design/bindings.md` § 3 (no change; protocol already committed); `adr/ADR-0.6.0-corruption-open-behavior.md` (no amendment).

---

### Tier 2 — Cross-cutting policy

#### `X4` — Foreign-error sanitization recipe ownership + content

**Agent prompt:**

> Recommend the canonical foreign-error sanitization recipe per foreign cause type, owned by `errors.md` and cited by every module-error wrapper in engine.md + recovery.md + op-store.md. ADR-0.6.0-error-taxonomy commits the policy; this HITL produces the per-type recipe.
>
> Foreign causes in scope:
>
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

**Status:** RESOLVED 2026-04-30 (confidence 84%).

**Resolution — per-type recipe table** (lives in `design/errors.md` § Foreign-error sanitization):

| Foreign type                  | Display keeps                                                                                            | Display drops                                                                         | `source` preserved   | Typed attrs                                             |
| ----------------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------- |
| `rusqlite::Error`             | `"sqlite operation failed"` + extended-code class label (`"corruption-class"`, `"busy"`, `"constraint"`) | SQL text, parameter values, table/column names, raw extended-code numeral, file paths | Y (debug-level only) | `sqlite_extended_code: i32`, `sqlite_primary_code: i32` |
| `std::io::Error`              | `"file IO failed"` + `ErrorKind` discriminant name                                                       | Absolute paths, relative paths, filename leaves, OsStr fragments, raw OS error text   | Y                    | `io_kind: ErrorKind`, `os_error: Option<i32>`           |
| `serde_json::Error`           | `"json parse failed"` + `Category` discriminant name                                                     | Byte offset, line, column, payload fragments, expected-token text                     | Y                    | `json_category: Category`                               |
| `tokio::time::error::Elapsed` | `"operation timed out after {timeout:?}"` (config-derived)                                               | n/a                                                                                   | Y                    | `timeout: Duration`, `operation: &'static str`          |
| `napi::Error` / `pyo3::PyErr` | `"binding-layer error"` + foreign type-name (e.g. `"PyErr<ValueError>"`)                                 | Message text, traceback, file paths, JS stack frames, Python frame locals             | Y                    | `binding_error_type: String`                            |

**Implementation contract:**

- `pub(crate) fn sanitize_<source>(e: <source>) -> SanitizedCause` lives next to first wrapper site; module-error variants call helper, never construct inline.
- `SanitizedCause = pub struct { display: Cow<'static, str>, attrs: SanitizedAttrs, source: Box<dyn Error + Send + Sync + 'static> }`.
- `Display` writes `display` only. `Error::source` returns `Some(&*source)`. Tracing emits chain at `debug` level (operator-controlled per ADR).
- Bindings marshal `attrs` per bindings § 3 protocol; `display` is user-visible message.
- CI invariant (test #23): `format!("{}", err)` MUST NOT contain `'/'`, `'.db'`, SQL keywords, numeric byte offsets.

**Cross-doc artifacts:** `design/errors.md` (§ Foreign-error sanitization — primary owner; `SanitizedCause` shape; CI-greppable Display invariant); `design/engine.md` (ENG1 step→variant cite by foreign type); `design/recovery.md` (detection wrappers cite recipe); `design/op-store.md` (JSON validation + IO failures cite recipe); `adr/ADR-0.6.0-error-taxonomy.md` (no amendment; add forward-pointer at draft); `test-plan.md` (lift recipe as test oracle for #23); `crates/fathomdb-engine/src/errors/sanitize.rs` (new module — implementation home).

---

#### `X6` — Round-trip validation contract for `recovery_hint`

**Agent prompt:**

> Recommend a CI/test contract that enforces round-trip integrity of `recovery_hint`:
>
> - Every `recovery_hint.code` produced by the engine has a `doc_anchor` that resolves to an existing heading in `design/recovery.md` (or the operator-facing recovery doc derived from it).
> - Every always-on detection step in recovery.md produces a code that exists in errors.md's enum.
> - Every `CorruptionLocator` variant in errors.md is produced by at least one detection step in recovery.md (no orphan locators).
>
> Alternative enforcement mechanisms:
>
> - Compile-time: `const` table in Rust + `static_assertions`; each ADR/design doc embeds the corresponding generated frontmatter.
> - Test-time: integration test parses errors.md + recovery.md as markdown, enforces the three round-trip invariants.
> - Doc-build-time: docs build script with hard-fail on broken anchors.
> - None: discipline only, reviewed by humans.
>
> Read: ADR-0.6.0-corruption-open-behavior § 2 + § 4, acceptance.md AC-035\*, design/bindings.md § 3.
>
> Return the 7-field structured output. Pro/con per **enforcement mechanism**. Recommendation = chosen mechanism + where its setup lives (CI pipeline, test crate, docs build hook).

**Depends on:** X1 (table shape), R1 (detection set), R3 (verb set).
**Blocks:** none; informs acceptance.md AC update + test-plan.

**Status:** RESOLVED 2026-04-30 (confidence 78%). Test-time integration test + compile-time backstop.

**Resolution:**

Primary gate: `tests/round_trip_recovery_hint.rs` integration test in `fathomdb` (or dedicated `fathomdb-doc-contract` xtest) crate that:

1. Parses X1 canonical table in `docs/0.6.0/design/errors.md` with `pulldown-cmark`.
2. Parses `docs/0.6.0/design/recovery.md` headings + always-on detection list.
3. Asserts three invariants:
   - Every `recovery_hint.code` referenced in engine.md / recovery.md exists in errors.md table.
   - Every always-on detection step in recovery.md produces a code in `CorruptionKind`.
   - Every `recovery_hint.doc_anchor` resolves to existing heading in recovery.md (slug-normalized).
4. Reads `pub(crate) const RECOVERY_HINT_TABLE: &[(CorruptionKind, &str, &str)]` exported from errors module for code introspection.

Compile-time backstop: `static_assertions::const_assert_eq!(RECOVERY_HINT_TABLE.len(), <CorruptionKind variant count>)` — catches locator→code closure without markdown parsing. Lives in `crates/fathomdb/src/errors.rs`.

Setup: integration test runs under existing `cargo test --workspace` per ADR-0.6.0-tier1-ci-platforms; no new docs build pipeline.

REJECTED:

- Doc-build-time only: cannot see invariants #2 + #3.
- Discipline only: violates `feedback_reliability_principles` (no punt for client-burning bugs); ADR § 4 anti-regression demands mechanical detector.

**Cross-doc artifacts:** `test-plan.md` (three test ids: round-trip-anchors, round-trip-detection-codes, round-trip-locator-coverage; map to AC-035b; layer = integration; owning crate = fathomdb); `acceptance.md` AC-035b (cite round-trip integration test as mechanical gate); `design/errors.md` (declare X1 canonical table as parser input contract — column names, ordering, slug-normalization rule); `design/recovery.md` (freeze heading slugs; commit always-on detection set as list with stable per-step ids); `adr/ADR-0.6.0-corruption-open-behavior.md` § 4 (append: anti-regression enforced by `crates/fathomdb/tests/round_trip_recovery_hint.rs`); `crates/fathomdb/src/errors.rs` (export `RECOVERY_HINT_TABLE` const + `static_assertions`); `crates/fathomdb/tests/round_trip_recovery_hint.rs` (new — three `#[test]` fns).

---

### Tier 3 — Remaining errors.md HITL

#### `E3` — `CorruptionKind` / `OpenStage` / `CorruptionLocator` enum membership

**Agent prompt:**

> Enumerate the variants of `CorruptionKind`, `OpenStage`, `CorruptionLocator` (per ADR-0.6.0-corruption-open-behavior § 2 delegation to errors.md). Justify every variant; ADR § 2 anti-regression: "design/errors.md MUST justify every locator variant."
>
> Hard constraints:
>
> - `OpenStage` MUST NOT include `LockAcquisition`.
> - `CorruptionLocator` MUST NOT have free-form `Unspecified`; opaque SQLite errors → `OpaqueSqliteError { sqlite_extended_code: i32 }`.
>
> Cross-references: `OpenStage` aligns 1:1 with the `Engine.open` steps that can detect corruption per ENG1. `CorruptionKind` aligns with the always-on detection set per R1. `CorruptionLocator` aligns with what each detection produces (file offset, page id, table+rowid, vec0-shadow-row, sqlite extended code, etc.).
>
> Return the 7-field structured output. Pro/con per **enum scope** (minimal vs full enumeration). Recommendation = full variant list per enum with one-sentence justification each.

**Depends on:** ENG1, R1, X1.

**Status:** AMENDED 2026-04-30 by R2 (original resolution 84%; amendment confidence 82%).

**Resolution:**

`OpenStage` (4 variants — 1:1 with ENG1 detection-emitting steps; `LockAcquisition` excluded per ADR § 2):

- `WalReplay` — ENG1 step 6 sub-check; rusqlite first-read WAL replay verdict; maps to `E_CORRUPT_WAL_REPLAY`.
- `HeaderProbe` — ENG1 step 6 sub-check; page-1 magic / page-size sanity. Distinct fix path (no recover; export only).
- `SchemaProbe` — ENG1 step 6 sub-check; `PRAGMA user_version` + `_fathomdb_migrations` existence. File structurally readable but not a fathomdb DB.
- `EmbedderIdentity` — ENG1 step 8 (post-migration); reads vector-profile row from migrated schema.

`CorruptionKind` (4 variants — aligns with the 0.6.0 `Engine.open` detection set only):

- `WalReplayFailure` — recovery: `recover --truncate-wal` (lossy only).
- `HeaderMalformed` — recovery: `safe-export` then external rebuild.
- `SchemaInconsistent` — recovery: `recover --rebuild-projections` or escalate.
- `EmbedderIdentityDrift` — row decode-failed; 0.6.0 remains fail-closed here. Intentional identity-swap workflow is deferred to 0.8.0. Distinct from non-corruption `EmbedderIdentityMismatchError`.

`CorruptionLocator` (6 variants — every variant justified per ADR § 2 anti-regression):

- `FileOffset { offset: u64 }` — produced by HeaderProbe + raw-page WAL diagnosis; needed for `safe-export` triage.
- `PageId { page: u32 }` — produced by WalReplay frame→page diagnosis; remains justified even after the R2 amendment removed opt-in `integrity_check` from the open path.
- `TableRow { table: &'static str, rowid: i64 }` — produced by SchemaProbe orphan-row + EmbedderIdentity row decode failure; only way to point at a logical row when page intact.
- `Vec0ShadowRow { partition: &'static str, rowid: i64 }` — produced by post-migration vec0 shadow-table consistency (cheap-only today; may promote); recovery is `recover --rebuild-vec0`. Distinct from `TableRow` because vec0 not user-named.
- `MigrationStep { from: u32, to: u32 }` — locator IS migration edge, not row.
- `OpaqueSqliteError { sqlite_extended_code: i32 }` — mandatory per ADR § 2; replaces forbidden `Unspecified`. SanitizedCause (X4) carries detail.

**R2 amendment note:** `IntegrityCheck` / `IntegrityCheckFailed` were removed from the
0.6.0 `Engine.open` enum surface because R2 concluded there is no justified
open-time opt-in knob surface in 0.6.0. `doctor check-integrity --full`
RETAINS dedicated page-damage finding code `E_CORRUPT_INTEGRITY_CHECK`, and
that code belongs to the doctor report surface rather than the `Engine.open`
`OpenStage` / `CorruptionKind` enum surface.

**Cross-doc artifacts:** `design/errors.md` (canonical X1 table — primary owner; SanitizedCause for OpaqueSqliteError); `design/engine.md` (ENG1 step→OpenStage cite by name); `design/recovery.md` (CorruptionKind → recovery_hint.code → R3 verb mapping; doc anchors per kind); `crates/fathomdb/src/errors.rs` (`RECOVERY_HINT_TABLE` const + `static_assertions::const_assert_eq!` X6 backstop); `crates/fathomdb/tests/round_trip_recovery_hint.rs` (X6 test asserts every locator produced by ≥1 stage; every kind has recovery.md heading); `adr/ADR-0.6.0-corruption-open-behavior.md` § 2 anti-regression now bound to variant list.

**Judgement-call flag:** `Vec0ShadowRow` included though shadow-table check is
cheap-only (not always-on). Defended because produced by `doctor
check-integrity` today. The key distinction after the R2 amendment is that
cheap-only doctor findings may justify locators or report codes without
therefore justifying new `Engine.open` corruption kinds.

---

#### `E5` — Migration error home

**Agent prompt:**

> Recommend whether migration failure surfaces as `EngineOpenError::Migration { step, cause }` or `EngineError::Storage(StorageError::Migration {...})`. This is partially decided by E2 but enumerated here for thoroughness.
>
> Pro/con per home. Interaction with REQ-042 (per-step structured progress events): does the variant carry the step list up-to-failure or just the failing step?
>
> Return the 7-field structured output.

**Depends on:** E2.

**Status:** RESOLVED by E2 (2026-04-30). `EngineOpenError::Migration { step: MigrationStep, cause: SanitizedCause }`. Per REQ-042: variant carries the failing step only; per-step progress events emitted via tracing during loop, accumulated into `OpenReport.migrations: Vec<MigrationStepReport>` (per ENG6) regardless of failure. SanitizedCause via X4 recipe. No further HITL needed.

---

#### `E6` — Embedder warmup failure home

**Agent prompt:**

> Recommend whether eager-warmup failure (Invariant D) at `Engine.open` surfaces as `EngineOpenError::EmbedderWarmupFailed` or `EngineError::Embedder(EmbedderError::Warmup{..})`.
>
> Constraint: Invariant D timeout default 30s. Failure may be caller-controllable (user-supplied embedder bug) vs default-embedder-internal. Does the variant carry that distinction?
>
> Return the 7-field structured output.

**Depends on:** E2.

**Status:** RESOLVED 2026-04-30 by HITL (confidence 78%). Runtime-rooted.

**Resolution:** Add `EmbedderError::Warmup { cause: SanitizedCause }` to embedder module enum. Surfaces at `Engine.open` step 9 via existing `EngineError::Embedder(#[from] EmbedderError)` composition, then `EngineOpenError::Engine(#[from] EngineError)`. **E2 unchanged** — `EngineOpenError` keeps 5 variants.

Caller-controllable distinction: NOT in variant shape. The sanitized cause chain preserves origin (default-embedder-internal vs user-supplied trait impl panic-converted-to-error) for engine-internal logging only; bindings see one variant.

**Cross-doc artifacts:** `design/errors.md` (add `EmbedderError::Warmup` row; cite by code `E_EMBEDDER_WARMUP`); `design/engine.md` (ENG1 step 9 binding); `adr/ADR-0.6.0-error-taxonomy.md` (no amendment — pattern is the locked one); `adr/ADR-0.6.0-async-surface.md` Invariant D (cite `EmbedderError::Warmup` as the surface).

**Sources:**

- [Luca Palmieri — Error Handling in Rust](https://www.lpalmieri.com/posts/error-handling-rust/) (per-module enum + compose)
- [thiserror+anyhow design guide](https://oneuptime.com/blog/post/2026-01-25-error-types-thiserror-anyhow-rust/view) (`#[from]` composition)
- [nrc error-docs — Error type design](https://nrc.github.io/error-docs/error-design/error-type-design.html) (design by how errors arise, not when first observed)

---

#### `E7` — `DatabaseLocked` variant shape

**Agent prompt:**

> Confirm shape of `EngineOpenError::Locked` per ADR-0.6.0-database-lock-mechanism. Specifically: is `holder_pid` required (PID written to sidecar by lock acquirer) or optional (sidecar may be stale / unreadable)? Should the variant also carry `holder_started_at: SystemTime` for staleness detection?
>
> Read: ADR-0.6.0-database-lock-mechanism, design/bindings.md § 7. Check `crates/fathomdb-engine/src/database_lock.rs` via Explore for current sidecar payload schema.
>
> Return the 7-field structured output.

**Depends on:** none.

**Status:** RESOLVED 2026-04-30 (confidence 95%). Confirms E2 pre-commit; no override.

**Resolution:**

```rust
EngineOpenError::Locked {
    holder_pid: Option<u32>,
}
```

Single field. Mirrors `database_lock.rs::read_pid` signature; tolerates documented best-effort race (empty file mid-write, Windows unreadable under exclusive lock, recycled PID, malformed contents) without lying to operator.

**Rejected:**

- `holder_started_at`: ADR § 4 delegates staleness correctness to kernel-held flock; kernel releases on crash, so operators never observe stale lock requiring PID-time disambiguation.
- `hostname` / `lock_version`: NFS/cross-host out of scope (ADR § 4); sidecar payload non-parsed-for-correctness, so version envelope dead weight. Adding either = speculative knob (bindings § 14 forbidden).
- REQUIRED `holder_pid: u32`: violates reality — write to lock file non-atomic; would force synthetic 0 or panic; contradicts ADR § 1 "best-effort diagnostic".

**Code-level reference:** `crates/fathomdb-engine/src/database_lock.rs` `try_lock` + `read_pid` already returns `Option<u32>`; tests `lock_file_contains_pid` + `lock_error_includes_holder_pid` (Unix-gated PID assertions) confirm cross-platform `Option` necessity. Test #9 (`holder_pid` matches via `/proc`) aligns with existing `lock_error_includes_holder_pid` assertion.

**Cross-doc artifacts:** `adr/ADR-0.6.0-database-lock-mechanism.md` (no amendment — § 1, § 3 Inv-Lock-1/4, § 4 failure-mode table, Consequences already commit `Option<u32>`); `design/bindings.md` § 7 (`Optional[int]` / `number | null` marshalling); `design/errors.md` (variant→class mapping matrix — downstream consumer); `crates/fathomdb-engine/src/database_lock.rs` (already implements; no change).

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

**Annotation post-ENG1 adoption (2026-04-30):** R2 must address pre/post-migration split. Opt-in checks (`quick_check`, `integrity_check`, `round-trip`) require stable schema → fire post-migration only. Knob name proposals should reflect this: `FATHOMDB_OPEN_INTEGRITY_CHECK=quick|full|off` semantically applies post-step-7 (post-migration) only. Pre-migration tier is FROZEN at the four R1 always-on checks; no operator-tunable surface exists for them (composition is frozen by ADR § 4 anti-regression).

**Status:** RESOLVED 2026-04-30 by amendment (confidence 86%).

**Original resolution (now SUPERSEDED):** Ship A + C; defer B (conf 78%).

**Why amended:** over-design-audit skill (2026-04-30) ran consumer enumeration including **AC fixture coverage** as a profile the original prompt missed. Found `acceptance.md` AC-035a (REQ-031d, T-035a) requires "for each documented corruption-injection fixture (one per `CorruptionKind` variant enumerated in `design/errors.md`), `Engine.open` returns `Err(EngineOpenError::Corruption(_))`". E3 enumerated `IntegrityCheckFailed` as a `CorruptionKind` variant. R1 places `quick_check`/`integrity_check` in opt-in tier (default off). With R2 deferring SDK config B, the AC-035a fixture for `IntegrityCheckFailed` cannot trigger detection cleanly:

- **Strategy A (env)** — `std::env::set_var` mid-test leaks across cargo test parallel execution; needs serial-test crate or test-mutex infra.
- **Strategy C (CLI doctor)** — runs against closed DB; cannot satisfy "`Engine.open` returns `Err(...)`" assertion shape.
- **Strategy B (SDK config)** — only path that cleanly satisfies AC-035a per-test.

**Re-evaluation options:**

(i) **Ship B in 0.6.0.** `EngineConfig.integrity_check_at_open: enum { Off, Quick, Full }` defaulting to `Off`; § 6 fan-out across Python kwarg + TS option + CLI flag. AC-035a fixture sets `Quick` or `Full` per-test. Restores symmetry with env, but the only concrete consumer is a test fixture.

(ii) **Drop `IntegrityCheckFailed` from `CorruptionKind` for 0.6.0.** Defer the open-path variant + the AC-035a fixture for it. Opt-in tier becomes operator-only (`doctor check-integrity`); `Engine.open` never produces `IntegrityCheckFailed`. E3 open-path enum membership shrinks by one variant.

(iii) **Strategy A + serial-test infra.** Add `serial_test` crate dependency; gate AC-035a `IntegrityCheckFailed` fixture with `#[serial]`. Env knob remains the only surface; SDK config still deferred. Test infrastructure cost exists solely to preserve a non-default open-path variant.

**Amended resolution: choose (ii).**

**Resolution:**

- **No open-time opt-in knob ships in 0.6.0.** Neither env A nor SDK config B lands.
- **Opt-in integrity work remains on the already-accepted operator surface only:** `fathomdb doctor check-integrity [--quick] [--full] [--round-trip]` per R3/R9.
- **`Engine.open` therefore never produces an integrity-check-specific corruption stage/kind in 0.6.0.** AC-035a covers every kind the real 0.6.0 open path can produce, and no serial-test infrastructure is required.

**Critique of the rejected surfaces:**

- **A only (env):** avoids SDK fan-out but creates hostile test ergonomics (`std::env::set_var` leakage across parallel tests) and still buys no operator capability beyond R3 doctor.
- **B only (SDK config):** gives clean per-engine tests, but the only concrete 0.6.0 consumer is that test fixture. Shipping a three-binding config field to satisfy one acceptance fixture is over-design by the project’s own standard.
- **A + serial infra:** explicitly spends dev-dependency and test-complexity budget to preserve a non-default `Engine.open` corruption kind that operators do not need at open time.
- **C already covers the real operator use case:** pre-open integrity assurance is `doctor check-integrity`, followed by `Engine.open` on the always-on tier.

**Atomicity rebuttal (accepted):** the supposed "doctor then open" gap does not
justify an `Engine.open` opt-in surface in 0.6.0. R8 already locks `recover`
and constrains `doctor` / `open` semantics tightly enough that a hypothetical
atomic verify-then-open guarantee would not protect against external file
mutation anyway.

**Important scope distinction:** this amendment removes the open-path knob and
the open-path `IntegrityCheckFailed` variant. `doctor check-integrity --full`
still emits structured page-damage finding code `E_CORRUPT_INTEGRITY_CHECK`;
that code is doctor-report surface rather than `Engine.open`
corruption-enum surface.

**Cross-doc artifacts:** `design/recovery.md` (primary owner — remove env-knob
surface; keep doctor-only opt-in tier and retain `E_CORRUPT_INTEGRITY_CHECK`
for `--full` findings); `design/errors.md` (drop `OpenStage::IntegrityCheck` /
`CorruptionKind::IntegrityCheckFailed` from the open-path enum table, but keep
doctor-report code row / anchor for `E_CORRUPT_INTEGRITY_CHECK`); `acceptance.md`
(remove AC-035a fixture expectation for the dropped open-path kind; add or keep
doctor-report coverage via AC-043\* or a follow-on AC in `acceptance.md`); `adr/ADR-0.6.0-corruption-open-behavior.md`
§ 4 (clarify that 0.6.0 opt-in integrity work is doctor-only, not `Engine.open`
surface); `design/engine.md` (remove env capture mention if drafted);
`design/bindings.md` (no new knob fan-out); `interfaces/cli.md` (no change
beyond existing R3 surface); `X1` / `R9` consumers (model
`E_CORRUPT_INTEGRITY_CHECK` as doctor-report code, not `Engine.open`
corruption kind).

---

#### `R4` — `--accept-data-loss` scope

**Agent prompt:**

> For each verb in the R3 verb set, decide bit-preserving y/n + whether `--accept-data-loss` is required. Constraint per ADR-0.6.0-corruption-open-behavior § 3: "any path not bit-preserving" requires the flag.
>
> Return the 7-field structured output. Output = annotated verb table.

**Depends on:** R3.

**Status:** RESOLVED 2026-04-30 (confidence 86%). Strategy A — root-level on `recover` only.

**Resolution:** `--accept-data-loss` is root-level mandatory flag on `fathomdb recover` and on no other verb. No per-sub-flag opt-in flags. Subcommand parser rejects `recover` without flag BEFORE any DB IO.

`--restore-logical-id` adjudicated **LOSSY**: restoring a previously-purged id rewrites canonical state (intervening writes against now-reused id are not rolled back). "Reversibility" argument is superficial — bit-preserving means on-disk bytes unchanged, not that future operator command can undo. Stays under `recover`.

**Per-verb table (ratifies R3 with bit-preserving column):**

| Verb / sub-flag                                                                                                                                                     | Bit-preserving | Requires `--accept-data-loss` |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ----------------------------- |
| `recover` (umbrella) + all sub-flags (`--truncate-wal`, `--rebuild-vec0`, `--rebuild-projections`, `--excise-source`, `--purge-logical-id`, `--restore-logical-id`) | N              | Y mandatory (root-level only) |
| `doctor check-integrity` / `safe-export` / `verify-embedder` / `trace` / `dump-*`                                                                                   | Y              | N                             |

**Over-design (Strategy B per-sub-flag) FALSIFIED:** Workflow enumeration showed no operator workflow mixes lossy + bit-preserving in one `recover` call. Two-root partition itself enforces consent boundary. Strategy C (hybrid) similarly rejected — no objective threshold for "high enough blast-radius" inside `recover`.

**Cross-doc artifacts:** `design/recovery.md` (primary — flag-scope section); `interfaces/cli.md` (clap/argparse: flag declared on `recover` parser only; missing-flag error class = `recover-* exit 70` before DB IO); `requirements.md` REQ-036 (no edit; spelling already matches); `acceptance.md` AC-035d (extend: "invoking `recover` without flag exits non-zero with NO DB IO and NO lock acquisition"; sibling AC: every `doctor` verb rejects `--accept-data-loss` as unknown flag); `adr/ADR-0.6.0-corruption-open-behavior.md` § 3 (no amendment; R4 ratifies); `crates/fathomdb-cli/` (greenfield).

Test plan additions: (a) `recover` without flag → exit 70, zero DB IO; (b) every `doctor` verb rejects `--accept-data-loss`; (c) every `recover` sub-flag reachable only with root flag set; (d) `--restore-logical-id` test fixture asserts lossy classification (writes between purge + restore not preserved).

---

#### `R5` — `safe_export` manifest format

**Agent prompt:**

> Recommend the manifest format for `doctor safe-export`. Constraints: REQ-039 (SHA-256 per file); REQ-040 latency target — cite P-NNN from acceptance.md, do NOT duplicate. Format candidates: NDJSON (one record per file), single JSON object with file array, plain `sha256sum`-compatible text.
>
> Return the 7-field structured output.

**Depends on:** R3.

**Status:** RESOLVED 2026-04-30 (confidence 88%). Plain `sha256sum`-compatible text.

**Resolution:**

Format: `sha256sum`-compatible text (`<64-hex-lowercase>  <relpath>\n`). Two-space separator. POSIX forward-slash relpaths relative to manifest dir. ASCII; LF terminator; no BOM; trailing newline. Sorted byte-wise by relpath → deterministic byte-identical output across runs of same DB.

- Default path: `<out>.sha256` (sibling of export). Override via `--manifest <path>`.
- Writer: stream each file, finalize hash, append line, fsync at end. No in-memory accumulation.
- Verifier (in `--help`): `cd <out-dir> && sha256sum -c <out>.sha256` (or `shasum -a 256 -c` on macOS).

**Over-design REJECTED:**

- `manifest_version: u32` — single consumer pair (operator + fathomdb verifier); format decades-stable; YAGNI.
- JSON envelope (`{"version":1,"files":[...]}`) — breaks `sha256sum -c` for zero benefit.
- Per-file size/mtime/tool-version metadata — not required by REQ-035 / AC-039a/b; if ever needed, add sibling `<out>.manifest.json` without disturbing hash file.
- Detached signature / GPG — deferred-doc territory (`dev/design-note-encryption-at-rest-in-motion.md`).

**Prompt errata flagged by agent:** Original R5 prompt cited "REQ-039 SHA-256 per file" + "REQ-040 latency target with P-NNN" — actual SHA-256 manifest requirement is **REQ-035**; REQ-039 is `check-integrity` aggregator and REQ-040 is projection-rebuild; **no P-NNN latency parameter exists for safe-export**. Resolution binds R5 to REQ-035 / AC-039a / AC-039b.

**Cross-doc artifacts:** `requirements.md` REQ-035 (append manifest format spec); `acceptance.md` AC-039a/b (clarify "manifest" = `sha256sum`-compatible text; verifier = `sha256sum -c`); `interfaces/cli.md` (`doctor safe-export` synopsis + `--help` example); `design/recovery.md` (verb row: format + streaming-hash writer + deterministic sort); `adr/ADR-0.6.0-cli-scope.md` (no change). No new ADR needed.

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

**Status:** RESOLVED 2026-04-30 (confidence 86%). Strategy β — per-verb tiered.

**Resolution — per-verb lock matrix:**

| Verb                                        | Sidecar flock     | Writer EXCLUSIVE PRAGMA | Reader conn                                                                             | Coexists with live operator                    |
| ------------------------------------------- | ----------------- | ----------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------------- |
| `fathomdb recover --accept-data-loss [...]` | YES (ENG1 step 2) | YES (ENG1 step 5)       | YES per ENG1                                                                            | NO — fails fast `Locked{holder_pid}` → exit 71 |
| `doctor check-integrity`                    | NO                | NO                      | YES NORMAL mode                                                                         | YES (WAL multi-reader)                         |
| `doctor safe-export <out>`                  | NO                | NO                      | YES NORMAL inside long-running `BEGIN IMMEDIATE` (WAL snapshot isolation; no torn read) | YES                                            |
| `doctor verify-embedder`                    | NO                | NO                      | YES NORMAL                                                                              | YES                                            |
| `doctor trace --source-ref <id>`            | NO                | NO                      | YES NORMAL                                                                              | YES                                            |
| `doctor dump-{schema,row-counts,profile}`   | NO                | NO                      | YES NORMAL                                                                              | YES                                            |

`recover` reuses `EngineRuntime::open` (ENG1) verbatim — recovery IS `Engine.open` followed by typed mutations, not parallel open path. Read-only `doctor` constructs stripped-down `ReaderHandle::open_read_only` (NORMAL `locking_mode`, no writer conn, no sidecar acquisition; no embedder warmup unless verb needs it — `verify-embedder` does).

R3 has zero mutating doctor verbs in 0.6.0; "mutating doctor" matrix row intentionally absent. Future lossy verb requires R3 re-open.

**Over-design REJECTED — Strategy γ ("read-lock sidecar"):** SQLite WAL already provides shared/exclusive coordination at conn level; `BEGIN IMMEDIATE` takes snapshot surviving concurrent writer commits; no failure mode where read-only `doctor` corrupts writer view. Adding second sidecar would invent lease semantics, fork acquisition recipe, protect against zero documented failures. DELETE before ships.

**Cross-doc artifacts:** `design/recovery.md` (primary — § Lock semantics with per-verb matrix); `design/engine.md` (ENG1 reused by `recover`; explicit non-acquisition list for read-only `doctor`); `interfaces/cli.md` (per-verb lock posture + exit codes; `Locked` → exit 71 only on `recover`); `adr/ADR-0.6.0-database-lock-mechanism.md` (no amendment — CLI is consumer of Inv-Lock-1..5, not new contract); `adr/ADR-0.6.0-cli-scope.md` (no amendment); `design/bindings.md` § 7 (no amendment — read-only `doctor` non-acquisition not a binding violation; doesn't call `Engine.open`); `crates/fathomdb-cli/src/{recover.rs,doctor.rs}` (greenfield); `acceptance.md` AC-035d (broaden: read-only `doctor` does NOT acquire sidecar; `recover` does).

---

#### `R9` — `check-integrity` aggregator output schema

**Agent prompt:**

> Recommend output schema for `doctor check-integrity`. REQ-024 mandates JSON output (machine-readable). Decide: schema shape (versioned), human pretty-print fallback (yes/no), per-check pass/fail granularity (boolean vs structured detail), exit-code semantics on partial failure.
>
> Return the 7-field structured output.

**Depends on:** R1 (which checks run).

**Status:** RESOLVED 2026-04-30 (confidence 80%). Single JSON object, three sections (AC-043a), per-check structured records keyed by stable `code`, single global exit code, optional human pretty-print.

**Resolution:**

1. **ID** — R9
2. **Title** — `doctor check-integrity` aggregator output schema
3. **Description** — `doctor check-integrity` emits a single JSON object on stdout with three top-level keys (`physical`, `logical`, `semantic`) per AC-043a, each holding either `clean: true` or a `findings: [...]` list (AC-043b). Each finding is a structured record carrying the X1 stable `code`, locator, and `doc_anchor` so CI scripts dispatch on `code` and operators read pretty-printed output via `--pretty`. A single process-level exit code (0 clean / 65 issues-found / 70 unrecoverable / 71 lock-held per R3) covers partial failure; per-check exit codes are NOT used.
4. **Pro/Con per shape**
   - **Single JSON object (recommended).** Pro: trivial `jq` access, atomic write to stdout, matches AC-043a/b key-equality assertion, mirrors the `OpenReport` shape (ENG6) so operators learn one mental model. Con: large reports (`--full` integrity_check on big DB) buffer in memory before emit; not friendly to live progress. Mitigation: opt-in `--ndjson` is a future R-knob, NOT 0.6.0.
   - **NDJSON streaming (per-check record).** Pro: progress observable; bounded memory. Con: AC-043a asserts JSON object key equality — NDJSON breaks the assertion verbatim; CI scripts must reassemble; operators piping to `jq` need `--slurp`; doubles the surface (consumers handle two encodings). Falsified against the actual consumer set (operator + CI assertions); progress is not a 0.6.0 acceptance bar.
   - **Hybrid object-with-events.** Pro: live progress + final summary in one stream. Con: highest surface area; needs schema discriminator field; no acceptance criterion demands progress; classic over-design (memory `feedback_reliability_principles` — net-negative LoC posture). Rejected.
5. **Interactions**
   - **R1 (which checks fire).** `--quick`/`--full`/`--round-trip` opt-in flags gate population of corresponding finding lanes; cheap-only tier (vec0 metadata + row-count parity, canonical referential sanity, profile parity) always populates. The set of checks attempted is reflected in a `checks_run: ["wal_replay", "header_probe", "schema_probe", "embedder_identity", "vec0_parity", "canonical_orphan_scan", "profile_parity", ...]` array so silently-skipped checks cannot masquerade as `clean: true`.
   - **R3 (CLI verb).** Verb is `fathomdb doctor check-integrity [--quick] [--full] [--round-trip] [--pretty]`. `--json` is the implicit default per R3 ("`--json` mandatory on every verb"); `--pretty` renders the same object via a deterministic human formatter (no separate schema). Exit class `doctor-check-*` (0/65/70) per R3.
   - **X1 (cite-by-code).** Every finding record carries the X1 `code` field as primary key (e.g. `E_CORRUPT_VEC0_PARITY`). Schema MUST include `code` + `doc_anchor` (relative path); CI scripts dispatch on `code` and never parse human prose. Round-trip backstop (X6) extended: every `code` emitted by check-integrity MUST resolve to a row in the X1 canonical table.
6. **Recommendation + concrete schema**

   No `schema_version` field. Justification: `--json` is mandatory + frozen by REQ-024 + AC-043a; field additions are non-breaking by convention (matches X1 cite-by-code stability); a version field invites consumers to branch on it, expanding surface for no current need. **Over-design flag if added.**

   No `manifest`/`metadata` envelope. Top-level is the report. Add `tool_version` (single string from `CARGO_PKG_VERSION`) and `target` (canonicalized DB path) at the root — both already needed for support triage; nothing speculative.

   ```json
   {
     "tool_version": "0.6.0",
     "target": "/var/lib/fathomdb/main.sqlite",
     "checks_run": ["wal_replay", "header_probe", "schema_probe",
                    "embedder_identity", "vec0_parity",
                    "canonical_orphan_scan", "profile_parity"],
     "physical": {
       "findings": [
         {
           "code": "E_CORRUPT_VEC0_PARITY",
           "stage": "Vec0Parity",
           "locator": { "Vec0ShadowRow": { "partition": "default", "rowid": 4711 } },
           "doc_anchor": "recovery.md#vec0-parity",
           "detail": "shadow row count 38122 != canonical 38123"
         }
       ]
     },
     "logical": { "clean": true },
     "semantic": { "clean": true },
     "summary": { "total_findings": 1, "exit_code": 65 }
   }
   ```

   Per-check record fields (frozen): `code` (X1), `stage` (X1 OpenStage when applicable, else CheckStage), `locator` (X1 enum), `doc_anchor` (X1), `detail` (free-form English; informational only — CI MUST NOT parse). Adding fields is non-breaking; removing/repurposing fields requires HITL re-resolution.

   Exit-code semantics: process emits a single exit code from the R3 doctor-check class. `total_findings == 0` → 0; ≥1 finding → 65; lock-held at open → 71; opaque/unrecoverable → 70. **No per-check exit code.** Justification: operators script `if ! check-integrity; then ...`; CI scripts inspect `summary.total_findings` or per-section `findings` length. Two dispatch surfaces is one too many.

   Pretty-print fallback: YES. `--pretty` renders the same JSON object via a deterministic human formatter (section heading + bullet list of `code: detail (doc_anchor)`). NOT a separate schema; NOT machine-readable. Default remains JSON to satisfy REQ-024.

   NDJSON streaming: REJECTED for 0.6.0. Acceptance criteria do not demand progress; consumers (operator + CI) are happy with a single object; introducing it later is non-breaking (new verb flag, e.g. `--ndjson`, when an actual progress consumer surfaces).

   **Over-design watch (called out and resolved):**
   - `schema_version` — REJECTED (speculative; no consumer; field additions already non-breaking).
   - `manifest`/`metadata` envelope — REJECTED (replaced by two flat root fields).
   - Per-check exit codes — REJECTED (one exit class is sufficient for both consumers).
   - NDJSON event mode — DEFERRED (no current consumer; non-breaking to add later).

7. **Confidence** — 80%. Calibrated against: AC-043a/b already pin top-level shape; R1 + R3 + X1 are resolved upstream so the per-record key set is fully determined; the only judgement call is rejecting `schema_version` (downside: future format break would require a verb-flag escape; mitigation: field-additions + cite-by-code keep schema additive). 20% reserved for the case where an operator review surfaces a real NDJSON consumer (bulk-DB-fleet check-integrity loop) before lock; would add `--ndjson` flag but not change the default object schema.

**Cross-doc artifacts:** `design/recovery.md` (primary owner — § `check-integrity` output schema; freeze top-level + per-record key sets); `interfaces/cli.md` (concrete `--pretty` flag spelling + exit-code numbers from R3 doctor-check class); `acceptance.md` AC-043a + AC-043b (no edit — schema satisfies both; consider adding AC-043c asserting per-finding `code` + `doc_anchor` presence to bind cite-by-code at output layer); `design/errors.md` (X1 canonical table is normative input — every emitted `code` MUST appear there; X6 round-trip extended to cover check-integrity emit path); `design/engine.md` (no edit — open-path errors surface via `EngineOpenError`, not via this report); `crates/fathomdb-cli/src/doctor/check_integrity.rs` (NEW — emits this schema; `--pretty` renderer co-located); `crates/fathomdb/tests/check_integrity_schema.rs` (NEW — golden JSON + AC-043a/b key-equality + per-finding code-resolves-in-X1 assertions).

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

**Required behavior post-ENG1 adoption (2026-04-30):**

`Engine.close` shutdown order MUST be strict reverse of ENG1 steps 10→2:

1. Drop scheduler runtime + vector actor handle.
2. Drain writer thread inbox; close inbox channel.
3. Join writer thread (bounded timeout per AC-022a).
4. Close reader pool connections.
5. Close writer SQLite connection (releases SQLite native EXCLUSIVE lock).
6. Drop sidecar lock fd (releases flock, then `unlink {path}.lock` — Inv-Lock-5).

**Reverse-ENG3 teardown also runs on post-step-10 open failure** (Inv-Lock-4): if any check after writer thread spawn fails, run steps 1–6 above BEFORE releasing lock and surfacing `EngineOpenError`. Engine handle MUST NOT be returned. ADR-corruption-open-behavior § 5 binds this even though writer thread was already alive.

Code-level reference: existing `crates/fathomdb-engine/src/runtime.rs` already encodes drop-order docstring; verify it matches reverse of corrected ENG1 11-step order, not 0.5.x order.

**Status:** RESOLVED 2026-04-30 by HITL (confidence 80%). Explicit fallible `close()` + best-effort `Drop`.

**Resolution:**

API shape:

```rust
impl Engine {
    /// Primary shutdown path. Runs reverse-ENG3 teardown with bounded drain.
    pub fn close(self) -> Result<CloseReport, EngineError>;
}

impl Drop for Engine {
    /// Best-effort fallback. Each step wrapped in catch_unwind; logs warn on
    /// failure; never panics; never returns. Runs same step sequence as close().
    fn drop(&mut self);
}

#[derive(Debug, Clone)]
pub struct CloseReport {
    pub scheduler_drain_duration: Duration,
    pub writer_drain_duration: Duration,
    pub writer_thread_joined: bool,    // false if drain timed out → detached
    pub reader_pool_close_duration: Duration,
    pub sqlite_close_duration: Duration,
    pub lock_release_duration: Duration,
    pub total_close_duration: Duration,
}
```

Drain timeout default: **30s** (mirrors Invariant D embedder timeout for symmetry).

Timeout behavior: writer thread is **detached** (Rust has no thread-kill primitive); `EngineError::CloseTimeout { stage: CloseStage, elapsed: Duration }` returned. Lock fd **STILL released** (operator preference: stale flock release > leak — process still holds DB writer guarantee until detached thread exits).

Post-step-10 open-failure teardown: runs `best_effort_close()` synchronously **before** lock release; original `EngineOpenError` surfaced (close error logged at `warn`, not returned).

Idempotency: `close()` consumes `self` (compile-time exclusion of double-call). `Drop` always runs; `Drop` checks an internal `closed: AtomicBool` flag set by `close()` and skips if true.

Panic safety: every step inside `Drop::drop` MUST be wrapped in `catch_unwind` per [std::ops::Drop docs](https://doc.rust-lang.org/std/ops/trait.Drop.html) — double-panic aborts process. CI invariant test: assert `Drop` runs same step sequence as `close()` (test #5 in test plan extension).

**Cross-doc citations to add at draft:**

- `design/engine.md` § Engine.close shutdown protocol — primary owner; cite [Sabrina Jewson — async-drop analysis](https://sabrinajewson.org/blog/async-drop), [Rust Book ch21 graceful shutdown](https://doc.rust-lang.org/book/ch21-03-graceful-shutdown-and-cleanup.html) for explicit-fallible-plus-Drop pattern; cite [std::ops::Drop](https://doc.rust-lang.org/std/ops/trait.Drop.html) for panic-in-drop rule; cite [Rust async-fundamentals roadmap](https://rust-lang.github.io/async-fundamentals-initiative/roadmap/async_drop.html) explaining why AsyncDrop is not used.
- `adr/ADR-0.6.0-scheduler-shape.md` § Engine.close shutdown protocol — append: "drain timeout 30s; on timeout, writer thread detached, lock fd released, `CloseTimeout{stage, elapsed}` surfaced. Drop is best-effort fallback wrapping each step in catch_unwind."
- `acceptance.md` AC-022a — extend Measurement: bounded drain timeout = 30s; `EngineError::CloseTimeout` test fires when writer thread holds longer.
- `requirements.md` REQ-020a — cross-cite `CloseReport` shape.
- `design/bindings.md` § 7 — `CloseReport` and `CloseTimeout` surface across bindings.
- `design/errors.md` — add `EngineError::CloseTimeout { stage, elapsed }` variant + `CloseStage` enum (matches reverse-ENG1 step names).
- `crates/fathomdb-engine/src/runtime.rs` — verify Drop docstring + impl match this protocol.

**Tracking issue:** [coreyt/fathomdb#60](https://github.com/coreyt/fathomdb/issues/60) — migrate `Drop for Engine` to `AsyncDrop` once stabilized.

**Test plan additions:**

- Test #27 — `Drop` runs same sequence as `close()` (assert via tracing span replay).
- Test #28 — drain timeout fires → `CloseTimeout`, lock released, writer thread detached (still alive).
- Test #29 — panic inside step 1-3 of Drop → `catch_unwind` swallows, subsequent steps still run, lock released.
- Test #30 — `close()` consumed semantics enforced at compile time (negative-compilation test).

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

**Required behavior post-ENG1 adoption (2026-04-30):**

`OpenReport` MUST carry per-stage timings reflecting R1 pre/post-migration split (not single "detection" duration):

```rust
#[derive(Debug, Clone)]
pub struct OpenReport {
    pub lock_acquired_at: SystemTime,           // step 2
    pub pragma_setup_duration: Duration,         // steps 4-5 combined
    pub detection_pre_migration: DetectionReport,// step 6 — WAL replay verdict, header, schema probe
    pub migrations: Vec<MigrationStepReport>,    // step 7 — per REQ-042, includes failed step
    pub detection_post_migration: DetectionReport,// step 8 — embedder identity probe
    pub embedder_warmup_duration: Duration,      // step 9
    pub thread_spawn_duration: Duration,         // step 10
    pub total_open_duration: Duration,
}

#[derive(Debug, Clone)]
pub struct DetectionReport {
    pub checks_run: Vec<DetectionCheck>,         // each with name, duration, verdict
    pub total_duration: Duration,
}
```

`embedder.identity()` is invoked at step 8 BEFORE warmup at step 9 (recommended ENG1 ordering). Bindings: Python flattens to dict; TS marshals struct. Field names exposed verbatim per bindings.md § 6 config-knob symmetry analog (report-field symmetry).

Code-level reference: emit one structured event per step (REQ-042); accumulate into `OpenReport` for return. Tracing span hierarchy: `engine.open` parent → child spans for each step keyed on `OpenStage` discriminant.

**Status:** RESOLVED 2026-04-30 by HITL (confidence 88%). Idiomatic-only marshalling.

**Resolution:**

`Duration` fields marshal as Python `timedelta` (μs precision) / TS `number` (ms, float allowed). `SystemTime` fields marshal as Python `datetime` (UTC) / TS `Date`. snake_case in Python; camelCase in TS. `OpenReport` and `CloseReport` are `#[non_exhaustive]` to permit field add without semver break.

**No sibling `_ns` BigInt fields.** Survey of `docs/0.6.0/` SLI ADRs confirmed:

- ADR-text-query-latency-gates (p50 ≤ 20ms), ADR-retrieval-latency-gates (p50 ≤ 50ms), ADR-write-throughput-sli, ADR-projection-freshness-sli — ALL gate at ms scale; SLI samples need sub-ms but NOT via OpenReport/CloseReport.
- OpenReport field ranges: pragma 1-50ms, detection 1-100ms, migrations 10ms-30min, warmup 100ms-30s, total 200ms-60s. ms idiomatic sufficient.
- CloseReport field ranges: same. ms idiomatic sufficient.
- AC-035 P-RECOV-N=10 budget = 2s; per-trial μs detail not consumed by budget gate.

Future SLI Report types (write/query/retrieval/projection metrics) — separate HITL not yet scoped — MUST use lossless marshalling (BigInt-ns or hybrid). NOT OpenReport/CloseReport's problem.

**Cross-doc artifacts:** `design/engine.md` (`OpenReport` shape — primary owner; cite [PyO3 std time conversions](https://pyo3.rs/main/conversions/tables.html), [napi-rs values](https://napi.rs/docs/concepts/values)); `design/bindings.md` § 6 (report-field symmetry — names exposed verbatim across bindings); `design/errors.md` (`CloseReport` parallel — owned by ENG3); `requirements.md` REQ-042 (per-step migration events accumulated into OpenReport); `adr/ADR-0.6.0-async-surface.md` Invariant D (eager warmup duration field).

**Test plan additions:**

- Test #17 (binding marshaling round-trip) — assert idiomatic types; no BigInt path required.
- Test #31 — Python `timedelta` round-trip preserves μs (not ns); document this in design/engine.md so reviewers do not regress to BigInt for SLI-irrelevant fields.

---

#### `ENG7` — `embedder_pool_size` default vs reader pool default

**Status:** RESOLVED 2026-04-30 by corpus amendment (confidence 84%).

**Resolution:**

Keep the divergence. `embedder_pool_size` is an operator-facing
throughput-vs-contention knob for the engine-owned embedder pool, while the
reader pool is a lighter SQLite read-concurrency facility. Embedded
deployments range from laptops sharing CPU with latency-sensitive host code to
dedicated ingest workers running heavier local models; embedder concurrency is
therefore the load-bearing tuning lever. Reader-pool sizing remains a separate
concern and does not force alignment.

**Cross-doc artifacts:** `adr/ADR-0.6.0-embedder-protocol.md`
(Invariant 4 rationale), `adr/ADR-0.6.0-scheduler-shape.md`
(embedder-pool rationale), `design/embedder.md`
(`embedder_pool_size` operator rationale).

---

#### `ENG8` — Per-call embedder timeout default citation

Trivial: cite P-NNN from acceptance.md Parameter table for 30s default. No HITL needed; mechanical during draft. Skip.

---

## Test planning — ENG1 + R1 + ENG3 + ENG6 + E6 (Tier 1 adoption, 2026-04-30)

Comprehensive test matrix to catch ordering bugs, race conditions, partial-state leaks, and binding-surface regressions. Lives in `docs/0.6.0/test-plan.md` once drafted; this section is the authoritative checklist.

### Step-ordering invariants (ENG1)

1. **WAL-before-EXCLUSIVE invariant.** Open against a fresh DB; assert no `-shm` file exists on disk after open succeeds. Inverted test: monkey-patch PRAGMA application to apply EXCLUSIVE first → assert `-shm` IS created → confirms detection. Cite [SQLite WAL docs](https://sqlite.org/wal.html) Inv-Lock-2.
2. **R1 pre-migration runs before migration loop.** Inject corrupt header on a fresh DB; assert `EngineOpenError::Corruption(HeaderProbe)` with NO migration-applied side effect (verify by checking `_fathomdb_migrations` table is unchanged from pre-open snapshot).
3. **R1 post-migration runs after migration applies schema.** Open against a v(N-1) DB requiring migration to v(N) where the embedder-profile column gains a constraint at v(N); identity check at step 8 must read the migrated row, not the pre-migration row. Test: trigger identity mismatch via stored profile that conflicts with migrated-but-not-pre-migrated constraint → assert `EngineError::EmbedderIdentityMismatch` surfaces with the post-migration row content.
4. **Step 1-10 monotonic span emission.** Assert tracing spans for `engine.open` parent emit child spans in numeric step order, never overlap, every span has `OpenStage` discriminant attribute.

### Failure-path teardown (ENG3 + ADR-corruption § 5)

5. **Failure at step 6 (corruption pre-migration).** Assert: lock fd released, SQLite conn closed, `.lock` sidecar `unlink`ed, NO writer thread spawned, NO `Engine` handle returned, NO migrations applied.
6. **Failure at step 8 (embedder identity mismatch).** Same as #5 plus: migrations were applied — verify they remain committed (no rollback) since corruption-of-data was not the failure cause.
7. **Failure at step 9 (warmup).** Same as #5 with migrations committed; assert `EngineError::Embedder(EmbedderError::Warmup)` cause chain preserves underlying embedder error for engine-internal logging but Display string is sanitized per X4 recipe.
8. **Failure at step 10 (post-thread-spawn).** Most subtle. Force a panic inside scheduler bring-up after writer thread is alive; assert reverse-ENG3 teardown runs (writer drained + joined within AC-022a bound), THEN lock released, THEN error returned. NO writer-thread leak. NO `Engine` handle. Check via `lsof` or equivalent that no fd survives.

### Race conditions (multi-process + multi-thread)

9. **Two processes racing `Engine.open` on same path.** Process A holds flock; process B `try_lock` returns immediately with `EngineOpenError::Locked{holder_pid: Some(A.pid)}`. Verify `holder_pid` matches via `/proc` or process listing.
10. **Stale `.lock` sidecar (holder crashed without `unlink`).** Lock fd auto-released on crash; new process should successfully acquire. Test: kill -9 holder mid-open, then immediately open from another process — assert success, no false `Locked` error.
11. **Reader pool concurrent open during writer warmup.** Should NOT happen with current architecture (readers spawned at step 10 after warmup), but assert via thread-sanitizer + `cargo test --release` that no reader connection is opened before step 10.
12. **Writer-thread shutdown timeout race.** `Engine.close` after a long-running write; writer drain hits AC-022a timeout. Assert close returns `EngineError` with timeout cause, lock STILL released, no zombie thread.
13. **Embedder warmup timeout (Invariant D 30s default).** Inject a 35s sleep in test embedder's `warmup()`; assert `EmbedderError::Warmup` with timeout-class cause, lock released, no thread spawned.
14. **Concurrent `Engine.open` + filesystem unlink of `.lock`.** Process A acquires flock; external `rm path.lock` while A is mid-open. Process B tries to open. Behavior: B creates a NEW `.lock`, gets a different fd, succeeds — DUAL WRITERS! This is a known fsync-policy edge case; document expected mitigation (ADR-database-lock-mechanism § Inv-Lock-1 plus advisory: operator MUST NOT manually unlink `.lock`).

### OpenReport correctness (ENG6)

15. **Per-stage durations sum to total within tolerance.** Sum of `pragma_setup_duration + detection_pre_migration.total_duration + Σ migrations[].duration + detection_post_migration.total_duration + embedder_warmup_duration + thread_spawn_duration` ≈ `total_open_duration` (allow <1ms drift for instrumentation overhead).
16. **`OpenReport` returned even when there are zero migrations.** Empty `migrations: vec![]` valid; not `None`.
17. **Binding marshaling round-trip.** Generate `OpenReport` from Rust → marshal to Python dict → assert all field names + types preserved. Same for TS struct via napi.
18. **`lock_acquired_at` precedes all other timestamps.** Sanity check on monotonic ordering.

### REQ-042 per-step migration events

19. **Successful migration emits one event per applied step.** Apply 3 migrations; assert 3 events with monotonically-increasing step numbers, durations, and final summary event with `applied_versions: [n+1, n+2, n+3]`.
20. **Failed migration mid-way.** Apply migration that fails at step 2 of 3; assert events for step 1 (success) + step 2 (failed with sanitized cause) + NO event for step 3, AND `EngineOpenError::Migration{step: 2, cause}` returned.

### Cross-binding error surface (E2 + E6)

21. **Python: `EngineError` is the sole top-level exception.** All variants surface as subclasses or attribute-tagged instances per bindings.md § 3. Test: trigger each ENG1 step→variant case from Python → assert `isinstance(e, EngineError)`.
22. **TS: two roots OK, but `EmbedderError::Warmup` surfaces from `Engine.open` failure as an `EngineOpenError`-rooted class (since it composes via `#[from]`).** Assert TS class chain: `EngineOpenError` → `engineError` field → `embedderError` field → `kind: 'Warmup'`.
23. **Sanitization: `Display` strings carry NO file paths, NO SQL fragments, NO parameter values, NO byte offsets.** Test: corrupt a DB at a known absolute path; assert error Display does not contain that path. Cite X4 recipe per foreign cause type.

### Documentation invariants (X1 cite-by-code)

24. **CI parses errors.md canonical table; every `code` referenced in engine.md or recovery.md exists.** Run as a doc-build hook (X6 territory but enforce here in advance).
25. **Every code in errors.md is cited by at least one of engine.md / recovery.md.** Orphan codes fail CI.
26. **Every `recovery_hint.doc_anchor` resolves to an existing heading in recovery.md.**

### Sources

- [SQLite Write-Ahead Logging](https://sqlite.org/wal.html) — WAL/EXCLUSIVE/`-shm` ordering invariant for tests #1-2.
- [Luca Palmieri — Rust Error Handling](https://www.lpalmieri.com/posts/error-handling-rust/) — per-module compose pattern, basis for tests #21-22.
- [thiserror+anyhow design](https://oneuptime.com/blog/post/2026-01-25-error-types-thiserror-anyhow-rust/view) — `#[from]` source-chain preservation, basis for test #7 + #23.
- [nrc error-docs — Error type design](https://nrc.github.io/error-docs/error-design/error-type-design.html) — design-by-arising pattern, justification for E6 runtime-rooted choice.

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

## Approach

- Use the agent orchestration runbook approach.
- This 'main' conversation is the agent orchestrator.
- Each item has agent prompt + 7-field deliverable contract + dependency arrows.
- Preflight permissions check.
- Manage potential and actual document conflicts.
- Mange sequence and document / decision dependency.
- Manage using correct subagent for the work (is a new skill required?), informing subagent at start, monitoring agent output
- Critic review
- Resolution

### Draft Sequencing

- Draft order on resolution: errors → recovery → engine.
- Next: dispatch Tier 1 agents.
  - Run sequentially due to dependency chain (E2 → ENG1; R1 → R3 → ENG1; X1 needs E2+R1+R3).
  - Suggested: E2 + R1 in parallel first (independent), then R3, then ENG1 + X1 in parallel.
