---
title: ADR-0.8.16-f9-importance-confidence-ranking
date: 2026-07-07
target_release: 0.8.16
desc: Opens the deferred F9 importance/confidence ranking signal as an OFF-by-default, observable, OPP-12-rankable-forward-compatible MECHANISM. Adds canonical_nodes.importance REAL (step-18, SCHEMA_VERSION 17->18) with a 3-way sentinel, integrates the existing canonical_edges.confidence into ranking, and surfaces both through the 0.8.8 PerHitExplain. Supersedes the open-condition of ADR-0.8.1-deferred-f9-confidence-importance.
status: SIGNED (HITL coreyt, 2026-07-08 — Slice-0 gate; all 5 decisions ratified)
supersedes_open_condition_of: dev/adr/ADR-0.8.1-deferred-f9-confidence-importance.md
---

# ADR-0.8.16 — F9 importance/confidence ranking (opens the deferred F9 signal)

**Status: SIGNED** (HITL coreyt, 2026-07-08 — Slice-0 gate). Ratified: (1) the §3 deferred-ADR supersession
(mechanism now / eval-tuning later) confirmed; (2) 3-way sentinel `NULL`=absent; (3) OFF-by-default
multiplicative-on-fused weighting; (4) AC candidates minted (R-F9-4 + F9 ranking-contract + ONNX-equivalence);
(5) step-18 migration (SCHEMA 17→18) authorized. Implementation (Slice 5) may proceed. This ADR opens the F9
implementation slice that `ADR-0.8.1-deferred-f9-confidence-importance.md` framed and deferred. Design
context: `dev/design/0.8.16-slice-0-f9-onnx-design.md`.

## 1. Context — why open F9 now, and the deferred-ADR gate

`ADR-0.8.1-deferred-f9-confidence-importance.md` (status `DEFERRED — 0.8.5+`) set a **3-gate open
condition** (§5): (1) R2 eval shows a temporal / knowledge-update gap between fused and
confidence-weighted retrieval; (2) ≥100 confidence-bearing edges in the test corpus; (3) HITL sign-off on
the weighting formula — with a hard "do NOT open speculatively."

Two things changed the calculus (master F-18/F-20, HITL 2026-07-07):

- **Observability (0.8.8 EXP-OBS).** `PerHitExplain` now exists and can surface a per-hit score breakdown.
  An importance/confidence weight is only useful product surface if a caller can *see it act* — that
  surface now exists.
- **OPP-12 forward-compat obligation.** Per `OPP-12-C1-converged-contract.md` Q6a, **F9's
  importance/confidence signal algebra IS the `rankable` projection-role ranking mechanism** the 0.8.20
  projection registry will graft. F9 must exist, shaped for graceful graft, by ~0.8.16 so 0.8.20's
  idempotent `configure_projections` can adopt a `rankable` role without reshape.

## 2. Decision

Open F9 at 0.8.16 as an **OFF-by-default, observable, OPP-12-`rankable`-forward-compatible MECHANISM**:

1. **Schema (step-18, SCHEMA_VERSION 17→18):** add `canonical_nodes.importance REAL`, nullable, with a
   **3-way sentinel** — `NULL` = never assigned (graceful-absent, ranks neutral); `0.0` = explicitly
   de-weighted (floor); `(0.0, 1.0]` = explicit importance. Symmetric with the existing genuine-NULL
   `canonical_edges.confidence` (from step-14). No re-embed, no vector rewrite (eu7 no-op basis).
2. **Ranking integration:** a new **opt-in, OFF-by-default** reweight (mirrors `recency_reweight_enabled:
   AtomicBool` + `apply_recency_reweight`). When enabled, node importance scales the fused RRF contribution
   and edge confidence scales the graph-arm contribution (`graph_rrf_score(edge) = confidence × 1/(K +
   bfs_rank)`, confidence NULL⇒1.0, per the deferred ADR §4.1). **The exact weighting formula is an HITL
   sign-off item** (deferred-ADR §5 gate 3).
3. **Observability (R-F9-2):** extend `PerHitExplain` (`#[non_exhaustive]`) additively with the
   importance/confidence contribution; `explain=True` surfaces it. A weighted query reorders vs unweighted
   on a fixture.
4. **Forward-compat (R-F9-4):** the design honors OPP-12 Q6a graceful-absent/idempotent-graft (mapping in
   the design package §4). `NULL`=neutral is the graceful-absent state; the reweight is opt-in and
   add/drop idempotent; the signal is a projection-addressable per-record scalar; no field on the OPP-12
   break-if-later list.

## 3. Honoring the deferred-ADR gate (R-F9-3 — "no scope beyond it")

The deferred ADR's "do NOT open speculatively" is honored, not overridden, by **narrowing scope to the
mechanism**:

- 0.8.16 ships F9 **OFF by default** and makes **no eval-quality claim.** It does not assert that
  confidence-weighting improves temporal/KU accuracy. So the original §5 gates 1 (eval-gap) and 2 (≥100
  confidence edges) — which gate an *enabled, quality-claiming* weighting — are **NOT** pre-conditions for
  landing the OFF-by-default mechanism, and are deferred to a later tuning slice (≥0.8.18 / the M-ranking
  work) that would turn it on with eval evidence.
- The independent trigger for landing the mechanism now is the **OPP-12 forward-compat obligation +
  observability**, which the original ADR did not contemplate. This ADR records that supersession of the
  §5 open-condition, HITL-ratified at the Slice-0 gate.
- Gate 3 (HITL sign-off on the weighting formula) **is** satisfied at this Slice-0 gate.

## 4. Consequences / non-goals

- **eu7 / R-GATE:** no re-embed, no vector rewrite ⇒ eu7 satisfied on a **no-op basis** (as 0.8.14 D6).
  If a future tuning slice re-embeds, eu7 re-clears CPU same-backend (policy `649a8d45`).
- **Migration is HITL-gated** (engine/schema). Slice 5 runs step-18 only after Slice-0 sign-off.
- **Non-goal:** graph-centrality importance computation (PageRank/degree) — importance is a caller/writer-
  supplied scalar at 0.8.16; auto-computed importance stays deferred (deferred-ADR §4.2).
- **Non-goal:** enabling the weight in the shipped eval / claiming a competitor delta.
- **X1 parity:** if F9 adds SDK surface (a write for importance, an explain field), it must reach Py↔TS
  parity (R-X-1); if it stays engine-internal, assert the no-new-verb like 0.8.14 R-X-1.

## 5. Open items for HITL (Slice-0 gate)

1. Ratify the 3-way sentinel encoding (`NULL`=absent recommended).
2. Ratify the OFF-by-default weighting formula (multiplicative-on-fused recommended).
3. Confirm the §3 supersession (mechanism now, tuning later).
4. Authorize the step-18 migration.
5. Mint R-F9-4 (rankable-forward) + the F9 ranking-contract AC.
