# Data map — experiments → raw data

Branch: `exp/subagent-persistence`. All paths are relative to the repo root, under
`dev/plans/runs/exp-harness/`. Pair this with `EXPERIMENT-DEFINITIONS.md`.

## ⛔ QUARANTINE — do NOT read these until you have finished your own analysis

These files contain the original author's interpretation, findings, best practices, and
recommended decision rules. Reading them before forming your own conclusions defeats the
purpose of an independent review. **Do not open them for tasks 1-4:**
- `ANALYSIS.md`
- `BEST-PRACTICES.md`
- `STEWARD-PROMPT-SECTION.md`
- `data/round2-results.md`
- `EXPERIMENT-PLAN.md` (contains scope decisions + some author expectations)

Everything you need is the definitions doc, this map, the raw transcripts, the parsed
data, and the harness — all listed below.

## Tooling

- `parse_usage.py` — segments a transcript JSONL by prompt turn and sums real billed
  tokens (`message.usage`) per segment + a $ estimate (Opus rates, parameterized at the
  top of the file). Run: `python3 parse_usage.py <transcript.jsonl>` (table) or
  `--json` (structured). You may re-derive every number yourself, change the rates, or
  parse the JSONL directly — `message.usage.{input_tokens, cache_creation_input_tokens,
  cache_read_input_tokens, output_tokens}` on each `type:"assistant"` line.

## Consolidated data

- `data/all-segments.csv` — every segment from all 16 agents in one flat table:
  `label, agentId, seg, seg_label, assistant_turns, input, cache_creation, cache_read,
  output, cache_hit_ratio, est_cost_usd`. Fastest way to see all numbers at once.
- `data/parsed/<label>.<agentId>.json` — per-agent segmented usage (same data, nested).
- `data/transcripts/<label>.<agentId>.jsonl` — RAW source transcripts (ground truth).
- `data/README.md` — data-layout manifest (safe to read; no conclusions).

## Inputs (what the agents read)

- `payloads/p4k.txt` ≈1k tok · `p40k.txt` ≈10k · `p240k.txt` ≈61k · `p600k.txt` ≈154k ·
  `py.txt` ≈2.5k (the "new file Y" for the overlap test).
- `payloads/distilled-domain.md` — the fresh-distiller summary (Round 4).
- `payloads/distilled-piggyback.md` — the raw-resident self-emitted summary (Round 5).
- Real domain inputs read in R1/R3/R4/R5: `src/rust/crates/fathomdb-py/Cargo.toml`,
  `src/rust/crates/fathomdb-py/src/lib.rs`, `dev/plans/runs/STATUS-0.8.9.md` (the latter
  is also embedded verbatim inside the relevant transcripts).

## Experiment → data file(s)

| experiment (def section) | transcript(s) (`data/transcripts/…`) | CSV label(s) |
|---|---|---|
| R1 bg-resident | `r1-bg-resident.a967909582618880d.jsonl` | r1-bg-resident |
| R1 control-fresh | `r1-control-fresh.a6c353575207e974e.jsonl` | r1-control-fresh |
| R1 fg-resident | `r1-fg-resident.aa213c7a6e25080ca.jsonl` | r1-fg-resident |
| R2 E1 (4 sizes) | `r2-e1-1k.*`, `r2-e1-10k.*`, `r2-e1-61k.*`, `r2-e1-154k.*` | r2-e1-1k/-10k/-61k/-154k |
| R2 E2 + R2 E3 | `r2-e2e3-resident.a3c27f072e6707da5.jsonl` (seg0=load, seg1-3=E2, seg4=post-idle, seg5=rewarm) | r2-e2e3-resident |
| R3 high-W reuse | `r3-rw-highw-general.ab876ed807426307d.jsonl` (seg0=load, seg1-2=high-W, seg3=H0 domain-Q) | r3-rw-highw-general |
| R3 high-W fresh | `r3-highw-fresh.ad1c620c4e4d482df.jsonl` | r3-highw-fresh |
| R3 specialist routing | `r3-rs-specialist.a0a90b9bbfd8f208c.jsonl` (seg0=load, seg1=cold-first H100, seg2=H50, seg3=warm H100) | r3-rs-specialist |
| R3 domain fresh baseline | `r3-domain-fresh.ab04f77bea3f9708e.jsonl` | r3-domain-fresh |
| R4 distiller (one-time) | `r4-distiller.ac13216e8c11e79e7.jsonl` | r4-distiller |
| R4 distilled resident (T≈9k) | `r4-distilled-resident.a3c67385babef26d6.jsonl` (seg0=load, seg1=warmup, seg2=warm-Q, seg3=cold-wake) | r4-distilled-resident |
| R4 raw resident (T≈60k) + R5 emit | `r4r5-raw-resident-and-emit.a858ea43b4145cd9b.jsonl` (seg0=load, seg1=warmup, seg2=warm-Q, seg3=cold-wake [R4]; seg4=self-emit summary [R5]) | r4r5-raw-resident-and-emit |
| R5 piggyback-loaded | `r5-piggyback-loaded.a130d192801d24338.jsonl` (seg0=load, seg1=fidelity-Q) | r5-piggyback-loaded |

## Reading the answers / fidelity

Each transcript's `type:"assistant"` turns contain the agent's reasoning and its replies
(including the text it sent back). For fidelity checks, the ground-truth facts are in the
real domain inputs (`lib.rs` etc.); compare an agent's answer turns against those.

## Notes / known caveats in the data (not conclusions, just facts to be aware of)

- The runner delivers each re-address as a `type:"user"`, `isMeta:true` message whose
  text begins "The coordinator sent a message…"; `parse_usage.py` treats those as segment
  boundaries. Tool-result `user` turns are NOT boundaries.
- Some segments carry an automated "auto mode could not evaluate / security" note in the
  runner's completion summary; the agents still produced answers (visible in-transcript).
- `r4r5-raw-resident-and-emit` is a SINGLE transcript spanning Round 4 and Round 5.
- n=1 per cell.
