---
status: PROPOSED
---

# EARP Slice 6 — corpus-scale characterization and replay

Design of record for S6 of `dev/plans/earp-foundation.md`. Depends on S1
(gold), S2 (metrics), S3 (resolver), S4 (writer), S5 (runner machinery).

## Contract

S6 is the first slice that makes a **retrieval-quality claim**. Everything
before it measured the harness; this measures FathomDB.

It ingests the frozen corpus, verifies the gold, runs each gold query through
one named search call, scores Evidence Recall@{5,10} strict and graded plus
abstention over the negatives, and writes per-query outcomes alongside the
aggregate. It also owns `replay`, because deterministic run identity only
becomes meaningful once there is a real campaign worth reproducing.

## Ingest

Corpus rows are JSONL shards under `<data_root>/raw/*.jsonl`, each carrying
`doc_id` and `body` among other fields. Verified shape:

```text
keys: author_or_sender, body, created_at, doc_id, license, modified_at,
      parent_doc_id, people_mentions, project_mentions, provenance,
      recipients, source_type, tags, thread_id, title, url_or_external_id
```

The write mapping is fixed by S5's doc-id decision and inherits its
preconditions:

| Corpus field | Write item |
| --- | --- |
| `doc_id` | `logical_id` — so `hit.id.value` maps back |
| `body` | `body`, required non-empty string |
| `source_type` | `kind` |
| shard name | `source_id` |

**Duplicate `doc_id` is a hard refusal, not a warning.** A duplicate silently
supersedes, leaving one active row with no error and no receipt signal — at
corpus scale that is a document deleted from the index while the gold still
requires it, producing an unattributable recall loss. S6 asserts
`len(set(doc_ids)) == n_docs` before the first write, exactly as S5's fixture
loader does at fixture scale.

## The gold basis, and why the real campaign is currently blocked

S1 already enforces D-6's four conditions. Two of them bite here, and the
design states plainly what that means rather than quietly working around it:

- **`data_root` is gitignored**, so it is absent from any worktree. A run
  without it is `corpus_root_absent` — a typed blocker, never a silent empty
  corpus.
- **The cached gold is `ir-c-reused-v1`; the committed generator emits v2.**
  S1 refuses v1 as `gold_stale_qrels_version`, so a corpus-scale campaign
  cannot run against the cache as it stands.

Regenerating is an **operator action**, not something S6 performs.
`build_ir_gold.py` resolves its own repo root and writes into
`<repo>/data/corpus-data/eval/ir_gold/`, so running it mutates a data tree and
belongs to whoever owns that tree. S6 names the command in the blocker message
and stops.

This is the honest position: **S6 lands complete and tested, and the real
corpus-scale number awaits gold regeneration.** The alternative — relaxing the
version check so a number can be produced today — would trade a blocked run for
an untrustworthy one, which is the whole failure mode this platform exists to
avoid.

Because the content is semantically identical between v1 and v2 (verified:
`evidence_spans` is non-empty on zero of 4,597 source rows), regeneration is
cheap and carries no re-baselining.

## Scoring

Per gold query: run the call, map hits to doc ids, and score through S2.
Nothing is recomputed here — S2 owns the metric semantics and is pinned to
executed parity with the Rust reference.

- Negatives are routed to abstention and held out of the recall means.
- A retrieval error is a typed per-query failure, never folded into an empty
  result set and scored as a miss or as a correct abstention.
- `ndcg` resolves `not_applicable`: no gold in this repo carries graded
  relevance.
- `supporting_coverage` is `not_applicable` per K: no gold carries supporting
  units.
- The fanout actually used is recorded with every number, per IR-B §(c).

Depth is checked once, at config time, by S3 — S6 does not re-derive it. The
ladder is whatever the config declared and the resolver admitted, which for a
hybrid call is `[5, 10]` and for `search_text_only` may legitimately go deeper,
since the FTS branch is genuinely untruncated (verified: 30 matching documents
returned 30 hits).

## Per-query artifacts

Every gold query produces one `earp.per-query.v1` line per K, carrying its
`query_id`, `query_class`, outcome, and — only when `outcome == "scored"` — the
numbers. The schema's `if`/`then` enforces that pairing, so a scored row
missing its numbers cannot be written.

At 4,597 queries × 2 K values that is ~9,200 lines, which is why the per-query
file is JSONL rather than a single JSON document.

## Replay

`replay` re-resolves a stored config, recomputes the run identity, and reports
drift. It lands here rather than with the writer because deterministic
`run_id` is the *mechanism* while a real campaign is the first thing that makes
reproducing one meaningful.

Drift is reported across three axes, each already recorded in the sidecar or
the shared record:

| Axis | Source | Meaning |
| --- | --- | --- |
| config | `config_sha256` | the declared scenario changed |
| code | `record.code.git_sha` / `dirty` | the engine changed |
| environment | `record.env` | the interpreter or lockfile changed |

A replay whose config hash matches but whose git sha differs is the
interesting case: same declared experiment, different engine. It is reported as
drift with the axis named, never as a pass or a failure — S6 measures, it does
not rule.

`replay` is currently refused by S3 as `config_campaign_inexpressible`, because
`earp.v1` has no key referencing a prior run. S6 therefore lands the replay
*mechanism* over a stored record plus a `scenario.replay_of` key, which is a
schema amendment this slice carries.

## Non-goals

- No comparison, no sweep. Those need arms, which `earp.v1` cannot express.
- No gold generation or regeneration.
- No threshold or better-than claim: a decision rule is evaluated and recorded,
  but S6 does not decide whether a number is good.
- No projection configuration. That is S7.

## Acceptance criteria

1. A corpus-scale characterization ingests the shards, verifies gold through
   S1, scores through S2, and writes aggregate plus per-query artifacts.
2. Duplicate corpus `doc_id`s are refused before the first write.
3. An absent `data_root` is `corpus_root_absent`; `ir-c-reused-v1` gold is
   `gold_stale_qrels_version`, with the regeneration command named.
4. Negatives are scored by abstention and excluded from the recall means.
5. A retrieval error is a typed per-query failure, never a miss or an
   abstention.
6. `ndcg` and `supporting_coverage` are `not_applicable`, never zero.
7. The fanout used is recorded with every number.
8. Per-query rows validate against `earp.per-query.v1`, including the
   scored-rows-carry-numbers conditional.
9. Replay recomputes the identity and reports config, code, and environment
   drift separately, without ruling on it.
10. Tests run offline in the default suite against a small corpus-shaped
    fixture; the real corpus-scale campaign is exercised only when the data is
    present, and skips visibly otherwise.
11. Every path is exercised by a test that was first observed to fail.

## Review

Pending — an independent code-grounded review is required before
implementation, per the per-slice governance in the plan.
