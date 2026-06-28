# Prompt for Memex — build a BYO-LLM fact-extraction harness for FathomDB

> **Audience:** this is a prompt to hand to **Memex** (the agent/project). Memex writes the
> harness; FathomDB proxies extraction to it. FathomDB owns the *protocol + extraction
> prompt + output schema*; Memex's harness owns *getting that to its LLM and back*.
> **Source of the design:** `dev/roadmap/0.8.1.md` §5.4 (R3, BYO-LLM construction) +
> `dev/plans/runs/IR-C-roadmap.md` (C7).
>
> **◆ v1 RATIFIED with Memex 2026-06-12** (decision record:
> `~/projects/memex/dev/elps/FATHOMDB-CONSULT.md`). Five additive pins folded in below (no
> `schema_version` bump): `options.instructions` (Q1) · `source_span` = UTF-8 byte half-open
> `[start,end)` (Q4) · **replay-determinism** (Q3) · `warnings.kind` enum (D5) · per-document
> timeout (Q7). Memex's ELPS implements this; the golden fixture freezes against these pins.
>
> **◆ Round 2 ratified 2026-06-13** (decision record: `~/projects/memex/dev/elps/FATHOMDB-CONSULT-2.md`).
> Three more additive fields (still no `schema_version` bump): `entities[].synthesized` (QA — boolean,
> marks an endpoint FathomDB-synthesized because the model under-listed it; resolve as low-confidence
> identity) · `warnings.kind:"temporal_fallback"` (QB — `t_valid` defaulted to `created_at`; carries
> `substituted_t_valid` + `raw_t_valid`) · `warnings.kind:"capped"` (DI-3 — `max_facts_per_doc`
> truncation; carries `kept`+`dropped`). Post-round-2 `warnings.kind` enum = {`supersedes`,
> `doc_dropped`, `no_facts`, `validation_failed`, `temporal_fallback`, `capped`}. DI-1/DI-2/DI-4 +
> QA(A)/QC accepted; QD sample requested. These fold into the **Slice-0 BYO-LLM ADR** (HITL signs;
> shapes won't reopen).

## Background (why this exists)

FathomDB is a local-first, **CPU-only, no-API, 1-bit-quantized** agent-memory store. We are
adding a **temporal fact-edge graph** as a third retrieval arm (Zep/Graphiti-shaped): facts
become edges carrying `body` text, `t_valid`/`t_invalid` (event valid-time), and `confidence`,
between entity nodes. Retrieval-time graph math (FTS over edge text + BFS over
`canonical_edges(from_id)/(to_id)` + RRF fusion + the `t_invalid IS NULL OR t_invalid > now`
filter) is fully in-footprint — **no LLM at query time.**

The one part that needs an LLM is **construction**: turning raw documents (emails, transcripts,
notes) into entities + fact-edges. We will **not** bake an LLM into FathomDB (it would break the
no-API/CPU footprint). Instead, FathomDB proxies extraction to a **caller-supplied provider** —
because FathomDB's consumers (Memex, Hermes, OpenClaw) are already LLM agents with a model in the
loop at ingest time. **You (Memex) build that provider: a local adapter that wraps your existing
LLM and speaks FathomDB's Extraction Provider Protocol v1.**

## What to build

A standalone **extraction provider** ("the harness") that:

1. FathomDB launches/connects to locally (no network egress *from FathomDB*; the harness alone
   owns LLM connectivity — local model, your own API key, whatever Memex already uses).
2. Receives FathomDB-authored extraction *requests* and returns schema-valid *fact-edge
   responses*.
3. Is the **only** component that talks to an LLM. FathomDB stays storage+retrieval only.

Think of it as the Mem0/Zep model where the host app supplies the LLM — but formalized as a small,
versioned, local protocol so any consumer (not just Memex) can implement it.

## Transport (pick the primary; the protocol is transport-agnostic)

**Primary (implement this): subprocess + NDJSON over stdio.** FathomDB spawns the harness as a
child process; sends one JSON request per line on **stdin**; reads one JSON response per line on
**stdout**; logs/diagnostics go to **stderr only** (never stdout). Language-agnostic, no network,
no ports. The harness is a single executable Memex ships.

Also acceptable (document if you choose one instead): a Unix-domain-socket daemon (same JSON
framing), or an in-process callback registered through FathomDB's Python/TS binding (a function
`(request_json) -> response_json`). Do **not** use a TCP/HTTP network endpoint as the primary —
it violates the local-first contract.

## Protocol v1 — messages

All messages are single-line UTF-8 JSON. Every message has `protocol: "fathomdb.extract.v1"` and a
`type`. Unknown fields are ignored (forward-compatible); unknown `type` → an `error` response.

### 1. Handshake (FathomDB → harness on startup; harness replies once)

Request `{"protocol":"fathomdb.extract.v1","type":"hello"}` →
Response:

```json
{"protocol":"fathomdb.extract.v1","type":"ready",
 "provider":"memex","model":"<id>","supports":{"deterministic":true,"batch":true,"max_docs_per_request":32},
 "schema_version":1}
```

FathomDB aborts if `protocol`/`schema_version` mismatch.

### 2. Extraction request (FathomDB → harness)

```json
{"protocol":"fathomdb.extract.v1","type":"extract","request_id":"<uuid>",
 "documents":[{"doc_id":"<id>","kind":"email|meeting|note|todo|...","body":"<text>","created_at":"<ISO-8601>"}],
 "ontology":{"entity_types":["person","org","project","artifact","event","topic"],
             "relation_hint":"freeform short relation labels; you choose"},
 "options":{"deterministic":true,"max_facts_per_doc":24,"language":"en",
            "instructions":"<optional: authoritative extra extraction guidance from FathomDB>"}}
```

FathomDB owns this shape and the extraction semantics; you fulfill it. **`options.instructions`
(optional, ratified 2026-06-12 / Q1):** absent ⇒ the provider authors the prompt freely; when
present ⇒ the provider **MUST incorporate it as binding guidance** (MAY adapt phrasing to its model)
**and MUST include it in the determinism cache key**. The provider owns the concrete prompt; FathomDB
owns the output schema + semantics + this optional steer.

### 3. Extraction response (harness → FathomDB)

```json
{"protocol":"fathomdb.extract.v1","type":"result","request_id":"<uuid>",
 "entities":[{"name":"Alice Chen","type":"person","aliases":["Alice"]}],
 "edges":[{
    "from_entity":"Alice Chen","to_entity":"Q3 launch","relation":"rescheduled",
    "body":"Alice said the Q3 launch slipped to November.",
    "t_valid":"2026-06-01T00:00:00Z","t_invalid":null,
    "confidence":0.82,
    "source_doc_id":"<id>","source_span":[120,168]
 }],
 "warnings":[]}
```

- **`entities`** → FathomDB nodes, deduped to a stable `logical_id` (you provide `name`+`type`+`aliases`;
  FathomDB owns the id assignment). `from_entity`/`to_entity` reference entities by `name`. **Dangling
  endpoints (ratified 2026-06-13 / QA):** if an edge references a `name` the model didn't list, the
  provider **synthesizes** a minimal entity `{"name":..,"type":"unknown","aliases":[],"synthesized":true}`
  rather than dropping the edge (preserve the fact). `synthesized:true` (additive; absent/false ⇒
  model-listed) lets FathomDB treat it as a low-confidence identity. Do **not** overload `type` for this.
- **`edges`** → rows in `canonical_edges` with the enrichment columns `body`, `t_valid`, `t_invalid`,
  `confidence` (the G11 additive set). `relation` is your short label (becomes the edge `kind`/relation).
- **Temporal semantics (load-bearing):** `t_valid` = when the fact *became true* (event time, not
  ingestion time); `t_invalid` = when it stopped being true, or `null` if still valid. If a new fact
  contradicts/supersedes an older one, **emit the new fact with its `t_valid`** and, when you can
  identify the prior fact, surface it in `warnings` — FathomDB does the invalidate-not-accumulate
  bookkeeping; you only need to date facts correctly.
- **`warnings` (typed; `kind` enum ratified 2026-06-12 / D5, extended 2026-06-13).** Each warning is
  `{"kind": "...", ...}`; kinds: **`supersedes`** (carries `supersedes_hint` + `prior_body`),
  **`doc_dropped`** (carries `source_doc_id` + `detail`), **`no_facts`** (carries `source_doc_id`),
  **`validation_failed`** (carries `source_doc_id`), **`temporal_fallback`** (QB — `t_valid` defaulted
  to `created_at`; carries `source_doc_id` + `substituted_t_valid` + `raw_t_valid` [null ⇒ omitted,
  string ⇒ unparseable]), **`capped`** (DI-3 — `max_facts_per_doc` truncation; carries `source_doc_id`
  - `kept` + `dropped`). Any document dropped from a `result` (no facts, or a per-doc validation
  failure while infra is healthy — D4) **MUST** emit a warning carrying its `source_doc_id`. Extra
  fields are additive; FathomDB ignores unknown `kind`s and keeps the `result`.
- **`confidence`** ∈ [0,1]: your calibrated extraction confidence; FathomDB may threshold/weight on it.
- **`source_doc_id`** required; **`source_span`** optional but preferred (citation-grade provenance).
  **Unit (ratified 2026-06-12 / Q4): UTF-8 byte offsets, half-open `[start_byte, end_byte)`**,
  0-based, end-exclusive, into the `body` field **as transmitted** (do not re-encode before
  measuring). Omit (null) rather than emit a guessed span. (FathomDB's store is Rust/byte-indexed.)

### 4. Error (harness → FathomDB, instead of a result)

```json
{"protocol":"fathomdb.extract.v1","type":"error","request_id":"<uuid>",
 "code":"llm_unavailable|invalid_request|extraction_failed|timeout","message":"<human text>","retriable":true}
```

A crash/blank line/non-JSON on stdout is treated as a fatal provider error — never do that; always
emit a `result` or `error` for every `request_id`.

## Hard requirements (these are the contract; conformance depends on them)

1. **FathomDB never makes a network call.** All LLM connectivity lives in the harness.
2. **Strict, schema-valid JSON only on stdout** — one object per line, `result` or `error` per
   `request_id`, no prose, no partial lines, no stdout logging.
3. **Determinism mode — "replay determinism" (REVISED 2026-06-12 / Q3).** Real LLM stacks are not
   token-deterministic, so v1 determinism is a **cache property**: under `options.deterministic=true`
   the provider MUST return **byte-identical bytes for any request whose canonical form is in its
   cache, including the shipped/seeded golden-fixture cache** (canonicalize → content-address → replay;
   greedy/temp-0 for *new* generations). It does **NOT** guarantee byte-identical *generation* across
   cold/novel environments. Cache key = `sha256(canonical(documents, ontology, options /*incl.
   instructions*/, ready.model, PROMPT_VERSION))`. **Conformance is asserted only over the shipped
   fixture cache.** FathomDB pins the *extracted-graph artifact* for eval reproducibility, not live
   re-extraction. Document the cold-cache cross-environment limit in `EXTRACTION_PROTOCOL.md`.
4. **Idempotency by `request_id`** — re-sending the same `request_id` may return the cached prior
   result.
5. **Batching + back-pressure.** Honor `max_docs_per_request` from your `ready` message; FathomDB
   will not exceed it. Process serially is fine; just don't drop or reorder responses without their
   `request_id`.
6. **Timeouts/failure are first-class** — emit a `timeout`/`extraction_failed` `error`, don't hang.
   **Timeout is PER-DOCUMENT (ratified 2026-06-12 / Q7):** `ELPS_TIMEOUT_S` (default 60 s) bounds each
   per-doc LLM call, not the whole `extract` (a 32-doc batch must not spuriously time out). A per-doc
   timeout is transient → **fails the whole `extract`** (D4) and the `timeout` error names the
   offending `source_doc_id`.
7. **Versioned** — `schema_version:1`; bump on any breaking change; keep v1 working.
8. **No FathomDB-side ontology lock-in** — entity/relation vocab is open (FathomDB's `source_type`
   vocab is closed, but graph relations are freeform); just be internally consistent within a run.

## Deliverables

1. **The harness** — a runnable executable (any language Memex uses) implementing the stdio NDJSON
   protocol above, backed by Memex's LLM. Include a `--selftest` that round-trips the golden fixture.
2. **A conformance test + golden fixture** — a small set of input documents and the expected
   `result` JSON (under `deterministic=true`), so FathomDB can pin the contract and catch drift.
   Cover: a simple fact, a temporal/superseding fact (emit `t_valid` + a `supersedes_hint` warning),
   a multi-entity sentence, and a "no extractable facts" doc (empty `edges`, valid response). **Also
   cover the round-2 additions** (so FathomDB pins them): a synthesized dangling endpoint
   (`synthesized:true`), a `temporal_fallback` warning, a `capped` warning, and a **multibyte body** to
   pin the UTF-8 byte `source_span`.
3. **Protocol doc** — `EXTRACTION_PROTOCOL.md` restating v1 (messages, fields, semantics, errors)
   so a non-Memex consumer can implement the same provider.
4. **README** — how FathomDB launches it (the subprocess command + any env the harness needs for
   its LLM), determinism notes, and the footprint statement ("FathomDB makes no network calls; the
   harness owns all model access").

## Acceptance

- FathomDB can spawn the harness, complete the `hello`→`ready` handshake, send a batch `extract`,
  and get schema-valid `result`s for every `request_id`.
- The golden fixture reproduces byte-identically under `deterministic=true`.
- stdout carries only protocol JSON; stderr carries logs; no network egress observed from FathomDB.
- Temporal facts carry correct `t_valid`/`t_invalid`; superseding facts surface a `supersedes_hint`.

## Out of scope (FathomDB's side, not yours)

Entity→`logical_id` resolution, the `canonical_edges` migration, BFS/RRF retrieval, the
invalidate-not-accumulate edge bookkeeping, and the embedding/1-bit-quant of edge text. You produce
entities + dated fact-edges; FathomDB stores, links, and retrieves them.
