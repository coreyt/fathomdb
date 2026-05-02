---
title: Bindings Subsystem Design
date: 2026-04-29
target_release: 0.6.0
desc: Cross-language binding strategy (Python, TypeScript, CLI) — concerns that span all bindings collectively, distinct from per-surface signatures owned by interfaces/{python,ts,cli}.md
blast_radius: src/rust/crates/fathomdb (Rust facade); src/rust/crates/fathomdb-embedder-api (semver-pinned trait crate; REQ-047 link-time check); src/python/ (PyO3 cdylib); src/ts/ (napi-rs cdylib); src/rust/crates/fathomdb-cli (binary); interfaces/{python,ts,cli,rust,wire}.md (per-surface signatures); design/errors.md (variant→class matrix owner); design/release.md (CI smoke gate owner); ADR-error-taxonomy + ADR-async-surface + ADR-python-api-shape + ADR-typescript-api-shape + ADR-cli-scope + ADR-embedder-protocol + ADR-prepared-write-shape + ADR-corruption-open-behavior; dev/design-logging-and-tracing.md (Tier 1/2 carryovers cited in § 8); build pipelines (pip + npm)
status: locked
---

# Bindings — Design

This file is **cross-cutting**. It commits to invariants that hold *across* every public binding (Python, TypeScript, CLI) collectively, plus the protocol that connects each binding to the underlying Rust engine. Per-binding signatures, examples, and error cases live in `interfaces/{python,ts,cli}.md`; this file does not duplicate those.

## 0. Value-test against `interfaces/`

Per `plan.md` Phase 3 step 4, `design/bindings.md` is written first to test whether it fills a role distinct from per-surface `interfaces/*.md`. Verdict: **distinct role; KEEP**. The following concerns are *properties of bindings collectively*, not statements about a single binding's surface, and have no clean home in any per-language interface file:

| Cross-cutting concern | Why not in `interfaces/<lang>.md` |
|---|---|
| Surface-set parity (every SDK binding exposes the same five verbs and *only* those five) | Per-surface files would each enumerate their own list; a parity claim across bindings cannot be expressed by any single file. |
| Error-mapping *protocol* (one class per variant; typed attrs not stringified args; single rooted hierarchy; no string-pattern dispatch) | The protocol commitments are properties OF the bindings collectively. The mapping *matrix itself* lives in `design/errors.md` (architecture.md § 2) and is cited from interfaces/*.md; this file commits the protocol that any matrix must satisfy. |
| Async dispatch model and where Invariants A–D land per binding | A property of the engine boundary expressed differently per language; the *protocol* (e.g. "Promise edges live in TS only; Python is sync") is shared. |
| Embedder identity + cross-language consistency (same DB opened from Python and TS resolves the same `EmbedderIdentity`) | Multi-binding invariant; not a per-language signature. |
| Lock + process-exclusivity contract spanning bindings | Same DB file opened from Python in process A and TS in process B must observe the engine's exclusive WAL lock identically. |
| Build/packaging contracts (`pip install -e src/python/`; `npm install` against napi-rs `src/ts/`; CLI binary release) | Operations on the binding *shipping path*, not on the binding's runtime surface. |
| Recovery-surface unreachability across all SDK bindings (REQ-037 / REQ-054 / REQ-031d) | A *non-presence* claim that holds across every SDK; awkward to assert in any single per-language file. |

Per-binding *signatures* still belong in `interfaces/{python,ts,cli}.md`. This file commits to *protocol*, not to syntax.

## 1. Surface-set parity invariant

Every **SDK** binding's public top-level surface MUST be exactly the five-verb canonical set in that binding's idiomatic casing:

```text
Engine.open    admin.configure    write    search    close
```

(REQ-053; gated by AC-057a.) The parity claim is symmetric *across SDK bindings* (Python + TypeScript): a verb appears in every SDK binding or in none. Adding a verb requires updating all SDK bindings together; per-SDK-binding surface sets are not allowed to drift.

Rust is a stable public facade contract, but it is not part of the
Python/TypeScript SDK parity set. `interfaces/rust.md` owns the Rust facade
shape.

CLI is a separate, non-SDK surface (per ADR-0.6.0-cli-scope, architecture.md § 1: "Does NOT mirror full SDK 5-verb surface"). CLI exposes a structurally distinct operator command set (`fathomdb doctor <verb>` and `fathomdb recover`). CLI parity with SDK is NOT required and NOT promised; adding an SDK verb does not imply adding a CLI command.

The SDK surface MUST NOT contain any name in `{recover, restore, repair, fix, rebuild, doctor}` (REQ-037, REQ-054, REQ-031d). These verbs exist exclusively on the CLI surface.

REQ-030's bounded-completion surface is **not** a sixth top-level SDK verb.
It is an `Engine` instance method. The per-language method spelling is owned by
`interfaces/{rust,python,typescript}.md`; this file commits only that it must
not widen the top-level parity set from five verbs to six.

## 2. Lifecycle dispatch model

Each binding owns the dispatch model for its language. The protocol that connects them to the engine is shared.

| Binding | Dispatch model | Engine call shape | Owning ADR |
|---|---|---|---|
| Python | Sync surface (Path 1) | Direct PyO3 call into Rust; engine is sync; Python `asyncio` users wrap with `run_in_executor` (documented pattern, not first-party API). | ADR-0.6.0-python-api-shape; ADR-0.6.0-async-surface (Path 1 + Invariants) |
| TypeScript | Promise surface (Path 2) | napi-rs `ThreadsafeFunction` + TS binding-owned Rust handoff pool sized `num_cpus::get()`. Decoupled from libuv's `fs/dns/crypto` thread pool. TS may expose adapter-specific sizing control, but that control is owned by the TS binding runtime rather than the canonical engine-config set. | ADR-0.6.0-typescript-api-shape; ADR-0.6.0-async-surface (Path 2) |
| CLI | Sync subcommand entry; binary returns process exit code per ADR-0.6.0-cli-scope. | Direct sync call into Rust; same engine as SDK. | ADR-0.6.0-cli-scope |

Async invariants A–D from ADR-0.6.0-async-surface manifest in every binding. The invariants themselves are owned by that ADR; this file commits only the *binding-side* assertion that no binding exposes an escape hatch:

- **Invariant A (scheduler post-commit).** Cite ADR-async-surface § Decision. Bindings expose no escape hatch: bindings do not introduce additional locking around `write`, do not pre-dispatch scheduler work from the caller thread, and do not provide a "skip-scheduler" path.
- **Invariant B (engine-owned embedder thread).** Cite ADR-async-surface § Decision. Bindings expose no escape hatch: embedder calls always run on the engine-owned embedder pool, never on the binding's caller thread (Python GIL-holder, TS libuv worker, CLI main thread).
- **Invariant C (embedder-protocol no-reentrancy).** Cite ADR-async-surface + ADR-embedder-protocol. Bindings document this constraint in their `Embedder` impl docs; tests verify a buggy user-supplied embedder cannot deadlock the engine. Bindings expose no escape hatch (no "reentrant embedder" config flag).
- **Invariant D (eager model warmup + per-call timeout).** Cite ADR-async-surface § Decision (eager warmup at `Engine.open`; per-call timeout default 30s). Bindings expose no escape hatch: no cold-load path, no per-call-timeout override that disables the watchdog. Reporting of model-load duration is part of `Engine.open`'s structured result, the shape of which is owned by `design/engine.md`.

## 3. Error-mapping protocol

The Rust error variant set is owned by **`ADR-0.6.0-error-taxonomy`** (BEHAVIOR commitment). The variant→class mapping matrix (Rust variant → Python exception → TS error class) is owned by **`design/errors.md`** per architecture.md § 2 (which assigns the `errors` module to that file). This file commits only the *protocol* claims that the mapping must satisfy.

Protocol commitments (apply to every binding):

- **One concrete class per variant.** Never a generic `EngineError` with a `kind` discriminator string. Tests dispatch on `except <Specific>` (Python) and `instanceof <Specific>` (TS) without inspecting messages. (REQ-056; AC-060a.)
- **Single rooted hierarchy.** Every binding's exception/error hierarchy roots at one base class (Python: `fathomdb.EngineError`; TS: `FathomDbError`) so callers can catch broad failures while still narrowing per variant.
- **Typed attributes, not stringified payloads.** Variant fields surface as typed attributes on the exception/error object, not as `args[0]` tuples or stringified blobs. The exact attribute spelling (idiomatic per binding) is owned by `interfaces/{python,ts}.md`; this file commits that the attribute exists and is structurally typed.
- **Stable dispatch key for `CorruptionError`.** `recovery_hint.code` (idiomatic casing per binding) is the stable dispatch key per ADR-0.6.0-corruption-open-behavior § 2; `recovery_hint.doc_anchor` is the doc pointer. Bindings do not flatten or stringify `recovery_hint`.
- **CLI is structural.** Each variant maps to a stable exit code + a structured stderr line carrying the variant's stable code (e.g. `recovery_hint.code` for `Corruption`). The exit-code table itself lives in `interfaces/cli.md`; the variant→exit-code authority is in `design/errors.md`.
- **Top-level Rust error type plurality.** The Rust surface exposes more than one top-level error type (`EngineError` for runtime ops, `EngineOpenError` for `Engine.open` per ADR-0.6.0-corruption-open-behavior). `design/errors.md` enumerates which top-level Rust types exist and how each variant routes to a binding class; Python and TypeScript expose one catch-all base while preserving distinct leaf classes.

Decision note:

- 0.6.0 uses one exported catch-all base type across Python and TypeScript.
- TypeScript's catch-all base is `FathomDbError`; every concrete leaf class in
  `design/errors.md` extends it.

This file does NOT enumerate the variant→class matrix; that authority lives in `design/errors.md` and is cited by `interfaces/{python,ts,cli}.md`. Per-language interface files do not duplicate the matrix.

## 4. Marshalling strategy

`PreparedWrite` (per ADR-0.6.0-prepared-write-shape) is the only typed shape the engine accepts. Bindings construct `Vec<PreparedWrite>` from idiomatic per-language inputs and never touch raw SQL.

| Direction | Boundary | Owning ADR |
|---|---|---|
| Caller → engine: `PreparedWrite::Node` / `Edge` / `OpStore(OpStoreInsert)` / `AdminSchema` | Per-binding builder constructs the variant. No raw SQL accepted. | ADR-0.6.0-typed-write-boundary; ADR-0.6.0-prepared-write-shape |
| Caller → engine: vector input | Bindings accept idiomatic typed arrays. When the input is already LE-f32 contiguous (NumPy `ndarray[float32]`, TS `Float32Array`), the binding zerocopy-casts to `&[u8]` per ADR-0.6.0-zerocopy-blob (no copy). Non-contiguous inputs (e.g. Python `list[float]`) are converted once into an LE-f32 buffer at the binding boundary. | ADR-0.6.0-zerocopy-blob; ADR-0.6.0-vector-identity-embedder-owned |
| Caller → engine: op-store payload | Bindings accept idiomatic dict / object; bindings serialize to JSON; engine validates against `schema_id` save-time before commit. | ADR-0.6.0-json-schema-policy; AC-060b |
| Engine → caller: search results | Engine returns owned typed result rows; bindings expose them as idiomatic types (Python `list[dict]`/dataclass; TS object). No lazy cursors crossing the FFI boundary. | ADR-0.6.0-retrieval-pipeline-shape |
| Engine → caller: errors | Per § 3. | ADR-0.6.0-error-taxonomy |

JSON-Schema validation behavior (AC-060b) is invariant across bindings:
validation fires save-time, pre-commit, on the writer thread; failure surfaces
as `SchemaValidationError`; no open-time re-validation; bindings do *not* run
their own pre-engine validation pass (single source of truth).

## 5. Embedder identity invariant

Per ADR-0.6.0-vector-identity-embedder-owned + ADR-0.6.0-embedder-protocol, vector identity is owned by the embedder, not by per-DB config. Cross-binding consequence:

- A DB written by Python with embedder identity `E` and re-opened by TypeScript MUST resolve the same `EmbedderIdentity`. Bindings do not pin or override identity at open.
- Identity-mismatch surfaces uniformly as `EmbedderIdentityMismatch` (per § 3 mapping). Bindings do not auto-rebuild on mismatch (consistent with ADR-0.6.0-corruption-open-behavior's no-auto-recover posture for open failures).
- A user-supplied embedder (Python or TS impl) declares its `EmbedderIdentity` per ADR-0.6.0-embedder-protocol Invariants 1–4. Binding-side bindings do *not* synthesize identity from binding metadata; identity is the embedder's responsibility.
- REQ-047 (embedder version-skew detection) is satisfied at link/resolution time by the semver-pinned `fathomdb-embedder-api` trait crate (architecture.md § 1) plus the embedder-owned `EmbedderIdentity` carried in stored vectors. Bindings do NOT perform runtime version-skew checks; they consume the typed mismatch error surfaced by the engine.

## 6. SDK symmetry, CLI boundary, and config classes

Two different symmetry rules exist and must not be conflated:

- **SDK surface symmetry.** Python and TypeScript expose the same canonical SDK
  verb set. This is the hard parity rule from § 1.
- **Engine-config symmetry.** Engine-owned knobs named in `design/engine.md`
  must be reachable from every SDK binding in idiomatic form.

CLI is outside both parity claims. It is a separate operator surface and does
not promise flag-for-flag mirroring of SDK verbs or SDK config.

The engine-config symmetry rule applies only to knobs that change engine
behavior after the binding boundary. It does **not** apply to binding-runtime
mechanics that exist only to bridge one host runtime into the engine.

Examples:

- `embedder_pool_size` — engine-owned runtime knob; symmetry required across
  SDK bindings.
- `scheduler_runtime_threads` — engine-owned runtime knob; symmetry required
  across SDK bindings.
- Python `run_in_executor` usage — caller-side runtime pattern, not an engine
  knob.
- TypeScript `ThreadsafeFunction` pool sizing — TS binding runtime mechanic per
  ADR-0.6.0-async-surface. A TS binding may surface it near `Engine.open`, but
  it is not part of the canonical engine-config set and does not create a
  Python-parity obligation.

CLI inherits engine defaults unless `interfaces/cli.md` explicitly grants a
flag. That inheritance posture does not weaken the SDK symmetry rule because
CLI is not an SDK binding.

## 7. Lock + process-exclusivity contract

Per `architecture.md` § 5 + ADR-0.6.0-database-lock-mechanism (#30), only one `Engine` instance per database file may be open at a time. The lock mechanism is **hybrid**: a sidecar `{database_path}.lock` flock (Rust std `File::try_lock`, per-OFD exclusion semantics) PLUS `PRAGMA locking_mode=EXCLUSIVE` on the writer connection in WAL. Sidecar = pre-open fail-fast + operator-diagnostic PID. SQLite EXCLUSIVE writer-lock = same-process backstop + removes `-shm`. Reader connections use NORMAL locking_mode (REQ-018 multi-reader concurrency preserved).

Cross-binding consequences:

- Python in process A holding `Engine.open(path)` → TS in process B calling `Engine.open(path)` MUST fail with `DatabaseLocked { holder_pid }` regardless of binding identity. The sidecar flock is the load-bearing layer for cross-process exclusion; it surfaces BEFORE SQLite I/O begins, so Engine.open does not pay migration / embedder warmup cost on a doomed open.
- The lock's lifetime is bound to the `Engine` instance; closing or dropping the engine releases it (drops the sidecar lock fd + closes SQLite connections). `Engine.close` is required (REQ-020a; AC-022a).
- Same-process two-`Engine` (Python `Engine` and TS `Engine` in the *same* process targeting the *same* path) is also forbidden. The second `Engine.open` opens a NEW `File` handle for the sidecar; per-OFD `flock` semantics return `WouldBlock`, surfacing as `DatabaseLocked`. The SQLite EXCLUSIVE writer-lock is the defense-in-depth backstop. Bindings do NOT maintain an in-process registry of held paths.
- Path canonicalization: `Engine.open` canonicalizes the parent directory and appends the leaf filename before deriving `{...}.lock` path, defeating symlink + bind-mount aliasing. Bindings do not perform their own canonicalization.

The corruption-on-open path (ADR-0.6.0-corruption-open-behavior § 5; AC-035c) MUST release the sidecar lock + close any opened SQLite connection before returning `CorruptionError`. Bindings inherit this guarantee; no binding-level workaround is permitted.

## 8. Logging / tracing subscriber attachment

The engine emits structured tracing events (per `dev/design-logging-and-tracing.md` Tier 1/2 carryovers). Binding adapters attach a host subscriber:

- Python: caller registers a `logging`-backed adapter via a binding-provided helper that maps tracing events into Python `LogRecord`s.
- TypeScript: caller registers a callback invoked per event.
- CLI: when run in human-facing mode, attaches a console subscriber; when run in machine-facing `--json` mode, emits the verb-owned JSON shape from `interfaces/cli.md` / `design/recovery.md`. `doctor check-integrity` is a single JSON object; other verbs own their own machine-readable contract.

Across bindings, the *engine event payload* is wire-stable: same field names,
same types, same lifecycle phase tag enum (AC-001). The host adapter MAY
translate or derive **host-native required fields** from the engine event when
the host logger backend requires them. Engine event fields appear under a
stable `fathomdb` payload key in the host record so downstream filters can
target them precisely; the engine never emits a host-named field directly.

### `fathomdb` payload envelope

Within the host record, the stable `fathomdb` payload key carries one of these
surface shapes:

- response-cycle event: `phase` plus producer-owned operation context
- diagnostic event: `source`, `category`, and producer-owned detail payload
- counter snapshot: `counter_snapshot`
- profile record: `profile_record`
- stress-failure payload: `stress_failure`
- migration step event: `migration_step`

Ownership split:

- `design/lifecycle.md` owns `phase`, `source`, `category`,
  `counter_snapshot`, `profile_record`, and `stress_failure`
- `design/migrations.md` owns `migration_step`
- binding adapters own how these are embedded into host-native logger records

Acceptance note: current ACs lock the typed payload members more directly than
the outer `fathomdb` envelope key. Do not rename the envelope key without
updating both this file and the interface docs.

Default subscriber posture: when no subscriber is registered, the engine writes nothing — no log files, no stderr noise (AC-002).

## 9. Build / packaging strategy

Binding-shipping protocol — the canonical build entry point per binding. (CI smoke gates and post-publish verification are owned by `design/release.md` per architecture.md § 2; this file commits only the per-binding build path.)

| Binding | Build path | Authoritative source |
|---|---|---|
| Python | `pip install -e src/python/` (PyO3 cdylib package `fathomdb` under `src/python/`) | architecture.md § 1; memory `feedback_python_native_build` |
| TypeScript | `npm install` against `src/ts/` (napi-rs cdylib package `fathomdb` under `src/ts/`) | architecture.md § 1; ADR-0.6.0-typescript-api-shape |
| CLI | `cargo build --release -p fathomdb-cli`; release artifact is a single binary per platform | ADR-0.6.0-cli-scope; ADR-0.6.0-tier1-ci-platforms |

CI build, per-binding smoke testing, and post-publish verification (memory `feedback_release_verification`; AC-056) are owned by `design/release.md`. Developer ergonomics (e.g. "do not manually `cargo build && cp`") are out of scope here and live in `src/python/README.md` / contributor docs.

## 10. Recovery surface unreachability

The bindings collectively MUST NOT expose any recovery verb on the SDK surface. (REQ-037, REQ-054, REQ-031d; gated by AC-057a + AC-035d.) Recovery is invoked exclusively via the CLI binary `fathomdb recover` and `fathomdb doctor <verb>`. The application SDK contains no path that mutates a corruption-marked database.

This is a *non-presence* claim, not a per-binding signature, and is therefore owned here rather than in `interfaces/{python,ts}.md`.

## 11. Failure modes + recovery

| Failure mode | Surfacing | Bindings contract |
|---|---|---|
| Lock contention on open | `DatabaseLocked` per § 3 + § 7 | Every binding maps identically; no retry loop in the binding. |
| Corruption on open | `CorruptionError` carrying `detail` per § 3 + ADR-0.6.0-corruption-open-behavior | Every binding surfaces `detail` as a structurally-typed object (not a stringified blob) exposing `recovery_hint.code` (idiomatic casing per binding) as the stable dispatch key, per ADR-corruption-open-behavior § 2. Exact attribute spelling owned by `interfaces/{python,ts}.md`. Bindings do NOT auto-invoke recovery. |
| Embedder identity mismatch | `EmbedderIdentityMismatchError` per § 3 | Bindings do NOT auto-rebuild and 0.6.0 provides no open-time bypass. Caller must resolve the mismatch outside the SDK surface. |
| Schema migration failure | `MigrationError` per § 3 | Bindings surface per-step duration + failure reason from the engine's structured open result (REQ-042). |
| Write validation failure (JSON Schema) | `SchemaValidationError` per § 3 | Surfaces save-time, pre-commit (AC-060b); engine state is unchanged after rejection. Bindings do NOT pre-validate. |
| Engine.close while writes in flight | `ClosingError` per § 3 (variant per ADR-0.6.0-error-taxonomy § Consequences) | Bindings surface as a typed close-race error; `Engine.close` itself is required (REQ-020a; AC-022a) and bindings do not auto-retry. |
| Backpressure exhaustion / overload | `OverloadedError` per § 3 (variant per ADR-0.6.0-error-taxonomy + ADR-0.6.0-projection-model layer-4 backpressure) | Bindings surface as a typed overload error; do NOT auto-retry; caller decides retry policy. |
| User-supplied embedder buggy / blocking | Engine watchdog enforces per-call timeout per Invariant D (default 30 s); surfaces as `EmbedderError`. | Bindings document Invariant B + C constraints in their `Embedder` impl docs; tests verify a buggy embedder cannot deadlock the engine. |

## 12. Boundaries with `interfaces/*.md`

| Lives here (`design/bindings.md`) | Lives there (`interfaces/{python,ts,cli}.md`) |
|---|---|
| Surface-set parity claim across all bindings | The exact public symbol list per binding |
| Error-mapping protocol (the matrix itself lives in `design/errors.md`) | Per-language stability posture, deprecation notes, exception hierarchy diagram |
| Async dispatch model + how Invariants A–D manifest per binding | Per-symbol async/sync signature, parameter types, return types |
| Marshalling protocol (typed boundary; vector encoding; JSON-Schema cadence) | Per-language type names, type aliases, idiomatic accessors |
| Embedder identity invariant across bindings | Per-binding `Embedder` trait/protocol shape and impl examples |
| Config knob parity claim | Per-language config object shape, kwargs spelling, CLI flag name |
| Lock contract across bindings | (Not duplicated; cite this file.) |
| Logging schema invariance | Per-binding subscriber registration call signature |
| Build/packaging strategy across bindings | (Not duplicated; cite this file.) |
| Recovery non-presence claim | Per-binding surface enumeration that incidentally satisfies it |

## 13. ADR + REQ + AC trace

**ADRs owned/cross-cited by this design doc:**

- ADR-0.6.0-async-surface (Invariants A–D; Path 1 / Path 2)
- ADR-0.6.0-python-api-shape
- ADR-0.6.0-typescript-api-shape
- ADR-0.6.0-cli-scope
- ADR-0.6.0-error-taxonomy (variant table = canonical mapping source)
- ADR-0.6.0-prepared-write-shape (typed write boundary materialization in bindings)
- ADR-0.6.0-typed-write-boundary
- ADR-0.6.0-zerocopy-blob (vector encoding boundary)
- ADR-0.6.0-vector-identity-embedder-owned + ADR-0.6.0-embedder-protocol (cross-binding identity invariant)
- ADR-0.6.0-json-schema-policy (save-time validation behavior)
- ADR-0.6.0-corruption-open-behavior (cross-binding corruption surface)
- ADR-0.6.0-tier1-ci-platforms (per-binding smoke gate)
- ADR-0.6.0-deprecation-policy-0-5-names

**REQs covered:**

- REQ-053 (five-verb SDK surface) — § 1
- REQ-054 (recovery CLI-only) — § 10
- REQ-037 (SDK unreachability of recovery) — § 10
- REQ-056 (typed errors) — § 3
- REQ-020a (close releases lock) — § 7
- REQ-031d (refuse-on-corruption) — § 11 + § 3
- REQ-042 (open reports migration progress) — § 11
- REQ-046a/b (deprecation discipline at the bindings facade) — delegated to `interfaces/{python,ts,cli}.md` for messaging shape; this file commits no deprecation contract
- REQ-047 (embedder version-skew detection at resolution) — § 5 (via ADR-0.6.0-vector-identity-embedder-owned + the `fathomdb-embedder-api` semver-pinned trait crate per architecture.md § 1; skew detected at link/resolution time, not by bindings at runtime)

**ACs cited:**

- AC-001 (lifecycle phase tag enum) — § 8
- AC-002 (no log files without subscriber) — § 8
- AC-022a (close releases lock) — § 7
- AC-035a/b/c/d (corruption refuse + shape + lock release + recovery CLI-only) — § 7, § 10, § 11
- AC-056 (registry-installed wheel smoke gate) — § 9
- AC-057a (five-verb SDK surface set) — § 1, § 10
- AC-060a (typed errors per variant) — § 3
- AC-060b (JSON-Schema save-time cadence) — § 4

## 14. Invariants summary (no speculative knobs)

The bindings layer is normative on:

1. Surface-set parity across SDK bindings (five verbs; no recovery on SDK).
2. Error-mapping *protocol* (one class per variant; typed attrs; single rooted hierarchy; no string-pattern dispatch). The matrix itself lives in `design/errors.md`.
3. Dispatch model and Invariant A–D no-escape-hatch posture.
4. Typed write boundary across bindings.
5. JSON-Schema save-time validation cadence (no binding-side pre-validation).
6. Cross-binding embedder identity.
7. Engine-config knob symmetry across SDK bindings; binding-runtime mechanics
   like the TS dispatch pool are not canonical engine knobs.
8. Lock contract uniformity (hybrid: sidecar flock + SQLite EXCLUSIVE writer per ADR #30).
9. Logging engine-event payload wire-stability (host-native fields derived by adapter).
10. Recovery non-presence on SDK surface.

The bindings layer is *non-normative* on (delegated to per-language `interfaces/*.md`): exact symbol names, parameter spellings, idiomatic casing, deprecation messaging, per-language documentation tone, hierarchy diagram presentation, ergonomic shortcuts that do not change semantics.

This file commits no knobs that are not already committed by an upstream ADR. It does not introduce a new `BindingsConfig` struct, a new dispatch model, or a new error class.

## 15. Followups out of scope here

- Subprocess bridge wire format (FU-WIRE15; ADR-0.6.0-subprocess-bridge-deferral; revisit 0.8.0).
- Embedder cancellation semantics (cited under ADR-0.6.0-async-surface followups).
- Asyncio-first Python API (not 0.6.0; sync is settled).
- Streaming result cursors across the FFI boundary (rejected for 0.6.0; results are owned/materialized per § 4).

## 16. Critic + lock protocol

Per `plan.md` Phase 3 step 4, this draft is critic-passed by `architecture-inspector` (cross-cutting design with strong ADR coupling). Findings applied; HITL gate before status flip to `locked`.
