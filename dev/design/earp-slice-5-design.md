---
status: COMPLETE
---

# EARP Slice 5 — the diagnostic runner

Design of record for S5 of `dev/plans/earp-foundation.md`. Depends on S3
(resolver), S4 (writer), and S2 (`check_depth`). The first slice that opens a
real engine.

## Contract

S5 proves the whole machinery end to end **without making a retrieval-quality
claim**. It owns the `diagnostic` campaign kind, which the pre-review plan
declared and no slice delivered.

A diagnostic run creates a fresh temporary SQLite database, writes a small
human-authored fixture through the real Python SDK, runs one named search call,
captures integrity and open-time witnesses, and hands the result to S4's
writer. It emits no Evidence Recall, because it has no gold — and it must be
impossible for it to emit one.

That separation is the point. Everything measurable here is a property of the
*system* (did the write land, did the search return, what did open report),
never of *relevance*. So a green diagnostic says the harness works; it says
nothing about whether FathomDB retrieves well.

## Ground truth from the real engine

Verified against the built binding rather than assumed:

- **`body` is a string, not an object.** A dict body raises
  `WriteValidationError: string contains characters not representable as UTF-8
  (lone surrogate)` — a badly misleading message for a type error, and exactly
  the kind of thing a fixture author would lose an hour to. The fixture spec
  pins `body: str`.
- **`source_id` is mandatory** on every canonical item.
- **`SearchHit.id` is an `IdSpace`**, with `space` and `value`. With a
  `logical_id` on the write it is `IdSpace(space='logical', value='doc-a')`;
  without one it is `IdSpace(space='content', value=<sha256>)`.
- `SearchHit` carries `body`, `branch`, `ce_score`, `id`, `kind`, `score`,
  `source_id`.
- `read.projections(engine)` returns `[]` when none are configured.

## The doc-id mapping

IR-B scores presence at doc-body granularity, so every hit must map to a
document identity. S5 fixes that mapping now, because S6 inherits it and a late
change would invalidate any number already recorded:

**Fixture items carry `logical_id`, and the doc id is `hit.id.value` when
`hit.id.space == "logical"`.** A hit in the `content` space means the fixture
omitted a `logical_id`, which is a fixture defect rather than a retrieval
outcome — it is a typed failure, not a miss.

This is also why the fixture is authored rather than generated: the mapping is
only sound if the author controls the identity.

## What is measured

| Witness | Source | Meaning |
| --- | --- | --- |
| `open_report` | `Engine.open_report()` | schema before/after, query backend, embedder identity |
| `embedder_fetched` | `open_report.embedder_download_ms` | post-hoc detection that weights were fetched |
| `dense_disabled` | `open_report.dense_disabled` | the vector-equivalence degraded open |
| `projection_coverage` | `read.projections(engine)` | declared projections and their readiness |
| `write_receipt` | `Engine.write` | the fixture actually landed |
| `search_returned` | the search call | the query executed and returned a result object |

Each is a `Witness` with its real `source` and `call_path`, never a bare
boolean — the S0 lock made a witness an admission gate, so it must record where
it came from.

## Typed failures, not misses

A diagnostic run distinguishes four dispositions, and none of them collapses
into "the search found nothing":

- **success** — the call returned and its witnesses were captured;
- **skip** — an arm was not requested;
- **blocker** — a typed precondition refused (`embedder_fetched`,
  `dense_disabled`, `corpus_root_absent`);
- **failure** — the SDK raised.

An SDK exception is caught, recorded with its type and message, and produces a
`failed` verdict. It is never allowed to look like an empty result set, which
is the codex §9 [P2] defect the reference's own history records.

## Embedder policy

Network is denied by default, and the embedder-cache check is **post-hoc**, not
preflight — there is no Python cache-status API, and the Rust cache path
derives from `pub(crate)` constants, so a preflight would hardcode a model
revision into Python and drift from it silently.

So a diagnostic run defaults to `use_default_embedder: false`, and when it is
enabled, a non-null `embedder_download_ms` is a `embedder_fetched` blocker
recorded *after* the fact. The run is marked blocked rather than pretending it
ran offline.

## Database lifecycle

One fresh temporary SQLite database per scenario, removed after the run.
Scenarios never share a mutable database.

This is a **run-isolation policy, not an engine requirement**:
`configure_projections` diffs against the durable registry and backfills in one
transaction, so the engine does not need a fresh database. Stating it as a
policy stops a later matrix slice from treating it as a constraint.

## What S5 does not do

- No gold. `verify_gold` is not called; a diagnostic config declares none, and
  S3 already refuses `evidence_recall_k` on a diagnostic campaign.
- No metric computation. No Evidence Recall, no abstention, no nDCG.
- No projections beyond observing them. Configuring them is S7.
- No corpus-scale run. That is S6.

## Acceptance criteria

1. A diagnostic run creates a fresh database, writes the fixture through the
   real SDK, runs the named call, and produces a complete verdict with
   witnesses.
2. Every fixture item carries `source_id` and `logical_id`; a missing
   `logical_id` is refused before the write, not discovered at scoring time.
3. A hit in the `content` id space is a typed failure, not a miss.
4. An SDK exception yields a `failed` verdict recording the exception type,
   and no metric is emitted for that query.
5. No Evidence Recall, abstention, or document metric appears in a diagnostic
   result under any configuration.
6. `embedder_download_ms` being non-null yields an `embedder_fetched` blocker
   and a `blocked` verdict.
7. Witnesses record their real `source` and `call_path`; none is a bare
   boolean.
8. The temporary database is removed after the run, including on failure.
9. Artifacts are written through S4's writer, so the sidecar precedes the
   index line and the run is never indexed as complete when blocked.
10. Every path is exercised by a test that was first observed to fail.

## Review

Independent review, 2026-08-06 — the first that could **execute** against the
built binding rather than reason about the SDK. It hand-ran the design's whole
shape end to end and it produced a valid sidecar, a valid index line, and six
well-sourced witnesses on the first attempt. Verdict: **changes required**;
five findings closed before implementation.

| # | Severity | Finding | Resolution |
| ---: | --- | --- | --- |
| B-1 | BLOCKER | The fixture — the slice's central artifact — had no config surface, no format, and no blocker code; `earp.v1` is closed, so it could not even be named | `scenario.fixture` and `scenario.query.text` added to the schema and registry; `fixture_missing` / `fixture_invalid` added to the lock, kept distinct from `corpus_root_absent`, which is about the gitignored corpus tree a diagnostic never declares |
| B-2 | BLOCKER | "No metric under any configuration" had no enforcing mechanism: S3 refused only `evidence_recall_k`, so `document_metrics` and `integrity` flowed through, and the writer accepted a full recall metric on a diagnostic config | S3 refuses all three metric keys and a `gold` block; the runner's sidecar is structurally `metrics: {}` |
| M-3 | MAJOR | A null body is **accepted**, stored as `'{}'`, invisible to FTS, behind a healthy-looking receipt — so a broken fixture would report green | `body` must be a non-empty string, refused before the write; landing is witnessed by a `read.get_many` round trip, not the counter-only receipt |
| M-4 | MAJOR | `write_receipt` had no legal `WitnessSource`, and filing it under `store_query` would reintroduce the conflation the lock closed | `write_receipt` added to the enum and the result schema |
| M-5 | MAJOR | `search_projected_text` takes `name`, not `projection_name`, so the dispatch would `TypeError` on the only call with a required knob | A `PARAM_RENAMES` table, stated in the design |
| M-6 | MAJOR | AC-6 was undriveable: a non-null `embedder_download_ms` needs a real fetch, which policy forbids | `classify_open` factored out as a pure function over a report mapping |
| M-7 | MAJOR | A duplicate `logical_id` silently supersedes — for S6 an unattributable recall loss | Uniqueness enforced in `load_fixture`, with a forward note for S6 |
| N-10 | MINOR | `close()` leaves a `.lock` sidecar, so per-file deletion is wrong | One temp **directory**, `close()` in a `finally`, then `rmtree` |
| N-12 | MINOR | The zero-hit verdict was undefined | `complete` — a diagnostic makes no relevance claim — paired with the landing witness so a broken fixture is distinguishable |

Confirmed by execution, no change required: the doc-id mapping is sound and
`logical|content` is total for `search_text_only`; a hash-shaped `logical_id`
still reports the logical space; superseded nodes and edges never appear;
`source_id`'s own error message is clear and helpful. The observation that the
FTS branch returned 30 matching documents was made before Slice 18
introduced the public result-limit contract. It is not an as-built EARP fact:
every search mode now defaults to `limit=10` and refuses a limit above 100.
S6a owns the corresponding depth and `fanout_used` cutover.
