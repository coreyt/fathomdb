# Round 2 — raw billed-token results

Harness: `parse_usage.py` (real `message.usage` from .output JSONL). Opus rates
($/M): input 15, output 75, cache_write 18.75, cache_read 1.50. n=1 per cell.

## Round-1 retroactive (real billing, segmented)

| op | turns | input | cwrite | cread | output | hit% | $ |
|---|---|---|---|---|---|---|---|
| BG resident: load (3 real files) | 15 | 13134 | 237973 | 332445 | 1619 | 57.0 | 5.279 |
| BG resident: FU1 (first reuse) | 2 | 4 | 93962 | 12870 | 6 | 12.0 | 1.782 |
| BG resident: FU2 (warm) | 2 | 4 | 2578 | 106832 | 6 | 97.6 | 0.209 |
| BG resident TOTAL | 19 | 13142 | 334513 | 452147 | 1631 | 56.5 | 7.270 |
| Control: fresh read+2 cross-refs (one shot) | 11 | 11023 | 167517 | 211777 | 1647 | 54.3 | 3.747 |
| FG resident: load | 9 | 11167 | 162010 | 132628 | 1198 | 43.4 | 3.494 |
| FG resident: FU1 (first reuse) | 2 | 232 | 90462 | 12684 | 439 | 12.3 | 1.752 |

Note: first-reuse cost scales with transcript size at completion (round-1 load was
15 turns/3 real files → FU1 was a 12%-hit $1.78 cold rewrite). Warm consecutive
reuse (FU2) = 97.6% hit, $0.21.

## E1 — cold fresh-spawn cost vs payload P (each its own agent)

| P (≈tok) | turns | input | cwrite | cread | output | hit% | $ |
|---|---|---|---|---|---|---|---|
| 1k  | 6  | 8047 | 83701  | 19331   | 741  | 17.4 | 1.775 |
| 10k | 6  | 5944 | 86510  | 84693   | 418  | 47.8 | 1.870 |
| 61k | 10 | 8918 | 130524 | 199726  | 1734 | 58.9 | 3.011 |
| 154k| 23 | 6841 | 347997 | 1098172 | 3705 | 75.6 | 8.553 |

- FIXED SPAWN FLOOR ≈ $1.77 even at 1k payload (subagent system prompt + tool
  schemas + deferred-tool list cache-written on boot: ~80–87k cwrite floor).
- Scales with P; 154k is superlinear because >256KB forces chunked reads (23 turns)
  → file re-cached across chunks (cread 1.1M). Single-read payloads (≤~60k tok)
  scale gently: ~$1.8 + ~$0.02/1k-tok-of-payload.

## E2 — resident reuse, accretion (one resident, 10k payload, Δt≈0 between FUs)

| seg | turns | input | cwrite | cread | output | hit% | $ |
|---|---|---|---|---|---|---|---|
| load (sunk) | 6 | 5944 | 86941 | 85096 | 357 | 47.8 | 1.874 |
| FU1 (first reuse) | 2 | 867 | 29705 | 42463 | 153 | 58.1 | 0.645 |
| FU2 (warm) | 2 | 867 | 768 | 72456 | 302 | 97.8 | 0.159 |
| FU3 (warm) | 2 | 867 | 479 | 73703 | 98 | 98.2 | 0.140 |

- Warm follow-up marginal cost ≈ $0.14–0.16 (≈12× cheaper than a $1.77 fresh spawn).
- First reuse is the dearest follow-up ($0.65 here; $1.78 round-1) — penalty scales
  with transcript size at completion.
- cread creeps up (72456→73703) as transcript accretes — gentle slope at this size.

## E3 — cache expiry (idle gap) — RESULT

Same resident, continued. ~372s (6.2 min) idle between FU3 and FU4.

| seg | turns | input | cwrite | cread | output | hit% | $ | note |
|---|---|---|---|---|---|---|---|---|
| FU2 (warm) | 2 | 867 | 768 | 72456 | 302 | 97.8 | 0.159 | warm |
| FU3 (warm) | 2 | 867 | 479 | 73703 | 98 | 98.2 | 0.140 | warm |
| FU4 (after 6.2min idle) | 2 | 867 | 37704 | 37414 | 294 | 49.2 | 0.798 | CACHE EXPIRED |
| FU5 (immediately after) | 2 | 867 | 487 | 75589 | 310 | 98.2 | 0.159 | re-warmed |

- Idle > ~5m TTL expires the cache → next reuse re-caches (cwrite 479→37704),
  hit 98%→49%, cost ×5.7 ($0.14→$0.80).
- Partial: only ~half re-cached (1h-ephemeral cache survived 6 min) → cold-resume
  ($0.80) still < fresh spawn ($1.77).
- One immediate reuse re-warms fully ($0.16).
- Wall-clock is NOT a signal: FU4 (post-idle) was the FASTEST op (8.2s) vs FU3 84s.
  duration_ms FU1..FU5 = 54.8 / 65.8 / 84.0 / 8.2 / 26.1 s.

## Crossover — resident reuse vs fresh respawn (10k payload)

Per-incremental-task: warm reuse $0.15, cold-idle reuse $0.80, fresh spawn $1.77.
For K tasks over the SAME payload context:
- RESPAWN (fresh each): K × $1.87
- RESIDENT: $1.87 (load+task1) + follow-ups
  - warm: + $0.65 (first) + (K-2)×$0.15
  - cold-spaced: + (K-1)×$0.80

| K | respawn | resident-warm | resident-cold-spaced |
|---|---|---|---|
| 1 | 1.87 | 1.87 | 1.87 |
| 2 | 3.74 | 2.52 | 2.67 |
| 3 | 5.61 | 2.67 | 3.47 |
| 5 | 9.35 | 2.97 | 5.07 |

=> Crossover at K=2: for ≥2 tasks on the same context, a resident beats respawning,
warm OR cold-spaced. Margin grows with K.

## Failure modes observed (round 2)
- Intermittent "SECURITY WARNING: auto mode could not evaluate ... blocked for safety
  ... because of earlier conversation content" on resident FU1/FU3 and the 154k E1
  agent. Work still completed correctly each time. Appears tied to the repeated
  resume + coordinator-message pattern in this session, not the (trivial) action.
- Resume wall-clock high-variance & rising (FU1 54.8s, FU2 65.8s, FU3 84.0s) — grows
  with transcript; tokens are the stabler metric.

## Agent IDs (round 2)
E1: 1k a918cd606bb1a412d · 10k a38a01a0c234bb4c4 · 61k ab40b5b885e2fd4b4 ·
154k abcaa6025cf6817d0. Resident: a3c27f072e6707da5.

---

# Round 3 — high-W, E4 overlap, E6 specialist

## High-W (large output W per task)

Resident R-W holds p40k (10k payload); fresh control reads it from scratch.

| op | turns | input | cwrite | cread | output(W) | hit% | $ |
|---|---|---|---|---|---|---|---|
| R-W load (sunk) | 7 | 10152 | 131101 | 35540 | 460 | 20.1 | 2.698 |
| R-W high-W FU1 (reuse) | 3 | 869 | 3228 | 107274 | 2154 | 96.3 | 0.396 |
| R-W high-W FU2 (reuse) | 3 | 869 | 10889 | 112407 | 3214 | 90.5 | 0.627 |
| high-W FRESH (one shot) | 7 | 6809 | 108749 | 100214 | 2268 | 46.4 | 2.462 |

- High-W reuse $0.40-0.63 vs fresh $2.46 → 4-6x cheaper. Same W (~2.2-3.2k output);
  reuse skips the re-read + the $1.77 spawn floor.
- High-W follow-ups cost more than trivial ones ($0.15) purely from output W
  (~2-3k out x $75/M ≈ $0.15-0.24) on top of cache-read of held context.
- Delegation's high-W value: that W lands in a disposable transcript, NOT in the
  orchestrator context where it would be reprocessed every future turn.

## E4 overlap (H) + E6 specialist vs general — domain-Q routing

Domain question (pyo3 version + cfg-gated feature). Routed four ways + H50.

| route | turns | cwrite | cread | output | hit% | $ | note |
|---|---|---|---|---|---|---|---|
| WARM specialist (H≈100) | 2 | 2538 | 121376 | 548 | 97.3 | 0.284 | answer from memory (seg3) |
| COLD specialist 1st-reuse (H≈100) | 3 | 96717 | 67540 | 67 | 40.9 | 1.933 | cold rewrite of 60k transcript (seg1) |
| WARM general misroute (H≈0) | 11 | 8906 | 491375 | 1349 | 97.7 | 1.044 | reads domain files; warm base |
| FRESH spawn baseline | 11 | 49786 | 146855 | 622 | 71.7 | 1.321 | reads + answers |
| specialist H≈50 (held + read Y) | 9 | 12325 | 527860 | 1810 | 97.6 | 1.172 | partial overlap (seg2) |
| specialist load (sunk) | 11 | 185151 | 213552 | 425 | 50.6 | 4.179 | seg0 |

Ranking (cheapest→dearest): warm-specialist $0.28 < warm-general-misroute $1.04
< fresh $1.32 < cold-specialist-first-reuse $1.93.

KEY FINDINGS:
- **Warmth (Δt) is first-order; overlap (H) second-order.** A warm general resident
  reading from scratch ($1.04) BEAT a cold specialist answering from memory ($1.93).
- **A cold specialist costs MORE to wake than a fresh spawn does the whole job**
  ($1.93 > $1.32): waking re-caches its entire held transcript. Held context is not
  free — it is a liability when idle.
- **Resident cost scales with transcript size T.** Warm reuse: $0.15 (10k payload
  resident, round 2) vs $0.28 (60k-domain specialist). Cold-first-reuse: $0.40 (10k)
  vs $1.93 (60k). => keep residents SMALL/distilled, and keep them WARM.
- Specialist (high held overlap) wins ONLY when warm: warm-specialist $0.28 is the
  cheapest of all routes (4.7x under fresh).

## Agent IDs (round 3)
R-W/general ab876ed807426307d · specialist a0a90b9bbfd8f208c ·
high-W-fresh ad1c620c4e4d482df · domain-fresh ab04f77bea3f9708e.

---

# Round 4 — distillation test ("keep T small" lever)

Same domain content held two ways: DISTILLED (~9k-token targeted summary) vs RAW
(~60k original files). Identical warm-up → warm query → 6.2-min idle → cold-wake.
Distilled summary produced by a one-time distiller agent.

| op | DISTILLED (T~9k) $ | RAW (T~60k) $ | ratio | hit% D/R |
|---|---|---|---|---|
| load (sunk) | 1.910 | 4.562 | 2.4x | 47.7 / 55.7 |
| warm-up (cold first-reuse) | 0.653 | 1.019 | 1.6x | 58.1 / 55.4 |
| warm query (measured) | 0.218 | 0.426 | 2.0x | 97.4 / 97.9 |
| cold-wake (after 6.2min idle) | 0.818 | 1.082 | 1.3x | 49.2 / 55.1 |

One-time distiller cost (fresh agent reads 60k + writes 36KB summary): **$6.25**.

FIDELITY: both answered ALL probes correctly, incl. the deep two-level exception
hierarchy (KindNotVectorIndexedError←VectorError, EmbedderNotConfiguredError←
EmbedderError), gil_used=true, the 3 test-hooks fns, all features + cfg-gating.
=> targeted distillation lost nothing the queries needed.

AMORTIZATION (distill-from-scratch):
- Raw path:      4.56 (load) + K·0.43
- Distilled path: 6.25 (distil) + 1.91 (load) + K·0.22
- Upfront premium = (6.25+1.91) − 4.56 = $3.60; per-query saving ≈ $0.21 warm / $0.26 cold.
- Crossover ≈ K=14–17 queries. So POST-HOC distillation pays only under heavy reuse.

FINDINGS:
- "Keep T small" VALIDATED for per-op cost: ~2.4x cheaper load, ~2x cheaper warm
  query, ~1.3x cheaper cold-wake — and NO fidelity loss with targeted distillation.
- BUT a fixed ~80k boot baseline rides in every resident, so the savings are bounded
  (cold-wake ratio only 1.3x because both re-cache the shared baseline).
- Distilling-from-scratch is EXPENSIVE ($6.25 > a raw load). The cheap way to get
  small T is to SCOPE THE INITIAL LOAD (read only what's needed), not load-big-then-
  distil. Post-hoc distillation is worth it only with >~15 downstream queries, OR when
  the summary is produced as a cheap by-product of an agent that already holds the raw.
- Risk: a query needing a detail the distillation dropped forces a re-read (expensive)
  or a wrong answer. Distillation must target the expected query distribution.

## Agent IDs (round 4)
distiller ac13216e8c11e79e7 · distilled-resident a3c67385babef26d6 ·
raw-resident a858ea43b4145cd9b.

---

# Round 5 — cheap-distillation break-even (hypothesis REFUTED)

Hypothesis (from round 4 deferred): having a resident that ALREADY holds the raw
files emit its own summary would be cheap (~$0.5, like a high-W follow-up), making
distillation worthwhile. TEST: raw resident (held 60k + 4 prior follow-ups) wrote a
33KB (~8k-tok) distilled summary FROM HELD MEMORY (no re-read); a fresh resident then
loaded it.

| op | $ | detail |
|---|---|---|
| piggyback EMIT (raw resident writes 33KB summary from memory) | **5.59** | output 17,759 tok; hit 66.9%; 9 turns |
| fresh distiller (round 4, reads 60k from scratch) | 6.25 | for comparison |
| piggyback summary load (fresh resident) | 2.09 | ~matches round-4 distilled $1.91 |
| piggyback first query (cold-first) | 0.63 | warm would be ~$0.22 |
| FIDELITY of piggyback summary | full | all probes correct incl. deep exception hierarchy |

REFUTED: emit cost $5.59 ≈ the $6.25 fresh distiller, ~11× the $0.5 estimate.

WHY distillation is ~$6 regardless of path (two unavoidable costs):
1. **Generating the summary is high-W output** — ~8-9k output tokens × $75/M ≈ $1.3+
   just for the text, plus the reasoning turns around it.
2. **The full source must be in context to distil from** — either read fresh
   (fresh-distiller) or reprocessed as a large held transcript (piggyback, cwrite
   194k + cread 395k here). Piggybacking only saves the re-READ, a small fraction.

BREAK-EVEN (Δq saving = $0.43 raw − $0.22 distilled = $0.21/query):
- No raw resident yet, distil-from-scratch: premium $3.60 → **~17 queries**.
- Raw resident already loaded (sunk), piggyback a smaller successor: cost $7.50
  → **~36 queries** (WORSE — you re-pay to generate + load).

CONCLUSION: there is no cheap-distillation shortcut. Distillation is a ~$6 capital
expense that amortizes only under heavy reuse (>~15-35 queries). The cheap way to a
small transcript remains: **scope the initial load** (read only what's needed).
Distil only when (a) you will query many times, OR (b) the summary is a deliverable
you needed anyway.

## Agent IDs (round 5)
emit on raw-resident a858ea43b4145cd9b (seg4) · piggyback-loaded a130d192801d24338.
