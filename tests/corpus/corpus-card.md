# FathomDB 0.7.0 test corpus — corpus card

Status: **active corpus** (0.8.x QA expansion in progress).
Drafted: 2026-05-27.
Authoritative handoff:
[`dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md`](../../dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md).
Research:
[`dev/notes/0.7.0-test-corpus-research.md`](../../dev/notes/0.7.0-test-corpus-research.md).
HITL locks:
[`dev/plans/0.7.0-HITL-recommendations.md`](../../dev/plans/0.7.0-HITL-recommendations.md).

## What this corpus is

A composite real-data + synthetic test corpus (10K+ raw documents after
the 0.8.x QAConv/QASPER expansion) that exercises FathomDB's three retrieval modalities
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

- **Target version: Version B** (~10K docs). **Amended 2026-06-09 (HITL coreyt):**
  the ~10K *must* is **waived** for the IR freeze — freeze the current 8 datasets
  **+ QAConv + QASPER ≈ 10.1K**; PMC OA · S2ORC · ELITR stay deferred. See
  `dev/plans/runs/STATUS-0.8.0.md` §7 (2026-06-09) + the COR-2 runbook.
- **Recall floor: 0.90 @ k=10** (initial; tighten later).
- **Partition key: `source_type`** (cardinality ~8:
  `email`, `article`, `paper`, `meeting`, `note`, `todo`,
  `event`, `kb`). Supersedes the earlier `kind` default in
  Pack 1's draft. **Amended 2026-07-02 (HITL coreyt):**
  cardinality raised 6 → 8 with `event` + `kb` for cross-source
  QID-native corpora — see §"Source vocabulary" below.
- **Enron OK to commit** with the CMU April-2026 impersonation
  note recorded here (see §"Provenance notes" below).
- **Cross-doc chain generator is in 0.7.0 scope** (Corpus-Pack 2).
- **CI artifact strategy: GitHub Actions cache** as the
  primary distribution path. **All produced JSONL lives under
  `data/corpus-data/` (gitignored).** Acquisition scripts in
  `tests/corpus/scripts/` are the reproducible source of truth;
  data is rebuilt locally or restored from CI cache. The
  per-source `distribution` field in `scripts/manifest.json`
  records license posture only — license-clean sources stay
  candidates for direct commit if policy ever permits, but the
  current default is "scripts in git, data out of git" for
  every source.
- **Determinism**: a second run of the build pipeline on the
  same upstream snapshots produces a bit-identical artifact.
  Upstream checksums recorded in this card.

## Source vocabulary (`source_type`)

Locked to exactly 8 values. New sources must map onto one of
these — do not extend without a HITL pass.

**Amended 2026-07-02 (HITL coreyt):** extended 6 → 8 with
`event` + `kb` to admit cross-source QID-native corpora into
the shared key space (Slice B1). The original 6 are unchanged.

| `source_type` | Examples |
|---|---|
| `email`   | Enron messages, EnronQA |
| `meeting` | QMSum transcripts, ELITR minutes |
| `paper`   | PMC OA articles, S2ORC papers |
| `article` | CNN/DailyMail news articles |
| `note`    | bahmutov daily-logs, synthetic notes |
| `todo`    | Landes/Di Eugenio to-do corpus + synthetic |
| `event`   | event-centric passages (e.g. WEC-Eng) |
| `kb`      | knowledge / infobox-table facts (e.g. CompMix, Wikidata) |

## Document schema (canonical JSONL)

Every per-source JSONL in `raw/` and every synthetic doc in
`chains/` shares this shape:

```jsonc
{
  "doc_id":           "stable hash of provenance + source-native ID",
  "source_type":      "one of {email,meeting,paper,article,note,todo,event,kb}",
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
  "provenance":       "short upstream tag (e.g. cmu-enron-2015-05-08)",
  "entity_ids":       [{"id": "Q42", "kind": "qid", "surface": "Douglas Adams"}]
}
```

`thread_id` / `parent_doc_id` are populated for email threads,
meeting → minutes pairs, and synthetic-chain connective docs.
Null elsewhere.

`entity_ids` (**additive; HITL-approved amendment 2026-07-02,
coreyt** — Slice B1) is the cross-corpus join key: a list of
resolved entity references that let heterogeneous QID-native
corpora be joined on a shared key space. Each element is an
`EntityRef` with:

- `id` — canonical identifier, non-empty (e.g. `"Q42"`, `"10.1000/xyz"`);
- `kind` — one of `{"qid","doi"}` (validated; other kinds raise);
- `surface` — optional mention text the id was resolved from, or null.

A `qid` whose `id` does not match `Q\d+` still constructs but
emits a warning (upstream QIDs are trusted, not rewritten).
The field is **additive and backward-compatible**: it defaults
to `[]`, so every existing per-source corpus serializes
`"entity_ids": []` with no acquisition-script change. License
and provenance carry through unchanged — an `entity_ids` entry
adds a join key, it does not relicense the doc.

## Source catalogue + license posture

Per the research doc §5 license roll-up. Targets are Version B
soft minimums. All output paths are
`data/corpus-data/raw/{source}.jsonl` (gitignored). The
"License posture" column records whether the source would be
*eligible* for direct commit if we ever flipped the policy.

| Source | `source_type` | Target docs | License | License posture |
|---|---|---|---|---|
| Enron Email Dataset (CMU) | `email` | 2,000 | research-use; ambiguous redistribution; **HITL OK to commit** with impersonation note | commit-eligible |
| EnronQA (HF MichaelR207/enron_qa_0922) | `email` (existing source; QA export added) | 200 + 710 eval QA | undeclared on HF card | cache-only until license clarified |
| QMSum | `meeting` (existing source; QA export added) | 600 + 1,584 eval QA | derived from AMI (CC-BY) / ICSI (CC-BY-NC) — chain unverified | cache-only until chain verified |
| ELITR Minuting Corpus | `meeting` | 400 | CC-BY-NC-SA 4.0 | cache-only — NC + SA blocks redistribution |
| QAConv | mixed `email`/`meeting`/`note` | 1,250 + 2,303 eval QA | BSD-3-Clause | commit-eligible |
| QASPER | `paper` | 1,585 + 7,993 answer-level eval QA | CC-BY-4.0 | commit-eligible with attribution |
| PMC OA — Commercial-Use bucket | `paper` | deferred | CC0 / CC-BY / CC-BY-SA / CC-BY-ND | defer after QASPER; see `dev/notes/0.8.x-pmc-oa-reconsideration.md` |
| S2ORC (Semantic Scholar Bulk Dataset API) | `paper` | 1,000 | ODC-By 1.0 (attribution) | cache-only (Semantic Scholar TOS) |
| OpenAlex (AWS Registry) | `paper` (metadata enrichment only) | — | CC0 | commit-eligible |
| CNN/DailyMail (HF abisee/cnn_dailymail) | `article` | 2,500 | Apache-2.0 | commit-eligible |
| Landes/Di Eugenio to-dos (plandes/todo-task) | `todo` (seed) | 500 | MIT | commit-eligible |
| bahmutov daily-logs | `note` (style seed) | 300 | MIT | commit-eligible |
| Synthetic notes (this project) | `note` | 1,200 | project license | commit-eligible |
| Synthetic chain connectives (Corpus-Pack 2) | mixed (`note`/`email`/`todo`) | ~200 chains, ~600 docs | project license | commit-eligible; provenance=`synthetic-chain` |
| WEC-Eng (HF `Intel/WEC-Eng`) | `event` | 3,000 (sampled; QID-carrying) | undeclared on HF card; Wikipedia-derived (CC-BY-SA-4.0) + extract-wec Apache-2.0 | cache-only until posture clarified |
| **Total target** | | **~10,000** | | |

### Cross-source QID spine (2026-07-02 — HITL coreyt, Slice B1)

**HITL-approved cross-source expansion (2026-07-02).** The `event` + `kb`
`source_type` values and the additive `entity_ids` join key (see §"Source
vocabulary" and §"Document schema") admit QID-native corpora into a shared key
space so heterogeneous corpora can interconnect — the existing corpus had zero
cross-source joins.

**WEC-Eng** (Intel/WEC-Eng, Wikipedia Event Coreference; Eirew et al. 2021,
NAACL) is the pattern-setter: each gold event mention is anchored to a Wikipedia
event page (`coref_link`), whose title is resolved to a Wikidata **QID** via the
MediaWiki `pageprops.wikibase_item` API and carried as
`entity_ids=[EntityRef(id=<QID>, kind="qid", surface=<mention>)]`. The
acquisition (`acquire_wec_eng.py`) is config-driven (`--split`, `--sample-size`,
`--seed`) and deterministic (fixed provenance `created_at`; seeded,
mention_id-sorted sampling). A 3,000-mention train sample resolved at **~99.9 %
QID coverage** (2,998/3,000 docs, ~2,056 distinct QIDs). This is the template to
be copied for CompMix / MultiHop-RAG / S2ORC cross-source acquisitions.

PMC OA remains **deferred** after the 2026-06-02 reconsideration.
QASPER now fills `paper` for 0.8.x, and PMC OA still needs
per-article Commercial-Use filtering plus an allowed PMC retrieval
channel.

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

- All produced data lives under `data/corpus-data/`
  (gitignored):
  - `data/corpus-data/downloads/` — raw upstream artifacts
    (e.g. `enron_mail_20150507.tar.gz`). Kept as-is once
    downloaded; never deleted by the pipeline.
  - `data/corpus-data/raw/{source}.jsonl` — canonical
    per-source JSONL produced by acquisition scripts.
  - (Future) `data/corpus-data/chains/{chain_id}.json` —
    Corpus-Pack 2 cross-document chain definitions.
- GitHub Actions cache key:
  `corpus-vB-<checksum of scripts/manifest.json>`.
- Cache hit → restore `data/corpus-data/`.
- Cache miss → run the per-source `acquire_*.py` /
  `generate_*.py` scripts in `tests/corpus/scripts/`. Each
  script re-fetches from upstream at its pinned revision,
  verifies upstream checksums, and writes the JSONL. The
  manifest sha256 must match after the run.
- Scripts are the reproducible source of truth; data is not
  tracked in git regardless of license posture.

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

- verified by the build pipeline). Summary table:

| Source | Upstream ID | Snapshot | Output SHA-256 |
|---|---|---|---|
| CNN/DailyMail | HF `abisee/cnn_dailymail` config `3.0.0` | rev `96df5e68…6223d` (2024-01-18) | `7c371528…493d09` |
| Landes to-dos | GH `plandes/todo-task` `resources/todo-dataset.json` | rev `06bcd261…d26f` (2018-06-29) | `74f02482…0e812` |
| bahmutov daily-logs | GH `bahmutov/daily-logs` monthly Markdown | rev `521476da…19b7` (2020-09-01) | `bdecf3e8…0e81` |
| Enron (CMU) | CMU `enron_mail_20150507.tar.gz` | tarball sha `b3da1b3f…48ca7` (2015-05-07) | `c4df0c71…486ab` |
| Synthetic notes | generator seed `0x53EEDFA7C012B0F1` | n/a | `b4d19f05…55eb3` |
| QMSum | GH `Yale-LILY/QMSum` archive | rev `83d7768c…bb7e` (2023-08-29), arch sha `b6970687…7d7f` | `19a2e5b4…5e2e` |
| EnronQA | HF `MichaelR207/enron_qa_0922` | rev `c0b3a919…221e` (2024-09-22) | `bc30eb06…48ab`; eval `756e7c48…85c0` |
| QAConv | GH `salesforce/QAConv` `dataset/QAConv-V1.1.zip` | rev `b1f140c3…6407` | `8c416c76…5d1d`; eval `a4a3bc33…f4cf` |
| QASPER | HF `allenai/qasper` static v0.3 archives | rev `fdc9d821…4a54` | `e84746e1…8a0a`; eval `299866fc…39c1` |
| QMSum eval QA | GH `Yale-LILY/QMSum` archive | rev `83d7768c…bb7e` | eval `85eb381c…8a5` |

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
