# EARP evaluation package

EARP is a developer-side, EVAL-ONLY experiment harness. It is off-wheel by
construction — maturin packages only the `fathomdb` module — and it never
gates FathomDB.

The implemented v1 platform provides strict configuration resolution, pinned
gold validation, IR-B metric parity, diagnostic and corpus-scale runners,
projection witnesses, deterministic comparison/sweep statistics, and an
opt-in, budget-guarded R2 answer arm. Its command entry point is:

```text
python -m eval.earp.cli validate <earp-config.json-or-yaml>
```

The schemas in `schema/` remain the v1 lock artifact. They define the strict
campaign configuration, result sidecar, per-query JSONL, frozen dataclasses,
run verdicts, and blocker vocabulary used by every runner component.

Versioned campaign inputs belong in `experiments/configs/earp/`. A completed
or blocked run writes its structured artifacts under
`experiments/runs/<run_id>/`; retain evidence according to the repository's
experiment-record policy before making a campaign claim.

See [`dev/design/earp.md`](../../../../dev/design/earp.md),
[`dev/plans/earp-foundation.md`](../../../../dev/plans/earp-foundation.md), and
[`dev/notes/earp-hitl-decisions.md`](../../../../dev/notes/earp-hitl-decisions.md).
