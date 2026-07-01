# ADR-0.8.12 — Consolidation / Recency Provider (OPP-2)

> **Status:** PROPOSED (Slice-0 design gate, 0.8.12). Implemented at Slice 15; value-gated at Slice 20.
> **Builds on:** `ADR-0.8.6-generalized-provider-protocol.md` (OPP-8) — the ONE generalized transport.
> **Supersedes:** nothing. **Does NOT** introduce a new sibling transport (that is the entire point).
> **Footprint invariant:** FathomDB makes NO network call; the consolidation LLM lives entirely in the
> caller's harness (CALLER-SIDE BYO-LLM). The library write/index/query path stays CPU-only/deterministic.
> Default-OFF until the Slice-20 lossiness-vs-latency value test clears.

---

## 1. Context

Mem0-parity has an **update / temporal axis** FathomDB does not yet serve: when a new fact
merges/supersedes an earlier one (a role change, a corrected value, a status flip), the store should
be able to reconcile the two rather than accumulate contradictory rows. OPP-2 ("consolidation/recency
provider", AGREED) is that capability. ADR-0.8.6 §1 already named it as the **second consumer** of the
generalized provider seam and reserved its shape:

> "0.8.12's consolidation provider adds a **payload + error leaf** (and a `fathomdb.consolidate.v1`
> task string), **not a transport**. I-5 rework edge closed." (ADR-0.8.6 §5)

The hard rework edge **I-5 (#8 → #7)** is: if we build consolidation on a *second* bespoke transport we
pay twice for spawn/handshake/framing/error-model/binding-exposure and a later forced merge. ADR-0.8.6
landed the transport seam (`provider_session(task, …)`) precisely so this ADR only has to add a
**payload + error leaf + a named engine method**, never a transport.

## 2. Decision

Add consolidation as a **new typed task on the existing OPP-8 transport**, mirroring how `extract`
rides it today. Concretely, the four (and only four) additive pieces:

1. **Task string / wire:** `fathomdb.consolidate.v1` (per-task family naming, ADR-0.8.6 §2.2). Same
   NDJSON-over-stdio transport, same `hello`/`ready` handshake, same `request_id` framing,
   `schema_version = 1`. A harness advertises `consolidate` in `ready.supported_tasks`; FathomDB
   refuses to dispatch it if unadvertised (ADR-0.8.6 §2.2 negotiation, reused verbatim).
2. **`ProviderTask::Consolidate`** variant on the existing enum
   (`fathomdb-engine/src/lib.rs` `ProviderTask`, alongside `Extract`), so `provider_session` formats
   `fathomdb.{task}.v1` for it with zero transport change.
3. **A task-specific payload** (namespaced; extraction's `{documents:[…]}` is untouched):
   - **Request** (FathomDB → harness): a **candidate cluster** of competing facts for one
     (subject-entity, relation) axis — the existing edges with their `body`, `t_valid`/`t_invalid`,
     `confidence`, `source_doc_id`, and `extractor_model_id` provenance. FathomDB assembles the cluster
     deterministically (CPU-only) and asks the harness to judge recency/supersession over it.
   - **Response** (harness → FathomDB): a **consolidation verdict** — for each input edge, one of
     `{keep, supersede(by: edge_ref), merge(into: edge_ref), invalidate(t_invalid: ts)}`, plus an
     optional short rationale `body`. The harness NEVER deletes; it emits a verdict and FathomDB applies
     it (mirrors the ELPS `supersedes_prior` discipline: the caller flags, the engine records).
4. **Error leaf:** `EngineError::Consolidator` / `FDB_CONSOLIDATOR` (the per-task catch-all leaf
   ADR-0.8.6 §2.1 reserved — "New tasks get their own leaf … at the slice that introduces them").

### 2.1 Post-retrieval weight, NOT a pre-retrieval content rewrite

Per ADR-0.8.6 §1, consolidation is "a post-retrieval rerank weight (not a pre-retrieval content
rewrite)." The 0.8.3 lesson (`0.8.3-mem0-parity-closed`) — **blind content-merge HURT accuracy** — is
load-bearing here: consolidation must not destructively rewrite/merge fact bodies at ingest. The
verdict is recorded as **supersession/recency metadata** (`t_invalid`, a `superseded_by` link,
provenance), so retrieval can down-weight or hide superseded facts, and the original rows survive for
audit and for the value-test's lossiness measurement. This keeps the engine deterministic and lets
Slice 20 measure the accuracy delta of *applying* the verdict vs not.

### 2.2 Default-OFF until value-gated

The provider is **opt-in and default-OFF**. Slice 20's pre-registered lossiness-vs-latency value test
(design: `dev/design/0.8.12-coverage-probe-and-value-test.md` §B) decides whether it ships ON by
default. A failing test ⇒ the seam stays built but opt-off, and the negative is recorded (R-CON-2). The
seam is still shippable (the capability exists for callers who want it) even if not default-on.

## 3. Footprint honesty (R-CON-3)

- The consolidation LLM is **caller-side only** — the harness is a caller-supplied subprocess; FathomDB
  emits no network symbol reachable from the library query path (the same no-egress guard as
  `slice15_byo_llm_ingest.rs`, extended to the consolidate task).
- Cluster assembly + verdict application are **CPU-only, deterministic**.
- Every technique tagged CALLER-SIDE BYO-LLM / OFFLINE-BUILD in the code and docs.

## 4. Acceptance (Slice 15 DoD)

- **R-CON-1:** functional harness — ingest conflicting/updated facts on a (subject, relation) axis →
  consolidated result with correct supersession + temporal bounds (a stub harness like
  `tests/fixtures/slice15_byo_llm/stub_harness.py`, deterministic).
- **R-CON-3:** no-egress guard green for the consolidate task; no second transport (codex §9 confirms
  `provider_session` is reused, exactly as ADR-0.8.6 §2.3 requires).
- **Back-compat:** an extract-only harness (no `consolidate` in `supported_tasks`) is unaffected; the
  ELPS golden suite (`elps_conformance_golden.rs`, 8 cases) passes untouched.
- **X1:** if any binding-visible surface changes, it lands in **both** Py + TS with a live functional
  harness (R-X-1).

## 5. Consequences

- I-5 rework edge stays closed: one transport, two tasks (`extract`, `consolidate`), one error model
  family, one binding-exposure path.
- Community-summarize (OPP-4), if it survives its gate, rides the same seam for free (ADR-0.8.6 §5).
- The value test (Slice 20) is the gate for default-on; a negative is a legitimate, recorded outcome
  (build ≠ adopt).

## 6. Alternatives rejected

- **A second bespoke transport for consolidation.** Rejected — this is exactly the I-5 rework the
  OPP-8 seam was built to prevent (ADR-0.8.6 §1).
- **Destructive content-merge at ingest.** Rejected — 0.8.3 measured it as accuracy-negative
  (`0.8.3-mem0-parity-closed`); consolidation is recorded as recency/supersession metadata, not a
  rewrite (§2.1).
- **In-process callback instead of subprocess.** Rejected/deferred — ADR-0.8.6 §2.1 keeps the
  subprocess transport for all tasks (no in-process callback yet); consolidation inherits that.

## 7. Sources

- `ADR-0.8.6-generalized-provider-protocol.md` §1, §2.1–2.3, §5 (the OPP-8 seam + reserved shape).
- `ADR-0.8.1-byo-llm-extraction-protocol.md` (the `fathomdb.extract.v1` precedent).
- Engine: `fathomdb-engine/src/lib.rs` `ProviderTask` / `provider_session` (~2843, ~9083).
- `dev/plans/plan-0.8.12.md` §1 (#7 rationale); memory `0.8.3-mem0-parity-closed` (blind-merge caution).
