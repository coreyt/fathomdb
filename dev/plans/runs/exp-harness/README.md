# Subagent-persistence study — orchestration economics

**Branch:** `exp/subagent-persistence` (not merged; no PR). **Status:** complete,
independently reviewed, directionally validated. One optional follow-up (Round 6) is
pending and described at the bottom.

## What this work was about

Question: can an orchestrator that must protect its own limited context offload
mechanical work to **stand-by ("resident") subagents** that hold bulk context and get
**re-addressed** later (instead of being re-spawned), and is that actually cheaper?

A resident is spawned once; re-addressing it "resumes" it by replaying its persisted
transcript. We measured the **real billed token cost** (cache-write / cache-read /
input / output, at Opus rates) of every operation by parsing each subagent's transcript,
across five rounds manipulating payload size, reuse count, idle time, output size,
context overlap, specialist-vs-general routing, and distillation.

### Hypotheses tested (a-priori)
H1 persistence · H2 warm reuse cheaper · H3 amortization/crossover · H4 keep transcript
small · H5 cheap distillation. (Definitions: `review/EXPERIMENT-DEFINITIONS.md`.)

### Headline findings (see ANALYSIS.md for the full, caveated version)
- **H1 supported** — residents answer later follow-ups from retained context, no re-read.
- **Fresh-spawn floor ≈ $1.77**; **warm reuse ≈ $0.15–0.28** (~6–12× cheaper).
- **Warmth is first-order, overlap second** — a *cold* resident can cost MORE to wake
  than a fresh spawn ($1.93 vs $1.32). Keep residents warm (cache TTL ~5 min) or retire.
- **Crossover K=2** — reuse beats re-spawning from the 2nd task on the same context.
- **Keep transcript small** (scope the load) — ~2× cheaper per query, no fidelity loss
  on probed facts. **Distillation is ~$6 by any path** (cheap-distillation **refuted**).

## Where to find things

| Path | What |
|---|---|
| `ANALYSIS.md` | Full analysis: cost model, decision rule, per-hypothesis findings, **accepted caveats**. Start here. |
| `BEST-PRACTICES.md` | Distilled do/don't + decision order + confidence caveats. |
| `STEWARD-PROMPT-SECTION.md` | Drop-in prompt section for a Steward Orchestrator agent. |
| `data/round2-results.md` | Human-readable result tables for all 5 rounds + break-even math. |
| `data/all-segments.csv` | Machine-readable: all 38 measured segments (tokens by class, $). |
| `data/transcripts/*.jsonl` | **Raw source data** — 16 full subagent transcripts. |
| `data/parsed/*.json` | Per-agent segmented billing. |
| `data/README.md` | Data manifest: agentId → round/role mapping. |
| `parse_usage.py` | The measurement harness (parses transcripts → billed tokens/$). |
| `payloads/` | Inputs: synthetic size-controlled files + the two distilled summaries. |
| `EXPERIMENT-PLAN.md` | Original plan/scope decisions. |
| `review/` | Independent adversarial review package + results (below). |

### The independent review (`review/`)
- `ADVERSARIAL-REVIEW-PROMPT.md` — the prompt given to codex (hypothesis + design only;
  quarantines the conclusion docs so the review is unbiased).
- `EXPERIMENT-DEFINITIONS.md` / `DATA-MAP.md` — procedures (no results) + data links.
- `CODEX-REVIEW-OUTPUT.md` — codex gpt-5.5 (high effort) review. It re-parsed all 16
  transcripts (**0/38 mismatches**) and independently reproduced our conclusions.
- `REVIEW-RESPONSE.md` — our response + the caveats we accepted and folded into the docs.
- `codex-run.log` — raw codex run output.

## Reproducing the numbers
From this directory: `python3 parse_usage.py data/transcripts/<file>.jsonl` (table) or
`--json`. Rates are parameterized at the top of `parse_usage.py`; ratios hold if the
model/rates differ. The CSV is regenerable from the parsed JSON.

## Confidence
**Directionally validated, n=1.** Data arithmetic independently verified. Effect sizes
(2–12×) make directions robust; exact crossovers are point estimates. Key accepted
caveats: "fresh" is realistic-fresh not zero-cache cold; warm-vs-fresh aren't identical
tasks (the resident legitimately skips the reload); "no fidelity loss" = on probed facts.
Full list in `ANALYSIS.md` → "Accepted caveats".

---

## Pending: optional Round 6 (causal-precision upgrade)

Not required — no conclusion is in doubt — but it would upgrade "directional" to
"controlled." Run it only if you want publication-grade rigor.

**What to run** (≈10–15 agents; reuse `parse_usage.py` unchanged):
1. **Paired identical-task fresh-vs-warm** — same exact question, same output budget,
   same permitted tools, K=1..10, immediate + post-idle. Isolates skip-reload savings
   from cache savings (addresses the task-equivalence caveat).
2. **Cache-state probe** — spawn two identical fresh agents back-to-back; check whether
   the 2nd shows lower system-prompt cache-write (cross-agent sharing?). Settles whether
   the $1.77 floor understates a truly-cold spawn.
3. **Clean routing re-run** — repeat E4/E6 (specialist vs general, overlap H) with a
   strict file-access policy (no grep/partial-read mixing, no path retries); fail any
   cell that corrects a fact after a tool check.
4. **n≥5** on the load-bearing cells (warm reuse, first-reuse, cold-wake) for variance/CIs.
5. **Broader hidden fidelity probes** across API methods, exception mappings, status-board
   facts, and negative questions, to bound distillation loss.

**How to proceed** (same protocol as rounds 1–5):
- Work on this branch (or a child). Spawn background general-purpose subagents; drive
  follow-ups via SendMessage by agentId; copy each agent's `.output` transcript into
  `data/transcripts/` (use `cp -L`); run `parse_usage.py` to bill it; append a "Round 6"
  section to `data/round2-results.md` and regenerate `data/all-segments.csv`.
- Keep residents warm by batching follow-ups < 5 min apart; insert explicit ~6-min idle
  gaps only where you intend to test cache expiry.

**What to do with the results:**
- If Round 6 **confirms** the current orderings (expected): update `ANALYSIS.md`/
  `BEST-PRACTICES.md` to drop the n=1 / task-equivalence / cache-state caveats and label
  the findings **controlled**; note any corrected absolute numbers (esp. the true cold
  floor). The decision rule and Steward prompt likely need no change.
- If Round 6 **contradicts** a result (e.g., paired same-task reuse is NOT cheaper, or
  cross-agent cache makes the cold floor much higher): treat that cell's conclusion as
  overturned, correct the affected claim in all three docs + the Steward prompt, and run
  a fresh adversarial review (reuse `review/ADVERSARIAL-REVIEW-PROMPT.md`) on the new data.
- Either way, record outcomes in a new `review/ROUND6-NOTES.md` and re-run the codex
  review for an independent check before declaring the study controlled.
