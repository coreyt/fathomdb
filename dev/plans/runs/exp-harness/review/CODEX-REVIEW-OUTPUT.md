# Codex adversarial review: subagent-persistence orchestration economics

Scope note: I followed the quarantine rule. I did not open `ANALYSIS.md`,
`BEST-PRACTICES.md`, `STEWARD-PROMPT-SECTION.md`, `data/round2-results.md`, or
`EXPERIMENT-PLAN.md`. I read `review/EXPERIMENT-DEFINITIONS.md`,
`review/DATA-MAP.md`, `data/all-segments.csv`, every `data/parsed/*.json`,
spot-checked raw `data/transcripts/*.jsonl`, and inspected `parse_usage.py`.

## Task 1: design verdict

### Per-hypothesis soundness

- H1 Persistence: moderately sound for "can answer later without tool re-read" in this
  harness. The design directly observes follow-up transcripts and tool calls. It does
  not prove human-like memory fidelity beyond the probed facts, and "without re-reading"
  means no explicit file/tool read, not no hidden platform transcript replay.
  Confidence: medium-high.
- H2 Warm reuse is cheaper: partially sound. It measures real billed segment costs, but
  several comparisons are not equivalent tasks. A warm follow-up asks for one small fact
  while a fresh spawn often includes load/read and sometimes richer output. Global prompt
  cache effects also appear in fresh cells, so "fresh" is not necessarily cache-cold.
  Confidence: medium-low.
- H3 Amortization: partially sound for point-estimate crossover arithmetic, not for a
  stable general rule. n=1, no variance, task sizes differ, and some warm costs spike
  after idle. The design can compute crossovers for the measured workflows only.
  Confidence: medium-low.
- H4 Keep transcript small: comparatively sound. R4 compares a distilled resident and a
  raw resident on matched warm-up, fidelity query, and cold-wake query segments. It is
  still confounded by different load artifacts and output lengths, but the same probes
  were used. Confidence: medium.
- H5 Cheap distillation: weak as designed. It measures fresh distillation
  (`r4-distiller` seg0, $6.252796) and piggyback emission from a raw resident
  (`r4r5-raw-resident-and-emit` seg4, $5.593923), but does not isolate the marginal
  cost of "already holds raw context" from the transcript size and very large output.
  The emitted piggyback summary is not cheaper to query than the raw resident on the
  measured fidelity query. Confidence: low-medium.

### Design issues

- n=1 per cell is the largest statistical limitation. Every verdict is a point estimate;
  no variance, no confidence intervals, no retry robustness.
- The metric is reasonable but narrow: `message.usage` is a sound billing-source measure
  for model tokens, and `parse_usage.py` correctly prices input/cache-write/cache-read/
  output at parameterized Opus rates. It does not include orchestration overhead, wall
  time value, context-window risk, or failure/retry cost.
- Opus rates are load-bearing for dollar break-evens. Token-class comparisons are more
  robust than dollar comparisons because cache-write/output prices dominate.
- Cache-class splits are used coherently for billing, but experimental independence is
  weak. Many "fresh" tasks show large `cache_read` values, e.g. `r3-domain-fresh` seg0
  has 146855 cache-read tokens and a 0.7174 hit ratio (`data/all-segments.csv`), so a
  fresh subagent is not equivalent to an uncached fresh process.
- Payload realism is mixed. Synthetic p* payloads are controlled and useful for size
  sweeps, but line-recitation and synthetic vocabulary are easier than real engineering
  tasks. Domain files add realism, but probes are narrow factual lookups.
- Segmentation is plausible: prompt turns are initial non-meta user messages and
  coordinator `isMeta` user messages containing "sent a message"; tool-result user turns
  are excluded. That maps to the experiment definition, but it couples correctness to
  runner wording.
- H2 and H3 are vulnerable to task-equivalence confounds. Example: `r2-e1-10k` seg0
  reads a 10k payload and reports final line/count ($1.869612), while `r2-e2e3-resident`
  seg1 only answers one held line ($0.645143). That is a useful orchestration comparison,
  but not an apples-to-apples "same task" comparison.
- H4's "same information" premise is imperfect. The raw resident holds full source; the
  distilled resident holds a summary that may omit facts outside the probe set.
- The design cannot cleanly separate platform prompt-cache behavior from resident
  persistence. Warm resident savings are partly cache-read economics, not only avoided
  file I/O.
- Idle-gap tests are useful but single-gap only. R2 and R4 use roughly 6-7 minute gaps,
  enough to perturb a 5-minute cache, but not enough to map 1-hour behavior or longer
  resident lifetimes.

## Task 2: execution verdict

### Parser and arithmetic checks

- I independently re-parsed all 16 transcript files using the same boundary semantics
  described in `parse_usage.py` and compared against `data/all-segments.csv`. Result:
  38 CSV segment rows, 0 mismatches on `assistant_turns`, `input`,
  `cache_creation`, `cache_read`, `output`, or `est_cost_usd`.
- Spot-check arithmetic examples:
  - `r2-e2e3-resident` seg4 at transcript line 25: input 867, cache_write 37704,
    cache_read 37414, output 294. Cost =
    `(867*15 + 37704*18.75 + 37414*1.5 + 294*75)/1e6 = $0.798126`, matching
    `data/all-segments.csv`.
  - `r4r5-raw-resident-and-emit` seg4 at transcript line 36: input 1744,
    cache_write 194274, cache_read 395467, output 17759, cost $5.593923, matching
    the CSV.
  - `r4-distilled-resident` seg2 at transcript line 17: input 869, cache_write 2025,
    cache_read 109549, output 41, cost $0.218402, matching the CSV.

### Per-experiment checks

- R1 bg-resident executed the intended load plus two follow-ups. Segment 0 starts at
  `data/transcripts/r1-bg-resident.a967909582618880d.jsonl:1` and reads Cargo.toml,
  lib.rs, STATUS, plus an extra lib.rs read and `wc`. Follow-ups at transcript lines 26
  and 29 have no `Read` tools and answer from retained context. Costs: load
  $5.279096, FU1 $1.781602, FU2 $0.209095. Caveat: load did an extra lib.rs read and a
  shell count, so load cost is not a minimal read.
- R1 control-fresh executed a one-shot read and both cross-reference answers. Transcript
  line 1 uses three `Read` calls plus `grep`/`wc`; cost $3.747479. This is a valid
  no-reuse baseline for "both questions in one pass," and it is much cheaper than the
  bg-resident total $7.269793.
- R1 fg-resident executed load plus one follow-up. Segment 1 at transcript line 17 has
  no `Read` tool and answers pyo3/test-hooks from context. Costs: load $3.493984,
  follow-up $1.751594. It shows foreground residents can be re-addressed in this run.
- R2 E1 executed four fresh payload-size spawns. Costs were 1k $1.774670, 10k
  $1.869612, 61k $3.010734, 154k $8.552692. The 61k and 154k runs required multiple
  reads/chunking (`r2-e1-61k` and `r2-e1-154k` transcript line 1), so the size curve
  includes tool pagination behavior.
- R2 E2/E3 executed one resident load and five follow-ups. The only `Read` is in seg0
  (`data/transcripts/r2-e2e3-resident.a3c27f072e6707da5.jsonl:1`); follow-ups at
  lines 13, 17, 21, 25, and 29 use only `SendMessage`. Immediate reuse costs fall from
  $0.645143 to $0.158739 to $0.139891. After the idle gap, FU4 is $0.798126, then FU5
  returns to $0.158770. Timestamp gap from FU3 first_ts 15:32:37 to FU4 first_ts
  15:39:20 is about 6.7 minutes.
- R3 high-W executed. `r3-rw-highw-general` loads p40k at line 1 ($2.698234), then two
  no-read high-output analyses at lines 14 and 19 ($0.396021, $0.626864). Fresh high-W
  is `r3-highw-fresh` seg0 at line 1, $2.461600. Output sizes differ: reuse outputs
  2154 and 3214 tokens; fresh output is 2268 tokens.
- R3 routing executed but has anomalies. The specialist seg1 first H100 question
  (`r3-rs-specialist` line 21) uses no `Read` and costs $1.932814, which is more than
  domain fresh $1.321275. Specialist seg3 warm H100 (`line 40`) costs $0.283757. The
  H50 segment at line 26 reads `payloads/py.txt`, sends an incorrect 105-line answer,
  then corrects to 104 lines after `wc`; cost $1.171849. The H0 general at
  `r3-rw-highw-general` line 24 reads Cargo.toml and uses shell grep on lib.rs, not the
  full three domain files, and costs $1.044390. The fresh domain run first tries bad
  `dev/src/...` paths, then finds the correct source paths; cost $1.321275. These
  routing cells are useful but not clean.
- R4 distiller executed: `r4-distiller` transcript line 1 reads the three domain files,
  writes `payloads/distilled-domain.md`, and costs $6.252796. The transcript reports
  36,634 bytes; current `wc -c` confirms 36,634 bytes.
- R4 distilled-vs-raw residents executed on matched probes. Distilled resident segments
  cost load $1.910212, warm-up $0.653065, fidelity query $0.218402, cold-wake $0.818083.
  Raw resident segments cost load $4.561873, warm-up $1.019419, fidelity query
  $0.425802, cold-wake $1.081557. Follow-up transcripts for both have no `Read` tools.
- R4 fidelity: both residents preserved the probed facts. Ground truth from
  `src/rust/crates/fathomdb-py/Cargo.toml` lines 14-40 and `src/rust/crates/fathomdb-py/src/lib.rs`
  lines 62-68, 838-847, 1383-1396, and 1427: six features including `default`,
  pyo3 "0.29", only `test-hooks` cfg-gated, `gil_used = true`, test hooks
  `_configure_vector_kind_for_test`, `_write_vector_for_test`, `force_panic_for_test`,
  root `EngineError`, two-level leaves `KindNotVectorIndexedError` and
  `EmbedderNotConfiguredError`. The raw resident explicitly corrected the prompt's
  "five features" wording; the distilled resident listed the five non-default features
  "beyond default", which is acceptable only if default is intentionally excluded.
- R5 piggyback executed. Raw resident seg4 writes `payloads/distilled-piggyback.md`
  from held memory with no `Read` tools; cost $5.593923. The piggyback-loaded resident
  reads that summary (load $2.085804) and answers the fidelity query (seg1 $0.633359).
  Its fidelity answer is correct. Note: the piggyback query is more expensive than the
  raw resident's measured warm fidelity query ($0.633359 vs $0.425802), so it does not
  demonstrate cheaper subsequent queries in this cell.

### Data I would discard or mark weak

- Discard for clean H2/H3 routing inference: R3 H0 general vs fresh domain, because the
  general used a different read strategy (Cargo read plus grep) and the fresh run had
  path retries.
- Mark weak: R3 H50, because it includes a wrong answer and correction in the same
  segment, inflating cost and mixing execution quality with cost.
- Mark weak: R1 bg-resident amortization, because the resident load did extra reads and
  the control fresh answered both questions in one pass.
- Keep but label as point estimates: all other cells, due to n=1 and cache-state
  dependence.

## Task 3: reviewer memo

Highest-impact issues:

1. Task equivalence is the main threat. Several "warm vs fresh" comparisons differ in
   input work, output length, and tool strategy. Re-run H2/H3 with paired prompts:
   same exact question, same output budget, same permitted tools, one fresh and one
   warm resident.
2. Cache state is not controlled. Fresh spawns often have substantial cache reads, so
   the study is measuring "new agent in this global cache environment," not cold start.
   Re-run with explicit cache state labels: cold-cache fresh, warm-cache fresh,
   warm-resident immediate, warm-resident post-TTL, and post-1h.
3. n=1 makes crossovers brittle. Repeat each cell at least 5-10 times or until cost
   variance stabilizes, especially post-idle and high-output tasks.
4. H5 is not proven. Piggyback distillation cost is close to fresh distillation
   ($5.593923 vs $6.252796), and the piggyback summary query costs more than the raw
   warm query in the measured fidelity cell ($0.633359 vs $0.425802). Re-run H5 with
   matched summary sizes, multiple query types, and a separate "summary resident already
   loaded" condition.
5. Routing cells need stricter execution controls. Prevent path retries, require the
   same source files or same grep/read policy, and fail the cell if the agent corrects a
   factual answer after a tool check unless the correction is a separately measured
   retry.
6. Fidelity should be broader. The R4/R5 probes check a small set of facts. Add hidden
   probes across API methods, exception mappings, status-board facts, and negative
   questions to measure summary loss.

Recommended re-runs:

- Paired 10k line-retrieval: fresh vs resident for the exact same line lookup, K=1..10,
  immediate and post-idle.
- Paired domain lookup: fresh vs specialist first-use vs specialist warmed, using the
  exact same file-access policy and output template.
- H4 matrix: raw resident, fresh-distilled resident, piggyback-distilled resident, each
  already loaded, same fidelity query set, repeated after 0, 6, 30, and 65 minutes.
- H5 break-even: include one-time distillation cost, summary load cost, and per-query
  savings over at least 20 matched queries.

## Task 4: independent hypothesis test

Assumption for this section only: the design, execution, segmentation, and data are
valid, as required by the prompt.

### H1 Persistence

Verdict: supported for the measured prompts.

- R1 bg follow-ups at transcript lines 26 and 29 use no `Read` tools and cost
  $1.781602 and $0.209095 while answering from held A/B/C context.
- R2 resident follow-ups at lines 13, 17, 21, 25, and 29 use no `Read` tools. The
  post-idle FU4 after about 6.7 minutes still answers from held payload context
  ($0.798126), followed by rewarm FU5 at $0.158770.
- R3 specialist H100 follow-ups at lines 21 and 40 use no `Read` tools and answer held
  domain facts; costs $1.932814 and $0.283757.
- R4 raw and distilled residents answer fidelity and cold-wake queries without `Read`
  tools and with correct probed facts.

Confidence: medium-high. The evidence is strong for no explicit re-read and retained
task facts; it does not prove unbounded memory or complete source fidelity.

### H2 Warm reuse is cheaper

Verdict: supported in many cells, refuted in at least one important first-reuse cell,
therefore context-dependent rather than universally supported.

Supporting numbers:

- R2 10k fresh spawn costs $1.869612 (`r2-e1-10k` seg0). Warm resident follow-ups cost
  $0.645143, $0.158739, $0.139891, post-idle $0.798126, and rewarm $0.158770
  (`r2-e2e3-resident` seg1-5). All are cheaper than fresh.
- R3 high-output fresh costs $2.461600 (`r3-highw-fresh` seg0). High-W resident
  follow-ups cost $0.396021 and $0.626864 (`r3-rw-highw-general` seg1-2), cheaper even
  with similar or larger output.
- R4 same-query small-vs-raw follow-ups are cheaper when already warm than raw load or
  fresh distillation; e.g. distilled fidelity query $0.218402.

Counterexample:

- R3 specialist first H100 reuse costs $1.932814 (`r3-rs-specialist` seg1), more than
  domain fresh $1.321275 (`r3-domain-fresh` seg0). The later warmed specialist H100
  costs $0.283757 and is cheaper.

Confidence: medium. Warm reuse is often cheaper after the resident has been warmed, but
first reuse can be expensive because it may create/write cache for the resident
transcript.

### H3 Amortization / crossover

Verdict: supported for measured 10k and high-output workloads; refuted for R1 two-task
domain baseline; inconclusive as a general law.

Measured crossovers:

- R2 10k line tasks: resident load + FU1 = $2.518866 vs one fresh $1.869612, so K=1
  loses. Resident load + FU1 + FU2 = $2.677605 vs two fresh tasks $3.739224, so crossover
  is K=2 for this workflow. All five resident follow-ups total $3.774392 vs five fresh
  $9.348060.
- R3 high-W: resident load + one follow-up = $3.094255 vs one fresh $2.461600, so K=1
  loses. Resident load + two high-output follow-ups = $3.721119 vs two fresh
  $4.923200, so crossover is K=2.
- R1 bg-resident: load + two follow-ups = $7.269793 vs fresh one-pass for both
  questions $3.747479. No crossover by K=2 in that setup.

Confidence: medium-low. The arithmetic is clear for these rows, but the fresh baselines
are not consistently equivalent.

### H4 Keep transcript small

Verdict: supported for the measured R4 probes.

- Load: distilled resident $1.910212 vs raw resident $4.561873, saving $2.651661.
- Warm-up: distilled $0.653065 vs raw $1.019419, saving $0.366354.
- Fidelity query: distilled $0.218402 vs raw $0.425802, saving $0.207400.
- Cold-wake after idle: distilled $0.818083 vs raw $1.081557, saving $0.263474.
- Both residents answered the probed facts correctly, so the cheaper distilled path did
  not lose fidelity on this probe set.

Confidence: medium. Strong measured direction, narrow fidelity sample.

### H5 Cheap distillation

Verdict: mostly refuted or inconclusive, depending on the exact claim.

- Fresh distillation costs $6.252796 (`r4-distiller` seg0).
- Piggyback distillation from an already loaded raw resident costs $5.593923
  (`r4r5-raw-resident-and-emit` seg4). That is only $0.658873 cheaper, not obviously
  "cheap" compared with a large-output follow-up. It is also the single most expensive
  follow-up segment in the study.
- Piggyback summary load + fidelity query costs $2.719163 (`r5-piggyback-loaded`
  seg0-1). The query alone costs $0.633359, which is higher than the raw resident warm
  fidelity query $0.425802 and much higher than the fresh-distilled resident fidelity
  query $0.218402.
- Break-even using fresh-distilled query savings over raw warm query is about
  $6.252796 / ($0.425802 - $0.218402) = 30.15 warm fidelity queries. Using cold-wake
  savings is $6.252796 / ($1.081557 - $0.818083) = 23.73 cold-wake queries.
- Break-even using piggyback emit cost and fresh-distilled query savings is
  $5.593923 / $0.207400 = 26.97 warm fidelity queries, or
  $5.593923 / $0.263474 = 21.23 cold-wake queries. Using the measured piggyback query
  against raw warm query has no break-even because the piggyback query is more expensive
  ($0.633359 vs $0.425802).

Confidence: medium. The data does support that a resident can emit a correct summary
from held context, but not that doing so is cheap or rapidly repaid.

### Unnamed effects visible in the data

- First reuse can be a cache-write tax. R2 FU1 is $0.645143, then FU2/FU3 fall to
  $0.158739/$0.139891. R3 specialist first H100 is $1.932814, then later warm H100 is
  $0.283757.
- Idle gaps partially reset economics. R2 post-idle FU4 is $0.798126 vs immediate FU3
  $0.139891 and rewarm FU5 $0.158770. R4 distilled cold-wake is $0.818083 vs warm
  fidelity $0.218402; raw cold-wake is $1.081557 vs warm fidelity $0.425802.
- Output size can dominate but does not erase warm savings. High-W reuse with 2154 and
  3214 output tokens still costs only $0.396021 and $0.626864 versus fresh $2.461600.
- Fresh is often cache-assisted. Large `cache_read` appears even in fresh rows, e.g.
  `r2-e1-154k` seg0 has 1,098,172 cache-read tokens and `r3-domain-fresh` has 146,855.

### Data-implied decision rule

- Do the task in the orchestrator itself when the needed context is already in the
  orchestrator and the operation is a single small lookup or synthesis. The study does
  not directly measure orchestrator self-cost, so this rule is an assumption from the
  absence of a self baseline.
- Spawn fresh when K=1, the resident is not already loaded, and the task is small or can
  be done in one pass. Evidence: R2 K=1 fresh $1.869612 beats resident load+FU1
  $2.518866; R3 high-W K=1 fresh $2.461600 beats resident load+one high-W $3.094255;
  R1 fresh both-questions $3.747479 beats bg resident load+two FUs $7.269793.
- Reuse a warm resident when it already holds high-overlap context and the task is not
  the first cache-writing touch, especially for repeated or high-output work. Evidence:
  R2 FU2/FU3/FU5 around $0.14-$0.16; R3 high-W FUs $0.396021/$0.626864; R3 warm
  specialist H100 $0.283757.
- Expect and budget for a first-reuse and post-idle penalty. First reuse may be several
  times later warm cost (R2 $0.645143 vs $0.158739; R3 $1.932814 vs $0.283757). After
  roughly 6-7 minutes, a wake-up can cost about 4-5x an immediate warmed follow-up in R2
  ($0.798126 vs $0.158770).
- Keep resident transcripts small when fidelity requirements fit a summary. In R4 the
  smaller distilled resident is cheaper for load, warm-up, warm query, and cold-wake.
- Do not pay for distillation expecting quick repayment unless many future queries are
  likely. In these measurements, fresh distillation needs about 24-30 matched raw-vs-
  distilled queries to repay itself; piggyback distillation does not repay against the
  measured piggyback query because that query is more expensive than raw warm query.

