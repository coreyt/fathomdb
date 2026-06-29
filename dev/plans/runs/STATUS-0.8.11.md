# STATUS — 0.8.11 (live)

> Orchestrator session `fathom-0.8.11-orchestrator-b`. Plan → `dev/plans/plan-0.8.11.md`;
> contracts → `dev/plans/0.8.11-implementation.md`; deps → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (F-11).
> Branch: `0.8.11` (off `origin/main` @ `80c6b8b8` + F-11 plan rewrite). Goal: **complete Slices 0–40.**

## Headline

0.8.11 owns and discharges the missed planner-router experiment ladder (F-11) as **Track E** (eval
spine, Slices 5–35) plus **Track G** (#17 filter-grammar + F-8b, Slice 40), converging at Slice 45.
Slice 0 (this) freezes the pre-registrations + ADRs + HITL decisions. **Budget: ~$20 priced-LLM
ceiling** (raised from $0, HITL 2026-06-28); running tally below.

> **✅ SLICES 0–40 COMPLETE (2026-06-28).** All four HITL gates resolved (F-8b keep-instrumentation;
> filter Option A; keep bge-small; EXP-AF KILL). Total spend **~$3.73 / $20**. **All results are
> PROVISIONAL SCREENING DATA** — confidence ledger + the **Pre-0.8.15 Validation Gate (V-1..V-7)**
> live in **`runs/0.8.11-handoff-to-0.8.15.md`**; nothing downstream may treat the tuples as a
> contract until that gate passes. Remaining (post-40, outside this goal): the `origin/main` sync to
> clear the one orthogonal pre-existing red (`test_decision_rule_083`, fixed on main) + Slice 45 verification.

## Slice board

| Slice | Title | Track | State | Notes |
| ---: | --- | :---: | --- | --- |
| **0** | ADRs + ladder pre-registration + STATUS standup | — | **DONE** | contracts ✅; STATUS ✅; ADR-0.8.11 ✅ (`a9ba8a5a`); ledger scaffold ✅ (F-11 rows REGISTERED); 2 HITL Slice-0 decisions ✅ (A / conditional) |
| 5 | Gate-0 + Gate-2 (eval foundation) | E | **DONE** | $0; Gate-0 re-scope + Gate-2 oracle ceiling (+0.392 reconciled); ledger rows RESOLVED |
| 10 | EXP-A ‖ EXP-M4 | E | **DONE** | $0; EXP-A **GO** (multi_session gold-in-pool @10→@200 +0.45/+0.40, CI clears floor; candidate_k=200, not saturated); EXP-M4 **KEEP bge-small** (no swap-candidate clears eu7 re-clear+cost; GPU device-invariance ✅); ledger rows RESOLVED |
| 15 | EXP-B′ joint tuning (KEYSTONE) | E | **DONE** | $0; per-intent optima DIVERGE (3 distinct → NO KILL, routing has value); crux reproduced (pooled α=1.0 ck200 pn10→50 r@10 0.540→0.498, needle-specific); B′.5 catches real regressions (multi_session opt→needle −0.147); global+multi_hop provisional; build-blocker (CE feature OFF→0.8.3 CE-pass); ledger EXP-B′/B′.5 rows RESOLVED |
| 20 | EXP-Fr-acc base | E | **DONE** | ~$0.05; classifier macro 0.768 (NO KILL, all 5 > chance); needle→C asymmetry confirmed (only negative Δ_C; −0.300 [−0.47,−0.10] @8-distractor ≈ prior −0.362); ledger row RESOLVED |
| 25 | EXP-Fr-acc/VoI finalize | E | **DONE** | $0.0151; CE reranker ACTIVE (guarded). Value-of-signal: cheap agent (`gemini-flash-lite`) relevance **DOMINATED by free `ce_score`** — lift −0.138 [−0.189,−0.087] (n=450), AUC ce 0.667 vs 0.545 → **QUALIFIED KILL (cheap agent)**: ask-or-not buys nothing, route on internal `ce_score`. Asymmetric weighting **CONFIRMED** (6× cost ratio, c_rt\* 0.30 vs 0.05; cross-wire rare 4/606). VoI landscape (low-ce+narrow-margin) → EXP-AF (Slice 30). Ledger row RESOLVED |
| 30 | EXP-AF value test (KILL/GO) | E | **DONE** | $3.66/$5; **KILL** (HITL #4). Stronger agent (`claude-sonnet`) on Slice-25 break-even cells (ce_top<0.2, n=406) does NOT beat `ce_score` net of round-trip: depth-1 reranking lift +0.0074 [−0.0074,+0.0222] (CI spans 0 even @c_rt=0); NET @c_rt=0.02 −0.0126 [−0.0274,+0.0022]. Realized 6% of the 0.118 headroom (promoted 6/demoted 3) → signal-bound. Detection −0.0296 [−0.0715,+0.0123] (closes most of cheap-agent −0.138 gap, still loses). Depth-2 gross+ but net-neg → one-shot. → L2 prototype (Slice 35) drops feedback arm; `record_feedback` STAYS instrumentation (overrides F-8b promote). Ledger row RESOLVED |
| 35 | L2 router prototype + pre-stage | E | **DONE** | $0, CALLER-SIDE (commit `523fca3d`). `recommend(query,*,agent_hint=None)->Recommendation` recommends a stack WITHOUT executing; registry built from EXP-B′+Gate-2. Smoke test **42/0**. **R-L2-1..4 met**: all 5 classes route (1); each carries a registered tuple + cost_tier (2); `agent_hint` verbatim conf 1.0, no fallback, unknown raises (3); ZERO diff to `src/rust`/`src/python/fathomdb`/`src/ts`, `fathomdb` never imported (4). Provenance honest: 3 measured (needle/multi_session/temporal) + 2 provisional (global/multi_hop); confidence header = SCREENING DATA, 0.8.15 re-validates. `feedback_arm=False` hard-wired (EXP-AF KILL); EXP-B′.5 forbidden-composition validator seam (`check_forbidden`) inherited by 0.8.15. `dev/prototypes/l2-router/` + handoff `runs/slice-35-l2-prototype.md` |
| 40 | #17 filter-grammar + F-8b exec | G | **DONE** | merged `slice-40`→`0.8.11`; unified `Filter`+2 backends (no reserved-gap); Rust 6/0 + G10 byte-identity pin 6/0; **X1 GREEN** Py 31 (filter-unif 23 + read.list 8) + TS 26; F-8b = KEEP instrumentation (no allowlist change; revisit iff EXP-AF GO); rebuilt `.venv` w/ `default-reranker`+`default-embedder`+`test-hooks` |
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
   Gates the L2 feedback arm + F-8b promotion. — **RESOLVED 2026-06-28: KILL.** A stronger agent
   (`claude-sonnet`, $3.66/$5) seeing the full top-20 pool, targeted at the Slice-25 break-even
   cells (ce_top<0.2, n=406), does NOT beat internal `ce_score` net of round-trip (depth-1
   reranking lift NET @c_rt=0.02 = −0.0126 [−0.0274,+0.0022], GO=False; lift CI spans 0 even at a
   free round-trip). **L2 prototype (Slice 35) drops the agent-signal loop** (`feedback_arm=False`);
   **`record_feedback` STAYS instrumentation** — the EXP-AF KILL overrides any F-8b promote (no
   Slice-40 reserved-gap patch, no allowlist change). This also closes HITL #1 (F-8b) negatively.

## Budget — running `$` tally (ceiling ~$20)

| Experiment | Ceiling | Spent | Status |
| --- | ---: | ---: | --- |
| Gate-0 (scoped labeling) | $1 | $0 | not started |
| EXP-B′ judge | $6 | $0 | **DONE — $0** (judge not spent; gold sufficient, global provisional) |
| EXP-Fr-acc base | $3 | ~$0.05 | **DONE** (gemini-flash-lite; local vLLM down) |
| EXP-Fr-acc/VoI | $3 | $0.0151 | **DONE** (gemini-flash-lite; 450 calls; CE-active build) |
| EXP-AF | $5 | $3.66 | **DONE — KILL** (claude-sonnet; 624 calls; CE-active build) |
| Reserve | $2 | $0 | — |
| **Total** | **$20** | **~$3.73** | EXP-Fr-acc base + VoI + EXP-AF spent (≪ ceiling) |

Gate-2 / EXP-A / EXP-M4 are $0 (local / GPU). No priced run starts before its pre-registration
(`0.8.11-implementation.md §1`) is committed; cheap-validate (gemini-flash-lite) before each spend.

## Cross-cutting DoD (X1/X2/X3)

- **X1** SDK parity (Py↔TS) — applies to any surface change (Slice 40 filter contract; `record_feedback`
  if promoted). Eval slices' "shippable" DoD = landed result doc + reproducible script + ledger row.
- **X2** `mkdocs build --strict` green.
- **X3** docs + DOC-INDEX entry per surface-touching slice.

## Verification log

- 2026-06-28: **Slice 35 DONE ($0, CALLER-SIDE) — L2 router prototype + dispatcher pre-stage**
  (commit `523fca3d`; `dev/prototypes/l2-router/{router,build_registry,test_smoke}.py` +
  `registry.json` + `README.md`; hand-off `runs/slice-35-l2-prototype.md`). `recommend(query, *,
  agent_hint=None) -> Recommendation` (frozen dataclass `intent/stack/config/confidence/cost_tier/
  rationale/feedback_arm`) **recommends a stack WITHOUT executing retrieval**. Intent resolution
  (PSD §II.A): `agent_hint` verbatim (conf 1.0, **no** classifier fallback) else internal lexical
  Rocchio classifier (Slice-20 mirror, lower-bound proxy). Registry generated from
  `expb-joint-tune-output.json` (per-intent tuples + B′.5 guard) + `gate2-oracle-output.json`
  (per-arm cost tiers → bucketed `cost_tier`); provenance honest: **3 measured** (needle/
  multi_session/temporal) + **2 provisional** (global/multi_hop EXP-0 pins); `confidence_header` =
  SCREENING DATA, **0.8.15 must re-validate**. `feedback_arm=False` hard-wired (EXP-AF KILL — router
  stays on internal `ce_score`, no agent-signal loop). EXP-B′.5 **forbidden-composition validator
  seam** (`check_forbidden` → `ForbiddenCompositionError`; map_reduce_qfs/community_summary
  `global`-only) the 0.8.15 plan validator inherits. **Smoke test 42 passed / 0 failed (exit 0).**
  **R-L2-1..4 met:** all 5 classes route (1); each carries a registered tuple + valid cost_tier (2);
  `agent_hint` verbatim/conf 1.0/overrides query text/unknown raises — no silent fallback (3);
  **zero diff to `src/rust` / `src/python/fathomdb` / `src/ts`**, `fathomdb` never in `sys.modules`,
  no retrieval executed (4). Ledger: Track-E L2-prototype row closed (Slice 35 = last Track-E slice).
- 2026-06-28: **Slice 30 DONE ($3.66/$5) — KILL (HITL #4).** EXP-AF agent-feedback value test
  (`expaf-value-output.json` + `expaf-value.md`, `eval/expaf_value_run.py`; pricing alias pinned in
  `eval/gap_decomposition_run.py`). **CE-active guard PASS** (same as Slice 25). The decisive test:
  does a STRONGER agent (`claude-sonnet`) seeing the **full top-20 ce-reranked pool** (not just
  top-1, fixing the Slice-25 caveat) and used to **actually re-rank** beat internal `ce_score` net of
  round-trip, focused on the Slice-25 break-even cells (ce_top<0.2, n=406 LME)? **$0 headroom
  pre-gate:** depth-1 ceiling 0.118 / depth-2 0.209 (room exists). **Arm 1 (primary) reranking lift
  +0.0074 [−0.0074,+0.0222]** — CI spans 0 even at a free round-trip; realized only ~6% of the 0.118
  ceiling (promoted 6 gold / demoted 3 → signal-bound, not headroom-bound); **NET @c_rt=0.02
  −0.0126 [−0.0274,+0.0022], GO=False.** **Arm 2 detection −0.0296 [−0.0715,+0.0123]** (closes most
  of the cheap-agent −0.138 gap but still loses to the free `ce_score`). **Arm 3 depth-2** (one
  re-plan, trigger 0.537) gross-positive +0.0222 [0.0049,+0.0395] but net-negative at any c_rt>0 →
  **one-shot, not iterative** (depth question moot under KILL). **Verdict KILL** → L2 prototype
  (Slice 35) drops the agent-signal loop (`feedback_arm=False`); **`record_feedback` STAYS
  instrumentation** (overrides any F-8b promote — no Slice-40 reserved-gap patch / no allowlist
  change). Root cause: in low-`ce` cells the engine is uncertain because the answer genuinely is not
  cleanly present (recall-bound) — the agent does not manufacture recall the substrate never produced
  (PSD §II.C). Resilient harness (per-item checkpoint / `--resume` / `BudgetLedger --max-usd 5.0`
  pre-call guard; 624 calls, 0 errors); cheap-validated (3 calls) first. Ledger EXP-AF row RESOLVED.
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
- 2026-06-28: **Slice 25 DONE ($0.0151/$3).** EXP-Fr-acc/VoI finalize (`fracc-voi-output.json` +
  `fracc-voi.md`, `eval/fracc_voi_run.py`). **CE-active FIRST STEP guarded PASS** (engine `rerank`
  real `ce_score`, max ce_norm 0.99994, alpha=1.0 reorders adversarial probe → rank-1; not the
  Slice-15 identity passthrough). Three deliverables (PSD §III.D): **(1) value-of-signal** — a cheap
  agent (`gemini-flash-lite`) relevance signal is **DOMINATED by the free internal `ce_score`** at
  predicting retrieval-correct (gold-in-top-10) over n=450 (150×{needle,ms,temporal}): lift **−0.138
  [−0.189,−0.087]** (paired), **AUC ce 0.667 vs agent 0.545** (conservative LB — ce got an oracle
  threshold; agent saw only the top-1 passage, not deployed user-intent). **(2) ask-or-not VoI** —
  oracle-upper-bound `(ce_top×route_margin)` loss-landscape; value concentrates in **low-ce(<0.2) +
  narrow-margin** cells (E[loss]→0.72). **(3) asymmetric weighting CONFIRMED** — 6× cost ratio,
  ask-threshold c_rt\* **0.30 cross-wire vs 0.05 same-tier**; for c_rt∈(0.05,0.30] the policy asks to
  block needle→C but declines a same-tier miss; cross-wire rare (4/606 realized) but heaviest per
  incident. **KILL = QUALIFIED KILL (cheap agent):** ask-or-not buys nothing with `gemini-flash-lite`
  → route on internal `ce_score`; the break-even landscape (low-ce + cross-wire) is the shape a
  STRONGER agent would exploit → **feeds EXP-AF (Slice 30)** + the 0.8.15 dispatcher. Route-margin via
  leakage-free 5-fold OOF classifier (LME routing acc 0.642). Resilient harness (per-item checkpoint /
  `--resume` / `BudgetLedger` $3 guard); cheap-validated (6 calls) first. Ledger EXP-Fr-acc/VoI row
  RESOLVED. **§6 sequencing finding:** the agent-signal loop is NOT yet justified (cheap agent loses to
  `ce_score`); Slice 30 EXP-AF must clear the agent-beats-`ce_score` bar with a stronger agent before
  the L2 prototype (Slice 35) wires any escalation — else the dispatcher ships on internal `ce_score`.
- 2026-06-28: **Slice 20 DONE (~$0.05/$3).** EXP-Fr-acc base (`fracc-base-output.json` +
  `fracc-base.md`, `eval/fracc_classifier_run.py`). **Classifier ($0):** pure-numpy lexical TF-IDF
  nearest-centroid (Rocchio), stratified 5-fold CV, balanced 100/class — macro **0.768
  [0.732,0.802]**, all 5 classes > 0.20 chance → **NO KILL** (needle weakest 0.500; global 1.000;
  multi_hop 0.940). *(No torch/sklearn → lexical fallback proxy, likely a lower bound.)*
  **Mis-route matrix (gemini-flash-lite; local vLLM qwen3.6-27b/gemma-4 were HTTP-500 down):**
  oracle-context answer-quality, C(map-reduce/QFS) vs retrieval, same judge both arms, paired
  bootstrap. **needle is the ONLY negative Δ_C** (others +0.04). Load-bearing needle→C **scales with
  map-reduce breadth**: −0.080 [−0.28,+0.12] @3-distractor → **−0.300 [−0.47,−0.10] @8-distractor**
  (CI excludes 0; ≈ prior −0.362 — itself a weak-distiller artifact per 0.8.3 ledger). Router-isolation
  (C forbidden on needle) supported → EXP-B′.5 `forbidden_ops`; asymmetry feeds Slice-25 VoI.
  Resilient harness (checkpoint/resume/`BudgetLedger` $3 guard); cheap-validated. Ledger EXP-Fr-acc
  row RESOLVED.
- 2026-06-28: **Slice 15 DONE — KEYSTONE ($0/$6).** EXP-B′ + EXP-B′.5 (`expb-joint-tune-output.json`
  + `expb-joint-tune.md`, `eval/expb_joint_tune_run.py`). Joint sweep candidate_k{200,300,500} ×
  pool_n{10,20,50,100,200} × α{0,0.3,0.5,0.7,1.0} × final_K=10 over LME 606Q node-level gold.
  **NO KILL — per-intent optima DIVERGE (3 distinct):** needle (200/50/0.7, r@10 **0.644**),
  multi_session (300/100/1.0, **0.467**), temporal (500/20/1.0, **0.513**) → config-carrying router
  has measured value. **Crux reproduced** (pooled α=1.0 ck200 pool_n 10→50 r@10 0.540→0.498,
  Δ−0.041) and refined: **needle-specific** (multi_session/temporal do NOT drop). **B′.5:** static
  router-isolation rule (`map_reduce_qfs`/`community_summary` global-only) + empirical
  cross-application — **multi_session opt→needle r@10 −0.147**, temporal opt→needle −0.075 (clear
  noise) → real joint regressions the 0.8.15 validator blocks. global+multi_hop pinned provisional
  (global = decide_084 win-rate, no node labels; multi_hop = build-blocker). **Build-blocker (loud,
  justified deviation):** .venv build compiled `rerank_fused` CE inference gated OFF
  (`#[cfg(feature="default-reranker")]`→identity); rebuild forbidden → rerank tuple+crux from landed
  0.8.3 CE-pass (same gold+weights, feature-ON), recall envelope fresh; `ce_norm_is_active` guard
  added. **Judge $0** (gold sufficient). Ledger EXP-B′/EXP-B′.5 rows RESOLVED. *(24MB recall-pool
  checkpoint not committed — regenerable; envelope preserved in output JSON.)*

## Experiments-ledger (F-11 closure tracker)

Rows to be added to `dev/experiments-ledger.md` (zero planner-router rows today): Gate-0, Gate-2,
EXP-A, EXP-M4, EXP-B′ (+B′.5), EXP-Fr-acc base, EXP-Fr-acc/VoI, EXP-AF. Each lands at its slice.
