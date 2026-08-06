---
status: PROPOSED
---

# EARP Slice 3 — strict resolver and knob catalog

Design of record for S3 of `dev/plans/earp-foundation.md`. Depends on S2
(`check_depth`) and the S0 lock. **Revision 2** — the first revision was
returned **REWORK**; § Review records what changed and why.

## Contract

S3 makes it impossible to express a run that cannot be honestly executed. A
configuration either resolves into a typed, fully-accounted scenario, or it is
rejected with every offending key named at once.

Pure: no SDK calls, no database, no network, no filesystem beyond the config
file and EARP's own packaged schema.

## Rejection classes

| Class | Meaning | Blocker code |
| --- | --- | --- |
| unknown | a key the schema does not define | `config_unknown_key` |
| missing | a required key absent | `config_missing_key` |
| invalid | a defined key with an out-of-domain value | `config_invalid_value` |
| inapplicable | a declared knob the named call does not accept | `config_inapplicable_knob` |
| unused | a declared key no slice will ever consume | `config_unused_key` |
| inexpressible | a campaign kind `earp.v1` cannot represent | `config_campaign_inexpressible` |

All rejections are **collected**, not first-failure. This deliberately differs
from `verify_gold`, which returns on the first failure because its checks are
ordered by trust — an unverified file's fields cannot be trusted enough to
name a second defect. Config keys carry no such dependency, and a config author
needs every offending key in one pass.

## The schema is the resolver's source of truth

The first revision proposed hand-written key tables plus a test comparing them
to the JSON Schema. That was wrong twice over: the comparison could not catch
`pattern`, `const`, `minimum`, `minItems`, or `uniqueItems`, and — decisively —
it made AC-2 unsatisfiable, because adding a schema key without wiring it would
be refused as *unknown* rather than *unused*.

Instead, `eval/earp/schema/validate.py` is a small pure-stdlib walker that
interprets the exact keyword subset this schema uses: `type`, `enum`, `const`,
`required`, `properties`, `additionalProperties: false`, `items`, `minimum`,
`maximum`, `minItems`, `uniqueItems`, `pattern`. That subset is total over
`earp.config.v1.schema.json`, which has **no `$defs`, no `$ref`, no
`if`/`then`, no `allOf`/`anyOf`/`oneOf`**, and a maximum nesting depth of 3.
(The `$defs`/`oneOf` nesting exists only in the *result* schema, which S3 does
not consume.)

So the known-key set is **derived from the schema**, unknown/missing/invalid
fall out mechanically, and the hand-written tables carry only cross-field
semantics — where a divergence is a design question, not a transcription error.

`jsonschema` is not used. It is importable in this environment but absent from
`pyproject.toml`'s `dependencies` and every extra, which is exactly how the
repo previously shipped a harness that failed a clean install — the numpy
declaration still carries the codex §9 [P1] note. A guard test asserts no
module under `eval/earp/` imports it, so the trap cannot be walked into later.
(`pyyaml` *is* declared, but only in the `test` and `dev` extras; that is
correct for a harness under D-1 and is not a runtime dependency.)

## Consumption: two assertions, not one

The first revision said the resolver "refuses any declared path it never
touched". That would refuse almost every legal config, because S3 is a pure
resolver and most keys are consumed by later slices — `corpus.*` and `gold.*`
by S1, `store.mode` by S5, `document_metrics` by S6, `comparison.*` by S8,
`budget.*` by S9.

Consumption is instead tracked at **declaration site**, and split:

1. **Static, one test — the real anti-drift device.** Every path the schema
   declares must appear in `CONSUMER_REGISTRY: dict[str, Consumer]`, mapping
   each path to an owning slice and an applicability predicate. This is what
   makes "adding a config key without wiring it" a red test, and it works
   today against not-yet-landed slices.
2. **Per-config, at resolve time.** A declared path is refused only when its
   applicability predicate is false for *this* config — `comparison.*` on a
   characterization, `budget.*` with no priced arm. A path owned by a
   later slice resolves as **carried**, not refused.

Paths are marked at leaf nodes, with arrays marked at the array node and never
per element.

Two paths are genuinely unconsumed by any slice today — `scenario.store.mode`
and `metrics.integrity`. They are registered with their owning slice (S5, S6)
and carried. (The first revision cited `gold.corpus_hash`/`qrels_version` as
the live example; that is now stale — S1 consumes both.)

## Mode is derived, never declared

Depth validation needs a retrieval mode, and the first revision assumed one was
declared. Three problems, all fatal as written:

- `scenario.query.mode` is **optional** in the schema, so there may be nothing
  to pass to `check_depth`, which takes a non-optional `RetrievalMode`.
- `call` already determines the mode. `Engine.search_text_only` is FTS-only by
  construction; `search_projected_text` is one property-FTS projection;
  `search` is hybrid.
- **`RetrievalMode.VECTOR_ONLY` has no SDK entry point at all.** There are
  exactly three search verbs and none is vector-only.

So the mode is derived from `call` through an explicit `CALL_MODE` table,
`scenario.query.mode` is **removed from the schema**, and a config naming
`vector_only` is refused with the reason that no vector-only SDK entry point
exists. Depth is then checked per `evidence_recall_k` entry against the derived
mode, at validation time, so a config asking @20 on hybrid is refused before it
can produce three copies of the @10 number.

## Knobs the call actually accepts

The schema lets any config carry `rerank_depth`/`alpha`/`pool_n`/
`use_graph_arm` alongside `call: Engine.search_text_only`, which accepts none
of them. That is precisely the "expresses a run that cannot be honestly
executed" case this slice exists to prevent, and the first revision had no
class for it.

A `CALL_PARAMS` table keyed on the three call names drives the **inapplicable**
class. Separately, `search_projected_text` takes a required positional `name`
(the projection to query) for which the schema has **no key at all**, so a
config naming that call resolves cleanly and cannot be run. `scenario.query.
projection_name` is added, required when and only when that call is named.

## Bounds the engine would silently swallow

`alpha` is unbounded in the schema, and the engine **clamps** it to `[0, 1]`
rather than refusing. So `alpha: 5.0` resolves, runs as `1.0`, and the sidecar
records `5.0` — a recorded configuration that is not the configuration that
ran. For a platform whose one failure mode is "a confident number that is not
true", that is disqualifying. The resolver refuses `alpha` outside `[0.0, 1.0]`
and any non-finite value (YAML `.nan`/`.inf` parse to floats and would reach
the engine), and the schema gains `minimum`/`maximum`.

`pool_n` carries `minimum: 1` while the engine accepts `0`. Stricter is
harmless, and the divergence is deliberate: a pool of zero is never a
meaningful evaluation request.

## Gold is required by metrics, not by campaign kind

The first revision required gold for a "retrieval-quality campaign" and never
defined the term. The decidable rule is not campaign-keyed: **any config
declaring `metrics.evidence_recall_k` or `metrics.document_metrics` requires
`gold` and `corpus`**, whatever its kind. It catches the real error — asking
for recall with no gold — and needs no taxonomy. A `diagnostic` campaign may
additionally not declare `evidence_recall_k` at all.

## Campaign kinds v1 cannot express

`earp.v1` has exactly one `scenario` object and no arms array. A comparison
needs two arms, a sweep needs N, and a replay needs a reference to a prior
`run_id` for which there is no key. Accepting them silently is the worst
option, so S3 refuses `comparison`, `sweep`, and `replay` with
`config_campaign_inexpressible`, naming the owning slice (S8, S8, S6) and
stating that the arms structure is a later schema amendment. `characterization`
and `diagnostic` are expressible and resolve.

## The metric namespace

`decision_rule.metric` is an unconstrained string, and **nothing in the landed
code enumerates metric names** — so "refuse a rule naming a metric the campaign
cannot emit" was prose. S3 lands the namespace as a real artifact:

- `METRIC_NAMES`, mapping each name to its emitting condition, drawn from the
  result schema's own field names: `strict_evidence_recall`,
  `graded_evidence_recall`, `supporting_coverage`, `abstention_rate`, plus
  `mrr` and `ndcg`.
- A `<metric>@<k>` grammar, stated explicitly, where `k` must appear in the
  campaign's `evidence_recall_k`.
- `emits(name, scenario) -> bool`, which AC-5 calls. `ndcg` is always false;
  `<name>@20` under a hybrid call is false via `check_depth`;
  `abstention_rate` is false when gold carries no negatives.

The design-of-record's example uses `evidence_recall_strict@10`, which matches
no field in the result schema (spelled `strict_evidence_recall`). That example
is corrected, along with its missing required `schema_version: earp.v1`.

## The knob catalog

Keyed on whether a concrete SDK call path exists, never on dataclass
membership. `eval/earp/knobs.py::CATALOG: tuple[KnobEntry, ...]`.

| Knob | Class | Call path |
| --- | --- | --- |
| `use_default_embedder` | semantic | `Engine.open(use_default_embedder=)` |
| `slow_threshold_ms` | runtime | `Engine.set_slow_threshold_ms` |
| `profiling` | observability | `Engine.set_profiling` |
| `embedder_pool_size`, `scheduler_runtime_threads`, `provenance_row_cap`, `embedder_call_timeout_ms` | unsupported | — (`EngineConfig` fields never forwarded) |
| `rerank_depth`, `use_graph_arm`, `alpha`, `pool_n` | semantic | `Engine.search` |
| `explain` | observability | `Engine.search(explain=)` |
| `view` | semantic | `Engine.search(view=)` — changes which nodes are eligible |
| `filter` | semantic | `Engine.search(filter=)` — no config surface yet |
| `projection_name` | semantic | `Engine.search_projected_text(name=)` |
| `search_expand.depth` | semantic | `graph.search_expand(depth=)` |
| `enable_telemetry`, `record_feedback` | observability | `Engine.*` |
| `drain` | runtime | `Engine.drain(timeout_s=)` — can change whether writes are visible before search, so it can move a recall number |
| `configure_projections` | indexing | `Engine.configure_projections` |
| `attach_logging_subscriber` | unsupported | path exists but is inert — its own docstring defers wiring to a later slice |

`attach_logging_subscriber` is the catalog's proof of purpose: a call path that
exists and does nothing. The witness requirement is what keeps it honest, and
it must be *present* to be tested.

### Completeness without reflection defining the surface

Two bounded introspections, each asserting coverage of a known surface:

1. Every field of `EngineConfig` appears with an individual verdict.
2. Every keyword-only parameter of `Engine.search`,
   `Engine.search_projected_text`, `Engine.search_text_only`, and
   `graph.search_expand` appears with a verdict.

The first revision had only (1), bounded to five fields, and therefore could
not detect the eight omissions above — while naming silent under-coverage as
the failure mode. The remaining `Engine` methods stay a maintained,
code-reviewed candidate list.

## Return shape

Mirrors S1's established convention — returned, never raised:

- `ConfigResolution(blockers: tuple[Blocker, ...] | (), scenario: ResolvedScenario | None)`,
  never both, never neither.
- `ResolvedScenario` carries the resolved values, the derived retrieval mode,
  the metrics ladder, the decision rule or its explicit absence, and the
  consumed-path set — so AC-2 has something to assert against.

## S0 lock amendments carried by this slice

Reviewed here rather than slipped in:

1. Six new `BlockerCode` members for the rejection classes above, mirrored into
   the `blocker` enum of `earp.result.v1.schema.json`.
2. `earp.config.v1.schema.json`: remove `scenario.query.mode`; add
   `scenario.query.projection_name`; add `minimum`/`maximum` to `alpha`.
3. `dev/design/earp.md`: correct the example config's missing `schema_version`
   and its non-existent metric name.

## `earp validate`

`python -m eval.earp.cli validate <path>`, following the established harness
convention of `def main(argv) -> int` plus `if __name__ == "__main__"`, since
`pyproject.toml` has no `[project.scripts]` and `eval/` is not installed. Exit
0 on success, non-zero on rejection. It touches no gold, corpus, or engine — a
config can be well-formed while its data is absent, and conflating the two
would make validation impossible in a worktree.

## Acceptance criteria

1. Each rejection class is produced with its own blocker code and a message
   naming the offending path; multiple defects are reported together.
2. Every path the schema declares appears in `CONSUMER_REGISTRY` with an owning
   slice, asserted by a static test.
3. A declared path whose applicability predicate is false for this config is
   refused as `config_unused_key`; a path owned by a later slice is carried.
4. `evidence_recall_k` beyond the derived mode's measurable depth is refused at
   validation time; `mode` is derived from `call`, and `vector_only` is refused
   as having no SDK entry point.
5. A `decision_rule` naming a metric `emits()` returns false for is refused.
6. A knob the named call does not accept is refused as inapplicable;
   `projection_name` is required exactly when `search_projected_text` is named.
7. `alpha` outside `[0, 1]` or non-finite is refused.
8. `comparison`, `sweep`, and `replay` are refused as inexpressible, naming the
   owning slice.
9. Gold and corpus are required whenever recall or document metrics are
   declared, regardless of campaign kind.
10. Every `EngineConfig` field and every keyword-only search parameter appears
    in the catalog with a verdict; supported entries carry a call path and
    witness.
11. No module under `eval/earp/` imports `jsonschema`.
12. `python -m eval.earp.cli validate` exits 0 on a good config and non-zero on
    each rejection class, touching no gold, corpus, or engine.
13. Every path is exercised by a test that was first observed to fail.

## Review

Independent code-grounded review, 2026-08-06. Verdict on revision 1:
**REWORK** — scope and purity boundary sound, but three load-bearing pieces
were unbuilt and two acceptance criteria were unimplementable. All eighteen
findings are resolved in this revision.

| # | Severity | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | BLOCKER | "Refuse any path never touched" would refuse almost every legal config, since most keys are consumed by later slices; "touched" was undefined | Split into a static registry test and a per-config applicability predicate; later-slice paths are carried |
| 2 | BLOCKER | No return type and no rejection vocabulary — the closed `BlockerCode` cannot represent a config rejection | Six new codes as a governed S0 amendment; `ConfigResolution` + `ResolvedScenario` mirroring S1's convention; rejections collected |
| 3 | BLOCKER | Depth AC unreachable: `mode` optional, contradicts `call`, and `vector_only` has no SDK entry point | Mode derived via `CALL_MODE`; `mode` removed from the schema; `vector_only` refused |
| 4 | MAJOR | AC-2 impossible against hand-written tables — a new schema key would be refused as unknown, not unused | Known-key set derived from the schema by a stdlib walker |
| 5 | MAJOR | The drift test could not catch `pattern`/`const`/`minimum`/`minItems`/`uniqueItems` | Representations collapsed rather than compared; guard test forbids importing `jsonschema` |
| 6 | MAJOR | Catalog under-covered by eight real call paths, and AC-7 was structurally blind to it | Catalog expanded; second bounded introspection over search signatures |
| 7 | MAJOR | `search_projected_text` requires a positional `name` with no schema key, so a legal config cannot run | `projection_name` added, conditionally required; `CALL_PARAMS` drives a new inapplicable class |
| 8 | MAJOR | `comparison`/`sweep`/`replay` are structurally inexpressible in v1 yet were blessed | Refused as inexpressible, naming the owning slice |
| 9 | MAJOR | The budget rule is undecidable — no priced-arm declaration exists | Deferred to S9 and removed from S3 |
| 10 | MAJOR | AC-5 unimplementable: no metric-name space exists anywhere | `METRIC_NAMES`, an `@k` grammar, and an `emits()` predicate land here |
| 11 | MAJOR | `alpha` unbounded and silently clamped by the engine, so the sidecar would record a configuration that did not run | Refused outside `[0, 1]` and when non-finite; schema bounds added |
| 12 | MAJOR | "Retrieval-quality campaign" was never defined and gold is optional in the schema | Replaced with a metrics-keyed rule |
| 13 | MINOR | The design-of-record's example omits the required `schema_version` | Corrected as an S0 amendment |
| 14 | MINOR | The cited unconsumed-key example is stale — S1 now consumes both | Re-cited to `store.mode` and `metrics.integrity`, which are still open |
| 15 | MINOR | "No filesystem" contradicted by reading the schema | Reworded |
| 16 | MINOR | Catalog location unspecified, so AC-7 had no subject | `eval/earp/knobs.py::CATALOG` |
| 17 | MINOR | `earp validate` is not a real invocation — no `[project.scripts]`, `eval/` not installed | `python -m eval.earp.cli validate`, per the `m1`/`r2` convention |
| 18 | MINOR | `pyyaml` is declared only in extras | Stated |

Confirmed correct by the review, with no change required: the four-class
taxonomy's mechanical decidability for *unknown* (every schema object carries
`additionalProperties: false`); the `pool_n`-is-not-a-depth-control
clarification; the call-path-not-dataclass-membership key, with every original
catalog row's call path verifying; calling `check_depth` at both config and run
time as one predicate with two enforcement points; and refusing a `jsonschema`
dependency, for the right reason.
