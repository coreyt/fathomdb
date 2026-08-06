---
status: PROPOSED
---

# EARP Slice 2 — metric port with pinned parity

Design of record for S2 of `dev/plans/earp-foundation.md`. Independent of S1;
both depend only on S0.

## Contract

S2 ports the IR-B evidence-recall metrics into Python and holds the port to the
Rust reference. Pure: no SDK, no database, no network, no gold file on disk.

This is the component most likely to be *silently wrong*. It has no
dependencies, so it is testable in isolation, which is why the revised plan
promoted it ahead of the runner instead of burying it inside one.

The word is **port**, not reuse. IR-B lives in
`fathomdb-engine/tests/support/ir_eval.rs` — a Rust `tests/support` module, not
a library crate, with no PyO3 surface. Python already carries at least six
ad-hoc forks of recall/MRR/nDCG; EARP exists partly to stop that spread, so the
port is held to an explicit parity contract rather than to good intentions.

## What is ported

| Reference | Python | Semantics |
| --- | --- | --- |
| `evidence_recall_at_k` (`ir_eval.rs:385`) | `evidence_recall_at_k` | strict all-of, graded fraction, both over the `required`-only denominator |
| `negative_abstained` (`:409`) | `negative_abstained` | correct iff top-K is empty |
| `ClassAgg` (`:417`) | `ClassAgg` | per-class sums, `supporting_query_n` denominator |

Semantics that must survive the port exactly, each of which is a place a
plausible reimplementation would drift:

- Strict is **all-or-nothing per query**: 1.0 only when every required unit is
  in top-K. A commitment with the date but not the obligor is not actionable.
- Strict and graded share the **same `required`-only denominator**, so they are
  directly comparable. `supporting` is in neither.
- An empty required set scores **1.0**, not 0.0, for both strict and graded
  (`ir_eval.rs:396-397`). Negatives have empty denominators by construction.
- `supporting_coverage` is `None` when a query has no supporting units — never
  0.0. See below.
- `ClassAgg.supporting()` averages over **`supporting_query_n`**, not over `n`.

## The `supporting_coverage` shape

The reference returns `Option<f64>`, `None` for an empty supporting set, and
`ClassAgg` tracks `supporting_query_n` as the aggregate denominator so a
support-bearing average is not diluted by queries carrying none.
`experiment_to_json` serialises the unavailable case as `null`.

An earlier revision of the plan carved this field out of the parity assertion,
because the reference then returned `0.0` while EARP's rules require
`not_applicable`. That was resolved upstream instead, so **parity is now full
and no field is excluded**. `null` maps to `MetricStatus.NOT_APPLICABLE`;
`supporting_query_n` is carried so an empty denominator is never mistaken for a
genuine zero.

## The parity mechanism

Parity must be asserted against the reference's *own* expectations, not against
a Python re-derivation of them — a re-derivation would pass by construction and
prove nothing.

EARP cannot execute Rust from its test suite: `ir_eval.rs` is a
`tests/support` module compiled only into the engine's test target, EARP is a
Python harness that must run in the default suite with no toolchain
dependency, and building the engine to check a pure function would make a fast
test slow and fragile. Emitting a golden file from a new Rust test would mean
adding a test to the engine crate, which D-1 puts outside EARP's remit.

So parity is asserted in two layers:

1. **Mirrored reference assertions.** `ir_recall_eval.rs` is the reference's
   own committed, reviewed test suite, and its numeric expectations are the
   reference's pinned behaviour. S2 mirrors those scenarios in Python — same
   inputs, same expected numbers — with each Python test citing the Rust test
   it mirrors by name and line. If the Rust expectations change, the citation
   is the thread that leads a maintainer here.
2. **Structural parity on the committed fixture.**
   `tests/fixtures/ir_gold/synthetic_gold.json` is committed (so it survives
   into worktrees) and is exercised by the reference at
   `ir_recall_eval.rs:289-294`. S2 loads the same file through S1's typed
   loader and asserts the aggregate shape the reference produces.

This is weaker than executing the reference, and the design says so plainly
rather than claiming byte-parity it cannot demonstrate. The honest claim is:
**parity against the reference's committed expectations**. Strengthening it to
executed parity requires a golden-file emitter in the engine crate, which is
separately-scoped work and is recorded here as the upgrade path.

## Mode-aware depth

The depth rule is metric logic, so it lands here rather than in the resolver.
`MAX_MEASURABLE_K` (S0 lock) encodes it: FTS-only unbounded, vector-only and
hybrid capped at `PRODUCTION_RERANK_LIMIT` = 10.

Grounding: `final_limit` reaches only `build_vector_phase1_sql`
(`lib.rs:10834,10848`) and the vector survivor loop (`:11374`); the FTS SQL
carries no `LIMIT` (`:11120-11126`). A request for @20 or @50 on a vector or
hybrid mode is refused with `metric_not_measurable`, naming
`SEARCH_RERANK_LIMIT` and the commissioned fanout slice — never silently
scored, which would emit three copies of the K=10 number presented as a ladder.

## Eligibility

- Evidence Recall@K requires gold with required evidence.
- nDCG requires graded relevance. No gold in this repo carries it, so nDCG
  resolves to `not_applicable` with a reason — not zero, and not an error the
  caller must special-case.
- Negative queries are scored by abstention correctness and are held out of
  the recall means entirely (`ir_eval.rs:514-518`).
- An inapplicable metric is `MetricValue(status=NOT_APPLICABLE, value=None,
  reason=...)`. The dataclass makes `value is None` structurally equivalent to
  `status is NOT_APPLICABLE`, so "could not compute" is not representable as a
  zero.

## Non-goals

- No retrieval. S2 scores a supplied ranked list of doc ids; obtaining one is
  S5/S6.
- No gold loading. That is S1's `verify_gold`.
- No aggregation into a sidecar. That is S4.
- No MRR/nDCG implementation beyond eligibility resolution — the document
  metrics land with the first campaign that can legitimately emit them.

## Acceptance criteria

1. Every mirrored reference scenario produces the reference's expected numbers,
   with the mirrored Rust test cited in the Python test.
2. `supporting_coverage` is `None` for an empty supporting set and a real
   fraction otherwise; `ClassAgg` averages over `supporting_query_n`.
3. An empty required set scores 1.0 strict and 1.0 graded.
4. Negative queries are scored by abstention and excluded from recall means.
5. K > 10 is refused for vector-only and hybrid with `metric_not_measurable`,
   and admitted for FTS-only.
6. nDCG resolves to `not_applicable` with a reason on every gold set that
   exists in this repo.
7. Every path is exercised by a test that was first observed to fail.

## Dependency

The upstream `supporting_coverage` fix
(`integrate/0.8.22-eval-supporting-coverage-20260806`) is not merged to `main`.
S2's mirrored expectations follow the fixed semantics, so S2 may be written and
pass now, but it cannot be *closed* until that branch lands — otherwise the
port would be parity with a reference state that is not on the mainline.

## Review

Pending — an independent code-grounded review is required before
implementation, per the per-slice governance in the plan.
