---
status: HITL-RULED
---

# EARP — HITL rulings on the four deferred decisions

Ruled 2026-08-06. These close the four items listed as "Decisions deferred to
the human" in `dev/plans/earp-foundation.md`. The **Ruling** lines are the
HITL's words of record; the **Consequences** are the implementation
consequences drawn from them, and are proposed by the implementer — they are
subject to correction, the rulings are not.

## D-1 · Placement

**Ruling.** EARP is **not an official part of FathomDB**. It is a
developer-side harness for running experiments.

**Consequences.**

- Off-ladder. EARP takes no release slot, no version anchor, and no
  `dev/plans/plan-0.8.x.md` scheduling row. It does not block or ride 0.8.22.
- It lands on the existing developer-harness convention rather than inventing
  one: `src/python/eval/` already hosts `m1_baseline.py`, `m1_ppr.py`, and
  `r2_parity_eval.py`. `pyproject.toml:80-81` records that maturin packages
  only `fathomdb`, so nothing under `eval/` reaches the wheel. EARP is
  off-wheel by construction, not by policy.
- Harness-only dependencies follow the `m1` precedent: declared in
  `[project.optional-dependencies] dev`, never linked into the library, and
  therefore footprint-invariant.
- It is still repo code: `ruff` and `pyright` (which includes `eval`,
  `pyproject.toml:103`) apply, and EARP owes them clean.

## D-2 · Gating

**Ruling.** EARP must **never** gate FathomDB. This is not expected to change
for the foreseeable future.

**Consequences.**

- No EARP metric, run outcome, verdict, or threshold may ever appear in a
  required CI check, a release gate, an AC, or a merge condition.
- Distinguish two things that are easy to conflate:
  - **An EARP *run* gating FathomDB — forbidden, permanently.**
  - **EARP's own unit tests running in CI — expected**, since it is repo code.
    Those tests assert EARP's own logic (config rejection, metric eligibility,
    writer ordering); they must never assert a retrieval-quality outcome.
- Therefore every EARP test that needs a real model load, a network fetch, or a
  priced service is opt-in and visibly SKIPs by default, following the
  established markers in `pyproject.toml:89-96` (`integration`,
  `requires_test_hooks`) and the `FDB_S15A_INTEGRATION=1` env-gate pattern. A
  skipped arm is never reported as a pass or a zero.

## D-3 · Cost authorization

**Ruling.** **$5.00 pre-authorized.** Any work beyond that budget requires
HITL approval.

**Consequences.**

- The budget is a **ceiling to enforce**, not merely a figure to record.
  Recording alone is already available — `Record.cost_usd` and the index row's
  `cost_usd` exist (`experiments/_lib.py:103,481`).
- Enforcement mechanism (proposed): a preflight sums `cost_usd` across
  `experiments/index.jsonl`, adds the declared worst-case estimate for the
  pending run, and refuses to start when the projected total exceeds the
  remaining authorization. Refusal is a typed blocker with a durable record,
  not a silent skip.
- The $5.00 is cumulative across all priced EARP runs, not per-run.
- Every priced arm remains individually opt-in per D-2, and cheap-validation
  precedes any priced execution.

## D-4 · Acceptance thresholds and better-than claims

**Ruling.** These **differ between experiments**.

**Consequences.**

- There is no global threshold table and no repo-wide "better" rule. This is a
  design change, not an annotation: the `earp.v1` config schema needs an
  optional per-campaign decision-rule block, which the current draft does not
  have.
- The rule is **predeclared in the campaign configuration before the run** and
  persisted into the resolved config and the sidecar, so a threshold can never
  be chosen after seeing the result.
- A campaign that declares no decision rule may report metrics but may not
  claim one configuration is better than another.
- This composes with, and does not relax, the existing comparison
  preconditions (paired immutable query IDs, declared strata, fixed CI method
  and seed, predeclared power conditions).

## D-5 · Metric depth (K) and evaluation fanout

**Ruling.** The K-ladder problem is real for **vector and hybrid** retrieval
only. Two nuances correct the blunt reading:

- **FTS-only results are not capped at 10**, so @20/@50 can be valid there.
- **Hybrid/vector @20/@50 are invalid** until the fanout is set and recorded;
  EARP v1 must **reject** them, not silently score them.

Rulings:

1. EARP's v1 example and default become `evidence_recall_k: [5, 10]`, with
   **mode-aware validation**.
2. A dedicated **cross-binding "evaluation fanout control" slice** is
   commissioned **after 0.8.22**: define a supported evaluation/query
   configuration, wire it Rust → Python → TypeScript, test vector/hybrid
   results beyond 10, bound and record the fanout, and document its
   latency/semantic implications.
3. **Do not simply export `set_search_limit_for_test`.** It is explicitly a
   hidden test seam, not a stable SDK contract.

**Code grounding (verified).** `final_limit` reaches only the vector path —
`build_vector_phase1_sql` (`lib.rs:10834,10848`) and the survivor loop
(`:11374`); production floor `SEARCH_RERANK_LIMIT = 10` (`:9851`), raisable
only via the test seam (`:8105`), which has no PyO3 binding. The property-FTS
SQL (`:11120-11126`) carries **no `LIMIT`**. So depth-validation is per-mode:

| Mode | @5 / @10 | @20 / @50 |
| --- | --- | --- |
| `search_text_only` (node FTS) | valid | **valid** |
| `search_projected_text` (property FTS) | valid | **reject** — see correction below |
| vector-only | valid | **reject** — typed `metric_not_measurable` |
| hybrid (`search`) | valid | **reject** — typed `metric_not_measurable` |

### Correction (2026-08-06) — `search_projected_text` IS capped

**This refines D-5's ruling in the stricter direction and needs HITL
confirmation.** The original table grouped both FTS verbs as uncapped, on the
reading that "the property-FTS SQL carries no `LIMIT`". The SQL indeed carries
none — but the cap is a Rust `break` 35 lines below it, which that reading
missed:

```text
lib.rs:6427-6432   search_projected_text computes
                   limit = search_limit_override.max(SEARCH_RERANK_LIMIT)  // 10
lib.rs:11161-11163 the projected-text reader: if results.len() >= limit { break; }
```

`search_limit_override` initialises to `SEARCH_RERANK_LIMIT` (`lib.rs:1301`) and
is raisable only through `set_search_limit_for_test` (`:8113`), which D-5.3
forbids exporting. So `search_projected_text` truncates at 10.

`search_text_only` is genuinely different and the ruling holds for it: its
`search_limit` bounds only the vector branch (`:11374`) and the explain trace
(`:11877`), and the node-FTS SQL takes a `LIMIT` only under
`FATHOMDB_PERF_EXPERIMENTS` (`:11457,11471`), which is off by default.

Consequence: EARP refuses @20/@50 for `search_projected_text` as well. The
ruling's *intent* — never silently score a depth the engine cannot deliver — is
preserved and applied to one more verb, so proceeding on this reading refuses
more rather than less. Flagged for HITL confirmation rather than treated as
settled.

### Correction (2026-08-06) — mode depends on the embedder, not the call alone

`Engine.search` is hybrid only when an embedder is configured.
`use_default_embedder` defaults to `False` (`engine.py:143`), and with no
embedder `query_vector` is `None`, the vector branch is skipped, and the run is
pure node FTS — the same path `search_text_only` takes (`lib.rs:6377`). Two
consequences: the sidecar would record `retrieval_mode: hybrid` for a run that
was FTS-only, and @20/@50 would be refused for a configuration that could
measure them honestly. EARP therefore derives the mode from
`(call, use_default_embedder)`, not from the call alone.

The rejection error names `SEARCH_RERANK_LIMIT` and D-5.2 as the unblocking
work. Until that slice lands, EARP records the fanout it used (10) with every
number, per IR-B §(c).

## D-6 · Gold basis for v1 — IR-C reuse tier, with conditions

**Ruling.** `all.gold.json` is a **usable IR-C reuse-tier GoldSet**: its corpus
hash exactly matches `tests/corpus/snapshot.json`; 4,597 queries (2,888
exact-fact, 1,584 exploratory, 125 negative); dataset-authored
evidence-document pointers converted to `required_evidence`. Use it for
**bounded corpus-scale Evidence Recall and abstention results**, under four
conditions:

1. **Pin** `data/corpus-data/eval/ir_gold/all.gold.json` by SHA-256, and
   require its `corpus_hash` to equal the frozen snapshot hash.
2. **Record both identities**: the snapshot is the GoldSet's identity;
   `manifest.json` remains raw-corpus provenance. They are not
   interchangeable.
3. **Regenerate/validate first.** The cached files are `ir-c-reused-v1`; the
   committed generator now emits `ir-c-reused-v2` with provenance/spans.
   **Do not silently use the stale cache.**
4. **Keep claims scoped.** This is reuse-tier, document/body-level evidence
   gold — *not* fresh FathomDB-specific human adjudication. It has no
   supporting-evidence rows, and hybrid/vector @20/@50 still require the D-5.2
   fanout slice.

**Consequence for the plan.** Real EARP quality campaigns are **not** blocked
on producing gold from scratch. They are blocked only on (a) validating or
regenerating this local gold to v2, and (b) for the full K ladder, the fanout
control. This supersedes the "one human-authored document-level fixture" v1
scope in `dev/plans/earp-foundation.md:12-13` as the *campaign* basis; a small
fixture may still back the fast, network-free unit tests.

**Verified (2026-08-06).** `build_ir_gold.py:46` sets
`QRELS_VERSION = "ir-c-reused-v2"`, and every on-disk gold file reports
`ir-c-reused-v1` — **stale, exactly as ruled**. `necessity: "required"` is the
generator's only emission site (`:132`), so v2 adds no supporting rows.

**Correction to an earlier gloss.** This note previously justified condition 3
as preventing a *silent metric change*, on the grounds that v2 emits span
locators. That justification is **false against the data**. The generator emits
`locator.kind = "span"` only when a source row supplies `evidence_spans`
(`:106-127`), and `evidence_spans` is non-empty on **zero of 4,597 source
rows**:

```text
enronqa_qa.jsonl  rows= 710  has_key= 710  non_empty_spans=0
qaconv_qa.jsonl   rows=2303  has_key=2303  non_empty_spans=0
qmsum_qa.jsonl    rows=1584  has_key=1584  non_empty_spans=0
```

So regenerating to v2 changes only the `qrels_version` string, the tracer key
renames `_source`/`_answer_type` → `source`/`answer_type`, and a new
`query_origin: "human_dataset"` — a value the Rust reference already defaults
to when absent (`ir_eval.rs:292-296`). No span locator appears and no metric
moves.

**The ruling stands; only the reason changes.** Condition 3 is correct as
**provenance and version hygiene**: a gold file must declare the version its
committed generator actually emits, so the identity recorded with every number
cannot name a version no code produces. It is now also known to be *cheap* to
satisfy — the content is semantically identical, so regeneration carries no
metric consequence and no re-baselining.

## Correction · the gold set exists

An earlier statement in this session that no gold set exists was wrong. It is
gitignored (`.gitignore:9`), so it is absent from worktrees but present in the
primary checkout:

```text
data/corpus-data/eval/ir_gold/all.gold.json    2.8 MB  (2026-06-10)
                             enronqa|qaconv|qmsum.gold.json
```

`all.gold.json` carries `corpus_hash: fe973fcd49fbbda0…` (matching
`tests/corpus/snapshot.json`) and `qrels_version: ir-c-reused-v1`, and its
schema matches IR-B §(b) exactly. Class counts: `exact_fact` 2888,
`exploratory` 1584, `negative` 125.

Consequences:

- The design's cited path `data/corpus-data/eval/ir-c.gold.json` is
  **fabricated**; the real path is `.../eval/ir_gold/all.gold.json`.
- `"necessity": "supporting"` occurs **0 times**. The "supporting evidence
  remains separate" distinction is an empty bucket in v1, and no graded
  relevance exists anywhere — so `ndcg` is `not_applicable`, not merely
  ineligible-in-principle.
- **Operational constraint:** because the data is gitignored, an EARP run
  launched from a worktree cannot see it. The corpus/gold path must be
  configurable and resolved against an explicit root, with a typed blocker
  when absent — never a silent empty gold set.

## Finding · IR-B §(e) is stale and must not be ported forward

`dev/design/ir-recall-measure.md` §(e) describes the production FTS branch as
`ORDER BY write_cursor` (insertion order) with bm25 as a score only, and
treats a bm25-ranked baseline as harness-constructible. `lib.rs:11463-11467`
records that IR-C (2026-06-10) changed the text branch to
`ORDER BY bm25(...) ASC`, calling write-cursor fusion "the single biggest
fusion bug"; the SQL at `:11120-11126` confirms it. EARP must port the
**current** behaviour, and the staleness should be fixed in the IR-B document
separately.

## D-7 · `supporting_coverage` — resolved upstream, not by divergence

**Ruling.** Resolved in the Rust reference rather than by an EARP-side
divergence. Branch `integrate/0.8.22-eval-supporting-coverage-20260806`
(`19765415`, `4d478daa`).

**What changed.** `PerQueryRecall.supporting_coverage` becomes `Option<f64>` —
`None` when the query has no supporting units, "so an inapplicable diagnostic
is never represented as a failed one". Beyond the empty-set case, the
**aggregate denominator was also wrong**: `ClassAgg::supporting()` divided
`supporting_sum` by `n` (all queries), diluting the average with queries that
had no supporting evidence at all. It now tracks `supporting_query_n` and
averages over support-bearing queries only, returning `Option<f64>`.
`experiment_to_json` emits `null` for the unavailable case and carries
`supporting_query_n` alongside each value.

**Consequences for EARP.**

- The planned deliberate divergence is **withdrawn**. A faithful port is now
  the correct port; the parity test asserts every field with **no exclusions**.
- `null` maps to `not_applicable`; `supporting_query_n` is carried into the
  sidecar so a reader can distinguish an empty denominator from a real zero.
- **Dependency:** the fix is not merged to `main` (the branch is 114 commits
  ahead). S2's parity test is written against it and cannot close until it
  lands.

## Cross-cutting note

D-1 and D-2 together mean EARP's failure mode of concern is **not** "EARP
breaks the build" — it is **EARP producing a confident number that is not
true**. Nothing downstream will catch a wrong EARP result, because by ruling
nothing downstream depends on it. That places the whole integrity burden on
EARP's own evidence discipline: typed blockers, metric eligibility, honest
skips, and pinned corpus/gold identities.
