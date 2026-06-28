# ELPS consult #3 — `ready.provenance` (PRE-3) — ANSWERED (2026-06-14)

> Stored record of the Memex/ELPS reply to FathomDB's PRE-3 protocol consult.
> **Source (authoritative, relayable as-is):** `dev/elps/FATHOMDB-CONSULT-3.md` in
> the Memex repo (`/home/coreyt/projects/memex/`). This is the FathomDB-side copy so
> the decision lives in our tree too. Precedent chain: `FATHOMDB-CONSULT-2.md`
> (round-2 additive fields), round-1 (`options.instructions`).

## The question

Is an additive **OPTIONAL** `ready.provenance` object within the ratified
`fathomdb.extract.v1` envelope at `schema_version 1`, or does it need a
`schema_version` bump / re-ratification?

## The answer — ◆ ACCEPT at `schema_version 1`. No bump, no re-ratification

FathomDB's default is correct: treat `ready.provenance` as **optional at v1**; no
Memex change is required for FathomDB to start reading it. Rationale:

1. **Forward-compat posture the contract already guarantees (FR-V2):** an optional
   field where *absent ⇒ ignored* is non-breaking by construction in both directions.
2. **Direct precedent:** same principle as the round-2 additive fields ratified at v1
   with no bump (`entities[].synthesized`, `warnings.kind += temporal_fallback/capped`,
   `FATHOMDB-CONSULT-2.md`). `ready.provenance` is strictly weaker (handshake-only,
   informational).
3. **Touches nothing load-bearing:** not the 5 frozen pins, not the `result`/`error`
   schema, not the determinism **cache key**, not the golden conformance fixture
   (golden asserts `result` bytes, not `ready`).

## Load-bearing clarifications

- **`ready.model` stays authoritative** (FR-H3) — it keys the determinism cache and
  MUST change when the logical model family changes. The richer `provenance` object
  (resolved model, effort, temperature, `prompt_version`, …) is an informational
  **superset** that complements, never replaces, `ready.model`.
- **Contract-allowed-now ≠ ELPS-emitted-now.** FathomDB reading optional
  `ready.provenance` is v1-conformant today; production ELPS doesn't emit it yet
  (FathomDB sees "absent ⇒ ignored"). ELPS *adopting* it is a separate optional
  Memex-side slice on their timing — **not** required by this consult, **not** a gate.
- **⚠️ Correction to our E0b framing:** **production ELPS does NOT truncate document
  bodies** — it sends the full `body`, one LLM call per doc (DESIGN §7 / FR-B2). The
  2k truncation (`_ELPS_MAX_CHARS`) is **purely a FathomDB-eval-harness concern**, so
  our E0b chunking fix is eval-harness-only (confirms the "caller-side" framing) and
  has no production-ELPS counterpart.

## Deferred (agreed)

- **E4 grounding context as a protocol *input* field WOULD touch `extract.v1` →
  real re-ratify** (a request field the provider must honor + fold into the
  determinism cache key, like round-1 `options.instructions`). Flag when it firms up.
- **Production ELPS adopting provenance / backoff:** additive, Memex's timing,
  non-blocking. (429 → ELPS maps to retriable `llm_unavailable` today, FR-B4; ELPS-side
  backoff would be an internal, protocol-invisible enhancement.)

## Net for FathomDB

**Proceed with PRE-3 as designed** — read/emit `ready.provenance` as optional at
`schema_version 1`; no Memex dependency. Memex will fold the "sanctioned optional v1
field" note into their `EXTRACTION_PROTOCOL.md`. **E4 is the only item that will
bring us back to the table.**
