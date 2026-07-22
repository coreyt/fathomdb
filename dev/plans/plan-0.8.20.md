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
| `transition` / `purge` **shipped in both SDKs** | 0.8.19 Phase-1 surface is live | ✓ `fathomdb-py:1274,1297`; `fathomdb-napi:1070,1102` |
| **Phase-2 surface = 100 % NET-NEW** | `ReadView`, `valid_from`, `valid_until`, `dense_readiness`, `configure_projections`, `ProjectionSpec`, `EntityTypeSpec`, `id_prefix` | ✓ **ZERO hits** across all crates |
| `derive_logical_id` = `SHA256("{kind}:{name}")` | natural-key derivation, **not** an opaque surrogate | ✓ `engine lib.rs:11152` |
| `search_index_v2` = **content-storing** FTS5 | holds the **body verbatim** (no `content=''`) | ✓ `fathomdb-schema/src/lib.rs:427` |
| `truncate_wal()` **already exists** | `PRAGMA wal_checkpoint(TRUNCATE)`, returns typed `TruncateWalStatus::{Done,Busy}` | ✓ `engine:6379`; **CLI-only** (`fathomdb-cli:389`); **NOT called by `purge`/`excise`** |
| op-store record erasure | **does not exist** — only a cap-based retention sweep | ✓ `engine:10083` (`enforce_provenance_retention`) |
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

---

## 4. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 25 → 30 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | **X0 design gate** — reqs/ACs frozen; erasure Slice-0 design; **TC-11 pin ✅ ALREADY RATIFIED**; eu7-basis + embed_batch_cls-TS decisions; **TC-RUBRIC-5** (dedicated-checkout/`preflight.sh --landing`) folded in; stand up `runs/STATUS-0.8.20.md`; **codex §9** | design-adr + steward-review | — |
| **5** | **Erasure completeness** (R-20-E1…E8) — registry + total projector + `erase_source` + provenance + WAL + telemetry + op-store + migration | implementation | 0 |
| **10** | **`ReadView` / read-modes** + **node-validity** (R-20-RV, R-20-NV) — ✅ **COMPLETE on-branch @ `93a57b10`** (SCHEMA 21→22) | implementation | 0 |
| **15** | **Projection registry (C-1 co-land) + EAV/property-FTS** (R-20-PR, R-20-EAV) **+ TC-34 node-validity write-side authoring verb + TC-33 temporal-representation harmonisation** *(both folded in by HITL 2026-07-20)* | implementation | 0, 10 |
| **20** | **`dense_readiness` + `flush_embeddings()`** (R-20-DR) | implementation | 15 |
| **25** | **Surrogate minting — registry-admitted governed entities ONLY** (R-20-SUR) | implementation | 15 |
| **30** | **RUBRIC-H7 `can-i-deploy` contract-conformance gate** (R-20-H7) | implementation | 10,15,20,25 |
| **40** | **Verification + release readiness** — full DoD, X1, **AC-079 sign-off** (R-20-EU7 is **CLOSED — run NO eu7**, see §3), **publish-or-hold per the HITL gate** | verification | 5,30 |

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
> reads as **"still valid"** (`fathomdb-schema/src/lib.rs:339`; relied on at `engine:9910`, `:10059`,
> `:10455`) — i.e. junk silently **resurrects an invalidated edge**. Required, defence in depth:
> **enforce `strftime('%s', <user value>)` and reject a NULL result with a typed error at the write
> boundary**, plus schema-level `NOT NULL`/`CHECK` constraints so the invariant is structural rather
> than upheld by call sites, plus a RED test proving malformed input **fails loudly**. Note
> `temporal_fallback` is matched by **string equality** against `substituted_t_valid`
> (`engine:4253`, `:4434`) — a representation change silently stops flagging fallback edges, so that
> comparison must be re-grounded, not left to drift.
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
> `migrate_vector_partition_pack1_to_pack2` (`engine:13492-13529`) — **follow that precedent.**
>
> **The four load-bearing conditions (steward-investigated against code; all MUST hold or query results
> go silently wrong):**
>
> 1. **List `rowid` explicitly** in the re-insert — a vec0 row maps to its node by `rowid == write_cursor`
>    (`engine:7086`, `:10005`); letting vec0 auto-assign rowids silently decouples every embedding.
> 2. **New attribute column is plain metadata OR a partition key — NEVER a vec0 `aux`/`+` column.** An aux
>    column hard-**errors** every filtered KNN query (`engine:13460-13464`).
> 3. **Back-fill old rows with the `''` sentinel** (vec0 TEXT metadata is NOT-NULL-able, `engine:7091`) so
>    they cleanly fail-to-match a filter rather than erroring.
> 4. **Copy `embedding_bin` verbatim via `vec_bit(...)` — do NOT re-quantize.** Re-deriving bits from the
>    raw `embedding` leaves old rows quantized **un-centered** while new rows stay mean-centered ⇒
>    incomparable Hamming distances, silent recall corruption, no error (`engine:13514-13527`, and the
>    anti-pattern to avoid at `:13584-13599`).
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
>   status`); attribute-filtering is **engine-internal only** (comment `engine:5561` — "a later slice adds
>   that surface"). So the edge-semantics choice governs a feature no consumer can call this release; (A)
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
TC-33.** It is the Phase-2 keystone: 20 and 25 both depend on it. Slice 30 (H7) additionally depends on
10/15/20/25.

> **⚠ SLICE 15 IS OPEN. It had FOUR parts; ONE has closed.**
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
  stay blocked**.

---

## 11. Open questions for the human (raise at Slice 0)

> **Status: ALL RESOLVED at the X0 sign-off (HITL, 2026-07-19)** — see §10. Retained for the record.

1. **Publish gate.** 0.8.20 is the **first real publish** (`0.8.9 → 0.8.20`) and a **coordinated breaking pair**.
   Confirm the cut, and confirm Memex `0.5.x-successor` is co-land ready. *(Publish is never implied by build.)*
2. **eu7 basis** (F-22). ✅ **RESOLVED — CLOSED BY DECISION, ZERO runs authorized** (§3). *(The original text
   "confirm no-op after Slice-40 proves it" is **withdrawn**: nothing is to be proven by running eu7.)*
3. **`embed_batch_cls` TS-binding parity** (F-22): add-TS, or ratify Py-first? ✅ **RESOLVED — proceeds inside
   X1 per recommendation.**
4. **Adoption arms** (build ≠ adopt, F-21): does any Phase-2 item change **shipped default behavior**? Each such
   item needs its own adoption call. *(Default expectation: read-modes/registry/readiness are opt-in;
   the erasure fixes are defect repairs and ship ON.)*
