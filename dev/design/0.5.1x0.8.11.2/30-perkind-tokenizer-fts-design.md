# Per-kind precision tokenizer FTS — design-on-spec (~0.8.15)

> **DESIGN-ON-SPEC — targeted at fathomdb ~0.8.15, NOT 0.8.11.x. Paired with Memex 0.5.3.
> Do not implement now.**
>
> This is a *contingent, source-anchored* design produced so that — **if** the value test (§4) clears —
> there is a ready build plan for **per-kind tokenizer selection** (precision / no-stemming for
> identifier kinds vs recall / stemming for prose). It is **not** a build order. Per the R-I4 / Q-B5
> resolution (HITL 2026-06-30, `dev/design/0.5.1x0.8.11.2/10-fathomdb-side-design.md` §6), custom /
> per-kind tokenizers are **off-table for 0.8.x** and any per-kind normalization stays Memex-side. The
> companion multi-field design (`20-multifield-fts-design.md` §1.3c, §10.8) explicitly flags *this*
> feature — per-kind precision tokenization — as the **least-substitutable** of the three FTS residuals,
> i.e. the one most likely to be genuinely needed. This document gives that residual a concrete design so
> HITL can weigh it on its merits, paired with **Memex 0.5.3**. **Nothing here is greenlit.** The
> load-bearing section is **§4 (Value Test)**: it must pass before any of §3 is built.
>
> **Reconciling "~0.8.15" with "off-table for 0.8.x" (no contradiction).** Under the *current* posture
> the feature is off-table for the 0.8.x line — that is the standing default, in force unless changed.
> `~0.8.15` names the **earliest release at which it could land** *if and only if* §4 passes **and** HITL
> ratifies the governance re-open (§5.3). Promotion into the `plan-0.8.15.md` ladder is **itself** the
> governance act that lifts the off-table status; absent that explicit promotion it stays deferred (and
> may slip past the 0.8 line entirely). So: deferred-by-default now; **0.8.15 is the candidate target,
> not a commitment** — the value test + governance sign-off are the gate that decides.

---

## 0. Provenance, scope, and what this is not

This design is a **scoped revival** of the v0.5.x **per-kind FTS tokenizer-profile** surface
(`FtsProfile` / `configure_fts` / `resolve_tokenizer_preset` / `projection_profiles`) that the
0.6.0 "5-verb strip" deliberately removed. The removed surface still exists in git for reference at
tag `v0.5.6`:

- The tokenizer-preset table is `TOKENIZER_PRESETS` in
  `crates/fathomdb-engine/src/admin/mod.rs:374-386` (v0.5.6), with the two names this design cares about:
  - `recall-optimized-english` → `"porter unicode61 remove_diacritics 2"` — **identical to the current
    0.8.x global default** (see §1.1).
  - `precision-optimized` → `"unicode61 remove_diacritics 2"` — **same normalization, no `porter`
    stemming**.
  - resolved by `resolve_tokenizer_preset` (admin/mod.rs:390-397; unknown names pass through as raw
    FTS5 tokenizer strings).
- The stored per-kind profile type was `FtsProfile { kind, tokenizer, active_at, created_at }`
  (admin/mod.rs:325-337), persisted in a `projection_profiles` table and (re)activated via
  `set_fts_profile` / `configure_fts`, with `preview_projection_impact` (admin/mod.rs ~`ProjectionImpact`,
  362-374) surfacing the rebuild cost behind an `agree_to_rebuild_impact` gate.

Re-introducing any part of this surface is a **governance event** (§5.3) — it re-opens a per-kind
declaration surface that was removed on purpose, exactly as `20-multifield-fts-design.md` §9.3 frames the
`ProjectionTarget` revival.

**Tokenizer axis, not ranking axis.** Like the multi-field design, this is a **recall/precision-of-the-
candidate-pool** change, not a ranking-weight change. It changes *how `body` text is tokenized into the
FTS index for a given kind*, which changes *what `bm25 MATCH` can find* for exact identifiers. Ranking
stays uniform BM25 + CE-rerank (`SearchHit.ce_score`); no per-column weights are added (that remains the
§0 non-goal of the multi-field design).

**Relationship to `20-multifield-fts-design.md`.** Orthogonal and composable. Multi-field changes *which
fields' text enters `body`* (recall via more fields); this design changes *how that text is tokenized*
(precision via no-stemming for identifier kinds). They share the same per-kind-registry + drop/recreate +
reproject machinery and the same default-off discipline, and could ship as one program — but each carries
its **own** value test and its own governance sign-off.

---

## 1. The need — precision / no-stemming for identifier kinds

### 1.1 What FathomDB FTS does today (global, fixed, per-VTABLE)

The 0.8.x engine has exactly **one global, fixed tokenizer**, set by FTS5 `tokenize=` at table-create
time and changeable only by drop+recreate (FTS5 has no `ALTER … tokenize`):

- Node FTS vtable `search_index` is created with `tokenize = 'porter unicode61 remove_diacritics 2'`
  (`src/rust/crates/fathomdb-schema/src/lib.rs:268-278`, migration step 11 — the
  `MIGRATION-ACCRETION-EXEMPTION` tokenizer-default upgrade that drops+recreates the vtable).
- Edge FTS vtable `search_index_edges` is created with the **same** tokenizer string
  (`src/rust/crates/fathomdb-schema/src/lib.rs:355-360`, migration step 14, G11).
- On migration into step 11, the engine re-tokenizes from the canonical source rows via the open-path
  `reproject_search_index_after_tokenizer_upgrade` (`engine/src/lib.rs:7776-7802`).

The `porter` token in that string is the **Porter stemming** filter. It is applied uniformly to *every*
kind's `body` — there is no per-kind, per-row, or per-vtable variation, because **an FTS5 tokenizer is a
property of the virtual table, not of a row or a kind.** That per-VTABLE constraint is the central
architectural fact this design must work around (§2).

### 1.2 Why stemming corrupts exact-identifier matching

`porter` stemming normalizes inflected English forms to a common root so that prose queries recall
related word forms (e.g. *running / ran / runs* → *run*). That is exactly what you want for natural-
language prose kinds — it is a **recall** win. But for kinds whose `body` is an **exact identifier,
mapping key, or structured token** (not English prose), the same normalization is **destructive**:

- Porter mangles identifier-shaped tokens by stripping suffixes it (wrongly) reads as English inflections
  — e.g. a trailing `-s`, `-ed`, `-ing`, `-ies`, `-ization` inside a symbol, slug, or mapping key gets
  stemmed, so `WMSemanticMapping`/`WMProvenanceLink` keys that differ only by such a suffix **collide or
  mis-normalize**, and a query for the exact key matches the *wrong* row or fails to match at all.
- Stemming is **lossy and non-invertible**: once two distinct identifiers stem to the same root, FTS
  cannot tell them apart — a precision failure that no amount of CE-reranking can fix (rerank only
  re-orders the pool; it cannot recover an identifier the tokenizer already conflated).

The fix is the `precision-optimized` preset: `unicode61 remove_diacritics 2` — the **same** Unicode
normalization and diacritic folding, **without** `porter`. Tokens are matched as-written, so exact
identifiers stay exact. This is precisely the discriminating choice m006 made (§1.3).

### 1.3 The Memex requirement (m006) — the concrete driver

Memex migration `m006_configure_fts_tokenizers`
(`memex/src/memex/migrations/versions/m006_configure_fts_tokenizers.py`) is the real-world driver. Its
`upgrade()` (lines 35-37) splits Memex's node kinds into two groups and applies a different tokenizer to
each via the (v0.5.x) `engine.admin.configure_fts(kind, tokenizer, agree_to_rebuild_impact=True)` verb
(line 48):

- **`_PRECISION_KINDS` (lines 29-32): `WMSemanticMapping`, `WMProvenanceLink`** → `precision-optimized`
  (no stemming). These are exact identifier / mapping kinds where stemming corrupts matching (§1.2).
- **`_RECALL_KINDS` (lines 14-28): 13 prose kinds** (`WMGoal`, `WMTask`, `WMKnowledgeObject`,
  `WMObservation`, …) → `recall-optimized-english` (porter stemming) — the same as today's global
  default, applied deliberately because prose *wants* stemming.

m006 thus encodes the exact need: **two identifier kinds need no-stemming; the prose kinds keep
stemming.** Under the current 0.8.x engine, m006 has **no API to call** — the per-kind tokenizer surface
it targets was stripped in 0.6.0 (§0). Memex 0.5.1 therefore adopts the governed 0.8.x FTS *as-is* (one
global `porter` tokenizer) and carries this as a **known deferred gap**; closing it is the
**Memex-0.5.3-paired** work this design covers.

---

## 2. The core architectural challenge + approach

### 2.1 The constraint: FTS5 tokenizer is per-VTABLE, not per-row or per-kind

You cannot tokenize two rows of the same `search_index` vtable with two different tokenizers. `tokenize=`
is fixed DDL set once at `CREATE VIRTUAL TABLE` time (schema lib.rs:272-277). So "precision tokenizer for
kind A, recall tokenizer for kind B" **cannot** be expressed within a single FTS5 table.

### 2.2 The approach: per-tokenizer-group FTS vtables + write/query routing

Introduce **one additional FTS vtable per distinct tokenizer group**, alongside the existing recall-
default vtables, and **route each kind to the vtable whose tokenizer it is configured for** at both WRITE
and QUERY time. For the m006 requirement exactly two groups exist, so the concrete shape is:

- **Recall group (default, unchanged):** `search_index` + `search_index_edges`, tokenizer
  `porter unicode61 remove_diacritics 2`. Every kind lands here **by default** — no opt-in ⇒
  byte-identical to today.
- **Precision group (new, opt-in):** `search_index_precision` (+ `search_index_precision_edges` if edge
  precision is ever needed), tokenizer `unicode61 remove_diacritics 2` (no `porter`). A kind lands here
  **only** if it is explicitly configured `precision-optimized`.

This revives the v0.5.x **per-kind tokenizer-preset selection** (`resolve_tokenizer_preset`,
admin/mod.rs:390-397) — but instead of one mutable global vtable, the preset selects **which fixed-
tokenizer vtable a kind's rows are projected into and queried from.** Generalizes to N groups (one vtable
per preset in `TOKENIZER_PRESETS`) but **N = 2 is the only configuration the requirement needs**; the
design recommends shipping exactly the recall + precision groups and treating further presets
(`global-cjk`, `substring-trigram`, `source-code`, admin/mod.rs:381-385) as YAGNI until a real consumer
asks (§6 Q3).

### 2.3 Per-kind config, mirroring the existing vector opt-in gate

The "which tokenizer group does this kind use" decision is a **per-kind registry membership probe** —
the engine already has this exact pattern for vector indexing:

- `kind_is_vector_indexed(connection, kind)` probes `_fathomdb_vector_kinds`
  (`engine/src/lib.rs:8711-8719`), returning `false` (→ default behavior) for any unregistered kind.
- The write path branches on it at `engine/src/lib.rs:9600` (`if kind_is_vector_indexed(&tx, kind) …`),
  with the **non-indexed kind taking the unchanged path** (lib.rs:9607-9611, "Non-vector-indexed nodes
  will never be projected").

This design mirrors that **verbatim in shape**: a `_fathomdb_fts_tokenizer_kinds(kind TEXT PRIMARY KEY,
preset TEXT NOT NULL, created_at INTEGER)` registry and a `fts_tokenizer_group(connection, kind) ->
TokenizerGroup` probe that returns `Recall` (the default) for any unregistered kind. **Default = current
global recall (`porter`); a kind opts into `precision-optimized` by registering a row.** Default-off,
default-recall, default-byte-identical.

---

## 3. Implementation design

### 3.1 Schema migration — add the precision vtable + the registry (drop+recreate pattern)

A new forward-only migration step adds (a) the per-kind registry table (additive `CREATE TABLE`, no
exemption marker — matches the accretion-guard `names_addition` branch, like `_fathomdb_vector_kinds`)
and (b) the precision FTS vtable, created with the no-stemming tokenizer:

```text
-- additive registry (no FTS reshape → no exemption marker)
CREATE TABLE IF NOT EXISTS _fathomdb_fts_tokenizer_kinds(
    kind        TEXT PRIMARY KEY,
    preset      TEXT NOT NULL,          -- e.g. 'precision-optimized'
    created_at  INTEGER NOT NULL DEFAULT 0
);

-- new precision-group vtable, no `porter` (mirrors schema lib.rs:272-277 sans stemming)
CREATE VIRTUAL TABLE IF NOT EXISTS search_index_precision USING fts5(
    body,
    kind UNINDEXED,
    write_cursor UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);
```

Notes:

- Creating a **new, initially-empty** vtable is purely additive — **no drop+recreate, no exemption
  marker** on the create itself. The drop+recreate+reproject pattern (the step-11 tokenizer-upgrade
  idiom, schema lib.rs:268-278 + engine lib.rs:7776-7802, carrying `MIGRATION-ACCRETION-EXEMPTION`) is
  reused **only** if a configured kind's *existing* rows must be moved between groups (§3.4 reproject),
  not by the migration.
- The edge precision vtable `search_index_precision_edges` is created **only if** edge-kind precision is
  in scope; the m006 requirement is node-kinds only (`WMSemanticMapping`/`WMProvenanceLink` are node
  kinds), so the recommended first cut is **nodes-only** and edges stay recall-default (§6 Q4).

### 3.2 Write-path routing (insert into the right vtable by kind config)

Today the node write unconditionally inserts into `search_index` (`engine/src/lib.rs:9596-9599`). The
change adds a **per-kind group branch** in front of the FTS insert, exactly mirroring the vector gate at
lib.rs:9600:

```text
// at the node write site (engine lib.rs:9596-9599):
match fts_tokenizer_group(&tx, kind)? {
    TokenizerGroup::Recall =>                          // unchanged default path
        tx.execute(
            "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, ?2, ?3)",
            params![body, kind, cursor])?,
    TokenizerGroup::Precision =>
        tx.execute(
            "INSERT INTO search_index_precision(body, kind, write_cursor) VALUES(?1, ?2, ?3)",
            params![body, kind, cursor])?,
}
```

Key properties:

- **Mutually exclusive routing.** A kind's rows live in **exactly one** group's vtable — never both — so
  there is no double-indexing and no row duplication. The `body` content is unchanged (this design does
  *not* alter `body`; that is the multi-field design's job — §0 orthogonality).
- **Default path byte-identical.** An unregistered kind takes `TokenizerGroup::Recall`, i.e. the
  identical `INSERT INTO search_index` it takes today. Stores that never configure a precision kind are
  byte-for-byte unchanged (mirrors lib.rs:9607-9611's non-vector default).
- The same group-branch is added at the **edge** write site (`engine/src/lib.rs:9703-9708`) **only if**
  edge precision is in scope; otherwise edges remain unconditional `search_index_edges` inserts.

### 3.3 Query-path routing (search the right vtable / union)

The text-search read issues one `bm25 MATCH` over `search_index` today
(`engine/src/lib.rs:6115-6121`). With rows split across two group-vtables, the reader must search **both
groups and merge**, because a query is not pre-classified by kind. Two viable shapes — HITL chooses (§6
Q2):

- **Shape Q-A — UNION ALL across group vtables (RECOMMENDED).** Run the same `bm25 MATCH` against
  `search_index` **and** `search_index_precision`, `UNION ALL` the rows, and let the existing
  fusion/order path handle ranking. Because the two vtables are **disjoint by kind** (§3.2), the union is
  a clean concatenation with no dedup needed. Ordering by `bm25()` across two vtables is **rank-fused**,
  not raw-score-compared — consistent with the existing fusion discipline (the codebase already fuses
  `bm25()`/dense on **rank**, never raw, `engine/src/lib.rs:1093`, 6096-6100). The returned
  `SearchHit.body` is still sourced via the `canonical_nodes` JOIN already present at lib.rs:6116-6118,
  so the public hit contract is unchanged.
- **Shape Q-B — kind-pre-filtered single-vtable query.** If the query already carries a kind filter
  (`SearchFilter`, #17 / 0.8.11), route to **only** the vtable that holds that kind and skip the union.
  This is a pure optimization layered on Q-A (Q-A is the correctness floor; Q-B is the fast path when the
  caller has constrained kinds).

**Default-off keeps the query path byte-identical:** if no precision kind is registered,
`search_index_precision` is empty, the UNION's second leg returns zero rows, and the result is identical
to today's single-vtable query. (An implementation may also *skip* the second leg entirely when the
registry is empty — a cheap registry-count probe — so default stores pay no query cost at all.)

### 3.4 Both rebuild paths must route by group

There are **two** code paths that reproject FTS from canonical source rows, and **both** must learn the
group routing or they will rebuild every row into the recall vtable and silently erase precision routing:

1. **Shadow rebuild** (`include_fts` branch, `engine/src/lib.rs:5031-5062`). Today it truncates
   `search_index` + `search_index_edges` (the `DELETE FROM` at lib.rs:5011-5015) and re-inserts every
   `row.body` into `search_index` (lines 5033-5037). It must (a) **also truncate the precision vtable**
   `search_index_precision` (and `…_edges` if in scope) in the same transaction — clearing/refilling
   **every** tokenizer-group vtable, not just routing the inserts — or stale precision rows survive a
   rebuild and a kind moved back to recall keeps phantom precision hits; and (b) route each re-inserted
   row to its group's vtable via the same `fts_tokenizer_group(kind)` probe used at write time (§3.2).
2. **Tokenizer-upgrade reproject** (`reproject_search_index_after_tokenizer_upgrade`,
   `engine/src/lib.rs:7776-7802`). After a drop+recreate it deletes and re-inserts every canonical node
   body into `search_index` (lines 7780-7787). It too must route by group **and** must clear/refill
   `search_index_precision` in the same transaction, or a tokenizer upgrade would drop all precision
   routing.

A **single shared router helper** `fts_target_vtable(kind) -> &str` (or the `compose`-style helper the
multi-field design factors out, if both ship together) called from **all three sites** (write + both
rebuilds) is the only safe factoring — it guarantees a store's FTS layout does not depend on *when* it
was last rebuilt. This single-helper invariant is a **correctness requirement**, not a style choice
(same argument as `20-multifield-fts-design.md` §6).

An **explicit reproject** entry point (triggered by the admin verb, §3.5) reuses the shadow-rebuild path
so that *changing* a kind's preset — e.g. registering `WMSemanticMapping` as precision after rows already
exist in the recall vtable — moves its existing rows to the correct group's vtable on demand (the v0.5.x
`agree_to_rebuild_impact` / `preview_projection_impact` gate, admin/mod.rs `ProjectionImpact`, is the
precedent for surfacing that rebuild cost before it runs).

### 3.5 Admin verb (revive the per-kind tokenizer selection m006 used)

Extend the existing admin surface rather than inventing a new verb family. `admin_configure`
(`src/rust/crates/fathomdb-py/src/lib.rs:1036-1064`) today only registers an **operational collection**
(`kind ∈ {latest_state, append_only_log}`). Add a sibling admin operation
`admin.configure_fts_tokenizer(kind, preset, agree_to_rebuild_impact=False)` that:

1. Resolves `preset` against the revived preset table (`TOKENIZER_PRESETS` /
   `resolve_tokenizer_preset`, v0.5.6 admin/mod.rs:374-397). For the first cut, the **accepted set is
   exactly `{recall-optimized-english, precision-optimized}`**; unknown/other presets are rejected with a
   typed error (do **not** pass arbitrary raw tokenizer strings through — that re-opens more surface than
   the requirement needs; §6 Q3).
2. Writes/updates the `_fathomdb_fts_tokenizer_kinds` row (cursored, replayable, auditable — it rides the
   same `PreparedWrite` / `Engine::write` path as `admin_configure` and returns a `PyWriteReceipt`).
3. If existing rows of that kind are already in the wrong group, requires `agree_to_rebuild_impact=True`
   and triggers the explicit reproject (§3.4) — matching the m006 call shape
   `configure_fts(kind, tokenizer, agree_to_rebuild_impact=True)` (m006 line 48).

It is exported in the pymodule alongside `admin_configure`, with a matching TS (napi) binding and
`_fathomdb.pyi` stub, and added to `governed-surface-allowlist.json` (the X1 SDK-parity discipline that
binds every governed-surface add). Registration **does not** retro-project by default; it changes routing
for **subsequent** writes plus the explicit opt-in reproject.

### 3.6 Backward-compat / default-off / probe-gated

- **Default-off is byte-identical.** No precision kind registered ⇒ empty registry ⇒
  `fts_tokenizer_group` returns `Recall` everywhere ⇒ write, query, and both rebuild paths take the
  unchanged `search_index` branch. The precision vtable exists but stays empty. Indistinguishable from
  today at the byte level (the vector-gate "behaves as before" property, lib.rs:9607-9611).
- **Existing stores unaffected until reprojected.** The migration adds an empty registry + empty vtable
  only; existing FTS content is untouched. Precision routing applies to rows written after a kind is
  configured, or after an explicit reproject.
- **Probe-gated registration (fail-closed).** Registration is rejected unless a probe confirms the kind
  exists and the preset is in the accepted set, mirroring the migration-preflight fail-closed discipline
  (schema lib.rs preflight) and the multi-field probe-gate (`20-...md` §2.1).
- **Crash-safety reuses existing idioms.** The reproject rides the same single-transaction durable-marker
  pattern as the tokenizer reproject (engine lib.rs:7776-7802 — reindex + completion marker commit
  together; crash-before-commit re-runs, after-commit skips; idempotent).

---

## 4. VALUE TEST (REQUIRED — the load-bearing section)

**This is the gate. It must pass before any of §3 is built.** The question it answers:

> *Does `porter` stemming actually degrade exact-identifier matching enough — on the two real identifier
> kinds — to justify reviving a per-kind tokenizer surface 0.6.0 removed?*

Unlike the multi-field design (whose skeptical prior is "the modern stack already recovers it"), this
feature attacks a **precision** failure that CE-rerank **cannot** fix: rerank only re-orders the pool; an
identifier the tokenizer has already conflated never enters the pool distinctly. So the test is a direct
**stemming on vs off** comparison on identifier kinds — no rerank/vector ablation is needed to isolate the
effect (rerank is downstream of the tokenizer conflation).

### 4.1 Corpus + query set

The natural corpus is **Memex's own `WMSemanticMapping` + `WMProvenanceLink` rows** (the two
`_PRECISION_KINDS`, m006 lines 29-32) — this is the population the feature exists to serve, and Memex
0.5.3 supplies it. Construct a gold set of **exact-identifier lookup queries**: for each identifier/
mapping key present in those kinds, the query is the exact key (and near-miss variants — keys differing
only by a porter-stemmable suffix, the §1.2 collision case), and the gold answer is the *specific* row
that key denotes.

If the live Memex identifier corpus is unavailable at test time, a **synthetic identifier corpus** that
deliberately includes porter-collision pairs (e.g. `…Mapping` vs `…Mappings`, `…Link` vs `…Linked`) is
the fallback — but the verdict is **provisional** until confirmed on real Memex data (the multi-corpus /
real-gold discipline from `0.8.11-handoff-to-0.8.15.md` §2).

### 4.2 Conditions (stemming on vs off)

```text
{ tokenizer:  porter (recall-optimized-english, today's global default)
            | no-porter (precision-optimized) }
```

Index the **same** identifier rows under each tokenizer (the precision vtable vs the recall vtable) and
run the **same** exact-identifier query set against each.

### 4.3 Metrics

- **precision@k (k ∈ {1, 5})** — for an exact-key query, is the *correct* row the top hit, and is it
  un-contaminated by stemming-collided siblings in the top-k. **Primary** (this is the precision the
  feature targets).
- **exact-match recall@1** — does the exact key retrieve its own row at all (a stemmed index can fail to
  return the literal key).
- **collision rate** — fraction of identifier queries whose top-k under `porter` contains a
  *wrong* identifier that stemmed to the same root (the direct mechanism of §1.2). Zero is the
  precision-optimized expectation.
- **prose-recall guard (negative control)** — on a sample of `_RECALL_KINDS` prose queries, confirm
  no-porter does **not** degrade prose recall enough to matter (it should, by design, lose some
  inflection recall — this guard quantifies the cost of *mis*-applying precision to prose, justifying the
  per-kind split rather than a global tokenizer change).

### 4.4 The decision rule

Let `Δp@1 = precision@1(no-porter) − precision@1(porter)` and `C = collision rate under porter`, both on
the identifier kinds.

> **Per-kind precision tokenization is genuinely valuable IFF, on the identifier kinds,
> `Δp@1 ≥ +0.05` absolute (equivalently exact-match recall@1 improves ≥ +0.05) with the lower bound of
> its 95% CI > 0, OR the porter collision rate `C ≥ 0.05` (≥1 in 20 identifier queries returns a
> stemming-conflated wrong identifier in top-k).** Either condition clearing is sufficient — a material
> collision rate is itself a correctness defect even if p@1 looks acceptable on average.
>
> **If both `Δp@1` collapses toward 0 (CI includes 0, point estimate < +0.02) AND `C < 0.02`, then
> `porter` does NOT meaningfully corrupt these identifiers, and FathomDB does NOT build the feature** —
> Memex keeps the global recall tokenizer and the gap stays closed-as-acceptable.

The `+0.05` / `1-in-20` bar mirrors the decisive-margin discipline used elsewhere in 0.8.x gating (the
M1 graph-arm CI-upper +0.04 rule; the multi-field `Δ_persist ≥ +0.05` rule, `20-...md` §8.4). The
prose-recall guard (§4.3) is a **secondary veto in the other direction**: it confirms the feature must be
*per-kind*, not a global flip — if no-porter helped prose too, you'd just change the global default and
skip this whole design.

### 4.5 What a pass / fail produces

- **PASS** → promote this design-on-spec to a build plan; HITL governance sign-off on re-opening the
  per-kind tokenizer surface (§5.3) still required; build paired with Memex 0.5.3 so the two identifier
  kinds are configured the moment the engine supports it.
- **FAIL** → record the verdict; FathomDB ships **no** per-kind tokenizer; Memex keeps the global recall
  tokenizer; close the m006 gap as "acceptable under global `porter`."

---

## 5. Cost / effort + governance

### 5.1 Engineering cost — MEDIUM

New surface area: a `TokenizerGroup` enum + preset-resolution (revive `resolve_tokenizer_preset`,
admin/mod.rs:390-397), a registry table + membership probe (mirrors
`_fathomdb_vector_kinds` / `kind_is_vector_indexed`, engine lib.rs:8711-8719), the new precision vtable
(schema migration), **write-path routing** (engine lib.rs:9596-9599 + the edge site 9703-9708 if in
scope), **query-path routing** (the UNION/skip logic over lib.rs:6115-6121), **both rebuild-path edits**
(engine lib.rs:5031-5062 + 7776-7802), the admin verb (`admin.configure_fts_tokenizer`, extending
fathomdb-py lib.rs:1036-1064 + exports), Python/TS bindings + `.pyi` stub + governed-surface allowlist,
and the §4 value-test harness. Comparable in size to the multi-field design (`20-...md` §9.1) — both ride
the same registry + drop/recreate + reproject machinery; the incremental delta over multi-field, if they
ship together, is mostly the second vtable + the query-time UNION.

### 5.2 Runtime cost — WRITE + STORAGE + a query-routing branch, NOT a per-response tax

- **Write:** one extra registry probe per write of a configured kind (cheap, single-row PK lookup,
  identical cost shape to `kind_is_vector_indexed`); the insert itself is the same single-row FTS insert,
  just into a different vtable.
- **Storage:** a second FTS index. But because routing is **mutually exclusive by kind** (§3.2), total
  indexed rows are **unchanged** — precision-kind rows move out of `search_index` into
  `search_index_precision`; there is no duplication. The only storage delta is FTS5 per-table overhead
  for the second vtable (small, fixed).
- **Query:** a UNION over two vtables when ≥1 precision kind is registered (Shape Q-A), reducible to a
  single-vtable query when the caller supplies a kind filter (Shape Q-B) or when the registry is empty
  (skip the second leg). **No new query API; no LLM; no per-response model tax** — the query path stays
  CPU-only and deterministic, consistent with the footprint invariant.

### 5.3 Governance note (REQUIRED)

This design **re-opens the per-kind FTS tokenizer-profile surface that 0.6.0 deliberately removed**
(`FtsProfile` / `configure_fts` / `set_fts_profile` / `projection_profiles` /
`resolve_tokenizer_preset`, v0.5.6 admin/mod.rs:325-397). Re-introducing even the scoped two-preset
subset is a reversal of a prior simplification decision and must be an **explicit HITL governance call** —
not an incremental feature add — exactly as `20-multifield-fts-design.md` §9.3 frames the
`ProjectionTarget` revival. The scoping discipline (two presets only; node-kinds first; default-off;
probe-gated; no arbitrary raw tokenizer strings) is what keeps the revival *minimal*, but it is still a
revival. **Do not build without that sign-off, and not before §4 passes.** Per the standing 2026-06-30
resolution this is **off-table for 0.8.x**; the target is **~0.8.15, paired with Memex 0.5.3**.

---

## 6. Open questions for HITL

1. **Build at all?** This is the *least-substitutable* of the three FTS residuals (`20-...md` §1.3c,
   §10.8) — but it is still gated by §4. Is there confirmation from Memex that `porter` is *observably*
   corrupting `WMSemanticMapping`/`WMProvenanceLink` lookups in production, or is §4 run cold to find out?
2. **Query shape — Q-A union vs Q-B kind-pre-filter** (§3.3). Q-A (UNION across both group vtables) is the
   correctness floor and is recommended; Q-B (route to one vtable when a kind filter is present) is a
   layered optimization. Ship Q-A first, Q-B as a fast-path follow-up — agreed?
3. **Preset surface width** (§3.5). Recommend accepting **only** `{recall-optimized-english,
   precision-optimized}` and rejecting the other v0.5.x presets (`global-cjk`, `substring-trigram`,
   `source-code`, admin/mod.rs:381-385) and arbitrary raw tokenizer strings until a real consumer asks.
   Confirm the minimal two-preset set, or widen?
4. **Nodes-only vs nodes+edges** (§3.1). The m006 requirement is node-kinds only. Recommend a nodes-only
   first cut (no `search_index_precision_edges`). Any near-term edge-identifier need that argues for doing
   edges at the same time?
5. **Joint with multi-field?** This design and `20-multifield-fts-design.md` share the registry +
   drop/recreate + reproject machinery and both pair with Memex 0.5.3. Ship as **one** FTS program (one
   migration, one admin-verb family, one reproject) or as two independently-gated efforts? (Each keeps its
   own value test regardless.)
6. **Value-test threshold** (§4.4). Is `Δp@1 ≥ +0.05` **OR** porter-collision-rate `C ≥ 0.05` the right
   bar, or should the collision-rate veto be stricter (a single wrong-identifier collision is arguably a
   correctness defect at any rate)?
7. **Reproject default** (§3.4-3.5). On `configure_fts_tokenizer` for a kind that already has rows, should
   the engine require `agree_to_rebuild_impact=True` (the m006 contract) and reproject eagerly, or only
   route *new* writes and leave existing rows until an explicit reproject is called?
