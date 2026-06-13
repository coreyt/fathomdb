# ADR-0.8.1 — BYO-LLM Extraction Provider Protocol (`fathomdb.extract.v1`)

> **Status:** ACCEPTED — HITL-SIGNED 2026-06-13.
> **Ratified operationally:** 2026-06-12 (Memex ELPS consult + FathomDB decision record).
> **Implements at:** Slice 15 (engine-side spawn + ingest surface).
> **Footprint invariant:** FathomDB makes NO network call; all LLM access lives in the caller's harness.

---

## 1. Context

FathomDB is a local-first, **CPU-only, no-API, 1-bit-quantized** agent-memory store. The 0.8.1
roadmap adds a temporal fact-edge graph as a third retrieval arm (R3). Graph **construction** —
turning raw documents into entities and fact-edges — requires an LLM. Baking an LLM into FathomDB
would break the no-API/CPU footprint and the local-first contract.

FathomDB's named consumers (Memex, Hermes, OpenClaw) are themselves **LLM agents** — they already
have a model in the loop at ingest time. The correct design is therefore to let the consumer supply
the extraction capability as a **caller-provided harness** that FathomDB spawns and communicates
with over a versioned local protocol. This mirrors how Mem0/Zep operate as libraries: the host
app supplies the LLM, the library supplies the storage+retrieval substrate.

The `fathomdb.extract.v1` Extraction Provider Protocol formalizes this boundary. The full harness
specification (transport, request/response schema, error envelope, and the five additive pins
from the 2026-06-12 Memex ELPS consult) is in:

- **`dev/plans/prompts/IR-C-byo-llm-extraction-harness-memex.md`** — the v1 brief (primary
  contract spec; Memex's ELPS harness implements the harness side)
- **`~/projects/memex/dev/elps/FATHOMDB-CONSULT.md`** — the Memex ELPS decision record (the 5
  additive pins ratified 2026-06-12; all additive, no `schema_version` bump)

This ADR records the **engine-side contract** that Slice 15 implements.

---

## 2. Decision

### 2.1 Transport

**Primary (Slice 15 implements): subprocess + NDJSON over stdio.** FathomDB spawns the harness
as a child process; sends one JSON request per line on **stdin**; reads one JSON response per line
on **stdout**; harness logs/diagnostics go to **stderr only** (never stdout). Language-agnostic,
no network, no ports. The spawn command is **caller-configured** — FathomDB does not hardcode a
specific binary name; the consumer registers the provider command + args.

**Also acceptable** (the protocol is transport-agnostic): Unix-domain-socket daemon (same JSON
framing), or an in-process callback registered through FathomDB's Python/TS binding. TCP/HTTP
network endpoints are **not acceptable as the primary** — they violate the local-first contract.

### 2.2 Protocol: `fathomdb.extract.v1`

All messages are single-line UTF-8 JSON. Every message carries `"protocol": "fathomdb.extract.v1"`
and a `"type"` discriminant. Unknown fields are ignored (forward-compatible); unknown `type` →
an `error` response.

**Four message types:**

1. **`hello`** (FathomDB → harness on startup) — handshake initiation
2. **`ready`** (harness → FathomDB) — capability advertisement (`provider`, `model`, `supports`,
   `schema_version`)
3. **`extract`** (FathomDB → harness) — extraction request
4. **`result`** (harness → FathomDB) — extraction response

Plus **`error`** (harness → FathomDB) in place of a `result` on failure.

FathomDB aborts if `protocol` or `schema_version` mismatch in the `ready` response.

### 2.3 Five pinned additions (ratified 2026-06-12; all additive, no `schema_version` bump)

All five additions are **additive clarifications** to the v1 wire contract. They do not change the
`schema_version`; they freeze previously underspecified behavior.

**Pin 1 — `options.instructions` (Q1).** An optional field on the `extract` message's `options`
object:

```jsonc
"options": {
  "deterministic": true,
  "max_facts_per_doc": 24,
  "language": "en",
  "instructions": "<optional: authoritative extra extraction guidance from FathomDB>"
}
```

When absent: the harness authors the extraction prompt freely (provider ownership).
When present: the harness **MUST incorporate it as binding guidance** (MAY adapt phrasing to its
model, NOT strictly verbatim), and **MUST include it in the determinism cache key**.

**Pin 2 — `source_span` unit (Q4).** `source_span` = UTF-8 byte offsets, half-open
`[start_byte, end_byte)`, 0-based, end-exclusive, into the `body` field **as transmitted in the
request** (same bytes; do not re-encode before measuring). Rationale: FathomDB's store is
Rust/byte-indexed. If the harness cannot derive a reliable byte span, **omit (`null`)** rather
than emit a guessed span.

**Pin 3 — Replay determinism (Q3).** Real LLM stacks are not token-deterministic. The v1 guarantee
is **"replay determinism" — a cache property**:

> Under `options.deterministic=true`, the harness MUST return **byte-identical bytes** for any
> request whose canonical form is present in its cache, **including the shipped/seeded golden-fixture
> cache**. It does NOT guarantee byte-identical generation across cold/novel environments.

Cache key: `sha256(canonical(documents, ontology, options /*incl. instructions*/, ready.model, PROMPT_VERSION))`.
Conformance is asserted **only over the shipped fixture cache**. FathomDB pins the extracted-graph
artifact for eval reproducibility, not live re-extraction.

**Pin 4 — `warnings.kind` enum (D5).** v1 typed enum:

| `kind` | Required extra fields |
|--------|----------------------|
| `supersedes` | `supersedes_hint`, `prior_body` |
| `doc_dropped` | `source_doc_id`, `detail` |
| `no_facts` | `source_doc_id` |
| `validation_failed` | `source_doc_id` |

Unknown `kind` → FathomDB ignores the warning but keeps the `result`. Any document dropped from
a `result` MUST emit a warning carrying its `source_doc_id`.

**Pin 5 — Per-document timeout (Q7).** `ELPS_TIMEOUT_S` is a **per-document** timeout (default
60 s/doc). A per-doc timeout is transient → fails the whole `extract` request (D4) and the
`timeout` error names the offending `source_doc_id`. The whole-`extract` wall-clock is
approximately `n_docs × per_doc`; batches are bounded (≤ `max_docs_per_request`) to control it.

---

## 3. What FathomDB (engine-side) commits to implement (Slice 15)

### 3.1 Spawn + handshake

- Configure the harness command + args from caller registration (not hardcoded)
- Spawn the subprocess with stdio streams attached
- Send `{"protocol":"fathomdb.extract.v1","type":"hello"}` on startup
- Verify the `ready` response: `protocol` must match, `schema_version` must be 1; abort otherwise
- Record the `ready.model` value in the `extractor_model_id TEXT` column (G11, step-14) on every ingested edge as extractor provenance

### 3.2 Extract dispatch

- For each ingest batch, send one `extract` message per `request_id`
- Honor the harness's advertised `max_docs_per_request` (never exceed it)
- For large corpora, stream multiple bounded batches rather than one huge batch
- On `result`: proceed to ingest mapping (§3.3)
- On `error`: propagate the typed error code (`llm_unavailable|invalid_request|extraction_failed|timeout`) as a FathomDB-typed error; mark the batch as failed; caller decides whether to retry (retriable codes) or surface as permanent failure

### 3.3 `entities` → nodes mapping

- Map each `entities[]` entry to a FathomDB `canonical_nodes` row via `logical_id` resolution:
  - Derive a stable `logical_id` from `name`+`type` (normalization algorithm is engine-internal)
  - If a node with that `logical_id` already exists, skip insertion (idempotent ingest)
  - If new: insert with `kind = entity.type`, `body = entity.name` (plus aliases as a JSON property)
- FathomDB owns canonical identity; the harness's intra-request `(name, type)` dedup is advisory

### 3.4 `edges` → `canonical_edges` enrichment columns

Map each `edges[]` entry from the extract `result` to a `canonical_edges` row with the G11
enrichment columns (step-14, SCHEMA_VERSION 14 — see ADR-0.8.1-graph-substrate-g11-migration.md):

| Extract response field | `canonical_edges` column |
|------------------------|--------------------------|
| `relation` | `kind` |
| `from_entity` (resolved `logical_id`) | `from_id` |
| `to_entity` (resolved `logical_id`) | `to_id` |
| `body` | `body` (TEXT) |
| `t_valid` | `t_valid` (TEXT, ISO-8601 or NULL) |
| `t_invalid` | `t_invalid` (TEXT, ISO-8601 or NULL) |
| `confidence` | `confidence` (REAL ∈ [0,1]) |
| `source_doc_id` | `source_id` |
| `ready.model` (from handshake) | `extractor_model_id` (TEXT, G11 column) |

### 3.5 Invalidate-not-accumulate bookkeeping

Before inserting a new fact-edge, the engine **checks for existing active edges** on the same
`(from_id, to_id, kind)` tuple with overlapping temporal scope:

1. Query `canonical_edges` for rows where `from_id = ?`, `to_id = ?`, `kind = ?`, and
   `superseded_at IS NULL`
2. For each matching active row: set `superseded_at = now_write_cursor` (tombstone)
3. Insert the new enriched row as the active fact-edge

This preserves the full history (invalidate-not-delete) while ensuring at most one active row
per `(from_id, to_id, kind)` at any transaction-time point.

### 3.6 Conformance fixture

Slice 15 ships a **golden input → expected output** conformance fixture in the test suite:

- A small set of `extract`-request JSON objects (covering: a simple fact, a temporal/superseding
  fact, a multi-entity sentence, and a "no extractable facts" doc)
- The expected schema-valid `result` JSON under `deterministic=true` against the seeded cache
- The fixture validates: handshake protocol version, schema-valid `result` per `request_id`, the
  superseding warning carries `supersedes_hint`, the empty-edges case emits `no_facts` warning
- Footprint assertion in test: no network egress observed from FathomDB's process during the
  conformance run

---

## 4. What FathomDB does NOT implement (ever)

The following **never appear** in the FathomDB codebase:

- Any LLM API call (OpenAI, Anthropic, local model server, etc.)
- Any network socket opened by FathomDB itself for LLM access
- Any stored API credential, model weight, or embedding model for extraction
- Any HTTP/TCP endpoint for the extraction protocol (local-first constraint)

The only LLM boundary in 0.8.1 is the `fathomdb.extract.v1` subprocess handshake. Everything
inside that boundary belongs to the caller's harness.

---

## 5. Falsifiable acceptance bar (Slice 15 tests)

The following are the concrete testable assertions that gate Slice 15 acceptance:

1. **Handshake**: FathomDB can spawn the harness, send `hello`, receive a `ready` with matching
   `protocol`/`schema_version`, and record `model` as provenance.
2. **Schema-valid result per `request_id`**: every `extract` request receives exactly one `result`
   or `error` with the same `request_id`.
3. **Entity→node mapping**: extracted entities appear in `canonical_nodes` with the correct `kind`
   and a stable `logical_id`.
4. **Edge→`canonical_edges` mapping**: extracted edges appear in `canonical_edges` with the G11
   columns (`body`, `t_valid`, `t_invalid`, `confidence`) correctly populated.
5. **Invalidate-not-accumulate**: ingesting a superseding fact-edge tombstones the prior active
   edge (`superseded_at` set) and inserts the new row; the prior row is retained (not deleted).
6. **Footprint**: no network egress from FathomDB's process during the conformance test run.
7. **Golden fixture reproducibility**: the conformance fixture reproduces byte-identically under
   `deterministic=true` against the seeded cache.

---

## 6. Status (decision-ready)

This ADR is **decision-ready** with a falsifiable spec. Slice 15 tests the handshake + schema-valid
response + footprint. The HITL gate package is the Slice-0 close artifact; sign-off is required
before Slice 15 opens.

---

## 7. Explicitly deferred / not in this ADR or Slice 15

- **Bundled CPU local-LLM extractor (R3b)** — deferred to 0.8.2 (`dev/roadmap/0.8.2.md`).
  0.8.1 graph construction is BYO-LLM only.
- **TCP/HTTP network endpoint for the protocol** — explicitly rejected (local-first contract).
- **`options.min_confidence` provider-side filtering** — forward-compatible future addition,
  not in v1.
- **Cache/PII purge-by-doc hook** — Memex owns this on their data plane; not a v1 blocker.

---

## 8. References

- v1 brief (harness side): `dev/plans/prompts/IR-C-byo-llm-extraction-harness-memex.md`
- Memex decision record (5 pins): `~/projects/memex/dev/elps/FATHOMDB-CONSULT.md`
- G11 schema migration: `dev/adr/ADR-0.8.1-graph-substrate-g11-migration.md`
- Graph model substrate: `dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` (H3 reservation)
- 0.8.1 slice contracts: `dev/plans/0.8.1-implementation.md` (Slice 15)
- 0.8.2 deferred bundled extractor: `dev/roadmap/0.8.2.md`
