# FathomDB-side design — what 0.8.x owes the 0.5.1 refit & the 0.8.11.2 experiments (#1.7)

> **Per-repo design (#1.7, FathomDB side).** Derives from the integrated design
> (`dev/design/0.5.1x0.8.11.2/00-integrated-design.md`, #1.5). This document states what FathomDB
> **owes and guarantees** so that (a) the Memex 0.5.1 storage-adapter refit can proceed against a
> stable governed surface, and (b) the 0.8.11.2 Phase-A experiments (V-1/V-3/V-7) have their
> FathomDB-side prerequisites in hand. The Memex-side design (#1.7, Memex side) is the mirror.
>
> **Grounding (read these for the line-level contract):**
>
> - FathomDB contract-of-record: `dev/plans/runs/B-1-fathomdb-answer-sheet.md` (file:line cites in
>   the 0.8.11.2 worktree; `fathomdb-engine` = `src/rust/crates/fathomdb-engine/src/lib.rs`,
>   `fathomdb-py` = `src/rust/crates/fathomdb-py/src/lib.rs`,
>   `fathomdb-schema` = `src/rust/crates/fathomdb-schema/src/lib.rs`).
> - Identity/recall model: `dev/design/0.5.1x0.8.11.2/identity-recall-model.md` (audit-confirmed).
> - Integrated design: `dev/design/0.5.1x0.8.11.2/00-integrated-design.md`.
>
> **One-paragraph thesis.** FathomDB's job in this refit is to be a *stable, additive* substrate.
> The governed 0.8.x surface (write-receipts + read-verbs + fixed FTS projection + embed + rerank)
> is already the contract; this document pins each shape the refit binds to, guarantees they stay
> additive/stable for the refit window, and tracks the four FathomDB-owned action items (A-1 LANDED,
> A-2 resolved, A-3 deferred, Cause-A `stable_id` merge-gated) plus the $0/local experiment-support
> deliverables that gate V-1/V-3/V-7. This is a **label-only pico** — no manifest bump, no tag, no
> publish.

## 1. The governed-contract invariants the refit depends on

The refit (~90% inside `fathom_store.py`/`fathom_facade.py`) binds to exactly these shapes. FathomDB
**guarantees each stays additive and stable for the refit window** (no breaking reshape; new fields
may be added but existing fields/semantics do not move). Each row cites the answer sheet.

| Contract | Governed shape (the guarantee) | Stability cite |
|---|---|---|
| **Write-receipt** | `engine.write(batch: list[dict]) → WriteReceipt{cursor: u64, row_cursors: list[u64] (1:1 input order), dangling_edge_endpoints: u64}`. Closed `PreparedWrite` variant set `{node, edge, op_store, admin_schema}`. Row id = server-assigned `write_cursor`; no `new_id`/`new_row_id`. | B-1 §I-5 (py 795, engine 2719/1034/1728; py 311/1280) |
| **Read-verbs (point)** | `read.get(logical_id) → Option<NodeRecord>` (active-only) and `read.get_many([logical_id]) → [Option<NodeRecord>]` (request-order, partial). Op-store recall = `read.collection(collection) → rows`, then locate by `record_key` within results (`after_id` cursor for ranges). | B-1 §I-5; identity-recall-model.md §"Does recall still exist? YES" (engine 3661, pyo3 1077; `read.collection` engine 492/3824) |
| **Read-verbs (list)** | `read.list_filter(kind, Filter{terms}) → [NodeRecord{logical_id,kind,body,write_cursor}]`. AND-only, allowlisted predicate paths, `limit` only (no cursor/`ORDER BY`/`after_id`). Anonymous rows excluded. | B-1 §I-2 (py 1230, engine 1537/1174/6779) |
| **FTS / projection (fixed)** | **No consumer FTS/projection API.** Engine-owned fixed projection: single indexed `body` column per node/edge, tokenizer `porter unicode61 remove_diacritics 2`, uniform BM25, built at store-open by immutable internal migrations. JSON subfields filter-only via `json_extract(body,'$.x')`. | B-1 §I-4 (schema 158/268/347, engine bm25 6116, py 1036/engine 9456) |
| **Embed** | `engine.embed(text) → list[float]`, default embedder `fathomdb-bge-small-en-v1.5`; raises `EmbedderNotConfiguredError` when `use_default_embedder=False`. `BuiltinEmbedder` + `VectorRegenerationConfig` removed tree-wide. | B-1 §I-6 (py 988, TS binding.ts:272) |
| **Rerank/score** | `SearchHit.ce_score: Option<f64> = sigmoid(ce_logit) ∈ [0,1]` (mirrored on `PerHitExplain.ce_score`); `Some` only for in-pool reranked hits, never participates in ranking. Knobs `alpha` (clamp [0,1], default 0.3; `1.0`/`pool_n=10` = measured-parity) and `pool_n`. | B-1 §I-3 (engine 1121/1281, py 833/1523/393) |
| **Provenance/identity** | `source_id: Option<String>` nullable column (provenance/excise; no `ProvenanceMode`). `logical_id` caller-supplied → transaction-time supersession (tombstone-then-insert); `None` = plain insert. | B-1 §I-5/§I-7; identity-recall-model.md §id/cursor map (engine 9584/9635/1114) |

**The guarantee, stated plainly.** For the duration of the refit window, FathomDB will not reshape
any of the above. Changes are **additive only** (e.g. A-1 adds an allowlist entry; Cause-A adds a
parallel `stable_id` field). The default query path stays byte-unchanged (§5). This is what lets the
Memex refit treat the boundary as a fixed target while it generates the adapter.

## 2. FathomDB-side action items + state

Four items, from B-1 §"FathomDB-side action items". State as of this design:

- **A-1 — `$.action_kind` predicate-path allowlist. LANDED (`9e0a3459`), merge-gated.**
  Adds the literal `"$.action_kind"` to `PREDICATE_PATH_ALLOWLIST` (engine 1322; the prior set was
  `{$.status, $.priority, $.tags, $.kind, $.created_at}`, engine 1321) + a guard test. Additive, no
  API change. Enforced at construct (1350) and read (3894). Gates only Slice-5's hot WMAction
  filter; Memex's client-side `action_kind` split is the fallback until merge. **Reaches Memex's
  pinned build only when 0.8.11.2 → main (§4).**

- **A-2 — bool-eq server-side. RESOLVED (no FathomDB work).**
  Bool-eq is `Predicate::JsonPathEq{ScalarValue::Bool}` executing server-side on `read.list` with a
  `json_type IN ('true','false')` guard (engine 1401; Python `bool` → `ScalarValue::Bool`, py 1160).
  The tasklist's client-side bool fallback is unnecessary once the path is allowlisted. Caveat:
  rejected on the **search** backend post-KNN by design (engine 1614); `action_kind` bool-eq still
  depends on A-1's path allowlisting.

- **A-3 — stable paging on `read.list`. DEFERRED (precise trigger below).**
  `read.list` exposes `limit` only — no `ORDER BY`, no cursor, no `after_id` (engine 6779); the
  `after_id` cursor lives only on the op-store `read.collection` surface (engine 492/3824).
  **Exact trigger to un-defer:** a single Memex session whose within-kind partition exceeds the
  ~10k `read.list` cap, such that timestamp-windowing no longer suffices and deep/stable paging is
  required (integrated design §2/§8, T2.11 paging audit). **The fix (pre-specified):** add a
  deterministic `write_cursor` order + an `after_cursor` parameter to `read.list`, mirroring the
  op-store `read.collection` cursor pattern (small, additive). Until the Memex paging audit confirms
  such a call site, A-3 stays deferred and client-side timestamp windowing is the path.

- **Cause-A `stable_id` — merge-gate + outstanding verify.**
  `SearchHit.stable_id: Option<String>` (Cause-A, engine 1132) is **distinct from** `source_id`
  (engine 1114); both coexist. `stable_id = "l:<logical_id>"` else `"h:<sha256(body)>"`
  (`derive_stable_id`, engine 9027); telemetry exposes `result_stable_ids` parallel to `result_ids`.
  Lives **only** on `0.8.11.2-pico-umbrella` (commit `d3f6b994`), not yet an ancestor of
  `origin/main`. **Outstanding verify before merge:** main-tree `maturin develop` + py/TS import
  smoke (confirm the rebuilt native module imports and the `stable_id`/`result_stable_ids` fields are
  reachable from both bindings). Memex wires the `stable_id` slot inert until Cause-A merges.

## 3. Experiment-support FathomDB owes for Phase A ($0/local)

These are **FathomDB-owned, $0/local** prerequisites. The experiments themselves are sequenced
**after** the refit (Phase A → V-1 → V-3 → V-7), but the prereqs below are FathomDB's to deliver and
they **gate** the corresponding verifications. Per B-1 §"Experiment-timeline impact", the
academic/screening arms (OPP-6 EXP-COV, OPP-3 characterization, OPP-1 build arms, V-1/V-3/V-7) run
FathomDB-side regardless of the refit; only the as-Memex/real-gold ADOPTION arms need
Memex-on-0.8.x (and those remain a HITL Adopt-GO hard-stop).

- **P0-3 — MuSiQue re-pull preserving `question_decomposition`.** Re-pull the MuSiQue corpus
  preserving the `question_decomposition` field (2,417 rows). This is the multi-hop decomposition
  signal the OPP-1 build arms and the V-* multi-hop checks read; without it the decomposition-aware
  arms cannot be scored. FathomDB deliverable; **gates V-1/V-3/V-7** multi-hop coverage.

- **P0-4 — eval-support primitives.** Three measurement/knob deliverables on the FathomDB side:
  1. **Expose `margin` as a measurement** — surface the score margin (top-hit vs. next) as an
     observable so the experiments can read separation, not just rank.
  2. **Distractor-injection / gold-rank-demotion knobs** — controlled levers to inject distractors
     and demote gold rank, so the V-* arms can probe robustness under degraded retrieval.
  3. **Per-corpus `decide_08x`** — a per-corpus decision hook so each corpus carries its own
     screening verdict rather than a single global gate.
  All three are FathomDB-side, $0/local, and **gate V-1/V-3/V-7** (the experiments cannot reach a
  decision without `margin`/the knobs/`decide_08x` in place).

**Sequencing note.** These prereqs are owned and delivered before the experiments run, but the
run-order itself is preserved: Phase A (refit substrate ready) → V-1 (keystone) → V-3 → V-7.

## 4. Merge sequencing

Memex's build is pinned to `origin/main` (`ba80866d`). The two not-yet-merged deliverables —
**A-1** (`9e0a3459`) and **Cause-A `stable_id`** (`d3f6b994`) — live on `0.8.11.2-pico-umbrella` and
reach Memex's pinned build **only when 0.8.11.2 merges to main**. Until then:

- Memex uses **client-side fallbacks** (the `action_kind` split for A-1) and **inert slots**
  (`source_id` wired inert now; `stable_id` consumed only post-merge).
- `ce_score`, `source_id`, `read.*`, and `embed` are already on `ba80866d` and **consumable now**.
- When 0.8.11.2 merges, it delivers A-1 (`$.action_kind` allowlisted server-side) + `stable_id`
  (parallel `result_stable_ids` telemetry) to Memex's pinned build in one step.

**Label-only pico.** 0.8.11.2 is a label-only pico under the two-tier policy: **no manifest bump,
no tag, no publish.** The merge moves the additive deliverables onto main; it does not cut a release.
Cause-A must be kept merge-ready (the §2 outstanding verify: main-tree `maturin develop` + py/TS
import smoke).

## 5. Behavioral invariants

FathomDB guarantees the following behavioral invariants across this work — they are what make the
substrate safe for the refit and keep the default product path unchanged:

- **Additive-only.** Every FathomDB-side change in this window is additive (A-1 = one allowlist
  entry; Cause-A = a new parallel field; A-3, if un-deferred, = a new optional parameter). No
  existing field, verb, or semantic is reshaped or removed for the refit's sake.
- **Default query path byte-unchanged.** The default search/query path is untouched: `ce_score`
  never participates in ranking (engine 1121), `alpha` defaults 0.3, and Cause-A added `stable_id`
  **without** reshaping ranking or the default result order. Flag-off == pre-refit behavior.
- **F-8a telemetry contract preserved.** Cause-A added a **parallel** `result_stable_ids` field
  alongside the existing `result_ids` — it did **not** reshape or replace `result_ids` (B-1 §I-7,
  engine 1132/9027). Consumers reading `result_ids` see the same shape they did before; the
  `stable_id` channel is purely additive telemetry.

## 6. Open items carried into this design

- **R-I4-parity (Memex-side, FathomDB-fixed).** FathomDB's FTS is fixed single-`body`/uniform-BM25/
  fixed-tokenizer with no consumer API (§1, B-1 §I-4). FathomDB will **not** add a multi-field /
  weighted / custom-tokenizer FTS surface for this refit; the parity risk is resolved Memex-side by
  content-modeling searchable text into `body`. This is a stated FathomDB **non-deliverable** (the
  boundary is fixed by design), recorded so the refit does not wait on a FathomDB FTS change.
- **A-3 un-defer trigger** (§2) — revisited only on the Memex paging audit (T2.11).
- **Cause-A verify → merge** (§2) — main-tree `maturin develop` + py/TS import smoke before merge.

## 7. HITL decisions carried (2026-06-30, from B-1)

- **Q-B1** = no DB migration (consistent with §1 fixed FTS: nothing to forward-migrate).
- **Q-B2** = keep the `0.5.1` label for the refit; **0.8.11.2 stays a label-only pico** FathomDB-side.
- **Q-B3** = greenlight Slice-15-core after the codex pass.
