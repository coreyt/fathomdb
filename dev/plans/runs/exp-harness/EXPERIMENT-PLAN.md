# Experiment plan — orchestrator build-vs-delegate cost model (round 2)

Isolated worktree: `exp/subagent-persistence` (branched from HEAD 4880f0d3).
All artifacts under `dev/plans/runs/exp-harness/`.

## Precondition P0 — measurement harness (REQUIRED before any cost claim)

`parse_usage.py <agent.output>` parses a subagent transcript JSONL and emits **real
billed tokens** per prompt-segment:
- `input` (fresh uncached input), `cache_creation` (cache write), `cache_read`
  (cache hit), `output`.
- Derived: `total_input_processed`, `cache_hit_ratio`, `est_cost_usd` (Opus rates,
  parameterized).
- Segments the accumulating `.output` by user-prompt turns so each follow-up is
  billed separately.

Rates (Opus 4.x public, $/M, parameterized in script):
input 15, output 75, cache_write_5m 18.75, cache_read 1.50.

Validation: run on round-1 files; resident file must segment into 3 ops
(task1 + fu1 + fu2).

## Scope decisions (bounded run; deferrals explicit)

RUN this round: P0 harness, E1, E2, E3 (keystone). DEFER: E4 (overlap/H*),
E5 (orchestrator shadow price), E6 (specialist) — need more design; noted in analysis.

Substrate: synthetic payload files of controlled size (precise P), generated in
`exp-harness/payloads/`. Avoids depending on real-file sizes.

### E1 — payload threshold (cold-spawn cost vs P)
4 fresh background agents; each reads ONE payload ∈ {1K, 10K, 60K, 150K tokens-ish
by bytes ~ 4 char/token → ~4KB/40KB/240KB/600KB... use byte targets} and returns a
trivial 1-line summary. Measure billed tokens (esp. cache_creation = read-in cost) +
duration. → curve: cold-spawn cost vs P; find where delegation overhead matters.
Byte targets (≈4 B/token): 4 KB(~1K tok), 40 KB(~10K), 240 KB(~60K), 600 KB(~150K).

### E2 — amortization curve (N follow-ups on one resident)
1 resident loads the 40 KB payload, then 6 sequential trivial follow-ups (Δt≈0,
warm). Harness segments the 6 follow-ups. Measure marginal billed tokens per
follow-up + cumulative + transcript growth. Compare reuse cost (1 load + N cheap
follow-ups) vs respawn cost (N × E1-cold for same payload; validate with 2 real
respawns). → crossover N* and accretion slope.

### E3 — cache-warmth penalty (KEYSTONE)
1 resident loads the 40 KB payload. Follow-up #1 at Δt≈0 (warm). Then idle past the
~5-min cache TTL (background `sleep 360`), follow-up #2 at Δt≈6 min (cold). Compare
cache_read vs cache_creation on the two follow-ups. → quantifies the stand-by idle
penalty; decides whether "resident on standby" is real or a warm-cache illusion.

## Decision-model parameters each experiment feeds
- E1 → P* (payload threshold for delegation)
- E2 → N* (reuse-vs-respawn crossover), accretion slope
- E3 → cold-resume penalty as f(Δt vs TTL)
- (E4 → H*, E5 → orchestrator shadow price, E6 → specialist premium) — deferred

## Instrumentation notes / threats to validity
- `.output` accumulates across resumes → segment by prompt turn (harness handles).
- subagent model may differ from Opus → $ is rate-parameterized; ratios are the
  robust signal.
- n=1 per cell this round → treat as point estimates, not distributions.
- wall-clock has high variance (round 1: 7.5–58.8 s) → tokens are the primary metric.
