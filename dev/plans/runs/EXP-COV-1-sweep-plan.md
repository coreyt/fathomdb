# EXP-COV-1 — coverage→outcome sufficiency sweep (execution plan, `$0`-ready / spend HELD)

> **Status:** PREPARED, NOT RUN. A coordinator relayed a HITL decision authorizing a **$20** ceiling for
> this sweep; per the system reminder + `push-scope-fathomdb-only`, a coordinator relay carries **no user
> authority for real spend**, so **no priced call has been executed** (not even the pilot). This doc is the
> ready-to-run spec so a session with the **user's own** spend confirmation can execute immediately.
> Purpose (HITL): a **sufficiency test** — does closing the edge/relation coverage gap move a downstream
> metric **above the ~0.571 embedder ceiling**? Pre-registration: `dev/design/0.8.12-coverage-probe-and-value-test.md` §A;
> arm design: Memex `dev/fathomdb/OPP-6-experiments.md` EXP-COV-1.

---

## 1. What Slice 5 already settled (why this sweep is the right next step)

The `$0` EXP-COV-0 census (`EXP-COV-results.md`) found: entity coverage is solved (0.85, a cheap local
model matches the frontier); the gap is **edges/relations** (ELPS strict 0.23, CI95 [0.157, 0.306]).
The census establishes *necessary* headroom but **cannot** establish *sufficiency* — whether closing the
edge gap actually lifts a downstream retrieval/answer metric, or whether the embedder ceiling absorbs it.
EXP-COV-1 is exactly that sufficiency read.

## 2. Design (coverage as the independent variable, stack held fixed)

Sweep a small set of coverage conditions, holding the embedder + retrieval/CE stack FIXED (OPP-6 §2):

| Condition | Extractor | Priced? |
|-----------|-----------|---------|
| C0-floor | heuristic (Slice-5 census) | `$0` |
| ELPS-baseline | current `claude-haiku-4-5` (pre-computed outputs already on disk) | `$0` |
| **C-relation** | a relation-targeted extraction pass (the scoped lever the census points at) | **priced** |

For each condition: rebuild the fact-graph/index from that extraction, run the SAME query set through the
SAME retrieval + CE-rerank stack, score downstream metrics **per intent class**. Plot outcome vs coverage.

- **Dependent variables:** gold-in-pool / recall@k and MRR/r@1-after-CE; answer EM/F1 where gold exists.
- **Held fixed:** CLS-corrected bge-small (no swap), 1-bit ANN, RRF, CE-rerank knobs, candidate breadth,
  query set. Only the fact-graph/index contents change with coverage.
- **Corpus:** a multi_session/temporal slice where edge coverage should matter (LOCOMO real gold; academic
  MuSiQue for the multi-hop consumer). LOCOMO/AP-News stay gitignored EVAL-ONLY — persist only metrics.

## 3. Decision rule (pre-registered — sufficiency)

- **SUFFICIENT (coverage is the lever → a scoped Slice-10 run is justified)** iff a coverage increase
  produces a downstream Δ(gold-in-pool) or Δ(F1) with **paired-bootstrap CI lower bound > +0.04** on ≥1
  powered class (expected: multi_session/temporal), net of cost, with the precision guard intact — i.e.
  the metric moves **above** what the ~0.571 embedder ceiling would cap.
- **CEILING-ABSORBED (redirect → resolve OPP-6 #6)** iff the outcome curve is flat at the ceiling across
  classes (all CIs span ≤ the noise floor). Per OPP-6 §7 this **resolves** OPP-6 by redirecting to
  embedder/recall — a legitimate closed outcome; recommend the redirect and do NOT run the full Slice-10
  extraction.

## 4. Priced-run resilience preconditions (ALL mandatory BEFORE any spend)

Hard gate — none of these may be skipped (memory `priced-runs-need-resilience-before-spend`):

- [ ] **Incremental atomic checkpoint** — per-doc/per-batch extraction results written atomically
      (temp-file + rename), so a crash loses ≤ 1 unit.
- [ ] **Verified `--resume`** — re-invoking skips completed units by a stable key (doc_id + prompt_version
      + model); verified by a kill-and-resume dry run on the `$0` conditions first.
- [ ] **429 / 5xx backoff** — exponential backoff + cap; a rate-limit is a retry, never a silent drop.
- [ ] **Window-fit** — per-doc token budget check before the call (truncate/split, never overflow-fail).
- [ ] **Completeness guard** — `failure ≠ abstention`: a failed extraction is recorded as FAILED (not an
      empty/valid result), and the run refuses to score until every unit is present-or-explicitly-failed.
- [ ] **Running `$` ledger** — every priced batch appends to the STATUS-0.8.12 `$` ledger with cumulative
      spend; the runner auto-stops at the cap.

## 5. Cheap-validate ladder inside the $20 cap (the coordinator's directive)

1. Run the two `$0` conditions (C0-floor, ELPS-baseline) end-to-end through the full sweep pipeline FIRST
   — proves the index-rebuild + scoring path works with zero spend.
2. **~$0.05 pilot** of C-relation on a tiny doc sample; record actual tokens + `$`.
3. **Extrapolate** the full C-relation extraction cost = (pilot `$` / pilot docs) × full docs, with a
   headroom margin.
4. **If the extrapolation ≤ $20:** run the full C-relation pass under §4 resilience + the ledger cap.
   **If it would exceed $20:** STOP and report the estimate instead of spending (do not partial-run into
   an incomplete, unscoreable sweep).

## 6. Outputs (persisted, fathomdb-only, no licensed payloads)

- `dev/plans/runs/EXP-COV-1-results.md` — outcome-vs-coverage curve per class with paired-bootstrap CIs,
  the sufficiency verdict, and the full Slice-10 extraction cost estimate.
- STATUS-0.8.12 `$` ledger updated per batch.

## 7. Hard-stop after the sweep

Report to HITL: the sufficiency verdict + the full-Slice-10 cost estimate. **Do NOT run the full
relation-targeted Slice-10 extraction without a fresh explicit HITL go.**
