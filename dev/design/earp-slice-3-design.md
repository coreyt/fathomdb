---
status: COMPLETE
---

# EARP Slice 3 — strict resolver and knob catalog

Design of record for S3 of `dev/plans/earp-foundation.md`. Depends on S2
(`check_depth`) and the S0 lock. **Revision 3** — revision 1 was returned
**REWORK**, and revision 2 **REWORK (narrow)**; § Review records both rounds.

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
| unused | a declared key inapplicable to *this* config — not "never consumed" | `config_unused_key` |
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

`$schema`, `$id`, `title`, and `description` are **explicitly ignored** as
annotations; any keyword outside the union of interpreted and ignored is a hard
error, which is what makes the totality claim load-bearing rather than
decorative.

Two semantics the walker must pin, because the config is YAML:

- **`type: integer` must reject `bool`.** PyYAML yields Python `bool`, and
  `isinstance(True, int)` is `True`, so `rerank_depth: true` would resolve as
  `1`. The SDK rejects bools explicitly everywhere; the walker must too.
- **`minimum`/`maximum` cannot catch NaN.** `nan < 0.0` and `nan > 1.0` are
  both `False`, so `alpha: .nan` passes bounds. The non-finite rule is a
  resolver rule *outside* the walker and runs regardless of walker outcome.
  (`.inf` and `-inf` are caught by the bounds.)

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

**Precedence, where the two rules collide.** An inexpressible-campaign refusal
outranks carrying. `comparison.*` is owned by S8 *and* has a false predicate for
every campaign v1 admits, because `comparison` as a kind is itself refused; it
is therefore `config_unused_key` with the reason that the only campaign able to
consume it is inexpressible in `earp.v1`, not silently carried.

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

And a fourth problem the re-review caught, which no reading of `call` alone
would surface: **`Engine.search` is hybrid only when an embedder is
configured.** `use_default_embedder` defaults to `False`, and with no embedder
the vector branch is skipped entirely and the run is pure node FTS. Deriving
`hybrid` from the call alone would record a mode the run did not use *and*
refuse depths it could honestly measure.

The resolver derives retrieval mode from **`(call, use_default_embedder)`** so
the sidecar records whether a vector branch was actually configured. S6a then
superseded this design's per-mode maximum-K table: every search call has the
same public result-limit contract, and `@K` is measurable exactly when
`K <= limit <= 100`.

`scenario.query.mode` is **removed from the schema**.
`RetrievalMode.VECTOR_ONLY` is **retained** — it is live in `MAX_MEASURABLE_K`,
in the result schema's `retrieval_mode` enum, and in S2's landed parity tests —
but it becomes unreachable from any config by construction, which is the point.
Because the key is gone, `mode: vector_only` surfaces as `config_unknown_key`
on a removed key; the resolver carries a named legacy-key rule so that message
says "removed in favour of derivation from `call`" rather than a bare unknown.

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
for recall with no gold — and needs no taxonomy. A `diagnostic` campaign
**MUST NOT** declare `evidence_recall_k` at all — it runs without gold, so a
recall request is a contradiction, refused as `config_inapplicable_knob`.

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
- A `<metric>@<k>` grammar, where `k` must appear in the campaign's
  `evidence_recall_k`. `@k` is **required** for the three per-K names, and
  **forbidden** for the rest, because only `metrics.per_k` is K-keyed:

| Metric | `@k` | Emits when |
| --- | --- | --- |
| `strict_evidence_recall` | required | gold has required evidence and `k` is declared |
| `graded_evidence_recall` | required | as above |
| `supporting_coverage` | required | gold carries supporting units — never on today's gold |
| `abstention_rate` | forbidden | gold carries negatives |
| `mrr` | forbidden | **never** in v1 — no slice computes it |
| `ndcg` | forbidden | **never** — no graded relevance exists |

`mrr` is `emits() == False` with a reason, exactly as `ndcg` is. Nothing landed
or planned computes it — S6's scope is Evidence Recall@{5,10} plus abstention —
so licensing a decision rule against it would let a campaign gate on a number
that never arrives.

`abstention_rate` is K-free in the result schema while
`negative_abstained(retrieved, k)` is per-K. That is a **result-schema**
mismatch, not an S3 one; it is recorded here and left to S6, which owns the
negative aggregate, rather than being quietly papered over by an `@k` this
slice invents.

`emits(name, scenario) -> bool` is what AC-5 calls: `ndcg` and `mrr` are always
false; a depth above the scenario's public result limit is false via
`check_depth`; `abstention_rate` is false when gold carries no negatives.

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
2. Every parameter of `Engine.search`, `Engine.search_projected_text`,
   `Engine.search_text_only`, and `graph.search_expand` — minus
   `{self, engine, query}` — appears with a verdict. **Not** keyword-only:
   `filter` and `name` are positional-or-positional, `search_text_only` has no
   keyword-only parameters at all, and `search_expand`'s `depth` is positional,
   so a keyword-only basis would silently cover none of the rows that matter.

`search_expand`'s four keyword-only filters (`source_type`, `kind`,
`created_after`, `status`) get catalog rows of their own.

**This introspection is not pure**, and the design says so rather than
pretending: importing `fathomdb.engine` loads the native extension, which is
absent in a fresh worktree. The resolver and all its other tests import nothing
from `fathomdb`; this one test **skips visibly** — never silently — when the
binding is unavailable, following the repo's existing
`requires_test_hooks` precedent. Introspection (1) over `EngineConfig` stays
pure, since `config.py` imports only `dataclasses`.

The first revision had only (1), bounded to five fields, and therefore could
not detect the eight omissions above — while naming silent under-coverage as
the failure mode. The remaining `Engine` methods stay a maintained,
code-reviewed candidate list.

## Return shape

Mirrors S1's established convention — returned, never raised:

```python
@dataclass(frozen=True)
class ConfigResolution:
    blockers: tuple[Blocker, ...] = ()      # empty iff scenario is not None
    scenario: ResolvedScenario | None = None

@dataclass(frozen=True)
class ResolvedScenario:
    campaign: CampaignKind
    config_sha256: str                      # S4 pre-derives run_id from this
    query_call: str
    retrieval_mode: RetrievalMode
    max_measurable_k: int | None
    use_default_embedder: bool
    query_params: Mapping[str, Any]
    evidence_recall_k: tuple[int, ...]
    document_metrics: tuple[str, ...]
    corpus: Mapping[str, str] | None
    gold: Mapping[str, str] | None
    decision_rule: DecisionRule | None      # None means no better-than claim
    consumed_paths: frozenset[str]
    carried_paths: frozenset[str]
```

`config_sha256` is here because S4 must pre-derive the run identity from the
resolved config before staging, and the sidecar requires it.

Every rejection carries a code, including the four the first revision left
unassigned:

| Rejection | Code |
| --- | --- |
| `vector_only` named (via the removed-key rule) | `config_invalid_value` |
| decision rule names a metric `emits()` rejects | `config_invalid_value` |
| `projection_name` absent for `search_projected_text` | `config_missing_key` |
| gold/corpus absent while metrics declared | `config_missing_key` |
| `diagnostic` declaring `evidence_recall_k` | `config_inapplicable_knob` |

## S0 lock amendments carried by this slice

Reviewed here rather than slipped in:

1. Six new `BlockerCode` members for the rejection classes above, mirrored into
   the `blocker` enum of `earp.result.v1.schema.json`.
2. `earp.config.v1.schema.json`: remove `scenario.query.mode`; add
   `scenario.query.projection_name`; add `minimum`/`maximum` to `alpha`.
3. `dev/design/earp.md`: correct the example config's missing `schema_version`
   and its non-existent metric name (`evidence_recall_strict@10` matches no
   field; the result schema spells it `strict_evidence_recall`), and update
   "the twelve blocker codes" — `models.py` already declares 14, and these six
   make 20.

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

### Round 2 — revision 2, verdict REWORK (narrow)

11 of 18 findings fully resolved; the structural pieces that made revision 1
unimplementable were sound. Three things forced a third revision, the first of
which reaches past this design.

| Severity | Finding | Resolution |
| --- | --- | --- |
| BLOCKER | `CALL_MODE` encoded a **false statement about the SDK**, inherited from D-5: `search_projected_text` truncates at 10 via a reader `break` 35 lines below the SQL whose missing `LIMIT` the ruling relied on. A config could resolve cleanly and emit three copies of the @10 number | Verified in code; D-5's table corrected in the decisions record and flagged for HITL. Derivation now yields `(mode, max_k)` with this call capped |
| BLOCKER | `Engine.search` is hybrid only with an embedder; with none the vector branch is skipped and the run is pure FTS, so the sidecar would record a mode the run did not use | Mode derived from `(call, use_default_embedder)` |
| BLOCKER | The two consumption rules collided on `comparison.*` — owned by a later slice *and* predicate-false | Precedence stated: inexpressible-campaign refusal outranks carrying |
| MAJOR | Budget was declared removed and then reintroduced as a live predicate with no decidable input | Predicate deleted; `budget.estimated_usd` registered to S9 and carried |
| MAJOR | The second introspection keyed on **keyword-only** parameters, which covers none of `filter`, `name`, or `search_expand.depth`, and yields the empty set for `search_text_only` | Rebased on all parameters minus `{self, engine, query}`; the four `search_expand` filters get rows; the test skips visibly without the native binding |
| MAJOR | `@k` grammar had no slot for `abstention_rate`, `mrr`, `ndcg`; `mrr` has no implementation anywhere | Per-name `@k` required/forbidden table; `mrr` is `emits() == False` |
| MAJOR | Four acceptance criteria had no blocker code; `ResolvedScenario` was a prose list missing `config_sha256` | Codes assigned; the dataclass specified with field types |
| MINOR | Walker subset omitted `$schema`/`$id`/`title`/`description`; bool-as-integer and NaN-vs-bounds unspecified | All stated |
| MINOR | AC-4's `vector_only` clause unreachable once the key is removed; `RetrievalMode.VECTOR_ONLY` has three live consumers | Named legacy-key rule; retention stated with its consumers |
| MINOR | `diagnostic` + `evidence_recall_k` ambiguous; `config_unused_key` misnamed for its new semantics; `earp.md`'s "twelve blocker codes" stale | MUST NOT; class redefined; amendment extended |

### Round 1 — revision 1, verdict REWORK

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
