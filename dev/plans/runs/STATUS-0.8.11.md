# STATUS — 0.8.11 (live)

> Orchestrator session `fathom-0.8.11-orchestrator-b`. Plan → `dev/plans/plan-0.8.11.md`;
> contracts → `dev/plans/0.8.11-implementation.md`; deps → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (F-11).
> Branch: `0.8.11` (off `origin/main` @ `80c6b8b8` + F-11 plan rewrite). Goal: **complete Slices 0–40.**

## Headline

0.8.11 owns and discharges the missed planner-router experiment ladder (F-11) as **Track E** (eval
spine, Slices 5–35) plus **Track G** (#17 filter-grammar + F-8b, Slice 40), converging at Slice 45.
Slice 0 (this) freezes the pre-registrations + ADRs + HITL decisions. **Budget: ~$20 priced-LLM
ceiling** (raised from $0, HITL 2026-06-28); running tally below.

## Slice board

| Slice | Title | Track | State | Notes |
| ---: | --- | :---: | --- | --- |
| **0** | ADRs + ladder pre-registration + STATUS standup | — | **DONE** | contracts ✅; STATUS ✅; ADR-0.8.11 ✅ (`a9ba8a5a`); ledger scaffold ✅ (F-11 rows REGISTERED); 2 HITL Slice-0 decisions ✅ (A / conditional) |
| 5 | Gate-0 + Gate-2 (eval foundation) | E | **DONE** | $0; Gate-0 re-scope + Gate-2 oracle ceiling (+0.392 reconciled); ledger rows RESOLVED |
| 10 | EXP-A ‖ EXP-M4 | E | **DONE** | $0; EXP-A **GO** (multi_session gold-in-pool @10→@200 +0.45/+0.40, CI clears floor; candidate_k=200, not saturated); EXP-M4 **KEEP bge-small** (no swap-candidate clears eu7 re-clear+cost; GPU device-invariance ✅); ledger rows RESOLVED |
| 15 | EXP-B′ joint tuning (KEYSTONE) | E | pending | blocked-by 10 (A∧M4) |
| 20 | EXP-Fr-acc base | E | pending | blocked-by 5 |
| 25 | EXP-Fr-acc/VoI finalize | E | pending | blocked-by 20 |
| 30 | EXP-AF value test (KILL/GO) | E | pending | blocked-by 25; HITL #3 |
| 35 | L2 router prototype + pre-stage | E | pending | blocked-by 15∧25∧30 |
| 40 | #17 filter-grammar + F-8b exec | G | pending | blocked-by 0 only (∥ spine) |
| 45 | Verification + release readiness | — | pending | blocked-by 5–40 |

## HITL decision points (four; owner: steward → HITL)

1. **F-8b** (Slice 0) — `record_feedback` instrumentation → governed command?
   **RULED 2026-06-28: conditional on EXP-AF** (stays instrumentation; promote at Slice 40 iff
   EXP-AF GO). `enable_telemetry`/`last_telemetry_query_id` stay instrumentation regardless.
2. **Filter-grammar shape** (Slice 0) — unified type vs thin adapter?
   **RULED 2026-06-28: Option A — one unified `Filter` + two backends**, with the anti-laziness
   mandate (reserved-gap only for a genuine field-resolution constraint). → ADR-0.8.11.
3. **EXP-M4 swap KILL/GO** (Slice 10 readout) — beat bge-small? Default keep; productized swap
   out-of-0.8.11, separately gated. — **RESOLVED 2026-06-28: KEEP bge-small.** No swap-candidate
   clears the gate net of eu7 re-clear + cost (bge-base/e5 fail the 0.90 1-bit floor; nomic not
   cpu_feasible; gte-base measurement-failed). No ceiling escalation triggered (no passer). Swap
   stays out-of-0.8.11.
4. **EXP-AF KILL/GO** (Slice 30 readout) — agent signal beats `ce_score` net of round-trip?
   Gates the L2 feedback arm + F-8b promotion. — PENDING.

## Budget — running `$` tally (ceiling ~$20)

| Experiment | Ceiling | Spent | Status |
| --- | ---: | ---: | --- |
| Gate-0 (scoped labeling) | $1 | $0 | not started |
| EXP-B′ judge | $6 | $0 | not started |
| EXP-Fr-acc base | $3 | $0 | not started |
| EXP-Fr-acc/VoI | $3 | $0 | not started |
| EXP-AF | $5 | $0 | not started |
| Reserve | $2 | $0 | — |
| **Total** | **$20** | **$0** | — |

Gate-2 / EXP-A / EXP-M4 are $0 (local / GPU). No priced run starts before its pre-registration
(`0.8.11-implementation.md §1`) is committed; cheap-validate (gemini-flash-lite) before each spend.

## Cross-cutting DoD (X1/X2/X3)

- **X1** SDK parity (Py↔TS) — applies to any surface change (Slice 40 filter contract; `record_feedback`
  if promoted). Eval slices' "shippable" DoD = landed result doc + reproducible script + ledger row.
- **X2** `mkdocs build --strict` green.
- **X3** docs + DOC-INDEX entry per surface-touching slice.

## Verification log

- 2026-06-28: branch `0.8.11` cut off `origin/main` (`80c6b8b8`); plan rewrite + F-11 sequencing +
  BEIR manifest committed; merged `origin/main`. Working tree clean at Slice 0 start.
- 2026-06-28: Slice 0 closed. ADR-0.8.11 filter-grammar (`a9ba8a5a`) found **NO reserved-gap
  trigger** — all 4 G10 fields resolve in both stores (`source_type` constant-folds via
  `resolve_source_type(kind)`); arbitrary json-paths are typed-rejected by `search_filtered` (a
  defined dispatch outcome). **Correction:** G4 `read.list`/`Predicate` is **already built +
  SDK-exposed** (0.8.0) → Slice 40 is a type-unification + dispatch **refactor**, not greenfield.
  Unified type: `Filter { terms: Vec<FilterTerm> }` (implicit AND); one impl-level TBD (kind
  redundancy/constant-fold) left for Slice 40.
- 2026-06-28: **Slice 5 DONE ($0).** Gate-0 re-scope (`gate0-rescope-output.{md,json}`,
  `eval/gate0_rescope.py`): 4 corpora (LME 606Q · LOCOMO 1443Q · AP-News 1397 art/350 AutoQ ·
  MuSiQue 2417 answerable) cover all 5 intents; node-level labels present/derivable for
  needle+multi_hop, not needed for global (win-rate), and the ONE scoped gap = LOCOMO
  multi_session/temporal session→node (≤$1, unspent). EXP-D excluded → 0.8.17. Gate-2 oracle
  ceiling (`gate2-oracle.md` + `gate2-oracle-output.json`, `eval/gate2_oracle_run.py`):
  oracle-CONTEXT pooled **+0.392 [0.346,0.436]** (reconciles exactly with the 0.8.3 ledger; fresh
  recompute is priced → deferred). KILL check: NO KILL on the context axis (huge headroom), but
  static arm-selection buys ≈0 (within recall noise; multi_session 0.00) → router value =
  config-carrying per-intent tuning (EXP-A/B′), not arm routing. Ledger Gate-0/Gate-2 rows RESOLVED.
  *(Gate-0 committed in parallel @`a370ae86`/`3e3c5585`; this session corrected the script's
  label-gap to match the authoritative .md (global needs no labels) and added the JSON companion.)*
- 2026-06-28: **Slice 10 DONE ($0).** **EXP-A** (`expa-recall-output.json` + `expa-recall.md`,
  `eval/expa_recall_run.py`): LME n=160 (40/class, seed 20260614, 7,154-session union); candidate
  breadth K∈{10,20,50,100,200}, gold-in-pool + bootstrap CI (2000×, seed 0xEA), per-query per-arm
  gold-rank log. **GO** — F2 multi_session gold-in-pool @10 0.20/0.275 (fts/bm25) → @200
  0.65/0.675 (**lift +0.45/+0.40**, @200 CI-lo 0.50/0.525 clears the @10 floor); all 4 classes
  lift. **candidate_k=200 maximizes** (still rising at 200 → EXP-B′ probe ≥200). Per-query
  arm-selection oracle (deferred at Gate-2) now computable. Lexical arms measured at $0; fused-RRF
  arm corroborates (CPU-embedder build not run in-session; anchored by Gate-2 fused≈bm25
  multi_session). **EXP-M4** (`expm4-ceiling-output.json` + `expm4-ceiling.md`,
  `eval/expm4_embedder_ceiling_run.py`): ceiling is a device-invariant model-weights property →
  consolidate the FULL `s15a` probe (n=10,506) + `eu-0` sweep + **GPU device-invariance confirmation
  on RTX 3090 cuda:0** (bge-small GPU-vs-CPU cosine 1.000000, max abs diff 1.2e-7). **KEEP
  bge-small** — no swap-candidate clears `probe_15a_pass`: bge-base proj_eu7 0.786<0.90 + hard CI-lo
  −0.004; e5-base-v2 0.896<0.90; nomic 0.932 but not cpu_feasible; gte-base measurement-failed.
  eu-0 raw r@10 (K=256) bge-small 0.933 / bge-base 0.964 / e5 0.664 — confirms ordering, revises the
  swap decision (bge-base's raw edge dies under 1-bit eu7). HITL #3 RESOLVED (no escalation; swap
  out-of-0.8.11). Ledger EXP-A/EXP-M4 rows RESOLVED.

## Experiments-ledger (F-11 closure tracker)

Rows to be added to `dev/experiments-ledger.md` (zero planner-router rows today): Gate-0, Gate-2,
EXP-A, EXP-M4, EXP-B′ (+B′.5), EXP-Fr-acc base, EXP-Fr-acc/VoI, EXP-AF. Each lands at its slice.
