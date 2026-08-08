---
status: CURRENT
---

# EARP — current HITL rulings

This note records the current decisions governing EARP. It is intentionally
symbol-based: implementation detail and line numbers belong in the code and
tests, not in a decision record that must survive unrelated edits.

## D-1 · Placement

EARP is developer-side experiment tooling, not a production FathomDB feature.
It lives under `src/python/eval/earp/`, is EVAL-ONLY and off-wheel, takes no
release slice, and does not change an SDK, engine, schema, or CI gate.

## D-2 · Gating

An EARP run, score, threshold, or verdict must never gate FathomDB. EARP unit
tests do run as repository tests: they verify harness behavior, not a
retrieval-quality outcome. Network, real-model, and priced paths are opt-in
and visibly skipped by default.

## D-3 · Cost authorization

$5.00 is cumulatively pre-authorized for priced EARP work. The guard sums
previous recorded spend with the run's declared worst-case estimate and returns
a typed blocker before exceeding that ceiling. Each priced arm remains
individually opt-in; anything above the ceiling requires a new HITL ruling.

## D-4 · Claims and decision rules

There is no global quality threshold. A campaign that makes a better-than claim
must declare its metric, direction, threshold, paired comparison conditions,
confidence-interval method, seed, and power conditions before the run. A
campaign without that declared rule may report metrics but may not claim one
configuration is better than another.

## D-5 · Result depth and public limits

The pre-0.8.22 fanout discussion is retired. The current cross-SDK contract is
the public result limit: every EARP search mode may measure `@K` exactly when
`K <= limit <= 100`, with a default limit of 10. The resolved limit is recorded
with every metric; a deeper rung is a typed `metric_not_measurable` blocker.
The hidden `set_search_limit_for_test` seam remains unexported.

## D-6 · Gold basis

The IR-C reuse-tier gold at
`data/corpus-data/eval/ir_gold/all.gold.json` is a valid v1 basis only after
its SHA-256 and `corpus_hash` are checked against the frozen snapshot. EARP
requires the generator's `ir-c-reused-v2` identity and refuses stale v1 gold.
Claims remain scoped to reuse-tier, document/body-level evidence; EARP never
manufactures gold or human labels.

The corpus and gold are gitignored inputs. A missing configured root is a typed
blocker, never an empty data set. The real gold has no supporting evidence rows
and no graded relevance; EARP reports supporting coverage and nDCG as
`not_applicable`, never as zero.

## D-7 · Supporting coverage

The Rust reference resolves empty supporting sets as `None` and aggregates only
over supporting-bearing queries. EARP ports that behavior without divergence:
`null` becomes `not_applicable`, and the sidecar carries
`supporting_query_n` so an empty denominator cannot be mistaken for a zero.

## Integrity consequence

Because EARP does not gate FathomDB, its important failure mode is a confident
number that is not true. The countermeasures are strict configuration, pinned
corpus and gold identities, typed blockers, metric eligibility, durable
sidecars before an index entry, and predeclared comparison rules.
