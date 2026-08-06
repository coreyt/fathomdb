---
status: PROPOSED
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

Pending — an independent code-grounded review is required before
implementation, per the per-slice governance in the plan.
