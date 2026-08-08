---
status: COMPLETE
---

# EARP Slice 4 — the durable writer

Design of record for S4 of `dev/plans/earp-foundation.md`. Depends on S3
(`ResolvedScenario`) and the S0 lock. **Revision 2** — revision 1 was returned
REWORK; § Review records what changed.

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

`run_id = slug(experiment) + minute + sha256(config)[:8]`, so a collision needs
the **same experiment, same UTC minute, and same config hash**. Revision 1 keyed
the refusal on a differing `config_sha256`, which was exactly backwards: in a
real collision the hashes are equal *by construction*, and the branch was
reachable only through an 8-hex prefix collision.

The collision that actually happens is a same-config re-run inside one minute,
whose sidecar is **not** byte-identical — different metrics, witnesses,
timestamps, `embedder_download_ms`. Revision 1 matched neither branch and left
it unspecified, so `write_record` would do precisely what this slice exists to
prevent: `mkdir(exist_ok=True)` then overwrite `record.json`/`metrics.json`
while suppressing the second index line.

The rule is therefore on **bytes**, not on the config hash:

- Run directory exists and the staged sidecar differs in any byte →
  `run_id_collision`.
- Byte-identical → idempotent, proceed.

Equal `config_sha256` with differing measurements is a **collision**, not an
idempotent re-write.

The directory is claimed with `mkdir(parents=True, exist_ok=False)` — one
atomic syscall — rather than an exists-check followed by a write, which would
be TOCTOU-racy. The index-append dedupe inside `_lib` races independently and
S4 cannot fix it from outside; that is an accepted, documented limitation for
single-writer use.

### A collision-blocked run is the one run that is not indexed

A collision blocker is by definition a run whose `run_id` is already taken. It
cannot be indexed under that id, and it cannot write its sidecar into the
colliding directory without performing the overwrite the policy forbids. So it
**returns** `Blocker(code=RUN_ID_COLLISION, stage="writer.stage")` and writes
nothing — the sole named exception to "a blocked run is still indexed" — with
the remedy in the message (advance `ts` by a minute, or change the config).
Returned, never raised, matching S1/S2/S3.

## Blocked runs

A blocked run is still durable evidence and is still indexed, but only with an
explicit `blocked` verdict and its blockers recorded. What must never happen is
a blocked run indexed as `complete`, or a blocked run left unindexed and
therefore invisible.

`Record.verdict` is an untyped `str`, so the writer accepts only the pinned
`RunVerdict` tokens and refuses anything else before touching the filesystem.

## Ordering, precisely

Revision 1 said both "stage the sidecars in the run directory" and "validate
before any file is written", which cannot both hold — staging into the
directory creates it. The pinned order:

1. **Serialize** the sidecar and per-query lines to text in memory.
2. **Validate** by parsing that text back and checking the parsed values, not
   the object graph. Validating the serialized form is what makes the file on
   disk trustworthy rather than the object that produced it.
3. **Claim** the run directory with `mkdir(exist_ok=False)`; on `FileExistsError`
   run the byte-comparison collision check.
4. **Write** the sidecars.
5. Call `write_record`, which materializes the shared record and appends the
   index last.
6. Call `regen_index_md` **after** the append, per the standing rule that
   `INDEX.md` is generated from `index.jsonl`.

So an invalid sidecar prevents the directory from being created at all, and the
sidecar is on disk before the index line exists.

If `write_record` raises after materializing but before appending, the result
is an unindexed run directory — the safe direction, since `index.jsonl` is the
source of truth and an unindexed directory is inert. S4 does not try to make
`write_record` atomic: it is not EARP's file, and its failure mode is already
the harmless one.

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

Revision 1 said the walker needed "exactly three" new keywords. Measured
against the real files, that understates the work by about half:

| schema | uninterpreted today |
| --- | --- |
| `earp.config.v1` | none — passes |
| `earp.result.v1` | `$defs` ×1, `$ref` ×10, `oneOf` ×2 |
| `earp.per-query.v1` | **`allOf` ×1, `if` ×2, `then` ×2** |

The per-query schema was not mentioned at all in revision 1, yet its
`if`/`then` is exactly the logic making `outcome: scored` carry numbers.
Skipping it would silently under-validate the file this slice claims to
validate.

Three further mechanisms the "three keywords" framing hid:

- **`additionalProperties` as a schema.** The walker honours only
  `additionalProperties: false`. The result schema uses it with a `$ref` value
  for `metrics.per_k`, `document_metrics`, and `per_class` — so adding `$ref`
  alone would leave every per-K aggregate **completely unvalidated**, which is
  the highest-value part of the sidecar.
- **Union types.** `_type_ok` takes a single `str`; given a list it falls
  through and returns `True`. `["string", "null"]`-style unions appear 9 times
  in the result schema and 11 times in the per-query schema, so every one of
  those slots currently accepts anything at all, including a dict where a
  number belongs.
- **`assert_supported` recursion is too shallow to be a totality check.** It
  descends only into `properties` and `items` — never `$defs`, `oneOf`
  branches, `additionalProperties` schemas, or `if`/`then`. Since the result
  schema puts nearly all its structure inside `$defs`, adding keywords without
  extending the recursion buys a totality claim that is vacuous.

`oneOf` also needs a stated error rule, because the walker collects every
defect by contract and one `oneOf` branch must always fail: if any branch
yields zero findings, emit none; otherwise emit the findings of the branch with
the fewest, tagged with its index.

So the extension is `$defs`, `$ref` (fragment-only `#/$defs/<name>`, with a
cycle guard), `oneOf`, `allOf`, `if`/`then`, `additionalProperties`-as-schema,
and list-valued `type` — with `assert_supported` recursing into all subschema
positions and a test asserting it passes on **all three** schemas.

## What S4 is given

`write_record` requires ten arguments and `Record` is closed both ways, so
every one needs a stated source. Revision 1 named two.

| Argument | Source |
| --- | --- |
| `experiment` | **An S4 parameter.** The config schema has no name/title key, so the run-id slug cannot come from the config — and it is not covered by `config_sha256`. S6 replay needs it from the prior record. |
| `ts` | Caller-supplied, and **required to be UTC**: `_ts_compact` stamps a literal `Z` without converting, so a naive or local `ts` yields a `run_id` that lies about its timezone. |
| `config_obj` | The **raw config document** — the same object S3 hashed. Not `ResolvedScenario`: that hashes differently *and* fails destructively, because `asdict` keeps the enums, `record.json` writes fine, and then `yaml.safe_dump` raises on `RetrievalMode`, leaving a half-materialized directory. |
| `metrics` | Plain JSON, so `MetricValue`/`ClassAgg` are `asdict`-ed first. |
| `verdict` | A pinned `RunVerdict` token, refused otherwise before any file is touched. |
| `read` | The honest one-line finding, per the `experiments/` standing rules. |
| `code`, `env` | **Injected parameters, not gathered.** `_lib.git_info()` shells out and raises outside a git repo, and gathering would break this slice's purity claim. The caller supplies them. |
| `corpus` | `_lib.Corpus` has no `snapshot_sha256` slot. The snapshot hash lives in the sidecar's `corpus_identity`; it is **not** squeezed into `manifest_sha256`, which D-6.2 forbids conflating. Both identities already come from S1. |
| `seeds` | Caller-supplied; empty for a diagnostic run. |
| `cost_usd` | Load-bearing for S9's budget preflight, which sums it across `index.jsonl`. |
| `tdd_evidence` | Accepted and forwarded — the plan's governance requires RED/GREEN evidence in this field. |
| `n`, `headline` | Feed `INDEX.md` columns and are silently empty if unset, so S4 passes them. |
| sidecar path | Recorded in `artifacts`, the only slot in the closed record that can hold it. |

### The experiments root is a parameter, not a constant

`write_record` defaults `base_dir` to the real `experiments/` directory, and
`regen_index_md` defaults `md_path` to the real committed `INDEX.md`. A test
that forgets either writes into the repo — and one that passes only
`index_path` would **overwrite the committed `INDEX.md` from a tmp index**.

So `experiments_root` is threaded explicitly through derivation, staging,
`write_record`, and `regen_index_md` (both `index_path` and `md_path`
together). Pre-derivation must use the same root it later passes: that is the
one genuine path by which the pre-derived and materialized directories could
differ.

### Reaching `_lib` at all

`experiments/` is a repo-root package; `eval/` lives under `src/python`, and
pytest's `pythonpath = ["."]` adds only `src/python`. The one existing importer
inserts the repo root on `sys.path` by hand, and the repo's test script happens
to run from the root with `python -m`, so the import resolves by accident.

S4 adds one explicit resolution module, `eval/earp/_experiments.py`, which puts
the repo root on `sys.path` and re-exports `_lib`, with a test that imports it
from a subprocess whose cwd is *not* the repo root. This makes the
boundary decision explicit: off-wheel `eval/` now depends on repo-root
`experiments/`.

`_lib.config_sha256` is also called with `dict(doc)`, because `_resolved_dict`
raises `TypeError` on a `Mapping` that is not a `dict`, and `resolve_config`
documents that it returns rather than raises. `_resolved_dict` returns the same
object rather than a copy, so the document is copied at derivation time to stop
a later mutation silently changing the directory.

## Non-goals

- No run execution. S4 is handed results; producing them is S5/S6.
- No metric computation, no gold verification.
- No `INDEX.md` regeneration beyond calling `_lib.regen_index_md`, which owns
  its own format.

## Acceptance criteria

1. The sidecar exists on disk **before** the index line does. Asserted by
   spying on `_lib.append_index` -- which `write_record` calls by module-global
   lookup as its last statement -- and checking, at call time, that the sidecar
   file exists and the index does not yet contain the `run_id`. Not by mtime
   comparison, whose granularity makes it flaky, and not by asserting both
   exist afterwards.
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
9. `assert_supported` passes on all three schemas, and a union-typed slot
   rejects a wrong-typed value.
10. No test writes into the real `experiments/` tree; `experiments_root` is
    threaded everywhere, including both `regen_index_md` paths.
11. `_lib` imports successfully from a subprocess whose cwd is not the repo
    root.
12. A `ResolvedScenario` passed as `config_obj` is refused before any file is
    written, rather than half-materializing a directory.
13. A non-UTC `ts` is refused.
14. Every path is exercised by a test that was first observed to fail.

## Review

Independent code-grounded review, 2026-08-06. Verdict on revision 1:
**REWORK (bounded)** — the pre-derivation contract, the `ensure_ascii`
diagnosis, the `run_id_collision` code choice, and the failure-atomicity
analysis were all verified correct. Four sections were rewritten.

| # | Severity | Finding | Resolution |
| ---: | --- | --- | --- |
| B-1 | BLOCKER | The collision policy keyed on a differing `config_sha256`, which in a real collision is equal by construction; the collision that actually occurs — a same-config re-run in one minute — matched neither branch | Rule inverted to a byte comparison; equal hash with differing measurements is explicitly a collision |
| B-2 | BLOCKER | "Stage in the run directory" and "validate before any file is written" cannot both hold | Order pinned: serialize → validate parsed text → claim with `mkdir(exist_ok=False)` → write |
| B-3 | BLOCKER | The `experiments._lib` import path was unstated and resolves today only by accident of cwd | An explicit `_experiments.py` resolution module, tested from a foreign cwd |
| M-1 | MAJOR | The walker needs six keywords plus three mechanisms, not three keywords; the per-query schema's `allOf`/`if`/`then` went unmentioned, and 20 union-typed slots currently accept anything | Full extension specified, including `oneOf` error selection and deeper `assert_supported` recursion |
| M-2 | MAJOR | `_lib.config_sha256` raises on a non-`dict` `Mapping`, turning a total function partial | Called with `dict(doc)` |
| M-3 | MAJOR | `config_obj` was ambiguous; passing `ResolvedScenario` half-materializes a directory before `yaml.safe_dump` raises on an enum | Raw document pinned, with the failure mode recorded; document copied at derivation; `ts` required UTC |
| M-4 | MAJOR | `base_dir`/`index_path`/`md_path` unmentioned — tests would write into the real repo and could overwrite the committed `INDEX.md` | `experiments_root` threaded explicitly |
| M-5 | MAJOR | A collision-blocked run cannot be indexed under its own id, and the refusal channel was unstated | Returns a `Blocker` and writes nothing — the sole named exception to indexing blocked runs |
| M-6 | MAJOR | Ten required `write_record` arguments unaddressed; `experiment` has no source in the config at all | A "What S4 is given" table with a source for each |
| m-1 | MINOR | AC-1's mechanism was hand-wavy | `append_index` spy specified |
| m-2 | MINOR | TOCTOU on the collision check | Atomic `mkdir(exist_ok=False)`; the `_lib` index race documented as an accepted single-writer limitation |
| m-3 | MINOR | Failure-atomicity reasoning | Confirmed correct, no change |
