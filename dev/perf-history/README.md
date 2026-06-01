# `dev/perf-history/` — append-only canonical perf history

One JSON file per canonical / release-time performance run. The
`perf-regression-check` binary (`fathomdb-cli` crate) reads this directory,
groups records by `(ac_id, n)`, and flags regressions of the most-recent run
in each group against the rolling median of the prior runs.

**This directory is append-only.** The bin only ever *reads* it; CI (or a
human, for the locally-measured AC-013/AC-013b/AC-019 numbers) *writes* new
files. Never edit or delete an existing row — a regression is a new file
whose numbers the bin flags, not a mutation of history.

Filename convention: `<ac_id>-<commit_sha:8>-<n>.json`
(e.g. `AC-013-c893d8b0-10000.json`).

Schema, thresholds, and provenance: `dev/design/perf-regression-detection.md`.

```json
{
  "commit_sha": "c893d8b0",
  "ac_id": "AC-013",
  "n": 10000,
  "p50_ms": 36.0,
  "p99_ms": 49.0,
  "recall": 0.937,
  "timestamp": "2026-06-01T07:23:39Z"
}
```

Unknown fields are ignored (forward-extensible) — e.g. the optional `note`
field used to annotate provenance / artifacts is not interpreted by the bin.
`recall` is omitted/null for latency-only ACs (AC-012, AC-019);
`p50_ms`/`p99_ms` are omitted for recall-only ACs (AC-013b). `timestamp` must
be strict RFC3339 — an unparseable one is a data-integrity failure (exit 2),
because the parsed instant (not the raw string) selects the latest run.

The 2026-05-27 `AC-013b` arc records a known artifact: the batch-collapse bug
(`035cfa3`) read a *degenerate* recall=1.0 (it masqueraded as perfection), and
the fix (`4a95cfd`) revealed the honest 0.1572. See the honesty note in
`dev/design/perf-regression-detection.md` — a degradation detector flags the
*correction*, not the bug itself.
