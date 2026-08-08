---
status: CURRENT
---

# EARP agent orientation

EARP is evaluation tooling around FathomDB, not FathomDB itself. It calls the
public Python SDK to create real databases, write declared fixtures, and record
observed retrieval behavior. It must not change the production SDK, library
query path, storage schema, release gate, or CI policy as a side effect of
building an evaluation.

## Navigate from the contract outward

Read [the EARP design](../design/earp.md) before the implementation plan in
`dev/plans/earp-foundation.md`. The design defines the evidence contract; the
plan deliberately starts with a narrow v1 rather than pretending the entire
SDK configuration surface is already executable.

The owned source and test locations are `src/python/eval/earp/` and
`src/python/tests/earp/`. Versioned campaign inputs belong in
`experiments/configs/earp/`; generated run evidence belongs in
`experiments/runs/<run-id>/`. Reuse the shared `experiments/` helper for its
record, metrics, and index conventions. Put EARP-specific campaign state,
per-query results, blockers, and comparison analysis in the versioned EARP
sidecar, rather than expanding the shared record informally.

Corpus data and gold data are different inputs. Pin both identities, retain the
gold basis that matches the corpus semantics, and do not manufacture gold or
turn an unavailable metric into zero.

The gold is real and already exists — `data/corpus-data/eval/ir_gold/all.gold.json`,
the IR-C reuse tier, 4,597 queries whose `corpus_hash` matches the frozen
snapshot. It is **gitignored**, so it is absent from worktrees: resolve it
through an explicit configured root and treat absence as a typed blocker, never
as an empty gold set. Pin it by SHA-256 and **refuse `qrels_version:
ir-c-reused-v1`** — the cached files are v1 while the committed generator emits
v2. Claims stay scoped to reuse-tier, document-level evidence gold.

You **port** IR-B's metric definitions; you do not reuse them. IR-B lives only
in `fathomdb-engine/tests/support/ir_eval.rs` with no Python surface. Hold the
port to a pinned parity test against the committed fixture
`tests/fixtures/ir_gold/synthetic_gold.json`. Parity is **full — no excluded
fields**. `supporting_coverage` is `Option<f64>` in the reference (`None` when
a query has no supporting units) and `ClassAgg` averages over
`supporting_query_n` rather than all queries; the JSON carries `null` for the
unavailable case. Map `null` → `not_applicable` and carry `supporting_query_n`
into the sidecar, so an empty denominator is never mistaken for a genuine
zero. The R2 identical-answerer protocol applies where relevant.

## Treat configuration as demonstrated behavior

Never feed arbitrary YAML through to SDK calls. A supported knob needs a
maintained catalog entry with its concrete call path and an observed witness.
`use_default_embedder` is the only `EngineConfig`-adjacent setting that reaches
native open.

Key the catalog on **whether a concrete call path exists**, not on whether a
knob is an `EngineConfig` field — they are different questions.
`slow_threshold_ms` is an `EngineConfig` field that `Engine.open` never
forwards, yet it has its own live path, `Engine.set_slow_threshold_ms`, so it
is supported. Classifying it unsupported because of where it sits in a
dataclass would write a false statement about the SDK into a test.

For projection work, use actual typed `ProjectionSpec` declarations. Three
signals come from three different places — do not collapse them:

- `vector_dense_readiness` — polled from `read.projections()`; `None` means
  both "no vector sub-target" and "caller-authored", so disambiguate before
  interpreting.
- `vector_unsupported_kinds` — from the `ProjectionDelta` that
  `configure_projections` returns; a poll loop never sees it.
- `dense_disabled` — from `open_report`, and it is **not** "dense indexing
  disabled": it is the 0.8.18 vector-*equivalence* degraded open.

Each scenario gets a fresh temporary SQLite database — a run-isolation policy,
not an engine requirement. Default network policy is deny. Embedder-cache
evidence is **post-hoc, not preflight**: there is no Python cache-status API
and the cache path derives from `pub(crate)` Rust constants, so record
`OpenReport.embedder_download_ms` + `embedder_events` and blocker-mark a run
that fetched, rather than hardcoding a model revision into Python.

## Preserve evaluation integrity

Characterization is a valid one-arm result. Comparisons and sweeps are optional
and require their additional rules: immutable paired query IDs, declared
inclusion/exclusion and strata, fixed confidence-interval method and seed, and
predeclared power conditions. Do not infer a causal claim from an unpaired
sweep.

Write artifacts atomically enough that incomplete work is never indexed as a
completed run: validate staged EARP artifacts first, materialize the shared
record next, then append the experiment index last. A blocked run may be
recorded only with an explicit blocked verdict and its durable evidence.

That ordering is **not** what you get by calling `_lib.write_record` and then
writing the sidecar — `write_record` materializes and appends the index in one
call, so the naive version writes the sidecar after the index line, inverting
the rule. Derive the identity yourself first
(`config_sha256` → `make_run_id` → `runs/<run_id>/`), stage and validate
sidecars there, then call `write_record` with a byte-identical `config_obj` and
the same `ts`. `run_id` is minute-resolution, so an identical config inside one
UTC minute collides and silently overwrites — refuse a pre-existing run
directory whose sidecar differs.

Depth follows the public result limit in every mode: `@K` is measurable exactly
when `K <= limit <= 100`; deeper rungs are `metric_not_measurable`. Do not
reach for the hidden `set_search_limit_for_test` seam.

Remember what EARP is for. It never gates FathomDB, which means nothing
downstream will ever catch a wrong EARP number. The failure mode that matters
is not a broken build — it is a confident number that is not true.
