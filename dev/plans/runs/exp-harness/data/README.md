# Experimental data — subagent persistence study

All raw and derived data for rounds 1-5. Every cost number in `ANALYSIS.md` /
`round2-results.md` is reproducible from here.

## Layout

- `transcripts/<round-role>.<agentId>.jsonl` — **RAW source data**: the full subagent
  conversation transcript (JSONL) for each experiment agent, dereferenced from the
  harness's `.output` symlinks. Contains every turn with `message.usage` (real billed
  tokens incl. cache_creation / cache_read / 5m+1h ephemeral split).
- `parsed/<round-role>.<agentId>.json` — per-agent segmented billing, produced by
  `../parse_usage.py --json` (one segment per prompt turn).
- `all-segments.csv` — consolidated flat table of all 38 segments across all 16 agents
  (label, agentId, seg, tokens by class, cache_hit_ratio, est_cost_usd). Machine-readable.
- `round2-results.md` — the human-readable analysis tables (rounds 1-5).

Regenerate parsed/CSV at any time: `python3 parse_usage.py transcripts/<f>.jsonl --json`.
Rates are parameterized in `parse_usage.py` (Opus; ratios hold if model differs).

Total measured spend across persisted transcripts: **$76.61** (38 segments).
Note: the round-4 raw resident and its round-5 emit share ONE transcript
(`r4r5-raw-resident-and-emit`), so its later segments belong to round 5.

## Agent manifest (round → role → agentId)

| round | role | agentId | what it measured |
|---|---|---|---|
| 1 | bg-resident | a967909582618880d | persistence + 2 warm follow-ups (cross-file) |
| 1 | control-fresh | a6c353575207e974e | fresh read + 2 cross-refs (baseline) |
| 1 | fg-resident | aa213c7a6e25080ca | foreground re-address + 1 follow-up |
| 2 | e1-1k | a918cd606bb1a412d | cold-spawn cost, 1k payload |
| 2 | e1-10k | a38a01a0c234bb4c4 | cold-spawn cost, 10k payload |
| 2 | e1-61k | ab40b5b885e2fd4b4 | cold-spawn cost, 61k payload |
| 2 | e1-154k | abcaa6025cf6817d0 | cold-spawn cost, 154k payload (chunked) |
| 2 | e2e3-resident | a3c27f072e6707da5 | reuse accretion (5 FUs) + cache-expiry idle test |
| 3 | rw-highw-general | ab876ed807426307d | high-W reuse (2 FUs) + H=0 misroute |
| 3 | rs-specialist | a0a90b9bbfd8f208c | specialist routing: H100 cold/warm + H50 |
| 3 | highw-fresh | ad1c620c4e4d482df | high-W fresh control |
| 3 | domain-fresh | ab04f77bea3f9708e | domain-Q fresh baseline |
| 4 | distiller | ac13216e8c11e79e7 | one-time distillation cost (read 60k, write summary) |
| 4 | distilled-resident | a3c67385babef26d6 | T≈9k resident: load/warm/cold-wake |
| 4/5 | raw-resident-and-emit | a858ea43b4145cd9b | T≈60k resident (r4) + piggyback emit (r5) |
| 5 | piggyback-loaded | a130d192801d24338 | loads raw-resident's own summary + fidelity check |

## Payloads (inputs)
`../payloads/` — synthetic controlled-size files (p4k/p40k/p240k/p600k/py.txt) and the
two distilled summaries (distilled-domain.md = fresh-distiller; distilled-piggyback.md
= raw-resident self-emit). Real domain inputs (Cargo.toml, lib.rs, STATUS-0.8.9.md) are
tracked elsewhere in the repo / worktree.
