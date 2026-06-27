# Response to the codex adversarial review

Reviewer: codex gpt-5.5, high reasoning effort. Quarantine honored (it did not open the
author's conclusion docs). Full review: `CODEX-REVIEW-OUTPUT.md`. Run log: `codex-run.log`.

## 1. Did the data support the author's analysis? (review Task 4)

**Yes — independently corroborated on every headline.** codex's assume-correctness
analysis reproduced, from the raw data alone:
- H1 persistence **supported**; H2 warm-reuse-cheaper **supported but warmth-dependent**
  (it independently surfaced the cold-specialist $1.93 > fresh $1.32 counterexample —
  the author's central "warmth is first-order" claim); H3 crossover **K=2** for the
  10k/high-W workflows; H4 keep-T-small **validated** (no fidelity loss on probes);
  H5 cheap-distillation **refuted/inconclusive**.
- The "unnamed effects" it listed (first-reuse cache-write tax, idle-TTL reset, high-W
  still cheaper, fresh-is-cache-assisted) match the author's findings.
- Its data-implied decision rule is near-identical to the author's.

## 2. Data integrity

codex **re-parsed all 16 transcripts independently: 0/38 segment mismatches** vs
`data/all-segments.csv`, and spot-checked three cost computations by hand — all match.
The harness and data are verified correct.

## 3. Findings accepted (folded into ANALYSIS.md / BEST-PRACTICES.md / STEWARD-PROMPT-SECTION.md as "CAVEAT")

| # | Finding | Significance | Disposition |
|---|---|---|---|
| 1 | Task-equivalence: warm-vs-fresh aren't the same task | medium-high | Accepted as a caveat. Note: the asymmetry (resident skips reload) is largely the *mechanism* of savings, not an error; a paired same-task test would attribute savings precisely → Round 6. |
| 2 | "Fresh" is cache-assisted (large cache_read in fresh cells) | medium | Accepted. ~$1.77 relabeled a *realistic-fresh* floor (mostly intra-agent caching), not zero-cache cold. Cross-agent cache sharing unverified → Round 6 probe. |
| 3 | n=1 per cell | medium | Accepted as a stated limitation; effect sizes keep direction robust. → Round 6 n≥5 on key cells. |
| 4 | Routing cells H0/H50 executionally soft | medium-low | Accepted. Routing *headline* rests on clean specialist segments and stands; H0/H50 numbers marked soft. → Round 6 clean re-run. |
| 5 | Fidelity probes narrow | low | Accepted. "No fidelity loss" qualified to "on probed facts." → Round 6 broader hidden probes. |

## 4. Verdict on re-running

**No conclusion is overturned; no full redo is warranted.** The data is verified clean
and an independent model reproduced the conclusions. Findings stand as
**directionally validated, large effect sizes, n=1**. Items 1-2 are significant enough
to justify an **optional Round 6** for causal precision (paired same-task tests +
cache-state control); items 3-5 are refinements. Round 6 would upgrade "directional" to
"controlled" without changing the practical guidance already encoded in the Steward
prompt. See `../README.md` → "Pending: optional Round 6".
