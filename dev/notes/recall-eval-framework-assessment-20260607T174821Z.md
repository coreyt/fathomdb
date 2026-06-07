# Recall-evaluation-framework assessment (FathomDB-specific)

**Date:** 2026-06-07 (UTC 20260607T174821Z)
**Author:** retrieval-evaluation analyst (read+analyze; no code/test/ADR/build/git changes)
**Repo state:** `main` at `9dcdab8` (HEAD; the task brief's `76caa22` is the docs/prompts
baseline the GA hand-off references — the GA work lives unmerged on
`slice-40-20260607T145013Z`). I verified state from git, not from the brief.
**Scope:** evaluate the pasted multi-layer recall-measurement framework against FathomDB's
actual architecture + the current GA-blocking recall halt. Every load-bearing claim is cited
`file:line`. Things I could not verify are flagged explicitly.

---

## 1. FathomDB — purpose + retrieval/graph capabilities (cited)

**Purpose.** FathomDB is a local-first, embedded (SQLite-based) memory/retrieval database for
personal AI agents. The 0.6.0/0.7.0 rewrite deliberately scoped it as a *retrieval/index
engine* on a locked SDK surface, sequencing substrate-correctness first
(`dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md:32-40`). Its named consumers are
real, public, local-first agent-memory products — Memex, Hermes (Nous Research), OpenClaw —
two of which run on the *same* SQLite + sqlite-vec / FTS5 substrate FathomDB is
(`ADR-0.8.0-agent-memory-retrieval-and-identity.md:46-49`). The consumer class is the
requirement surface.

**Retrieval stack.**
- **FTS:** SQLite FTS5 with `tokenize = 'porter unicode61 remove_diacritics 2'`
  (`src/rust/crates/fathomdb-schema/src/lib.rs:276`, migration 011), scored with `bm25()`.
- **Vector:** sqlite-vec `vec0` virtual table, two-phase **bit-KNN (sign-quant, K=192) → f32
  rerank** (`ADR-0.7.0-vector-binary-quant.md:97-106`, § 2 point 4 line 144-149). The f32
  column is retained for rerank and for the recall ground-truth pass
  (`ADR-0.7.0-vector-binary-quant.md:92-95`).
- **Hybrid fusion (G9):** the two ranked branches are fused with **Reciprocal Rank Fusion**,
  `Σ 1/(RRF_K + rank)`, `RRF_K = 60` (`src/rust/crates/fathomdb-engine/src/lib.rs:3564`,
  `fuse_rrf` at `:3584-3623`). Fusion is on **rank**, never raw `vec_distance_l2`/`bm25`
  scores (they are not comparable; `lib.rs:912-917`). This is the *unconditional* new ranking —
  no legacy-union path, no `fusion_mode` knob
  (`ADR-0.8.0-agent-memory-retrieval-and-identity.md:259-263`).
- **Recency reweight (G12):** an additive weight smaller than one RRF rank-step, off by
  default (`lib.rs:3566-3569`, `apply_recency_reweight` at `:3630-3651`).
- **Rerank hook (G9 rerank):** `rerank_fused()` is an **identity stub** — "returns the fused
  order unchanged for now; the MMR/cross-encoder rerank lands additively in a later slice"
  (`lib.rs:3653-3660`). **FathomDB has no real reranker today.**
- **Filtered KNN (G10):** a **closed** `SearchFilter { source_type, kind, created_after,
  status }` struct (`lib.rs:984-1001`), applied as `AND col=?` fragments on the bit-KNN phase
  (`vector_filter_clause`, `lib.rs:3667`). Not an open DSL. `status` currently prunes every row
  (empty-string sentinel; population deferred — `lib.rs:976-982`).

**Storage granularity (the chunking question).** FathomDB stores **whole node bodies** — one
`body` per `PreparedWrite::Node` (`lib.rs:1010-1019`), and `SearchHit`/`NodeRecord` carry that
same whole `body` (`lib.rs:922-928`, `:938-943`). There is **no chunker** in the engine; the
only `chunk` hits are byte-slicing helpers and a legacy `fathom_chunks` table-existence probe
in recovery (`lib.rs:5374,5647,5684`). So FathomDB indexes and returns documents/notes
whole, not chunks. This is load-bearing for §3.

**Graph.** Graph traversal — G5 `read.neighbors(id, edge_type?, depth=1)` and G6
`search(..., expand=N)` — is **designed but deferred to 0.8.1**, accepted as *revisable
roadmap direction*, not shipped in 0.8.0 (`dev/adr/ADR-0.8.0-graph-traversal-scope.md`
front-matter `target_release: 0.8.1` + status block lines 18-27; `dev/roadmap/0.8.1.md` §1).
The substrate (canonical_edges, `from_id`/`to_id` indexes, `logical_id`-alone identity,
invalidate-not-delete supersession) is landed in 0.8.0 (Slice 15), so graph is "on-track," not
"graph-complete today." Entity/graph expansion as a retrieval *mode* therefore does not exist
yet.

---

## 2. What FathomDB measures for recall TODAY — and the fidelity-vs-relevance axis

FathomDB has **two** recall harnesses on **two different axes**. The distinction is not
incidental; it is written into the recall-floor ADR explicitly.

### 2a. eu7 — ANN/quantization **fidelity** recall (the GA gate)

`src/rust/crates/fathomdb-engine/tests/eu7_real_corpus_ac.rs` measures recall@10 of the
production bit-KNN+f32-rerank path **against the exact full-precision f32 top-10 over the same
model** as ground truth (`measure_recall`, `:423-527`; ground truth computed in-Rust by
brute-force L2 over the same embeddings, `:391-420`). The ADR states the axis in plain words:

> "This is an **ANN / quantization-FIDELITY** recall: ground truth = the exact full-precision
> f32 top-10 over the *same* model… It is **NOT IR-relevance** — it says nothing about whether
> the retrieved results are semantically relevant to a query."
> — `ADR-0.7.0-vector-binary-quant.md:151-158`

This is a **system-internal** property: *does the cheap quantized index reproduce what the
expensive exact index would have returned?* The HITL-locked floor is **≥ 0.90 recall@10**
(`ADR-0.7.0-vector-binary-quant.md:126-133`, § 2 point 4). The 0.937 anchor (CI 0.913–0.957)
was measured on the **pre-expansion** corpus (`:144-149`). It is now the asserting GA gate as
AC-075 (`slice-40-20260607T145013Z:dev/acceptance.md ## AC-075`).

### 2b. eu8 — IR (relevance-judged) recall (a ceiling, deliberately *not* a gate)

`src/rust/crates/fathomdb-engine/tests/eu8_ir_validation.rs` scores `engine.search()` against
**externally-labelled relevant doc_ids** carried in each chain's `ground_truth_queries`
(`:1-34`, `measure_ir_recall` at `:309-364`). This is a **qrels** harness in everything but
name. It already computes **recall@10, precision@10, MRR, and NDCG@10** with binary relevance
(`:127-135`, `:218-278`), and buckets results **per relation-type and per chain-shape**
(`:138-179`, `:321-344`). The ADR records its result and its status:

> "EU-8 measured the embedder's **IR-relevance** ceiling at recall@10 = **0.571**
> (CI 0.530–0.614) on 301 labeled queries… This 0.571 is the embedder/task ceiling and is
> deliberately **not** turned into a gate."
> — `ADR-0.7.0-vector-binary-quant.md:169-179`

This is a **product-value** property: *when the agent issues a real query, are the
externally-judged-relevant documents actually retrieved?*

### 2c. The axis distinction (the central fact)

| | eu7 (the GA gate) | eu8 (report-only) | The framework's "recall" |
|---|---|---|---|
| Ground truth | exact-f32 top-10, *same model* | external human/labelled qrels | qrels |
| Question | does quant reproduce exact-f32? | are relevant docs retrieved? | are relevant docs/facts retrieved? |
| Axis | **ANN/quant fidelity** | **IR relevance** | **IR / agentic relevance** |
| Number | 0.937 → **0.8710** @ N=7,667 | **~0.571** ceiling | n/a (proposed) |
| Property | system-internal | product-value | product-value |

The two numbers (0.937 fidelity vs 0.571 relevance) are **~37 pp apart on purpose** and they
are **not the same metric**. ANN fidelity exceeding the IR ceiling by ~37 pp is the ADR's
own load-bearing conclusion: *quantization is not the bottleneck; the lever for end-to-end
quality is a better embedder or the graph, not K/ANN tuning*
(`ADR-0.7.0-vector-binary-quant.md:172-179`). **This is exactly the framework's "did vector
search work? vs is the necessary evidence present?" distinction — and FathomDB already
discovered it empirically.**

**Is the 0.90 fidelity floor the right *GA gate* for "is the agent's memory useful"?** No — it
is **necessary but not sufficient.** It guarantees the cheap index is faithful to the model's
own opinion; it says nothing about whether the model's opinion is *right*. A product could sit
at 0.99 fidelity and 0.57 relevance and still be a poor agent memory. FathomDB's docs already
acknowledge this (the 0.571 ceiling is recorded but un-gated). The fidelity floor is a
**system-health** gate, not a **product-value** gate.

---

## 3. Point-by-point framework evaluation (Part 2 items 1–3)

### Item 1 — Axis mismatch (the central question)

The framework's three layers — retrieval recall, **evidence recall** (primary), task recall —
are **all on the IR/agentic-relevance axis** (qrels-based: "did the right doc/fact/answer
appear"). FathomDB's current GA gate (eu7/AC-075) is on the **fidelity axis**. **They are not
the same thing**, and the framework, read literally, would replace or supplement a
fidelity gate with relevance gates.

Mapping the framework's layers onto FathomDB:
- **Retrieval recall (layer 1):** FathomDB *already has a partial implementation* — eu8 is a
  qrels-based Recall@10/precision/MRR/NDCG harness (`eu8_ir_validation.rs:309-364`). It is
  report-only and capped at the embedder's ~0.571 ceiling.
- **Evidence recall (layer 2, the framework's primary gate):** **FathomDB does not measure
  this at all.** It has no fact-level labels ("did the *commitment* appear?"), only
  doc-id-level qrels in eu8.
- **Task recall (layer 3):** **Not measured.** No agent-in-the-loop eval exists in this repo.

So of the framework's three layers, FathomDB measures a weak form of layer 1 (report-only),
and nothing of layers 2–3. Its *gating* metric is on a fourth axis (fidelity) the framework
doesn't discuss. The framework is therefore **complementary**, not a substitute: it tells you
what FathomDB is *not* measuring (product value), while eu7 keeps measuring what it *is*
measuring (index health).

### Item 2 — Applicability of each framework element

- **Evidence Recall@K as the *primary* gate — partial fit, large effort.** Conceptually
  correct and aligned with FathomDB's own framing (the 0.571-vs-0.937 gap *is* the argument for
  it). But "ALL evidence required for the correct answer/action" requires **fact-level gold
  labels** FathomDB does not have. eu8 has doc-id qrels, not atomic-fact labels. Making this
  the *primary* gate is a multi-slice 0.8.1+ effort, not a 0.8.0 move.
- **K ladder (5/20/50/200) — fits, cheaply.** eu8/eu7 already parameterize K
  (`eu8_ir_validation.rs:70` `K=10`; eu7's exclude-before logic at `:444-512`). Adding @5/@20/@50
  is a small harness change. But note: FathomDB's production `search()` `LIMIT` is **10**
  (`measure_recall` raises it via a test seam only, `eu7_real_corpus_ac.rs:444-445`). Recall@50/200
  is **diagnostic** here, not a UX surface — which is exactly how the framework labels it.
- **MRR / nDCG — already implemented.** eu8 computes MRR and NDCG@10 with binary relevance
  (`eu8_ir_validation.rs:244-278`). Graded relevance (nDCG's full value) needs graded qrels,
  which the chain labels don't yet carry — flagged, not verified.
- **Tiered thresholds (world-class/excellent/…/not-valuable) — arbitrary engineering picks,
  and *dangerous* if imported naively.** The author admits they are "engineering thresholds,
  not academic," which is honest. But against FathomDB's **measured IR ceiling of 0.571**, the
  framework's "good" floor (Evidence Recall@20 ≥ 80–90%) and "not valuable" line (<70%) would
  classify FathomDB's *current relevance* as well below "not valuable." That verdict is
  *misleading*: 0.571 is an **embedder/task ceiling** (bge-small-en-v1.5, dim 384), not a
  defect the gate can shame into improving — it moves only with a better embedder or graph
  (`ADR-0.7.0-vector-binary-quant.md:172-179`). Adopting these thresholds without re-anchoring
  them to FathomDB's embedder class would produce a permanently-red, uninformative gate. The
  thresholds are reasonable *relative targets*; they are not absolute pass/fail lines for a
  dim-384 single-vector embedded store.
- **Must-not-miss category gates (commitments ≥98–99%, etc.) — aspirational, currently
  unreachable + unmeasurable.** FathomDB has no category-labeled gold set (no "commitment" /
  "calendar" / "exact-fact" query classes). And 98–99% evidence recall is implausible from a
  store whose *overall* IR recall@10 ceiling is 0.571. eu8's **per-relation-type / per-chain-shape
  buckets** (`eu8_ir_validation.rs:321-344`) are the right *mechanism* to grow into this, but
  the categories are RAG-generic, not yet FathomDB's.
- **qrels + TREC-style pooling — fits; partially built.** eu8 *is* a qrels harness. Pooling
  across FTS/vector/hybrid variants is not built (eu8 only runs the fused `search()`), but
  FathomDB has the primitives — it can run the FTS-only, vector-only, and fused branches
  separately (the branches exist inside `read_search_in_tx`; `fuse_rrf` consumes them as two
  lists, `lib.rs:3584-3607`). Pooling is a harness-orchestration task, not an engine change.
- **Document vs chunk vs fact/atomic-fact recall — chunk axis collapses; fact axis is the real
  gap.** FathomDB **does not chunk** (§1) — it stores whole bodies — so "document recall" and
  "chunk recall" are the *same thing* here. But the framework's warning ("document recall looks
  great while fact recall is poor — long notes/PDFs hide the answer") is **acutely relevant**:
  the expanded corpus includes `cnn_dailymail`, `qmsum` (meeting transcripts), and `enron`
  (`AC-075` body; `ORCHESTRATOR-CONTINUE-GA-RECALL.md:109-112`) — long, multi-fact documents
  where a single-vector embedding of the *whole body* can rank a doc highly while burying the
  specific fact. So fact-level recall is the genuinely missing axis; the chunk-level axis is
  moot **until FathomDB chunks** (which it doesn't, and isn't planned to in 0.8.0/0.8.1 per the
  docs I read — flagged as not verified beyond the engine).
- **Retrieval-mode matrix (FTS / vector / union / hybrid / +reranker / +graph) — fits for the
  first four; the last two are not real yet.** FTS, vector, union, and RRF-hybrid all exist
  (§1). **+reranker is a no-op identity stub** (`rerank_fused`, `lib.rs:3653-3660`), so "rerank
  top 200 → top 20" cannot be measured as real today. **+graph expansion is 0.8.1** (§1). So
  the matrix can be run for 4 of 6 modes now; the two highest-leverage modes for product
  quality are exactly the two FathomDB hasn't shipped.
- **Union-then-rerank architecture (FTS top100 + vector top100 + graph top50 → dedup → rerank
  200→20) — partially matches, with two gaps.** FathomDB *does* union-then-fuse (RRF over the
  two branches, dedup-on-body), which is the recall-oriented half. It does **not** have the
  precision-oriented rerank half (stub) nor the graph branch (0.8.1). And its candidate fanout
  is K=192 bit-KNN → rerank → top-10, narrower than the framework's top-100/top-200 picture —
  a tuning difference, not a structural one.

### Item 3 — what's missing / over-claimed

- **Principled vs arbitrary thresholds:** the *shape* (stricter K for stricter stakes; @50 as
  retriever-health; @200 diagnostic; union-for-recall/rerank-for-precision) is principled and
  matches FathomDB's own K-ladder instincts. The *numbers* (95/98/90/…) are arbitrary
  engineering picks (the author says so) and, worse, are **anchored to a class of system
  FathomDB is not** — they implicitly assume a chunked, rerankable, possibly larger-embedder
  RAG stack. They need re-anchoring to a dim-384 whole-body embedded store before they mean
  anything for FathomDB.
- **Does FathomDB need a fact-level labeled gold set?** Yes — for Evidence/Fact recall it is
  the *prerequisite*, and FathomDB does not have one. It has doc-id qrels (eu8 chains) only.
- **Does it have a reranker for "rerank 200→20"?** No — identity stub (`lib.rs:3658`). The
  framework's reranker-dependent claims are aspirational for FathomDB.
- **Over-claim:** the framework presents Evidence Recall@K as a near-universal primary gate
  without acknowledging the **embedder-ceiling reality** FathomDB has already measured (0.571).
  For an embedded single-vector store, the dominant lever is the embedder, not the eval — the
  framework under-weights this. FathomDB's own ADR is *ahead* of the framework on this point.

---

## 4. Bearing on the current GA recall halt (B-1)

**The halt number (0.8710 < 0.90 @ N=7,667) is a *fidelity* number, not a relevance number.**
It says the bit-KNN+rerank index, on the bigger/harder 8-dataset/7,667-doc corpus, now
reproduces only 87.1% of the exact-f32 top-10 (`AC-075`; `STATUS-0.8.0.md:414-419`;
`ORCHESTRATOR-CONTINUE-GA-RECALL.md:104-113`). The N=1,000 tier still passes at 0.9100. The
framework does **not** directly resolve this, because the framework is about the *relevance*
axis and this is the *fidelity* axis. But it sharpens the thinking in four ways:

1. **It confirms the gate's *role*, which de-escalates the stakes of the number.** A fidelity
   floor is a **necessary-but-not-sufficient system-health gate**. So 0.8710 means "the cheap
   index lost a bit more of the exact index's opinion on a harder corpus," **not** "the agent's
   memory got 13% worse at being useful." Whether users feel anything depends on the
   *relevance* axis (the 0.571 ceiling), which this number doesn't touch. That argues against
   treating 0.8710 as a product-quality cliff — while still honoring that the gate must not be
   weakened.
2. **It strongly endorses option (a) + (c) over a naive read of the halt.** The framework's own
   discipline — *a measurement gate must run against a pinned, versioned eval set* (qrels
   hygiene) — directly supports **pinning the floor to a defined, versioned corpus snapshot**
   (the brief's option 3, and the "pin to the 0.7.x basis" option 1). Fidelity recall is known
   to *decrease with N* (eu7's own anchor note, `eu7_real_corpus_ac.rs:834-837`), so a fidelity
   floor that isn't tied to a fixed corpus/N is under-specified by construction. The framework
   makes "pin the eval corpus" the *default-correct* engineering posture, not a dodge.
3. **It supports the orchestrator's recommended OLD-vs-NEW A/B before any engine work**
   (`ORCHESTRATOR-CONTINUE-GA-RECALL.md:150-156`). Separating "harder corpus" from "regression"
   is exactly the measurement-controls hygiene the framework's pooling/qrels methodology
   embodies. The N=1,000=0.9100 vs N=7,667=0.8710 split already points at corpus-difficulty/scale.
4. **It reframes the *real* product gate as a separate, larger effort (option b's honest
   form).** If FathomDB wants a gate that answers "is the agent's memory useful," that gate is
   an **evidence/IR-recall** gate (eu8-class), not a tighter fidelity floor. Chasing fidelity
   from 0.87 → 0.95 buys ~nothing for users once fidelity already exceeds the 0.571 relevance
   ceiling (`ADR-0.7.0-vector-binary-quant.md:172-179`). So "do engine/quantization/K work to
   restore 0.90 fidelity on the expanded corpus" (option 2) is the **least leverage** path for
   *product value*, even though it may be the right path for *gate integrity*.

**Net for the human making the B-1 call (I am informing, not deciding):** the framework
supports treating B-1 as primarily a **corpus-basis/versioning question** (options 1/3 + the
A/B), keeping the fidelity floor as an un-weakened system-health gate, and — *separately, post-
GA* — standing up an evidence/IR-recall product gate. It does **not** support inventing a new
relevance threshold and bolting it onto the GA decision under time pressure; that would import
arbitrary numbers against an embedder ceiling FathomDB has already characterized. It does
**not** support lowering the fidelity floor (consistent with the standing "do not weaken the
assert" constraint and the vacuous-green-trap memory).

---

## 5. Prioritized, FathomDB-specific recommendation

Distinguish two gates throughout: the **fidelity gate** (eu7/AC-075 — *keep*, system health)
and a future **evidence/task gate** (the framework's real subject — *the product question*).

**Adopt now (0.8.0 GA path — cheap, no engine change):**
1. **Keep eu7/AC-075 as the fidelity gate, pinned to a versioned corpus snapshot.** Resolve B-1
   by pinning the floor's corpus basis (option 1 or 3) after the OLD-vs-NEW A/B; document
   explicitly in the ADR that this floor is *fidelity, not relevance*. (The ADR already says
   this at `:151-158`; surface it in AC-075 too.)
2. **Promote eu8 from report-only to a tracked, versioned product-quality *signal* (not yet a
   gate).** It already computes Recall@10/precision/MRR/NDCG with per-relation/per-shape buckets
   (`eu8_ir_validation.rs:309-364`). Pin its corpus + qrels and report it every release
   alongside eu7. This gives the human a *relevance* trend line with near-zero new work.
3. **Add the K-ladder (@5/@20/@50) to eu8** (small harness change). Report @50 as
   retriever-health, @5/@10 as UX-proximal.

**Build for 0.8.1 (moderate effort, real product value):**
4. **A fact/atomic-fact gold set** ("commitment_due_friday"-style labels) over a *pinned subset*
   of the corpus — the prerequisite for any Evidence Recall. Start small (the enron/qmsum
   long-doc cases where fact-burial is worst). This is the single highest-leverage eval
   investment and the framework is correct that it's the missing axis.
5. **A retrieval-mode pooling harness** that runs FTS-only / vector-only / RRF-hybrid as
   separate qrels runs and pools+labels the union — the primitives exist (`fuse_rrf` consumes
   two branch lists, `lib.rs:3584-3607`); this is orchestration, not engine work.
6. **Land the rerank seam for real** (`rerank_fused` is a stub, `lib.rs:3658`) so "+reranker"
   and "rerank 200→20" become *measurable* modes rather than aspirational ones. Until then the
   framework's reranker-dependent gates are not applicable.

**Defer / treat as embedder-bound (do not gate on these yet):**
7. **The framework's absolute thresholds** (95/98/90 tiers; commitments ≥98–99%). Re-anchor
   them to FathomDB's embedder class *before* adopting; against the 0.571 ceiling they are
   currently unreachable and would produce an uninformative permanent-red gate. The dominant
   lever is a better embedder or the **graph (0.8.1)** — `ADR-0.7.0-vector-binary-quant.md:172-179`
   — not the eval thresholds.
8. **Task recall (agent-in-the-loop) and graph-expansion modes** — blocked on 0.8.1 graph work
   (`ADR-0.8.0-graph-traversal-scope.md`; `dev/roadmap/0.8.1.md`). Chunk-level recall is moot
   unless/until FathomDB chunks (it doesn't, and I found no plan to).

**Infrastructure FathomDB would need for the full framework:** a versioned/pinned eval corpus +
**qrels** (partly exists: eu8 chains), **fact-level labels** (missing), a **pooling harness**
(missing; primitives exist), a **real reranker** (stub), graded relevance judgments for full
nDCG (missing), and a clear, documented separation of the **fidelity gate** (eu7) from the
**evidence/task gate** (new). The eu7↔eu8 relationship is the seed of all of this and is
already correctly understood in `ADR-0.7.0-vector-binary-quant.md` § 2.

---

## Things I could not verify
- Whether any chunking is planned beyond the engine (I checked the engine source + 0.8.1 roadmap
  ADRs; no chunker found, but I did not exhaustively read every design doc).
- Whether eu8's chain qrels carry graded (vs binary) relevance — the harness uses binary
  relevance (`eu8_ir_validation.rs:255-278`); I did not open the raw chain JSONs.
- The live `main` recall numbers are from the *unmerged* `slice-40-20260607T145013Z` branch
  artifacts as cited in the GA hand-off + STATUS board; I did not re-run the ~108-min eu7
  verdict (out of scope: read+analyze only).
- The framework's provenance/intended target system is not stated in the brief; I judged it as
  generic-RAG guidance and assessed fit accordingly.
