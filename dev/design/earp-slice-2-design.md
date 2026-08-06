---
status: PROPOSED
---

# EARP Slice 2 — metric port with pinned parity

Design of record for S2 of `dev/plans/earp-foundation.md`. **Revision 2** —
amended after the independent code-grounded review recorded in § Review.

Depends on S0 (types) and on S1's `GoldQuery`/`EvidenceUnit` **types**, but not
on S1's `verify_gold` loader. The earlier claim of full independence from S1
was wrong and is corrected here.

## Contract

S2 ports the IR-B evidence-recall metrics into Python and holds the port to
values the Rust reference actually produced. Pure: no SDK, no database, no
network.

This is the component most likely to be *silently wrong*. It has no runtime
dependencies, so it is testable in isolation — which is why the revised plan
promoted it ahead of the runner instead of burying it inside one.

The word is **port**, not reuse. IR-B lives in
`fathomdb-engine/tests/support/ir_eval.rs` — a `tests/support` module, not a
library crate, with no PyO3 surface. Python already carries at least six ad-hoc
forks of recall/MRR/nDCG; the port is therefore held to an executable parity
contract rather than to good intentions.

Citations below are by **symbol name**, pinned to reference commit `19765415`.
Line numbers are deliberately omitted: the pre-fix and post-fix files differ by
one to five lines throughout, and a citation that dangles after a merge is a
defect in a document whose whole purpose is traceability.

## What is ported

| Reference symbol | Ported | Why it matters |
| --- | --- | --- |
| `evidence_recall_at_k` | yes | strict all-of, graded fraction, shared `required`-only denominator |
| `required_doc_ids` / `supporting_doc_ids` | yes | Collapses to a **set of `doc_id`**, and falls back to `expected_top_k_doc_ids` only when `required_evidence` is *entirely* empty — never added on top (§(f)). The most drift-prone rule in the module. |
| `negative_abstained` | yes | correct iff top-K is empty |
| `ClassAgg` | yes, as a pure fold | `supporting_query_n` denominator; the `n == 0` asymmetry |
| `NegativeAgg` / `false_positive_rate` | yes | AC-4's aggregate; abstention is not recall |
| `KResult` | yes | the per-K container S4's sidecar shape derives from |
| `evaluate_gold_set` | yes | the K × class × negative loop, **and its `Err`-is-never-scored contract** |
| `K_LADDER` / `HEADLINE_K` | yes | drives the ladder; headline is @10 |
| `round4` | yes, explicitly | see the rounding note below |
| `validate_gold_set` methodology invariants | yes — **S2 owns them** | see the vacuous-`1.0` section |
| `experiment_to_json` field names | not ported; **matched** by S4 | `strict_evidence_recall`, `graded_evidence_recall`, `supporting_coverage`, `supporting_query_n`, `n`, `false_positive_rate` |
| `run_experiment` / `run_mode_bodies` | **not ported** | requires a live `Engine`; that is S5/S6 |
| `is_runnable_now` / `deferred_modes` | **not ported here**; S5 owns | the reference *records* requested-but-deferred modes rather than dropping them, and S5 must too |

**Rounding is a real divergence risk.** `round4` uses Rust `f64::round`, which
is half-away-from-zero; Python's built-in `round` is banker's rounding. At a
`.00005` boundary they differ. The port implements half-away-from-zero
explicitly and pins it with a boundary case.

Semantics that must survive exactly:

- Strict is **all-or-nothing per query**; graded is the fraction. Both use the
  **same `required`-only denominator**, so they are directly comparable.
  `supporting` is in neither.
- `supporting_coverage` is `None` for an empty supporting set and `Some(0.0)`
  when supporting units exist but none were retrieved. Those are different
  facts and the distinction is the whole point of the upstream fix.
- `ClassAgg.supporting()` averages over `supporting_query_n`; `strict()` and
  `graded()` return `0.0` when `n == 0` while `supporting()` returns `None`.
  That asymmetry is deliberate and is ported as-is, not "cleaned up".
- A failed retrieval is **never** folded into an empty result set and scored as
  misses or as a correct abstention. This is a codex §9 [P2] fix in the
  reference and re-introducing it would defeat EARP's own blocker vocabulary.

## The vacuous 1.0, and who owns the guard

`evidence_recall_at_k` returns `1.0` for both strict and graded when the
required set is empty. The reference documents why that is safe: the aggregator
routes negatives away from recall, and `validate_gold_set` refuses a
non-negative query with an empty denominator — so it only bites mislabeled data
the validator catches.

**EARP had neither guard and no slice owned them.** S1 validates hashes,
vocabularies, corpus equality, and version; it ports none of
`validate_gold_set`'s methodology invariants. Left as designed, any mislabeled
query in the 4,597-query basis would score a silent perfect 1.0 and inflate
both means — precisely the confident false number this platform exists to
avoid.

**S2 owns the methodology validator.** It is metric methodology, not file
integrity, so it belongs beside the metric rather than beside the hash pin.
Ported invariants: class/denominator coherence, duplicate `query_id`, duplicate
`evidence_id`, and span bounds.

S2 adds one invariant the reference does not test: `required_evidence`
non-empty but containing **only** `supporting` units. The legacy fallback fires
only when `required_evidence` is entirely empty, so such a query gets an empty
denominator *and* no fallback — a vacuous 1.0 with no guard. It does not bite
today (the generator emits only `required`) but it bites the moment supporting
rows are added, which is the stated direction. It is refused.

## The parity mechanism

Parity is asserted against **values the reference actually produced**, not
against a Python re-derivation, which would pass by construction.

EARP cannot execute Rust in its own suite: `ir_eval.rs` compiles only into the
engine's test target, and EARP must run in the default Python suite with no
toolchain dependency. D-1 forbids EARP *committing* a test to the engine crate
— it does not forbid running the reference once locally and committing the
result. So:

1. **A hash-pinned expectations file**,
   `src/python/tests/earp/fixtures/ir_parity_vectors.json`, generated by
   running the reference over a fixed case list and committing only the output.
   Each case carries the gold query, the retrieved list, `k`, and the
   reference's own `strict`, `graded`, `supporting_coverage`, `required_n`,
   `required_hits`, plus `negative_abstained` and `ClassAgg` outputs.
2. **A reference drift detector.** A SHA-256 over `ir_eval.rs`'s bytes, pinned
   in the Python suite, failing when the reference changes. Prose citation is
   not a drift detector; a hash is.

The case list deliberately covers six behaviours the reference's own tests do
**not**, each a plausible reimplementation choice that would otherwise pass
unnoticed:

| Blind spot | What a wrong port would do |
| --- | --- |
| Two required units on one `doc_id` | count a denominator of 2 instead of 1 |
| Duplicate `doc_id` in the retrieved list | dedupe before the K cut, freeing a rank slot |
| A doc that is both required and supporting | count it once instead of in both sets |
| `ClassAgg` with `n == 0` | return `None` uniformly instead of `0.0`/`0.0`/`None` |
| A `.00005` rounding boundary | use banker's rounding |
| `required_evidence` with only supporting units | silently score a vacuous 1.0 |

An earlier revision proposed loading
`tests/fixtures/ir_gold/synthetic_gold.json` through S1's loader as a second
layer. That is dropped. The reference computes **no metrics** on that fixture —
it asserts only loader facts — so there was nothing to mirror. Worse, the
fixture carries `corpus_hash: "TODO(COR-2-freeze)"`, so getting it past
`verify_gold` would require fabricating a snapshot asserting that the
deliberately-unpinned placeholder is a valid corpus hash, inverting the exact
refusal S1 exists to perform.

## Depth is not a metric property

The depth rule moves **out** of the metric layer. `evidence_recall_at_k` in the
reference takes any `k` and never refuses; a version with an extra refusal arm
is not the reference's function, so parity could not be asserted on it.

The cap is an **SDK-surface** fact, not a metric fact: the reference measures
vector @50 perfectly well by raising the fanout via `set_search_limit_for_test`
(`DEFAULT_FANOUT = 50`), and what blocks EARP is that this seam has no PyO3
binding and D-5.3 forbids exporting it.

So the rule lives once, in `earp.depth.check_depth(mode, k) -> Blocker | None`,
reading `MAX_MEASURABLE_K` from the S0 lock. S3 calls it at config validation
and the runner calls it before retrieval. A pure metric function receiving a
ranked list cannot know how that list was obtained, so a check inside it would
be advisory at best and duplicated at worst.

EARP's three-member `RetrievalMode` is **not** the reference's five-member
vocabulary; the mapping is stated in the module and carried in the sidecar.

## Refusal types

Two refusals, two shapes, stated so an implementer does not invent a third:

- **Unmeasurable depth** → `Blocker(code=METRIC_NOT_MEASURABLE)`, **returned**
  from `check_depth`, never raised, and never from inside a metric function.
- **Ineligible metric** → `MetricValue(status=NOT_APPLICABLE, value=None,
  reason=...)`, returned from eligibility resolution.

## S0 lock amendments carried by this slice

Reviewed here rather than slipped in during implementation:

1. `MetricValue` gains a `__post_init__` enforcing `(value is None) == (status
   is NOT_APPLICABLE)` and requiring a `reason` when not applicable. The
   invariant is currently prose in a docstring, and `MetricValue(status=EMITTED)`
   constructs happily today. This matters precisely because
   `MetricValue(EMITTED, 0.0)` is a *legitimate* state.
2. A `QueryClass` enum is added, with the reference's discriminant ordering
   (`commitment, action, exact_fact, preference, exploratory, negative`) since
   the reference's `BTreeMap` iterates in that order, not alphabetically. S1
   currently uses a bare `frozenset` of strings, which contradicts the lock's
   own closed-vocabulary doctrine.
3. Integer denominators (`required_n`, `required_hits`, `supporting_query_n`,
   `n`) do **not** live in `MetricValue`, whose `value` is `float | None`. They
   live in the `KResult`/`ClassAgg` aggregates.

## Non-goals

- No retrieval. S2 scores a supplied ranked list; obtaining one is S5/S6.
- No gold loading or hash pinning. That is S1.
- No sidecar assembly. That is S4.
- No MRR/nDCG implementation beyond eligibility resolution.

## Acceptance criteria

1. Every case in the hash-pinned expectations file reproduces the reference's
   value exactly, including all six blind-spot cases.
2. The reference drift detector fails when `ir_eval.rs` changes.
3. `supporting_coverage` is `None` for an empty supporting set and `Some(0.0)`
   when supporting units exist but none were retrieved.
4. `ClassAgg` averages supporting over `supporting_query_n`, and at `n == 0`
   returns `0.0`/`0.0`/`None`.
5. A non-negative query with an empty required denominator is **refused** by
   the methodology validator, so the vacuous 1.0 is unreachable on validated
   gold. A `required_evidence` list containing only supporting units is
   likewise refused.
6. A failed retrieval is surfaced as a typed outcome, never scored as misses or
   as a correct abstention.
7. K > 10 is refused for vector-only and hybrid by `check_depth`, and admitted
   for FTS-only.
8. nDCG resolves to `not_applicable` with a reason.
9. Rounding is half-away-from-zero, pinned at a boundary case.
10. Every path is exercised by a test that was first observed to fail.

## Dependency

The upstream `supporting_coverage` fix
(`integrate/0.8.22-eval-supporting-coverage-20260806`) is not merged to `main`.
The expectations file is generated against it, so S2 may be written and pass
now but cannot be **closed** until that branch lands — otherwise the port is
parity with a reference state that is not on the mainline. The drift detector
makes the skew visible rather than silent.

## Review

Independent code-grounded review, 2026-08-06. Verdict: **proceed with specified
revisions**, with an explicit warning that rejecting the expectations file or
leaving the depth rule inside the metric would make it rework. Both are
adopted. All findings resolved.

| # | Severity | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | BLOCKER | The vacuous `1.0` was promoted to required behaviour while the guards that make it safe were owned by nobody — S1 ports none of `validate_gold_set` | S2 owns the methodology validator; a new invariant refuses supporting-only evidence lists |
| 2 | BLOCKER | The ported surface omitted `required_doc_ids`' fallback rule, `evaluate_gold_set`'s `Err`-never-scored contract, `NegativeAgg`, `KResult`, the ladder constants, and `round4` | Table expanded with an explicit ported / not-ported verdict per symbol |
| 3 | MAJOR | The fixture parity layer was empty — the reference computes no metrics on `synthetic_gold.json`, and the cited lines were wrong in both file versions | Layer dropped; the placeholder-hash trap recorded as a second reason |
| 4 | MAJOR | The mirror was real but had six named blind spots a buggy port would pass | Replaced with an executed, hash-pinned expectations file covering all six, plus a reference drift detector |
| 5 | MAJOR | The depth rule does not belong in the metric and voided parity on the function it sat next to | Extracted to `earp.depth.check_depth`, called by S3 and the runner |
| 6 | MAJOR | S2 could not load the fixture through S1's loader without asserting an unpinned placeholder is a valid corpus hash | Layer dropped |
| 7 | MAJOR | `MetricValue`'s invariant was prose, and the type cannot carry integer denominators | `__post_init__` amendment; denominators live in the aggregates |
| 8 | MINOR | Every citation was to the pre-fix line numbering | Cite by symbol, pinned to commit `19765415` |
| 9 | MINOR | `ClassAgg` is a mutating accumulator in an all-frozen codebase | Ported as a pure fold |
| 10 | MINOR | Two refusal types, unassigned | Stated: `Blocker` returned from `check_depth`; `MetricValue` from eligibility |
| 11 | MINOR | Self-contradiction on the S1 dependency | Corrected: depends on S1's types, not its loader |
| 12 | MINOR | Per-class key type and ordering uninvented | `QueryClass` enum added with the reference's discriminant ordering |

Confirmed correct by the review, with no change required: the
`supporting_coverage` semantics as described; strict/graded/shared-denominator;
`negative_abstained`; negatives excluded from recall means; the depth
*constants* (only their placement was wrong); nDCG → `not_applicable`.
