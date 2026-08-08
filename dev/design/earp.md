---
status: COMPLETE
---

# EARP — configurable retrieval evaluation platform

> **Execution status:** S0–S10 are implemented and independently reviewed on
> the EARP integration candidate. EARP remains offline, EVAL-ONLY,
> developer-side tooling (D-1): it does not alter the library query path, mint
> a quality gate (D-2), select a release, or authorize priced or network-backed
> evaluation beyond the D-3 authorization. Current rulings are recorded in
> `dev/notes/earp-hitl-decisions.md`.

## Purpose

EARP runs a declared FathomDB configuration over separately supplied corpus and
gold data, then writes durable, structured evidence describing what that
configuration did. It answers both single-arm questions ("what does this
configuration do?") and comparative questions ("does one declared change
improve a metric on this fixed corpus?").

EARP joins three layers that are currently tested separately. Its first
implementation is deliberately a small, code-grounded subset; later campaigns
can span the full surface only when the SDK exposes a real, witnessed call path.

1. **Store semantics:** canonical writes, provenance, logical identity,
   lifecycle, validity windows, edges, and optional extractor ingestion.
2. **Engine and projection configuration:** `EngineConfig`, default-embedder
   opt-in, and durable filter/FTS/vector projection declarations.
3. **Retrieval:** text or hybrid search, filters and views, rerank depth, graph
   arm, alpha, pool size, and diagnostics.

It reuses the existing corpus pin, the R2 identical-answerer protocol where
applicable, and the `experiments/` record format. It **ports** the IR-B
evidence-recall definitions rather than reusing them: IR-B exists only as Rust
test-support (`fathomdb-engine/tests/support/ir_eval.rs`) with no Python
surface, so every EARP metric is necessarily a port and is held to a pinned
parity test against the Rust reference. It must not create a competing metric
or ad-hoc output convention — Python already carries at least six ad-hoc forks
of recall/MRR/nDCG, and EARP exists partly to stop that spread.

## Non-goals

- EARP is not a production router, an SDK feature, or a CI gate.
- EARP does not generate gold answers or relevance judgments autonomously.
- EARP does not treat all runtime knobs as retrieval-quality levers.
- EARP does not send data to a network service by default. Local/BYO answerer,
  Mem0, extraction, and GPU paths are explicit optional arms.
- EARP does not reflect arbitrary SDK arguments from YAML or claim a knob is
  configured merely because it can be represented in Python.

## Campaign kinds

`comparisons` are optional. A run declares exactly one campaign kind:

| Kind | Question | Required result |
| --- | --- | --- |
| `characterization` | What does one configuration do? | Metrics, integrity witnesses, uncertainty, blockers |
| `comparison` | Does treatment differ from control? | Paired delta, CI, power status, declared changed knobs |
| `sweep` | How do declared configurations trade off? | Per-arm metrics and optional Pareto view; no causal claim by default |
| `replay` | Did this resolved configuration drift? | Prior-record comparison with environment/code deltas |
| `diagnostic` | Is a store/index invariant true? | Named witness or typed failure; gold is optional |

Only `comparison` may claim a one-knob causal result. Its resolved arms must
differ only at the `changed_knobs` paths; otherwise validation fails.

## Inputs and validation

Each file-backed configuration is strict and versioned (`earp.v1`): unknown
keys, absent required keys, invalid knob values, and unused declared knobs are
hard errors. It references rather than embeds data:

```yaml
campaign: characterization
corpus:
  # The snapshot is the GoldSet's identity. `manifest.json` is raw-corpus
  # provenance — recorded separately, never conflated (D-6 condition 2).
  snapshot: tests/corpus/snapshot.json
  manifest: tests/corpus/scripts/manifest.json
  # Gitignored, so absent from worktrees. An explicit root; absence is a
  # typed blocker, never a silent empty gold set.
  data_root: data/corpus-data
gold:
  path: data/corpus-data/eval/ir_gold/all.gold.json
  sha256: <64-hex of the pinned file>
  corpus_hash: fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e
  qrels_version: ir-c-reused-v2   # v1 is refused as a typed blocker (D-6.3)
scenario:
  engine:
    use_default_embedder: false
  store: { mode: canonical_docs }
  query:
    # A named SDK call, not a symbolic label. Three exist:
    # Engine.search | Engine.search_projected_text | Engine.search_text_only
    call: Engine.search
metrics:
  # Every search mode requires K <= the declared public result limit (default 10,
  # maximum 100). The runner records that limit with each metric.
  evidence_recall_k: [5, 10]
  document_metrics: [mrr]        # ndcg is not_applicable — no graded gold exists
  integrity: [projection_coverage, provenance]
decision_rule:                    # optional (D-4); absent ⇒ no better-than claim
  metric: evidence_recall_strict@10
  direction: greater
  threshold: 0.42
```

The corpus snapshot and gold-set hash are required whenever a run makes a
retrieval-quality claim. A transformation that changes corpus semantics must
declare a new corpus identity and matching gold basis; EARP must not reuse a
gold set by convenience.

Acceptance thresholds and better-than claims differ per experiment (D-4), so
the decision rule is **declared per campaign and predeclared before the run**,
then persisted into the resolved config and the sidecar. A campaign with no
declared rule may report metrics but may not claim one configuration is better
than another.

## Code-grounded v1 surface

EARP v1 does not promise universal runtime configuration. It supports two
bases, deliberately: a small human-authored document-level fixture for the
fast, network-free diagnostic path, and the frozen corpus snapshot with the
IR-C reuse-tier gold for the quality campaign (D-6). Both use canonical
document writes through the public Python SDK and one **named** production
search operation. Three search entry points exist — `Engine.search`,
`Engine.search_projected_text`, and `Engine.search_text_only` — so a scenario
names the call rather than a symbolic label. `Engine.write` requires every
canonical item to carry `source_id`. Each scenario uses a fresh temporary
SQLite database and records store and result integrity witnesses.

Every catalog entry names its SDK call path and its expected witness before it
can be `supported`. In the current SDK, `Engine.open(...,
use_default_embedder=...)` is the only `EngineConfig`-adjacent setting passed
to the native open call: `Engine.open` calls `_NativeEngine.open` with
`use_default_embedder`; the `EngineConfig` object itself does not cross the
native boundary.

**The catalog is keyed on whether a concrete SDK call path exists, not on
dataclass membership.** Those are different questions, and conflating them
produces false entries. `slow_threshold_ms` is an `EngineConfig` field that
`Engine.open` does not forward, yet it has a live native path of its own —
`Engine.set_slow_threshold_ms` — so it is **supported** (`runtime`), not
`unsupported`. Its sibling `Engine.set_profiling` belongs on the candidate
list for the same reason. Each of `EngineConfig`'s five fields therefore
receives an individual verdict rather than a blanket one. EARP records the
native-boundary distinction as an observed finding rather than repeating the
SDK's broad `EngineConfig` docstring.

Typed store, projection, and retrieval matrices are later extensions. They
must expose concrete SDK calls, not symbolic labels. For example, a projection
scenario must declare an actual `ProjectionSpec` shape (name, roles, and
enabled FTS/vector behavior), rather than a shorthand such as `body_vector`.

## Knob catalog

EARP owns a typed catalog rather than passing arbitrary YAML through to the
SDK. Each exposed knob is classified as one of:

- `semantic`: can alter stored data or returned results;
- `indexing`: changes an index/projection. EARP uses a fresh database per
  scenario as a **run-isolation policy**, not because the engine requires one:
  `configure_projections` diffs against the durable registry and backfills the
  difference in one transaction, with explicit `drop` semantics and
  `ProjectionDestructiveError` for unnamed destructive changes
  (the Python `Engine.configure_projections` method);
- `runtime`: can alter performance or execution but is not a relevance claim;
- `observability`: captures evidence without changing the result contract; or
- `held_constant`: known but intentionally unchanged for the scenario, with a
  reason; or
- `unsupported`: explicitly unavailable, with a reason.

Catalog coverage is tested against a maintained, code-reviewed candidate list,
not broad reflection over public Python parameters. Each entry has a
classification, SDK call path, witness, and reason. All `EngineConfig` fields
are represented as held or unsupported until forwarding is real. A new
candidate cannot become configurable without a call path and witness.

Custom embedder implementations are currently unsupported by the Python SDK;
the pinned default embedder may be enabled or disabled. A stored projection
embedder identity is not treated as proof that a custom runtime implementation
was supplied.

## Execution model

The runner resolves defaults and scenario inheritance before execution. It then
creates one fresh database per scenario, configures any supported projections
before ingest, writes the declared corpus form, runs queries, and records every
success, miss, skip, and typed failure. Scenarios never share a mutable
database.

When a scenario requires a dense projection, the runner captures three
distinct signals from **three different sources** — they are not
interchangeable, and only the first comes from polling:

- `vector_dense_readiness` — polled from `read.projections()` to a declared
  timeout. The binding values are `"unavailable"`, `"embedding"`, and
  `"ready"`; `None` belongs only to a declaration without a vector sub-target.
  The runner distinguishes no declared dense projection from a declared but
  unavailable one before interpreting it. A transient `embedding` state that
  times out is a typed blocker.
- `vector_unsupported_kinds` — captured from the `ProjectionDelta` returned by
  `configure_projections` at declaration time (the projection-spec and
  engine configuration contracts). It is **not** on `read.projections()` output; a poll
  loop will never see it. Permanent unsupported kinds are a typed unsupported
  outcome.
- `dense_disabled` / `dense_disabled_reason` — captured from `open_report` at
  open time (the `Engine.open_report` contract). This is **not** "dense indexing
  disabled": it is the 0.8.18 vector-*equivalence* degraded open, after which
  every vector-dependent arm refuses at query time while `search_text_only`
  stays serviceable. It is a typed blocker when dense retrieval was required,
  and the companion `vector_equivalence_refusal_count()` is recorded with it.

None of the three is converted into an empty retrieval result.

Metrics follow their eligibility rules. Evidence Recall@K is emitted only when
gold has required evidence; document-only gold may emit document recall or MRR
but not evidence recall. Negative queries use abstention correctness, not
recall. A requested inapplicable metric is a configuration error or an explicit
`not_applicable` value, never zero. Answer accuracy is available only through
the R2-compatible, explicitly configured identical-answerer arm.

Three eligibility rules are, on the gold that actually exists, permanently
unsatisfiable and must be reported as such rather than as numbers:

- **nDCG** requires graded relevance. No graded gold exists in this repo — the
  IR-C reuse tier carries a three-value `query_class` and binary `necessity`
  only. nDCG is `not_applicable`, not merely ineligible in principle.
- **Supporting-evidence coverage** is a separate diagnostic in principle, but
  `build_ir_gold.py` is the generator's only `necessity` emission site and
  it emits `required` exclusively — the bucket is empty on every gold set in
  this repo. The Rust reference now models this correctly:
  `PerQueryRecall.supporting_coverage` is `Option<f64>`, `None` when the query
  has no supporting units, and `ClassAgg::supporting()` averages over
  `supporting_query_n` (the support-bearing queries) rather than over all
  queries, emitting that count alongside the value. `experiment_to_json`
  serialises the unavailable case as `null`.

  EARP therefore ports this **faithfully, with no divergence**: `null` maps to
  `not_applicable`, and `supporting_query_n` is carried into the sidecar so a
  reader can tell an empty denominator from a genuine zero. The parity test
  asserts every field with no exclusions.
- **Depth beyond the public result limit** is unavailable. The 0.8.22
  result-limit contract applies to every EARP search mode: `@K` is measurable
  exactly when `K <= limit <= 100`. The runner records the resolved limit with
  every metric and returns a typed `metric_not_measurable` blocker above it.
  EARP does not export the hidden `set_search_limit_for_test` seam.

Comparison pairs use immutable query IDs, state their inclusion/exclusion and
strata, and resolve a fixed confidence-interval method and seed before running.
The result records effect direction, treatment-arm blockers, and all sweep-arm
outcomes. A comparative result is withheld when pairing, metric eligibility, or
predeclared sample/power conditions are not met; `UNDERPOWERED` has meaning
only against that declared rule.

That rule cannot stay implicit: the CI method, seed policy, and power
conditions are **named concretely in the lock artifact**, not left to the
implementing slice. The in-repo prior art is the percentile bootstrap over
per-query recall: `bootstrap_ci` in the Rust harness, with its pinned
`BOOTSTRAP_SEED` and deterministic SplitMix64 RNG, plus the Python
`paired_bootstrap_delta`/`bootstrap_mean_ci` helpers. That duplication is
itself an instance of the fork problem EARP is meant to end. EARP adopts one
method explicitly, records which, and pins its seed rather than inventing a
new one.

## Outputs

The existing `experiments/` helper remains the owner of its compact
`record.json`, `config.resolved.yaml`, `metrics.json`, and index conventions.
EARP adds a versioned sidecar rather than overloading that shared record schema.
Every completed or blocked run has a durable directory:

```text
experiments/runs/<run-id>/
  config.resolved.yaml
  record.json
  metrics.json
  earp.result.v1.json
  earp.per-query.v1.jsonl
```

The sidecar shapes are **locked as machine-checkable schemas**, not prose:
`src/python/eval/earp/schema/earp.result.v1.schema.json`,
`earp.per-query.v1.schema.json`, and `earp.config.v1.schema.json`, mirrored as
frozen dataclasses in `schema/models.py`. The `Witness` structure, the twelve
blocker codes, the run-verdict tokens, and the per-mode depth limits are pinned
there and are what the S3-S5 tests assert against.

The sidecar contains campaign kind, scenario manifest, per-query outcomes,
blockers, comparison data, and environment/open evidence. `record.json` and
append-only `experiments/index.jsonl` remain the shared summary source of
truth. The record schema is closed — `record_from_dict` rejects unknown *and*
missing keys, top-level and nested — so the sidecar is
forced, not stylistic.

The writer stages and validates all artifacts first, materializes the shared
record and metrics second, and appends the index last. **That ordering is not
achievable by calling `_lib.write_record` and then writing the sidecar**:
`write_record` materializes the record, config, and metrics and appends the
index in a single call with no hook between them, so the naive implementation
puts the sidecar *after* the index append — the exact inversion this rule
forbids. The required sequence is:

1. Resolve the config, then derive the identity directly — `config_sha256` →
   `make_run_id` → `experiments/runs/<run_id>/`.
2. Stage and validate the EARP sidecars in that directory.
3. Call `write_record` with a **byte-identical** `config_obj` and the **same**
   `ts`, so it recomputes the same `run_id` and materializes into the same
   directory. Any drift in the config object silently produces a second run
   directory.

`run_id` is minute-resolution, so two runs with the same resolved config
inside one UTC minute collide: `write_record` suppresses the duplicate index
line but still overwrites `record.json` and
`metrics.json` in place. For a tool that offers `replay`, that is a durability
defect in the shared substrate. EARP refuses when a run directory already
exists carrying a differing sidecar, rather than overwriting.

A partial or invalid run is never indexed as complete; a valid blocked run is
indexed only with its explicit blocked verdict. Because `Record.verdict` is an
untyped `str`, the exact verdict tokens are pinned in the lock
artifact so "blocked" cannot be spelled three ways. Comparison output includes
N, effect size, confidence interval, decision-rule result, and `UNDERPOWERED`
when that is the honest outcome.

## Footprint and cost boundaries

The default policy denies network access. Remote weight fetch is an explicit
opt-in.

**Embedder-cache evidence is post-hoc, not preflight.** A true preflight would
have to prove the cache is populated before open, and the SDK offers no way to
ask: there is no Python cache-status API, and the Rust cache directory derives
from `sha256("<repo>@<revision>")[..12]` over `pub(crate)` constants
(the embedder loader), with `expected_cache_dir()`
gated behind a test cfg. Reimplementing that in Python would hardcode a model
revision and drift from it silently — the run would report "cached" against a
stale directory and download anyway. So EARP instead records
`OpenReport.embedder_download_ms` and `embedder_events`
and **blocker-marks a run that fetched**, which is detectable, drift-free, and
honest. A real preflight becomes available only if a cache-status accessor is
added to the SDK as separately-reviewed work.

Every open records the SDK `open_report`, embedder identity, and material
events. Any answerer, Mem0, extractor, remote weight fetch, GPU, or paid
service is an individually named opt-in arm with its model/configuration, cost,
and blocker status recorded. EARP never hides a skipped arm as a zero score or
a successful result.

Cost is **enforced, not merely recorded** (D-3): $5.00 is pre-authorized
cumulatively across all priced runs, and anything beyond requires HITL
approval. Recording alone already exists — `Record.cost_usd` and the index
row's `cost_usd`. Enforcement sums `cost_usd` across
`experiments/index.jsonl`, adds the pending run's declared worst-case
estimate, and refuses to start when the projected total exceeds the remaining
authorization. The refusal is a typed blocker with a durable record, never a
silent skip.

## Directory ownership

| Path | Ownership |
| --- | --- |
| `src/python/eval/earp/` | EARP runner, schema, adapters, metrics, and CLI |
| `src/python/eval/earp/schema/` | The v1 lock artifact — JSON Schemas + frozen dataclasses |
| `src/python/tests/earp/` | EARP unit and real-SDK integration tests |
| `experiments/configs/earp/` | Versioned campaign configurations |
| `experiments/runs/<run_id>/` | Durable generated run artifacts |
| `dev/design/earp.md` | This design of record |
| `dev/plans/earp-foundation.md` | Implementation plan and slice sequence |
| `dev/notes/earp-hitl-decisions.md` | HITL rulings D-1…D-6 |

`experiments/runs/<run_id>/` uses an underscore, matching the `_lib` run path.

**A noted convention divergence.** `experiments/README.md` states the
standing rule that "an experiment is a typed CONFIG … new experiments are new
config files (the `eval/*/config.py` convention), not bespoke runners with
inlined constants." EARP introduces declarative YAML under a new
`experiments/configs/` directory instead. That is the better fit for a
configuration-driven evaluation platform — a declarative, strictly-validated,
version-pinned campaign file is precisely the artifact EARP exists to
consume — but it **is a new convention, not reuse**, and it is recorded as such
rather than presented as following the existing one. `experiments/README.md`
should be updated to describe both.
