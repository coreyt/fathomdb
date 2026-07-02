# Consolidation value-test — results + SHIP-ON/STAY-OFF decision (0.8.12 Slice 20)

> Pre-registration: `dev/design/0.8.12-coverage-probe-and-value-test.md` §B (frozen at Slice 0).
> Harness: `src/rust/crates/fathomdb-engine/tests/consolidation_value_test.rs` (`$0`, deterministic,
> LLM-free — the local recency stub; no network, no randomness). Provider under test: the OPP-2
> consolidation/recency provider landed in Slice 15 (ADR-0.8.12). **Date:** 2026-07-01.

---

## 1. What was measured (`$0` mechanism corpus)

Independent variable: consolidation **OFF** (accumulate all facts) vs **ON** (apply the recency verdict
via the deterministic stub). Corpus: 6 `(subject, works_for)` axes, each with a STALE fact (older
`t_valid`) and an UPDATED fact (newer `t_valid`), both bodies matching the query term `works`. Dependent
variables on edge-FTS retrieval:

- **Precision** = updated-fact hits / all hits for the shared query term (a returned stale contradiction
  is a precision loss).
- **Lossiness** = false-supersede rate (still-valid updated facts wrongly hidden). Target 0.

## 2. Result

| Arm | updated hits | stale hits | precision | lossiness |
|-----|:-----------:|:----------:|:---------:|:---------:|
| consolidation OFF | 6 | 6 | **0.500** | — |
| consolidation ON | 6 | 0 | **1.000** | **0** |

**Precision lift = +0.500; lossiness = 0** (all 6 updated facts retained; all 6 stale contradictions
hidden from active edge-FTS retrieval). Deterministic; reproduce with
`cargo test -p fathomdb-engine --test consolidation_value_test -- --nocapture`.

- **Latency / footprint:** consolidation is applied at **ingest** (caller's BYO-LLM verdict pass + CPU-only
  cluster assembly + a metadata/prune write); the **query path is unchanged / CPU-only** (superseded/
  invalidated edges are excluded by shadow-prune, not by a per-query filter). Added query latency ≈ 0.
  Added ingest cost = the caller's own consolidation LLM pass (BYO, caller-controlled), plus a bounded
  CPU write. (R-CON-3 footprint honesty holds.)

## 3. Decision (pre-registered §B.3 rule, applied honestly)

The pre-registered SHIP-ON rule requires a **paired-bootstrap CI lower bound > +0.04 on ≥1 powered
temporal/update class of a REAL corpus (LOCOMO multi_session/temporal), net of lossiness/latency.**

- The `$0` measurement **validates the mechanism**: consolidation removes stale contradictions from
  retrieval with a large precision lift (+0.50) and **zero lossiness** on the deterministic corpus. The
  recency semantic is correct and non-destructive (Slice-15 `consolidate_provider` 12/12).
- It is **NOT** the real-corpus at-power evidence the rule demands for **default-ON**: this is a synthetic
  mechanism corpus, not the real temporal-QA gold, and carries no bootstrap CI over a real class. The
  real-corpus at-power confirmation (LOCOMO, with a real/priced consolidation harness) is **out of the
  `$0` scope** of this slice (same discipline as the Slice-10 priced hold).

### Verdict: **STAY-OFF by default (opt-in) — default-ON gate NOT cleared; mechanism validated but not rebuild-durable.**

Per R-CON-2 and ADR-0.8.12 §2.2, the provider **ships built but default-OFF**. Two independent reasons the
default-ON gate is NOT cleared:

1. **No real-corpus at-power evidence.** The `$0` result is a synthetic mechanism corpus, not the real
   temporal-QA gold (LOCOMO) with the paired-bootstrap CI the pre-registered §B.3 rule requires for
   default-ON. That confirmation is a heavier/priced eval, out of `$0` scope (same discipline as the
   Slice-10 hold).
2. **The exclusion is not durable across a projection rebuild (named blocker).** The measured value is
   ACTIVE-retrieval-immediately-after-consolidation. Per the Slice-15 fix-2 KNOWN LIMITATION, an operator
   `rebuild_projections()` re-materialises an invalidated (non-superseded) edge's FTS/vec shadows —
   re-surfacing the stale contradiction (graph traversal still excludes it via `t_invalid > now`; FTS/vec
   do not). **DEFAULT-ON IS BLOCKED until the FTS/vec projection SQL applies the same `t_invalid` filter as
   graph traversal.** Until then even the opt-in capability carries this caveat. (`rebuild_projections` is
   operator-feature-gated and not reachable from the default-feature value-test, so this is established by
   code-path analysis + this blocker, not exercised in the test — raised by codex §9 on this slice.)

This is a legitimate pre-registered outcome (build ≠ adopt): the mechanism is positive and lossless for
active retrieval, and default-ON is correctly withheld. The negative recorded names BOTH reasons.

## 4. Follow-ups

- **[blocker-before-default-ON]** Teach the FTS/vec edge projection SQL (incl. the `rebuild_projections`
  path) the `t_invalid > now` temporal filter graph traversal already applies, so consolidation's exclusion
  is rebuild-durable. (Elevated from a soft follow-up to a named default-ON blocker by codex §9 on Slice 20.)
- Real-corpus at-power value test (LOCOMO multi_session/temporal) with a real/priced consolidation
  harness → the default-ON decision. Pairs naturally with the held EXP-COV-1 priced sweep budget.
