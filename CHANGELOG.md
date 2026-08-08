# Changelog

Release notes for the FathomDB engine, Python SDK, and TypeScript SDK.
Cuts follow the version tagged on `0.6.0-rewrite`. Each released section
MUST list every removed public symbol under a `### Removed` heading;
the removal-detect linter (`scripts/security/check-removal-changelog.sh`,
AC-050c) gates merges against this invariant.

## [Unreleased]

### 0.8.22 release candidate — publication held

This local candidate is not tagged or published. The tag, ordered platform
publication, registry smokes, and `latest` promotion remain explicitly held.
Move this section to a dated release heading only after those gates complete.

Schema version **25 → 26**.

### Added

- Native Python wheels and npm binaries for macOS x64/ARM64 and Windows x64,
  alongside the supported Linux glibc x64/ARM64 targets.
- Actual-runner native build and registry-smoke coverage for the supported
  five-target matrix. Linux musl and Windows ARM/32-bit remain unsupported.
- Projection-runtime status reads in Rust, Python, and TypeScript distinguish
  durable declarations from this engine session's usable dense runtime.

### Changed

- The npm main package is released under `next` before promotion to `latest`;
  promotion requires every platform registry smoke and co-tagging check.
- Updated the SQLite stack to `rusqlite` 0.40 and `sqlite-vec` 0.1.9.
- Ranked retrieval APIs default to 10 results and reject requested limits outside
  1 through 100, while preserving the vector candidate fanout needed for deeper
  K-ladder evaluation.
- Canonical FTS hydration uses additive `write_cursor` join indexes for nodes
  and edges.

### Removed

None.

## 0.8.21 — 2026-08-05

Schema version **24 → 25**.

### Added

- Nested-source projections can declare a canonical-body source path, including
  projected full-text search over the declared source.
- Python and TypeScript expose attribute filters on search, and explanations
  report candidates dropped by node-scoped attribute filtering.
- CI now separates fast and heavy verifier tiers, runs early shell and governance
  gates independently, and retains redacted verifier spill logs for diagnosis.

### Changed

- Release dry-run dispatches rehearse the selected commit before a release tag
  exists; tag pushes and recovery publishes remain tag-bound.

### Removed

None.

## 0.8.20 — 2026-07-30

**The first real publish since 0.8.9, and a BREAKING one.** The 0.8.10–0.8.19 work
landed on `main` as a label-only line (manifests held at `0.8.9`; no tag, no
artifacts), so **no `0.8.10`–`0.8.19` sections exist** and this one section covers the
whole `0.8.9 → 0.8.20` span. Schema version **15 → 24**.

### Upgrading from 0.8.9 — read this first

Five things break a working 0.8.9 integration. In descending order of damage:

1. **Every edge row in an existing database is DESTROYED by the upgrade, with no
   preserving migration path.** Schema migration **step 23** (TC-33) recreates
   `canonical_edges` with INTEGER temporal columns and **does not migrate the data**:
   it runs `DELETE FROM search_index_edges` and then `DROP TABLE canonical_edges` —
   dropping the canonical **source-of-truth** table itself, not merely a derived
   index. The engine's own migration source states it verbatim: *"NO DATA MIGRATION
   (HITL 2026-07-21): existing edge rows do NOT survive and no stored ISO-8601 value
   is converted."* This is a deliberate ruling, not an oversight — the old columns
   held unvalidated ISO-8601 text that the engine had no safe basis to reinterpret.

   Nodes, node bodies, node FTS rows and node vectors are **unaffected**; only edges
   are lost. Step 23 is the **only** canonical-data-destroying step in the whole
   15 → 24 span.

   **Before upgrading: back up the database file, and be prepared to re-ingest every
   edge.** Migration runs automatically at `Engine.open` and is one-way.

2. **`SearchHit.id` is no longer an integer.** Any code doing `int(hit.id)` or
   arithmetic on `hit.id` now fails at runtime. See the first *Changed — BREAKING*
   entry for the exact migration.

3. **`source_id` is mandatory on every canonical write.** A write item without one is
   refused (a **compile error** on the Rust facade).

4. **Edge `t_valid` / `t_invalid` are INTEGER epoch seconds, not ISO-8601 strings**
   — on the governed SDK write surface in all three languages.

5. **Rust facade consumers only:** four read verbs gained a **required**
   `view: &ReadView` parameter, and `EngineError` / `PreparedWrite` gained variants
   and fields (neither is `#[non_exhaustive]`, so exhaustive matches and struct
   literals stop compiling).

Schema **15 → 24** is one-way: an engine older than 0.8.20 refuses to open a migrated
database.

### Changed — BREAKING

- **`SearchHit.id` is now a typed `IdSpace`, not an integer (0.8.19, OPP-12 C-2 /
  TC-8).** It used to be the row's positional `write_cursor`; it is now a two-field
  value carrying the **id space** and the **bare id**. The positional cursor is no
  longer surfaced to Python or TypeScript at all. **This is the largest
  consumer-visible break in this release.**

  | | before (0.8.9) | after (0.8.20) |
  | --- | --- | --- |
  | Rust | `pub id: u64` | `pub id: IdSpace` (`{ space: IdSpaceKind, value: String }`); the positional cursor moved to a separate `write_cursor: u64` field |
  | Python | `id: int` | `id: IdSpace` — read `.space` (`"logical"` / `"content"` / `"passage"`) and `.value` |
  | TypeScript | `id: number` | `id: IdSpace` — `{ space, value }`, `space` typed as the same three strings |

  *Why:* the old id was reassigned on **every** re-projection and re-ingest, so it
  was never a durable identity — and nothing else on the hit was either. `IdSpace` is
  cross-session stable and **total** over the three id spaces: governed rows are
  `logical` (`l:`), doc-seeded rows `content` (`h:`), synthetic passages `passage`
  (`p:`). Only `logical` ids are lifecycle-addressable, which is precisely what the
  new `transition` / `purge` verbs consume.

  *Migration, concretely:*

  - **Python.** `int(sh.id)` raises `TypeError` on **every** hit — there is no
    coercion. Use `sh.id.value` for the bare id and `sh.id.space` for the space. The
    old prefixed string form is `f"{prefix}{sh.id.value}"` with `l:` / `h:` / `p:`.
  - **TypeScript.** `hit.id` is an object, so `hit.id.toFixed(…)`, `===` against a
    number, and use as an index key all break. Use `hit.id.value` / `hit.id.space`.
  - **Rust.** `IdSpace::to_prefixed()` (and its `Display`) reproduces the pre-swap
    `stable_id` string **byte-for-byte**, and `IdSpace::parse()` round-trips it — so
    any keying that was already done on the prefixed string is a true no-op. Code
    that used the integer directly must move to the new `SearchHit::write_cursor`
    field, which keeps the old value and the old (unstable) semantics.
  - **If you kept a map from hit id to source document**, rebuild it from
    `SearchHit.source_id`, which 0.8.20 populates on **every** hit path (TC-31). The
    positional cursor is no longer recoverable from a hit in Python or TypeScript, so
    a cursor-keyed map is not merely broken but unrecoverable.

- **Edge `t_valid` / `t_invalid` are INTEGER epoch seconds, not ISO-8601 strings
  (0.8.20, TC-33).** On the governed SDK write surface an edge item's
  `tValid`/`t_valid` and `tInvalid`/`t_invalid` were untyped ISO-8601 **text** with no
  format validation at all; they are now integer epoch seconds (UTC), validated
  identically to the node `validFrom`/`validUntil` window. `None`/absent still means
  "still valid" — that semantic is unchanged. On the Rust facade
  `PreparedWrite::Edge.t_valid` / `.t_invalid` changed from `Option<String>` to
  `Option<i64>`.

  *Scope:* ISO-8601 survives **only** on the BYO-LLM extractor wire, where the engine
  normalises it with hard rejection. Storage now uses the same representation as the
  write surface, and `typeof` CHECK constraints make a junk value **unstorable**
  rather than silently accepted.

  *This is the change that costs your edge data on upgrade* — see step 23 in
  *Upgrading from 0.8.9* above.

- **Rust facade: four read verbs gained a required `view: &ReadView` parameter
  (0.8.20, R-20-RV).** `Engine::read_get`, `Engine::read_get_many`,
  `Engine::read_list` and `Engine::graph_neighbors` no longer compile against a
  0.8.9 call site. **Pass `&ReadView::default()` to get exactly the shipped
  behaviour** — every `ReadView` field is a relaxation and every default is the
  strict view. `Engine::search` is unchanged; the view-taking variants are the
  net-new `search_view` / `search_reranked_view` / `search_text_only_view`.

  **The Python and TypeScript SDKs are NOT source-broken by this**: there `view` is
  an optional keyword/parameter (`view: ReadView | None = None`), so omitting it is
  the 0.8.9 behaviour exactly.

- **Rust facade: `EngineError` and `PreparedWrite` grew, and neither is
  `#[non_exhaustive]`.** `EngineError` gained `Consolidator`, `ErasureIncomplete`,
  `IllegalTransition`, `NotLifecycleAddressable`, `ProjectionDestructive` and
  `VectorEquivalenceMismatch`, so an exhaustive `match` stops compiling — add a
  wildcard arm. `PreparedWrite::Node` gained `state`, `reason`, `valid_from` and
  `valid_until`, and `PreparedWrite::Edge` changed its temporal field types, so
  struct-literal construction and exhaustive destructuring must be updated. Python
  and TypeScript are unaffected (their write items are loose maps and their errors
  are a class hierarchy, so both additions are additive there).

- **An `fts` or `vector` sub-object without the `searchable` role is now
  REJECTED, not accepted (0.8.20, R-20-SV).** `configure_projections` refuses a
  `ProjectionSpec` that sets `fts` or `vector` while `roles` does not contain
  `searchable`.

  | | before | after |
  | --- | --- | --- |
  | Rust | accepted; stored and round-tripped verbatim | `EngineError::WriteValidation` |
  | Python | accepted | `WriteValidationError`, message `"write validation error"` |
  | TypeScript | accepted | `WriteValidationError` (`FDB_WRITE_VALIDATION`), `data: null` |

  *Why:* `searchable→FTS` and `searchable→vector` are tier labels, not roles —
  the sub-objects **select** a sub-target of `searchable` and do not **confer**
  one. Both engine build predicates are conjunctions with the role, so without it
  the declaration builds no property-FTS, enrols no node kind and embeds nothing.
  It was a config that could not do anything, accepted silently. Fail-fast
  matches the hard-reject philosophy applied everywhere else in the projection
  registry, and additive strictness is safe pre-1.0.

  *Scope:* the refusal is keyed on the **absence of `searchable`** and nothing
  else — `filterable` and `rankable` neither supply nor substitute for it. A
  rejected request is a **total no-op**: validation runs before any write, so one
  invalid spec anywhere in the list aborts the whole call and valid siblings are
  not registered. `read.projections` is a pure read and is **unaffected**.

  *The cost, stated plainly:* `WriteValidation` carries no payload, so when you
  pass a LIST of specs **the error does not name which spec was invalid**.
  Validate before calling, or apply specs one at a time while migrating.

  *Migration for existing databases:* a projection declared in this shape while
  the engine accepted it is still stored, and `read.projections` still reports it
  verbatim — but that output can no longer be fed straight back into
  `configure_projections`. Re-applying it raises; re-declaring only its valid half
  raises `ProjectionDestructiveError`, because that removes the stored
  sub-object. Two remedies: **add the `searchable` role** (a non-destructive
  change, accepted), or **name the projection in `drop`**.

- **An unsatisfiable node validity window is now `WriteValidationError`, not
  `InvalidArgumentError` (0.8.20, R-20-VC decision #18).** Writing a node with
  both `valid_from` and `valid_until` present and `valid_from >= valid_until`
  still fails the whole batch — but with a different error class, and **without
  the offending bounds**.

  | | before | after |
  | --- | --- | --- |
  | Rust | `EngineError::InvalidArgument { msg }` | `EngineError::WriteValidation` |
  | Python | `InvalidArgumentError`, message naming both bounds | `WriteValidationError`, message `"write validation error"` |
  | TypeScript | `InvalidArgumentError` (`FDB_INVALID_ARGUMENT`), message naming both bounds | `WriteValidationError` (`FDB_WRITE_VALIDATION`), `data: null` |

  *Why:* the engine's `validate_write` rejected across **two** error families
  from one function, so the same call raised `InvalidArgumentError` for an
  inverted window but `WriteValidationError` for a non-integer bound.
  `dev/design/errors.md` defines `WriteValidationError` as "malformed typed write
  shape", which is exactly that boundary, so the code now matches the taxonomy.

  *The cost, stated plainly:* `WriteValidation` carries no payload, so **the
  offending `valid_from` / `valid_until` values are no longer recoverable from
  the error**. If you parsed them out of the message, validate the pair before
  calling instead.

  *Unaffected:* `InvalidArgumentError` / `FDB_INVALID_ARGUMENT` is unchanged
  everywhere else — traversal `depth`, projection **name** and `drop` **name**
  rejections, `ReadView` misuse. (The projection **shape** reject added by
  R-20-SV above is `WriteValidationError`; the name rejections keep
  `InvalidArgumentError` because their message names the offending value.)
  A one-sided window is still never refused. **An unrenderable
  edge `t_valid` / `t_invalid` epoch also still raises `InvalidArgumentError`
  naming the offending field**, deliberately: that message is the TC-33 contract
  that stops a `null`-rendering epoch silently resurrecting an invalidated edge.

- **Multi-document `ingest_with_extractor` batches now require per-entity
  attribution (0.8.20, R-20-E2).** Provenance is taken from the caller's
  `ExtractDocument.source_doc_id`; the model's echo of that field is accepted
  only as a *selector* among the ids the caller supplied in the same batch, and
  never as a value.

  - **Single-document batches are unaffected.** Attribution is unambiguous, so
    the echo is ignored and the caller's id is used even if the model omits it.
  - **Multi-document batches** must carry a `source_doc_id` on **every entity
    and every edge**, naming one of that batch's documents. An absent or
    unrecognised value now fails the whole ingest with `ExtractorError`
    (Rust: `EngineError::Extractor`).

  *Why it fails instead of defaulting:* with several documents in flight the
  engine cannot know which one an entity came from, and every fallback would
  silently file the row under the wrong document — leaving it behind when that
  document is erased. That is the un-erasable-row defect this release closes, so
  the engine refuses to guess.

  *Migration:* have your extractor set `source_doc_id` on each element it
  returns. **If it cannot attribute its output, submit single-document
  batches** — one `ExtractDocument` per call. See
  [Erasure](https://fathomdb.dev/operations/erasure/).

- **`source_id` (provenance) is now MANDATORY on every canonical write
  (0.8.20, R-20-E3).** On the Rust surface `PreparedWrite::Node.source_id` and
  `PreparedWrite::Edge.source_id` changed from `Option<String>` to the new
  `SourceId` newtype, so an un-provenanced write is now a **compile error** for
  facade consumers rather than a runtime rejection. In Python and TypeScript a
  write item lacking `source_id` / `sourceId` raises `WriteValidationError`.
  `SourceId::new` additionally rejects empty/whitespace-only ids and the whole
  reserved `_`-prefixed namespace.

  *Why, and why no deprecation window:* `excise_source` addresses rows **by**
  `source_id`. A row written without one was reachable by no erasure call at
  all — permanently un-erasable. A compatibility shim would have re-opened
  exactly the hole this closes, so 0.8.20 takes the break deliberately as the
  approved coordinated breaking-pair release.

  *Migration:* add a `source_id` to every write. Use an opaque document or
  tenant id — **not** personal data; see
  [Erasure](https://fathomdb.dev/operations/erasure/). Schema migration step 21
  back-fills `source_id = '_legacy:pre-0.8.20'` onto pre-existing rows that had
  NULL provenance **and** no `logical_id`, so historical anonymous rows become
  erasable; governed rows keep NULL provenance and stay `purge`-addressable.
  Audit any database with `fathomdb doctor orphan-provenance`.

- **`ExciseReport.projections_invalidated` / `RebuildReport.rows_invalidated`
  now report higher counts (0.8.20, R-20-E1).** Both figures previously omitted
  `search_index_v2` deletions, because the erasure and rebuild paths carried
  hand-maintained projection lists that had drifted from the actual schema. They
  are now registry-driven and total. The counts were under-reporting real work;
  code that asserts an exact value will need re-baselining.

  *This was a data-leak defect, not a cosmetic one:* `search_index_v2` stores
  the body, so before this fix an excised body survived erasure in that table.
  It never surfaced in search results (both read paths gate on
  `canonical_nodes`), which is precisely why it went unnoticed.

- **Retention `cap` semantics changed (0.8.20, R-20-E7/D-A).** The
  `enforce_provenance_retention` cap now bounds the **sweepable** rows in
  `operational_mutations`, not every row in the table: the erasure-audit
  collections (`excise_source_audit`, `excise_record_audit`) and the engine's
  pending-redaction intent queue (`erasure_pending_redaction`) are exempt from
  the sweep. A database with audit rows will therefore retain more total
  op-store rows than the configured cap. An operator who sized `cap` against a
  physical row count should re-check that assumption.

  *Why:* accountability — demonstrating *that* an erasure occurred — is a
  distinct obligation from erasure itself, and the previous behaviour let a
  retention cap silently destroy the proof. The pending-redaction queue is
  exempt for a second reason: it records an *undischarged* erasure obligation,
  so sweeping it under cap pressure would silently convert a pending redaction
  into a reported success.

- **Schema version 15 → 24.** 0.8.9 shipped `SCHEMA_VERSION = 15`; 0.8.20 ships
  **24**. Nine migration steps (16–24) run automatically and in order at
  `Engine.open`. The bump is one-way: an engine older than 0.8.20 refuses to open a
  migrated database.

  **Step 23 destroys all edge data** — see *Upgrading from 0.8.9* above. It is the
  only step in the span that removes canonical rows.

  **Step 21** back-fills legacy NULL provenance (see the `source_id` entry above).
  Its back-fill predicate is **asymmetric between the two canonical tables**,
  deliberately. On `canonical_nodes` it is
  `source_id IS NULL AND logical_id IS NULL`; on `canonical_edges` it is
  `source_id IS NULL` alone. The `logical_id` gate exists so that a governed
  row — which `purge` can already reach by `logical_id` — is not made
  collateral of an `excise_source('_legacy:pre-0.8.20')` call. That rationale
  holds for nodes and **not** for edges: `purge` resolves its target only
  through `canonical_nodes` and then erases edges by *endpoint*, never by edge
  `logical_id`. Gating edges on `logical_id` would therefore have left legacy
  edges reachable by no erasure verb at all.

  *Ordering note:* steps run in id order, so on an in-place 0.8.9 upgrade the
  **edge** half of the step-21 back-fill is subsequently discarded by step 23's
  `DROP TABLE canonical_edges`. The node half is unaffected.

### Added

- **The projection registry — `configure_projections` / `read.projections`
  (0.8.20, R-20-PR / OPP-12 C-1).** A declarative way to state which attributes get
  which index treatment — there was no such mechanism at 0.8.9.
  Available in Python (`Engine.configure_projections`, `read.projections`),
  TypeScript (`configureProjections`, `read.projections`) and the Rust facade.

  - A `ProjectionSpec` is `{ name, roles, fts?, fts_tokenizer?, vector?,
    vector_embedder? }`. `roles` is a **set** drawn from the three
    `ProjectionRole` members — `filterable`, `rankable`, `searchable`.
  - `searchable→FTS` and `searchable→vector` are **tier labels, not roles**: the
    `fts` / `vector` sub-objects *select* a sub-target of `searchable`, they do not
    *confer* it. (Declaring one without the `searchable` role is refused — see
    *Changed — BREAKING*.)
  - `configure_projections` returns a `ProjectionDelta`
    (`built` / `dropped` / `deferred` / `unchanged` / `vector_unsupported_kinds`).
    Re-registering an identical set is idempotent and returns `unchanged=True` with
    empty lists.
  - A **destructive** change to a live projection — removing a role, swapping a
    tokenizer or embedder — is refused with the new `ProjectionDestructiveError`
    unless the caller also names it in `drop`, so an index is never silently
    rebuilt underneath a running application.
  - `read.projections` is **net-new in this release**; it did not exist at 0.8.9.
    Its output re-applies to `configure_projections` as a no-op.

- **`transition` and `purge` — governed record-lifecycle verbs (0.8.19,
  OPP-12 Phase-1).** Net-new in Python, TypeScript and the Rust facade. They have
  never appeared in a published section: 0.8.19 was a label-only close.

  - `transition(logical_id, to_state, reason=None)` moves a governed node between
    **existence** states against an engine-enforced legal-transition table:
    promote `pending`→`active`, reject `pending`→`deleted`, soft-delete
    `active`→`deleted`, undelete `deleted`→`active`. Promote/undelete CLEAR
    `reason`; reject/soft-delete SET it. `reason` is advisory and never
    engine-interpreted.
  - `purge(logical_id)` irreversibly hard-erases a governed node across every
    row-owned target — all versions, FTS and vector shadows, and touching edges by
    cascade. **DELETED-FIRST**: legal only from `deleted`. It is idempotent, and it
    is a **separate verb** from `transition` (and not a recovery-denylist name; the
    REQ-054 five-name denylist is unchanged).
  - Both key on the **bare `logical_id`** and address the `l:` (Logical) id space
    **only**. A non-`l:` id raises the new `NotLifecycleAddressableError` (carrying
    `id_space`); an illegal move raises the new `IllegalTransitionError` (carrying
    `from_state` / `to_state` / `legal`). Content (`h:`) and passage (`p:`) hits are
    total-but-not-addressable by design.
  - The create-time existence state is authorable on a node write item as
    `state: "pending" | "active"` with an advisory `reason`. `active` is the
    back-compat default; `deleted` / `purged` are unrepresentable at create time.

- **`ReadView` — an explicit, opt-in relaxation of the default read predicates
  (0.8.20, R-20-RV / R-20-NV).** `ReadView` carries `include_superseded`,
  `include_inactive`, `include_out_of_window` and `valid_as_of`. **Every field is a
  relaxation and every default is the strict view**, so `ReadView()` — and omitting
  `view=` entirely — reproduces the shipped 0.8.9 read behaviour exactly. Accepted
  by `read.get`, `read.get_many`, `read.list`, `read.crossed_boundary_since`,
  `graph.neighbors`, `Engine.search` and `Engine.search_text_only`.

  **In Python and TypeScript the parameter is OPTIONAL, so no existing call site
  breaks.** On the Rust facade it is required on four verbs — see
  *Changed — BREAKING*. There is deliberately no `history_as_of`: the view is
  world-time only.

- **Node validity windows are now authorable, not just queryable (0.8.20, TC-34).**
  A node write item accepts `valid_from` / `valid_until` (`validFrom` / `validUntil`
  in TypeScript) as INTEGER epoch **seconds**, forming a HALF-OPEN
  `[valid_from, valid_until)` world-time window. An omitted bound is unbounded on
  that side, so omitting both — the default — makes the node valid at every instant,
  which is the 0.8.9 behaviour. Read it back with a `ReadView`'s `valid_as_of`, or
  with the new `read.crossed_boundary_since`.

- **`read.crossed_boundary_since(since)` (0.8.20, R-20-NV).** Returns the nodes that
  crossed a validity boundary in `(since, as_of]` as `BoundaryCrossing` records
  (`node`, `became_valid_at`, `became_invalid_at`). `since` is an epoch-second
  instant and the upper bound is the view's own `valid_as_of` (defaulting to now);
  both are bound parameters, so the answer is deterministic for a fixed pair.

- **`erase_source` / `eraseSource` — provenance-addressed erasure as a
  first-class SDK verb (0.8.20, R-20-E4).** Available in Python, TypeScript and
  the Rust facade. `purge` addresses a governed node by `logical_id`;
  `erase_source` addresses anonymous content — rows with no `logical_id` — by
  its provenance. Previously the only provenance-addressed erasure was the
  operator CLI, so an SDK-only consumer with no `fathomdb` binary on `PATH`
  could not delete anonymous content at all. `excise_source` remains CLI-only
  and is still the only verb that may address the engine's reserved namespace.
  Not a recovery-denylist name; the REQ-054 five-name denylist is unchanged.

- **`fathomdb doctor orphan-provenance` (0.8.20, R-20-E8).** Read-only
  per-`source_id` census over the canonical tables. Reports each provenance
  bucket and, load-bearingly, `unerasable_rows` — rows carrying neither a
  `source_id` nor a `logical_id`, which no erasure verb can reach. Exits `65`
  (`DOCTOR_FOUND_ISSUES`) when that count is non-zero.

- **Erasure completeness at rest (0.8.20, R-20-E5/E6).** Erasure verbs now
  truncate the write-ahead log (the erased bytes were previously still
  `grep`-able in `<db>-wal`, since a `DELETE` appends frames rather than
  rewriting them) and selectively redact the erased ids from the telemetry sink,
  leaving unrelated sink records intact. On a persistent checkpoint `BUSY`, on
  a telemetry sink that could not be redacted, and on a sink that has since
  been **rotated** (a rotated file is not a redacted file — the erased ids may
  still be in the rotated copy), the verb raises the typed `ErasureIncomplete`
  rather than reporting success. An erasure verb must never report success on
  an incomplete erasure. A redaction that cannot be completed in-line is
  recorded as a durable pending-redaction obligation rather than dropped.

- **`excise_collection_record(collection, record_key)` (0.8.20, R-20-E7).**
  Record-level op-store erasure. The op-store previously had no record-level
  delete at all, so a caller holding an erasure obligation over an op-store
  record had no way to discharge it. It **refuses** to address the engine's
  own erasure bookkeeping (the audit collections and the pending-redaction
  queue), which would otherwise let an operator destroy the proof of an
  erasure, or an undischarged obligation, through a normal-looking call.

- **Dense-index readiness on `ProjectionSpec.vector` (0.8.20, R-20-DR).**
  `read.projections` / `read_projections` now reports whether the async
  `searchable→vector` projection has caught up, as engine-set READ METADATA:
  `vector_dense_readiness` (Python), `vectorDenseReadiness` (TypeScript),
  `ProjectionVector::dense_readiness` (Rust). `filterable` and `searchable→FTS`
  are same-transaction and carry no readiness at all.

  - **Exactly two values, `"ready"` and `"embedding"`.** `"pending"` is NOT one
    of them: that token is RESERVED for the orthogonal **admission** axis
    (quarantine/trust, an app judgment), and index-readiness is a different
    dimension — a record can be admissible and still read `"embedding"`.
  - **Derived, never stored.** No schema step; `SCHEMA_VERSION` is unchanged.
    Deriving it from the same outstanding-work predicate `drain` uses is what
    makes `{vector-insert ∧ readiness := ready}` atomic by construction —
    `"ready"` can never be observed with the vector row absent.
  - **Round-trip.** The value is INERT on the way in to
    `configure_projections` / `configureProjections`, so `read.projections`
    output still re-applies as a no-op. The two shapes that could never
    round-trip are refused with the EXISTING `InvalidArgumentError` /
    `FDB_INVALID_ARGUMENT` (no new error type): a readiness supplied with
    `vector=false`, and any spelling outside `{"ready", "embedding"}`.
  - **Additive.** Callers who never read the field see identical behaviour, and
    the change adds no governed commands.

- **`drain` is now the flush-to-readiness barrier (0.8.20, R-20-DR).**
  Declaring a `searchable→vector` projection over a corpus that already holds
  rows now actually **backfills** it: `configure_projections` /
  `configureProjections` enrols the vector kinds and enqueues the deferred embed
  work, and the existing `drain(timeout)` flushes it. There is **no new verb** —
  `drain` carries the `flush_embeddings()` semantics, so this adds **zero**
  governed commands.

  - **Bug fixed:** previously the declaration acknowledged the work
    (`ProjectionDelta.deferred`) and then dropped it — the kind was never
    registered, so `drain` returned immediately and readiness reported
    `"ready"` while **no vector rows existed and nothing would ever create
    them**. Only an operator `rebuild` recovered. The pinned invariant is now
    `drain() ⟹ dense_readiness == "ready"` **and** every vector-eligible row has
    its vector at rest, asserted in Rust, Python and TypeScript.
  - **Ordering-independent.** Write-then-declare and declare-then-write reach
    the same state; a kind first written after the declaration is enrolled on
    the write path, and that write-path enrolment runs the **same** backfill the
    declaration does — so rows of that kind written by an earlier session (for
    instance one opened without an embedder, where the declaration persisted but
    deferred) are picked up rather than left behind a `"ready"` that is not true
    of them.
  - **Scoped to the engine's locked `kind` vocabulary.** The dense arm turns on
    for node kinds in `{email, article, paper, meeting, note, todo, doc}` (plus
    the internal `edge_fact` for edge bodies). Rows of any other `kind` are
    accepted and stay lexically searchable but get no vector, and are not counted
    as outstanding work, so readiness still reaches `"ready"`. This is not an
    error condition: nothing rejects the write. It is **no longer silent**
    either — see `vector_unsupported_kinds` below (R-20-VC).
  - **Idempotent.** Re-applying a satisfied declaration rewinds nothing and
    re-embeds nothing.
  - **Reversible.** `drop`ping the last `searchable→vector` declaration turns
    the dense arm back off: the node kinds that declaration enrolled are
    un-enrolled, so later writes enqueue no embed and `drain` no longer waits on
    them. It **deletes no embedding** — vectors already at rest survive a `drop`,
    exactly as they always have — and re-declaring re-enrols and backfills, so a
    row written while the arm was off is picked up rather than stranded.
    Edge-body vectors are unaffected (`edge_fact` is registered off the presence
    of an edge body, not off the projection registry).
  - **Graceful-absent without a live embedder.** With no embedder there is no
    dense arm, so the declaration persists and defers rather than queueing
    embeds that could only fail; it grafts on when re-applied in a session that
    has one — the same contract as `rankable`.
  - **Graceful-absent stops at the enrolment boundary.** Once a kind IS enrolled
    — some earlier session DID have an embedder — writing that kind without one
    leaves real dense work outstanding. The write is **accepted** and stays
    lexically searchable, but readiness reads `"embedding"` and `drain` reports
    its timeout for the rest of that session, however long you wait. The write is
    **not lost**: no failure is recorded and no terminal is written, so the next
    session opened with an embedder embeds it through the ordinary scheduler — no
    re-apply and no operator `rebuild`. Reporting `"ready"` there would be a torn
    `ready`-without-vector, i.e. the silent miss this work exists to eliminate.
  - **Additive**, no schema step (`SCHEMA_VERSION` unchanged), and no data
    migration: this re-enqueues work inside one live database.

- **`ProjectionDelta.vector_unsupported_kinds` — FathomDB now tells you which of
  your kinds will never get vectors (0.8.20, R-20-VC).**
  `vector_unsupported_kinds` (Python, Rust), `vectorUnsupportedKinds`
  (TypeScript). A list of node **kinds** — not attribute names, unlike the
  `built` / `dropped` / `deferred` lists beside it.

  **Why you want this.** The dense arm covers only the engine's locked `kind`
  vocabulary (`{email, article, paper, meeting, note, todo, doc}`, plus the
  internal `edge_fact`). If your domain uses other kinds — an `entity`, an
  `invoice` — those rows are accepted and stay fully **FTS- and
  lexically searchable**, but they will **never** get an embedding, in this
  session or any future one. Until now that exclusion was silent: the
  declaration came back with the projection's name in `deferred`, which is
  *also* what you see while you are simply waiting on the embedder. There was
  no way to tell "not embedded yet" from "never will be", so the honest reading
  of a deferred projection was "keep waiting" — sometimes forever.

  Now `configure_projections` / `configureProjections` names the kinds:

  - **Sorted, de-duplicated, and empty rather than absent** — read it
    unconditionally.
  - **A state report, not a diff.** It does not affect `unchanged`, so an
    idempotent re-apply comes back `unchanged` **with the report populated** —
    which is also how you refresh it, since it is computed at declaration time
    and will not know about kinds you write afterwards.
  - **The same with or without an embedder.** The vocabulary is static, so a
    session opened with no embedder still gets the permanent answer. Do not
    confuse it with the deferral.
  - **Output-only, and not an error.** `configure_projections` accepts specs,
    never a delta, so `read.projections` output still re-applies as a no-op.
    Nothing is rejected, and readiness still reaches `"ready"` — an un-enrolled
    kind is not outstanding work.

  **Additive**, no new type, no new governed command, no schema step
  (`SCHEMA_VERSION` unchanged). Your options if a kind is reported: use one of
  the mapped kinds, or accept lexical-only retrieval for those rows.

- **New public types.** Rust: `SourceId`, `OrphanProvenanceReport`,
  `OrphanProvenanceSource`; `ExciseReport` is no longer behind the `operator`
  feature (it is `erase_source`'s return type). Python: `EraseReport`.
  TypeScript: `EraseReport`. Rust `DenseReadiness` and the TypeScript
  `DenseReadiness` string union (`"ready" | "embedding"`) are the only net-new
  types from R-20-DR; Python surfaces the value as a plain `str | None` field.

#### Added earlier in the label-only 0.8.10–0.8.19 line

Everything below shipped on `main` between 0.8.9 and 0.8.20 without a tag, so this
is its **first published** appearance.

- **The unified `Filter` grammar (0.8.11, `#17`).** One typed, closed contract —
  `Filter { terms: FilterTerm[] }` with implicit AND — dispatched to two compilation
  backends: the vec0-metadata pre-KNN `WHERE` for `search`, and `json_extract` over
  `canonical_nodes.body` for `read.list`, which gained an additive `filter=`
  parameter. Exactly **five** term variants (`source_type`, `kind`, `created_after`,
  `status`, `json`); no DSL, no unchecked builders, values always bound as
  parameters.

  The shipped `SearchFilter` (G10) and `Predicate` dicts (G4) **still work** — they
  are now sugar that lowers into `Filter`. A `json` term passed to `search` is a
  typed `InvalidFilterError` rather than a silent post-KNN demotion. Python
  `fathomdb.filter`; TypeScript `Filter` / `FilterTerm` plus the
  `filterToSearchFilter` / `searchFilterToFilter` helpers.

- **Consolidation via a BYO-LLM harness (0.8.12, OPP-2).**
  `Engine.consolidate_with_provider(cmd, axes)` / `consolidateWithProvider` runs a
  caller-supplied subprocess speaking the `fathomdb.consolidate.v1` protocol over a
  list of `(subject_logical_id, relation)` clusters and returns a
  `ConsolidateReceipt`. **Opt-in and off by default** — nothing runs unless you pass
  a command. Harness failures surface as the new `ConsolidatorError`.

- **Vector-equivalence self-check + a text-only search path (0.8.18).** At open, the
  engine verifies that the live embedder still agrees with the vectors already at
  rest (a 45-probe two-stage check). On divergence it opens **degraded** rather than
  serving silently-wrong dense results: `Engine.dense_disabled` / `denseDisabled` and
  `dense_disabled_reason` / `denseDisabledReason` report the state,
  `OpenReport` carries the same two fields, every vector-dependent arm then refuses
  at query time with the new `VectorEquivalenceMismatchError`, and
  `vector_equivalence_refusal_count` / `vectorEquivalenceRefusalCount` meters those
  refusals.

  `Engine.search_text_only(query)` / `searchTextOnly` is the companion: FTS-only, it
  never embeds the query and **never** raises `VectorEquivalenceMismatchError`, so it
  stays serviceable on a degraded open.

  *Scope, stated so it is not over-read:* this guards against **accidental** backend
  drift and a corrupt baseline. It is **not** tamper evidence — nothing in a FathomDB
  file is authenticated.

- **Ranking-signal fields on the explain sidecar (0.8.16, F9).** `PerHitExplain`
  gained nullable `importance` and `confidence` (`None` = graceful-absent =
  ranks neutral). They are visible **only** via `search(..., explain=True)`.

  *Deliberately narrow, and stated so nobody reads a ranking change into it:* the
  importance/confidence reweight is **off by default with no production toggle** (the
  only switch is a `#[doc(hidden)]` test seam), so **default ranking is unchanged**.
  The `importance` scalar has **no Python or TypeScript write path** — the only
  writer is the Rust `Engine::write_node_importance`, which addresses a row by the
  positional `write_cursor` that the SDKs no longer surface. In practice a
  Python/TypeScript consumer will read `importance` as `null` on every hit.

- **Opt-in cross-vendor ONNX embedder backend (0.8.16, Rust consumers only).** A new
  **non-default** `onnx-embedder` cargo feature on `fathomdb-embedder` (pulling
  `ort = 2.0.0-rc.10` with `load-dynamic`, plus `tokenizers` and `sha2`) reaches the
  execution providers candle cannot (ROCm / DirectML / OpenVINO / CUDA), selected at
  runtime from `FATHOMDB_EMBED_DEVICE`. `default = []` is unchanged, so the shipped
  wheel, npm package and default crate build are byte-identical without it, and the
  Python/TypeScript SDKs are unaffected.

- **`embed_batch_cls(texts)` (Python).** Batch CLS-pooled embedding, with the CLS
  embedder singleton retrying on transient load failure and surfacing the real
  loader error instead of a generic one.

*Deliberately not announced as an SDK feature:* the 0.8.14 **BM25F / EXP-S**
multi-index retrieval substrate shipped with "no public Py/TS SDK surface this
release", and that is still true at 0.8.20 — `bm25f_search` appears nowhere in
`_fathomdb.pyi` or the TypeScript surface. It is reachable only as
`fathomdb_engine::Engine::bm25f_search` by a direct Rust consumer of the engine
crate; a Python or TypeScript consumer has nothing to call.

### Changed

- **`Engine.open` no longer re-embeds the 45-probe vector-equivalence set on
  every open (0.8.20, R-20-VC TC-68).** The 0.8.18 self-check's verdict is now
  cached against an **embedder fingerprint**, so an open whose fingerprint is
  unchanged performs **zero** probe embeds instead of 45. (Measured on the
  shipped path: 0 embeds with no enrolled vector kind, **90** on the one-time
  baseline-population open — 45 to persist plus 45 to confirm — and **45 on every
  open thereafter**, which is what this removes. The cost never scaled with the
  number of enrolled kinds.) No public type, field, method or error changes.

  The fingerprint covers the embedder identity, **the live pinned `mean_vec`**
  (the Phase-1 check quantizes against it, so a rewritten mean changes the
  verdict), the committed probe fixture, both divergence floors, and the stored
  reference baseline including its vectors. Any change to any of those re-runs
  the full probe. It is stored as one internal marker row; **no schema-version
  bump and no migration** — an existing database simply runs the probe once more
  and then stops.

  Fail-safe behaviour is unchanged: an unreadable or absent cached verdict
  **runs** the probe rather than trusting it, a failing verdict is never cached,
  and a divergence found on a re-run still yields `denseDisabled` /
  `dense_disabled` with the dense arm refusing and the text-only path serving.

  *Scope, stated so it is not over-read:* this self-check guards against
  **accidental** backend drift and a corrupt baseline. It is **not** tamper
  evidence, and `denseDisabled` / `dense_disabled` is not a tamper signal.
  Nothing in a FathomDB file is authenticated: an actor with write access can
  rewrite the stored probe baseline (which defeats the check even when it runs in
  full, exactly as it did before this release), the cached marker, or the corpus
  and its vectors outright. **Stated without softening: the cache does make one of
  those routes cheaper.** Forging the marker needs only a digest anyone can
  compute from the file; re-authoring the baseline needs the other backend's 45
  exact probe embeddings. So a writer of your database file can now skip the
  check more easily than before — though the drift it would have caught is, in the
  steady state, already served without any forgery, because a same-identity swap
  moves nothing in the fingerprint (see the paragraph below). Threat model, the
  concession and the bound: §8.4/§8.5 of
  `dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`.

  *The cost, stated plainly:* a backend that drifts **without changing its
  declared identity** — the same embedder name/revision moved between CPU and
  CUDA, or rebuilt against a new library or driver — is **no longer caught on
  every open**. Nothing in the fingerprint moves, so the cached verdict answers
  and the drifted backend is never re-probed; it is caught only at the next open
  whose fingerprint does change. If you rely on open-time re-verification to
  detect such a swap, force a re-run (for example by recomputing the pinned
  mean). Identity *changes* are unaffected — they still refuse the open outright,
  ahead of any cache. Rationale, the inputs in full, and the mitigations
  considered and rejected:
  `dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`.

### Fixed

- **Bulk governed ingest no longer fails with `EngineError::Storage` while the
  embedding worker is busy (0.8.20, TC-57).** Writing records that carry a
  `logical_id` (the governed upsert/supersession path) against a vector-indexed
  kind could return an opaque storage error part-way through an ingest — reliably
  within the first few hundred rows, and as early as the third write. Nothing was
  corrupted (the failed batch rolled back), but the error was indistinguishable
  from a permanent storage failure and no `busy_timeout` setting could avoid it,
  so there was no host-side workaround short of blind retries.

  *Cause:* the write transaction read the prior version before it wrote the
  tombstone, so it had to upgrade a read lock to a write lock; SQLite refuses that
  upgrade outright, without waiting, when another connection — here the engine's
  own background embedding worker — holds the write lock. The transaction now
  takes the write lock up front, so there is no upgrade to refuse and ordinary
  contention is simply waited out.

  *Impact:* no API, schema or on-disk change; governed ingest under a live
  embedder is also measurably faster end-to-end, because the failed attempts and
  caller-side backoff are gone. Anonymous (no `logical_id`) writes were never
  affected and are unchanged.
- **A `vector` sub-object declared WITHOUT the `searchable` role no longer
  activates the dense arm (0.8.20, R-20-CR).** A projection such as
  `{roles: ["filterable"], vector: true}` is documented as accepted, faithfully
  round-tripped and otherwise **inert** — but the engine keyed the dense arm off
  the stored `vector` sub-object alone and never read the declared roles. So in
  any session with a live embedder that combination enrolled the node kinds,
  backfilled the existing corpus, and made every later write of those kinds
  enqueue an embedding: real embed spend and unexpected vectors at rest for a
  declaration that was supposed to do nothing. The dense arm now requires the
  `searchable` role, exactly as the `fts` sub-object already did.

  **The inverse moved with it.** The engine turns the dense arm back off when the
  LAST `searchable→vector` declaration goes away, and that transition is now
  evaluated on the same role-aware rule. Two cases that previously kept embedding
  now correctly un-enrol: demoting the last `searchable→vector` projection to
  `filterable` + `vector` (drop-then-redeclare), and dropping a
  `searchable→vector` projection while an inert `filterable` + `vector` sibling
  survives in the registry.

  **Upgrading now actually stops the unwanted embedding.** The role rule above
  only closes the doors that *start* the dense arm; a database that already ran
  the old code has the node kinds registered, and the write path decides whether
  to enqueue an embedding from that registration alone. Such a database therefore
  kept spending embed calls after the upgrade. Opening it now **reconciles** the
  stale registration away, so the fix reaches the databases it was raised for
  without the application calling anything. Any `configure_projections` call does
  the same, for a long-lived process that never reopens.

  This is deliberately narrow: it applies only when the projection registry
  exists, declares a `vector` sub-object somewhere, and declares **no**
  `searchable→vector` projection. A database whose dense arm was enrolled by
  other means — including every pre-registry one — is left exactly as it is.

  *No API change and nothing is deleted.* The declaration still persists and
  round-trips verbatim, is still reported in `ProjectionDelta.deferred`, still
  stores its `filterable` value at rest, and vectors already embedded are
  untouched (including on the reconciling open — no embedding is ever removed).
  Adding the `searchable` role turns the dense arm on and backfills the rows
  written while it was off.

- **`drain` no longer hangs to its full timeout on an edge whose `edge_fact` kind
  is not registered (0.8.20, TC-56).** This is a **behaviour change on the shipped,
  already-published `Engine.drain` surface**, closed as a side effect of the Slice-20
  projection work rather than raised as its own fix.

  The pending-work probe behind `drain` omitted the scheduler's join against the
  registered vector kinds on the edge arm, while the scheduler itself carried it. A
  live canonical edge **with a body** whose `edge_fact` kind was not registered
  therefore counted as outstanding work the scheduler would never schedule — it never
  gained a projection terminal, so it was phantom-pending forever and `drain` burned
  its whole timeout and returned `Err(Scheduler)` / `SchedulerError`. The probe and
  the scheduler now share one definition, so they cannot drift apart again.

  *Confirmed pre-existing empirically, not by inspection:* a detached checkout at the
  pre-Slice-20 baseline, using only baseline surfaces, reproduced
  `drain → Err(Scheduler)` after the full timeout with zero Slice-20 code in the
  build. `drain` can still time out for other reasons — see *Known issues*.

- **`erase_source` and `purge` no longer fail on rows carrying a long filterable
  attribute value (0.8.20, TC-76).** An **upstream `sqlite-vec` defect
  ([#274](https://github.com/asg017/sqlite-vec/issues/274), present in 0.1.7 and
  0.1.8, fixed in 0.1.9)** makes a vec0 `DELETE` fail when a TEXT metadata column
  holds 12 or more bytes. FathomDB stores one `attr_*` metadata column per declared
  `filterable` projection, so an attribute value of 12+ UTF-8 bytes made
  `erase_source`, `purge`, edge supersession and the open-path orphan sweep fail with
  `EngineError::Storage` — **leaving the row and its shadowed value at rest**. This
  broke erasure on real data, not in theory.

  0.8.20 ships an engine-side workaround: the `attr_*` values are blanked by a
  metadata-only UPDATE first, which takes vec0's shortening branch and removes the
  shadow row, after which the `DELETE` has no over-length value left to clear. The
  embedding bytes and the other metadata columns survive verbatim, so this is
  erasure-**complete**, not error-suppressing. A corpus with no `filterable`
  projection declared — which is every corpus predating this release — issues exactly
  the statements it always did. A tripwire test asserts the upstream defect still
  exists, so the workaround cannot be silently carried past the upstream fix. See
  *Known issues*.

### Deprecated

Nothing is deprecated in this release. Pre-1.0, 0.8.20 takes its breaks outright
rather than shipping compatibility shims: for `source_id` in particular a shim would
have re-opened the very hole the change closes (a row written without provenance is
reachable by no erasure call at all), so there is deliberately no deprecation window.

### Removed

**None.** No public symbol was removed anywhere across the whole `0.8.9 → 0.8.20`
span — not from the Rust `fathomdb-engine` facade, not from the Python SDK, not from
the TypeScript SDK. Verified two ways: the AC-050c removal-detect linter
(`scripts/security/check_removal_changelog.py --base v0.8.9 --head HEAD`) reports zero
removals, and a direct symbol-set diff of `v0.8.9..HEAD` over
`fathomdb-engine/src/lib.rs`, `src/python/fathomdb/**` and `src/ts/src/**` is
additions-only.

⚠ That linter matches on symbol **names**, so it does not see a **signature** change
on a same-named symbol. Several of those did happen and they are genuinely breaking —
they are documented under *Changed — BREAKING* above, not here.

### Security

- **An excised body could survive erasure at rest.** `search_index_v2` stores the
  body, and the erasure and rebuild paths carried hand-maintained projection lists
  that had drifted from the actual schema, so before this release an excised body
  remained in that table. It never surfaced in search results (both read paths gate on
  `canonical_nodes`), which is exactly why it went unnoticed. The paths are now
  registry-driven and total.
- **Erased bytes remained `grep`-able in the write-ahead log**, because a SQLite
  `DELETE` appends frames rather than rewriting them. Erasure verbs now truncate the
  WAL, and refuse to report success (raising `ErasureIncomplete`) when they cannot.
- **The telemetry sink retained erased ids.** Erasure verbs now selectively redact
  them, leaving unrelated records intact, and raise `ErasureIncomplete` on a sink that
  cannot be redacted or has been **rotated** — a rotated file is not a redacted file.
- **Provenance is now structurally mandatory**, so no new row can be written that no
  erasure verb can reach; `fathomdb doctor orphan-provenance` audits existing
  databases for rows already in that state.
- **`excise_collection_record` refuses to address the engine's own erasure
  bookkeeping**, so an operator cannot destroy the proof of an erasure — or an
  undischarged redaction obligation — through a normal-looking call. The retention
  sweep exempts the same collections, for the same reason.

None of these has a CVE. FathomDB is an embedded library with no network surface;
all five are data-at-rest completeness defects in the erasure path, found by
auditing it.

### Known issues

Characterized, reproducible and **shipping unfixed** in 0.8.20. Each is listed because
a consumer can observe it; internal hardening debt and tooling items are deliberately
excluded.

- **`transition` can fail immediately with an opaque storage error under concurrent
  writes (TC-90, p1).** `Engine::transition` runs a read-then-upgrade transaction
  under `BEGIN DEFERRED`, so SQLite refuses the lock upgrade outright instead of
  waiting. **Reproduces 10/10 under stress**: a plain `SQLITE_BUSY`, the busy handler
  invoked **zero** times, failing in 0 ms against a 5000 ms timeout — and
  `transition` never emits a diagnostic code, so the error carries nothing to
  distinguish it from a permanent storage failure. No `busy_timeout` setting avoids
  it. **Workaround:** serialise `transition` calls against other writers, or retry on
  `StorageError`. A fix is scheduled for 0.8.21. (This is the same
  lock-upgrade shape as the `TC-57` ingest defect fixed above, on a verb that fix did
  not cover.)

- **A projection worker can fail to persist its work with no signal at all (TC-91,
  p2).** Projection-worker commit outcomes are discarded (`let _ =
  commit_projection_outcomes(..)`), so a failure produces **no error, no log and no
  telemetry**. A caller relying on eventual search or vector freshness gets no
  indication that it will not arrive. **Workaround:** verify freshness explicitly —
  `read.projections`' `vector_dense_readiness` and a `drain` barrier — rather than
  assuming eventual convergence.

- **A rejected validity window no longer tells you which bounds were rejected
  (TC-95, p2).** `WriteValidation` / `WriteValidationError` is a message-less
  variant, so a `valid_from >= valid_until` refusal arrives as the static string
  `"write validation error"` with `data: null`. A Python or TypeScript caller cannot
  recover the offending values from the error. The same limitation applies to a
  rejected list of `ProjectionSpec`s: the error does not name which spec was invalid.
  **Workaround:** validate before calling, or submit one item at a time.
  Restoring a message-carrying variant is ~14 engine and ~44 binding sites and is its
  own slice.

- **`drain` can burn its full timeout and return an indistinguishable
  `SchedulerError` in two known cases (TC-62 / TC-64, p2).** (i) While the projection
  runtime is frozen — during a `purge`, an erasure, or an operator `rebuild` — the
  dispatcher will not schedule, so a `drain` issued in that window cannot make
  progress; there is no distinct error for "frozen" versus "genuinely slow embedding".
  (ii) A lost-wakeup race in the idle wait: a worker that finishes the last job while
  the waiter has released its lock notifies before the waiter re-acquires it, so the
  waiter sleeps out the remaining timeout. Measured on one test across parallel runs:
  30.19 s / 0.25 s / 0.26 s / 30.15 s. **A short `drain` timeout can therefore fail
  spuriously.** Both are pre-existing, not introduced by this release.

- **A retention `cap` no longer bounds the physical op-store table (TC-24).** It
  bounds the **sweepable** rows; the erasure-audit collections and the
  pending-redaction queue are exempt. See *Changed — BREAKING* for the full rationale.
  **If you sized `cap` against a physical row count, re-check that assumption.**

- **TypeScript: an absent optional field is `undefined`, not `null` (TC-35).** The
  napi object codegen **omits** the property entirely for a Rust `Option::None` rather
  than emitting `null`. The public TypeScript layer manufactures `null` where it
  wraps a value, but a bare passthrough leaks `undefined`. **Test with `== null` or
  `?? …`, not `=== null`.**

- **A calendar-impossible date is silently rolled over, not rejected (TC-47, p2).**
  Timestamp validation uses SQLite `strftime`, which rejects an impossible **month**
  (`2025-13-01` → NULL) but silently **rolls over** an impossible **day**:
  `2025-02-30` becomes `2025-03-02`, `2025-04-31` becomes `2025-05-01`. The
  shape gate checks format, not calendar validity. A BYO-LLM-extracted edge timestamp
  of Feb 30 is therefore stored as a **different, wrong instant**. The stored value is
  always a real instant, never NULL, so the "no resurrection via a NULL boundary"
  property holds — this is wrong-instant, not fail-open. **Workaround:** validate
  calendar dates before submitting them.

- **The `sqlite-vec` vec0 DELETE defect is worked around, not gone (TC-76, p1).**
  Upstream [#274](https://github.com/asg017/sqlite-vec/issues/274) is fixed in
  `sqlite-vec` 0.1.9; 0.8.20 stays pinned at `=0.1.7`, which still carries it, plus
  the engine-side workaround described under *Fixed*. The erasure paths are correct with
  the workaround in place and a tripwire test guards it, but the underlying dependency
  defect is still present in the shipped artifact. If you audit this yourself, note
  that the correct upstream id is **#274** — earlier FathomDB material cited `#99`,
  and a search against that number comes back empty and looks reassuring.

### Documentation

- **[Erasure](https://fathomdb.dev/operations/erasure/) (new).** What
  `erase_source` guarantees, and — explicitly — what it does **not**: copies,
  SQLite free pages absent a `VACUUM`, copy-on-write/snapshotted filesystems,
  and backups. Also documents the erasure-audit record and the non-PII
  `source_id` rule.

### Maintenance

- **Library Sweep #1 (0.8.11.1, label-only).** Contained dependency-hygiene bumps,
  no public-symbol change and no manifest version bump: `sha2` 0.10→0.11 (RustCrypto
  `digest` 0.11; 7 hex-format sites updated, hash output byte-identical), `typescript`
  5→6 + `@types/node` 25→26 in the TS binding dev toolchain (tsconfig `types: ["node"]`
  for TS6's dropped `@types` auto-discovery; emitted `.js`/`.d.ts` byte-identical),
  `actions/checkout` v6→v7 (26 pins), and a `dependabot.yml` coverage reconciliation.
  `action-gh-release` v2→v3 was deferred to 0.8.20 (release-path; cannot be validated
  without a real publish). No `Removed` entries.

## 0.8.9 — 2026-06-28

First publish since 0.8.0. The 0.8.1–0.8.9 work landed on `main` as a
label-only line (manifests held at 0.8.0); this cut bumps the Axis-W version
to 0.8.9 and ships the accumulated 0.8.x line to crates.io / PyPI / npm so
downstream consumers can pin a real `0.8.9`. Axis E (`fathomdb-embedder-api`)
intentionally stays at 0.6.0 (unchanged; the release pipeline's idempotency
guard skips the already-published 0.6.0 at publish time). Engine behavior and
the Python/TypeScript SDK surface are additive over 0.8.0; no public symbols
were removed.

### Added

- **`embed` Python↔TypeScript parity (0.8.6).** Restores cross-binding `embed`
  parity that had regressed to Python-only in 0.8.4; both SDKs now expose the
  same surface.
- **Per-task provider seam (0.8.6, OPP-8).** Per-task `fathomdb.<task>.v1`
  provider identity; the extract path is byte-identical to 0.8.0.
- **CE-rerank α exposed (0.8.5, EXP-0).** Cross-encoder rerank lever
  (`alpha` / `pool_n` / `ce_score`) is exposed on the search path. Default
  blend stays conservative (a product decision); the lever is opt-in.
- **GPU embedder device seam (0.8.7).** `parse_device_request` + device
  selection allow opt-in CUDA embedding (user-controlled spend). The default
  remains CPU and is byte-identical to 0.8.0; CPU↔CUDA 1-bit codes are
  identical.
- **Explain sidecar + opt-in telemetry + real-gold eval (0.8.8, EXP-OBS).**
  An explanation sidecar on the retrieval path; opt-in, no-egress telemetry
  (off by default); and a real-gold evaluation path on the eval side.
- **CI-integrity hardening (0.8.9).** Honest unmasking and fixing of
  pre-existing masked gates (AC-037 environment-scoping, live-surface
  conformance, pyo3 macOS/Windows link + Windows-portability fixes yielding
  first-ever green `rust-macos` + `rust-windows` on `main`, dependency and
  bootstrap checks). No gate tolerances were weakened.

### Removed

(none)

## 0.8.0 — 2026-06-08

Agent-memory enablement (G0–G12). See `docs/release-notes/0.8.0.md` (user) and
`dev/releases/0.8.0.md` (internal record). Schema version 13.

### Added

- **Governed `read.*` surface (G2/G3).** `read.get` / `read.get_many`
  (active-only point lookup by `logical_id`, request-order, partial) and
  `read.collection` / `read.mutations` (paginated op-store read-back,
  `ORDER BY id`, mandatory limit clamped to ~1M, exclusive `after_id` cursor).
  These four retrieval verbs ride the reader-worker DEFERRED-tx path; they never
  take the writer lock.
- **Hybrid retrieval.** `search` is the unconditional RRF fusion of the vector
  and FTS5 branches (G9); `search_filtered` adds an optional closed metadata
  filter (G10, unfiltered phase-1 SQL byte-identical to 0.7.2); a recency
  reweight seam (G12, off by default).
- **Structured search hits (G1).** `SearchHit { id, kind, body, score, branch }`.
- **Additive `WriteReceipt` fields:** `row_cursors`, `dangling_edge_endpoints`.
- **Recall fidelity verdict (AC-075).** The asserting recall@10 gate is the
  real-embedder `eu7_real_corpus_ac.rs`, measured on the pre-fusion vector stage
  (ANN+ bit-KNN K=192 + f32 rerank vs exact-f32 vector top-10) ≥ 0.90
  (measured 0.937) — an ANN-quantization-fidelity gate, NOT the RRF-fused
  `search()` output (◆ B-1). The synthetic `perf_gates::ac_013b` is report-only.
- **Tiered text-query latency (AC-076).** `ac_012` is binding at the 10k tier
  (p50 ≤ 20 / p99 ≤ 150 ms); 100k/1M tracked. The FTS5 tokenizer upgrade
  (`porter unicode61 remove_diacritics 2`) is latency-neutral (Slice 6).

### Changed

- **Canonical identity substrate (G0): active-uniqueness is `logical_id` alone**
  on `canonical_nodes` + `canonical_edges` (correcting a v0.5.x compound-key
  regression). One active row per `logical_id`; superseded versions retained,
  never returned by the active read path.
- **`search` return type:** `Vec<String>` → `Vec<SearchHit>` (behavior-compat
  event 2). `SearchHit` does not derive `Eq` (it carries an `f64` score).
- **`search` ranking:** unconditional RRF hybrid order; no `fusion_mode` knob
  (behavior-compat event 1). The pinned contract is determinism, not legacy
  reproducibility.
- **Rust facade: operator/recovery seam gated behind the `operator` feature
  (0.8.0 Slice 27 fix-1, Q5=BIND-RUST / AC-074).** The default `fathomdb`
  facade is now recovery-clean and raw-SQL-free at the method level: the 12
  operator/recovery `Engine` methods (`rebuild_projections`, `rebuild_vec0`,
  `excise_source`, `dump_schema`, `dump_row_counts`, `dump_profile`,
  `trace_source_ref`, `truncate_wal`, `verify_embedder`, `check_integrity`,
  `safe_export`, `recompute_mean`) and the 20 operator-seam re-export types
  (`CheckIntegrityOpts`, `IntegrityReport`, `SafeExportArtifact`, `TraceReport`,
  `TraceEvent`, `RebuildReport`, `RebuildKind`, `ExciseReport`,
  `VerifyEmbedderReport`, `VerifyEmbedderStatus`, `DumpSchemaReport`,
  `SchemaObject`, `DumpRowCountsReport`, `TableRowCount`, `DumpProfileReport`,
  `TruncateWalReport`, `TruncateWalStatus`, `Finding`, `MeanRecomputeReport`,
  `Section`) are absent from the **default** build and present only with
  `--features operator` (which `fathomdb-cli` enables). Gating, not deletion:
  engine behavior is identical with the feature on. The Python/TypeScript SDK
  surface and engine behavior are unchanged.

### Removed

(none yet)

## 0.7.2 - 2026-06-01

RELEASE-HARDENING. The campaign that takes the held 0.7.0 + 0.7.1 work to a
fully shipped, validated release. **Phase A closed the release:** a doc-drift
sweep (PR-1), embedder concurrency hardening (PR-9), a recall-floor reframe
(PR-2 family — the "recall gap" was a measurement artifact, not a defect), a
tiered real-corpus latency budget (PR-3), and the 0.7.0+0.7.1 publish (PR-4 —
`v0.7.1` is the first published 0.7.x; `v0.7.0` stays a local historical
marker, intentionally never published). **Phase B hardened the test/perf
foundation** for the 0.8.x agent-memory work: a corpus-driven test harness
(PR-5), always-on dev-loop perf gates (PR-6), and append-only perf-regression
detection (PR-7). Ledger: `dev/plans/runs/STATUS-release-hardening.md`.

### Added

- **Embedder concurrency hardening (PR-9, `21f4df6`).** Closes the two
  robustness items EU-5f surfaced on the projection embed path:
  - **Invariant-5 per-embed watchdog.** `embed_with_watchdog` runs each embed
    on a detached thread under a deadline (30 s default, configurable); a hung
    embed surfaces `RuntimeEmbedderError::Timeout` into the existing retry/
    failure path instead of parking a projection worker. Panic-transparent
    (preserves the EU-5f `ProjectionPanic` path).
  - **Engine-side embed serialization.** An `embed_serialize` guard invokes the
    shared `Arc<dyn Embedder>` one call at a time. Justified on **safety**
    (caller-supplied pyo3/napi embedders are `Sync`-only by contract and may not
    be concurrency-safe), not throughput — it is throughput-neutral on the
    candle default (candle uses one process-wide rayon pool; the earlier "~13×"
    throughput rationale was withdrawn).
  - **Circuit breaker.** `live_embed_threads` counts watchdog threads currently
    alive; at `embed_circuit_threshold` (8) the breaker latches and jobs fail
    fast without spawning, bounding the abandoned-thread leak that Invariant-5's
    no-abort rule makes unavoidable. Keyed on *concurrent* live threads (not a
    consecutive-timeout streak) so it bounds intermittent hangs and self-clears
    for slow-but-returning embeds.
  - codex: 5 passes, BLOCK→PASS (the pass-4 BLOCK on the original consecutive-
    timeout breaker design was not overridden — it was redesigned to the live-
    thread-count form above).
- **Per-push read-path smoke (PR-3, `d9f9b65`).** New always-on,
  fixture-independent canary `perf_gates::ac_013_vector_read_path_smoke`: an
  exact-match sentinel must rank 1 through the two-phase bit-KNN + f32 rerank
  path. No `AGENT_LONG`, `default-embedder` feature, or corpus needed — this is
  the CI guard that replaces the infeasible canonical N=1M perf run.
- **Corpus-driven test harness (PR-5, `c605a18`).** `tests/support/
  corpus_harness.rs` is now the canonical entry point for corpus-driven tests:
  `CorpusFixture` (`small`/`medium`/`full` + `per_source` + `from_docs`), a
  synthetic/real embedder toggle, one-line `ingest_into`, a `query_set`
  held-out query builder, and three invariant helpers (vec0 row count, FTS
  populated, non-empty search). Backed by a **per-(model, subset) embedding
  cache** under `data/corpus-data/.cache/embeddings/` (gitignored;
  `$FATHOMDB_CORPUS_CACHE_DIR`-overridable; key = sha256(identity + subset
  label + doc-id manifest); byte-deterministic blob; atomic write; the hit
  path re-verifies identity + manifest before trusting bytes; **every miss is
  loud**). Pack-4 tests (`corpus_vector`/`corpus_fts`/`corpus_graph`) migrated
  with no behavior change. Test-only `VaryingEmbedder::with_identity` seam
  added (the production `EmbedderIdentity` contract is untouched).
- **Always-on dev-loop perf gates (PR-6, `e2c886d`).** `tests/
  perf_gates_devloop.rs` — three perf ACs at N≈1000 that run on **every**
  `cargo test` (not `AGENT_LONG`-gated), in ≈16 s warm (≤30 s budget), over
  the real production read path. **Structural invariants HARD-assert** (vec0
  row count + FTS) to catch the batch-collapse class of regression; **latency
  is soft/NOTIFY-only** (synthetic p50≤50 / p99≤150 ms) with **one hard
  catastrophic ceiling** (10× soft) so an orders-of-magnitude scanner
  regression still RED-fails; **recall floor 0.85 is NOTIFY-only on the real
  path**. Signal split: synthetic isolates latency, real carries recall, the
  off-signal is report-only on each. Two-tier model documented in
  `dev/design/perf-gates.md` (devloop = fast feedback; canonical = ship
  verdict).
- **Perf-regression detection (PR-7, `84446ef`).** New `perf-regression-check`
  binary in `fathomdb-cli` reads the append-only `dev/perf-history/` store,
  groups runs by `(ac_id, n)`, and flags the single latest run against the
  rolling median of the prior ≤10 when p50/p99 degrades >15 % or recall drops
  >0.03 absolute (HITL-locked, conservative vs the 10 %/0.02 default). Strict
  fixed-width RFC3339→epoch parsing (dependency-free); exit codes 0/1/2; a
  `--json` mode; read-only (never mutates the store). CI integration in
  `perf-canonical.yml` posts verdicts to `$GITHUB_STEP_SUMMARY`. History
  backfilled v0.6.x → v0.7.2 (9 rows). **Honest limitation:** the detector is
  degradation-only, so it cannot flag the 2026-05-27 batch-collapse bug
  *itself* — that bug masqueraded as *improved* recall (degenerate 1.0), so
  the detector instead flags the regression-shaped *correction* (recall
  1.0 → 0.1572 at `4a95cfd`). Catching the bug itself needs a row-count /
  implausible-perfection anomaly rule, logged as future work in
  `dev/design/perf-regression-detection.md`.

### Changed

- **Recall floor reframe — 0.90 HOLDS, now correctly validated (PR-2 family,
  `78164b9`/`a154037`).** The AC-013b recall@10 "gap" measured at 0.7.1
  (0.828) was a **measurement artifact** — exclude-after-top-10 plus
  body-string ground truth over a corpus with ~5.6 % duplicate bodies — **not**
  an engine deficiency. The corrected ANN-fidelity measurement (exclude the
  query-source doc *before* top-10; dedup-by-id GT) on the real default embedder
  (bge-small, N=7,667, K=192, mean-centering) is **recall@10 = 0.937 (CI
  0.913–0.957, σ=0.0116)**; the full CI clears the 0.90 floor, so the floor is
  **kept at 0.90, not raised**. This number is **ANN/quantization fidelity**
  (how faithfully the 1-bit sign-quant index reproduces the same model's exact
  f32 top-10), **not IR relevance**. The separate embedder IR-relevance ceiling
  (recall@10 ≈ 0.571, CI 0.530–0.614, on 301 labeled queries) is **not a gate**.
  `ADR-0.7.0-vector-binary-quant.md` § 2 point 4 was amended to cite the
  corrected measurement; a fast sentinel `ac_013b_floor_matches_adr` pins the
  test constant to the ADR. Evidence: `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`,
  `dev/plans/runs/0.7.2-PR-3-perf-data.md`.
- **Tiered AC-013 / AC-019 latency budget (PR-3, `d9f9b65`; HITL 2026-06-01).**
  The vec0 bit-KNN candidate stage is a per-query **O(N) linear scan** (no ANN
  index), so a single N-independent latency budget is not meaningful. The budget
  is now **tiered**: the **10,000-row tier is the binding release gate for the
  0.x and 1.x lines** (`AC013_GATE_N = 10000`; AC-013 asserts 80/300 ms only at
  `n ≤ gate`, reports above it). The **100k and 1M tiers are tracked targets**
  for post-1.0 (pre-2.1) ANN-index work, not gated. `ADR-0.7.0-text-query-
  latency-gates-revised.md` was amended with the tiered table and HITL-locked.
- **Latency/recall measurement is now LOCAL once-per-release** (PR-3). Real-
  embedder canonical N=1M is infeasible on CI (~166 h seed at the PR-9-measured
  1.67 docs/s vs a 240-min workflow timeout; the synthetic 1M seed also did not
  drain in 3 h locally). Heavy measurement runs locally; CI carries only the
  read-path smoke above.
- **Synthetic AC-019 perf gate is now REPORT-ONLY** (PR-3;
  `perf_gates::ac_019_mixed_retrieval_stress_workload_tail`,
  `AC019_REPORT_ONLY`). The synthetic isotropic fixture cannot meet the
  `max(baseline_p99 × 10, 150 ms)` bound — a property of the synthetic data
  (instant embed → unrealistically fast baseline → too-tight 10× bound), not the
  engine. The asserting AC-019 signal lives in the real-corpus harness
  `eu7_real_corpus_ac.rs`, which PASSES at the 10k tier (clean run 343 ms <
  405 ms bound). AC-013 keeps its hard 10k-tier gate.
- **Architecture/design/ADR docs aligned to shipped 0.7.x reality (PR-1,
  `aebf959`).** Doc-drift sweep: 10 HITL-approved corrections across design,
  architecture, and ADR docs. Docs-only.

### Deferred

- **Automatic in-ingest mean-drift detector → 0.8.x** (PR-2 family,
  `64f72e0`/`2ef8c3d`). The adaptive mean-recompute drift detector was built and
  ratified, then **carved out** because its sole justification (recall) collapsed
  with the measurement-artifact finding and its benefit is unmeasured. It is
  parked for 0.8.x behind a RED guard (`dev/plans/prompts/0.8.x-auto-mean-drift-
  DEFERRED.md`), not silently dropped. The **manual doctor verb** (operator-
  triggered mean recompute) ships.
- **AC-013/AC-019 at 100k and 1M corpus** — tracked, not gated, pending a
  post-1.0 ANN index (HNSW/IVF/DiskANN) on the vec0 table to take per-query cost
  from O(N) to O(log N)/O(√N). (See Known limitations.)

### Removed

(none)

### Known limitations

- **No ANN index — vector search is O(N).** The vec0 bit-KNN candidate stage is
  a per-query linear scan over all N rows; there is no HNSW/IVF/DiskANN index.
  The 10k latency tier is met (real bge p50 36 / p99 49 ms at N≈7,667); 100k is
  ~147 ms p50 (synthetic 384-d) and 1M extrapolates to ~1.5 s — i.e. the 80 ms
  p50 budget is not met above ~50k. The ANN index is the named post-1.0
  (pre-2.1) follow-up, tracked in `dev/design/ann-index-vec0.md`.
- **1M real-corpus recall/latency not freshly measured.** ~166 h seed makes it
  infeasible on this hardware; 0.937 @ N=7,667 is treated as an upper-ish bound
  (recall decreases slowly with N) and the 1M latency tier is an O(N)
  extrapolation off the 0.7.0 W4.1 anchor, not a fresh run.

## 0.7.1 - 2026-06-01

EMBEDDER-UNDEFER: the default embedder is no longer deferred. fathomdb now
ships a real in-process default embedder (`BAAI/bge-small-en-v1.5`, 384d, via
`candle-transformers`), opt-in per binding, feeding the 0.7.0 sign-bit +
f32-rerank retrieval pipeline with mean-centering. The tag was intentionally
held until the 0.7.2 RELEASE-HARDENING campaign re-derived the real-corpus
recall floor (`AC013B_RECALL_FLOOR`): that work found the apparent 0.828 recall
"gap" was a measurement artifact, the corrected ANN-fidelity number is 0.937,
and the 0.90 floor holds (see the 0.7.2 "Changed" section). `v0.7.1` is tagged
on the version-bump commit and is **the first published 0.7.x** (crates.io +
PyPI + npm + GitHub release, 2026-06-01); `v0.7.0` was held as a local
historical marker and intentionally not published (see the 0.7.0 section). See
`dev/plans/0.7.1-implementation.md`.

### Added

- **Default embedder (opt-in).** `BAAI/bge-small-en-v1.5` (384d) runs in
  process via `candle-transformers`; WordPiece tokenization (truncated to 512
  tokens) → mean-pool → L2-norm → mean-centering → sign-bit quantization →
  bit-KNN (K=192) → f32 rerank → top-10. Opt in with Python
  `Engine.open(path, use_default_embedder=True)` or TypeScript
  `Engine.open(path, { useDefaultEmbedder: true })`; default is OFF (no embedder
  configured, vector writes fail with `EmbedderNotConfigured` as before).
  Caller-supplied embedders remain available in Rust; custom Python/TS embedder
  bridges are deferred to 0.8.x.
- **First-use weight download (visible, verified).** With the default embedder
  enabled, first use downloads the pinned weight set from a fixed Hugging Face
  URL set, caches it under the platform cache directory, and verifies every
  file by sha256 (no trust-on-first-use). The activity is surfaced in
  `OpenReport.embedder_events` (per-file url + bytes + sha256 + cache path) and
  `OpenReport.embedder_download_ms`. `HF_TOKEN` is honored for token-gated
  mirrors; public bge-small needs none. This is the scoped opt-in exception to
  NEED-017 / REQ-033 (see `ADR-0.7.1-default-embedder-weight-fetch`).
- **Mean-centering.** A per-workspace corpus-mean f32 vector is stored in
  `_fathomdb_embedder_profiles.mean_vec` and subtracted before sign-bit
  quantization (the f32 rerank stays un-centered). It is pinned once at the
  first 256 ingested vectors and never silently recomputed.
- **Bindings.** Python `use_default_embedder` / TypeScript `useDefaultEmbedder`
  open flags; `OpenReport` gains `embedder_download_ms`, `embedder_events`
  (typed union), `embedder_mean_centering_required`, `embedder_mean_vec_pinned`.
- **Docs.** `docs/embedder.md` user guide (opt-in, first-use download, cache,
  offline notes, caveats, migration).

### Changed

- `OpenReport.default_embedder` now reports the real bge-small identity
  (`fathomdb-bge-small-en-v1.5` / HF snapshot `5c38ec7c...` / dim 384) instead
  of the `fathomdb-noop` scaffold identity.
- Schema migration step 10 adds the nullable `mean_vec BLOB` column.
- Wheel / `.node` binary size: the `default-embedder` feature pulls in candle +
  tokenizers and the ~133 MB weight set is fetched at first use (not bundled).
  The feature is opt-in, so builds without it pay no size cost; a per-platform
  wheel-size gate guards regressions.

### Fixed

- **Mean-centering is now applied on the production write path.** Prior to this
  release the corpus-mean pin/apply only ran via an internal test seam; the
  async `engine.write` → projection path never pinned, so real ingests were
  sign-quantized un-centered. The pin + re-quantize now run inside the
  projection commit (serialized for cross-worker correctness), with an
  open-time recovery pin for crash-before-pin workspaces.
- **Embedder inputs over 512 tokens are truncated** instead of erroring. Long
  documents previously failed the BGE forward pass
  (`index-select invalid index 512`).
- **Projection workers are fault-isolated.** A panic inside an embedder no
  longer wedges the projection scheduler (`drain` could previously hang into a
  scheduler timeout); the faulted batch is recorded and the worker recovers.

### Removed

(none)

### Known limitations

- **Real-corpus recall caveat (SUPERSEDED in 0.7.2 — see below).** At 0.7.1
  measurement time, dev-box scouting over the 7,667-doc corpus measured
  recall@10 ~0.828 (95% CI ~0.80–0.86) — below the 0.90 floor — and the floor
  re-derivation + canonical validation were deferred to 0.7.2 (PR-2/PR-3).
  **This 0.828 was later shown to be a measurement artifact** (exclude-after +
  body-string ground truth). The corrected ANN-fidelity number is **0.937 (CI
  0.913–0.957)** and the 0.90 floor HOLDS; see the 0.7.2 "Changed" section and
  `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`. Do not cite the 0.828
  number; it is superseded.
- **Topic-drift mean.** Because the mean is pinned on the first 256 ingested
  docs and never recomputed, a workspace whose first 256 docs are
  unrepresentative may under-center. Remedy is reindex (a later campaign).
- **Migration.** Workspaces previously opened with the `fathomdb-noop` profile
  (0.6.x / 0.7.0) fail closed when re-opened with the default embedder (identity
  mismatch, by design per `ADR-0.6.0-vector-identity-embedder-owned` +
  `ADR-0.8.0-embedder-identity-change-workflow`). The remedy is wipe-and-rewrite;
  there is no in-place swap.

## 0.7.0 - 2026-06-01

PERF-VECTOR-QUANT (PVQ). A perf-focused release line whose load-bearing change
is **binary vector quantization + f32 rerank** to bring vector retrieval latency
(AC-013) within budget. The workspace was bumped to `0.7.0` (`38d5f4f`) and a
local `v0.7.0` tag was cut, then **held unpushed pending 0.7.1's embedder work**
(per the 2026-05-27 instruction). `v0.7.0` **remains a local historical marker
and was never published** — its tagged tree predates the release-gate CI fixes
and would fail preflight; `v0.7.1` is the first published 0.7.x and ships on its
own, carrying both the 0.7.0 latency win and the 0.7.1
default embedder that finally lets recall be measured on real text. AC-013b
recall was held OPEN at 0.7.0 ship
(synthetic isotropic fixture cannot reach the floor; real-embedder validation
deferred to 0.7.1). Ledger: `dev/plans/runs/STATUS-perf-vector-quant.md`;
decision: `ADR-0.7.0-vector-binary-quant.md`.

### Added

- **Binary vector quantization (Pack 1).** `vector_default` gains a sibling
  `embedding_bin bit[768]` column computed via sqlite-vec `vec_quantize_binary`
  inside the same writer transaction as the f32 insert (double-write at both
  insert sites). The f32 `embedding` column is retained for the rerank phase and
  the recall ground-truth pass. Schema migration step 9
  (`migrations/009_vector_binary_quant.sql`) with an unknown-kind preflight
  CHECK; dim-aware in-place reshape (`migrate_vector_partition_to_pack1`).
  Commits: `9b9f840`, `f5da3e4`, `7d4aa2c`, `d96c4b0` (RED).
- **`source_type` partition key + metadata columns (Pack 1).** vec0
  partition_key `source_type` (cardinality ~6: email/article/paper/meeting/
  note/todo) mapped from vector kind at write time via `resolve_source_type`
  (6-value HITL lock, `doc→article` coercion), plus `kind`/`created_at`/`tags`/
  `project_mentions` metadata within the vec0 16-column budget. This is the
  correct shape for real workloads (single-kind AC-013 fixture sees no benefit;
  bundled to avoid a second migration).
- **Two-phase query path (Pack 2).** `read_search_in_tx` replaced the
  single-phase f32 brute-force scan with two-phase **bit-KNN
  (`TOP_K_BIT_CANDIDATES`, K=64 at 0.7.0; raised to 192 in 0.7.1) + f32 rerank
  via `vec_distance_l2`**, in a single Deferred read transaction. Commit
  `26ef3dc`.
- **Real-corpus test corpora (CORPUS-1..4).** ~7,667-doc multi-source corpus
  with raw source assets under `data/corpus-data/raw/` (CNN/DailyMail, Enron,
  QMSum, EnronQA, synthetic
  notes/todos/daily-logs) + cross-doc chain generator + ingest harness +
  search-validation gates. Commits across `5c1e92a`..`d9a219d`.
- **New perf-gates recall test.** `ac_013b_recall_at_10_floor` asserts
  recall@10 ≥ 0.90 against in-test f32 brute-force ground truth (`d468999`).

### Changed

- **AC-013 latency closed (the load-bearing win).** Replacing the single-phase
  f32 brute-force scan with two-phase bit-KNN + f32 rerank (plus the
  `4a95cfd` batch-collapse fix below) brought AC-013 well within budget: at the
  dev-box anchor, p50 fell from the W4.1 single-phase 2048 ms / p99 2327 ms (at
  N=1M) to p50 12 ms / p99 16 ms (post-batch-fix), and the scanner fix below cut
  the AC-013 seed ~11×. (These are dev-box figures; canonical N=1M validation
  was deferred and later reframed to a tiered, local-measurement posture in
  0.7.2 PR-3 — see the 0.7.2 "Changed" section.)
- **AC-019 mixed-retrieval stress budget maintained.** The two-phase path also
  brought the AC-019 stress tail within budget at the dev-box anchor (p99 8388 ms
  at N=1M W4.1 → 131 ms, under the 175 ms dev-box bound). The AC-019 budget was
  later restructured in 0.7.2 PR-3 (synthetic gate report-only; real-corpus
  harness is the asserting signal).
- **AC-013 latency budget re-pinned to 80 / 300 ms** (`AC013_BUDGET_P50/P99`,
  `d468999`), superseding the 50 / 200 ms unindexed-path values. Tracked in
  `ADR-0.7.0-text-query-latency-gates-revised.md`.
- **Projection scanner throughput fix** (`53a270d`): `PROJECTION_INFLIGHT_LIMIT`
  raised 8→32 and the dispatcher now fills the full inflight budget per scan
  cycle (was one job per cycle). Dev-box AC-013 seed dropped ~11× (28.1 s → 2.5 s
  at N=10K).

### Fixed

- **`engine.write` batch-collapse bug** (`4a95cfd`). `write_inner` now allocates
  one write cursor per row in a batch, so a batch of N produces N distinct vec0
  rows (previously collapsed to ~1 unique row, which masked the recall/scanner
  issues with a degenerate recall=1.0). Regression test
  `tests/batch_write_per_row_cursor.rs`.

### Deferred / known gaps at 0.7.0 ship

- **AC-013b recall@10 ≥ 0.90 — held OPEN, not retconned.** The synthetic
  `VaryingEmbedder` fixture cannot reach the floor: sparse (6 of 768 coords)
  scored 0.1572; dense isotropic (`38f5e3a`) scored 0.5124 — the isotropic-noise
  floor, since random vectors carry no semantic structure for sign-bit ANN. Only
  real embeddings can validate the floor, deferred to 0.7.1 EMBEDDER-UNDEFER
  EU-7. 0.7.0 ships the **latency** win (the load-bearing AC-013 closure) with
  recall surfaced as a known gap.
- **Canonical N=1M validation + numeric budget lock** deferred (the seed cost is
  itself the gate). Later reframed in 0.7.2 PR-3 to a tiered, local-measurement
  posture.
- **AC-020 architectural lever** (`ADR-0.7.0-ac020-architectural-lever`,
  status `draft, HITL-required`) — PCACHE2 remains the named 0.7.0 architectural
  lever; the binary-quant change is explicitly a data-encoding change, not a
  second lever.

### Removed

(none)

## 0.6.1 - 2026-05-25

Promotion of `0.6.1-rc.1` to GA following V-slice fresh-install
verification (GREEN on all three bindings — see
`dev/plans/runs/0.6.1-V-transcript.txt`). Scope identical to RC1
below; no code or interface change between RC1 and GA. Axis E
(`fathomdb-embedder-api`) remains at `0.6.0` per Wake decision
`d-001`.

## 0.6.1-rc.1 - 2026-05-25

Patch release. Closes three 0.6.0 deferred items (Python and TypeScript
`OpenReport` surfacing, plus the axis-E independence demonstration),
resolves three Dependabot advisories, and carries the AC-012 canonical-
runner re-measurement as documented evidence (verdict RED; Pack 7 perf
work escalates to 0.7.0 per HITL 2026-05-24).

Axis-E (`fathomdb-embedder-api`) stays at `0.6.0` per Wake decision
`d-001`: no trait-surface change in this release, so axis-E does not
bump in lockstep with axis-W. This is the first post-GA exercise of
the two-axis discipline.

### Fixed

- `OpenReport` is now surfaced from both bindings via an engine-attached
  accessor (closes 12-TX-OPENREPORT carry-over from 0.6.0 GA):
  - Python: `engine.open_report()` returns the native `OpenReport`
    fields verbatim under snake_case identifiers
    (`schema_version_before`, `schema_version_after`,
    `migration_steps`, `embedder_warmup_ms`, `query_backend`,
    `default_embedder`). Idempotent — repeat calls return identical
    data (snapshot, not live state). Closes **AC-068c**.
  - TypeScript: `engine.openReport()` returns the camelCase mirror
    (`schemaVersionBefore`, `schemaVersionAfter`, `migrationSteps`,
    `embedderWarmupMs`, `queryBackend`, `defaultEmbedder`). Sync
    return — data lives in the napi engine struct after `open`.
    Closes **AC-068d**.
  - `Engine.open(...)` signatures are unchanged from 0.6.0 in both
    bindings (additive accessor; no return-shape regression).
- `scripts/security/check_removal_changelog.py` and its bash wrapper
  point their `--base` default at `v0.6.1` (was `v0.6.0`), advancing
  the "removals since last released API" anchor as 0.6.1 becomes the
  new GA reference. AC-050c regression-sentinel test #4 will be
  transiently RED in the BUMP → RC1 → GA window until the `v0.6.1`
  tag is pushed.

### Security

- **RUSTSEC-2025-0020** — bump `pyo3` `0.22.6` → `0.24.1` across the
  workspace; rename `*_bound` PyO3 APIs (24 callsites) to drop the
  deprecation warnings under `-D warnings`.
- **GHSA-mh29-5h37-fv8m** — bump `js-yaml` `4.1.0` → `4.1.1` via
  `markdownlint-cli2` `0.18` → `0.22.1` (transitive).
- **CVE-2024-3651 (idna)** — confirmed false-positive against
  fathomdb (not in the Python dependency graph after lock-file
  audit); `src/python/uv.lock` checked in to make the audit
  reproducible.

### Changed

- `scripts/set-version.sh --workspace 0.6.1` exercises the axis-E
  independence invariant for the first time post-GA: axis-W
  (`Cargo.toml`, `pyproject.toml`, `package.json`, and the five
  workspace.dependencies pins for `fathomdb`, `fathomdb-embedder`,
  `fathomdb-engine`, `fathomdb-query`, `fathomdb-schema`) advances
  to `0.6.1`; axis-E (`fathomdb-embedder-api` `[package].version`
  and its `workspace.dependencies` pin) stays at `0.6.0`.
  Regression sentinel codified in
  `scripts/tests/test_set_version.sh` test #13.
- B-001 forward-retag — `scripts/security/check_removal_changelog.py`
  and `scripts/security/check-removal-changelog.sh` default `--base`
  advanced from `v0.6.0` to `v0.6.1`.

### Removed

(none — patch release, no public symbol removals.)

### Deferred (carry-over)

- **AC-012** text-query latency on FTS5 (p50 ≤ 20 ms / p99 ≤ 150 ms):
  re-measured 2026-05-23 on canonical x86_64 tier-1 CI (AMD EPYC
  9V74, 4 cores, Ubuntu 24.04.4, rustc 1.95.0, SQLite 3.45.x via
  `libsqlite3-sys` 0.28.0) at N=1,000,000. Verdict **RED**:
  p50 = 140.95 ms (7.05× over budget), p99 = 458 ms (3.05× over
  budget). Pack 7 un-defer trigger fires; AC-012 closure target
  moved to **0.7.0** (perf-only release; budget revision + tuning).
  Evidence: `dev/notes/perf-canonical-runner-2026-MM.md` and
  `dev/plans/runs/0.6.1-AC012-measure-output.json` (workflow run
  26346417896). 0.6.1 carries this measurement as evidence and
  does NOT claim AC-012 closure.
- **AC-013** vector retrieval latency, **AC-019** mixed-retrieval
  stress tail, **AC-020** N=8 concurrent reader scaling: stay
  deferred per Pack 7 trigger evaluation (Pack 7 escalated to
  0.7.0 alongside AC-012).
- **Logical-id verbs** (`purge_logical_id` / `restore_logical_id`)
  stay deferred to **0.8.0** per HITL 2026-05-24 rescope.

## 0.6.0 - 2026-05-19

First stable release of FathomDB 0.6.0 — local-first retrieval
engine on SQLite (FTS5 + `sqlite-vec`) with Rust, Python, and
TypeScript SDKs.

### Added

- Local-first retrieval engine on SQLite (FTS5 + `sqlite-vec`):
  canonical writes, vector projections, scheduler, op-store, reader
  pool with thread-affine workers.
- Five-verb runtime SDK surface: `Engine.open`, `engine.write`,
  `engine.search`, `engine.close`, `admin.configure`.
- Typed error hierarchy under `EngineError` (`StorageError`,
  `ProjectionError`, `VectorError`, `EmbedderError`,
  `EmbedderNotConfiguredError`, `KindNotVectorIndexedError`,
  `SchedulerError`, `OpStoreError`, `WriteValidationError`,
  `SchemaValidationError`, `OverloadedError`, `ClosingError`,
  `DatabaseLockedError`, `CorruptionError`,
  `IncompatibleSchemaVersionError`).
- Engine-attached instrumentation: `engine.drain`,
  `engine.counters`, `engine.set_profiling`,
  `engine.set_slow_threshold_ms`, host logging subscriber attach.
- Python SDK (`fathomdb`) — PyO3 binding with type stubs.
- TypeScript SDK (`fathomdb`) — napi-rs binding, Promise API,
  handoff pool, typed exception envelope (TS milestone 1; not yet
  Python-parity — see Deferred).
- Rust facade crate (`fathomdb`) re-exporting runtime verbs from
  `fathomdb-engine`.
- CLI (`fathomdb-cli`) — `doctor` and `recover` verbs (Phase 10a).
- Two-axis versioning (Axis W workspace lockstep + Axis E
  independent embedder-api semver) with `scripts/set-version.sh
--check-files` enforcement and pre-push hook integration.
- 8-tier topological publish workflow `.github/workflows/release.yml`
  with crates.io index-propagation sleeps, post-publish smoke
  against fresh registry installs, co-tagging assert.
- actionlint v1.7.7 wired as canonical workflow validator.
- External user docs: install + quickstart + reference + concepts
  - compatibility (Phase 12-DX).

### Changed

- Release workflow: napi build matrix uses the canonical
  `win32-x64-msvc` target label for npm `optionalDependencies`
  resolution.
- Release workflow: `publish-rust` dry-run cascade restored via
  the rc.1 bootstrap publish that seeded sibling-dep versions on
  crates.io.
- Release workflow: npm `publish` passes `--tag next` for
  prerelease versions so the `latest` dist-tag stays pinned to
  the most recent stable.
- Release workflow: post-publish gates (`assert-co-tagging.sh`
  and the three `smoke-{crates,pypi-wheel,npm}.sh` scripts)
  accept `MAJOR.MINOR.PATCH(-PRERELEASE)?` SemVer.
- Release workflow: `smoke-pypi-wheel.sh` normalizes SemVer to
  PEP 440 (e.g. `0.6.0-rc.4` → `0.6.0rc4`) before `pip install`
  so the wheel resolves under pip's normalized version index.
- Release workflow: `assert-co-tagging.sh` sends a `User-Agent`
  header on crates.io API calls (the registry returns HTTP 403
  without one).
- Release workflow: PyPI + npm smoke scripts write a minimal
  valid record (`{"kind":"doc","body":"{}"}`) instead of an
  empty batch that the engine rejects per the 5-verb invariant.
- Release workflow: new `src/ts/tsconfig.build.json` emits
  `dist/index.js` at the path `package.json "main"` points to.
- Release workflow: `github-release` job explicitly sets
  `prerelease: ${{ contains(github.ref_name, '-') }}` so future
  RC tags are flagged as prereleases on GitHub.

### Deferred

- **Performance gates AC-012 / AC-013 / AC-019 / AC-020** deferred
  to 0.6.1 + Pack 7 (HITL re-confirmed 2026-05-17, Phase 12-P).
  AC-020 N=8 concurrent reader scaling is an architectural gap
  requiring vendor-SQLite work; AC-012 expected to close on
  canonical-runner re-measurement; AC-013/AC-019 close via Pack 7
  batched-insert vec0 API. See `dev/test-plan.md` § Current Perf
  Attribution.
- **`Engine.open` structured open report** dropped by both Python
  and TypeScript bindings in 0.6.0; populated native-side but not
  surfaced. Closes in 0.6.1 (slice `12-TX-OPENREPORT`). Symmetric,
  not a parity gap.
- **Logical-id verbs** (`purge_logical_id`, `restore_logical_id`)
  deferred to 0.8.0 (originally deferred to 0.7.x at Phase 12-V-VERBS
  2026-05-17; re-targeted to 0.8.0 per HITL 2026-05-24 alongside the
  canonical-identity substrate and Memex knowledge-store work — see
  `dev/roadmap/0.8.0.md`). Canonical-identity substrate design-only
  in 0.6.0. Client workaround: `fathomdb recover --excise-source <id>`.
- **TypeScript SDK Python-parity** — TS milestone 1 shipped
  2026-04-07; full Python-parity did NOT land at 0.6.0 GA and
  carries forward as a post-GA deliverable. Prefer Python SDK
  for production pilots until parity ships.

### Removed

(none — 0.6.0 is a rewrite; no 0.5.x→0.6.0 deprecation shims)
