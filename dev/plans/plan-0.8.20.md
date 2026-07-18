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
| `SCHEMA_VERSION = 20` | 0.8.19 landed the 19→20 existence migration | ✓ `fathomdb-schema/src/lib.rs:6` |
| manifests = **`0.8.9`** | every release since 0.8.9 was label-only ⇒ **0.8.20 is the first manifest bump `0.8.9 → 0.8.20`** | ✓ `src/python/pyproject.toml:7`, `src/ts/package.json:3` |
| `transition` / `purge` **shipped in both SDKs** | 0.8.19 Phase-1 surface is live | ✓ `fathomdb-py:1274,1297`; `fathomdb-napi:1070,1102` |
| **Phase-2 surface = 100 % NET-NEW** | `ReadView`, `valid_from`, `valid_until`, `dense_readiness`, `configure_projections`, `ProjectionSpec`, `EntityTypeSpec`, `id_prefix` | ✓ **ZERO hits** across all crates |
| `derive_logical_id` = `SHA256("{kind}:{name}")` | natural-key derivation, **not** an opaque surrogate | ✓ `engine lib.rs:11152` |
| `search_index_v2` = **content-storing** FTS5 | holds the **body verbatim** (no `content=''`) | ✓ `fathomdb-schema/src/lib.rs:427` |
| `truncate_wal()` **already exists** | `PRAGMA wal_checkpoint(TRUNCATE)`, returns typed `TruncateWalStatus::{Done,Busy}` | ✓ `engine:6379`; **CLI-only** (`fathomdb-cli:389`); **NOT called by `purge`/`excise`** |
| op-store record erasure | **does not exist** — only a cap-based retention sweep | ✓ `engine:10083` (`enforce_provenance_retention`) |
| REQ-037 → AC-041 | recovery surface CLI-only; AC-041 tests the **REQ-054 five-name denylist** only | ✓ `dev/requirements.md:332`; `dev/acceptance.md:688` |
| AC ceiling | highest existing AC | ✓ **AC-077** (`dev/acceptance.md`) |

### 0.1 THE ROOT-CAUSE FINDING — `search_index_v2` is maintained by **ONE** of FIVE sites

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

- **`ReadView` / read-modes** — composable relax-flags; uniform on `get`/`list`/`neighbors`.
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
  it *is* hashed (`engine:11152`); §2:98 names it a **separate** mechanism.
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
`README.md:108` (struck) · `api-surface.md:64` (surrogate leg **CANCELLED, not deferred**).
Design of record: `dev/design/0.8.20-erasure-and-h-end-state-v4.md`.

### 2.2 The erasure axis is **PROVENANCE**, not the `l:`/`h:` id-space

`transition`/`purge` are **`l:`-only by design** (an anonymous row's identity *is* its bytes; "change the record"
is incoherent). Anonymous content — **the dominant corpus class** — is erased by **`source_id`**.
**Pin A is therefore ORTHOGONAL to GDPR and costs nothing there.**

### 2.3 REQ-037 lawful-erasure carve-out · ✅ **HITL-APPROVED 2026-07-12**

The project's real policy is **"RECOVERY-*NAMED* verbs are CLI-only"** — **not** "destructive ⇒ CLI-only".
Proof: **`purge(logical_id)` is already an SDK verb** (`py:1297`, `napi:1102`, 0.8.19) *despite being named in
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

Tracked by **requirement id + TDD test name** (per the locked-`acceptance.md` policy). **New ACs are permitted:
0.8.20 Slice-0 IS a gated slice.** AC ceiling today = **AC-077**.

### Phase-2 (A)

| ID | Requirement | Acceptance signal (falsifiable, offline) |
|----|-------------|------------------------------------------|
| R-20-RV | `ReadView`/read-modes: composable relax-flags, uniform on `get`/`list`/`neighbors` | read-mode matrix test; `include_superseded` returns history; default view unchanged (no silent behavior drift) |
| R-20-NV | Node-validity `valid_from`/`valid_until` + `valid_as_of` (bound-`:now` seam) | validity-window matrix; `crossed_boundary_since` hook; world-time only (`history_as_of` explicitly OUT) |
| R-20-PR | Projection registry (C-1): `configure_projections(spec, drop?)` idempotent diff + backfill; engine is **sole** authority; incompatible change ⇒ destructive delta requiring explicit `drop` | re-registration is a no-op; role add/remove builds/drops exactly; boot re-derive is crash-safe + idempotent |
| R-20-EAV | EAV / property-FTS — the store the registry projects from | property-level filter/search green; body-FTS behavior unchanged |
| R-20-DR | `dense_readiness` + `flush_embeddings()` + atomic readiness-flip | readiness never reports ready with pending embeds; flip is atomic under concurrent write |
| R-20-SUR | Surrogate minting serves **ONLY** registry-admitted governed entities; decided **at write time** | **migration-guard: rows transitioning `logical_id NULL → NOT NULL` == 0** (the pin's invariant); registering a kind does **not** alter any pre-existing row's `IdSpace` |

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
| R-20-E8 | Legacy NULL-provenance rows become erasable | migration back-fills `source_id='_legacy:pre-0.8.20'` **WHERE `logical_id IS NULL` ONLY** — governed rows keep NULL and stay `purge`-addressable by `logical_id`. **`excise_source _legacy:` deletes NO governed row.** `doctor orphan-provenance` lists per-`source_id` counts. |

### Release / gates (C)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-20-H7 | **RUBRIC-H7 GATE (TC-RUBRIC-2)** — a **Pact-style `can-i-deploy`** mechanical contract-conformance check: as-built code still satisfies the ratified `OPP-12-C1-converged-contract.md` at the co-land. **Not humans re-reading prose.** | Gate exists and is GREEN. **An absent-or-failing gate HOLDS the breaking pair** (hard, HITL-directed 2026-07-10) |
| R-20-X1 | SDK parity (Py + TS) — **live functional harnesses, not symbol presence** | X1 GREEN. **Run parity BEFORE land, same unit** (rubric G6 carry: 0.8.19 ran X1 green *after* land via a native-import env trap — **treat an env trap as a landing blocker, not a follow-up**) |
| R-20-EU7 | **eu7 basis decision** (F-22): no-op vs real re-baseline — the **real-publish gate** | Recorded at Slice 0. If OPP-12 touches retrieval/gold-keying ⇒ bounded re-baseline. **Pin A keeps `SearchHit.id` byte-identical (`to_prefixed()` == prior `stable_id`) ⇒ the no-op basis is EXPECTED to hold; prove it, don't assume it.** |
| R-20-PUB | Coordinated breaking-pair publish; manifests **`0.8.9 → 0.8.20`** | Publish executed exactly per the **separate HITL gate** (F-21). Uses 0.8.18 #11-full machinery (proven, never fired). Pairs with Memex `0.5.x-successor` |
| R-20-AC | Governed-surface delta signed | **new AC (AC-078+)** mirroring AC-074: the Phase-2 + erasure API delta vs the conformance allowlist, **HITL-SIGNED**. `recovery_denylist` **unchanged (stays five)** |

---

## 4. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 25 → 30 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | **X0 design gate** — reqs/ACs frozen; erasure Slice-0 design; **TC-11 pin ✅ ALREADY RATIFIED**; eu7-basis + embed_batch_cls-TS decisions; **TC-RUBRIC-5** (dedicated-checkout/`preflight.sh --landing`) folded in; stand up `runs/STATUS-0.8.20.md`; **codex §9** | design-adr + steward-review | — |
| **5** | **Erasure completeness** (R-20-E1…E8) — registry + total projector + `erase_source` + provenance + WAL + telemetry + op-store + migration | implementation | 0 |
| **10** | **`ReadView` / read-modes** + **node-validity** (R-20-RV, R-20-NV) | implementation | 0 |
| **15** | **Projection registry (C-1 co-land) + EAV/property-FTS** (R-20-PR, R-20-EAV) | implementation | 0 |
| **20** | **`dense_readiness` + `flush_embeddings()`** (R-20-DR) | implementation | 15 |
| **25** | **Surrogate minting — registry-admitted governed entities ONLY** (R-20-SUR) | implementation | 15 |
| **30** | **RUBRIC-H7 `can-i-deploy` contract-conformance gate** (R-20-H7) | implementation | 10,15,20,25 |
| **40** | **Verification + release readiness** — full DoD, X1, eu7 basis, AC-078 sign-off, **publish-or-hold per the HITL gate** | verification | 5,30 |

**Keystones / hard gates.**

- **Slice 0 blocks everything** (X0 process gate, carried from 0.8.18 §5 / 0.8.19).
- **Slice 5 is INDEPENDENT of Phase-2** — it fixes **defects in shipped code** and can run fully parallel to
  10/15. It does **not** wait on the registry.
- **Slice 15 is the Phase-2 keystone** — 20 and 25 both depend on it.
- **Slice 30 (H7) is a PUBLISH PRECONDITION.** Absent-or-failing ⇒ **the breaking pair HOLDS.**

**Tracks (parallelizable).** `5 ∥ 10 ∥ 15` after Slice 0. Slice 5 touches the erasure/projection paths; 10/15
touch read + registry. They share `engine/src/lib.rs` ⇒ **serialize the merges** (rebase-then-merge one at a
time). **One `maturin develop` at a time** (shared `.venv` mutex).

---

## 5. Reserved-gap policy

Gaps `1–4, 6–9, 11–14, 16–19, 21–24, 26–29, 31–39` absorb unplanned follow-on. Fully orchestrated, not ad-hoc.
**HALT to HITL on band overflow** — never spill into the next mod-5.

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
4. **Baseline captured** — eu7 recall + FTS/vector numbers + X1, so R-20-EU7 equivalence has a reference.
5. **Memex `0.5.x-successor` co-land readiness** confirmed (breaking **pair** — one side alone is not a release).

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
- **`source_id` MUST NOT contain PII** — it is retained **permanently** in the `excise_source_audit` row
  (`:6479`), i.e. in the record of the erasure itself. Document at the write surface.
- **Outside the erasure boundary** (enumerate + disclose, do not silently omit): `safe_export` archives,
  operator backups, curated gold files. **Re-generate or destroy after any erasure.**

---

## 9. Immediate next slice

**Slice 0 — X0 design gate.** Stand up `runs/STATUS-0.8.20.md`. Freeze §3 reqs + RED-testable ACs. Author the
**erasure Slice-0 design** (registry + total projector + `SourceId` + `erase_source` + WAL + telemetry
redaction + op-store + the `_legacy:` migration) on top of `dev/design/0.8.20-erasure-and-h-end-state-v4.md`. Record the
**eu7-basis** and **embed_batch_cls-TS-parity** decisions (F-22). Fold **TC-RUBRIC-5** into the process gate.
Run **codex §9** on the package, persist the transcript (TC-RUBRIC-7), and take X0 to HITL.
**TC-11 is CLOSED — do not re-open it.**

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

---

## 11. Open questions for the human (raise at Slice 0)

1. **Publish gate.** 0.8.20 is the **first real publish** (`0.8.9 → 0.8.20`) and a **coordinated breaking pair**.
   Confirm the cut, and confirm Memex `0.5.x-successor` is co-land ready. *(Publish is never implied by build.)*
2. **eu7 basis** (F-22). Pin A keeps `SearchHit.id` byte-identical ⇒ the **no-op basis is expected to hold**.
   Confirm no-op after Slice-40 proves it, or authorize a bounded re-baseline.
3. **`embed_batch_cls` TS-binding parity** (F-22): add-TS, or ratify Py-first? Folds into X1.
4. **Adoption arms** (build ≠ adopt, F-21): does any Phase-2 item change **shipped default behavior**? Each such
   item needs its own adoption call. *(Default expectation: read-modes/registry/readiness are opt-in;
   the erasure fixes are defect repairs and ship ON.)*
