---
title: FathomDB 0.8.20 — Plan (state-machine ladder)
subtitle: OPP-12 record-lifecycle Phase-2 + erasure completeness + the coordinated breaking-pair publish
date: 2026-07-12
status: ACTIVE
target_release: 0.8.20
---

# FathomDB 0.8.20 — Plan (state-machine ladder) · **OPP-12 record-lifecycle Phase-2 + erasure completeness + the coordinated breaking-pair publish**

> **DE-STALED 2026-07-12.** This file previously held the **Library Sweep / major-dependency-migration**
> runbook (napi 2→3 · rusqlite 0.31→0.40 + sqlite-vec). Per master **F-19/F-20** that content was re-homed to
> **`0.8.22`** — it is **removed here** and must be authored at `plan-0.8.22.md`. Nothing in this file is a
> dependency migration.
>
> **0.8.20 = OPP-12 Phase-2 + erasure completeness + the FIRST REAL PUBLISH.** Even micro, publishable.
> **BUILD-AUTHORIZED (F-21).** Publish remains a **separate per-`x.y.z` HITL gate**.
>
> **Base verified from `origin/main` @ `d526d15c` (code, not memory — every anchor below re-read at this SHA).**

## 0. Base verification (primary sources, re-read at `d526d15c`)

| Anchor | Claim | Verified |
|---|---|---|
| ~~`SCHEMA_VERSION = 20`~~ → **`SCHEMA_VERSION = 22`** | **CORRECTED 2026-07-20 (Slice 15b).** The `20` row was **stale**: it recorded the 0.8.19 19→20 existence migration, but 0.8.20 has since added **step 21** (legacy provenance backfill, Slice 5c) and **step 22** (`canonical_nodes` validity window, Slice 10) | ✓ **`fathomdb-schema/src/lib.rs:6` reads `pub const SCHEMA_VERSION: u32 = 22;`**, pinned by `s22_is_head_and_schema_version_is_22` (`fathomdb-schema/tests/step22_migration.rs:285`) |
| manifests = **`0.8.9`** | every release since 0.8.9 was label-only ⇒ **0.8.20 is the first manifest bump `0.8.9 → 0.8.20`** | ✓ `src/python/pyproject.toml:7`, `src/ts/package.json:3` |
| `transition` / `purge` **shipped in both SDKs** | 0.8.19 Phase-1 surface is live | ✓ `fn transition` / `fn purge` in `fathomdb-py/src/lib.rs`; `pub async fn transition` / `pub async fn purge` in `fathomdb-napi/src/lib.rs` |
| **Phase-2 surface = 100 % NET-NEW** | `ReadView`, `valid_from`, `valid_until`, `dense_readiness`, `configure_projections`, `ProjectionSpec`, `EntityTypeSpec`, `id_prefix` | ✓ **ZERO hits** across all crates |
| `derive_logical_id` = `SHA256("{kind}:{name}")` | natural-key derivation, **not** an opaque surrogate | ✓ `fn derive_logical_id` in `fathomdb-engine/src/lib.rs` |
| `search_index_v2` = **content-storing** FTS5 | holds the **body verbatim** (no `content=''`) | ✓ `fathomdb-schema/src/lib.rs:427` |
| `truncate_wal()` **already exists** | `PRAGMA wal_checkpoint(TRUNCATE)`, returns typed `TruncateWalStatus::{Done,Busy}` | ✓ `pub fn truncate_wal` / `fn wal_checkpoint_truncate_once` in `fathomdb-engine/src/lib.rs`; **CLI-only** (the `args.truncate_wal` → `wire_recover(…, "truncate-wal", …)` arm in `fathomdb-cli/src/lib.rs`); **NOT called by `purge`/`excise`** |
| op-store record erasure | **does not exist** — only a cap-based retention sweep | ✓ `fn enforce_provenance_retention` in `fathomdb-engine/src/lib.rs` |
| REQ-037 → AC-041 | recovery surface CLI-only; AC-041 tests the **REQ-054 five-name denylist** only | ✓ `dev/requirements.md:332`; `dev/acceptance.md:688` |
| AC minting floor | ~~highest existing AC = AC-077~~ **CORRECTED (TC-14):** highest **defined, non-reserved** AC = **AC-076**; AC-077/078 are **live IR-1/IR-2 reservations** ⇒ **0.8.20 mints from AC-079**. Never mint by "max AC id + 1" — see the warning in §3. | ✓ `dev/acceptance.md:1147` (AC-076), `:1286`/`:1297` (reservations) |

### 0.1 THE ROOT-CAUSE FINDING — `search_index_v2` is maintained by only **TWO** of FIVE sites

| # | Site | fn | `search_index` | `search_index_edges` | **`search_index_v2`** | `vector_default` |
|--:|------|----|---|---|---|---|
| 1 | **WRITE** | `project_canonical_node_row` `:11779` | ✓ `:11789` | — | **✓ `:11806`** | ✓ |
| 2 | **PURGE** | `purge_inner` `:6164` | ✓ `:6225` | ✓ `:6227` | **✓ `:6229`** | ✓ `:6231` |
| 3 | **EXCISE** | `excise_source_inner` `:6398` | ✓ `:6427` | ✓ `:6432` | **❌ MISSING** | ✓ `:6438` |
| 4 | **REBUILD** | `rebuild_shadow_state` `:6515` | ✓ `:6525`/`:6548` | ✓ `:6529` | **❌ NEVER TOUCHED** (no DELETE, no INSERT) | ✓ `:6533` |
| 5 | **TOKENIZER-REPROJECT** | `reproject_search_index_after_tokenizer_upgrade` `:9515` | ✓ `:9519`/`:9522` | — | **❌ NEVER TOUCHED** | — |

**`search_index_v2` is WRITTEN by the write path and DELETED only by `purge`.** Excise misses it; rebuild and
tokenizer-reproject never touch it at all. Consequences, all verified:

- **Erasure leak (data-at-rest).** After `excise_source`, the body **survives verbatim** in `search_index_v2`
  (`SELECT body FROM search_index_v2`). `secure_delete=ON` cannot help — it only zeroes pages freed by a real
  `DELETE`, and these rows are never deleted.
- **Unbounded retention.** `search_index_v2` **monotonically accumulates every body ever written** —
  superseded, excised, all of it — prunable by no path except `purge`.
- **Tokenizer divergence (correctness).** After a tokenizer upgrade, v1 is re-tokenized and **v2 is not** ⇒
  BM25F scores against a stale tokenizer.
- **Invisible to every functional test.** Both FTS read paths gate candidates on `canonical_nodes` (BM25F
  inner-joins for corpus stats `:11948`; intersects an `active` set `:11982`/`:12002`). **A test that *searches*
  for the excised text PASSES on the broken code.** ⇒ **RED tests MUST assert on raw table contents.**

**The bug is not a missing `DELETE`. It is that the row-owned table list is implicit and duplicated five
times.** Patching site 3 fixes today and re-opens the hole at the next projection table. **Fix the mechanism.**

There is also **no edge projector** — only `project_canonical_node_row`; edge projection is inlined in
`commit_batch` (`:12166+`). Any "replay through the projector" rebuild MUST cover edges or it silently drops
edge FTS + edge vectors.

---

## 1. Goal & scope

**Theme.** Finish OPP-12 (Phase-2), make deletion actually work, and **publish the breaking pair**.

### In scope

**A · OPP-12 Phase-2 (net-new; master §4 0.8.20 row, F-19/F-20/F-21)**

- **`ReadView` / read-modes** — composable relax-flags; uniform on the five read verbs `read_get` /
  `read_get_many` / `read_list` / `read_list_filter` / `graph_neighbors` *(names corrected at Slice 10)*.
- **Node-validity** — `valid_from` / `valid_until`, integer windows, bound-`:now` seam, `valid_as_of`.
- **Projection registry (C-1 co-land)** — `configure_projections(spec, drop?)`, `ProjectionSpec {name, roles:
  {filterable, rankable, searchable}}`, idempotent diff + backfill; **engine is the sole projection authority**;
  **EAV / property-FTS** (the store it projects from — only body-FTS exists today).
- **`dense_readiness` + `flush_embeddings()`** + the atomic readiness-flip (additions to the existing worker).
- **Surrogate `logical_id` minting — SCOPE-CORRECTED (see §2.1).** Serves **ONLY registry-admitted governed
  entities**. **NOT doc-chunks.**
- **X1 SDK parity** (Py + TS, live functional harnesses).

**B · Erasure completeness (net-new; HITL steer todos-ledger seq 23 + this plan's §0.1)**

- One **shared row-owned projection registry**; a **total projector** (node + edge); five sites collapse to one.
- Provenance made **mandatory and caller-sourced**; a **reachable erasure verb**.

**C · The coordinated breaking-pair publish** — manifests **`0.8.9 → 0.8.20`**; pairs with a Memex
`0.5.x-successor`. Prereq **0.8.18 #11-full publish machinery** ✓ (proven + exercised via staging, never fired).

### Out of scope

- **Dependency migrations** (napi 2→3 · rusqlite/sqlite-vec) → **0.8.22** (F-19/F-20).
- **HNSW / ANN** → **2.x** (F-16). Not here, not anywhere in 0.8.x.
- **Scale-bound** (soft/stated) → **0.8.23 / 0.8.24** (F-20).
- **TC-5** (full eu7 floor re-baseline) → 0.8.23 · **TC-9** (`ort` GPU-EP) → 0.8.22 · **TC-10** (#5 open-latency
  optimization) → ≥0.8.21 only if warranted (F-22).

---

## 2. Decisions already taken (do NOT re-litigate)

### 2.1 TC-11 — the doc-seeded `h:` end-state pin · ✅ **HITL-RATIFIED 2026-07-12** (F-23 guardrail **DISCHARGED**)

> **Pin A — terminal-forever, by explicit OVERRULE of `structural-lifecycle-contract.md §2(ii)`.**
> Anonymous / doc-seeded nodes stay **`h:<content-hash>` PERMANENTLY** — no backfill, no forward-mint, no split.
> Any Phase-2 surrogate serves **ONLY registry-admitted governed entities**; eligibility is decided **at write
> time**, and a stored row's id-space is **NEVER re-derived**.

**This CANCELS — not defers — the surrogate leg for the anonymous class.** The master 0.8.20 row and F-23 both
carried it as "deferred from Phase-1"; it is now **cancelled** for doc-chunks. Grounds (all code-verified):

- An opaque surrogate is **not re-derivable from content** ⇒ re-ingest mints a *different* id, destroying the
  re-ingest-stable content-addressed identity `h:` provides (`same bytes ⇒ same handle` — the basis of
  cross-session gold keying, telemetry `result_stable_ids`, and explain-correlation).
- `derive_logical_id` is **not** the surrogate mechanism — `:54` requires "never content-derived/**hashed**", and
  it *is* hashed (`fn derive_logical_id` in `fathomdb-engine/src/lib.rs`); §2:98 names it a **separate** mechanism.
- §2(ii) has **no consumer** — the contract itself states Memex's lifecycle problem is closed by **(i) alone**.
- Its stated goal is **already met** — the shipped C-2 `IdSpace` is **total** (`l:`/`h:`/`p:`, non-null) ⇒
  **`h:` IS an address**.

**Enforcement (no new column).** The record **is** `canonical_nodes.logical_id`'s null-ness. The invariant is a
**prohibition**: *no migration, backfill, or verb shall ever populate `logical_id` on an existing canonical row.*

**Accepted corollary (document it; do not "fix" it).** An anonymous row and a later governed row for the same
real-world thing **both stay active and both surface in search** — supersession keys on `logical_id`, and the
partial-unique index is `ON canonical_nodes(logical_id) WHERE superseded_at IS NULL`, so **NULL never collides**.
The engine does not dedupe them. Supplying a `logical_id` **at write time** is what makes a record governed.
Remove the anonymous row by excising its source.

**Applied to the authority surfaces:** `structural-lifecycle-contract.md` §2(ii) (**OVERRULED**) ·
the struck `opaque surrogate` bullet in `dev/design/record-lifecycle-protocol/README.md` ·
`api-surface.md:64` (surrogate leg **CANCELLED, not deferred**).
Design of record: `dev/design/0.8.20-erasure-and-h-end-state-v4.md`.

### 2.2 The erasure axis is **PROVENANCE**, not the `l:`/`h:` id-space

`transition`/`purge` are **`l:`-only by design** (an anonymous row's identity *is* its bytes; "change the record"
is incoherent). Anonymous content — **the dominant corpus class** — is erased by **`source_id`**.
**Pin A is therefore ORTHOGONAL to GDPR and costs nothing there.**

### 2.3 REQ-037 lawful-erasure carve-out · ✅ **HITL-APPROVED 2026-07-12**

The project's real policy is **"RECOVERY-*NAMED* verbs are CLI-only"** — **not** "destructive ⇒ CLI-only".
Proof: **`purge(logical_id)` is already an SDK verb** (`fn purge` in `fathomdb-py/src/lib.rs`, `pub async fn purge`
in `fathomdb-napi/src/lib.rs`, 0.8.19) *despite being named in
REQ-037's forbidden list*, because **AC-041 tests only the REQ-054 five-name denylist**
{`recover`,`restore`,`repair`,`fix`,`rebuild`} — and `purge` is not one of them.

**The defect is an ASYMMETRY:** the `l:` axis got a first-class application erasure verb; the `h:` axis (the
dominant corpus class) got none. **And `excise_source` is unreachable from any SDK consumer — the wheel declares
no `[project.scripts]` and the npm package no `"bin"`, so no `fathomdb` CLI is shipped.**

**RULING (HITL 2026-07-12):**

- **`excise_source` stays CLI-only.** It is the **recovery** seam (REQ-026 — built to excise a *corrupt ingest*).
- **Add `erase_source(source_id)`** — a **first-class SDK lifecycle verb**, alongside `transition`/`purge`. Not a
  recovery name ⇒ **AC-041 stays GREEN, the denylist stays five, the byte-frozen guardrail is untouched.**
  **One shared engine code path** with `excise_source` — no second implementation to drift.
- **REQ-037 prose amended** (carve-out); `purge_logical_id` **struck** from its forbidden list — shipped code
  already contradicted it. **The amendment records reality rather than changing it.**

---

## 3. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

Tracked by **requirement id + TDD test name** (per the locked-`acceptance.md` policy) — and per that policy,
**prefer requirement id + TDD test name over minting a per-feature AC at all.** **New ACs are permitted:
0.8.20 Slice-0 IS a gated slice.**

> **⚠ AC MINTING FLOOR — `AC-079`. Do NOT mint by "max AC id + 1".**
> The highest **defined, non-reserved** AC is **AC-076** (`dev/acceptance.md:1147`). **AC-077**
> (`dev/acceptance.md:1286`) is a **RESERVED PLACEHOLDER** for the agentic-IR **IR-1/IR-2** initiative
> ("not yet a gate; no fabricated numbers"), and **AC-078 is conditionally reserved to the same
> initiative** (`:1297` — "+ AC-078… only if the consensus splits the measure").
> **0.8.20 mints from `AC-079`** *(HITL-ratified 2026-07-19)*.
> **The trap:** a naive grep for the maximum `AC-0NN` returns **AC-078** *from that reservation prose*, so
> "max + 1" silently collides with a live reservation. Read `dev/acceptance.md:1286-1300` before minting.
> *(Slice-0 finding **TC-14**; the earlier "AC ceiling today = AC-077, continue from it" text was **wrong**.)*

### Phase-2 (A)

| ID | Requirement | Acceptance signal (falsifiable, offline) |
|----|-------------|------------------------------------------|
| R-20-RV | `ReadView`/read-modes: composable relax-flags, uniform on the **five** read verbs — **`read_get`, `read_get_many`, `read_list`, `read_list_filter`, `graph_neighbors`** *(CORRECTED at Slice 10: the earlier "`get`/`list`/`neighbors`" shorthand named no real symbol)* | read-mode matrix test; `include_superseded` returns history; default view unchanged (no silent behavior drift) · **CLOSED at Slice 10** |
| R-20-NV | Node-validity `valid_from`/`valid_until` + `valid_as_of` (bound-`:now` seam) | validity-window matrix; `crossed_boundary_since` hook; world-time only (`history_as_of` explicitly OUT) · **CLOSED at Slice 10**, with **TC-34** open (no write-side authoring verb — §11 of the STATUS board) |
| R-20-PR | Projection registry (C-1): `configure_projections(spec, drop?)` idempotent diff + backfill; engine is **sole** authority; incompatible change ⇒ destructive delta requiring explicit `drop` | re-registration is a no-op; role add/remove builds/drops exactly; boot re-derive is crash-safe + idempotent |
| R-20-EAV | EAV / property-FTS — the store the registry projects from | property-level filter/search green; body-FTS behavior unchanged |
| R-20-DR | `dense_readiness` + `flush_embeddings()` + atomic readiness-flip | readiness never reports ready with pending embeds; flip is atomic under concurrent write |
| R-20-SUR | Surrogate minting serves **ONLY** registry-admitted governed entities; decided **at write time** | **migration-guard: rows transitioning `logical_id NULL → NOT NULL` == 0** (the pin's invariant); registering a kind does **not** alter any pre-existing row's `IdSpace` · **CLOSED at Slice 25** (`83b1c818`) — enforcement + proof, not a new mechanism: the governed write-time path already existed and no anonymous surrogate is minted anywhere (pin A) |

### Erasure completeness (B)

| ID | Requirement | Acceptance signal — **assert on RAW TABLE CONTENTS, not search results** |
|----|-------------|------------------------------------------|
| R-20-E1 | **ONE** row-owned projection registry + a **total projector (node + edge)**, consumed by `purge_inner`, `excise_source_inner`, `rebuild_shadow_state`, and `reproject_search_index_after_tokenizer_upgrade`. All five hand-rolled lists deleted. | `guard_row_owned_registry`: introspect `sqlite_master`; **every** `write_cursor`-keyed table is registered — a new projection table cannot be added without failing this test. Post-excise: `SELECT count(*) FROM search_index_v2 WHERE write_cursor=?` **= 0**. Post-rebuild: `search_index_v2` row-count == active canonical node count; **edge** FTS + edge vectors match the write path. |
| R-20-E2 | Ingest provenance comes from the **caller** (`ExtractDocument.source_doc_id`, `:1934`, already serialized into the prompt at `:3598`) — **never** the model's JSON echo (`:3644`, `:3771`) | an extractor that **omits** `source_doc_id` still yields **excisable** rows |
| R-20-E3 | Provenance is **structurally mandatory** on public writes: `SourceId` newtype replaces `source_id: Option<String>` (`:2024`, `:2051`). "No provenance" is **inexpressible** on the public type — **not merely rejected** (a validation-only fix leaves a Rust-facade hole: `fathomdb/src/lib.rs` re-exports `PreparedWrite` and `Engine::write` is public `:3364`). Engine-derived rows bypass `PreparedWrite` and take a reserved `_engine:*` provenance. | Rust/Py/TS: an un-provenanced public write **does not compile / raises**; **no canonical row has NULL `source_id`** post-change |
| R-20-E4 | **`erase_source(source_id)`** — first-class SDK lifecycle verb (Py + TS + Rust facade, X1 parity); one engine path with `excise_source` | an **SDK-only** consumer (no CLI on `PATH`) erases anonymous content end-to-end; **AC-041 still GREEN** |
| R-20-E5 | Erasure covers the **WAL**. `truncate_wal()` **already exists** (`:6379`, typed `Busy` status) but `purge`/`excise` **do not call it** | post-erasure the raw `.db` **and `-wal`** bytes do not contain the erased body; **`Busy` ⇒ typed `ErasureIncomplete` + non-zero CLI exit — an erasure verb must NEVER report success on an incomplete erasure** |
| R-20-E6 | Telemetry retains `l:`/`h:` ids after erasure (`result_stable_ids`, `:4462`); `logical_id = SHA256("{kind}:{name}")` over a **low-entropy natural key** is dictionary-attackable | `purge`/`erase` **selectively redact** the sink (drop records referencing erased ids) — **not truncate**: the sink path is **caller-supplied**, so truncation would destroy unrelated operator eval history. Purged id absent; **unrelated records survive.** |
| R-20-E7 | Op-store records are erasable: `excise_collection_record(collection, record_key)` (no record-level delete exists — only a cap sweep, `:10083`) | app-authored op-store payload erasable by key |
| R-20-E8 | Legacy NULL-provenance rows become erasable | **CORRECTED AT SLICE 5 (TC-26) — the gate is NODE-ONLY, and the shipped step-21 migration is deliberately ASYMMETRIC.** `canonical_nodes`: `WHERE source_id IS NULL AND logical_id IS NULL` — governed nodes keep NULL and stay `purge`-addressable by `logical_id`, so stamping them would make them collateral of an `excise_source('_legacy:pre-0.8.20')` aimed at anonymous rows (the over-erasure the TC-11 pin forbids). `canonical_edges`: `WHERE source_id IS NULL` **alone** — an edge's `logical_id` is only a *supersession* identity and `purge_inner` resolves targets **exclusively** via `canonical_nodes` (`:6556`/`:6580`/`:6596`), so a legacy edge with `source_id IS NULL AND logical_id IS NOT NULL` would be erasable by **no verb at all**, defeating this very requirement. ~~The earlier unqualified "WHERE `logical_id IS NULL` ONLY" rule~~ was correct for nodes and **wrong for edges**. **TC-11's pin is unaffected** — step 21 writes only `source_id`, never `logical_id`. `doctor orphan-provenance` lists per-`source_id` counts. |

### Release / gates (C)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-20-H7 | **RUBRIC-H7 GATE (TC-RUBRIC-2)** — a **Pact-style `can-i-deploy`** mechanical contract-conformance check: as-built code still satisfies the ratified `OPP-12-C1-converged-contract.md` at the co-land. **Not humans re-reading prose.** | Gate exists and is GREEN. **An absent-or-failing gate HOLDS the breaking pair** (hard, HITL-directed 2026-07-10) |
| R-20-X1 | SDK parity (Py + TS) — **live functional harnesses, not symbol presence** | X1 GREEN. **Run parity BEFORE land, same unit** (rubric G6 carry: 0.8.19 ran X1 green *after* land via a native-import env trap — **treat an env trap as a landing blocker, not a follow-up**) |
| R-20-EU7 | **eu7 basis decision** (F-22) — ✅ **CLOSED BY HITL DECISION 2026-07-19, NOT BY MEASUREMENT** | **ZERO eu7 runs are authorized — on ANY backend, at ANY N.** No GPU run, no CPU run, no reduced-N run, no smoke run, **no "confirmation" run.** The basis is settled on existing evidence and **needs no gate execution.** See the closure note below. |

> **🚫 R-20-EU7 — CLOSED BY DECISION. RUNNING eu7 IS FORBIDDEN, NOT MERELY UNNECESSARY.**
> *(HITL, 2026-07-19 — decider named per the "name the decider" rule.)*
>
> **The ruling.** R-20-EU7 is **closed by HITL decision, not by measurement.** **ZERO eu7 runs are
> authorized: not GPU, not CPU, not reduced-N, not a smoke run, not a confirmation run.** HITL verbatim:
> *"no one run. it always turns into 10 runs and 3 days and token burn only for it to become a HITL decision
> anyway. I did already."*
>
> **The basis (settled on existing evidence).** CPU and GPU are **equivalent** — 0.8.7 proved the CPU↔CUDA
> embed path **bit-identical**. Therefore the **0.8330** GPU eu7 figure
> (`dev/plans/runs/0.8.14-slice-20-eu7-gpu-run-20260705T205222Z.log:73`) was a **cross-backend comparison
> artifact** measured against a CPU reference (0.8960, CI [0.8640, 0.9250] ⇒ PASS) — **not a regression and
> not a backend defect.** Nothing about OPP-12 disturbs this: pin A keeps `SearchHit.id` byte-identical
> (`to_prefixed()` == prior `stable_id`) ⇒ **no gold-remap**, so the no-op basis holds on the argument.
>
> **To a future reader or agent:** do **not** schedule, price, or propose an eu7 run to "confirm" this —
> proposing a cheap confirming run on a question the HITL has already decided **re-opens a closed decision**
> and is the exact measurement-spiral this ruling guards against. **Close the item and route around it.**
>
> **Consequently closed / withdrawn:** **TC-19** (the CPU harness cannot drain `BATCH = 256` inside its
> hardcoded 600 s at 0.179 docs/s) is **CLOSED by this routing, not by a fix** — the `BATCH 256→64` proposal
> is **WITHDRAWN**, and **investigating the CPU throughput shortfall is EXPLICITLY FORBIDDEN** (HITL: *"we
> understand it"*). **TC-12**'s "capture on CPU only" recommendation is **OVERRULED on the merits** — it was
> right that *cross-backend* comparison is invalid, but the equivalence makes the whole capture moot.
> **TC-13**'s harness hazards (the documented `--features` invocation is wrong — needs
> `default-embedder,operator`; the gitignored corpus makes a worktree run a **vacuous skip-exit-0**) remain
> **recorded as knowledge** for whoever eventually touches that harness — they are **NOT scheduled work.**
| R-20-PUB | Coordinated breaking-pair publish; manifests **`0.8.9 → 0.8.20`** | Publish executed exactly per the **separate HITL gate** (F-21). Uses 0.8.18 #11-full machinery (proven, never fired). Pairs with Memex `0.5.x-successor` |
| R-20-AC | Governed-surface delta signed | **new AC (`AC-079`+ — see the minting-floor warning in §3; NOT AC-077/078, which are reserved)** mirroring AC-074: the Phase-2 + erasure API delta vs the conformance allowlist, **HITL-SIGNED**. `recovery_denylist` **unchanged (stays five)** |

### Reserved-gap requirements (D) — minted 2026-07-27

Added by the HITL scope rulings of 2026-07-27 (steward `seq-117`/`seq-119`). Both ride **reserved gaps in
the 20 band**, which §5 pre-authorizes; neither changes the mod-5 spine, the release theme, or any I-edge.

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-20-CR | **Concurrency + test-oracle repair** (Slice 21). Three legs: **TC-57** — the governed-write vs projection-worker race (`EngineError::Storage`, 7 of 8 baseline runs with `logical_id Some`, never with `None`, suspected `SQLITE_BUSY_SNAPSHOT`) is **characterized FIRST, then fixed in-release** (`seq-111`); **ac_002** — replace the PWD/HOME/XDG/TMPDIR substring scan with a **per-test sandbox**, fixing by isolation rather than detection (Part 1, the allowlist inside the DB parent, is hermetic and is KEPT); **TC-71** — `vector_projection_declared` must require the `searchable` role, not `vector_declared` alone. | TC-57 reproduced under a written characterization before any fix is scoped, then GREEN. ac_002 asserts against the sandbox, not the substring. TC-71: a RED test that runs **with a live embedder** (the shipped inertness test passes vacuously without one), re-verified across **all three** paths the predicate gates — forward backfill, drop inverse, late enrolment. |
| R-20-VC | **Vector-arm consumer contract** (Slice 22). Four legs: **TC-67** — declaring a `searchable→vector` projection over a kind the vector writer cannot commit must **REPORT**, per HITL option (c); the `resolve_source_type` vocabulary is **NOT grown** and the Pack-1 D3 partition-key lock is **not touched**; **TC-68** — cache the 0.8.18 equivalence probe against an **embedder-identity fingerprint** so `Engine::open` cost is constant again, preserving the guarantee rather than weakening it; **decision #18** — settle the `InvalidArgument` vs `WriteValidation` family inconsistency before the surface is signed; **sqlite-vec #99 probe** — exercise `excise_source`/`purge` against a `vec0` row whose `kind` or `attr_*` value exceeds 12 chars and record whether 0.1.7 reports a spurious `DELETE` error. | TC-67: a typed report, not silence; readiness semantics unchanged. TC-68: open cost independent of enrolled-kind count, with the probe still running on an identity change. #18: one family, tests updated. #99: a written finding either way — it informs whether `sqlite-vec` should move out of the 0.8.22 placement (**TC-76** re-open trigger). |
| R-20-SV | **Spec-validation reject + carried-defect characterization** (Slice 23). Two legs: **the `fts`/`vector` reject** — an `fts` or `vector` sub-object declared **without the `searchable` role** is an **invalid spec** and is REJECTED, replacing today's accept-and-round-trip; ruled **2026-07-24** (§11 item 4, *gated non-blocking at "the next `configure_projections` slice"*), landed here rather than in 22 because the reject needs an **error** and decision **#18** settles that family in 22 — the reverse order would pick a family #18 may change. Also corrects `dev/interfaces/rust.md:352`, which still cites the **overruled** accept-inert position as precedent (**TC-39** class). **TC-90/TC-91 characterization** — owed by `seq-136` (*characterize in 0.8.20, fix at 0.8.21*), which left it without a slot: **TC-90** `Engine::transition`'s read-then-upgrade under `BEGIN DEFERRED` (the TC-57 class at a second site) and **TC-91** the ~50% baseline duplicate-embed rate + silently-discarded worker commit failures. | Reject: a typed error on the invalid shape, round-trip preserved for valid specs, all four `dev/interfaces/*.md` updated (**AGENTS.md:25**). Characterization: a **written** repro protocol with numbers for TC-90 and TC-91 — **no fix**, per `seq-136`; TC-57's own lesson is that a concurrency fix scoped before measurement ships inert. |

> **Slices 31, 32 and 33 (Library Sweep #3) deliberately carry NO requirement id** — **TC-76**, HITL-ruled
> 2026-07-27 and re-affirmed for all three legs at `seq-153`. It is **dependency hygiene, not a release
> requirement**, and master **F-12** treats a Library Sweep as a **pico** (label-only, never published).
> Placing this one **in-ladder before publish** is a **deliberate departure** from that precedent, recorded
> here so a later Steward does not silently overwrite F-12 by following this instance. Minting `R-20-LIB`
> was declined: it would convert a recurring hygiene programme into a one-off requirement of 0.8.20, the
> category error F-12 exists to fix.

### 3a. Library Sweep #3 — RE-SCOPED to a TOOL (HITL 2026-07-29, steward `seq-153`)

**What Slice 31 was before this ruling: nothing enumerated.** No doc ever listed its contents. It inherited
the **`LIBRARY-BUMP-STEWARD.md` §1–2** manual loop by reference — *"union of open Dependabot PRs, a fresh
`cargo upgrade --dry-run` / `npm outdated` / equivalent, and `gh api …/dependabot/alerts`"*, then a
five-question triage per candidate. The only item ever *named* against 31 was **TC-76**, which moved to
**0.8.22** at `seq-151`.

**The HITL replaces that manual sweep with a tool**, expressly so the exercise is **~90% mechanical and
automated** rather than a token-burning search → review → check → reason loop. A small, tight, **isolated
mini-project under `scripts/`** with its own requirements, acceptance criteria, design, tests and code.
It produces a **CycloneDX-JSON SBOM** over every tracked manifest, enumerates **library↔library**
dependencies, and **diffs used-versus-published** versions. External libraries are permitted and **must be
named in the design**.

**Three legs, one per slice.**

| Leg | Slice | Deliverable | Gate |
|---|---|---|---|
| 1 | **31** | requirements · acceptance criteria · design · **RED tests**. **No code.** | **codex §9 reviews the req/AC/design; FIX-*n* complete before close.** No X0-style HITL sign-off |
| 2 | **32** | the code that turns 31's RED tests GREEN. **Code only** | codex §9 terminal-clean |
| 3 | **33** | **runs** the tool; writes the survey | codex §9 terminal-clean |

**Slice 33's scope guard — the one most likely to be violated.** 33 answers two questions and stops:
**(1)** what is actually stale across the Cargo / npm / Python (and any other tracked) manifests; **(2)**
whether a **surgical change of roughly 1–5 SLOC** would *likely* land each upgrade. **(2) is ASCERTAIN, NOT
IMPLEMENT.** Anything larger than surgical is **merely NOTED for the project itself to handle** — never
attempted, never scoped further inside 33.

**Hard guardrails binding all three legs.**

- **No leg applies a dependency bump.** **No manifest and no lockfile changes inside 0.8.20.** The survey is
  an **INPUT to 0.8.22**, which already owns `napi 2→3`, `rusqlite 0.31→0.40 + sqlite-vec`, the
  `sqlite-vec 0.1.9` bump (`seq-151`) and the six Dependabot advisories (`seq-152`).
- **Python**; **CycloneDX JSON**; the mini-project's **own manifests stay isolated** from the shipped and
  root dependency trees, so it can never enlarge the published graph or the advisory backlog.
- **Generated reports are GITIGNORED.** ⚠ Slice 33's *findings* therefore need a **tracked** durable home —
  the raw tool output is not one.
- **Recurring by design, NOT CI-gating.** It is an **informational** tool (F-12's recurring-programme shape),
  and wiring it into CI is explicitly out of scope.
- Every leg is **BOM scope = everything tracked on `main`**, with each component tagged by tier —
  `shipped` / `dev-tooling` / `eval-only`. That tagging is what made TC-93 a cheap call.

---

## 4. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 25 → [30] → 21 → 22 → 23 → 31 → 32 → 33 → DOC-HYGIENE-3 → ⟨batched surface decision⟩ → 40
                              ↑ publish precondition                  ↑ Library Sweep #3: spec → code → run
```

**Execution order ≠ numeric order, and that is deliberate** (HITL-approved 2026-07-27, steward `seq-119`).
**Slice 30 runs FIRST** of the remainder: it is the publish precondition, it is fully specified, and it is
the slice most likely to surface a contract-conformance failure that would reshape everything downstream —
so it is worth discovering before three slices of hygiene, not after. 21 and 22 are **reserved-gap** slices
in the 20 band (§5) and gate nothing. **DOC-HYGIENE-3 is not a ladder slice** — it is cross-cutting and
carries **no pico label** (`seq-106`; DOC-HYGIENE-1 precedent, F-33). Execution is **SEQUENTIAL**.

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | **X0 design gate** — reqs/ACs frozen; erasure Slice-0 design; **TC-11 pin ✅ ALREADY RATIFIED**; eu7-basis + embed_batch_cls-TS decisions; **TC-RUBRIC-5** (dedicated-checkout/`preflight.sh --landing`) folded in; stand up `runs/STATUS-0.8.20.md`; **codex §9** | design-adr + steward-review | — |
| **5** | **Erasure completeness** (R-20-E1…E8) — registry + total projector + `erase_source` + provenance + WAL + telemetry + op-store + migration | implementation | 0 |
| **10** | **`ReadView` / read-modes** + **node-validity** (R-20-RV, R-20-NV) — ✅ **COMPLETE on-branch @ `93a57b10`** (SCHEMA 21→22) | implementation | 0 |
| **15** | **Projection registry (C-1 co-land) + EAV/property-FTS** (R-20-PR, R-20-EAV) **+ TC-34 node-validity write-side authoring verb + TC-33 temporal-representation harmonisation** *(both folded in by HITL 2026-07-20)* | implementation | 0, 10 |
| **20** | **`dense_readiness` + `flush_embeddings()`** (R-20-DR) | implementation | 15 |
| **25** | **Surrogate minting — registry-admitted governed entities ONLY** (R-20-SUR) — ✅ **LANDED `83b1c818`** | implementation | 15 |
| **30** | **RUBRIC-H7 `can-i-deploy` contract-conformance gate** (R-20-H7) — ⚑ **RUNS FIRST of the remainder** | implementation | 10,15,20,25 |
| **21** | **Concurrency + test-oracle repair** (R-20-CR) — TC-57 characterize→fix · ac_002 oracle replacement · TC-71 | implementation | 20 |
| **22** | **Vector-arm consumer contract** (R-20-VC) — TC-67 (c) · TC-68 fingerprint-cache · decision #18 · sqlite-vec #99 probe | implementation | 15, 20 |
| **23** | **Spec-validation reject + carried-defect characterization** (R-20-SV) — the `fts`/`vector` reject (ruled 2026-07-24) · TC-90/TC-91 characterization (no fix, `seq-136`) | implementation + investigation | 22 (needs #18's error family settled) |
| **31** | **Library Sweep #3, leg 1/3** — SBOM/dependency-survey tool: **req + AC + design + RED tests, NO code**; **NO requirement id** (TC-76 precedent), a deliberate F-12 departure | design + RED tests | — (sequenced after 30 by ruling, no technical dep) |
| **32** | **Library Sweep #3, leg 2/3** — implement the tool against 31's RED tests. **CODE ONLY**; applies no bump, edits no manifest or lockfile | implementation | 31 |
| **33** | **Library Sweep #3, leg 3/3** — **RUN** the tool: what is stale across every tracked manifest, and whether a **surgical ~1–5 SLOC** change would likely land each upgrade — **ASCERTAIN ONLY, NEVER IMPLEMENT**. Output is an **input to 0.8.22** | investigation | 32 |
| **40** | **Verification + release readiness** — full DoD, X1, **AC-079 sign-off** (R-20-EU7 is **CLOSED — run NO eu7**, see §3), **TC-16 determination FIRST** (`seq-118`), **publish-or-hold per the HITL gate** | verification | 5,30 |

**Keystones / hard gates.**

- **Slice 0 blocks everything** (X0 process gate, carried from 0.8.18 §5 / 0.8.19).
- **Slice 5 is INDEPENDENT of Phase-2** — it fixes **defects in shipped code** and can run fully parallel to
  10/15. It does **not** wait on the registry.
- **Slice 15 is the Phase-2 keystone** — 20 and 25 both depend on it.
  **⚠ PARTIALLY COMPLETE (2026-07-20).** Only **TC-34** has closed (on-branch @ `a8087dfb`, with the
  search-validity coherence fix). **R-20-PR, R-20-EAV and TC-33 are NOT STARTED — no code exists.**
  **20 and 25 stay BLOCKED until R-20-PR lands**; TC-34 closing does **not** unblock them. See
  `runs/STATUS-0.8.20.md` §13.
- **Slice 30 (H7) is a PUBLISH PRECONDITION.** Absent-or-failing ⇒ **the breaking pair HOLDS.**

**Tracks (parallelizable).** `5 ∥ 10 ∥ 15` after Slice 0. Slice 5 touches the erasure/projection paths; 10/15
touch read + registry. They share `engine/src/lib.rs` ⇒ **serialize the merges** (rebase-then-merge one at a
time). **One `maturin develop` at a time** (shared `.venv` mutex).

---

## 5. Reserved-gap policy

Gaps `1–4, 6–9, 11–14, 16–19, 21–24, 26–29, 31–39` absorb unplanned follow-on. Fully orchestrated, not ad-hoc.
**HALT to HITL on band overflow** — never spill into the next mod-5.

**Band occupancy (updated 2026-07-28, `seq-139`).** The **20 band** now holds **21** (R-20-CR, landed),
**22** (R-20-VC) and **23** (R-20-SV) — **three of four slots used, ONE remaining**. The **30 band** (gaps
**31–39**, nine slots) holds **31**, **32** and **33** — Library Sweep #3's three legs (`seq-153`) —
**three of nine used, SIX remaining** (31, 32, 33) — Slice 34 was minted at `seq-178` and **CANCELLED at `seq-182`**.

**The tripwire stays at band overflow — TC-77, HITL-ruled 2026-07-27, and it is CONDITIONAL.** Tightening
it to halt on the *next* new slice was declined: the band was sized as slack deliberately, and halting
earlier spends HITL attention on exactly what this section exists to handle without them. The growth so far
came from HITL rulings, not from drift. **RE-OPEN TRIGGER — the Steward raises this unprompted:** if Slice
21 or Slice 22 spawns **two or more further slices**, the tighter tripwire becomes correct and this returns
to the HITL.

> **TRIPWIRE STATUS (2026-07-28, `seq-139`) — raised unprompted, as the trigger requires. NOT TRIPPED.**
> **Slice 23 (R-20-SV) is ONE further slice**, carrying the `fts`/`vector` reject and the TC-90/TC-91
> characterization that `seq-136` left without a slot. The trigger needs **two or more**. The 20 band is now
> at **three of four slots, one spare**. **If anything spawns a SECOND further slice, the tighter tripwire
> becomes correct and this returns to the HITL** — that is the next thing the Steward escalates here, and it
> is not conditional on anyone asking.

---

## 6. Cross-cutting DoD (X0/X1/X2/X3 — bind EVERY slice)

- **X0 — elevated process gate.** (A) reqs + **RED-testable** ACs → (B) **independent design review** → HITL
  sign-off, **before code**. Carried from 0.8.18 §5.
  **+ TC-RUBRIC-5 (HITL-ADOPTED 2026-07-11):** release orchestration and **all landing git-writes run in a
  dedicated linked worktree**; `scripts/preflight.sh --landing` **HARD-fails on the primary checkout**.
  **+ TC-RUBRIC-7:** persist **every codex §9 transcript** to a durable release-namespaced path.
- **X1 — SDK parity.** Py + TS equivalence via **live functional harnesses**. **Parity runs BEFORE land, as one
  unit** (rubric G6). An env trap is a **landing blocker**, not a follow-up.
- **X2 — `mkdocs build` green** for any `docs/` touched.
- **X3 — docs/changelog per slice + `dev/DOC-INDEX.md`.** This release ships **real** changelog lines (breaking).
- **Full-workspace gate.** `cargo clippy --workspace --all-targets` **and** `cargo check --workspace
  --all-targets` **both exit 0** before any green claim (per-crate verify masks cross-crate breaks).

---

## 7. Prerequisites

1. **Slice-0 X0 sign-off** recorded. *(TC-11 pin is already ✅ ratified 2026-07-12 — do not re-open.)*
2. **Dedicated worktree** off a verified `origin/main` tip. **Never the primary/shared checkout** (TC-RUBRIC-5).
3. **`0.8.18 #11-full` publish machinery** ✓ proven + exercised via staging — **never fired**. The 0.8.20 cut is
   its first real firing: **rehearse the tag→publish path before the HITL gate.**
4. **Baseline captured** — FTS/vector numbers + X1. **~~eu7 recall~~ — STRUCK (HITL 2026-07-19):** R-20-EU7 is
   **closed by decision**, so **no eu7 baseline is required and no eu7 run is authorized** (§3). This prereq was
   listed as *assumed* and was never actually met — Slice-0 found no baseline existed (**TC-19**); it is now moot.
5. ~~**Memex `0.5.x-successor` co-land readiness** confirmed (breaking **pair** — one side alone is not a
   release).~~ **✅ CLOSED BY DECISION (HITL 2026-07-25) — no Memex confirmation is required and none is to be
   sought.** Ruling: *Memex readiness completes after Slice 30, does not need review from Memex, and Memex will
   adjust to the surface of 0.8.20.* The publish therefore no longer waits on a cross-repo confirmation the
   FathomDB side cannot perform. **Do not re-introduce a "verify with Memex" step** — closing a decision and
   then scheduling a confirming check is the failure this repo has already ruled against. **Push scope is
   unchanged and absolute: fathomdb-only.** Signalling this ruling to Memex would require a separate, explicit
   per-push HITL directive and is NOT authorized by this closure. **Publish itself remains a hard HITL gate**
   (§ 11 item 3) — this closure removes a prerequisite, not the gate.

---

## 8. Out-of-band / parallel notes + key callouts

- **`13` remains HITL-forbidden** as minor and micro.
- **Publish ≠ build.** F-21 authorizes the *build*. The **publish is a separate explicit HITL call** on this
  `x.y.z`, and it is a **coordinated pair** with Memex.
- **First manifest bump in the line.** Everything since 0.8.9 was label-only. **`0.8.9 → 0.8.20`** touches every
  crate/py/npm manifest — use `scripts/set-version.sh`; cargo publish order is **embedder → engine**; a pushed
  `v*` tag **auto-fires REAL crates/PyPI/npm publish** ⇒ **dry-run first**.
- **Erasure is currently INCOMPLETE and UNREACHABLE.** Until Slice 5 lands: `excise_source` leaves the body in
  `search_index_v2`; the telemetry sink retains ids; op-store payloads are un-erasable; and **no CLI ships to SDK
  consumers**. **FathomDB MUST NOT be represented as GDPR-erasure-capable until 0.8.20 ships.**
- **`source_id` MUST NOT contain PII** — **the rule STANDS; its ORIGINAL RATIONALE WAS FALSE.**
  ~~It is retained **permanently** in the `excise_source_audit` row (`:6479`), i.e. in the record of the
  erasure itself.~~ **VERIFIED FALSE at Slice 0 (TC-15):** the audit row is written into
  `operational_mutations` (`engine lib.rs:6479`), and `enforce_provenance_retention` (`:10070`) sweeps **that
  same table** cap-first / oldest-id-first with **NO collection filter** (`:10083-10089`) — so the audit row is
  **destructible**, and the erasure record shares one retention pool with the very payloads it must prove
  erased. The **non-PII rule still holds** (an *unswept* audit row may still carry `source_id`, and it may
  outlive the subject's data), but it was **argued from a false premise**. Document at the write surface —
  and justify it on the surviving grounds, not on "permanent retention".
- **THE ERASURE AUDIT TRAIL MUST BE DURABLE** *(HITL ruling, 2026-07-19: **"there must be an auditable record
  of deletion event"**)*. Demonstrating **that** an erasure occurred is an obligation **distinct** from
  performing it, and a retention sweep must not silently destroy the proof. **Slice 5 EXEMPTS the
  erasure-audit collection from `enforce_provenance_retention`** (filter the sweep by collection) and **states
  the audit-durability guarantee explicitly**. This is a **retention-policy change**, not a pure defect
  repair — hence HITL-decided. Note the collision it resolves: work-item 9's `excise_collection_record`
  operates on the **same** op-store table the audit rows live in.
- **Outside the erasure boundary** (enumerate + disclose, do not silently omit): `safe_export` archives,
  operator backups, curated gold files. **Re-generate or destroy after any erasure.**

---

## 9. Immediate next slice

> ✅ **THE POINTER BELOW IS A GENERATED VIEW (TC-89, closed in `DOC-HYGIENE-3`).** It is rendered from
> `release-state-0.8.20.json` — `next_slice`, that slice's ladder `short`/`title`, and `remaining_ladder` —
> by `scripts/check-release-state-views.sh`, which hard-fails on any drift or hand-edit.
> **NEVER EDIT INSIDE THE MARKERS.** Change the fact in the state file, then run
> `scripts/check-release-state-views.sh --write`.
>
> It went **stale at three consecutive commissions** while it was hand-written, and
> `commission-manifest.sh` cites this section as a brief's **`{{MANDATE}}` anchor** — its CHECK 2 verifies
> that the *heading* exists, not that the prose under it is current — so the staleness was *copied into the
> next commission*. Everything **outside** the markers (deliverables, guardrails, the historical roll-up)
> stays hand-written and is **not** under generation.

### ▶ Immediate next

<!-- BEGIN GENERATED release-state:0.8.20:plan-immediate-next -->
**IMMEDIATE NEXT: Slice 39** (`R-20-DOC`) — Publish-facing documentation: MIT license reconciliation + a LICENSE that actually ships, the 0.8.20 CHANGELOG section, registry-facing READMEs, docs/ de-stale, dev/interfaces currency, docs.rs doc-comment corrections

**Remaining ladder:** 39 → 40.<!-- END GENERATED release-state:0.8.20:plan-immediate-next -->

> ⚠ **This prose block previously described Slice 31 and was inherited verbatim by every brief
> generated from this anchor.** It carried the line *"Applies no dependency bump. Edits no manifest and
> no lockfile"*, which directly contradicts the manifest work Slices 39 and 40 must perform. Rewritten
> 2026-07-30 by the Steward. Slices 31/32/33 have all LANDED; their guardrails are history and are
> recorded in §3a, not here.

**Slice 39 (`R-20-DOC`) is the immediate next.** Minted 2026-07-30 under the HITL directive to get
project / `dev/` / `docs/` documentation **not stale**, and the **MIT license ruling** (steward `seq-193`).
**Depends on Slice 30** (LANDED — the publish precondition is **SATISFIED**).

**Why it precedes Slice 40:** `scripts/verify-release-gates.sh` **check 4 hard-fails** without a CHANGELOG
heading matching the version, and no `## 0.8.20` section exists. Slice 40's `workflow_dispatch` rehearsal
therefore **cannot pass** until Slice 39 lands.

**Deliverables.** The four publishable manifests corrected **Apache-2.0 → MIT** to agree with the repo-root
`LICENSE`; a license file that **actually ships** in all 7 crates, the wheel and both npm packages (none
does today); a real **`## 0.8.20` CHANGELOG section** — including the `SearchHit.id` `u64`→`IdSpace` break,
which is absent entirely, and a correction of the false *"Schema version 20 → 21"* line (the real span is
**15 → 24**); registry-facing **READMEs**; `docs/` de-staled off 0.6.0; `dev/interfaces/*.md` currency
(**TC-39**); and the docs.rs doc-comments that contradict shipped behaviour.

**Guardrails.** **Changes `license` fields only — never a `version` field**; the `0.8.9 → 0.8.20` bump
belongs to Slice 40. **Nothing in `.github/`** (Slice 40's exclusive territory this release). Governed
surface **byte-identical** (`check-governed-surface-pin.sh` exit 0), **SCHEMA stays 24**, **zero eu7 runs**
(F-28). **Carries no `R-20-xx` requirement id and mints no AC** — gate by test name and by the new
license-consistency guard.

**Then Slice 40 (`R-20-PUB`)** prepares the publish and **STOPS before any tag**. Publish itself remains
the single unruled HITL decision.

**Slice 40 additionally carries the DISPATCH GUARD** (HITL 2026-07-30, steward `seq-198` ruling 1).
`.github/workflows/release.yml` must be hardened so a `workflow_dispatch` with `dry_run=false` **cannot
publish on one unchecked checkbox**: add a **required confirmation input whose value must literally match
the version being released**, and make `scripts/verify-release-gates.sh` **exit 1 — not warn** — when a
dispatch has `dry_run=false` without it. Measured at HEAD: the only thing guarding that path is the
`dispatch with dry_run=false is an emergency-republish path` warning in `scripts/verify-release-gates.sh`
(lines 58-61 at the time of the ruling), which **prints and continues**, **and the whole tag-format branch
is skipped on dispatch** — so the real publish step, the no-dry-run arm
`bash scripts/release/cargo-publish-if-new.sh fathomdb-embedder-api` in `.github/workflows/release.yml`
(line 261 at the time of the ruling), is reachable **with no tag at all**. Per the standing rule, this is
fixed in the repo rather than in brief wording.
**Slice 40 remains the only slice permitted to touch `.github/` this release.**

⚠ **crates.io `categories` is OUT OF 0.8.20** (`seq-198` ruling 2). Registry **`keywords` STAY in 0.8.20**,
because `cargo package` validates them **locally** (max 5, each ≤ 20 chars); **`categories` slugs are
validated SERVER-SIDE ONLY at the real publish**, **mid-tier**, after earlier crates have already uploaded
**immutably** — the **v0.8.9 partial-publish shape**. `categories` is **re-entered at 0.8.21** (odd ⇒ OOB,
label-only) so the metadata is ready for the **0.8.22** publish. *Steward note: `seq-198` does not say which
slice owns the `keywords` edit, and neither Slice 39's deliverables above nor `R-20-PUB` names registry
metadata today — the ruling drops `categories` from a scope that lived only in the draft Slice-39 brief. If
`keywords` are to ship at 0.8.20 the owning slice must be named explicitly.*

---

### Historical — the landed slices this section used to point at

**✅ SLICE 20 IS COMPLETE — Slice 20c LANDED at `841c307b`; R-20-DR is CLOSED.** Slice 20b had landed
**TC-45** (supersession terminal) and **R-20-DR part 1** (`DenseReadiness {Ready, Embedding}` derived onto the
`ProjectionSpec.vector` sub-object + the atomic flip) at `26b237c0`. **Slice 20c landed the remaining leg —
the flush-to-readiness barrier.**

**It shipped by REUSING the shipped `drain`, per `api-surface.md` C4 — there is NO `flush_embeddings()` verb.**
**ZERO net-new governed commands**; the allowlist is **byte-identical**, `check-governed-surface-pin.sh` exits
**0**, and **SCHEMA stays 24**. TC-55 (= INSTRUMENTATION, steward `seq-110`) and TC-59 (one re-pin at the
batched decision, `seq-113`) were the rulings that unblocked it; **the pin never tripped.**

The defect closed: C4's rider — *"deferred/backfill rows must be enqueued on the same projection runtime
`drain` waits on"* — **was not true of the code**. `configure_projections` recorded vector work as `deferred`
and dropped it, so `drain()` returned `Ok` and readiness read `ready` **with no vectors**. The fix is entirely
on the **enqueue** side: `drain` remains a passive barrier (C4) and the shared
`connection_has_pending_projection_work` predicate was never restructured (TC-56 blast radius kept closed).

**Five codex §9 rounds, every one RED-first** (transcripts under `dev/plans/runs/codex/0.8.20/`): the drop
inverse; enrolment restricted to commit-able kinds (`kind_is_vector_committable` **delegates** to
`resolve_source_type` so the two cannot drift) + shared un-stranding; the no-embedder write made recoverable
via `ProjectionOutcome::Deferred` (**option R2** — R1 was **rejected** because it would have amended design
§4.1 invariant 1, *"a torn `ready`-without-vector is FORBIDDEN"*, and rewritten two shipped tests); the
dispatcher filter moved **inside** the scan SQL + late enrolment made crash-atomic.

⚠ **ONE codex finding was left OPEN at the 5-round circuit breaker — `TC-71`:** `vector_projection_declared`
keys off the stored `vector` sub-object alone, so `{roles:[filterable], vector:true}` activates the dense arm
instead of staying inert. **Wasted embed work; NOT a false-ready and NOT data loss.** The predicate gates the
forward backfill, the drop inverse **and** late enrolment, so a fix must re-verify all three together.

⚠ **CONSUMER-VISIBLE:** a no-embedder session over an **already-enrolled** corpus now leaves readiness at
`embedding` and `drain` times out into `SchedulerError` **for that session**. Loud, not silent, and **not data
loss** — the write is accepted, stays lexically searchable, and is embedded on the next embedder-backed open.
Documented in `dev/interfaces/{rust,python,typescript}.md` + `CHANGELOG.md`.

**Landed since the Slice-20 narration above:** **Slice 25** at `83b1c818`, **Slice 30** (R-20-H7
`can-i-deploy` contract-conformance gate) at `9b3ed0e3` — **the publish precondition is SATISFIED** — the
cross-cutting **⟨TC-86 transcript-hygiene fix⟩** at `0a500de9`, and **Slice 21** (R-20-CR — TC-57 write race ·
ac_002 sandbox oracle · TC-71 role-gated dense arm) at **`77be504b`**, close record board §18. SCHEMA is **24**.

> ⚠ **This pointer is HAND-WRITTEN and has gone stale at two consecutive landings** (`62486a01`, and again
> here). `scripts/commission-manifest.sh` cites this section as a brief's `{{MANDATE}}` anchor, so a stale
> pointer is copied into the next commission. **Re-verify it against
> `release-state-0.8.20.json` `next_slice` before commissioning anything.** Making it a generated view is
> **TC-89**, placed in DOC-HYGIENE-3.

**➡ IMMEDIATE NEXT: Slice 23** (R-20-SV — the `fts`/`vector` reject ruled 2026-07-24, now unblocked because
**#18 settled the error family onto `WriteValidation` in Slice 22**, plus the **TC-90/TC-91 characterization
ONLY, no fix**, `seq-136`). ⚠ **CORRECTION (`seq-143`):** an earlier version of this line said two HITL items were open *first*.
**Neither gates Slice 23.** `sqlite-vec` 0.1.9 (TC-76, trigger **FIRED**) is a Slice 31 / 0.8.22 dependency
and publish-risk question — **HITL is holding it**. **#18**'s family is already settled onto
`WriteValidation`, which this slice's reject uses; its one exception is an edge-epoch bounds check on a
different path — **TC-98, deferred by the HITL until after Slice 23**, and retrospective about Slice 22.

**Remaining ladder, HITL-approved 2026-07-28 (F-36 · steward `seq-129`), SEQUENTIAL — this SUPERSEDES the
F-35 order (`seq-119`); its leading `30`, `TC-86` and `21` legs are all now landed:**
**⟨batched governed-surface decision — DUE NOW, HITL⟩ → 31 → 32 → 33 → DOC-HYGIENE-3 → 40** (`seq-153`).
**21** (R-20-CR, landed) and **22** (R-20-VC) are reserved-gap slices in the 20 band; **31** is Library Sweep #3 and
carries no requirement id (TC-76); **DOC-HYGIENE-3** is cross-cutting and is not a ladder slice. The batched
surface decision is taken **AT THE CLOSE OF SLICE 23** (HITL `seq-141`, superseding `seq-134`/`seq-140`).
**23 is the LAST unit that can touch the surface:** the allowlist is SDK *command names*, and neither
Slice 31 (dependency sweep) nor DOC-HYGIENE-3 (docs/tooling) can add or remove one — so the delta is FINAL
at 23 and deferring the ceremony past them buys nothing while pushing an open gate against Slice 40, which
mints AC-079 pinned to that file. **Non-blocking for 31 and DOC-HYGIENE-3**; Slice 40 does depend on it.
Commission each as an **orchestrator** — **NOT** `/goal` (standing ruling `927ffb35` · steward `seq-104`).
Board of record: `runs/STATUS-0.8.20.md`; open HITL decisions: §11 — the **live** open set is **two**
(batched governed-surface decision, publish), per `release-state-0.8.20.json` `decisions.unruled`.

> **Fix-round cap for these slices — PRODUCTIVITY predicate (TC-82, HITL-ruled 2026-07-28 — `seq-125`).**
> **This SUPERSEDES the TC-75 engine amendment (`seq-119`): the predicate is NO LONGER which directory a
> slice touches.** On **every** slice: **3 rounds on the same finding** (unchanged — the anti-thrash rule),
> and at **round 6** a **mandatory halt and Steward check-in** where the orchestrator reports what each round
> found and the Steward rules continue / re-scope / escalate. Rounds 7-10 are **not authorized without it**;
> the Steward may extend to **10 only where every round has been PRODUCTIVE** (each finding a *new and
> distinct* defect) — a repeated or empty round means **6 binds**. Beyond 10 is an **HITL halt**. Basis:
> Slice 30 touched **zero engine source** and still ran seven rounds, each finding a new, distinct
> false-negative class — directory is a poor proxy for risk.
>
> **What counts as a round (TC-84, `seq-125`):** a round is a **review-verdict cycle** (verdict → fix →
> re-review). A **mid-flight `SendMessage` steer is NOT a round** and does not advance the counter. That
> consequence is known and accepted; it is **not** licence to drip-feed corrections to dodge the check-in.

*(Everything below in §9 is retained as landed history — the Slice-15 ratified decisions and the pre-keystone
"Slice 15 is OPEN" close notes describe the state before the keystone merged. Do not act on the "OPEN / BLOCKED"
framing; 15 is landed and 20/25 are unblocked.)*

> ### ✅ HITL-RATIFIED 2026-07-21 — the four decisions that unblock the registry
>
> All three steward positions were **verified by codex against ground truth** before ratification
> (transcript: `dev/plans/runs/codex/0.8.20/slice-15-steward-positions-verify-20260721T041028Z.log`).
>
> 1. **`ProjectionSpec` gains the `fts?:{tokenizer}` / `vector?:{embedder}` sub-objects — APPROVED.**
>    The ratified C-1 contract has exactly three roles (`filterable`, `rankable`, `searchable`); the
>    FTS-vs-vector distinction is carried by these **sub-objects**, not by extra roles. `searchable→FTS`
>    and `searchable→vector` are **tier labels, not enum members** — **do NOT invent them as roles.**
>    `roles` must carry **set semantics** (`Set<ProjectionRole>`); no contract line mandates a literal
>    array encoding. **This is where Slice 20's `dense_readiness` attaches.** Change is ADDITIVE.
> 2. **`filterable` stays PRE-KNN — APPROVED; no ADR deviation.** `ADR-0.8.11 D3`
>    (`dev/adr/ADR-0.8.11-filter-grammar-unification.md:213-225`) forbids demoting an indexed-metadata
>    predicate to a post-KNN `json_extract`. **Honour it.** Reshape the vec0 table rather than scoping
>    `filterable` down to the read/EAV backend.
> 3. **TC-33 → INTEGER epoch in storage and in the governed SDK — APPROVED.** The BYO-LLM extractor
>    boundary keeps ISO-8601 with **engine-side hard-reject** normalisation — APPROVED.
> 4. **NO DATA MIGRATION (HITL, 2026-07-21).** *"No data migration is supported, so if 'migration'
>    relates to data, it is unneeded."* Schema steps may define the **new shape**; they must **NOT**
>    convert, backfill, or preserve existing rows. FathomDB is pre-1.0 beta and 0.8.20 is a breaking
>    pair — users do not carry data across it. **Consequence: the vec0 reshape does not stage/reinsert
>    blobs, and TC-33 does not convert stored ISO values.** *(Steward note, not a question: shipped
>    step 21 — the `_legacy:` provenance backfill — IS a data migration and predates this ruling. It is
>    left as-is; the ruling is read as forward-looking.)*
>
> **P3 hard-reject requirement (HITL Note 1) — "reduce failure mode":** the fail-open path is the
> defect. An unparseable timestamp must **never** coerce to SQL `NULL`, because a NULL `t_invalid`
> reads as **"still valid"** (`fathomdb-schema/src/lib.rs:339`; the predicate is generated by the ONE
> `fn edge_validity_sql` in `fathomdb-engine/src/lib.rs` and relied on at each of its call sites — grep
> `edge_validity_sql`) — i.e. junk silently **resurrects an invalidated edge**. Required, defence in depth:
> **enforce `strftime('%s', <user value>)` and reject a NULL result with a typed error at the write
> boundary**, plus schema-level `NOT NULL`/`CHECK` constraints so the invariant is structural rather
> than upheld by call sites, plus a RED test proving malformed input **fails loudly**. Note
> `temporal_fallback` is matched by **string equality** against `substituted_t_valid`
> (the `is_temporal_fallback` / `fallback_epochs` comparison in `fathomdb-engine/src/lib.rs`) — a
> representation change silently stops flagging fallback edges, so that comparison must be re-grounded,
> not left to drift.
>
> **TC-47 (steward-ruled 2026-07-22, within the Note-1 hard-reject mandate; HITL-veto-available):**
> `strftime('%s', …)` is a **leaky** ISO validator — it has now silently mis-stored input twice: Julian-day
> numeric strings (TC-44) and **calendar rollover** (`2025-02-30` → `2025-03-02`, `2025-04-31` → `2025-05-01`;
> well-formatted, non-NULL, *wrong instant*). A shape gate cannot catch a well-shaped-but-invalid date. Fix
> is a **round-trip completeness check** — parse to epoch, format the epoch back, reject if it does not equal
> the input — which is **complete by construction** (accepts exactly the strings whose parsed instant
> reproduces the input) and **also subsumes the TC-44 Julian-day class**, in pure SQL, **no `chrono`/`time`
> dependency** (option (c) is therefore unnecessary). Safety was never breached — a rolled-over date is a
> valid instant, not NULL, so the "no resurrection via NULL" property held throughout; this closes the
> **wrong-instant** residue. *If a temporal class ever survives the round-trip check, THAT is the trigger to
> escalate the "strftime is the wrong tool → add a date library" architecture question to the HITL.*
>
> ### ✅ HITL-RATIFIED 2026-07-21 — TC-46: the 15e vec0 reshape is NON-DESTRUCTIVE (Option 1)
>
> The vec0 `filterable` pre-KNN reshape (15e) is built as a **non-destructive re-insert**, not a wipe.
> **This DISSOLVES TC-46** — no embedding-loss, so no `configure_projections` reshape-acknowledgement
> parameter is minted, and **no new governed surface** is added for the reshape. (Decision 4's
> no-preservation stays scoped to the **cross-version 0.8.9→0.8.20 schema step**, as HITL stated it;
> it does **not** reach a runtime reconfiguration of a live DB.) `filterable` **already works** via the
> Slice-15d row-owned EAV table (`canonical_attributes`); 15e adds the **pre-KNN vector-path** routing
> only (ADR-0.8.11 D3). The tree already ships this exact operation as
> `fn migrate_vector_partition_pack1_to_pack2` (in `fathomdb-engine/src/lib.rs`) — **follow that precedent.**
>
> **The four load-bearing conditions (steward-investigated against code; all MUST hold or query results
> go silently wrong):**
>
> 1. **List `rowid` explicitly** in the re-insert — a vec0 row maps to its node by `rowid == write_cursor`
>    (the identity documented on `write_canonical_row_with_kind_for_test` and relied on by
>    `fn text_hit_passes_filter` / `fn edge_fts_hit_passes_filter` in `fathomdb-engine/src/lib.rs`); letting
>    vec0 auto-assign rowids silently decouples every embedding.
> 2. **New attribute column is plain metadata OR a partition key — NEVER a vec0 `aux`/`+` column.** An aux
>    column hard-**errors** every filtered KNN query (see the `aux` note on `fn vector_partition_create_sql`
>    in `fathomdb-engine/src/lib.rs`).
> 3. **Back-fill old rows with the `''` sentinel** (vec0 TEXT metadata is NOT-NULL-able — the `''` back-fill
>    in `fn vector_partition_create_sql`'s callers, e.g. the `status` sentinel in
>    `fn migrate_vector_partition_pack1_to_pack2`) so they cleanly fail-to-match a filter rather than erroring.
> 4. **Copy `embedding_bin` verbatim via `vec_bit(...)` — do NOT re-quantize.** Re-deriving bits from the
>    raw `embedding` leaves old rows quantized **un-centered** while new rows stay mean-centered ⇒
>    incomparable Hamming distances, silent recall corruption, no error (the `vec_bit(embedding_bin)`
>    re-insert inside `fn migrate_vector_partition_pack1_to_pack2`, and the anti-pattern to avoid — the
>    DELETE+INSERT re-quantize in `fn run_pin_and_requantize_pass` — both in `fathomdb-engine/src/lib.rs`).
>
> Idempotent re-registration still diffs to a **no-op**; a shape-changing reshape is an **explicit**
> drop (`api-surface.md:26-30`), never a silent boot-time wipe. `run_pin_and_requantize_pass` is a
> **separate same-shape** re-quantize and is untouched by the reshape.
>
> ### ✅ HITL-RATIFIED 2026-07-22 — Finding 1: attribute-filter × edge-hit semantics = **(A), with (D) reserved**
>
> **Ship (A) — edges excluded — as 0.8.20's behavior.** An attribute filter returns only attributed node
> rows; edge hits are dropped consistently on both arms (already the shipped behavior after 15e fix-1).
> **Document it as deliberate and pin a test.** **(B) declined, (C) not built.**
>
> - **Not consumer-reachable in 0.8.20 (verified):** `attributes` is **not** on the Py/TS `search` wire
>   (`fathomdb-napi SearchFilterInput`, `fathomdb-py search` carry only `source_type/kind/created_after/
>   status`); attribute-filtering is **engine-internal only** (the comment inside `pub fn search_filtered` in
>   `fathomdb-engine/src/lib.rs` — "a later slice adds that surface"). So the edge-semantics choice governs a
>   feature no consumer can call this release; (A)
>   forecloses nothing and is a pure query-time behavior with **zero stored-data and zero wire commitment**.
> - **(B) raw pass-through — DECLINED.** Predicate-honest failure (Memex): returning rows never evaluated
>   against the filter is a bug-factory for agent-memory consumers feeding results to an LLM as vetted
>   context; and it **reopens the 15e fix-1 [P2]** + is **RRF-surprising** (edges inherit the dominant text
>   weight 3.0 and can outrank a node that satisfied the filter). If pass-through is ever wanted, use the
>   coherent variants **B′ (filter-nodes-first-then-edges)** or **B″ (edges as a labeled sidecar)** via the
>   existing `TextEdge` branch tag — **never raw B.**
> - **(C) edge-attribute projection — NOT BUILT.** The only **one-way door** (stored-data commitment; a
>   retroactive backfill the 0.8.20 no-migration posture forbids), and Memex explicitly does not need it.
> - **(D) edges filtered by their ENDPOINT NODES' attributes — RESERVED, documented, not built.** The
>   principled widening (Memex): "the relationship connects things you asked for" is explainable, unlike
>   (B)'s "the relationship was exempt." **(A) is (D) with an empty endpoint rule, so shipping (A) forecloses
>   nothing.** The both-vs-either endpoint fork is a real semantic choice — leave it to the first consumer
>   that needs it. **No per-query opt-out flag** (Memex: it defers the call into every call site).
>
> **Two Memex consult requirements carried to the SDK-surface slice** (the slice that puts `attributes` on
> the `search` wire — NOT 0.8.20, since the filter isn't callable yet):
>
> 1. **Dropped-edge count MUST be observable** (in the result or under `explain=True`). A filter that drops
>    material with no trace is "indistinguishable from a corpus that never had it." Do **not** repeat the
>    `quality_counts`-hardcoded-to-zero anti-pattern.
> 2. **Node-side pushdown is where the consumer value is, not edge semantics.** The registry (15d) is the
>    mechanism that makes `entity_type`/`pinned`/`expires_at`/… filterable; keep node attribute pushdown
>    solid + well-documented. This is the steer that says **don't spend budget on (C).**

**The REMAINDER of Slice 15 — projection registry (C-1 co-land) + EAV/property-FTS (R-20-PR, R-20-EAV), plus
TC-33 — ✅ LANDED in the keystone `a2022957`.** It is the Phase-2 keystone: 20 and 25 both depend on it and are
now **UNBLOCKED**. Slice 30 (H7) additionally depends on 10/15/20/25.

> **✅ SUPERSEDED 2026-07-24 — Slice 15 is COMPLETE and LANDED (`a2022957`); all four parts closed.**
> The blockquote below is the historical pre-keystone note.
>
> **⚠ (HISTORICAL) SLICE 15 IS OPEN. It had FOUR parts; ONE has closed.**
>
> **✅ TC-34 — CLOSED at `a8087dfb`** (branch `orch-0.8.20-s15`, docs/artifacts `cd5620be`; **not landed**).
> Node-validity authoring shipped as **optional `valid_from`/`valid_until` fields on the existing node write
> item** — **not a new verb**, **zero new commands**, symmetric with `PreparedWrite::Edge`'s `t_valid`/
> `t_invalid`; validation in the **engine** so all three languages share one rule. It also carried an
> **unscoped but in-scope search-validity coherence fix**: TC-34 made window-authoring reachable from the SDK
> and thereby turned Slice 10's deliberate "`ReadView` covers the five read verbs, not `search`" narrowing
> into a **live defect**, reproduced at runtime. `ReadView` now governs `search` across **five** hydration
> sites, filters **before** the vector cutoff (a **recall** defect) and binds **one instant per query** (a
> **determinism** defect). **codex §9: four rounds to a TERMINAL PASS, no verdict overridden.**
> Governed-surface delta **PROPOSED / NOT SIGNED**; **AC-079 still unminted.** See `runs/STATUS-0.8.20.md` §13.
>
> **❌ The OTHER THREE parts are UNTOUCHED — `R-20-PR`, `R-20-EAV` and `TC-33` have design work but NO code.**
> **Slices 20 and 25 therefore REMAIN BLOCKED.** Two findings are load-bearing for this remaining work and
> should be resolved as part of its design, not discovered mid-build: the plan's
> `roles: {filterable, rankable, searchable}` **cannot express the ratified C-1 contract** (**TC-40**), and
> `filterable` has **two incompatible backends** (**TC-41**).
>
> **STILL LIVE — FOLDED IN BY HITL (2026-07-20), must close in this slice:**
>
> - **TC-33 — the temporal model is internally inconsistent.** Node validity is **INTEGER epoch seconds**
>   (step 22) while **edge** validity is **ISO-8601 TEXT**. Harmonise them. **Steward recommendation: converge
>   on INTEGER epoch seconds**, matching the node representation, §1's "integer windows", and the
>   deterministic bound-`:now` seam that makes validity testable without wall-clock. **Now is the cheapest
>   moment** — 0.8.20 is already a coordinated breaking-pair publish, so harmonising later costs a second
>   breaking change. If the slice's design work concludes the other representation should win, **escalate to
>   the Steward before implementing** — that is a direction call, not an implementation detail.
>
> ---
>
> **Slice 5 — erasure completeness: ✅ COMPLETE and LANDED** at **`1f8ed8bf`** (in `origin/main`).
> **AC-079 remains UNSIGNED and still blocks publish.**
>
> **Slice 10 — `ReadView` / node-validity: ✅ COMPLETE ON-BRANCH @ `93a57b10`** (branch `orch-0.8.20-s10`,
> rebased onto `origin/main` `ae44770f`). **Not landed — the Steward lands it.** **R-20-RV + R-20-NV closed.**
> **SCHEMA 21 → 22** (step 22: `canonical_nodes.valid_from`/`valid_until`, INTEGER epoch, nullable, half-open
> `[from, until)`, NULL = unbounded; existing rows back-fill NULL/NULL ⇒ always valid ⇒ **default-view
> visibility unchanged**). **TC-31 RESOLVED**; **TC-32 annotated** per the HITL ruling (accepted, no behavior
> change). Governed-surface delta is **PROPOSED / NOT SIGNED**; **no AC minted — AC-079 remains available and
> unminted**. **Zero eu7 runs.** **TC-34 has since been CLOSED by Slice 15b** (above); **TC-33 is still owed**,
> alongside the two carried sign-offs and Slice 15b's **§4 #18–#22** — see `runs/STATUS-0.8.20.md` §4, §12, §13.

**Slice 5 — erasure completeness (R-20-E1…E8) — LANDED at `1f8ed8bf`; retained for the record.** One
row-owned projection registry + a **total projector covering nodes AND edges** (extract the inlined edge
projection from `commit_batch` `:12166+`) + `SourceId` newtype + mandatory provenance + **`erase_source()` as an
SDK lifecycle verb** (Py + TS; `excise_source` stays CLI-only; **AC-041 stays green**, denylist stays five
names) + `truncate_wal()` inside the verb with typed `ErasureIncomplete` + telemetry **selective redaction** +
`excise_collection_record` + op-store record erasure + the `_legacy:pre-0.8.20` migration (`logical_id IS NULL`
only). **Plus the HITL audit-durability ruling (F-27):** exempt the erasure-audit collection from
`enforce_provenance_retention` so the record of a deletion event cannot be swept.
**RED tests MUST assert on RAW TABLE CONTENTS** — a test that *searches* for the erased text passes on the
broken code (§0.1). **Mint ACs from AC-079** (§3). **Run NO eu7 — R-20-EU7 is closed by decision (F-28).**
**TC-11 is CLOSED — do not re-open it.**

> **Slice 0 — X0 design gate: ✅ COMPLETE.** HITL-SIGNED and landed 2026-07-19 at `403eb254` (master **F-26**).
> Delivered the STATUS board, the frozen reqs/ACs, `dev/design/0.8.20-slice0-erasure-design.md` (the v5
> addendum, which **wins over v4 on conflict**), `scripts/preflight.sh --landing` (TC-RUBRIC-5 now
> **mechanically enforced**), and the pinned TC-RUBRIC-7 transcript path `dev/plans/runs/codex/0.8.20/`.
> Its findings are reconciled into master **F-26…F-31**.

---

## 10. Decisions taken (recorded)

- 2026-07-07 — **F-19/F-20:** OPP-12 into 0.8.x; 0.8.20 = Phase-2 + breaking-pair publish; deps → 0.8.22 · HITL.
- 2026-07-08 — **F-21:** OPP-12 **BUILD-AUTHORIZED**; build ≠ adopt; publish = separate per-`x.y.z` gate · HITL.
- 2026-07-09 — **F-22:** open-TC schedule ratified (eu7-basis + embed_batch_cls → 0.8.20) · HITL.
- 2026-07-09 — **F-23:** anonymous-surrogate deferred to Phase-2 (ruling 1a) · HITL. **← SUPERSEDED by TC-11.**
- 2026-07-10 — **RUBRIC-H7 / TC-RUBRIC-2:** `can-i-deploy` contract-conformance gate folded into the 0.8.20 row;
  **absent-or-failing gate HOLDS the pair** · HITL.
- 2026-07-11 — **TC-RUBRIC-5:** dedicated-checkout-per-orchestration guardrail ADOPTED; folds into X0 · HITL.
- 2026-07-11 — **Erasure axis = PROVENANCE**, not the `l:`/`h:` id-space; pin is orthogonal · HITL steer.
- **2026-07-12 — TC-11: pin A RATIFIED.** Anonymous nodes stay `h:` **permanently**; §2(ii) **OVERRULED**; the
  surrogate leg is **CANCELLED for the anonymous class, not deferred** · **HITL**.
- **2026-07-12 — REQ-037 lawful-erasure carve-out APPROVED.** `excise_source` stays CLI-only (recovery seam);
  **`erase_source()` ships as an SDK lifecycle verb**; `purge_logical_id` struck from REQ-037's forbidden list;
  **AC-041 unchanged and stays GREEN** · **HITL**.
- **2026-07-19 — Slice-0 X0 SIGNED.** Package landed to `main` at **`403eb254`**. **TC-RUBRIC-5 is now
  ENFORCED** via `scripts/preflight.sh --landing` (hard-fails on the primary checkout); **TC-RUBRIC-7**
  transcript path **pinned at `dev/plans/runs/codex/0.8.20/`** · **HITL**.
- **2026-07-19 — R-20-EU7 CLOSED BY DECISION, not by measurement. ZERO eu7 runs authorized, any backend, any
  N** (§3). CPU↔CUDA is bit-identical (0.8.7) ⇒ the 0.8330 GPU figure was a **cross-backend artifact**.
  **TC-19 closed by routing; TC-12 overruled on the merits; the CPU-throughput investigation is FORBIDDEN** ·
  **HITL**.
- **2026-07-19 — The erasure audit trail MUST be DURABLE** — *"there must be an auditable record of deletion
  event."* **Slice 5 exempts the erasure-audit collection from `enforce_provenance_retention`** (§8). The v4
  §3.6 "retained permanently" claim is **VERIFIED FALSE and SUPERSEDED** (**TC-15**) · **HITL**.
- **2026-07-19 — AC ids mint from `AC-079`** (§3); AC-077/078 are **live IR-1/IR-2 reservations** (**TC-14**) ·
  **HITL**.
- **2026-07-19 — The `SourceId` breaking change needs NO separate adoption call** — 0.8.20 is *already* a
  sanctioned coordinated breaking-pair publish. **`embed_batch_cls` TS binding proceeds inside X1** · **HITL**.
- **2026-07-20 — Slice 5 LANDED** at **`1f8ed8bf`** (in `origin/main`). AC-079 **still unsigned** ⇒ **publish
  stays blocked**.
- **2026-07-20 — TC-32 (co-named-entity dedupe) ACCEPTED as-is, no behavior change** — annotated in code
  rather than fixed · **HITL**. **Carry-forward caveat: the erasure guarantee MUST NOT be stated
  unconditionally to users while this stands.**
- **2026-07-20 — Slice 10 COMPLETE on-branch @ `93a57b10`.** R-20-RV + R-20-NV closed; **SCHEMA 21 → 22**;
  **TC-31 RESOLVED**. Opens **TC-33** (temporal-model split: node validity INTEGER epoch vs shipped edge
  `t_valid`/`t_invalid` ISO-8601 TEXT) and **TC-34** (node validity has **no write-side authoring verb**) —
  **both owed to the HITL**. Governed-surface delta **PROPOSED / NOT SIGNED**; **no AC minted**.
- **2026-07-20 — TC-34 CLOSED in Slice 15b** @ **`a8087dfb`** (branch `orch-0.8.20-s15`; **not landed**).
  Node-validity authoring ships as **optional fields on the existing node write item**, **zero new commands**.
  **`ReadView` EXTENDED TO `search`** — Slice 10's five-verb scope was a narrowing of a contract that already
  named `search`, and TC-34 made the gap reachable, so it was fixed here rather than deferred; the fix also
  moved validity **before** the vector cutoff (recall) and bound **one instant per query** (determinism).
  Governed-surface delta **PROPOSED / NOT SIGNED**. **codex §9: FOUR rounds to a TERMINAL PASS, with no
  verdict overridden and every [P2] fixed.** **AC-079 remains UNMINTED and publish remains BLOCKED.**
  **⚠ Slice 15 is NOT complete** — R-20-PR, R-20-EAV and TC-33 are **not started**, so **Slices 20 and 25
  stay blocked**. *(← SUPERSEDED 2026-07-24: the remainder landed in the keystone; 20/25 are now unblocked.)*
- **2026-07-24 — Slice 10 LANDED at `3cfb3cda`** (merge). R-20-RV + R-20-NV closed; SCHEMA 21 → 22; TC-31
  RESOLVED · **Steward**.
- **2026-07-24 — Slice 15 KEYSTONE LANDED at `a2022957`** (merge; ledger tip `3264114a`, steward seq-98).
  **R-20-PR + R-20-EAV + `filterable` pre-KNN + TC-33 + TC-34 + Finding-1 (A) + `#[non_exhaustive] SearchFilter`**;
  **SCHEMA →24** (step 24); codex §9 **terminal-clean**; **gates re-verified by the Steward** (clippy 0, check 0,
  (A) pin 1/1, AC-041 3/3, denylist five). **TC-46 + TC-47 RESOLVED; TC-11 + TC-32 CLOSED.** **Slices 20 and 25
  are now UNBLOCKED; the immediate next slice is Slice 20 (R-20-DR).** Governed-surface delta still **PROPOSED /
  NOT SIGNED** — **AC-079 sign-off gated to Slice 40** (§11 #1) · **Steward**.

---

## 11. Open HITL decision queue (as of 2026-07-24)

> The X0 questions (publish-gate / eu7-basis / `embed_batch_cls` / adoption arms) were resolved at the 2026-07-19
> sign-off (§10). This is the **current** queue after the Slice-15 keystone landed. Each item:
> **decision — description — options — recommendation — justification — when it's gated.**
>
> **Already settled — do NOT re-open:** **Finding-1 = (A)** (attribute-filter drops edge hits; (D) reserved,
> B/C declined); **TC-46 + TC-47 = RESOLVED** (non-destructive vec0 reshape; round-trip ISO validator);
> **TC-11 + TC-32 = CLOSED** (`h:` end-state pin; co-named-entity dedupe accepted as-is).
>
> ### ✅ HITL RULINGS 2026-07-24 — items 1–8 decided
>
> 1. **(a)** — sign AC-079 **once at Slice 40**. 2. **(a)** — Phase-2 surface **opt-in**, erasure fixes **ON**.
> 2. **(a)** — **publish at Slice 40**, subject to the H7-green / `#11`-rehearsal / Memex-ready prereqs.
> 3. **(b) REJECT** — an `fts`/`vector` sub-object without the `searchable` role is an **invalid spec**; implement
>    the rejection at the next `configure_projections` slice. 5. **(a)** — **add the `embed_batch_cls` TS binding**
>    (X1 / Slice 40). 6. **TC-16 → fold into Slice 40** (with the `#11`-full rehearsal). 7. **TC-45 → FIX IN
>    0.8.20, folded into Slice 20** (HITL 2026-07-24 — supersedes the earlier "HOLD / placement UNDECIDED" and the
>    Steward's "0.8.21 own fixup" recommendation). 8. **Hermes consult** — still pending input, unchanged.
>
> Effect lands at **Slice 40** for 1/2/3/5/6; at **Slice 20** for 7; at the **next `configure_projections` slice**
> for 4; item 8 is a future-slice input.
>
> ### ✅ HITL RULINGS 2026-07-25 — the run authorization (see master **F-34**)
>
> The remaining ladder (20 → 25 → 30 → 40) runs under a **standing Steward mandate**, preceded by the
> cross-cutting **DOC-HYGIENE-2** effort. Four rulings amend this queue:
>
> 1. **AC-079 is PRE-SIGNED** — item 1 below is **DECIDED, not open**. The HITL signed the *accumulated*
>    governed-surface delta as it stands today (Slices 5d + 10b + 15b + 15d). **The delta is pinned to the
>    content of `src/conformance/governed-surface-allowlist.json` as of this commit** — 30 `allowlist`
>    members, `recovery_denylist` unchanged at the five REQ-054 names. **Any diff to that file re-opens the
>    gate**, enforced mechanically by the DOC-HYGIENE-2 **T1e allowlist-pin gate**, not by anyone remembering.
>    **Pre-sign ≠ AC minted:** AC-079 is still *minted and recorded as SIGNED at Slice 40*, covering the
>    pre-signed set plus whatever the batched decision below adds.
> 2. **New governed surface at 20/25/30 — CONVERTED, not pre-signed.** Each pin trip is recorded as a
>    proposal (branch stays green, existing practice) and the **accumulated 20/25/30 delta goes to the HITL
>    ONCE, at the Slice 30 → Slice 40 boundary**. Slice 20 is expected to trip it (`flush_embeddings()` reads
>    as a net-new command; `dense_readiness` attaches additively to `ProjectionSpec.vector`).
>    **⚠ 2026-07-26 — this ruling is NOT MECHANICALLY EXECUTABLE as the gate is wired (TC-59, p1).**
>    `check-governed-surface-pin.sh` hashes the **raw bytes** of the allowlist and `preflight.sh --landing`
>    treats it as a **HARD** fail, so recording a proposal *and landing* is impossible — **any** allowlist diff
>    blocks the land. Slice 20 was split around it (part b adds zero commands and landed at `26b237c0`; part c
>    is HELD). Fix the tooling — a `pending_delta` block in the pin — before 20c/25/30 rely on this ruling.
>    Slice 20 did **not** in fact trip the pin: `dense_readiness` added **zero** governed commands.
> 3. **§ 7 prerequisite 5 (Memex co-land readiness) is CLOSED BY DECISION** — see § 7. Item 3 below
>    (publish) is **unchanged and still HITL-pending**: it is the one hard stop in the run.
> 4. **Item 8 (Hermes consult) is CLOSED — no input received.** It gated nothing in 0.8.20.
>
> **Remaining stops in the run:** the batched governed-surface decision (scheduled, C4 → C6) and **publish**
> (Slice 40, hard). Reserved-gap band overflow (§ 5) still halts. The fix-N circuit-breaker
> (`orchestration.md` § 6) escalates to the **Steward** first, reaching the HITL only on the Steward's call.

1. **AC-079 governed-surface sign-off** — the Phase-2 + erasure API delta (`erase_source`/`SourceId`/
   `EraseReport`/`ExciseReport`, `configure_projections`/`ProjectionSpec`/`ProjectionRole`, `read.projections`,
   `SearchFilter.attributes` + `#[non_exhaustive]`) ~~is **PROPOSED / NOT SIGNED**~~ **← SUPERSEDED 2026-07-25;
   see the ruling at the foot of this item**; signing mints AC-079 and permits
   publish. **Options:** (a) sign once at Slice 40 after the full surface + H7 are green; (b) sign per-slice now.
   **Rec:** (a). **Why:** the surface still grows (20 adds `dense_readiness`, 25 adds surrogate); 0.8.19 signed at
   its Slice-40 close. **Gated:** Slice 40 (publish precondition).
   **✅ RULED 2026-07-25 — PRE-SIGNED, this item is CLOSED.** The accumulated delta (5d + 10b + 15b + 15d) is
   signed as of this commit and pinned to the allowlist file's content; any diff re-opens it (T1e gate). The
   *minting* of AC-079 still happens at Slice 40. Growth from Slices 20/25/30 is handled by the **batched**
   decision at the Slice 30 → Slice 40 boundary, not by re-opening this item. See the 2026-07-25 rulings block.
2. **Adoption arms (build ≠ adopt, F-21)** — which Phase-2 features change shipped DEFAULT behavior vs opt-in.
   **Options:** (a) registry / read-modes / `dense_readiness` / `filterable` OPT-IN, erasure fixes ON; (b) some
   other subset ON. **Rec:** (a). **Why:** the erasure fixes correct shipped defects; the Phase-2 surface is
   deliberate-adopt (Memex confirmed, on its own timeline). **Gated:** Slice 40.
3. **Publish 0.8.20 breaking pair** — manifests `0.8.9 → 0.8.20`, the first real publish since 0.8.9, co-landing
   with a Memex `0.5.x`-successor. **Options:** (a) publish when H7 is green + `#11`-full tag→publish rehearsed +
   Memex ready; (b) hold. **Rec:** (a), subject to those hard prereqs. **Why:** a pushed `v*` tag auto-fires the
   real crates/PyPI/npm publish. **Gated:** Slice 40.
4. **`fts`/`vector` sub-object without the `searchable` role — reject or accept?** Currently accepted and
   round-trips faithfully (not a bug). **Options:** (a) accept (permissive); (b) reject as an invalid spec.
   **Rec:** (b) reject. **Why:** it is a meaningless config; fail-fast matches the hard-reject philosophy, and
   additive strictness is safe pre-1.0. **Gated:** non-blocking — the next `configure_projections` slice.
5. **`embed_batch_cls` TS-binding parity (F-22)** — py-only since 0.8.14. **Options:** (a) add the TS binding;
   (b) ratify py-first as documented. **Rec:** (a). **Why:** the first published release since 0.8.9 should not
   ship a Py/TS asymmetry; it is a small addition. **Gated:** X1 / Slice 40.
6. **PLACEMENT — TC-16 dead publish dry-run guard** — red since 0.8.14; masks a real publish-workflow regression
   in the very release that publishes. **Rec:** fold into Slice 40 with the `#11`-full rehearsal. **Why:** it is a
   publish-workflow guard and Slice 40 owns publish. **Gated:** HITL confirms the slot.
7. **PLACEMENT — TC-45 supersession-terminal CHECK defect** — terminals are silently dropped via
   `state='superseded'` vs `CHECK('failed','up_to_date')` + `INSERT OR IGNORE`, so the cursor can stall.
   **✅ RULED 2026-07-24 (HITL): FIX IN 0.8.20 — folded into Slice 20.** This **overrides** the Steward's
   "0.8.21 own fixup" recommendation. **Why:** 0.8.20 is the first real publish **and it publishes the projection
   registry**, so deferring turns an internal defect into a published silent cursor-stall; the twin of this class
   (TC-33 fix-4) was already fixed in-release; and "carried unfixed across releases" is the F-30 trap. **Fix
   shape (implementer + codex micro-call):** prefer `'up_to_date'` at both `record_projection_terminal` call
   sites that pass `"superseded"` — the two prune loops inside `fn commit_batch` in
   `fathomdb-engine/src/lib.rs`, one after `prior_edge_cursors_by_logical_id` (G0) and one after
   `prior_edge_cursors_by_triple` (G11); grep
   `record_projection_terminal(&tx, *sc as u64, "superseded")` for exactly those two —
   **no migration** — over widening the terminal CHECK to admit `'superseded'`
   (**a schema step + migration test**); escalate only if the terminal's semantics demand the distinct token.
   RED-first, per the standing TDD rule. Ledger: todos seq 65. **Gated:** Slice 20.
   **✅ CLOSED — LANDED 2026-07-26 at `26b237c0`** (Slice 20a; RED `ca32ec81` → GREEN `9db32765`; codex §9 **PASS,
   no findings**). Fixed **exactly as the ruling preferred**: `'up_to_date'` at **both** call sites in
   `fn commit_batch` (the G0 path after `prior_edge_cursors_by_logical_id`, the G11 path after
   `prior_edge_cursors_by_triple`), **no migration**, `SCHEMA_VERSION` **stays 24**, and the terminal CHECK was
   **not** widened. The escalation clause ("only if the terminal's semantics demand the distinct token") did not
   fire, and that was **checked rather than assumed**: no consumer discriminates the token —
   `advance_projection_cursor` and `commit_projection_outcomes` test only `.is_some()`, and `projection_status`
   maps `_ => UpToDate`. Close record: `runs/STATUS-0.8.20.md` §14.
8. ~~**PENDING INPUT (not a decision) — Hermes consult**~~ **✅ CLOSED 2026-07-25 (HITL): no input received
   from Hermes; the item is closed rather than carried.** It concerned the eventual **(D)** endpoint-node
   attribute-filter widening. Memex already replied: **(A) now, (D) reserved** — that reply stands and is
   unaffected. It gated nothing in 0.8.20; a future SDK-surface slice that widens (D) may re-open the consult
   on its own terms.
