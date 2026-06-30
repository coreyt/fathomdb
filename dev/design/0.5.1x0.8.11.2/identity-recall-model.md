# Identity & recall model — Memex 0.5.1 × FathomDB 0.8.x (building block for the integrated design)

> **HITL concern (2026-06-30):** "did the API surface lose Memex's ability to simply recall a
> specific item? If so, restore it. Don't conflate the ids/cursors/etc."
> **Verdict: NO capability was lost. Nothing to restore on the FathomDB side.** What changed is
> mechanical (the engine no longer mints caller-side ids). Verified from source + Memex usage.

## Does "recall a specific item" still exist? YES

- **Canonical nodes:** `read.get(logical_id) → Option<NodeRecord>` — first-class, **active-only**
  point lookup keyed on `logical_id` (engine `lib.rs:3661`, pyo3 `lib.rs:1077`).
  `read.get_many([logical_id]) → [Option<NodeRecord>]` batches it, request-order, partial.
- **Op-store / latest-state collections:** recall by caller `record_key` via `read.collection`
  (`after_id` cursor for ranges).
- "Active-only" nuance: `read.get` returns the **current** (non-superseded) version. Historical
  versions are not a `read.get` concern — the mutation trail is `read.mutations` (audit cursor).
  "Simply recall a specific item" = the current item = `read.get`. **Preserved.**

## Memex already recalls by `logical_id`, not by a server id — so the refit keeps recall semantics

- Memex assigns its own stable key today: `fathom_store.py:765` → `logical_id = f"turn:{new_id()}"`.
- **No read path recalls by `row_id`** (grep of `src/memex/` reads: zero `get_by_row`/`WHERE row_id`/
  `read…row_id` sites). The `row_id` references that exist are Memex's **own SQLite-era** tables
  (`doctor.py`, `conversation_search.py`, `conversation_embeddings.row_id`), not a fathomdb recall key.
- Memex's facade already encodes the truth: `fathom_facade.py:2806-2807` —
  *"Legacy callers pass integer `row_id`; fathomdb uses `logical_id` strings (e.g. `turn:abc123`)."*

## The conflation trap (this is the real, mechanical issue — dead weight, not a loss)

Memex currently mints **two** ids per item: `new_id()` (→ a uuid baked into `logical_id`) and
`new_row_id()` (→ a caller `row_id=` arg, ~25 write sites). Under governed 0.8.x:

- `new_id()` — symbol removed → replace with a **local** uuid generator (`uuid4()`); keep building
  `logical_id = "turn:<uuid>"`. Recall by `read.get("turn:<uuid>")` is unchanged. ✅
- `new_row_id()` / `row_id=` — **no analog and unused as a key.** The engine assigns the row id
  itself (`write_cursor`, returned in `WriteReceipt.row_cursors`). Since nothing recalls by `row_id`,
  these args are simply **dropped**. ✅ (No recall breaks.)

## The non-conflated id/cursor map (authoritative — carry into the design verbatim)

| Id / cursor | Who sets it | What it is | Recall key? |
|---|---|---|---|
| **`logical_id`** (string, e.g. `turn:<uuid>`) | **caller** | stable identity; supersession key; the `read.get` recall key | **YES — for nodes (active version)** |
| **`record_key`** (string, op-store) | **caller** | latest-state key within an operational collection | **YES — for op-store via `read.collection`** |
| **`write_cursor`** / `row_cursors[]` (u64) | **server** | write order + supersession watermark + receipt row identity; **not stable across re-ingest** | **NO** (replaces the dropped caller `row_id`) |
| **`source_id`** (string, nullable) | caller | provenance / excise key (graph-arm) | **NO** |
| **`stable_id`** (Cause-A: `l:<logical_id>` ｜ `h:<sha256(body)>`) | derived | cross-session **search-hit** key for gold/telemetry; lives on `SearchHit`, not a storage input | **NO** (not a `read.get` input) |

## Action (folds into tasklist T2.10, promoted to a hard gate)

Promote **T2.10** from "logical_id audit" to an **"identity & recall audit"** that proves, before
the Phase-2 swap lands:

1. every point-recallable item carries a **caller-assigned `logical_id`** (nodes) or **`record_key`**
   (op-store) at write time, and is recalled via `read.get`/`read.get_many`/`read.collection`;
2. all caller `row_id=` args are removed (server assigns `write_cursor`; read it from the receipt if
   the post-write row identity is needed);
3. edge endpoints (`from`/`to`) resolve to **`logical_id`** (the engine matches active nodes by
   logical_id; `dangling_edge_endpoints` counts misses) — not to any `row_id`;
4. **only if** the audit finds a real site that recalls by a server-minted id with no caller key
   available → escalate a FathomDB restoration (a `read.get`-by-`write_cursor`), gated like A-3.
   **Current evidence says there are none** → no FathomDB change expected.

**Bottom line for HITL:** specific-item recall is intact (`read.get(logical_id)`); Memex already keys
on `logical_id`; the refit drops a meaningless caller `row_id` and swaps `new_id()` for a local uuid.
No FathomDB-side restoration needed unless the identity audit surfaces a genuine recall-by-server-id site.
