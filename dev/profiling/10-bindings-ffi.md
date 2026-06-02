# 10 — Bindings / FFI marshalling (PyO3 + napi)

**Component:** the language-binding boundary. PyO3
(`src/rust/crates/fathomdb-py/src/lib.rs`) and napi-rs
(`src/rust/crates/fathomdb-napi/src/lib.rs`) marshal idiomatic inputs →
`PreparedWrite` / args, and engine results → Python dataclasses / TS objects
(bindings.md §4: owned typed rows, no lazy cursors crossing FFI).

## Why it matters

Every app call crosses this boundary twice (args in, results out). For cheap
operations (small `get`/`list`, short queries) the marshalling can be a real
share of end-to-end latency, and it's the layer most affected by G1's
`Vec<String> → Vec<SearchHit>` result-shape change. It is also where SDK-parity
cost lives — every new field/verb is paid in *both* bindings.

## Ingest path — what to measure

- **Write-batch marshalling** — `translate_node` / `translate_edge` /
  `translate_op_store` converting dicts/objects → `PreparedWrite`. Scales with
  batch size; measure per-row marshalling vs engine commit. For bulk ingest from
  Python/TS this can rival the SQL for small bodies.
- **Vector input zerocopy** — contiguous LE-f32 (NumPy `ndarray[float32]`, TS
  `Float32Array`) zerocopy-casts to `&[u8]` (ADR-zerocopy-blob); non-contiguous
  (`list[float]`) converts once. Measure the copy cost for the non-contiguous
  path — it's a hidden tax on naive callers.
- **GIL release** — PyO3 calls run inside `py.allow_threads` + `catch_unwind`;
  measure that the engine call actually releases the GIL (so Python threads
  aren't serialized during ingest).

## Retrieval path — what to measure

- **Result marshalling** — owned rows → Python `list[...]` / TS objects. Today
  `SearchResult.results` is `list[str]`; profile the per-hit conversion cost as
  its own retrieval stage (separable from the SQL).
- **G1 cost (the important part).** `Vec<String> → Vec<SearchHit>` means each hit
  becomes a structured object (id, kind, score, body) — more fields, more
  per-hit allocation across FFI. Baseline the string-list marshalling now so G1's
  structured-hit overhead is attributable. Profile **both** bindings (parity:
  the cost shows up twice).
- **Cross-binding consistency** — same DB, same query, from Python and TS: the
  marshalling cost differs (PyO3 sync vs napi `ThreadsafeFunction` + handoff
  pool). Report both; don't generalize from one.

## Key signals / seams

- Measure at the binding (Python `time.perf_counter` / TS `performance.now`)
  around the call, and subtract the Rust-side engine span (from the recording
  `Subscriber`) to isolate marshalling + FFI.
- `engine.counters()` is non-perturbing — safe to bracket calls with it.

## Sharp edges

- The slow-statement / `on_profile` seams see only the *engine* side; FFI
  marshalling is invisible to them. You must measure it from the binding and
  difference against the engine span — there is no in-engine seam for it.
- napi has no u64 — `WriteReceipt.cursor` / future `SearchHit.id` are i64 on the
  TS side; the cast is cheap but the type asymmetry is real (don't profile only
  Python and assume TS matches).
- Don't let marshalling cost hide in "query latency" — for G2/G4 (cheap reads) it
  may be the *dominant* cost; that's a finding, not noise.

## Scaling expectation

Per-call marshalling is ~constant; per-result marshalling scales with hit count
(bounded by `final_limit` for search, by `limit` for list). Constant w.r.t.
corpus N. Matters most for cheap, high-frequency calls and for G1's structured
hits.
