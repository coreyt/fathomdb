---
title: Findings — Option 2
date: 2026-05-01
target_release: 0.6.0
desc: Outward-facing contract clarity, cross-SDK inconsistency, SDK-vs-CLI boundary, missing precision, ownership ambiguity
status: living
---

# Findings

Each finding lists severity, affected files, a short explanation, the
recommended canonical owner, and a minimal doc fix. Findings focus on
outward-facing contract clarity and ownership precision per the
public-surface-first method.

---

## F-1: All three per-language interface docs are stubs

- **Severity.** high.
- **Affected files.** `interfaces/rust.md`, `interfaces/python.md`,
  `interfaces/typescript.md` (all status: `not-started`).
- **Explanation.** `design/bindings.md`, `design/errors.md`, and
  `requirements.md` all delegate per-language symbol spelling,
  exception class names, attribute spellings, and the realised verb
  for bounded-completion ("drain") to these files. The cross-SDK
  parity claim (REQ-053; AC-057a) and the typed-attribute error
  contract (AC-060a) are unverifiable as written until the per-binding
  surfaces are committed. The CLI surface is comparatively well-
  specified (`interfaces/cli.md`), which makes the asymmetry stark.
- **Recommended canonical owner.** `interfaces/rust.md`,
  `interfaces/python.md`, `interfaces/typescript.md` per their own
  front matter ("draft in Phase 3e (architect agent delegates after
  `architecture.md`)").
- **Minimal doc fix.** Land the Phase 3e drafts. Each per-language
  file needs: the five-verb signature in idiomatic casing, the
  per-variant exception class names mapped to `design/errors.md`'s
  module taxonomy, the subscriber registration call signature, and the
  drain verb name (per IF-026).

## F-2: Wire format file is a stub with no version-sentinel scheme

- **Severity.** medium.
- **Affected files.** `interfaces/wire.md` (status: `not-started`);
  `architecture.md` § 5 (de-facto layout source).
- **Explanation.** `interfaces/wire.md` itself proposes that "If no
  IPC + fresh-db-only with no compat reader, may reduce to: file
  layout references + version sentinel scheme." Neither the file-
  layout reference set nor the version sentinel scheme is committed
  anywhere in the read set. REQ-043 (hard-error on 0.5.x-shaped DB at
  POST naming the schema version seen) implies a sentinel
  must exist; AC-047 confirms it is read at open. There is no doc that
  declares its location or shape.
- **Recommended canonical owner.** `interfaces/wire.md` (file-layout
  references + sentinel scheme); `design/migrations.md` (sentinel
  semantics during the open path).
- **Minimal doc fix.** Promote `interfaces/wire.md` from `not-started`
  to `draft`; cite `architecture.md` § 5 for the layout list and add
  a one-section "Schema version sentinel" pointing at
  `PRAGMA user_version` (already cited in architecture.md § 5).

## F-3: `design/migrations.md` is a one-paragraph stub but

acceptance.md commits its event payload

- **Severity.** high.
- **Affected files.** `design/migrations.md` (one paragraph);
  `acceptance.md` AC-046b/c (commits `step_id`, `duration_ms`,
  `failed` fields); `design/errors.md` § Module taxonomy
  (`MigrationError` row); `design/bindings.md` § 11
  (`MigrationError` failure-mode entry).
- **Explanation.** The migration per-step event (IF-012) is the only
  AC-pinned subscriber-routed payload whose owning design doc has not
  enumerated the schema. The doc says it "owns the migration loop ...
  per-step event contract, and the accretion-guard rules" — but the
  contract is inferred from acceptance only. An interface inventory
  cannot point to a canonical source. There is also a naming
  inconsistency: `acceptance.md` AC-046c says `MigrationFailed` while
  `design/errors.md` says `MigrationError`.
- **Recommended canonical owner.** `design/migrations.md` (per
  architecture.md § 2 assignment).
- **Minimal doc fix.** Expand `design/migrations.md` to enumerate the
  per-step event schema (`step_id`, `duration_ms`, `failed`,
  optional `failure_reason` / `error_chain` if any), reconcile
  `MigrationFailed` vs `MigrationError`, and explicitly cite which
  lifecycle phase the events ride under.

## F-4: Drain / bounded-completion verb has no committed name

- **Severity.** high.
- **Affected files.** `requirements.md` REQ-030 ("API verb name owned
  by binding-interface ADRs"); `design/bindings.md` § 1 (five-verb
  parity invariant); `acceptance.md` AC-018, AC-032a/b;
  `interfaces/{python,ts}.md` (stubs).
- **Explanation.** REQ-030 / AC-032a/b commit a typed timeout error
  and a wall-clock tolerance for a verb whose name is not committed
  anywhere in the read set. Worse: `design/bindings.md` § 1 explicitly
  insists on exactly five top-level SDK verbs (`Engine.open`,
  `admin.configure`, `write`, `search`, `close`) with parity across
  bindings — drain must therefore be either subordinate to one of
  those (e.g. `Engine.drain`) or excluded from the SDK surface
  entirely. The corpus does not state which.
- **Recommended canonical owner.** `interfaces/{python,ts}.md` (per-
  language spelling); `design/scheduler.md` (verb semantics; cited
  but not in this option's read set).
- **Minimal doc fix.** Add a short § to `design/bindings.md` § 1 or
  § 12 clarifying that drain is a method on `Engine` (not a top-level
  SDK verb), AND name the method in `interfaces/{python,ts}.md` once
  those files are drafted.

## F-5: Soft-fallback record `branch` field set unspecified

- **Severity.** medium.
- **Affected files.** `requirements.md` REQ-029 ("Field name owned by
  binding-interface ADRs"); `acceptance.md` AC-031;
  `architecture.md` § 4 ("Search { ..., soft_fallback: Option<...> }");
  `interfaces/{python,ts}.md` (stubs).
- **Explanation.** AC-031 asserts `branch == Vector` is one valid
  value. The complete set of branch values, the field name on the
  soft-fallback record (the AC says "branch field" but is the only
  source), and any other fields on the record are not defined. The
  retrieval pipeline (architecture.md § 4) implies at least
  `{Vector, Text}` since hybrid is text + vector — but no doc lists
  the enum.
- **Recommended canonical owner.** `design/retrieval.md` (cited;
  not in this option's read set) for the enum;
  `interfaces/{python,ts}.md` for the per-binding field name.
- **Minimal doc fix.** Add the soft-fallback enum to `design/retrieval.md`
  (or to `design/bindings.md` § 4 if it is cross-binding), and link
  AC-031 to it.

## F-6: `EngineConfig` knob set is non-exhaustive

- **Severity.** medium.
- **Affected files.** `design/engine.md` § EngineConfig ownership
  ("Engine-owned 0.6.0 knobs include runtime controls such as
  `embedder_pool_size` and `scheduler_runtime_threads`");
  `design/bindings.md` § 6 (engine-config symmetry rule).
- **Explanation.** The bindings symmetry rule is "engine-owned knobs
  named in `design/engine.md` must be reachable from every SDK
  binding in idiomatic form." If the named set is "such as ...", the
  symmetry rule has no testable boundary. Tunables explicitly
  named or implied elsewhere include: provenance retention cap
  (REQ-031, P-RETENTION-CAP); slow-statement threshold default 100 ms
  with runtime reconfiguration (REQ-006a, AC-007a/b); per-call
  embedder timeout default 30 s (`design/bindings.md` § 2 Invariant D);
  heartbeat cadence (`design/lifecycle.md` § Slow and heartbeat policy
  "configurable as part of the feedback configuration surface");
  embedder-pool size; scheduler runtime threads. None of these is
  enumerated as a single, complete `EngineConfig` field set.
- **Recommended canonical owner.** `design/engine.md` § EngineConfig
  ownership.
- **Minimal doc fix.** Replace the "include runtime controls such as
  ..." sentence with an enumerated list of named fields, each with a
  type and default; reference each field from the cross-cutting
  symmetry rule in `design/bindings.md` § 6.

## F-7: Doctor JSON shapes other than check-integrity are TBD

- **Severity.** medium.
- **Affected files.** `design/recovery.md` § Machine-readable output
  ("their exact shapes remain owned here as the draft fills in").
- **Explanation.** `safe-export`, `verify-embedder`, `trace`,
  `dump-schema`, `dump-row-counts`, `dump-profile`, and the `recover`
  progress stream + summary all commit to `--json` as normative but
  do not specify the JSON shape. Operators and CI scripts cannot
  parse on these shapes today.
- **Recommended canonical owner.** `design/recovery.md`.
- **Minimal doc fix.** Add a sub-section per verb with at minimum the
  top-level keys + per-record fields; a stub of "exact field set TBD;
  shape contract: single object" is enough to remove ambiguity for
  callers.

## F-8: Variant→class mapping matrix not yet rendered

- **Severity.** high.
- **Affected files.** `design/errors.md` (owns the matrix per
  `architecture.md` § 2); `design/bindings.md` § 3 (commits the
  protocol but defers the matrix); `interfaces/{python,ts,cli}.md`
  (per-language casing only).
- **Explanation.** AC-060a tests that "every variant in the variant
  table of `ADR-0.6.0-error-taxonomy` § Decision maps to a distinct
  typed exception class in Python and a distinct typed error class
  in TypeScript." `design/errors.md` lists the module taxonomy
  table but does not render the variant→class matrix; the per-
  binding files where the class names live are stubs. AC-060a is
  therefore unverifiable today.
- **Recommended canonical owner.** `design/errors.md` (matrix);
  `interfaces/{python,ts}.md` (concrete class names per binding).
- **Minimal doc fix.** Add a "Variant → binding class" table to
  `design/errors.md` enumerating each `EngineError` /
  `EngineOpenError` variant with one column per binding (Python,
  TypeScript). Class names can be placeholders ("`fathomdb.Embedder
  IdentityMismatch`" etc.) pending interface lock.

## F-9: TS error-root plurality left ambiguous

- **Severity.** medium.
- **Affected files.** `design/bindings.md` § 3 ("TS may keep two
  roots"); `interfaces/typescript.md` (stub).
- **Explanation.** Cross-SDK inconsistency: Python flattens
  `EngineOpenError` variants under `fathomdb.EngineError` (single
  root); TypeScript "may keep two roots." This is a real inconsistency
  in the SDK error catch surface — Python authors and TS authors
  must use different `except` / `instanceof` patterns. The choice is
  framed as binding-discretion but is a public contract.
- **Recommended canonical owner.** `design/bindings.md` § 3 (decide
  whether single-root is required) OR `interfaces/typescript.md`
  (commit the chosen shape and the rationale).
- **Minimal doc fix.** Pick one. If TS keeps two roots, document the
  catch-all base type that catches both and require its export.

## F-10: SDK vs CLI boundary "regenerate" terminology

- **Severity.** low.
- **Affected files.** `interfaces/cli.md` § Recover root note;
  `design/recovery.md` § Two-root CLI split;
  `requirements.md` REQ-059;
  `acceptance.md` AC-063c.
- **Explanation.** "Regenerate" is used as a workflow name in three
  places, but "there is no separate `fathomdb regenerate` command in
  0.6.0." Operators reading prose for the first time will look for
  the command. The current resolution is correct ("regenerate"
  = `fathomdb recover --accept-data-loss --rebuild-projections`) but
  the lexical collision is a minor outward-contract clarity risk.
- **Recommended canonical owner.** `design/recovery.md`.
- **Minimal doc fix.** When "regenerate" appears in operator-facing
  prose, follow it with the canonical CLI invocation in parentheses
  on first mention per page.

## F-11: Subscriber registration helper / callback signature unspecified

- **Severity.** medium.
- **Affected files.** `design/bindings.md` § 8;
  `interfaces/{python,ts}.md` (stubs).
- **Explanation.** The protocol commits Python = "binding-provided
  helper that maps tracing events into Python `LogRecord`s" and TS =
  "callback invoked per event." Neither helper name nor callback
  shape is committed. AC-003a/b/c/d depend on a "binding-idiomatic
  logging hook" without naming it.
- **Recommended canonical owner.** `interfaces/{python,ts}.md`.
- **Minimal doc fix.** Name the helper / callback in each per-binding
  interface doc and cite the call from `design/bindings.md` § 8.

## F-12: Counter-snapshot read API + profile-toggle API unspecified

- **Severity.** medium.
- **Affected files.** `design/lifecycle.md` § Counter snapshot +
  § Per-statement profiling; `interfaces/{python,ts}.md` (stubs).
- **Explanation.** AC-004a and AC-005a both reference a "documented
  API" without naming the symbol. The five-verb SDK surface
  (`Engine.open`, `admin.configure`, `write`, `search`, `close`)
  does not include reading counters or toggling profiling. These
  must therefore be methods on `Engine` (not top-level SDK verbs) or
  belong to an instrumentation handle. The corpus does not say
  which.
- **Recommended canonical owner.** `interfaces/{python,ts}.md` for
  the call; `design/lifecycle.md` may need a § naming the handle if
  one is introduced.
- **Minimal doc fix.** State in `design/lifecycle.md` § Counter
  snapshot + § Per-statement profiling that the read / toggle is a
  method on `Engine` (e.g. `Engine.counters()`,
  `Engine.set_profiling(bool)`) and lock the spelling per binding in
  the interface files.

## F-13: `OpenStage` complete enum not enumerated; only the

corruption-emitting subset is named

- **Severity.** low.
- **Affected files.** `design/errors.md` § Engine.open corruption
  table (lists four corruption stages); `acceptance.md` AC-035b
  ("`stage: OpenStage` ... and never `LockAcquisition`").
- **Explanation.** The four corruption stages are exhaustive for the
  corruption surface, but `OpenStage` is the underlying enum and AC-
  035b implies at least one non-corruption value (`LockAcquisition`).
  The full enum membership is not declared anywhere in the read set,
  yet `OpenStage` is a public typed field on `EngineOpenError::Corruption`.
  Bindings cannot exhaustively type-match without it.
- **Recommended canonical owner.** `design/errors.md`.
- **Minimal doc fix.** List the complete `OpenStage` enum in
  `design/errors.md` and mark the four corruption-emitting members
  explicitly.

## F-14: `WriteReceipt` field set beyond `cursor` is unspecified

- **Severity.** low.
- **Affected files.** `architecture.md` § 3 ("Return WriteReceipt {
  cursor: c_w, ... }"); `design/engine.md` § Cursor contract.
- **Explanation.** The "..." is the only specification for additional
  fields. REQ-055 names only `cursor`. Bindings cannot construct the
  idiomatic Python / TS materialization of the receipt without the
  field set.
- **Recommended canonical owner.** `design/engine.md`.
- **Minimal doc fix.** Either commit "WriteReceipt has exactly one
  public field, `cursor`" or enumerate additional fields with types.

## F-15: SDK-vs-CLI boundary is well-drawn but `doctor`'s

non-presence on the SDK is asserted only by parity invariant

- **Severity.** low.
- **Affected files.** `design/bindings.md` § 1 + § 10;
  `acceptance.md` AC-035d, AC-041, AC-057a.
- **Explanation.** The SDK five-verb parity claim and the recovery-
  name exclusion list (`{recover, restore, repair, fix, rebuild,
  doctor}`) bound the SDK from above and from below. This is
  unusually clean. The minor risk is that the parity claim is
  enforced by name match — adding a synonym ("inspect", "diagnose")
  would not be caught by AC-041 / AC-057a, only by AC-057a's positive
  set-equality assertion. Worth noting but not a fix.
- **Recommended canonical owner.** `design/bindings.md` § 1.
- **Minimal doc fix.** Add a sentence: "AC-057a's exact set-equality
  is the load-bearing test. The exclusion list in § 10 is a
  prophylactic; positive parity is the actual guarantee."

## F-16: Op-store `design/op-store.md` is outside this option's

read set but its public surface is committed by acceptance

- **Severity.** low (process-level, not contract-level).
- **Affected files.** `design/op-store.md` (not in read set);
  `acceptance.md` AC-061a/b/c, AC-062;
  `requirements.md` REQ-057, REQ-058.
- **Explanation.** From the public-surface-first vantage, op-store is
  an `IF-021`-shaped contract whose owning subsystem doc was not
  required reading. The acceptance criteria pin the columns and the
  collection-kind enum, so the public surface is well-defined; this
  is more a note that the inventory was assembled without consulting
  the canonical owning doc. No public-surface gap was found via the
  AC-pinned shape; future inventory passes should still validate
  against `design/op-store.md` directly.
- **Recommended canonical owner.** `design/op-store.md`.
- **Minimal doc fix.** None at the public-surface level. Cross-check
  during a follow-on pass.

## F-17: Logging engine-event payload key is named but not

schema-rendered

- **Severity.** medium.
- **Affected files.** `design/bindings.md` § 8 ("Engine event fields
  appear under a stable `fathomdb` payload key in the host record");
  `design/lifecycle.md` (does not name the key).
- **Explanation.** `design/bindings.md` § 8 commits the wire-stable
  key but does not enumerate its sub-keys; `design/lifecycle.md`
  enumerates fields by category (counters, profile, stress) but does
  not connect them to the `fathomdb` envelope. A binding adapter
  cannot derive the on-the-wire shape from either doc alone.
- **Recommended canonical owner.** `design/bindings.md` § 8 (envelope)
  - `design/lifecycle.md` (field set per category).
- **Minimal doc fix.** Add to `design/bindings.md` § 8 a small
  sub-section "fathomdb payload structure" referencing
  `design/lifecycle.md`'s phase / counter / profile / stress
  sections, and naming the sub-keys (e.g. `fathomdb.phase`,
  `fathomdb.source`, `fathomdb.category`).

## F-18: Migration accretion-guard owner ambiguous

- **Severity.** low.
- **Affected files.** `design/migrations.md` (one paragraph claims
  ownership: "accretion-guard rules cited by REQ-042 / REQ-045");
  `architecture.md` § 2 (`migrations` row);
  `acceptance.md` AC-049 (linter described).
- **Explanation.** The accretion-guard linter is a release-time gate
  but is owned conceptually by `design/migrations.md`. AC-049
  describes a CI linter — that is closer to `design/release.md`
  (cited; not in read set). Two plausible owners; not contradictory
  but ambiguous.
- **Recommended canonical owner.** `design/migrations.md` (rule
  semantics) + `design/release.md` (linter implementation gate).
- **Minimal doc fix.** Both docs cite each other in one sentence
  each, distinguishing rule from gate.

## F-19: `interfaces/cli.md` defers exit-code enumeration to "classes"

without enumeration

- **Severity.** low.
- **Affected files.** `interfaces/cli.md` § Doctor verbs (table cites
  `doctor-check-*` = 0/65/70/71 etc. but does not name what each code
  means).
- **Explanation.** The exit-code classes are committed but the
  semantic mapping (which condition produces 65, 66, 70, 71, 64) is
  not in the read set. CI scripts dispatching on exit code today
  cannot do so from the doc.
- **Recommended canonical owner.** `interfaces/cli.md`.
- **Minimal doc fix.** Add a small enum table mapping each numeric
  exit code to a stable name (e.g. 0 = clean, 64 = lossy-recovered,
  65 = findings, 66 = export-failed, 70 = unrecoverable, 71 =
  lock-held), aligned with the variant→class matrix in
  `design/errors.md`.
