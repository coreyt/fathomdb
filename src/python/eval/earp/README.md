# EARP evaluation package

EARP is a developer-side experiment harness (D-1). It is EVAL-ONLY and
off-wheel by construction — maturin packages only the `fathomdb` module — and
it never gates FathomDB (D-2).

## What is here now

`schema/` is the Slice 0 lock artifact: declarations only, no runner and no SDK
calls.

| File | Locks |
| --- | --- |
| `schema/earp.config.v1.schema.json` | Campaign configuration (strict) |
| `schema/earp.result.v1.schema.json` | Run sidecar, witness and blocker shapes |
| `schema/earp.per-query.v1.schema.json` | Per-query JSONL line |
| `schema/models.py` | Frozen dataclasses + pinned vocabularies |

The vocabularies are closed on purpose. `experiments/_lib.Record.verdict` is an
untyped `str`, so the run verdict tokens (`complete` / `blocked` / `failed`)
and the twelve blocker codes are pinned here rather than being spelled
differently in each slice.

## What comes next

The runner, resolver, metric port, adapters, and CLI. They are built *against*
these schemas — S3-S5 in `dev/plans/earp-foundation.md`, which fixes the
test-first order.

Durable run records belong under `experiments/runs/<run_id>/`, and versioned
campaign configurations under `experiments/configs/earp/`.

See [`dev/design/earp.md`](../../../../dev/design/earp.md),
[`dev/plans/earp-foundation.md`](../../../../dev/plans/earp-foundation.md), and
[`dev/notes/earp-hitl-decisions.md`](../../../../dev/notes/earp-hitl-decisions.md).
