# ADR-0.8.6 — Generalized Typed-Task Provider Protocol (OPP-8)

> **Status:** ACCEPTED — HITL-SIGNED 2026-06-26 (Slice-0 gate; Option A "land the seam now" chosen).
> **Supersedes (generalizes):** `ADR-0.8.1-byo-llm-extraction-protocol.md` (`fathomdb.extract.v1`).
> **Implements at:** 0.8.6 Slice 5 (engine-side protocol + ELPS re-expression).
> **Gates:** 0.8.10 #6/#7 (consolidation OPP-2, community-summarize OPP-4 ride this contract).
> **Footprint invariant:** FathomDB makes NO network call; all LLM access lives in the caller's harness
> (CALLER-SIDE BYO-LLM). The library query path stays CPU-only/deterministic.

---

## 1. Context

FathomDB needs three caller-supplied LLM capabilities across the 0.8.x program:

- **ELPS extract (OPP-1, BUILT)** — documents → entities + fact-edges. Today this rides
  `fathomdb.extract.v1`: a subprocess the engine spawns, NDJSON over stdin/stdout, a `hello`/`ready`
  handshake then `extract`/`result|error` request-response. Spec: `ADR-0.8.1`. Engine surface:
  `Engine::ingest_with_extractor` (`fathomdb-engine/src/lib.rs:2514`); golden conformance:
  `tests/elps_conformance_golden.rs` (8 cases d1–d8). One error variant: `EngineError::Extractor`.
- **Consolidation (OPP-2, UNBUILT — 0.8.10)** — judge which of several competing facts is current
  (recency / supersession), as a post-retrieval rerank weight (not a pre-retrieval content rewrite).
- **Community-summarize (OPP-4, UNBUILT — conditional, 0.8.x)** — cluster-level summaries over
  entities/edges (GraphRAG-style sensemaking).

The 0.8.6→0.8.16 sequencing record marks a **hard rework edge I-5 (#8 → #7/#6)**: if 0.8.10 builds the
consolidation provider on a *second* bespoke transport, we pay for two transports, two error models, two
binding-exposure paths, and a later forced merge. The ledger therefore requires OPP-2 be built "as ONE
generalized provider per OPP-8." This ADR defines that one contract **before** the second consumer
lands, so 0.8.10 layers a payload schema onto an existing transport rather than inventing a sibling.

## 2. Decision

Promote `fathomdb.extract.v1` to a **typed-task** protocol `fathomdb.provider.v1` that keeps the
transport, handshake, framing, error model, and provenance **byte-identical** for extraction, and adds a
single discriminator — the **task type** — so other typed tasks ride the same envelope.

### 2.1 What stays identical (the invariant transport)

- **Transport:** caller-supplied subprocess; NDJSON over stdin (FathomDB→harness) / stdout
  (harness→FathomDB); stderr diagnostics only. No in-process callback (deferred, as today).
- **Handshake:** `hello` (FathomDB advertises `protocol`, `schema_version`) → `ready` (harness advertises
  `model`, capacity). Validation aborts on protocol/version mismatch (unchanged from `lib.rs:2598`).
- **Framing:** `request_id` request-response matching; one `result` **or** one `error` per request.
- **Error model:** one catch-all per task family mapped to a typed `EngineError`/`FDB_*` leaf
  (extract → `EngineError::Extractor` / `FDB_EXTRACTOR`, **unchanged**). New tasks get their own leaf
  (`EngineError::Consolidator` etc.) at the slice that introduces them — **not** in 0.8.6.
- **Provenance:** `ready.model` recorded on output rows (`extractor_model_id` and successors).
- **Timeout:** `FATHOMDB_*_TIMEOUT_MS` env override; bounded channel recv (unchanged).

### 2.2 What generalizes (the typed-task seam)

- The `hello`/`ready` envelope carries `task: "extract"` (and, later, `"consolidate"`, `"summarize"`).
  A harness advertises in `ready` **which tasks it serves** (`supported_tasks: ["extract", …]`); FathomDB
  refuses to dispatch a task the harness did not advertise.
- The request/response **payload** is task-specific and namespaced: extraction keeps exactly today's
  `{documents:[…]}` → `{entities:[…], edges:[…], warnings:[…]}`. Consolidation/summarize define their own
  payloads **at their own slices** (0.8.10), not here.
- **Protocol naming = per-task family `fathomdb.<task>.v1`.** The extract task's wire string stays
  **`fathomdb.extract.v1`, byte-identical** — this is deliberate and supersedes an earlier draft idea of a
  flat `fathomdb.provider.v1` rename, because byte-identical extract wire (no break for existing Memex ELPS
  harnesses, no churn to the golden stub) is a release requirement that a cosmetic rename would violate.
  The "generalization" is the **internal parameterization**, not a new wire string for extract: a private
  `provider_session(task, …)` formats `fathomdb.{task}.v1` (extract → the unchanged string; 0.8.10
  consolidation → `fathomdb.consolidate.v1`). schema_version stays `1`.
- `ready` MAY include `supported_tasks: [...]`; if present, FathomDB refuses to dispatch a task the
  harness did not advertise. If **absent**, it defaults to the single task being requested (extract) —
  so **existing extract-only harnesses keep working unchanged** (back-compat, pinned by a RED test).

### 2.3 Engine surface

`ingest_with_extractor` stays as the **extraction entry point** (no consumer-visible break). Internally it
calls a new private `provider_session(task, …)` that owns spawn + handshake + framing, parameterized by
task. New tasks get new **named** engine methods (e.g. a future `consolidate_with_provider`) that reuse
`provider_session` — they are added at 0.8.10, not 0.8.6. No second transport ever exists.

## 3. Acceptance (Slice 5 DoD)

- **R-PP-1:** a single session/transport implementation; ELPS re-expressed on it with **byte-identical**
  output — `elps_conformance_golden.rs` (8 cases) and `slice15_byo_llm_ingest.rs` pass unchanged; codex §9
  confirms no second transport/handshake remains.
- **R-PP-2:** BYO-LLM, caller-side only — no LLM/network symbol reachable from the library query path;
  the footprint test (`slice15_byo_llm_ingest.rs` no-egress guard) stays green.
- **Back-compat:** a `ready` **without** `supported_tasks`/`task` is treated as `task="extract"`; an
  existing extract-only harness passes the golden suite untouched (a RED test pins this).
- **X1:** if any binding-visible surface changes, it lands in **both** Py + TS with a live functional
  harness. (Expectation: the extract entry point is unchanged, so X1 is a no-op confirmation here.)

## 4. Risk — YAGNI / premature generalization (HITL decision)

The second consumer (consolidation) is **not yet designed**, so the typed-task seam is constrained only
by extraction + our *guess* at consolidation/summarize payloads. Over-generalizing a protocol against one
real consumer is a classic way to build the wrong abstraction.

**Mitigation chosen:** generalize **only the transport seam** (the expensive, rework-prone part the
ledger's I-5 edge is about), and **defer every task-specific payload + error leaf to the slice that
introduces its consumer.** 0.8.6 ships: the renamed protocol, the `task` discriminator with an
extract-only default, `supported_tasks` negotiation, and the `provider_session` refactor — all proven by
the **unchanged** ELPS golden output. We do **not** invent consolidation/summarize payloads now.

This is the smallest change that removes the I-5 rework risk without speculating on the second consumer's
schema. **Two options for the HITL:**

- **(A) Land the seam now (recommended).** Build §2.1–2.3 as above. Cost: one Rust refactor + a back-compat
  RED test, all gated by byte-identical golden output. Removes I-5. Recommended because the refactor is
  cheap, fully test-pinned, and 0.8.10 is close.
- **(B) Defer to 0.8.10.** Keep `fathomdb.extract.v1`; build the generalization together with the first
  consolidation payload. Cost: 0.8.10 carries both the generalization *and* a new consumer in one slice
  (more risk concentrated), and the rename churns a shipped protocol string later instead of now. The
  release theme ("every micro-release genuinely DoD-shippable; #8 first so #6/#7 don't force a throwaway")
  argues against this.

## 5. Consequences

- 0.8.10's consolidation provider adds a **payload + error leaf** (and a `fathomdb.consolidate.v1` task
  string), not a transport. I-5 rework edge closed.
- Extract's wire is **unchanged** (per-task naming) — no rename churn, no harness break; the seam is the
  internal `provider_session` parameterization plus the optional `supported_tasks` negotiation.
- Community-summarize (OPP-4), if it survives its gate, rides the same seam for free.

## 6. Sources

- `ADR-0.8.1-byo-llm-extraction-protocol.md` (the `fathomdb.extract.v1` it generalizes).
- `dev/plans/plan-0.8.6.md` §1 (#8 rationale), `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (I-5 edge).
- Engine: `fathomdb-engine/src/lib.rs:2514` (`ingest_with_extractor`); golden:
  `tests/elps_conformance_golden.rs`; footprint: `tests/slice15_byo_llm_ingest.rs`.
