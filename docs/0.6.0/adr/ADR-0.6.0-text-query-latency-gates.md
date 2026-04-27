---
title: ADR-0.6.0-text-query-latency-gates
date: 2026-04-27
target_release: 0.6.0
desc: FTS5 text-query p50/p99 end-to-end latency gates at 1M-row scale
blast_radius: test-plan.md perf-test suite; CI perf gate; design/retrieval.md FTS5 tuning section; sqlite FTS5 usage; requirements.md REQ-010
status: accepted
---

# ADR-0.6.0 — Text-query latency gates

**Status:** accepted (HITL 2026-04-27, decision-recording — promoted from
FU-REQ-010-TEXT-LATENCY).

Phase 3a-promoted acceptance ADR. Parallel to
ADR-0.6.0-retrieval-latency-gates; closes REQ-010 with an authoritative
numerical anchor.

## Context

Phase 3a critic [REQ-010] flagged that the harvested
`production-acceptance-bar.md` value of `p95 ≤ 150 ms` for text query had
no accepted ADR backing, no workload definition, no scale, and no
percentile model parallel to the vector retrieval ADR. Requirements lock
blocked on a real text-query latency anchor.

Two paths to resolution were available:
1. Promote to its own ADR mirroring `ADR-0.6.0-retrieval-latency-gates`.
2. Fold under canonical-read freshness (REQ-013) with a generous
   acceptance.md bound.

Path 1 chosen. Text query is a load-bearing user surface (REQ-053 lists
`search` among the five-verb application API; FTS5 is the engine
underneath text mode). It deserves the same gate-and-fixture treatment
vector retrieval gets, not a hand-waved acceptance bound.

## Decision

- **p50 ≤ 20 ms; p99 ≤ 150 ms.** (p99 set at 150 ms rather than 100 ms
  to absorb shared-runner scheduler jitter on tier-1 CI; tighten to
  100 ms when a measured baseline justifies it — followup
  `FU-TXT-LAT-TIGHTEN`.)
- **Scope:** text-only query mode (FTS5 path). If `search` (REQ-053)
  auto-routes to hybrid (text + vector) by default, the hybrid path
  inherits ADR-0.6.0-retrieval-latency-gates' embedder-bearing gate,
  not this one.
- **Workload definition:**
  - **Dataset:** 1,000,000 chunk rows. Synthetic-English-like text
    with a Zipfian token-frequency distribution; vocabulary size and
    text-generator parameters specified by `test-plan.md` fixture
    (the same fixture row-set that backs
    ADR-0.6.0-retrieval-latency-gates' chunk table — only the
    secondary indexes differ: this ADR exercises the FTS5 index;
    retrieval ADR exercises the `vec0` index).
  - **Mean chunk text:** ≈ 500 bytes.
  - **Query mix:** single-token MATCH and one phrase MATCH; query
    tokens drawn from the **50th–90th percentile term-frequency
    band** (avoids both stop-word degenerate slow paths and rare-token
    trivial-fast paths).
  - **Concurrency:** **QPS = 1** (sequential, one in-flight query at
    a time), single-process, **no concurrent writes**.
  - **Cache state:** warm. Warmup protocol = run the full query suite
    once and discard; measure on the second pass. Matches whatever
    `test-plan.md` codifies for the retrieval-latency-gates fixture.
  - **Sample count:** ≥ 1,000 measured queries per percentile
    calculation.
  - **Tokenizer:** whatever FTS5 tokenizer ships as default per
    `design/retrieval.md`. Gate re-validated if the default tokenizer
    changes.
- **Latency boundary:** **in-process** client call → result list.
  Includes safe-grammar parse (per REQ-034) + FTS5 MATCH + canonical
  row fetch + result serialization to in-process result type. Excludes
  IPC / network / subprocess-bridge envelope, reranker, graph-expand,
  and FTS5 `snippet()` / `highlight()` extraction.

Numbers tighter than the vector ADR because text query has no embedder
in the path: no model load, no inference, no GPU/CPU embedding cost.
FTS5 + canonical row fetch is index lookup + B-tree fetch.

## Options considered

**A — p50 ≤ 20 ms; p99 ≤ 150 ms (chosen).** FTS5 + canonical fetch is
expected to be well under p50 ≤ 20 ms on warm cache; p99 set with
shared-runner scheduler jitter headroom. Matches "text faster than
vector" intuition (no embedder); testable. (Original draft proposed
p99 ≤ 100 ms; relaxed per critic [high-1] pending measured baseline.)

**B — p50 ≤ 50 ms; p99 ≤ 200 ms** (mirror retrieval-latency-gates).
Trivial to hit; abdicates the "text is cheaper than vector" property
that should fall out of architecture; loose enough to mask FTS5
regressions.

**C — p50 ≤ 10 ms; p99 ≤ 50 ms.** Aggressive; requires careful FTS5
tokenizer tuning + heavy mmap; risks failing CI on slower runners; no
forcing function justifies it.

**D — p95 ≤ 150 ms** (the harvested value). p95 is the wrong percentile
for an interactive read path (tail latency matters more than median);
no scale / workload defined; rejected for the same reasons critic
[REQ-010] rejected the harvest.

**E — Fold under REQ-013 (canonical-read freshness) with generous
acceptance bound.** Conflates two distinct user concerns (freshness =
"how stale" vs latency = "how slow"). Loses the tier-1 perf-gate
discipline. Rejected.

## Consequences

- `requirements.md` REQ-010 updated to restate this ADR's numbers; ADR
  is authoritative.
- `test-plan.md`: perf AC with seeded fixture per the workload
  definition above; p50/p99 reported per CI run; fails build on
  regression. Fixture row-set shared with retrieval-latency-gates
  fixture (same 1M chunk-row table); only the secondary index under
  test differs (FTS5 here, `vec0` in the retrieval ADR).
- `design/retrieval.md`: documents FTS5 tokenizer choice (default + any
  alternatives), token-frequency-band fixture generator, and required
  SQLite PRAGMAs that achieve A. Tokenizer choice itself is owned by
  `design/retrieval.md` (or its own ADR), not by this latency ADR.
- CI perf job runs on the same pinned tier-1 runner shape as
  retrieval-latency-gates (`x86_64-unknown-linux-gnu` per
  ADR-0.6.0-tier1-ci-platforms reference target).
- Concurrent-write impact on text-query latency is a separate followup
  (parallel to retrieval-latency-gates' equivalent followup); not gated
  here.
- Reranker / graph-expand stages add their own latency; ADR sets the
  **default-pipeline** gate. Stage-augmented latency is documented but
  not gated in 0.6.0.
- FU-REQ-010-TEXT-LATENCY closed by this ADR.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-retrieval-latency-gates (parallel structure).
- ADR-0.6.0-tier1-ci-platforms (reference target).
- REQ-010, REQ-034, REQ-053 (cross-cited).
- Phase 3a critic [REQ-010] (forcing function).
