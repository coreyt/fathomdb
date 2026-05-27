# FathomDB 0.7.0 test corpus — corpus card

Status: **scaffold** (Corpus-Pack 1 acquisition not yet started).
Drafted: 2026-05-27.
Authoritative handoff:
[`dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md`](../../dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md).
Research:
[`dev/notes/0.7.0-test-corpus-research.md`](../../dev/notes/0.7.0-test-corpus-research.md).
HITL locks:
[`dev/plans/0.7.0-HITL-recommendations.md`](../../dev/plans/0.7.0-HITL-recommendations.md).

## What this corpus is

A composite real-data + synthetic test corpus (~10K documents,
"Version B") that exercises FathomDB's three retrieval modalities
together: SQLite FTS5, sqlite-vec (`bit[768]` + f32 rerank,
post-Pack-1), and FathomDB's chunk/document/relation graph.

The corpus has three jobs:

1. Drive Corpus-Pack 4 search-validation tests and the 0.7.0
   PERF-VECTOR-QUANT Pack 2 recall-floor RED test
   (`recall@10 ≥ 0.90` vs f32 brute-force ground truth).
2. Validate the post-Pack-1 vec0 schema (`bit[768]` + f32 +
   metadata + `source_type` partition_key) on realistic doc
   shapes.
3. Establish a cross-document retrieval baseline through
   synthetic multi-doc chains layered over real-data anchors.

## HITL locks (2026-05-27 — do not change without HITL sign-off)

- **Target version: Version B** (~10K docs).
- **Recall floor: 0.90 @ k=10** (initial; tighten later).
- **Partition key: `source_type`** (cardinality ~6:
  `email`, `article`, `paper`, `meeting`, `note`, `todo`).
  Supersedes the earlier `kind` default in Pack 1's draft.
- **Enron OK to commit** with the CMU April-2026 impersonation
  note recorded here (see §"Provenance notes" below).
- **Cross-doc chain generator is in 0.7.0 scope** (Corpus-Pack 2).
- **CI artifact strategy: GitHub Actions cache** as the
  primary distribution path. License-clean sources may also be
  committed directly to the repo. CC-BY-NC-SA sources
  (ELITR) live in the cache only.
- **Determinism**: a second run of the build pipeline on the
  same upstream snapshots produces a bit-identical artifact.
  Upstream checksums recorded in this card.

## Source vocabulary (`source_type`)

Locked to exactly 6 values. New sources must map onto one of
these — do not extend without a HITL pass.

| `source_type` | Examples |
|---|---|
| `email`   | Enron messages, EnronQA |
| `meeting` | QMSum transcripts, ELITR minutes |
| `paper`   | PMC OA articles, S2ORC papers |
| `article` | CNN/DailyMail news articles |
| `note`    | bahmutov daily-logs, synthetic notes |
| `todo`    | Landes/Di Eugenio to-do corpus + synthetic |

## Document schema (canonical JSONL)

Every per-source JSONL in `raw/` and every synthetic doc in
`chains/` shares this shape:

```jsonc
{
  "doc_id":           "stable hash of provenance + source-native ID",
  "source_type":      "one of {email,meeting,paper,article,note,todo}",
  "title":            "string or null",
  "body":             "string (the text we will chunk + embed)",
  "created_at":       "ISO-8601 UTC",
  "modified_at":      "ISO-8601 UTC or null",
  "author_or_sender": "string or null",
  "recipients":       ["..."],
  "people_mentions":  ["..."],
  "project_mentions": ["..."],
  "tags":             ["..."],
  "url_or_external_id": "string or null",
  "thread_id":        "string or null",
  "parent_doc_id":    "string or null",
  "license":          "SPDX identifier",
  "provenance":       "short upstream tag (e.g. cmu-enron-2015-05-08)"
}
```

`thread_id` / `parent_doc_id` are populated for email threads,
meeting → minutes pairs, and synthetic-chain connective docs.
Null elsewhere.

## Source catalogue + license posture

Per the research doc §5 license roll-up. Targets are Version B
soft minimums.

| Source | `source_type` | Target docs | License | Distribution |
|---|---|---|---|---|
| Enron Email Dataset (CMU) | `email` | 2,000 | research-use; ambiguous redistribution; **HITL OK to commit** with impersonation note | **commit** in `raw/enron.jsonl` |
| EnronQA (HF MichaelR207/enron_qa_0922) | `email` (QA augmentation) | TBD | undeclared on HF card | **cache only** until license clarified |
| QMSum | `meeting` | 600 | derived from AMI (CC-BY) / ICSI (CC-BY-NC) — chain unverified | **commit** if verified MIT/CC-BY; else cache only |
| ELITR Minuting Corpus | `meeting` | 400 | CC-BY-NC-SA 4.0 | **cache only** — NC + SA blocks redistribution |
| PMC OA — Commercial-Use bucket | `paper` | 1,500 | CC0 / CC-BY / CC-BY-SA / CC-BY-ND | **cache** (per-article license filter required) |
| S2ORC (Semantic Scholar Bulk Dataset API) | `paper` | 1,000 | ODC-By 1.0 (attribution) | **cache only** |
| OpenAlex (AWS Registry) | `paper` (metadata enrichment only) | — | CC0 | **commit** OK |
| CNN/DailyMail (HF abisee/cnn_dailymail) | `article` | 2,500 | Apache-2.0 | **commit** OK |
| Landes/Di Eugenio to-dos (plandes/todo-task) | `todo` (seed) | 500 | MIT | **commit** OK |
| bahmutov daily-logs | `note` (style seed) | 300 | MIT | **commit** OK |
| Synthetic notes (this project) | `note` | 1,200 | project license | **commit** OK |
| Synthetic chain connectives (Corpus-Pack 2) | mixed (`note`/`email`/`todo`) | ~200 chains, ~600 docs | project license | **commit** OK; provenance=`synthetic-chain` |
| **Total target** | | **~10,000** | | |

`raw/.gitignore` excludes the cache-only sources; the
acquisition script fetches them on demand.

## Cross-document chains (Corpus-Pack 2)

The generator emits ~200 multi-document chains anchored on real
documents. Chain definitions live in `chains/<chain_id>.json`:

```jsonc
{
  "chain_id": "string",
  "chain_shape": "EMAIL->MEETING->TODO->NOTE | PAPER->NOTE->TODO | ...",
  "doc_ids": ["..."],
  "ground_truth_queries": [
    {
      "query": "what did we decide about X?",
      "expected_top_k_doc_ids": ["..."],
      "relation_type": "summarizes|follows_up|contradicts|action_from|mentions|cites|replies_to"
    }
  ]
}
```

Relation vocabulary (locked):
`replies_to`, `follows_up_on`, `summarizes`, `action_from`,
`contradicts`, `mentions`, `cites`.

Synthetic connective docs (the notes/emails/todos generated
by the chain generator) live in the appropriate per-source
JSONL with `provenance: synthetic-chain` and `tags` containing
the chain_id. **Synthetic content must not exceed 20 % of the
total corpus by doc count** — escalation trigger per the
handoff.

Determinism: fixed RNG seed; documented here once generator
lands.

## CI artifact + cache layout

- GitHub Actions cache key:
  `corpus-vB-<checksum of source-version manifest>`.
- Cache hit → restore `tests/corpus/raw/*.jsonl` +
  `tests/corpus/chains/*.json`.
- Cache miss → run `tests/corpus/scripts/build.{sh,py}` which
  re-fetches from upstream, verifies upstream checksums match
  the pinned manifest, then writes JSONL.
- Committed sources (per the catalogue table) survive both
  paths — they live in-tree and are also re-emitted by the
  acquisition script for round-trip determinism.

## Provenance notes

- **Enron (CMU)**: CMU's public page carries a footer dated
  April 2026 noting that "digital forensics experts have raised
  authentication concerns about possible message impersonation
  within the corpus." This does not affect usefulness as a
  synthetic-realistic substrate, but is recorded here per HITL
  for any downstream paper-grade work.
- **CNN/DailyMail**: the canonical HF distribution strips the
  per-article dates and URLs. The acquisition script
  synthesizes `created_at` values uniformly across 2007-2015
  and marks them with `provenance: synthetic-date`.

## Upstream checksums

Authoritative copy lives in
[`scripts/manifest.json`](./scripts/manifest.json) (machine-readable
+ verified by the build pipeline). Summary table:

| Source | Upstream ID | Snapshot | Output SHA-256 |
|---|---|---|---|
| CNN/DailyMail | HF `abisee/cnn_dailymail` config `3.0.0` | rev `96df5e68…6223d` (2024-01-18) | `7c371528…493d09` |
| Landes to-dos | GH `plandes/todo-task` `resources/todo-dataset.json` | rev `06bcd261…d26f` (2018-06-29) | `74f02482…0e812` |
| bahmutov daily-logs | GH `bahmutov/daily-logs` monthly Markdown | rev `521476da…19b7` (2020-09-01) | `bdecf3e8…0e81` |
| Enron (CMU) | CMU `enron_mail_20150507.tar.gz` | tarball sha `b3da1b3f…48ca7` (2015-05-07) | `c4df0c71…486ab` |

## Out of scope for 0.7.0

- ANN graph index (vectorlite / sqlite-vec ANN alpha /
  Rust-side index) — declined per Pack 1/2 plan.
- Embedder change — locked, keep existing.
- Version A (fast dev-loop subset) and Version C (stress).
- Non-English content (ELITR Czech subset filtered out).
- Re-embedding the corpus under a different model.
- Web UI for browsing the corpus.

## Implementation order

Corpus-Pack 1 (acquisition + cleaning) → Corpus-Pack 2
(chain generator) → **wait on PERF-VECTOR-QUANT Pack 1 schema
landing** → Corpus-Pack 3 (ingest harness) → Corpus-Pack 4
(search validation tests).

Each pack closes with a commit on `main` and a closure note in
`dev/plans/runs/0.7.0-CORPUS-BUILD-output.json`.
