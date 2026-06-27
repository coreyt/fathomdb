# Analysis — orchestrator build-vs-delegate, calibrated from rounds 1-3

Isolated worktree `exp/subagent-persistence`. Real billed tokens via
`parse_usage.py` over subagent `.output` JSONL. Opus rates. n=1 per cell
(point estimates, not distributions). Raw data: `data/round2-results.md`.

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

## The cost model (calibrated)

Let P = payload tokens, K = tasks over that context, T = resident transcript size,
Δt = idle time since the resident last ran, W = task output tokens.

- **Fresh spawn:** `C_spawn(P) ≈ 1.77 + 0.02·(P/1k)` for single-read P (≤~60k tok);
  **superlinear** above ~60k where reads chunk (154k → $8.55). The $1.77 floor is
  fixed (boot), independent of work.
- **Resident load (sunk, once):** ≈ `C_spawn(P)`.
- **Resident follow-up cost ≈ f(T, Δt, W):**
  - **warm** (Δt < ~5 min): `~0.10·(T/10k) + W·$75/M`. Measured: $0.15 (T≈10k),
    $0.28 (T≈60k). ~98% cache hit.
  - **cold** (Δt > ~5-min TTL): re-caches ≈ half of T. Measured: $0.40 (T≈10k),
    $0.80 (mid), $1.93 (T≈60k). Hit ~40-50%.
  - **first-reuse after a completion** behaves cold (~the cold number), even at Δt≈0.
- **Wall-clock:** high variance (8-187 s), uncorrelated with cost. Do not use it.

Two consequences that drive everything below:
- **Keep T small** (distil, don't hoard raw files): every reuse pays ∝ T.
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

## Deferred (next rounds)
- E5: orchestrator-context shadow price — $ of a /compact event (the benefit side of
  delegation, still only argued analytically).
- n>1 per cell for distributions; current cells are point estimates.
- Finer Δt sweep around the TTL (bracketed 0 vs 6 min only); 1h-ephemeral-cache behavior.
- Distillation test: resident holding a 10k DISTILLED summary vs 60k raw files, same
  queries — quantify the "keep T small" lever directly.
