---
title: ADR-0.6.0-provenance-retention
date: 2026-04-27
target_release: 0.6.0
desc: Provenance event table retention — operator-configurable row-count cap, oldest-first eviction
blast_radius: acceptance.md AC-033; design/engine.md provenance section; CLI / admin.configure provenance knob; requirements.md REQ-031
status: accepted
---

# ADR-0.6.0 — Provenance retention

**Status:** accepted (HITL 2026-04-27, decision-recording — promoted from
FU-AC-PROTOCOL-BACKFILL).

Phase 3b-promoted acceptance ADR. Owns the retention bound + eviction
shape that AC-033 gates on. Resolves REQ-031's "configure or trigger
retention" with a concrete shape so AC-033 is testable.

## Context

REQ-031 requires bounded provenance growth without prescribing the
retention shape. Three shapes are common:

- **Row-count cap.** Bound rows; evict oldest when exceeded.
- **TTL.** Evict rows older than wall-clock duration D.
- **Hybrid.** Cap + TTL, whichever fires first.

Cap is the simplest operationally (no clock dependence; deterministic
behavior under heavy write bursts) and matches the engine-internal
single-writer model (eviction runs on the writer thread alongside
inserts).

## Decision

**Operator-configurable row-count cap on provenance event tables.**

- **Default cap:** 1,000,000 rows. Operator may override via
  `admin.configure` (typed config, not raw SQL — per
  ADR-0.6.0-typed-write-boundary).
- **Eviction policy:** oldest-first by primary key (monotonic insert
  order). Eviction runs on the writer thread when a write would push
  row count past `cap × (1 + slack)`; eviction batch size deletes
  rows back down to `cap`.
- **Slack:** 5% (bound is enforced as `≤ cap × 1.05` between
  eviction batches; permits batched deletes without per-row eviction
  overhead).
- **Disable:** `cap = None` (or operator-equivalent) opts out of
  retention. Engine then makes no claim of bounded growth — operator
  owns the consequence.

## Options considered

**A — Row-count cap, oldest-first, operator-configurable, 1M default
(chosen).** Simple; deterministic; matches single-writer model; default
covers small-to-mid deployments without operator intervention.

**B — TTL.** Requires reliable wall-clock; varies with traffic
(quiet periods retain ancient rows; bursts evict recent ones); harder
to test against (AC-033 would need clock manipulation).

**C — Hybrid (cap + TTL).** Two knobs; speculative; rejected for
Stop-doing on speculative knobs.

**D — Engine-managed bound, no operator config.** Removes the knob;
violates REQ-031 "operator can configure or trigger."

## Consequences

- AC-033 cites this ADR for the 1M default + 5% slack.
- `admin.configure` typed config gains a `provenance_retention_cap`
  field (Option<u64>); shape owned by `interfaces/python.md` etc.
- `design/engine.md` provenance section documents writer-thread
  eviction batch logic.
- Eviction is a write — counts toward write-throughput SLI
  (ADR-0.6.0-write-throughput-sli) only by its actual cost (small,
  amortized across inserts).
- `safe_export` of provenance rows: scope owned by FU-OPS2 deferral
  (0.8.0); not gated by this ADR.
- TTL alternative deferred to followup; revisit if a real operator
  forcing function lands.

## Citations

- HITL 2026-04-27.
- REQ-031 (parent requirement); AC-033 (forcing function).
- ADR-0.6.0-typed-write-boundary (config is typed, not SQL).
- ADR-0.6.0-single-writer-thread (writer-thread eviction).
