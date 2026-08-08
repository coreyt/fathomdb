---
status: COMPLETE
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

## Ingest — driven by the snapshot, never by a glob

Revision 1 said "JSONL shards under `<data_root>/raw/*.jsonl`". That is wrong
in a way that would have silently corrupted every number:

```text
raw/*.jsonl total          23,306 rows
snapshot.json total_docs   10,506
```

The glob picks up `compmix`, `musique_dev`, and `wec_eng` — 12,800 documents
that are **not in the frozen snapshot**. A run would pin `corpus_hash` to a
10,506-doc identity while measuring recall against 2.2× that index, and
Recall@10 would be strictly depressed by documents the corpus does not contain.
That is the "confident number that is not true" failure this platform exists
to prevent. Worse, `musique_dev.jsonl` has neither `doc_id` nor `body` on any
of its 4,834 rows, so the ingest would raise on its first row anyway.

Ingest is therefore driven by **`snapshot.per_source_sha256`**, which is the
authoritative shard list: for each of the 10 entries, resolve
`<data_root>/raw/<source>.jsonl`, verify its SHA-256 and line count against the
declared values, refuse on mismatch with a typed blocker, and ingest only
those. `source_id` is the snapshot's `source` name, which is what D-6.2's
corpus identity actually means.

Verified across those 10 shards: `doc_id` and `body` present and non-empty on
all 10,506 rows, `body` always a string, `doc_id` unique within *and* across
shards (0 collisions), `source_type` always present and drawn from
`{article, email, meeting, note, paper, todo}` — all accepted by a real
10,506-document write.

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

## Runtime, measured — and why retrieval is cached

Revision 1 was silent on runtime. Measured against the real engine:

| Scale | Write | `search_text_only` |
| --- | --- | --- |
| 2,000 docs | 0.44 s | mean 966 ms |
| **10,506 docs** | **2.15 s** | **mean 21.7 s, p95 38.1 s** |

Ingest is a non-issue. **Retrieval is ~28 hours for one pass over 4,597
queries**, and the cause is the very property cited elsewhere as a feature: the
node-FTS branch is untruncated, so it materializes every bm25 match — ~5,000
hits per query, of which Recall@{5,10} uses ten. About 99.8% of the work is
discarded, and it scales superlinearly (5.25× the documents gave 22× the
latency).

Composed naively it is worse. `metrics.aggregate` calls `retrieve` inside its
own per-K loop, so driving the ladder by calling it once per rung would
re-execute all 4,597 searches — **~55 hours**.

So S6 **retrieves once per query**, caches the ranked doc-id list truncated to
`max(ladder)`, and feeds every K rung from that cache. `aggregate` is handed a
cache lookup, never a live engine call.

This historical pricing was superseded by the Slice 19 canonical join-index
work. Do not use it as a current performance estimate or as a rationale for
the now-retired D-5.2 fanout proposal.

## Resumability

A 28-hour run that cannot resume is a run nobody will finish. S6 writes an
append-only checkpoint keyed on `query_id` and resumes from it.

Per-query rows are also validated **as they are produced**, not in one batch at
the end. Validation costs 30 µs per row (measured: 9,194 rows in 0.28 s), so
per-row validation is free — and it means one nonconforming row fails that
*query* rather than discarding a completed campaign, which is what batch
validation before `mkdir` would do.

## Scoring

Per gold query: run the call once, map hits to doc ids, cache, and score every
K rung through S2. Nothing is recomputed here — S2 owns the metric semantics
and is pinned to executed parity with the Rust reference.

The query text is **`GoldQuery.query`**, which every gold query carries. There
is exactly one `scenario.query.text` in the config and 4,597 gold queries, so
that key is refused as `config_unused_key` for a characterization — it is
diagnostic-only. Symmetrically, `scenario.fixture` is refused for a
characterization.

`required_doc_ids ⊆ ingested_doc_ids` is asserted before scoring. The join is
clean today — 1,650 distinct required doc ids, 100% present in the snapshot-10
set, every positive query carrying exactly one and the 125 negatives carrying
none — but the assert is free and prevents a silent 0.0 if the shard list ever
drifts.

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

`replay_of` is a **CLI argument, not a config key**. Putting it in the config
would change `config_sha256`, so the config-drift axis would fire on every
replay including a perfect one — destroying the one case worth reporting.
`"replay"` is also removed from `INEXPRESSIBLE`, since that refusal is keyed on
the campaign string independently of any key.

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

S5 writes empty `code`/`env` dicts, so for records it has already written those
axes are unrecoverable. `_lib.git_info()` and `_lib.env_info()` exist and were
simply unused; S6 calls them, and reports an empty prior `git_sha` as
`axis_unrecoverable` rather than as drift from `""`.

`replay` is currently refused by S3 as `config_campaign_inexpressible`. S6
lands the replay mechanism over a stored record and removes that refusal.

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
