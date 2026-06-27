# SCRATCH — Resident (warm) subagent reuse vs. fresh-spawn

> Untracked scratchpad. Raw data + first-pass interpretation from experiment run 1.
> A more complete analysis follows after additional data collection. Do NOT treat
> conclusions here as final.

## Purpose

Test whether an orchestration-driven session that must protect its own context can
keep **stand-by subagents** that:
- hold the bulk of the working context (files),
- do mechanical work and report distilled results back for orchestrator decisions,
- **stay active** across turns so the orchestrator re-uses one warm worker via
  `SendMessage` instead of paying full spin-up + context-load on every task.

Question under test: warm-reuse cost vs. cold-spawn cost, and whether
re-addressability + context retention actually hold in this harness.

## Setup (run 1)

Substrate files (trivial; the point is the plumbing, not the work):
- **A** `src/rust/crates/fathomdb-py/Cargo.toml` — 41 lines / ~1,696 B
- **B** `src/rust/crates/fathomdb-py/src/lib.rs` — 1,489 lines / ~60,432 B
- **C** `dev/plans/runs/STATUS-0.8.9.md` — 63 lines / ~5,800 B
- Total ≈ 1,592 lines / ~67,928 B

Procedure:
- Spawned a resident general-purpose subagent, had it read A/B/C in full, told it to
  retain and NOT re-read.
- Sent two successive `SendMessage` follow-ups (cross-references) WITHOUT re-supplying
  contents:
  - FU1: A↔B — which `[features]` flags in A are referenced via `cfg` in B.
  - FU2: C↔A — what pyo3 version A pins; does C mention pyo3 / dependabot work.
- Ran resident in BOTH background and foreground modes ("run both").
- Control: fresh general-purpose subagent that read all three from scratch and did the
  equivalent cross-references.

agentIds (this session): BG resident `a967909582618880d`; control `a6c353575207e974e`;
FG resident `aa213c7a6e25080ca`.

## Raw measurements

Wall-clock from each op's reported `duration_ms`; tokens from `subagent_tokens`.

| Operation | Subagent tokens | Wall-clock | Tool uses | Re-read? |
|---|---|---|---|---|
| BG resident — cold spawn + read 3 files | 53,373 | 36.8 s | 7 | yes (initial) |
| BG resident — warm follow-up 1 (A↔B) | 53,424 | 58.8 s | 7* | no |
| BG resident — warm follow-up 2 (C↔A) | 54,713 | 10.9 s | 0 | no |
| FG resident — cold spawn + read | 52,225 | 21.8 s | 4 | yes (initial) |
| FG resident — warm follow-up (A↔B + C↔A) | 51,695 | 7.5 s | 0 | no |
| CONTROL — fresh spawn, read + 2 cross-refs | 48,043 | 36.2 s | 6 | yes |

\* FU1 `tool_uses: 7` looks like a metric artifact (replayed/cumulative); it answered
correctly with no re-read. Trust `tool_uses: 0` cases + the agent's explicit statements.

Local Bash start-markers (epoch s), for cross-check:
- BG_RESIDENT_SPAWN_START 1782571704.389
- BG_FU1_START 1782571754.081
- BG_FU2_START 1782571810.220
- CONTROL_SPAWN_START 1782571868.190
- FG_RESIDENT_SPAWN_START 1782571873.835
- FG_FU_START 1782571909.626

## SendMessage behavior (key mechanism)

- FU1 result: *"Agent ... was stopped (completed); resumed it in the background with
  your message."*
- FU2 result: *"... had no active task; resumed from transcript in the background ..."*
- FG follow-up result: *"... had no active task; resumed from transcript ..."*

=> The resident is NOT a live idle process. It completes/stops after each task and is
**reconstituted by replaying its persisted transcript**. Context survives because the
files live in that transcript. Foreground and background behave identically for
re-addressing (both return an agentId; both resume-from-transcript).

## First-pass findings (provisional)

**(a) Retention:** YES — retained across both follow-ups, in both spawn modes. Strongest
proof: warm follow-ups with `tool_uses: 0` (no Read invoked). Answers were correct cross-
references. FG resident honestly disclosed its *initial* read truncated B at 1223/1489
lines (finished reading B before READY); no re-read after.

**(b) Cost — counter-intuitive:**
- `subagent_tokens` tracks the agent's WHOLE transcript size, not incremental work.
- Each resume **reprocesses the full ~52–55 K-token transcript** (which already holds all
  three files). A warm follow-up costs ~53 K tokens to answer one tiny question; the cold
  control read all three files AND did both cross-refs for ~48 K.
- Transcript **accretes**: 53,373 → 53,424 → 54,713. Each successive follow-up reprocesses
  a slightly bigger context. Crossover exists where a bloated resident > fresh narrow spawn.
- Wall-clock comparable + high-variance (warm 7.5–58.8 s; cold 21.8–36.8 s). No reliable
  speed win from warmth.

**Where the pattern DOES win:**
1. Orchestrator-context protection (real, large): re-address messages were ~50-word
   `SendMessage`s; orchestrator never ingested the 68 KB payload nor re-authored spawn
   prompts. This benefit held.
2. Likely $ discount from prompt caching (UNMEASURED): replayed transcript is mostly a
   cache hit, so reprocessed tokens bill below fresh tokens. Could not see cache-hit rates
   from notifications — this is the mechanism that would make warm reuse cheaper in dollars
   despite ~equal token COUNTS. NEEDS DIRECT MEASUREMENT.

**(c) Failure modes:**
- No re-addressing failure, no silent context loss. Every `SendMessage` succeeded; all
  warm answers correct.
- "Staying active" is a misnomer — completes/stops + transcript replay; pay full-transcript
  reload on every reuse (not amortized like a parked warm process).
- Unbounded transcript growth → residents get more expensive per follow-up over time;
  keep them task-scoped and retire before bloat erases the advantage.
- Metric noise in `tool_uses` (7 on a no-re-read follow-up).

## Open questions for next data-collection round

- Direct cache-hit / billed-$ measurement (the load-bearing unknown for the cost claim).
- Incremental vs. cumulative semantics of `subagent_tokens` — confirm it's context-size,
  not per-invocation billed tokens.
- Many-follow-up curve: how does per-follow-up cost grow as the transcript accretes? Where
  is the crossover vs. fresh narrow spawn?
- Larger / more numerous payload files (is 68 KB representative?).
- Effect of having the resident return only distilled state vs. holding raw files.
- Stability of wall-clock variance across more runs (n=1 per op here).
