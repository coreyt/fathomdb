# Integrated design — Memex 0.5.1 × FathomDB 0.8.x storage-API refit (Option B)

> **High-level cross-repo design (#1.5).** How Memex 0.5.1 comes onto FathomDB's 0.8.x governed
> API. This is the integration view that the two per-repo designs (#1.7) derive from. It is
> grounded in three artifacts (read those for the line-level contracts):
>
> - FathomDB contract-of-record: `dev/plans/runs/B-1-fathomdb-answer-sheet.md`
> - Identity/recall model: `dev/design/0.5.1x0.8.11.2/identity-recall-model.md` (audit-confirmed)
> - Memex refit plan: `memex/dev/fathomdb/memex-0.5.1-fathom-0.8.11.2-option-b-tasklist.md`
>
> **One-paragraph thesis.** Memex's storage adapter is built on FathomDB's retired *flat* pre-0.6.0
> API; 0.8.x replaced it with a narrower **governed** surface (write-receipts + read-verbs + a fixed
> FTS projection). The sqlite→fathomdb cutover already shipped, so this is a **bounded API-generation
> refit of the adapter** (~90% inside `fathom_store.py`/`fathom_facade.py`), **not** a new cutover.
> Specific-item recall is preserved; the one genuine design risk is FTS ranking parity (R-I4).

## 1. The two systems and the contract boundary

```text
Memex product code
  │   (unchanged protocols: MemexStore / ContentStore / SearchAPI — T2.9)
  ▼
FathomStore adapter  (fathom_store.py + fathom_facade.py)   ← the refit lives here
  │   governed API (the ONLY surface that crosses the boundary)
  ▼
FathomDB 0.8.x engine  (one Engine per DB file, exclusive .lock)
```

**The governed contract (what crosses the boundary).** Everything else is retired.

| Concern | Governed 0.8.x surface | Replaces (flat, retired) |
|---|---|---|
| Write | `engine.write([{node｜edge｜op_store｜admin_schema}]) → WriteReceipt{cursor,row_cursors,dangling_edge_endpoints}` | `WriteRequest`/`NodeInsert`/`EdgeInsert`/`new_id`/`new_row_id`/`ProvenanceMode` |
| Point recall | `read.get(logical_id)` / `read.get_many([logical_id])` (active-only); op-store: `read.collection(collection) → rows, then locate by record_key within results` | by-id reads |
| Within-kind list | `read.list_filter(kind, Filter{terms})` — AND-only, allowlisted paths, `limit` only (no cursor) | `engine.nodes(kind)…` |
| Mutations/audit | `read.mutations` (op-store cursor) | — |
| Graph | `graph.neighbors` / `graph.search_expand` | `TraverseDirection` |
| Rerank/score | `SearchHit.ce_score = sigmoid(ce_logit)∈[0,1]`; knobs `alpha`/`pool_n` | — |
| Embed | `engine.embed(text) → vec` | `BuiltinEmbedder`/`VectorRegenerationConfig` (removed) |
| Op-collection register | `admin.configure(name, schema_json)` | `Operational*Register` |
| FTS / projection | **none — engine-owned fixed projection of `body`** | `ProjectionTarget`/`FtsPropertyPath*`/tokenizer/BM25 specs (removed) |

## 2. Identity & recall model (audit-confirmed — the spine)

Recall is **preserved** and **caller-keyed**; nothing to restore on FathomDB's side. The non-conflated
id/cursor roles (full table in `identity-recall-model.md`):

- **`logical_id`** (caller string, e.g. `turn:<uuid>`) — stable identity, supersession key, and the
  `read.get` recall key (active version). Memex already assigns it everywhere; `new_id()` → local `uuid4()`.
- **`record_key`** (caller, op-store) — recall key for `read.collection`.
- **`write_cursor`/`row_cursors[]`** (server u64) — write order + supersession watermark + receipt row
  identity; **not** a recall key, not stable across re-ingest. **Replaces the dropped caller `row_id`.**
- **`source_id`** (caller, nullable) — provenance/excise; not recall.
- **`stable_id`** (Cause-A, `l:<logical_id>`｜`h:<sha256(body)>`) — cross-session **search-hit** key for
  gold/telemetry; lives on `SearchHit`, not a `read.get` input.

**Audit results that de-risk the refit:** 79 `row_id=new_row_id()` args are dead weight (no read
recalls by them) → dropped; all 14 `NodeRetire` sites already key on `logical_id` (retire =
supersede-by-rewrite, requirement already met); edges resolve `from`/`to` to `logical_id`. The one
public `row_id` method `get_conversation_context` is a no-op stub with **zero product callers**; the
working windowing already exists keyed on `logical_id` + client-side body-timestamp sort → fix is a
**signature alignment only**, and **A-3 (read.list ordering) is NOT required** (stays deferred; the
T2.11 paging audit only revisits it if a single session can exceed the `read.list` ~10k cap).

## 3. FTS / projection model — the one open risk (R-I4-parity)

0.8.x has **no consumer FTS/projection API**. FTS is engine-owned and fixed: a single indexed `body`
column per node/edge, tokenizer `porter unicode61 remove_diacritics 2`, **uniform** BM25, built at
store-open by immutable internal migrations. Therefore Memex **deletes `m003–m007`** (does not rewrite
them) and **composes its searchable text into each record's `body`** (structured fields stay
filterable via `json_extract(body,'$.field')`).

**R-I4-parity** is the only refit risk without a clean fallback: multi-field / weighted / custom-tokenizer
FTS is gone. Mitigation sequence (front-loaded — see §4): inventory the `m003–m007` specs **first**;
if Memex's ranking leans on field weights / multiple paths, content-model them into `body` and prove
ranking drift is acceptable (T3.4 parity check); escalate any irrecoverable loss to HITL (Q-B5).

## 4. Migration sequence across both repos

```text
FathomDB (0.8.11.2 worktree)            Memex (feat/0.5.1-fathom-chat)
─────────────────────────────           ─────────────────────────────────────────
A-1 ✅ (action_kind allowlist)          Spike: R-I4 FTS-spec inventory (T3.1)  ◄── FRONT-LOAD
A-2 ✅ (bool-eq server-side)              │     (decides R-I4 viability before any swap)
A-3 ⏸ deferred (timestamp windowing)     ▼
Cause-A ⏳ (stable_id; verify→merge)     Phase 1  insulate the WM-search seam  (golden-lock first)
   │                                      ▼
   │  merge-gate: A-1 + stable_id reach   Phase 2  governed-verb swap (per-repository slices):
   │  Memex's pinned build only when               writes→engine.write · reads→read.get/list ·
   ▼  0.8.11.2 → main merges                       op-store · graph · admin · embed · drop row_id
(client-side fallbacks until then)        ▼
                                          Phase 3  delete m003–m007 + content-model body + parity (R-I4)
                                          Phase 4  re-point leaky files (semantic_projection, etc.)
                                          Phase 5  consume seams (ce_score, intent_hint, source_id slot)
                                          Phase 6  behavioral-equivalence gate + codex + close B-1
```

**Why this order:** insulate-before-swap (Phase 1 wraps the un-abstracted world-model search seam so
the Phase-2 swap is internal); golden-lock current behavior before swapping (equivalence baseline);
delete-not-rewrite FTS only after the R-I4 spike confirms viability; consumption (Phase 5) is additive
on top of a working swap.

## 5. Merge-gate & timing

Memex's build is pinned to `origin/main` (`ba80866d`). **A-1** and **Cause-A `stable_id`** live on the
`0.8.11.2-pico-umbrella` branch and reach Memex's build **only when 0.8.11.2 merges to main**. Until
then Memex ships **client-side fallbacks** (the `action_kind` split) and **inert slots** (`source_id`,
`stable_id`). `ce_score`, `source_id`, `read.*`, `embed` are already on `ba80866d` and consumable now.
**Cause-A must be kept merge-ready** (outstanding: main-tree `maturin develop` + py/TS import smoke).

## 6. Validation & equivalence strategy

The refit is a substrate change, so "byte-identical" is replaced by **behavioral equivalence**:
golden-lock the world-model search path before the swap (Phase 1, T1.4); read/write round-trip
equivalence across the swap (Phase 6, T6.3); FTS ranking-parity under uniform BM25 (T3.4); perf smoke
within tolerance of the flat baseline (T6.4); probe-gated capability checks (never version-gated — the
build mislabels itself `0.6.0`); codex on the merged refit branch (T6.5). Consumption slices (Phase 5)
keep the default-off byte-identical posture (flag-off == pre-refit).

## 7. Cross-repo coordination & envelopes

Bus `fathom-memex-chat.jsonl` (`{ts,from,to,type,msg}`); FathomDB Steward ↔ memex-orchestrator (+
read-only monitor). **$0/local** for the Memex refit; **NO Memex pushes / NO publish**; priced FathomDB
experiment passes announce on the bus against the pooled **\$75** envelope before spend. One-writer per
checkout; Memex builds the pinned FathomDB worktree (`0.5.1-memex-build`) with `maturin develop`, never
the Steward's checkout. HITL hard-stops only at OPP-1 Adopt-GO and any publishable cut.

## 8. Open risks & decisions

| Item | State |
|---|---|
| **R-I4-parity** (FTS field-weights/tokenizer loss) | **OPEN — the only hard risk.** Front-loaded spike (§4); escalate irrecoverable drift (Q-B5) |
| A-3 (read.list paging) | **DEFERRED** — timestamp windowing suffices; revisit only if a session exceeds the ~10k `read.list` cap (T2.11) |
| Cause-A merge / `stable_id` | pending Cause-A verify → 0.8.11.2 merge; inert slot until then |
| Q-B1 no-migration / Q-B2 keep-0.5.1 / Q-B3 Slice-15-core GO | **decided** |
| Re-baseline + re-SIGN of plan-0.5.1 | pending (#3, after the per-repo designs) — auto-SIGN if no HITL fork |
