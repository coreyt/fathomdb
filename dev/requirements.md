---
title: 0.6.0 Requirements
date: 2026-04-27
target_release: 0.6.0
desc: User-visible needs + explicit non-goals for 0.6.0 rewrite
blast_radius: every public API surface; acceptance.md (1:1 mapping); test-plan.md (every REQ → ≥1 AC → ≥1 test); architecture.md (no orphan subsystems); design/* (each subsystem traces to ≥1 REQ)
status: locked
---

# Requirements

User-visible outcomes only. Each entry is a single, falsifiable statement
about what an operator, application caller, or release process can
observe — never an implementation verb. Each entry traces to ≥1 source
from `learnings.md` § "Raw requirement candidates" or to an accepted ADR.

`acceptance.md` issues one or more `AC-NNN` per REQ in Phase 3b. Drafted
from `learnings.md`; critic-pass amendments applied 2026-04-27.

Numbering: stable; do not renumber. Removed entries leave their REQ id
retired. Where a REQ has been split or reworked post-critic, the original
id range is preserved with `a / b / c` suffixes.

When a REQ contains a numerical target, an accepted ADR is the
authoritative source for the number; the REQ restates it for traceability
and may not diverge.

---

## Observability (REQ-001..REQ-008)

- **REQ-001 — Lifecycle phase attribution.** Operator can attribute any
  visible delay to a specific lifecycle phase (started / slow / heartbeat /
  finished / failed) without parsing stderr.
  _Source:_ `dev/design-response-cycle-feedback.md` Core Correctness Rule.
  _Cross-cite:_ ADR-0.6.0-async-surface (engine sync surface frames the
  lifecycle the operator sees).

- **REQ-002 — Host logging integration.** Operator owns subscriber
  configuration; fathomdb writes no log files of its own and emits all
  diagnostic events through whatever subscriber the host has registered
  in its language.
  _Source:_ `dev/design-logging-and-tracing.md` Principle 2.

- **REQ-003 — Cumulative engine counters.** Operator can read cumulative
  engine counters (queries, writes, write rows, errors by code, admin
  ops, cache hit/miss) on demand at any time without per-operation
  overhead.
  _Source:_ `dev/design-note-telemetry-and-profiling.md` Level 0.

- **REQ-004 — Per-statement profiling opt-in.** Operator can opt into
  per-statement profiling (wall-clock, statement-internal step counters,
  cache delta) without rebuilding the engine.
  _Source:_ `dev/design-note-telemetry-and-profiling.md` Level 1.

- **REQ-005 — SQLite-internal events surfaced.** Operator receives
  SQLite-internal corruption / recovery / I/O events through the same
  channel as fathomdb's own diagnostics.
  _Source:_ `dev/design-logging-and-tracing.md` Principle 5.

- **REQ-006a — Slow-statement signal.** Statements exceeding a
  configurable threshold (default 100 ms) are surfaced to the operator.
  _Source:_ `dev/design-note-telemetry-and-profiling.md` § Slow statement
  detection.

- **REQ-006b — Slow signal feeds lifecycle attribution.** The slow-statement
  signal contributes to the "slow" lifecycle transition surfaced under
  REQ-001.
  _Source:_ same as REQ-006a.

- **REQ-007 — Stress-failure context sufficiency.** Stress / robustness
  test failures carry enough context to identify thread group, kind, last
  error, and projection state without re-running the failure. (Field-set
  enumeration owned by `acceptance.md` per HITL F14.)
  _Source:_ `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md`
  § Observability Requirements.

- **REQ-008 — Vector-pending vs vector-failed distinguishability.**
  Operator can distinguish "vector work pending" from "vector work failed"
  via projection status; semantic-search timeout is interpretable by mode.
  _Source:_ `dev/notes/design-projection-freshness-sli-harness-2026-04-23.md`
  § Failure Interpretation.

## Performance (REQ-009..REQ-018)

Absolute targets; not deltas vs 0.5.x. ADRs / `acceptance.md` own gates,
tolerances, warmups, sample counts, and other acceptance-level parameters.
`test-plan.md` owns benchmark corpora, fixtures, harnesses, and executable
protocol wiring. Requirements lock blocks on `test-plan.md` fixture specs
landing first — no perf REQ is executable without them. Reference target =
`x86_64-unknown-linux-gnu` unless an ADR or AC names a different platform.

Numerical targets in this section are restatements of accepted ADRs;
the ADR is authoritative.

- **REQ-009a — Write throughput @ 1 KB ≥ 1,000 commits/sec** (sequential
  `WriteTx` commits, single client, `synchronous=NORMAL`, no projection
  load).
  _Source:_ ADR-0.6.0-write-throughput-sli; ADR-0.6.0-durability-fsync-policy.

- **REQ-009b — Write throughput @ 100 KB ≥ 100 commits/sec** (same
  workload; payload size as stated).
  _Source:_ same as REQ-009a.

- **REQ-010 — Text query latency p50 ≤ 20 ms; p99 ≤ 150 ms** on the
  text-only FTS5 path at 1 M chunk rows, QPS = 1 sequential, no
  concurrent writes, warm cache. Hybrid / auto-routed `search`
  inherits ADR-0.6.0-retrieval-latency-gates instead.
  _Source:_ ADR-0.6.0-text-query-latency-gates.

- **REQ-011 — Vector retrieval latency p50 ≤ 50 ms; p99 ≤ 200 ms** at
  1 M vectors @ 768-dim, `k=10`, single-process, no concurrent writes,
  warm cache.
  _Source:_ ADR-0.6.0-retrieval-latency-gates.

- **REQ-012 — `fathomdb doctor safe-export` ≤ 500 ms on the seeded benchmark dataset.**
  _Source:_ `dev/production-acceptance-bar.md`.

- **REQ-013 — Canonical-read freshness within the write transaction.**
  Canonical reads after a write commit reflect that write synchronously;
  no projection-style staleness window applies to canonical rows.
  _Source:_ `dev/notes/design-projection-freshness-sli-harness-2026-04-23.md`
  (canonical+FTS committed inside writer tx).

- **REQ-014 — FTS-search freshness within the write transaction.**
  FTS searches after a write commit reflect that write synchronously.
  _Source:_ same as REQ-013.

- **REQ-015 — Vector-projection freshness p99 ≤ 5 s post-commit.**
  Measured from primary write commit to projection table containing the
  corresponding vector row.
  _Source:_ ADR-0.6.0-projection-freshness-sli.

- **REQ-016 — Drain of 100 deterministic-embedder vectors ≤ 2 s.**
  _Source:_ `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md`.

- **REQ-017 — Mixed-retrieval stress workload keeps read p99 within
  `max(10 × baseline, 150 ms)`.**
  _Source:_ same as REQ-016.

- **REQ-018 — Reads do not serialize behind a single reader connection.**
  _Source:_ user-need preserved from `dev/design-reader-connection-pool.md`
  (dropped doc); concurrent-read SLI implied by ADR-0.6.0-retrieval-latency-gates'
  workload definition.

## Reliability (REQ-019..REQ-031)

- **REQ-019 — Zero `SQLITE_SCHEMA` warnings under concurrent reads + admin
  DDL.**
  _Source:_ `dev/notes/0.5.7-corrected-scope.md` T1 §4.

- **REQ-020a — Clean engine close releases all OS resources.**
  `Engine.close()` releases the database file lock and all FDs the
  engine opened.
  _Source:_ `dev/notes/0.5.7-corrected-scope.md` T1 §3.
  _Cross-cite:_ ADR-0.6.0-async-surface (sync surface — close is
  blocking on join).

- **REQ-020b — Host process exits ≤ 5 s of close.** Host process exits
  within 5 seconds of `Engine.close()` returning.
  _Source:_ memory `feedback_release_verification`.

- **REQ-021 — Bounded process exit ≤ 5 s without explicit close.** Host
  process exits within 5 seconds even when engine instances are not
  explicitly `close()`d.
  _Source:_ commit `b4fe850` (0.5.6 atexit Memex regression); HITL F8
  lifecycle lift.

- **REQ-022a — `DatabaseLocked` rejection on second open.** Second
  engine on the same DB file is rejected with a typed `DatabaseLocked`
  error while the first holds it, including while pending vector work
  exists.
  _Source:_ `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md`
  Test Set 3; `dev/notes/0.6.0-rewrite-proposal.md` POST 8.
  _Cross-cite:_ ADR-0.6.0-single-writer-thread.

- **REQ-022b — Second-open never corrupts state.** A rejected second
  open leaves the database file unmodified.
  _Source:_ same as REQ-022a.

- **REQ-023 — No deadlock on engine drop with pending vector work.**
  _Source:_ same as REQ-022a.

- **REQ-024 — `fathomdb doctor safe-export` covers committed WAL-backed state.** Never
  regresses to file-copy semantics.
  _Source:_ `dev/production-acceptance-bar.md`.

- **REQ-025a — Recovered databases preserve canonical rows.**
  _Source:_ `dev/production-acceptance-bar.md`.

- **REQ-025b — Recovery restores FTS usability.**
  _Source:_ same as REQ-025a.

- **REQ-025c — Recovery preserves vector profile metadata + table
  capability** (for vector-enabled DBs).
  _Source:_ same as REQ-025a.

- **REQ-026 — `excise_source` preserves auditability and leaves
  projections consistent.**
  _Source:_ `dev/production-acceptance-bar.md`.

- **REQ-027 — Canonical writes never blocked by projection unavailability**
  (FTS or vector).
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §13.

- **REQ-028a — No-embedder-wired hard-errors at call boundary.**
  Calling a vector-requiring operation with no embedder configured
  hard-errors at the call boundary; never silent-degrades.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §12.

- **REQ-028b — Kind-not-vector-indexed hard-errors at call boundary.**
  _Source:_ same as REQ-028a.

- **REQ-028c — Embedder dimension mismatch hard-errors at call
  boundary.**
  _Source:_ same as REQ-028a.
  _Cross-cite all three:_ ADR-0.6.0-vector-identity-embedder-owned.

- **REQ-029 — Hybrid retrieval surfaces a soft-fallback signal** when a
  non-essential branch could not contribute. (Field name owned by
  binding-interface ADRs.)
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §5.

- **REQ-030 — Tests and batch ingest can request bounded completion of
  background work**, with explicit timeout, through an `Engine` instance
  method rather than a sixth top-level SDK verb. (Method spelling owned by
  binding interfaces.)
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §14.
  _Cross-cite:_ ADR-0.6.0-async-surface Invariant A.

- **REQ-031 — Bounded provenance growth.** Operator can configure or
  trigger retention on provenance event tables; engine does not grow
  these tables unboundedly under steady-state writes.
  _Source:_ `dev/design-provenance-retention.md`.

- **REQ-031b — Zero corruption on power-cut.** Up to 100 ms of
  final-commit loss on power-cut acceptable; zero commit loss on
  OS-crash.
  _Source:_ ADR-0.6.0-durability-fsync-policy.

- **REQ-031c — Recovery time ≤ 2 s for a 1 GB DB at `Engine.open`** after
  unclean shutdown. Measured from process start to first accepted write
  transaction.
  _Source:_ ADR-0.6.0-durability-fsync-policy.

- **REQ-031d — Refuse-to-open on detected corruption.** `Engine.open`
  fails closed when corruption is detected at any open-path stage
  (WAL replay, header/format probe, schema probe, corrupt stored
  embedder-profile state). Failure
  surfaces as a structured `EngineOpenError::Corruption` carrying
  kind / stage / locator / `RecoveryHint { code, doc_anchor }`. The
  engine MUST NOT auto-truncate, auto-rebuild, auto-replay-with-skip,
  or auto-degrade to read-only. Recovery is reachable exclusively via
  the separate `fathomdb recover` CLI tool (consistent with REQ-037,
  REQ-054). On failure, no `Engine` handle is returned; the exclusive
  WAL lock is released; no SQLite connection is retained; no writer
  thread is spawned; no scheduler runs.
  _Source:_ ADR-0.6.0-corruption-open-behavior. _Cross-cite:_
  ADR-0.6.0-error-taxonomy (variant table extension);
  ADR-0.6.0-cli-scope (recovery CLI surface).

## Security (REQ-032..REQ-035)

- **REQ-032 — No network listener, no wire protocol.** All access is
  in-process or local subprocess; no TLS / auth / authz surface to
  misconfigure.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17 +
  Anti-requirements; `dev/design-note-encryption-at-rest-in-motion.md` §2.

- **REQ-033 — No implicit network fetch on `Engine.open`.** Engine never
  downloads or hosts embedder model weights; embedder is supplied by
  the caller.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` § Anti-requirements
  (embedder model hosting). _Cross-cite:_ ADR-0.6.0-default-embedder.

- **REQ-034 — FTS5 injection-safe text queries.** Agent / LLM-generated
  text queries cannot inject FTS5 control syntax; safe grammar tokenises
  at parse time and never passes raw input to FTS5.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §3 +
  Architecture § Query execution.

- **REQ-035 — `fathomdb doctor safe-export` artifact verifiable end-to-end.** Operator
  can verify a safe-export artifact via an SHA-256 manifest written
  alongside it.
  _Source:_ `dev/production-acceptance-bar.md` (current production
  control; harvest evidence). Design treatment in
  `dev/design-note-encryption-at-rest-in-motion.md` (deferred-doc).

## Operability (REQ-036..REQ-041)

- **REQ-036 — Two-root operator CLI: `recover` (lossy) + `doctor` (bit-preserving).**
  Operator recovers from physical, logical, or semantic corruption via a
  dedicated CLI without writing application code. Surface splits at the
  root by mutation semantics:
  - `fathomdb recover --accept-data-loss <sub-flag>...` — sole umbrella
    for any non-bit-preserving path. Sub-flags include `--truncate-wal`,
    `--rebuild-vec0`, `--rebuild-projections`, `--excise-source <id>`,
    `--purge-logical-id <id>`, `--restore-logical-id <id>`. The
    `--accept-data-loss` flag is mandatory; no default.
  - `fathomdb doctor <verb>` — read-only and bit-preserving only. Verbs:
    `check-integrity` (aggregator over R1 always-on + cheap-only tiers),
    `safe-export <out>`, `verify-embedder`, `trace --source-ref <id>`,
    `dump-schema`, `dump-row-counts`, `dump-profile`.
    Verb-level enumeration with concrete flag spelling + exit-code numbers
    lives in `interfaces/cli.md`; canonical verb table lives in
    `design/recovery.md`. `--json` is mandatory on every verb (REQ-024).
    Migrations are NOT a `doctor` verb — they run only inside `Engine.open`
    per REQ-042 / ADR-0.6.0-corruption-open-behavior § 5.
    _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` § Recovery tooling;
    `dev/dbim-playbook.md` §3, §11; HITL R3 (2026-04-30, conf 74%).
    _Cross-cite:_ ADR-0.6.0-cli-scope, ADR-0.6.0-corruption-open-behavior § 3,
    design/recovery.md, design/bindings.md § 1.

- **REQ-037 — Recovery tooling unreachable from runtime SDK.** Application
  callers cannot accidentally invoke `excise_source`, `purge_logical_id`,
  or `safe_export`. (Recovery surface is CLI-only — REQ-054 is the
  surface-shape corollary.)
  _Source:_ `dev/notes/0.5.7-corrected-scope.md` D2; rewrite-proposal
  § Recovery tooling. _Cross-cite:_ ADR-0.6.0-cli-scope.

- **REQ-038 — Source-ref blast-radius enumeration.** Operator can
  `trace --source-ref <id>` to enumerate every canonical row produced
  by a given run / step / action before excision.
  _Source:_ `dev/dbim-playbook.md` §6, §11.

- **REQ-039 — Single `check-integrity` invocation reports all integrity
  classes.** Physical, logical, and semantic integrity reported in one
  operator call. (Specific check set owned by `design/recovery.md`.)
  _Source:_ `dev/dbim-playbook.md` §10.

- **REQ-040 — Physical recovery rebuilds projections from canonical
  state.** Recovery never trusts recovered FTS5 / sqlite-vec shadow
  tables. (Specific canonical-table set owned by `design/engine.md`.)
  _Source:_ `dev/dbim-playbook.md` §7.

- **REQ-041 — Single-file deploy.** Operator deploys one binary +
  one `.sqlite` path; no server, no network dependency. Sidecar
  artifacts auto-managed by the engine (`-wal`, `.lock`) live at the
  same path with documented suffixes; they are part of the database
  file set, not separate operator-managed inputs.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17.
  _Cross-cite:_ ADR-0.6.0-vector-index-location,
  ADR-0.6.0-op-store-same-file,
  ADR-0.6.0-database-lock-mechanism (sidecar `.lock` interpretation;
  superseded 2026-05-02),
  ADR-0.6.0-database-lock-mechanism-reader-pool-revision (current
  contract; admits `-shm` as a normal WAL artifact during runtime).

## Upgrade / compatibility (REQ-042..REQ-046)

- **REQ-042 — Auto schema migrations on `Engine.open`.** No DBA step;
  open call reports applied version + per-step duration on completion
  **or failure**.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` Essentials §16;
  `dev/design-logging-and-tracing.md` Tier 1/2.

- **REQ-043 — Hard-error on 0.5.x-shaped DB.** Opening a 0.5.x-shaped
  database with 0.6.0 hard-errors at POST naming the schema version
  seen, never silently attempts partial reads.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` POST 1 + § What
  happens to 0.5.x.

- **REQ-044 — Hard-error on embedder mismatch at re-open.** Re-opening
  a store with a differently-dimensioned or differently-identified
  embedder hard-errors at POST naming both sides.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` POST 3–4. _Cross-cite:_
  ADR-0.6.0-vector-identity-embedder-owned.

- **REQ-045 — Schema-migration accretion guard.** Every post-v1
  migration that adds a table or column names a table / column it
  removes (or documents why removal is impossible).
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` § Future
  schema-migration policy.

- **REQ-046a — No 0.5.x → 0.6.0 deprecation shims.**
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` § What we got wrong #5.
  _Cross-cite:_ ADR-0.6.0-no-shims-policy,
  ADR-0.6.0-deprecation-policy-0-5-names.

- **REQ-046b — Within-0.6.x breaks announced + removed in same release.**
  No multi-release deprecation cycles.
  _Source:_ same as REQ-046a. _Cross-cite:_ ADR-0.6.0-no-shims-policy.

## Supply chain (REQ-047..REQ-052)

- **REQ-047 — Version-skew detected at resolution time.** Engine and
  sibling embedder packages share a `fathomdb-embedder-api` crate with
  a stable trait set; version-skewed installs are detected at
  resolution time, not at runtime.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` § Version-skew policy.

- **REQ-048 — Co-tagged sibling releases.** All three packages
  (`fathomdb`, `fathomdb-embedder`, `fathomdb-embedder-api`) tag and
  publish together every release even when content is unchanged.
  _Source:_ same as REQ-047.

- **REQ-049 — Single source of truth for version.** Cargo workspace and
  Python pyproject agree on version at publish time; mismatch blocks
  publish.
  _Source:_ `dev/release-policy.md` § Version Source Of Truth + § Release
  Gates.

- **REQ-050 — Atomic multi-registry publish.** A release is published
  only after all artifact targets succeed; partial publishes are
  forbidden. Targets enumerated by ADR-0.6.0-tier1-ci-platforms +
  per-binding publish workflow.
  _Source:_ `dev/release-policy.md` § Manual Fallback + § Release
  Workflow Shape; ADR-0.6.0-tier1-ci-platforms.

- **REQ-051 — `sqlite-vec` availability validated at open.** Engine
  validates the `sqlite-vec` extension is available at `Engine.open`
  whenever the store contains any vector rows; never fails per-query
  under partial extension installs.
  _Source:_ `dev/notes/0.6.0-rewrite-proposal.md` POST 5.
  _Cross-cite:_ ADR-0.6.0-sqlite-vec-acceptance,
  ADR-0.6.0-vector-index-location.

- **REQ-052 — Registry-installed wheel is the release gate.** Release
  evidence is the published wheel installed from the public registry
  into a fresh venv and run end-to-end.
  _Source:_ memory `feedback_release_verification`;
  `dev/notes/0.6.0-rewrite-proposal.md` Tests §5. _Cross-cite:_
  ADR-0.6.0-tier1-ci-platforms (per-binding smoke spec).

## Public surface (REQ-053..REQ-059)

- **REQ-053 — Five-verb application runtime SDK surface.** Public
  application runtime SDK surface is `Engine.open`, `admin.configure`,
  `write`, `search`, `close` — five verbs, no more. (CLI is a separate
  surface — see REQ-036, REQ-054.)
  _Source:_ `dev/notes/0.5.7-corrected-scope.md` § 0.6.0 RFC;
  `dev/notes/0.6.0-rewrite-proposal.md` § Public API: five verbs.
  _Cross-cite:_ ADR-0.6.0-prepared-write-shape (write-shape),
  ADR-0.6.0-typed-write-boundary.

- **REQ-054 — Recovery / repair verbs are CLI-only.** `rebuild_projections`,
  `restore_*`, `check_integrity`, `safe_export`, etc. are reachable
  only via the CLI surface, not via the runtime SDK. (Distinct from
  REQ-037 which is the SDK-side unreachability claim; this is the
  CLI-side completeness claim.)
  _Source:_ `dev/notes/0.5.7-corrected-scope.md` D2. _Cross-cite:_
  ADR-0.6.0-cli-scope.

- **REQ-055 — Freshness cursors exposed on read tx + write commit.**
  Read transactions expose a monotonic non-decreasing
  `projection_cursor`; write commits return a monotonic write cursor
  identifying the commit just accepted. Clients reason about staleness
  by polling until `read_projection_cursor >= write_cursor`; the write
  return value is not itself the read-side `projection_cursor`.
  _Source:_ ADR-0.6.0-projection-freshness-sli.

- **REQ-056 — Engine errors as typed, language-idiomatic exceptions.**
  In every binding (Python, TypeScript, CLI) engine errors surface as
  a typed exception hierarchy; clients distinguish error classes
  without string-pattern matching.
  _Source:_ ADR-0.6.0-error-taxonomy.

- **REQ-057 — Op-store collection kinds are authoritative.** 0.6.0
  operational-state persistence exposes exactly two authoritative collection
  kinds: `append_only_log` and `latest_state`. `append_only_log` preserves
  durable history by appending authoritative rows to
  `operational_mutations`; `latest_state` stores the authoritative current row
  directly in `operational_state` keyed by `(collection_name, record_key)`.
  0.6.0 does not reintroduce a derived `operational_current` table.
  _Source:_ ADR-0.6.0-op-store-same-file.

- **REQ-058 — Op-store collection lifecycle is explicit and narrow.**
  Operational collections are named registry entries with declared metadata
  (`kind`, schema/retention metadata, format version, creation time) and a
  fixed collection kind. 0.6.0 exposes no collection rename, disable,
  soft-retire, or alternate latest-state lifecycle.
  _Source:_ ADR-0.6.0-op-store-same-file. _Cross-cite:_
  `design/op-store.md`.

- **REQ-059 — Projection failure diagnosis and regeneration are operator
  workflows.** Exhausted projection failures are recorded durably in
  `projection_failures`, and operators can explicitly regenerate projections
  from canonical state through the accepted recovery surface. The workflow name
  "regenerate" in 0.6.0 maps to
  `fathomdb recover --accept-data-loss --rebuild-projections`; it is not a
  separate SDK verb or separate CLI root.
  _Source:_ ADR-0.6.0-projection-model; ADR-0.6.0-cli-scope.

---

## Security hardening (REQ-060..REQ-066)

Added 2026-05-02 as an HITL amendment to the locked corpus per
`dev/security-review.md` HITL decision. Each REQ closes a 0.6.0 security
finding with a user-visible falsifiable outcome.

- **REQ-060 — Op-store payload validation completes in bounded time.**
  Op-store JSON-Schema payload validation against any registered
  `schema_id` completes in bounded time even when the schema's `pattern`
  / `patternProperties` regex is adversarial. Caller observes
  `SchemaValidationError` (or success), never a wedged writer.
  _Source:_ `dev/security-review.md` SR-002. _Cross-cite:_
  `design/op-store.md` § Write contract; `design/errors.md`
  `SchemaValidationError`.

- **REQ-061 — Op-store schemas reject external `$ref`.** Schema
  registration via `admin.configure` rejects any schema containing a
  non-fragment `$ref` URI (`http://`, `https://`, `file://`, or any
  network-resolvable scheme). Rejection happens at registration time,
  before any payload validation, with a typed error.
  _Source:_ `dev/security-review.md` SR-003. _Cross-cite:_
  `design/op-store.md` schema registration.

- **REQ-062 — Embedder-returned vector dimension validated before vec0
  write.** A wrong-length vector returned by the caller-supplied
  embedder surfaces as `EmbedderDimensionMismatchError` and produces no
  vec0 row write; the writer transaction rolls back cleanly.
  Dimension validation occurs at exactly one boundary, between embedder
  return and vec0 write.
  _Source:_ `dev/security-review.md` SR-004. _Cross-cite:_
  `design/embedder.md`; `design/vector.md`; `design/errors.md`
  `EmbedderDimensionMismatchError`.

- **REQ-063 — FFI panics surface as language-native exceptions.** A
  Rust panic raised inside a Python or TypeScript binding entry point
  surfaces to the caller as a typed binding exception; it never aborts
  the host process and never unwinds across the FFI boundary as
  undefined behavior.
  _Source:_ `dev/security-review.md` SR-006. _Cross-cite:_
  `design/bindings.md`; `interfaces/python.md`;
  `interfaces/typescript.md`.

- **REQ-064 — FFI string inputs reject embedded NUL and unpaired
  surrogates.** Any caller-supplied string (Python `str` /
  TypeScript `string`) reaching a `write` / `search` /
  `admin.configure` argument is rejected with `WriteValidationError`
  if it contains an embedded `\0` byte or an unpaired UTF-16
  surrogate. Rejection happens at the binding layer, before the
  payload reaches the writer.
  _Source:_ `dev/security-review.md` SR-007. _Cross-cite:_
  `interfaces/python.md`; `interfaces/typescript.md`;
  `design/errors.md` `WriteValidationError`.

- **REQ-065 — Error `Display` output omits internal SQL, absolute
  paths, and parser byte offsets.** The `Display` representation of
  every `EngineError` and `EngineOpenError` variant (and their
  language-native binding counterparts) omits raw SQL fragments,
  absolute host filesystem paths, and parser byte offsets. Internal
  diagnostic chains remain available to engine logging.
  _Source:_ `dev/security-review.md` SR-008. _Cross-cite:_
  `design/errors.md` § Foreign-cause sanitization.

- **REQ-066 — Migration failure preserves prior `user_version`.** When
  a schema migration fails mid-way, the SQLite `PRAGMA user_version`
  remains at the prior version after `Engine.open` returns
  `MigrationError`. A subsequent reopen with a corrected migration
  step starts from the prior version, not from a partially-applied
  state.
  _Source:_ `dev/security-review.md` SR-010. _Cross-cite:_
  `design/migrations.md`; `design/engine.md` open-path step 5;
  `design/errors.md` `MigrationError`.

## Non-goals

(Carried from `plan.md` for cross-linking; authoritative in `plan.md`.)

- No data migration from 0.5.x (fresh-db-only).
- No 0.5.x upgrade path in 0.6.0.
- No 0.5.x backports from this branch.
- No perf baseline capture of 0.5.x.
- No risk register.
- No glossary / rename migration map.

## Source-trace summary

Every REQ above cites either a specific `dev/` source file (preserved
under `docs/archive/0.5.x/` per Phase 1a disposition where applicable),
an accepted ADR, or a memory id. Citations are not exhaustive — they
identify the originating source, not every place an outcome is restated.
For numerical targets, the cited ADR is authoritative.
