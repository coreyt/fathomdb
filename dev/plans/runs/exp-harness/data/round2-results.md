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
