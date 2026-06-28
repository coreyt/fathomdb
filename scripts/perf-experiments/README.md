# 0.7.0 perf-experiments harness

Driver + per-lever + aggregator scripts that produce the data
required to lock HITL Q1 / Q2 / Q4 in
`dev/plans/0.7.0-perf-experiments.md`.

## Layout

```text
scripts/perf-experiments/
  README.md                         (this file)
  run-experiment.sh                 (driver; takes EXP_ID + LEVER_ID)
  collect-host-spec.sh              (host CPU/kernel/glibc/SQLite/rustc capture)
  run-ac012.sh                      (AC-012 only; respects AC012_CORPUS_N)
  run-ac020.sh                      (AC-020 only)
  run-ac013.sh                      (AC-013 only; respects AC013_CORPUS_N)
  parse-ac012-numbers.py            (extracts p50/p99/seed/n from harness stderr)
  parse-ac020-numbers.py            (extracts seq/conc/speedup from harness output)
  emit-output-json.py               (assembles per-experiment closure JSON)
  aggregate.py                      (walks dev/plans/runs/0.7.0-PERF-EXP-*.json; emits master table)
  ledger-check.sh                   (cross-checks proposed lever against do-not-retry ledger)
```

## Quick start

Dev-box pre-screen (cheap, fast, 24-core x86_64 — informational):

```bash
EXP_ID=W1.1 LEVER_ID=L-A0 AC012_CORPUS_N=100000 \
  bash scripts/perf-experiments/run-experiment.sh
```

Canonical CI (authoritative for verdicts):

```bash
gh workflow run perf-canonical.yml --ref <branch> \
  -f experiment_id=W1.1 \
  -f lever_id=L-A0 \
  -f ac012_corpus_n=1000000 \
  -f run_full_scale=true \
  -f perf_record=false
```

## Closure JSON convention

Each experiment writes
`dev/plans/runs/0.7.0-PERF-EXP-<EXP_ID>-output.json` per the schema
in `dev/plans/0.7.0-perf-experiments.md § Closure-JSON schema`.

## Aggregation

```bash
python3 scripts/perf-experiments/aggregate.py \
  --runs-dir dev/plans/runs \
  --out dev/plans/runs/0.7.0-perf-experiments-results.md
```

## Ledger discipline

Before adding a new lever, run
`bash scripts/perf-experiments/ledger-check.sh <LEVER_ID>`. The
script reads
`dev/notes/performance-whitepaper-notes.md § 5` and surfaces any
match — if matched, the lever's honest-retry argument must already
be recorded in
`dev/plans/0.7.0-perf-experiments.md § Lever taxonomy`.
