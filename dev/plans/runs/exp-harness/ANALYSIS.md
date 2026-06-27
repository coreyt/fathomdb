# Analysis — orchestrator build-vs-delegate, calibrated from rounds 1-5

Isolated worktree `exp/subagent-persistence`. Real billed tokens via
`parse_usage.py` over subagent `.output` JSONL. Opus rates. n=1 per cell
(point estimates, not distributions). Raw data: `data/round2-results.md`.
Independently reviewed by codex gpt-5.5 (high effort); see
`review/CODEX-REVIEW-OUTPUT.md` and `review/REVIEW-RESPONSE.md`. Caveats accepted from
that review are folded in below (search "CAVEAT").

## TL;DR

1. **Context persists** across re-addressing in every case (rounds 1-3: cross-file
   recall, verbatim line recall through 5 follow-ups incl. across a 6-min idle,
   domain recall). Mechanism = **resume-from-transcript**, not a live idle process.
2. **Fresh general-purpose subagent has a hard ~$1.77 floor** — boot cost (system
   prompt + tool schemas cache-written) paid even for a 1-line task. Dominant fact.
3. **Warm reuse ≈ $0.15–0.28/task (~6-12× under the spawn floor).** Scales with the
   resident's transcript size T (small payload $0.15; 60k-domain specialist $0.28).
4. **Warmth (Δt vs ~5-min cache TTL) is FIRST-order; overlap (H) is second-order.**
   A *warm* general resident reading from scratch ($1.04) beat a *cold* specialist
   answering from memory ($1.93).
5. **A cold/idle resident is a liability, not an asset.** Waking a cold specialist
   re-caches its whole transcript — it cost MORE ($1.93) than a fresh spawn did the
   entire job ($1.32). Held context only pays off while warm.
6. **Crossover K=2:** for ≥2 tasks over the same payload, a *warm* resident beats
   respawning. Margin grows with K.
7. **High-W (large output) delegation:** reuse $0.40-0.63 vs fresh $2.46 (4-6×), and
   the work output W lands in a disposable transcript instead of polluting the
   orchestrator context permanently.
8. **"Keep T small" VALIDATED but nuanced (round 4):** a ~9k distilled resident vs a
   ~60k raw one was 2.4× cheaper to load, 2× cheaper per warm query, 1.3× cheaper to
   cold-wake — with NO fidelity loss (targeted distillation kept every probed fact).
   BUT distilling-from-scratch cost $6.25 (> a raw load), so post-hoc distillation
   amortizes only after ~15 queries. The cheap way to small T is to **scope the
   initial load**, not load-big-then-distil.

## Accepted caveats (from independent review)

The codex re-parse of all 16 transcripts matched our data exactly (0/38 segment
mismatches), and its independent assume-correctness analysis reproduced every verdict
above (H1 supported, H2 warmth-dependent, H3 K=2, H4 validated, H5 refuted, same
decision rule). The following limitations are accepted and qualify the claims:

- **CAVEAT (task-equivalence).** Most "warm reuse vs fresh spawn" comparisons are not
  the *same task*: the warm follow-up answers a small held-context question while the
  fresh spawn also pays to load/read. This asymmetry is largely the *mechanism* of the
  savings (the resident's value IS skipping the reload), not a measurement error — but
  it means the numbers show "reuse-in-practice is cheaper," not a controlled
  "same-task-cheaper." A paired identical-task test (Round 6) would separate
  skip-reload savings from cache savings.
- **CAVEAT (fresh is cache-assisted).** "Fresh" cells carry large `cache_read` (e.g.
  r3-domain-fresh 146,855; r2-e1-154k 1,098,172) — mostly *intra-agent* prefix caching
  (an agent re-reading its own accreting context across turns), which is legitimate and
  unavoidable. So the ~$1.77 figure is a **realistic-fresh** cost, NOT a zero-cache cold
  start; a truly-first cold spawn (no shared system-prompt cache) could be higher.
  Unverified whether cross-agent system-prompt cache sharing occurs (Round-6 probe).
- **CAVEAT (n=1).** Every cell is a single trial — point estimates, no variance/CIs.
  Effect sizes (2-12×) make the *direction* robust, but exact crossovers are brittle.
- **CAVEAT (routing cells).** The E4/E6 H0 and H50 cells are executionally soft (H0
  used grep+partial read not a full 3-file read; H50 included a wrong-then-corrected
  answer that inflated its cost; domain-fresh had path retries). The routing *headline*
  (warm-specialist $0.28 cheapest, cold-specialist $1.93 dearest) rests on the **clean**
  specialist segments and stands; the H0 $1.04 / H50 $1.17 numbers are soft.
- **CAVEAT (fidelity scope).** "No fidelity loss" holds **only on the probed facts**
  (pyo3 version, feature cfg-gating, gil_used, test-hooks fns, exception hierarchy).
  Loss in unprobed facts is not ruled out — broader hidden probes are deferred.

## The cost model (calibrated)

Let P = payload tokens, K = tasks over that context, T = resident transcript size,
Δt = idle time since the resident last ran, W = task output tokens.

- **Fresh spawn:** `C_spawn(P) ≈ 1.77 + 0.02·(P/1k)` for single-read P (≤~60k tok);
  **superlinear** above ~60k where reads chunk (154k → $8.55). The $1.77 floor is
  fixed (boot), independent of work. (CAVEAT: this is a *realistic-fresh* floor that
  includes intra-agent cache reads, not a zero-cache cold start — see Accepted caveats.)
- **Resident load (sunk, once):** ≈ `C_spawn(P)`.
- **Resident follow-up cost ≈ f(T, Δt, W):**
  - **warm** (Δt < ~5 min): `~0.10·(T/10k) + W·$75/M`. Measured: $0.15 (T≈10k),
    $0.28 (T≈60k). ~98% cache hit.
  - **cold** (Δt > ~5-min TTL): re-caches ≈ half of T. Measured: $0.40 (T≈10k),
    $0.80 (mid), $1.93 (T≈60k). Hit ~40-50%.
  - **first-reuse after a completion** behaves cold (~the cold number), even at Δt≈0.
- **Wall-clock:** high variance (8-187 s), uncorrelated with cost. Do not use it.

A fixed **~80k boot baseline** (system prompt + tool schemas) rides in every resident
and is re-cached on every cold wake — so per-op savings from shrinking the payload are
real but bounded (round 4: cold-wake only 1.3× cheaper at T=9k vs 60k because both
re-cache the shared baseline; warm query 2× cheaper).

Two consequences that drive everything below:
- **Keep T small** (validated round 4: ~2× cheaper warm reuse, no fidelity loss) — but
  achieve it by **scoping the initial load**, not by loading big then distilling.
  Distillation costs ~$6 by ANY path (round 5: a raw-holder emitting its own summary
  cost $5.59, ≈ the $6.25 fresh distiller — no cheap shortcut), amortizing only after
  ~15-35 queries.
- **Keep Δt small** (ping < 5 min or batch): cold reuse costs ~5× warm and, for big
  T, exceeds a fresh spawn.

## Q3 — build-vs-delegate decision rule (calibrated deliverable)

Order of consideration matters: **warmth first, then overlap, then size.**

```
1. Trivial one-off, orchestrator has context headroom?
     → DO IT YOURSELF.  Never pay the $1.77 fresh-spawn floor for a one-off.
2. Is there a WARM resident (active < ~5 min) that already holds the needed context?
     → REUSE IT.  ~$0.15-0.28, the cheapest option by far.
3. No warm holder, but ≥2 tasks coming over the same large/ polluting context?
     → SPAWN A RESIDENT, load once, then KEEP IT WARM (ping/batch < 5 min) and reuse.
4. A holder exists but is COLD (idle > 5 min) and big (T large)?
     → treat it as NOT free.  If you need just one answer, a FRESH spawn can be
       CHEAPER than waking it.  Only wake it if you'll do ≥2 follow-ups to amortize.
5. Need isolated context once, no warm holder?
     → SPAWN FRESH once (pay the floor), or do it yourself if it won't pollute you.
RETIRE residents when T bloats: warm-reuse cost and the cold-wake penalty both grow
with T; above ~60k held tokens, reads chunk and costs go superlinear.
```

Specialist vs general (E6): a specialist wins **only when warm** — warm-specialist
$0.28 was the cheapest of all routes (4.7× under fresh), but cold-specialist $1.93
was the dearest. A specialist's value is high held overlap; that value is destroyed
by idle. So: prefer a warm specialist; never wake a cold big specialist for a
one-off; a warm general resident can beat a cold specialist.

## Q1 — complexity / "replay", answered (high-W now measured)

- "Replay" is not a subagent-only penalty. Every LLM turn reprocesses its whole
  context; caching makes the unchanged prefix cheap **while warm**. The asymmetric
  cost is **cache eviction on idle** — the resident's defining risk.
- High-W tasks (round 3): the work output W is the same inline or delegated
  (~2-3k tokens), but delegated it lands in a **disposable** transcript. Inline, that
  W is permanently in the orchestrator context and re-billed (cache-read) every
  subsequent orchestrator turn — the real, compounding reason to delegate high-W work.
- Compounding confirmed: warm-reuse cost rises with T; cold-wake penalty rises with T.
  Residents are not free to keep — they must earn their transcript.

## Q2 — agent-status file / Steward Orchestrator, scored against data

- **Inter-agent note-passing: do NOT build a file for it.** Star topology + SendMessage
  already cover it (confirmed: every delivery landed across ~20 resumes). A file would
  be pull-based, racy, unsignalled.
- **Status file as the steward's EXTERNALIZED registry: build it, conditionally.**
  Value = routing among residents without holding their state in the orchestrator
  context. The **`last_active` (warmth) column is now load-bearing**: round 3 proved
  warmth dominates overlap, so the steward MUST track Δt per resident and prefer warm
  holders — that tracking is exactly what the file is for. Add `transcript_tokens` (T)
  too, to enforce the retire-when-bloated and small-is-cheaper rules.
- **Value test (proposed):** identical multi-resident workload, steward holds-registry
  -in-context vs externalizes-to-file. Score: orchestrator context tokens (↓), routing
  accuracy = % tasks to the right WARM holder (↑), file-maintenance tokens (↓),
  staleness incidents (↓), post-compact recovery (bin). Prior: file pays past ~4-6
  residents or one compact.

## Failure modes observed

- **Intermittent safety-block artifact.** Several resumes and the 154k agent returned
  "SECURITY WARNING: auto mode could not evaluate ... blocked ... because of earlier
  conversation content." Work still completed correctly. Tied to the repeated
  resume+coordinator-message pattern, not the action. Operational risk: a steward must
  not over-react to spurious safety flags on resident output.
- **Coordinator-authority hygiene (good behavior).** Round-3 residents flagged that
  "the coordinator's relay carries no user authority" and completed only because the
  tasks were benign + in-role. A steward's instructions to residents are peer-level;
  residents correctly treat them as non-authoritative for sensitive actions.
- **No re-addressing failures, no silent context loss** across ~20 resumes.
- **Wall-clock variance** (8-187 s) makes latency useless as a control signal.

## Distillation test (round 4) — DONE
"Keep T small" validated for per-op cost (2.4× load, 2× warm query, 1.3× cold-wake),
no fidelity loss with targeted distillation; but distill-from-scratch ($6.25) only
amortizes past ~15 queries — so scope the initial load rather than post-distil. Full
numbers in `data/round2-results.md` (Round 4).

## Cheap-distillation break-even (round 5) — HYPOTHESIS REFUTED
Having a resident that already holds the raw files emit its own summary was expected
to be cheap (~$0.5). Measured: **$5.59** — ≈ the $6.25 fresh distiller. Distillation
is a ~$6 capital expense regardless of path, because (1) generating the summary is
high-W output (~$1.3+) and (2) the source must be in context to distil from
(read fresh OR reprocess a big transcript); piggybacking only saves the re-read.
Break-even: distil-from-scratch ~17 queries; distilling an already-loaded raw resident
~36 queries (worse). Fidelity of the piggyback summary was full. => no cheap shortcut;
**scope the initial load** instead. Full numbers in `data/round2-results.md` (Round 5).

## Deferred (next rounds)
- E5: orchestrator-context shadow price — $ of a /compact event (the benefit side of
  delegation, still only argued analytically).
- n>1 per cell for distributions; current cells are point estimates.
- Finer Δt sweep around the TTL (bracketed 0 vs 6 min only); 1h-ephemeral-cache behavior.
