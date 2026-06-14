# Slice 25 — R2 end-to-end parity eval: harness design memo

> Authored before any source, per the slice-25 prompt §3.0. Binding spec:
> `dev/adr/ADR-0.8.1-ir-measure-eval-design.md §3`. This memo records the design
> decisions, the environment constraints discovered at authoring time, and the
> resulting (honest) scope of what the live eval can and cannot produce here.

## 0. The load-bearing constraint (ADR §3.2) — identical answerer

The **same answerer object** (same LLM, same prompt template, same context-window
budget) answers questions retrieved by **all three** systems (FathomDB post-R1,
Mem0-OSS, naive-RAG). Only the retrieval/memory layer differs. This is enforced
*structurally*: adapters expose **only** `retrieve(question, k)`; they have **no**
`answer` method and never build a prompt. The harness owns the single `Answerer`
and routes every system's retrieved context through it. An adapter therefore
*cannot* diverge the prompt — the divergence is impossible to express in the API.
RED-1 verifies this by recording the prompt template the answerer sees for each
system and asserting it is byte-identical across all three.

## 1. Query set

The R2 per-class scheme (ADR §3.4) needs five classes: `factoid`, `temporal`,
`multi_hop`, `knowledge_update`, `multi_session`. Two candidate sources:

- **LongMemEval proper** (500 public Qs; `github.com/xiaowu0162/longmemeval`,
  arXiv 2410.10813). It is the dataset that *natively* carries the temporal /
  multi-hop / knowledge-update / multi-session classes. **Status here: not locally
  cloneable** (no clone on disk; cloning + its conversational haystack + an
  answerer LLM are all required to score it). It also uses its *own* haystack, not
  the frozen FathomDB corpus, so it cannot be pinned to `fe973fcd…`.
- **FathomDB frozen corpus QA task** — the gold set at
  `data/corpus-data/eval/ir_gold/all.gold.json` (`corpus_hash fe973fcd…`,
  `qrels_version ir-c-reused-v1`, **4 597 queries**). Its class labels are
  `exact_fact` (2 888), `exploratory` (1 584), `negative` (125) — **it has no
  temporal / multi_hop / knowledge_update / multi_session labels.**

**Decision.** The harness is written to consume *either* source via a `QuerySet`
abstraction. The default, pinnable, reproducible source is the **frozen FathomDB
gold set**. Class mapping: `exact_fact → factoid`; `exploratory` and `negative`
are carried as report-only extra classes. The four memory classes
(`temporal`/`multi_hop`/`knowledge_update`/`multi_session`) have **no gold
evidence in the frozen corpus** and **no locally available LongMemEval** — so the
harness emits them with `n=0` and `recall_at_k = null` (the §9 null-vs-zero
distinction). This is the honest state and is surfaced loudly as a blocker, not
papered over with zeros.

Per-class N (frozen corpus): factoid=2888, temporal=0, multi_hop=0,
knowledge_update=0, multi_session=0; (extra) exploratory=1584, negative=125.

## 2. Answerer LLM

The answerer is **BYO/local test-infra**, gated behind `R2_RUN=1`. It is *not* a
FathomDB product dependency; FathomDB itself makes **zero** network calls during
the eval (local SQLite engine).

**Status here: no answerer LLM is available** — no `OPENAI_API_KEY` /
`ANTHROPIC_API_KEY` in env, no `ollama`, no local model server. Per the prompt
§3.3 / §7 "Answerer LLM unavailable" path, the live eval therefore produces
**retrieval-only** metrics (per-class Evidence Recall@K) and records
`answerer_available: false`; per-class `answerer_accuracy` is `null` (not `0.0`).

The harness still *implements* the LLM-backed answerer path (an `LLMAnswerer`
reading `R2_ANSWERER_MODEL` + an OpenAI-compatible base URL from env) so that a
future run on a box with a local model produces the answerer-accuracy column with
zero code change — only the env gate flips.

## 3. Ingest strategy for Mem0-OSS

The Mem0-OSS baseline uses the **local** `mem0ai` library (`Memory.add()` to
ingest the frozen corpus docs as memories; `Memory.search()` to retrieve), **never
the Mem0 cloud API** (a footprint violation per ADR §3.6). Ingest is batched per
source document via `Memory.add(text, user_id=...)`.

**Status here: Mem0-OSS cannot run live.** `mem0ai` is installable from PyPI
(verified reachable: latest 2.0.6, 0.1.x line available) but its default
extraction + embedding pipeline requires an LLM and an embedding API (OpenAI by
default); with no LLM/embedder available locally, `Memory.add()` cannot extract
memories. The `Mem0OSSAdapter` is implemented with a **lazy import** so it is
inert unless `mem0ai` + a configured backend are present; absent that, it is a
documented blocker and the live comparison runs **FathomDB vs naive-RAG** only.

## 4. Replay-determinism

- **Corpus pin:** the harness asserts `corpus_hash.startswith("fe973fcd")` before
  producing any number (COR-2). The hash is read from the gold set / snapshot, and
  the corpus was verified bit-identical via
  `freeze_corpus.py --reproduce` (`corpus_hash MATCH`).
- **Gold pin:** `qrels_version` is recorded in the output.
- **Retrieval determinism:** FathomDB RRF fusion is deterministic; naive-RAG BM25
  is a pure, seedless function of the corpus. No randomness in the retrieval path.
- **Answerer determinism (when present):** fixed model id + temperature 0 + fixed
  seed, recorded in the output. Not exercised here (no LLM).

## 5. Scoring function

`PerClassScorer` scores `(question, ground_truth, system_answer, query_class)`:

- **Answerer accuracy (end-to-end):** normalized match — case/whitespace/punct
  folded; correct if any gold answer is a normalized substring of the system
  answer (or vice-versa for short spans). Deterministic, judge-free (an LLM-judge
  is *documented* as the higher-fidelity option but is not used here: no judge LLM,
  and a judge would add variance the prompt warns against). Only computed when an
  answerer ran.
- **Abstention (scored, both directions):**
  - `system_answer is None` while `ground_truth` exists ⇒ **miss**, accuracy 0.0
    for that query (counted, never skipped — RED-2 `test_abstention_counted_as_miss`).
  - `system_answer` present while the query is the negative class (no answer
    exists) ⇒ **false positive**, also 0.0.
  - Per-class **abstention_rate** = fraction of queries the system returned no
    answer for, reported separately.
- **Evidence Recall@K (primary, retrieval-only-safe):** fraction of queries whose
  required gold doc-id appears in the system's top-K retrieved hits. This is
  computable **without** an answerer and is the metric the live run here produces.

## 6. Output / deliverables

- `output.json.r2_per_class_deltas` carries rows for **all five** classes incl.
  `temporal`/`multi_hop`/`knowledge_update` (the Slice 30 go/no-go reads exactly
  these). Where no data exists, `recall_at_k`/`answerer_accuracy` deltas are
  `null`, not `0.0`.
- Results doc: `dev/plans/runs/IR-C-r2-eval-results.md`.
- DOC-INDEX updated with the harness + results doc.

## 7. Consequence for Slice 30 go/no-go (stated up front)

The frozen corpus cannot, by itself, measure the three R3 classes
(temporal/multi_hop/knowledge_update). With LongMemEval unavailable and no answerer
LLM, the live R2 run is **data-limited**: it yields real retrieval-only recall for
`factoid` (+ report-only `exploratory`), and `null` for the memory classes. Per
prompt §11, the orchestrator's Slice 30 go/no-go is therefore a **data-limited
go/no-go → escalate to HITL**; it must NOT flip `use_graph_arm` to `true` on the
strength of absent data. The harness is the durable deliverable: the moment a box
with LongMemEval + a local answerer exists, the same harness fills the memory-class
rows with no code change.
