# Corpus creation — architecture

## 1. Why a real-data composite corpus exists

FathomDB's pre-existing retrieval fixture
(`tests/perf_gates.rs::seed_ac013_corpus`) generates 1M synthetic
"doc" rows whose embeddings concentrate on six of 768 vector
coordinates via FNV-1a hashing of a Zipf-sampled vocabulary. That
fixture is fine for **latency** gating (AC-013 / AC-019) and
survives the PVQ Pack 2 binary-quant migration, but it is **not
representative**:

- Zero metadata variety (every doc has the same `kind="doc"` and no
  other fields).
- Zero threading or cross-document links.
- No real natural-language structure for FTS to chew on.
- Atypically friendly to bit-sign quantization because of the
  embedder's concentrated-mass property.

PVQ Pack 2's recall-floor RED test (`recall@10 ≥ 0.90` vs f32
ground truth) was originally going to run against this fixture too,
which risks the gate passing for the wrong reason — recall on a
distribution that doesn't look like production. The composite
corpus exists to give the recall test (and downstream retrieval
work) something realistic to score against:

- **FTS** — real natural-language bodies across six modalities.
- **Vector** — real text run through the same embedder the perf
  gates use, so the bit-quant fast path can be exercised on
  production-shaped input.
- **Graph** — explicit `parent_doc_id` / `thread_id` relations so
  cross-document retrieval has something to traverse.

The composite is **one artifact, three modalities**. Not three
parallel sub-corpora — a single set of JSONLs that exercise all
three retrieval paths through the same ingest harness.

## 2. Source selection (how the catalogue was chosen)

The drafting research is in
[`dev/notes/0.7.0-test-corpus-research.md`](../notes/0.7.0-test-corpus-research.md);
it enumerates candidate sources per modality (email, meeting,
paper, article, note, todo) and grades each on (a) availability,
(b) license posture, (c) metadata richness, (d) FathomDB fit.

The decision lens, in priority order:

1. **License clarity before convenience.** A dataset with strong
   license terms (Apache-2.0, MIT, CC0) is preferred over a richer
   dataset with murky terms. Where the dataset annotations are
   clean but the underlying content has chain-of-license issues
   (QMSum's AMI/ICSI/Parliament heritage), the source lands as
   cache-only with a note.
2. **Real, not scraped.** Sources that ship as a documented dataset
   (HuggingFace, GitHub release, CMU archive) over sources that
   require scraping. Scraping introduces fragility, robots.txt
   compliance, and re-distribution ambiguity.
3. **Per-source-type coverage over single-source depth.** Six
   `source_type` values are locked
   ([§3](#3-source-vocabulary-locked)). Each gets at least one
   source. Where two viable sources exist for one modality (e.g.
   Enron + EnronQA for email), include both for blend variety.
4. **Production-shaped bodies.** Bodies should look like what a
   personal-knowledge user might have (a real email, a real
   meeting transcript, a real to-do list — not a `lorem ipsum`
   stub).
5. **Cross-modal anchoring potential.** Sources whose docs can
   plausibly link to other modalities are preferred so the chain
   generator has anchors. Enron emails (with senders, threads, and
   topics) anchor better than a CC-News article (no per-row
   structure).

Sources that were considered and rejected:

- **NOW Corpus** (news): access-blocked behind academic
  authentication, no bulk download path.
- **CC-News** (CommonCrawl): tracked for v2; preferred over
  CNN/DailyMail in the abstract, but the CC-News dump is much
  bulkier and the per-article copyright story is more nuanced.
  CNN/DM with Apache-2.0 on the HF distribution is a cleaner v1.
- **arXiv bulk** (papers): fallback if S2ORC API access closes.
- **AESLC** (Annotated Enron Subject Line Corpus): fallback if
  Enron itself becomes unavailable.

The HITL pass on 2026-05-27
([`dev/plans/0.7.0-HITL-recommendations.md`](../plans/0.7.0-HITL-recommendations.md))
locked the v1 set. PMC OA (1500 papers) and S2ORC (1000) were
deferred behind Corpus-Pack 2 because both have non-trivial access
machinery (PMC E-Utils + per-article CC filter; S2ORC needs a
Semantic Scholar API key) and the corpus had enough coverage
without them.

## 3. Source vocabulary (locked)

The vec0 `partition_key` and the `source_type` document field are
both locked to exactly six values. This vocabulary is the contract
between every acquisition script, the chain generator, the ingest
harness, and the (post-PVQ-Pack-1) vec0 partition layout.

| `source_type` | Examples | Anchor role for chains |
|---|---|---|
| `email`   | Enron sent-folder messages, EnronQA inbox | strong (sender, thread_id, subject) |
| `meeting` | QMSum transcripts + query-summary pairs | strong (intra-meeting parent_doc_id) |
| `paper`   | (PMC OA + S2ORC, deferred) | not used in v1 chains |
| `article` | CNN/DailyMail news | medium (no per-doc people/projects) |
| `note`    | bahmutov daily-logs, synthetic notes | strong (referenced by every chain shape) |
| `todo`    | Landes imperative-todo + synthesized variants | medium |

Do not add a seventh value without a HITL pass — the vec0
partition_key cardinality and the test vocabularies (`SOURCE_TYPES`
in `_corpus_lib.py`, `VALID_SOURCE_TYPES` in `ingest_corpus.rs`,
chain shapes in `generate_chain_corpus.py`) all depend on it.

## 4. Document schema (canonical JSONL)

Every produced JSONL row — across every source, real or synthetic
— shares the same shape. The contract is owned by
`CorpusDoc` in `tests/corpus/scripts/_corpus_lib.py`:

```jsonc
{
  "doc_id":            "16-hex SHA-256 of (provenance|native-id)",
  "source_type":       "one of {email,meeting,paper,article,note,todo}",
  "title":             "string or null",
  "body":              "string — the text the engine will chunk + embed",
  "created_at":        "ISO-8601 UTC",
  "modified_at":       "ISO-8601 UTC or null",
  "author_or_sender":  "string or null",
  "recipients":        ["array of strings"],
  "people_mentions":   ["array of strings"],
  "project_mentions":  ["array of strings"],
  "tags":              ["array of strings — including style/intent/relation markers"],
  "url_or_external_id":"string or null",
  "thread_id":         "string or null — groups related docs",
  "parent_doc_id":     "string or null — doc_id of a parent in a chain/thread",
  "license":           "SPDX identifier or LicenseRef-<...>",
  "provenance":        "short upstream tag (e.g. cmu-enron-2015-05-07)"
}
```

Invariants:

- **`doc_id` is deterministic.** `_corpus_lib.doc_id(provenance,
  native_id)` returns the first 16 hex chars of
  `SHA-256("{provenance}|{native_id}")`. Two scripts that emit the
  same `(provenance, native_id)` pair will collide on `doc_id` —
  that's intentional for idempotency-style joins, but it means
  every script must pick a `native_id` that is locally unique
  within its provenance.
- **Sorted output.** `write_jsonl` writes rows in the order the
  generator emits them. For determinism, every acquisition script
  enumerates upstream content in a documented, sorted, or
  index-bound order (e.g. tarball-internal order for Enron, sorted
  meeting paths for QMSum, HF stream order for CNN/DM at a pinned
  revision).
- **`relation:<kind>` tags are how the ingest harness picks edge
  kinds.** When a doc has a `parent_doc_id`, the harness emits a
  `PreparedWrite::Edge` whose `kind` is taken from any
  `relation:<kind>` tag on the doc. The 7-value relation
  vocabulary is locked in `RELATION_TYPES` (see
  `_corpus_lib.py` + `ingest_corpus.rs` + the chain generator).

## 5. Per-source acquisition

Every source ships as a `tests/corpus/scripts/acquire_<name>.py` (or
`generate_<name>.py` for purely synthetic content). Scripts use
`uv run --script` with PEP-723 inline metadata — each declares its
own deps and pinned Python version. Scripts must not import each
other except through `_corpus_lib` (which is path-injected, not
packaged).

The contract every script honors:

- **Upstream pin.** A specific revision/commit SHA or tarball SHA
  recorded both in the script's source (a top-level constant) and
  in `tests/corpus/scripts/manifest.json` under `upstream.revision`
  (or `upstream.archive_sha256`, etc.).
- **Output sha256.** After writing, the script prints `sha256 =
  <hex>` and that value lives in the manifest under `sha256`. A
  second run that doesn't match the manifest is a determinism
  regression and must be investigated.
- **Single output file.** One JSONL at
  `data/corpus-data/raw/<source>.jsonl`. No sidecars.

Per-source specifics:

### 5.1 CNN/DailyMail (`acquire_cnn_dailymail.py`)

- Source: HuggingFace `abisee/cnn_dailymail` config `3.0.0`.
- Pinned: revision `96df5e686bee6baa90b8bee7c28b81fa3fa6223d`
  (last-modified 2024-01-18).
- License: Apache-2.0 (on the prepared dataset). Commit-eligible.
- Count: 2,500 articles from the `train` split.
- Stream order: `datasets.load_dataset(..., streaming=True)`. Take
  the first 2,500 rows in stream order. Streaming avoids
  downloading the full 2.5 GB parquet bundle.
- Caveats: the HF distribution strips per-article timestamps and
  URLs. We synthesize `created_at` deterministically from
  SHA-256(hf_id) mod the 2007-04-01 → 2015-04-01 window, and mark
  provenance as `hf:abisee/cnn_dailymail@3.0.0+synthetic-date`.

### 5.2 Enron (`acquire_enron.py`)

- Source: <https://www.cs.cmu.edu/~enron/enron_mail_20150507.tar.gz>
  (443 MB).
- Pinned: tarball SHA-256
  `b3da1b3fe0369ec3140bb4fbce94702c33b7da810ec15d718b3fadf5cd748ca7`,
  Last-Modified 2015-05-07. ETag `"1a6b8803-51583da2f8640"`.
- License: research-use; no explicit OSI license. HITL approved on
  2026-05-27 for inclusion with the **CMU April-2026 message
  impersonation** note recorded in
  `tests/corpus/corpus-card.md` §"Provenance notes".
- Count: 2,000 messages across 149 users, round-robin balanced
  (3-14 per user).
- Critical implementation detail — the tarball has ~520k files and
  is gzipped, so it has no random-access index. The original
  implementation used `tarfile.getmembers()` + `extractfile(...)`
  per chosen file, which forced the gzip stream to be re-read for
  every extraction (O(N²) decompression). A 2000-message extraction
  against this scheme ran for hours before being killed. The
  shipped script uses **single-pass streaming** via `tarfile.open(
  mode="r|gz")` and `for m in tf: ...` — one full sequential
  decompression, ~2 minutes total.
- Subsample policy: walk the tarball in upstream-internal order;
  for each user, keep at most 30 sent-folder messages (folders
  matching `^_sent_mail$|^sent_items$|^sent$`); round-robin across
  alphabetically-sorted users in a second pass to fill exactly
  2,000.
- Body cleaning: signature strip cuts at the first standalone
  `--` line.
- Date parsing: `email.utils.parsedate_to_datetime`; fallback
  `2001-01-01T00:00:00Z` if the Date header is missing or
  malformed.
- Tarball cache: `data/corpus-data/downloads/enron_mail_20150507.tar.gz`
  (gitignored). Override path via `$ENRON_CACHE_TARBALL`.

### 5.3 EnronQA (`acquire_enronqa.py`)

- Source: HuggingFace `MichaelR207/enron_qa_0922` (Ryan et al. 2025).
- Pinned: revision
  `c0b3a9190fd970e83cfbe7d399a08860e43e221e` (last-modified
  2024-09-22).
- License: **not declared** on the HF dataset card; derived from
  the Enron base. **Cache-only** until clarified.
- Count: 200 emails from the `train` split.
- The dataset's per-row schema includes question/answer pairs that
  could feed chain ground-truth queries in a future pack; for now
  only the `email` body is ingested. QA pairs are deferred.
- The EnronQA bodies are richer than the raw Enron sent-folder
  messages (each ships with structured Subject / Sender /
  Recipients / File headers in the body itself), which is part of
  why the corpus blend keeps both.

### 5.4 QMSum (`acquire_qmsum.py`)

- Source: GitHub `Yale-LILY/QMSum` (NAACL 2021 dataset repo).
- Pinned: commit
  `83d7768c1f2b4dfeb091385d3dc7e239b8e5bb7e` (2023-08-29). Repo
  archive SHA-256
  `b6970687b0f56dbd0a7f66a2ff15c501a3e57e6f60750971466c678cf5b17d7f`.
- License: **mixed** — QMSum annotations are MIT, but the
  underlying transcripts derive from AMI (CC-BY-4.0), ICSI
  (CC-BY-NC variants), and Canadian Parliament Standing Committee
  meetings (Crown copyright, Reproduction-of-Federal-Law-Order).
  This chain is **not verified** for unrestricted redistribution;
  treat as **cache-only** until a future HITL pass.
- Count: 600 docs across 200 meetings.
- Composition (per meeting, in order):
  1. transcript doc (`body` = concatenated `Speaker: content`),
  2. first general-query summary (title = query, body = answer),
  3. first specific-query summary (same shape).
  → 3 docs/meeting × 200 meetings = 600.
- Why archive-tarball over per-file fetch: 700 round trips vs 1.
  GitHub's `codeload.github.com/.../tar.gz/<sha>` endpoint gives a
  pinned tarball.
- Threading: every QMSum doc carries `thread_id = meeting_id`.
  Query-summary docs set `parent_doc_id = transcript_doc_id` and
  carry `relation:summarizes` so the ingest harness gives their
  edges the right kind.

### 5.5 Landes/Di Eugenio to-dos (`acquire_landes_todos.py`)

- Source: GitHub `plandes/todo-task`
  `resources/todo-dataset.json` (Landes & Di Eugenio 2018).
- Pinned: commit
  `06bcd261fe09e767282c73bf59a480a71bd8d26f` (2018-06-29).
- License: MIT. Commit-eligible.
- Count: 500 docs (253 real + 247 synthesized variants).
- The upstream resource has only 253 imperative to-dos — short of
  the 500 target — so the script synthesizes 247 variants by
  remixing each real text with deterministically-chosen project,
  assignee, due date, and priority slot values. Synthesis seed is
  the row's native_id + variant index.
- Variant docs carry provenance
  `github:plandes/todo-task@<sha>+synth` to distinguish them from
  the 253 real-text-with-synth-metadata rows
  (`...@<sha>+synth-metadata`).

### 5.6 bahmutov daily-logs (`acquire_bahmutov_dailylogs.py`)

- Source: GitHub `bahmutov/daily-logs` (Gleb Bahmutov's public
  daily-standup notes for 2019-2020).
- Pinned: commit
  `521476da90da3c3f095e458c2b92e8bf379819b7` (2020-09-01). License
  MIT (verified via `package.json` on the pinned revision —
  GitHub's license-detection didn't surface it from the repo root
  because the repo has no `LICENSE` file).
- Count: 300 daily-section notes.
- The repo has 18 monthly Markdown files (e.g.
  `2019/03-March-2019.md`). The script fetches each file at the
  pinned SHA, splits on H2 day-headings
  (`## Wednesday 2019-03-06`, `## The Weekend`), and emits one
  note doc per section. Dates parsed from headings where present;
  undated weekend sections inherit the previous dated section's
  date. `@tag` markers in the body (`@feature`, `@example`, ...)
  are surfaced as corpus tags (`tag:feature`, etc.).

### 5.7 Synthetic notes (`generate_synthetic_notes.py`)

- Source: pure generator; no upstream fetch.
- License: Apache-2.0 (project license).
- Count: 1,200 notes — 150 per style × 8 styles (fleeting,
  project, reading, idea, decision-log, someday-maybe,
  personal-crm, meeting-follow).
- Seed: `0x53EEDFA7C012B0F1` (locked).
- Entity vocabularies (people, projects, technologies, papers) are
  fixed lists chosen so the chain generator can link Pack-1
  synthetic notes to:
  - Landes-style todos (same `PROJECTS` + `ASSIGNEES` lists),
  - Enron sent-folder messages (the `PEOPLE_ENRON` slice includes
    real Enron user handles like `phillip.allen` and
    `jeff.dasovich`).
  - No per-source coupling: each acquisition script doesn't import
    the others, but the vocabularies are picked to overlap.

### 5.8 Chain connectives (`generate_chain_corpus.py`)

- Source: pure generator that READS Pack-1 JSONLs.
- License: Apache-2.0.
- Output: `data/corpus-data/raw/chain_connectives.jsonl` (367
  docs) **plus** `tests/corpus/chains/chain-*.json` (200 chain
  spec files, committed).
- See [§7 Chain generator](#7-chain-generator-pack-2) for the
  design.

### 5.9 QAConv (`acquire_qaconv.py`)

- Source: GitHub `salesforce/QAConv` at commit
  `b1f140c39580dd4dadb4ecd35e9a247a90016407`, file
  `dataset/QAConv-V1.1.zip`.
- License: BSD-3-Clause. Commit-eligible.
- Count: 1,250 conversation segment docs, mapped only to existing
  `email`, `meeting`, and `note` source types.
- Eval: 2,303 grounded QA rows in
  `data/corpus-data/eval/qaconv_qa.jsonl`.
- Subsample policy: sort by split/source/segment/question and select
  balanced source-type coverage before applying the document cap.

### 5.10 QASPER (`acquire_qasper.py`)

- Source: Hugging Face `allenai/qasper` metadata at revision
  `fdc9d8214fbab5dd782958601db4d678e6934a54`; raw v0.3 JSON is
  read directly from the QASPER S3 archives instead of executing the
  HF custom dataset loader.
- License: CC-BY-4.0. Commit-eligible with attribution.
- Count: 1,585 paper docs, filling the previously-empty `paper`
  source type.
- Eval: 7,993 answer-level QA/evidence rows in
  `data/corpus-data/eval/qasper_qa.jsonl`.
- Chain impact: `generate_chain_corpus.py` now supports
  `PAPER->NOTE->TODO` using existing `summarizes` and `action_from`
  relation tags. Full chain regeneration requires all Pack-1 raw
  JSONLs to be present.

### 5.11 Existing-source QA exports

- EnronQA remains an existing 200-doc `email` source. Its acquisition
  script now also emits 710 eval-only QA rows, grounded only to the
  selected EnronQA docs. License posture remains cache-only.
- QMSum remains an existing 600-doc `meeting` source. Its acquisition
  script now also emits 1,584 original query-answer rows, grounded to
  the selected meeting transcript docs. License posture remains
  cache-only until the upstream transcript chain is verified.

### 5.12 PMC OA reconsideration

PMC OA is still deferred for 0.8.x. The Commercial-Use bucket remains
available, but automated retrieval must use PMC-approved channels and
per-article licenses still need filtering. See
[`../notes/0.8.x-pmc-oa-reconsideration.md`](../notes/0.8.x-pmc-oa-reconsideration.md).

## 6. License posture and distribution

All produced JSONL — regardless of upstream license posture —
lives under `data/corpus-data/` (gitignored). Scripts in
`tests/corpus/scripts/` are the reproducible source of truth; data
is rebuilt locally or restored from CI cache. This subsumes the
earlier "commit-eligible JSONL goes in-tree, cache-only stays out"
split.

The per-source `distribution` field in `manifest.json` retains a
license-posture marker (`commit` vs `cache`) so a future policy
decision could promote license-clean sources to in-tree without
re-litigating which ones qualify:

| Source | Distribution marker | Why |
|---|---|---|
| CNN/DailyMail | `commit` | Apache-2.0 on the prepared dataset |
| Landes to-dos | `commit` | MIT |
| bahmutov daily-logs | `commit` | MIT |
| Enron | `commit` | research-use; HITL approved with impersonation note |
| Synthetic notes | `commit` | project license |
| Chain connectives | `commit` | project license |
| QMSum | `cache` | upstream license chain not verified (AMI/ICSI/Parliament) |
| EnronQA | `cache` | undeclared license on HF card |
| QAConv | `commit` | BSD-3-Clause |
| QASPER | `commit` | CC-BY-4.0; attribution required |
| (Deferred) PMC OA | `cache`-eligible | mixed CC; needs per-article filter |
| (Deferred) S2ORC | `cache` | ODC-By 1.0; Semantic Scholar API TOS |
| (Deferred) ELITR | `cache` | CC-BY-NC-SA 4.0 |

The `tests/corpus/scripts/manifest.json` "comment" field carries
the canonical statement of this posture.

## 7. Chain generator (Pack 2)

The single most important Pack-2 design decision: chains anchor
on REAL documents from Pack-1, with synthetic connective documents
generated to glue real anchors together.

A chain is a small ordered set of doc_ids spanning multiple
`source_type` values. Each chain has:

- A `chain_shape` (e.g. `EMAIL->NOTE->TODO`, `MEETING->TODO->NOTE(contradicts)`).
- One or more `anchor_doc_ids` (real Pack-1 docs).
- Some number of `synthetic_doc_ids` (Pack-2-generated docs that
  reference the anchors via `parent_doc_id` + tags).
- A list of `ground_truth_queries` — each query is a natural-
  language string with a list of `expected_top_k_doc_ids` and a
  `relation_type` from the locked 7-value vocabulary.

### 7.1 Chain shapes shipped in v1

Seven supported shapes, rotated deterministically over 200 chain indices
when every anchor source is present:

| Shape | Anchor source | Synthetic docs | Why |
|---|---|---|---|
| `EMAIL->NOTE->TODO` | Enron message | note (summary) + todo (action_from) | covers the canonical "email → followup → action" flow |
| `ARTICLE->NOTE->EMAIL` | CNN/DM article | note + email (mentions) | "saved article → reading note → share" |
| `MEETING->TODO->NOTE(contradicts)` | QMSum meeting | todo + note (contradicts) | exercises the `contradicts` relation (decision reversal) |
| `EMAIL->MEETING->TODO` | Enron + QMSum (2 anchors) | todo (follows_up_on) | cross-anchor chain |
| `ARTICLE->NOTE->TODO` | CNN/DM article | note + todo | "article → followup TODO" |
| `TODO->NOTE->EMAIL` | Landes todo | note + email | "in-progress to-do → status note → ask for help" |
| `PAPER->NOTE->TODO` | QASPER paper | note + todo | "paper reading → note → follow-up action" |

The 7-value relation vocabulary
(`replies_to`, `follows_up_on`, `summarizes`, `action_from`,
`contradicts`, `mentions`, `cites`) covers email threading,
chain-of-decisions, and citation linkage. v1 chains use 5 of the
7: `replies_to` requires real intra-Enron threading (where the
parent message is also in the corpus, which is rare given our
subsample); `cites` requires a `paper` source_type (deferred with
PMC/S2ORC).

### 7.2 Determinism

`SEED = 0xC4A1C0A0C4A1AB1E`. For each chain index `i`, the
generator derives a per-chain RNG via
`SHA-256(SEED + "|" + chain_id)`. Every selection (anchor choice,
slot-filled person/project, body text variation, created_at
offset) draws from that RNG. A re-run with the same Pack-1 JSONLs
produces bit-identical chain_connectives.jsonl + chain JSONs.

Sort order is critical for anchor selection: anchor JSONLs are
**sorted by `doc_id`** before being indexed, so a Pack-1 re-run
that changes file order (but not content) doesn't change which
anchors get picked.

### 7.3 Volume cap (handoff guard)

Per the handoff:

> If the synthetic chain generator's output exceeds 20% of the
> corpus by doc count, escalate — the corpus should be
> predominantly real data with synthetic chains layered on, not
> the reverse.

The Pack-2 output (367 connectives on a 7,300-doc Pack-1 base) is
**4.8%** of the total corpus — well under the cap. This means the
chain generator was successfully scoped as a thin layer on top of
real data.

### 7.4 Output

- `tests/corpus/chains/chain-<shape>-<index>.json` (one file per
  chain, **committed**). Schema (sorted keys, indent=2 — readable
  diffs):

```jsonc
{
  "anchor_doc_ids":     ["..."],
  "chain_id":           "chain-email_note_todo-0000",
  "chain_shape":        "EMAIL->NOTE->TODO",
  "doc_ids":            ["anchor", "...", "synthetic"],
  "ground_truth_queries": [
    {
      "query":                  "what was decided after the email about ...?",
      "expected_top_k_doc_ids": ["..."],
      "relation_type":          "summarizes"
    }
  ],
  "synthetic_doc_ids":  ["..."]
}
```

- `data/corpus-data/raw/chain_connectives.jsonl` (gitignored —
  produced bulk data). One row per synthetic connective doc, same
  canonical schema as every other JSONL.

## 8. Ingest harness (Pack 3)

`src/rust/crates/fathomdb-engine/examples/ingest_corpus.rs`. A
single-file Rust CLI launched via:

```bash
cargo run --example ingest_corpus -p fathomdb-engine -- \
  --db tests/corpus/.cache/db \
  --jsonl-dir data/corpus-data/raw \
  --chains-dir tests/corpus/chains
```

Mapping:

| Corpus field | PreparedWrite mapping |
|---|---|
| `body` | `Node.body` |
| `source_type` | `Node.kind` (string passed through to the engine) |
| `doc_id` | `Node.source_id = Some(doc_id)` — the recovery seam |
| `parent_doc_id` (when target is in the corpus) | `Edge.from = parent_doc_id`, `Edge.to = doc_id`, `Edge.source_id = Some(doc_id)`. `Edge.kind` is taken from the child's `relation:<kind>` tag, defaulting to `"linked"`. |
| `thread_id` | Not currently ingested as an edge — would need a separate "thread" relation kind. Tracked as future work. |
| Chain JSONs (the `tests/corpus/chains/<chain_id>.json` files) | **Not ingested** as docs — they are eval-only ground truth. The harness validates each chain by checking every chain doc_id is present in the corpus. |

Idempotency: `engine.trace_source_ref(doc_id)` is called per doc
before any write. If the source_id already has any canonical
event, the doc + its edge are skipped. Re-running the harness on
the same DB produces 0 new writes.

Batching: nodes and edges are flushed in batches of
`NODE_BATCH = 200` / `EDGE_BATCH = 200`. The harness reports a JSON
closure summary on stdout when done (counts, per-source-type /
per-relation breakdowns, chains validated, elapsed time, engine
counters).

End-to-end timing on dev-box for the full 7,667-doc Pack-1+Pack-2
corpus: **1.77 seconds** — well under the 10-minute handoff
budget. Idempotent re-run completes in ~0.5s.

### Known issue with batched ingest

`engine.write(&batch)` reserves a single `write_cursor` for the
whole batch (cursor advances by `batch.len()` but every node in
the batch is INSERTed with the same final cursor). The projection
runtime then writes one `vector_default` row per cursor. So a
batch of N nodes lands as N rows in `canonical_nodes` but only
**1 row** in vec0 (the rest collide on `INSERT OR IGNORE
vector_default(rowid, ...)`).

Impact:

- The ingest harness's vec0 ends up ~`(N_docs / NODE_BATCH)` rows
  rather than `N_docs`. For 7,667 docs at batch 200, that's ~38
  vec0 rows — vastly under-populated.
- Per the same bug, `tests/perf_gates.rs::seed_ac013_corpus`
  (BATCH=1024) populates ~977 vec0 rows for a 1M ingest. The
  PVQ Pack 2 recall@10 ≥ 0.90 RED test's measurement is therefore
  not what it appears to be at face value, and needs review.

Full root cause and suggested resolution are in
[`../notes/0.7.0-engine-batch-vec0-collapse.md`](../notes/0.7.0-engine-batch-vec0-collapse.md).
The Pack-4 validation tests work around this by issuing
one `engine.write` call per node; the corresponding fix in the
ingest harness is deferred until the engine owner picks up the
underlying bug.

## 9. Validation gates (Pack 4)

Three `cargo test` integration tests under
`src/rust/crates/fathomdb-engine/tests/`, with shared helpers at
`tests/support/corpus_subset.rs`. Each test gracefully skips (with
a `SKIP:` log line, no failure) when `data/corpus-data/raw/` is
absent, so `cargo test` stays green in environments without the
corpus checked out.

### 9.1 `corpus_fts.rs` — FTS gate

For a small subset (5 docs/source × 8 sources × short-body filter
≈ 40+ docs), picks a salient long word from each body and asserts
`engine.search(term)` returns at least one hit. Floor: 0.80 hit
rate across 20+ queries.

This is a wiring gate, not a recall gate — it confirms the FTS5
path is wired through `engine.search` and that production-shaped
docs across all source_types are searchable.

### 9.2 `corpus_vector.rs` — vector wiring gate

Two assertions:

1. `engine.search(body)` returns non-empty results for body-derived
   queries — hit rate ≥ 0.90 (catches catastrophic FTS+vector
   silence).
2. After ingest + drain + close, `vector_default` has at least one
   row per short-body ingested doc — verified by opening a
   read-only rusqlite connection to the engine's `.sqlite` file
   directly. This catches the §8 batched-vec0-collapse bug at
   `N=1`-write granularity (the helper does one engine.write per
   node, exactly to make this assertion meaningful).

The handoff originally specified top-K self-recall as the vector
gate. That assertion is **deliberately moved to the PVQ Pack 2
AC-013b RED test**:

- VaryingEmbedder's 6-coordinate hash placement has high
  collision rates on natural-language bodies with shared
  structure (e.g. all bahmutov daily-logs start with bullet-list
  markers).
- AC-013b runs against the canonical 1M-row fixture under
  `AGENT_LONG=1`; that's where statistical recall on a real
  embedder belongs.

### 9.3 `corpus_graph.rs` — graph gate

For 20 chains, ingest every chain's anchor + synthetic docs, then
(after close) open a read-only sqlite connection and verify
`canonical_edges` has at least one in-chain edge for every chain.
Catches a regression where the ingest harness drops
`parent_doc_id` → `Edge` mappings.

No public engine API yet exposes "edges from <from_id>"
traversal; the test queries `canonical_edges` directly via
rusqlite. That direct read is a test-only escape hatch and
shouldn't migrate into product code.

### 9.4 Timing

All three tests finish in ~1.3s total on dev-box (handoff budget:
<30s). They run at default `cargo test` scale — no `AGENT_LONG`
gate, no fixture build step. The Pack-1 + Pack-2 JSONLs must be
present on disk (or the tests skip).

## 10. Layout summary

```text
fathomdb/
├── dev/
│   ├── corpus-creation/                ← you are here
│   │   ├── README.md
│   │   ├── architecture.md
│   │   └── extending.md
│   ├── notes/
│   │   ├── 0.7.0-test-corpus-research.md          ← source-selection research
│   │   └── 0.7.0-engine-batch-vec0-collapse.md    ← open engine bug
│   ├── plans/
│   │   ├── 0.7.0-HITL-recommendations.md          ← HITL locks
│   │   └── prompts/0.7.0-CORPUS-BUILD-HANDOFF.md  ← original handoff
│   └── ...
├── tests/corpus/
│   ├── corpus-card.md                  ← per-source quick reference
│   ├── README.md
│   ├── chains/                         ← Pack-2 ground-truth (200 JSON, committed)
│   └── scripts/
│       ├── _corpus_lib.py              ← shared CorpusDoc + helpers
│       ├── manifest.json               ← pin + sha256 contract
│       ├── acquire_*.py                ← per-source acquisition
│       └── generate_*.py               ← synthetic generators
├── src/rust/crates/fathomdb-engine/
│   ├── examples/ingest_corpus.rs       ← Pack-3 ingest CLI
│   └── tests/
│       ├── corpus_fts.rs               ← Pack-4 FTS gate
│       ├── corpus_vector.rs            ← Pack-4 vector wiring gate
│       ├── corpus_graph.rs             ← Pack-4 graph gate
│       └── support/corpus_subset.rs    ← Pack-4 shared loader/ingest
└── data/corpus-data/                   ← PRODUCED DATA (gitignored)
    ├── downloads/                      ← raw upstream artifacts (e.g. Enron tarball)
    └── raw/                            ← canonical per-source JSONL
```

## 11. Build sequencing

The minimum dependency graph between packs:

```text
Pack-1 acquire_*.py + generate_synthetic_notes.py
  └── produces data/corpus-data/raw/*.jsonl
       └── Pack-2 generate_chain_corpus.py
             ├── produces data/corpus-data/raw/chain_connectives.jsonl
             └── produces tests/corpus/chains/chain-*.json
                  └── Pack-3 ingest_corpus.rs
                        └── populates a FathomDB instance
                              └── Pack-4 corpus_*.rs (cargo test)
```

Pack-1 scripts are independent of each other — they can run in
any order or in parallel.

Pack 2 reads Pack-1 JSONLs; it must run **after** all Pack-1
scripts have produced output.

Pack 3 needs both Pack-1 and Pack-2 output to ingest a complete
corpus. (You can run it against just Pack-1 — chains are not
required for ingest to succeed — but the chain validation step
will report `chains_missing_doc > 0`.)

Pack 4 needs Pack-1 + Pack-2 output AND a working Rust toolchain.

## 12. Known issues and deferred work

1. **`engine.write` batch / vec0 collapse** (open). See
   [`../notes/0.7.0-engine-batch-vec0-collapse.md`](../notes/0.7.0-engine-batch-vec0-collapse.md).
   Affects the ingest harness's vec0 population and likely the
   AC-013b recall measurement. Owner: PVQ Pack 2 / engine.
2. **PMC OA, S2ORC, ELITR, OpenAlex enrichment** all deferred.
   Each has a license-posture or access-machinery story in
   [§2](#2-source-selection-how-the-catalogue-was-chosen) /
   [§5](#5-per-source-acquisition).
3. **`replies_to` + `cites` relations unused** in v1 chains. Need
   PMC/S2ORC (papers → `cites`) and richer Enron threading (parent
   message is rarely in the sent-folder subsample → `replies_to`
   edges drop on the floor in the ingest harness's "parent in
   corpus" check).
4. **`thread_id` is captured but not ingested as edges.** Future
   work: emit `PreparedWrite::Edge { kind: "in_thread" }` (or
   similar) for docs sharing a `thread_id`, deduped so we don't
   emit N² edges.
5. **EnronQA QA pairs are not ingested.** Only the email bodies
   are. The QA pairs are a natural future input to Pack-2 chain
   ground_truth_queries.
6. **No `paper` source_type docs in v1.** Six locked
   `source_type` values, but only five are populated. Adding PMC
   OA + S2ORC fills the gap.
7. **VaryingEmbedder + body-as-query collisions.** Documented in
   `corpus_vector.rs` and §9.2. Not a Pack-4 failure — moved to
   the PVQ recall gate.

## 13. Versioning policy for this doc

When you change any of the following, update this doc:

- The `source_type` vocabulary (§3).
- The relation vocabulary (§4).
- A source's pinned upstream revision OR output sha256 (§5).
- The chain-shape catalogue (§7.1) or seed (§7.2).
- The ingest harness's mapping rules (§8).
- The validation gate set (§9).

Don't update this doc for cosmetic-only refactors (renaming a
function, reformatting code) that don't change the contract.
