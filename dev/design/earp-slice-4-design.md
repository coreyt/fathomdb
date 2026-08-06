---
status: PROPOSED
---

# EARP Slice 4 — the durable writer

Design of record for S4 of `dev/plans/earp-foundation.md`. Depends on S3
(`ResolvedScenario`) and the S0 lock.

## Contract

S4 makes a partial run impossible to mistake for a complete one. It owns the
ordering — stage and validate the EARP sidecars, materialize the shared record,
append the index last — and the run identity that ordering depends on.

Pure apart from the filesystem: no SDK, no database, no network.

## Why the ordering needs a mechanism

`experiments/_lib.write_record` is a single call that mkdirs the run directory,
writes `record.json`, `config.resolved.yaml`, and `metrics.json`, and appends
`index.jsonl` — with **no hook between materialize and append**. The obvious
implementation (call it, then write the sidecar into the returned directory)
puts the sidecar *after* the index line: exactly the inversion the design of
record forbids.

The only way to honour the stated order is to pre-derive the identity:

1. `_lib.config_sha256(resolved)` → `_lib.make_run_id(experiment, ts, sha)` →
   `experiments/runs/<run_id>/`.
2. Stage and validate the sidecars there.
3. Call `write_record` with a **byte-identical** `config_obj` and the **same**
   `ts`, so it recomputes the identical `run_id` and materializes into the same
   directory.

Step 3's "byte-identical" is load-bearing, and the first thing to get wrong.

## A live defect this slice fixes

S3 computes `ResolvedScenario.config_sha256` with its own
`json.dumps(sort_keys=True, separators=(",", ":"))`. `_lib.canonical_json` uses
the same options **plus `ensure_ascii=False`**.

For an all-ASCII config the two agree, which is why S3's tests pass. For a
config containing any non-ASCII byte — a projection name, an accented path, a
note — they diverge. S4 would then stage into one run directory while
`write_record` materialized into another, leaving an orphaned sidecar and an
indexed run with no EARP evidence. That is the "drift silently produces a second
run directory" failure, live in landed code.

S4 removes the second implementation: `ResolvedScenario.config_sha256` is
computed by `_lib.config_sha256`, so there is exactly one canonicalisation. A
test pins it with a non-ASCII config, which is the only input that can tell the
two apart.

## Run-id collision

`_lib.make_run_id` is minute-resolution. Two runs with the same resolved config
inside one UTC minute collide: `write_record` suppresses the duplicate index
line but still **overwrites** `record.json` and `metrics.json` in place. For a
tool that offers `replay`, silently overwriting a prior run's evidence is a
durability defect.

S4 refuses. If the run directory already exists and carries a sidecar whose
`config_sha256` differs, that is `run_id_collision`. If the sidecar is
byte-identical, the write is idempotent and proceeds — a re-run of the same
config in the same minute is a no-op, not an error.

## Blocked runs

A blocked run is still durable evidence and is still indexed, but only with an
explicit `blocked` verdict and its blockers recorded. What must never happen is
a blocked run indexed as `complete`, or a blocked run left unindexed and
therefore invisible.

`Record.verdict` is an untyped `str`, so the writer accepts only the pinned
`RunVerdict` tokens and refuses anything else before touching the filesystem.

## Failure atomicity

Validation happens before any file is written. If sidecar validation fails, the
run directory is not created and the index is untouched. If `write_record`
itself raises after materializing but before appending, the result is an
unindexed run directory — the safe direction, since `index.jsonl` is the
source of truth and an unindexed directory is inert.

The writer does not attempt to make `write_record` atomic. It is not EARP's
file, and the failure mode it leaves is already the harmless one.

## What S4 writes

```text
experiments/runs/<run_id>/
  record.json              # _lib, closed schema
  config.resolved.yaml     # _lib
  metrics.json             # _lib
  earp.result.v1.json      # EARP sidecar, staged and validated first
  earp.per-query.v1.jsonl  # EARP per-query, one object per line
```

The sidecar is validated against `earp.result.v1.schema.json` using the S3
stdlib walker — the same one, not a second implementation. The per-query file is
validated line by line against `earp.per-query.v1.schema.json`.

The walker does not currently interpret `$defs`/`$ref`/`oneOf`, which the
*result* schema uses. S4 extends it with exactly those three keywords rather
than forking a second validator, and the `assert_supported` totality check
covers both schemas afterwards.

## Non-goals

- No run execution. S4 is handed results; producing them is S5/S6.
- No metric computation, no gold verification.
- No `INDEX.md` regeneration beyond calling `_lib.regen_index_md`, which owns
  its own format.

## Acceptance criteria

1. The sidecar exists on disk **before** the index line does, asserted by
   observing the filesystem between the two, not by asserting both exist.
2. `ResolvedScenario.config_sha256` equals `_lib.config_sha256` of the same
   resolved config, pinned with a non-ASCII config.
3. A pre-derived `run_id` equals the one `write_record` computes, so both write
   to the same directory.
4. A colliding run directory with a differing sidecar is refused as
   `run_id_collision`; a byte-identical re-write is idempotent.
5. A blocked run is indexed with the `blocked` verdict and its blockers; it is
   never indexed as `complete` and never left unindexed.
6. A verdict outside the pinned tokens is refused before any file is written.
7. Sidecar and per-query artifacts validate against their schemas; an invalid
   sidecar prevents the run directory from being created at all.
8. The stdlib walker interprets `$defs`/`$ref`/`oneOf` and remains total over
   both schemas.
9. Every path is exercised by a test that was first observed to fail.

## Review

Pending — an independent code-grounded review is required before
implementation, per the per-slice governance in the plan.
