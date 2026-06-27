# Adversarial review — subagent-persistence orchestration economics

You are an independent, skeptical reviewer. Another agent ("the author") ran an
empirical study and drew conclusions from it. Your job is to scrutinize the study from
scratch and form your OWN view. Be adversarial: assume nothing is right until the data
shows it. You have no stake in the hypothesis being true.

## Hard rule — quarantine

Do NOT open any of these files (they contain the author's conclusions and would bias
you): `ANALYSIS.md`, `BEST-PRACTICES.md`, `STEWARD-PROMPT-SECTION.md`,
`data/round2-results.md`, `EXPERIMENT-PLAN.md`. If you accidentally see their contents,
say so. Everything you need is below plus the definitions doc, the data map, the raw
transcripts, and the harness.

## Where things are

Repo branch `exp/subagent-persistence`, directory `dev/plans/runs/exp-harness/`:
- `review/EXPERIMENT-DEFINITIONS.md` — procedures + the hypotheses (no results).
- `review/DATA-MAP.md` — maps each experiment to its raw data files; lists tooling.
- `data/all-segments.csv`, `data/parsed/*.json` — measured per-segment billed tokens & $.
- `data/transcripts/*.jsonl` — raw subagent transcripts (ground truth; real
  `message.usage` per turn).
- `parse_usage.py` — the harness that produced the parsed data (you may re-run, re-derive,
  or re-rate it; rates are parameterized at the top).

## What was studied (context)

An "orchestrator" wants to protect its own limited context. The proposal: keep "resident"
subagents on stand-by that hold bulk context and do mechanical work, re-addressing them
instead of re-spawning. A resident is spawned once and later "resumed" (its transcript is
replayed). Cost is measured as real billed tokens (cache-write / cache-read / input /
output) parsed from each subagent's transcript, at Opus rates. n=1 per cell.

## Hypotheses (a-priori claims to test — see definitions doc for full procedures)

- **H1 Persistence:** a resident answers later follow-ups from retained context, no re-read.
- **H2 Warm reuse is cheaper:** re-addressing a warm resident costs less per task than a
  fresh spawn.
- **H3 Amortization:** over K tasks on the same context, one reused resident beats
  spawning fresh per task beyond some crossover K.
- **H4 Keep transcript small:** a smaller-transcript resident is cheaper to reuse than a
  larger one holding the same information.
- **H5 Cheap distillation:** producing a distilled summary from a resident that already
  holds the raw context is cheap (priced like one large-output follow-up), making
  distillation an inexpensive way to obtain a small-transcript resident.

## Your tasks (do them in order; keep them clearly separated in your output)

**1. Verify the experimental DESIGN.** Independent of the data: does this design actually
test H1-H5? Identify confounds, missing controls, n=1 limits, metric validity
(is parsing `message.usage` a sound cost measure? are the cache-class splits used
correctly? are Opus rates load-bearing?), payload realism, segmentation correctness, and
any hypothesis that the design cannot cleanly isolate. Rate each hypothesis's design
soundness.

**2. Verify the EXPERIMENTS as executed.** Cross-check the definitions against the raw
transcripts and parsed data: did each agent actually do what the definition says? Are the
segment boundaries right? Do the `parse_usage.py` numbers match what you compute directly
from the transcripts (spot-check a few)? Are there execution anomalies (errors, retries,
re-reads where "no re-read" was claimed, mislabeled segments, the R4/R5 shared transcript)
that undermine specific cells? Flag any data you would discard.

**3. Write your feedback.** A concise reviewer's memo: the design/execution issues you
found, ranked by how much they threaten the conclusions, and what you'd change or re-run.

**4. Assume correctness, then test the hypotheses from the data.** NOW assume the design,
experiments, and data are valid. Using ONLY the measured data, evaluate each of H1-H5:
supported / refuted / inconclusive, with the specific numbers that drive your verdict.
Where the data reveals an effect the hypotheses did not name, state it. Derive the
practical decision rule the data implies for when an orchestrator should (a) do a task
itself, (b) spawn fresh, (c) reuse a warm resident, and how to manage residents — strictly
from the numbers. Do not consult the author's version; this is your independent analysis.

## Output format

- Task 1: design verdict — per-hypothesis soundness + a list of design issues.
- Task 2: execution verdict — per-experiment checks, spot-check arithmetic, data to discard.
- Task 3: reviewer memo — ranked issues + recommended changes/re-runs.
- Task 4: independent hypothesis test — per-hypothesis verdict with numbers, any unnamed
  effects, and the data-implied decision rule.
State confidence and assumptions throughout. Quote specific files/segments/numbers.
