# Analysis — orchestrator build-vs-delegate, calibrated from round-1+2 data

Isolated worktree `exp/subagent-persistence`. Real billed tokens via
`parse_usage.py` over subagent `.output` JSONL. Opus rates. n=1 per cell
(point estimates, not distributions). Raw data: `data/round2-results.md`.

## TL;DR

1. **Context persists** across re-addressing in every case (round-1 cross-file
   recall; round-2 line recall through 5 follow-ups incl. across a 6-min idle).
   The mechanism is **resume-from-transcript**, not a live idle process.
2. **A fresh general-purpose subagent has a hard ~$1.77 floor** — the boot cost
   (system prompt + tool schemas + deferred-tool list cache-written) — paid even
   for a 1k-token, one-line task. This is the dominant fact for build-vs-delegate.
3. **Warm resident reuse ≈ $0.15/task (~12× under the spawn floor).** Cold-idle
   reuse ≈ $0.80; first-reuse-after-completion ≈ $0.65–1.78 (scales w/ transcript).
4. **Crossover K=2:** for ≥2 tasks over the same payload, a resident beats
   respawning — warm or cold-spaced.
5. **Prompt cache (~5-min TTL) is the cost driver, not "replay."** Keep residents
   pinged < 5 min apart or pay ~5× per task.

## The cost model (calibrated)

Let P = payload tokens, K = tasks over that payload, T = resident transcript size.

- **Fresh spawn:** `C_spawn(P) ≈ 1.77 + 0.02·(P/1k)` for single-read P (≤~60k tok);
  **superlinear** above ~60k where reads chunk (154k → $8.55, cread 1.1M from
  re-caching across chunks). The $1.77 is a fixed floor (boot), independent of work.
- **Resident load (sunk, once):** ≈ `C_spawn(P)`.
- **Resident follow-up:**
  - warm (Δt < ~5 min): **~$0.15**, ~98% cache hit. Creeps up slowly with T.
  - cold (Δt > ~5 min TTL): **~$0.80**, ~49% hit (partial re-cache; 1h-cache survives).
  - first-reuse after a completion: **$0.65–1.78**, rising with T at completion.
- **Wall-clock:** high variance (8–84 s), uncorrelated with cost. Do not use for costing.

## Q3 — build-vs-delegate decision rule (the deliverable)

Inputs the orchestrator should weigh: P (payload), K (expected reuse), O (its own
context fullness / shadow price), H (overlap with an existing holder), Δt-since
that holder ran, T (holder transcript size).

```
if task is trivial AND P small AND O has headroom:
    DO IT YOURSELF.                      # never pay the $1.77 fresh-spawn floor for a one-off
elif an existing subagent holds ≥~50% of the needed context (H high) AND T not bloated:
    REUSE IT (b/c).                      # ~$0.15 warm, ~$0.80 if idle>5m — both << $1.77
    └ if you'll reuse again soon, keep it warm: ping < 5 min, or batch follow-ups.
elif K ≥ 2 over the same payload, OR P is large enough to pollute O:
    SPAWN A RESIDENT, load once, reuse.  # crossover K=2; margin grows with K
else  # K=1, no holder, must isolate from O:
    SPAWN FRESH once (pay the floor).
retire a resident when T bloats — first-reuse penalty & warm-cread both grow with T;
above ~60k held tokens, re-reads chunk and costs go superlinear.
```

Specialized vs general (c vs b): not yet measured (E6 deferred). Hypothesis: a
specialist raises effective H (already holds domain context/tools), shifting the
reuse branch earlier — to be tested.

## Q1 — complexity & the "replay" question, answered

- "Replay" is not a subagent-only penalty. **Every** LLM turn (orchestrator or
  subagent) reprocesses its whole context; caching makes the unchanged prefix cheap
  while warm. The asymmetric cost is **cache eviction on idle** (the resident sits
  idle between follow-ups; the orchestrator usually doesn't).
- This round used trivial tasks (W≈0 work tokens), so context compounded only via
  the transcript/cache, not via work output. **Still to do:** high-W tasks to see
  where work output accumulates — inline it pollutes O permanently; delegated it
  lands in a disposable transcript. That is the real orchestrator-context argument
  and it is not yet quantified (needs E-complex; see deferred).
- Compounding observed: warm-reuse cread crept 72456→75589 as T grew; first-reuse
  penalty scales with T. Residents are not free to keep forever.

## Q2 — agent-status file / Steward Orchestrator, scored against data

- **Inter-agent note-passing: do NOT build a file for it.** Topology is a star
  (subagents address "main"; orchestrator addresses by agentId). SendMessage is the
  push channel; a file would be pull (costs each agent context to read), racy, and
  unsignalled. Confirmed: every SendMessage delivery landed; the resident answered 5×.
- **Status file as the steward's EXTERNALIZED registry: build it, conditionally.**
  Its value is letting the steward route among N residents without holding their
  state in O (one line/agent: id, purpose, payload-held, T, last_active, freshness).
  This is the same shadow-price benefit as the reuse branch above.
- **Value test (proposed, not yet run):** identical multi-resident workload, steward
  holds-registry-in-context vs externalizes-to-file. Score: orchestrator context
  tokens at end (↓), routing accuracy = % tasks to the right warm holder (↑),
  file-maintenance tokens (↓), staleness incidents (↓), post-compact recovery (bin).
  Prior from this data: the file pays only past ~4–6 concurrent residents or one
  compact; below that the registry fits in O for free. The `last_active` column is
  load-bearing because of the 5-min TTL — the steward should prefer routing to a
  warm holder, and the file is where it'd track warmth.

## Failure modes observed

- **Intermittent safety-block artifact.** Several resident resumes and the 154k E1
  agent returned "SECURITY WARNING: auto mode could not evaluate ... blocked ...
  because of earlier conversation content." Work still completed correctly. Tied to
  the repeated resume + coordinator-message pattern, not the (trivial) action. A real
  operational risk for a long-lived-resident architecture: outputs may carry spurious
  safety flags that an automated steward must not over-react to.
- **No re-addressing failures, no silent context loss** in any of ~11 resumes.
- **Wall-clock variance** (8–84 s) makes latency an unreliable control signal.

## Deferred (next rounds)
- E-complex: high-W tasks → quantify orchestrator-context pollution (the core Q1
  benefit of delegation), $ of a compact (E5 shadow price).
- E4: overlap sweep H ∈ {0,25,50,100%} → calibrate the reuse threshold.
- E6: specialist vs general resident.
- Repeat n>1 for distributions; current cells are point estimates.
- Δt sweep finer around the TTL (this round bracketed 0 vs 6 min only).
