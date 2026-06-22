# 0.8.3 — running $ ledger + status

Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]],
[[priced-runs-need-resilience-before-spend]]). Aggregate program target ≤ $30
(design `0.8.3-mem0-parity.md` §7): three priced runs — D0b parity (Slice 10),
D1 confirmatory (Slice 20), D2 confirmatory (Slice 25, conditional).

## $ ledger

| date | slice | item | reader | calls | USD | running total |
|---|---|---|---|---|---|---|
| 2026-06-21 | 5 (D0a) | cheap-validate seam (2 q) | gemini-flash-lite | 2 | 0.0001 | 0.0001 |
| 2026-06-21 | 10 (D0b, Phase A) | cheap-validate FULL pipeline (8 q × 3 arms) | gemini-flash-lite | 24 | 0.0275 | 0.0276 |
| 2026-06-21 | 10 (D0b, Phase A) | priced PILOT (12 q × 3 arms) | gpt-5.4 | 36 | 0.9693 | 0.9969 |

**Phase-A spend (Slice 10): $0.9968** (cheap-validate $0.0275 + pilot $0.9693) —
within the ~$1 Phase-A cap. **No full priced pass run** (phase-gate STOP).

## Slice 10 (D0b) Phase-A cost finding — projected full run EXCEEDS $20 at the pilot config

Pilot (gpt-5.4, k=10, full LME session bodies in context): per-(question×arm) call =
**$0.026925** (input-token-dominated: ~21,484 prompt tokens/call from long
multi-turn sessions; ~14 completion tokens). 3 priced arms ran (fathomdb, mem0_oss,
naive_rag; graphiti_zep blocked — see report).

**Projected full run = $0.026925 × 606 q × 3 arms (1818 calls) ≈ $48.95 → NOT ≤ $20**
(also blows the aggregate $30 program budget on D0b alone).

$0 cost-reduction levers (analytic, from the measured pilot tokens — see
`project_full_cost`):

| config | per-call USD | projected full (1818 calls) | ≤ $20? |
|---|---|---|---|
| k=10, full bodies (pilot) | 0.02692 | **$48.95** | no |
| context ≤ 48k chars (~12k tok) | 0.01514 | $27.53 | no |
| context ≤ 32k chars (~8k tok) | 0.01014 | **$18.44** | **yes** |
| context ≤ 24k chars (~6k tok) | 0.00764 | $13.90 | yes |
| k=4, full bodies | 0.01086 | **$19.74** | **yes** |
| k=3, full bodies | 0.00818 | $14.87 | yes |

**Phase-B authorization options (HITL/orchestrator):** (a) run with a window-fit
context budget ≤ ~32k chars (`--context-char-budget 32000`, projected ~$18) or k≤4
(projected ~$20), trading some context for budget; or (b) raise the D0b budget ceiling.
Reducing context may lower identical-answerer accuracy (a measurement-validity
tradeoff) — flag for the HITL. The window-fit lever is applied identically across all
arms, so the R2 same-context-budget invariant holds.

## Slice 10 — Phase-A + fix-1 LANDED on main (2026-06-21)

- Phase-A (D0b runner + Mem0 stand-up + cheap-validate + pilot) cherry-picked to main
  (`625fd946`..`9b425553`); codex §9 = **CONCERN → fix-1** (P2 pyright gate + P3
  cost.n_errors ledger reconciliation), remediated (`f7e2f62e` RED, `636b3ab4` fix),
  re-verified (pyright EXIT=0, 31 tests green). Slice-5 fix-1/fix-2 were already closed
  by a prior session (codex §9 PASS); not re-done.

## Corpus-adequacy decisions (HITL 2026-06-21) — see `0.8.3-corpus-adequacy-note.md`

1. **Research/EVAL-ONLY** use of LOCOMO (CC-BY-NC) + AP-News (MS-Research, no-redistribute);
   payloads gitignored, never committed.
2. **LOCOMO = a SECOND pinned corpus** (own `corpus_hash`), not merged into the LME hash
   — pre-registration amendment to the SIGNED Slice 0 (approved). Restores power on
   multi_session/temporal (paired-power proxy: LME-only underpowers all 3 non-factoid
   classes; +LOCOMO → powered-if-paired).
3. **`knowledge_update` = report-only** (no LOCOMO analog; underpowered even +LOCOMO).
4. Apply the ≥2-session predicate to LOCOMO cat-1 before counting its multi_session N.

## Slice 10 — Phase-B AUTHORIZED: **Option A — land the gap cheaply now, power at S20**

Rationale: `eu7=0.896 < 0.90` makes `decide_083` return NOT_REACHED/blocked_by:eu7
**regardless of power**, and eu7 only re-clears at Slice 20 — so spending to make classes
gateable now buys nothing. Phase-B = (a) **priced** identical-answerer accuracy on
**LME-only**, `--context-char-budget 32000`, `--max-usd 20` (~$18), resilient/resumable;
(b) **$0** strict Recall@K on the **powered LME+LOCOMO** corpus. The per-class gap LANDS
(capability-report #1); the powered **priced** verdict is deferred to Slice 20 (post-eu7).
Graphiti/Zep deferred (clean blocker; 2nd comparator) → run {Mem0, naive_rag}.

## eu7 0.937→0.896 bisect RESOLVED (2026-06-22, $0 / no-LLM / no-GPU / no-build)

Offline archaeology — `runs/0.8.3-eu7-bisect-report.md` (+ `.json` / `-output.json`).
**Verdict: case A (vector-path/SUT), NOT case B (CLS/embedding); corpus also ruled out.**
- Embedder src **byte-identical** v0.7.2→v0.8.0; CLS pooling is option-only (`c7afbfde`,
  default Mean, dated AFTER the 0.896 run) → CLS cannot be the cause.
- GA-1 already ruled out corpus (byte-identical). Every deterministic quant/KNN
  primitive (KNN SQL, Pack1→Pack2 bits, f32 rerank) is byte-identical → no
  fidelity-loss commit. Leading cause = the B-1 `vector_stage_only` SUT change
  (0.937 = pre-correction `search()` anchor; 0.896 = the seam — not comparable).
- **Reaffirms the Phase-B Option A premise above:** `eu7=0.896<0.90` is the true
  vector-stage fidelity, and it is **not a CLS problem**. Slice-20 must judge eu7
  **fresh** vs the 0.90 floor after the re-embed; if it breaches, the recovery lever
  is the **quant path** (whitening → K>192 → 2-bit), NOT pooling.
- Residual (definitive engine 0.7.x-vs-0.8.0 vector-stage A/B) blocked: no
  pre-embedded eu7-corpus DB. **$0 in API** (eu7 is LLM-free); needs an embed
  (minutes on GPU when util<5%, ~1h CPU) — schedule as a Slice-20 sub-task, not a blocker.
