# Gate-0 — golden-set re-scope (0.8.11 Slice 5) · $0 EVAL-ONLY

> **Deliverable type:** inventory + mapping (no engine build, no LLM spend, no priced run).
> **Spec:** `dev/design/planner-router-psd-0.8.x.md` §III.A (re-scope), §II.A (5 intent classes).
> **Pre-registration:** `dev/plans/0.8.11-implementation.md` §1 "Gate-0"; ledger row
> `dev/experiments-ledger.md` (F-11 scaffold).
> **Scope:** Gate-0 only (golden-set re-scope). Gate-2 (oracle ceiling) is run **separately**
> by the orchestrator after the engine builds — NOT in this deliverable.

This Gate-0 re-scopes the eval substrate from "build a fresh 50–100-query golden set" to
**"reuse existing assets + the registered `decide_083`/`decide_084` rules,"** per PSD §III.A. It
inventories the existing corpora, maps each to the **5 intent classes**
`{needle | multi_session | temporal | global | multi_hop}` (PSD §II.A), states which decision rule
governs which axis, identifies exactly which classes lack **FathomDB-node-level retrieval labels**,
and specifies the **scoped** labeling pass needed only for those gaps. The ~269-Q F4/M6 corpus
acquisition (EXP-D) is confirmed **excluded** (stays 0.8.17).

---

## 1. Reused-asset map

All payloads are **gitignored EVAL-ONLY** per `0.8.3-0.8.4-corpus-adequacy-and-locomo`; only
manifests/gold-schemas/notes are tracked. Counts below are inspected from the live files (not
quoted from prose).

| Corpus | Path | What it is (measured size) | License / redistribution | Intent class(es) served | FathomDB-node-level retrieval labels? |
| --- | --- | --- | --- | --- | --- |
| **IR gold (IR-C reuse tier)** | `data/corpus-data/eval/ir_gold/all.gold.json` (+ `enronqa/qaconv/qmsum` qa) | **4,597** queries, **4,472** carry `expected_top_k_doc_ids`; `query_class`: exact_fact 2,888 · exploratory 1,584 · negative 125. Sources: enronqa 710, qaconv 2,303, qmsum 1,584 | Public IR corpora (enronqa/qaconv/qmsum); tracked gold is qrels only | **needle** (exact_fact), some exploratory | **Doc-level qrels (✅ retrieval labels).** Closest existing thing to node-level; node-level derivable by doc→node map |
| **LOCOMO** | `data/corpus-data/raw/locomo10.json` + `data/corpus-data/eval/0.8.3-locomo-memory-gold.json` | 10 conversations (199 qa each); gold = **1,443** queries — `query_class`: factoid 841 · temporal 321 · multi_session 281. All 1,443 carry `required_evidence` | **CC-BY-NC-4.0** (Maharana et al. 2024) — EVAL-ONLY, not committed | **temporal**, **multi_session** (+ factoid→needle) | **Session-level only** (`required_evidence.doc_id` = e.g. `conv-26:session_1`). **NOT node-level → GAP** |
| **MuSiQue** | `data/corpus-data/raw/musique_dev.jsonl` | **4,834** total / **2,417 answerable** (hops: 2-hop 1,252 · 3-hop 760 · 4-hop 405); each Q has 20 paragraphs with `is_supporting` flags (mean **2.65** supporting per answerable Q) | Public (MuSiQue dev) | **multi_hop** (F5) | **Paragraph-level `is_supporting` (✅).** Node-level **derivable** from paragraph→node map (no new labeling) |
| **AP-News (BenchmarkQED)** | `data/corpus-data/raw/apnews_benchmarkqed/` | **1,397** articles; AutoQ sets: v1 4×50 (activity/data × local/global, text), v2 3×50 (data global/linked/local, **assertions**) = 350 questions total | **Microsoft Research License — NON-COMMERCIAL, NON-REDISTRIBUTABLE**, EVAL-ONLY, not committed | **global** (sensemaking, F4); `data_linked` = multi-hop-flavored | **No retrieval labels** — gold is **assertion/answer-content** (for AutoE LLM-judge win-rate). Sensemaking is judged by `decide_084` answer-quality, **not** retrieval recall → no node-level gold needed |
| **LME (memex-elps)** | `data/corpus-data/external/memex-elps/` | `memex_elps_golden.jsonl` 8 (extract.v1 protocol) · `personal.gold.jsonl` 60 (entities/edges) · `personal.baseline_outputs.jsonl` 60 | Project-internal eval set | **formation/extraction** gold; weak memory-class coverage | **No retrieval labels** — this is **extraction/formation** gold (entities/edges), not per-intent retrieval gold. Underpowers multi_session/temporal/ku (memory fact) |

**Measured discrepancy flagged:** PSD §III.A and the impl plan describe MuSiQue as "4,834
answerable." The live file is **4,834 total, 2,417 answerable** (4,834 = total; the unanswerable
half ships `answerable: false` + zero supporting paragraphs). Multi_hop usable-Q = **2,417**, not
4,834. Non-blocking for Gate-0 (still ample power), but the downstream EXP rows should cite 2,417.

---

## 2. Decision-rule adoption (which rule governs which axis)

Per PSD §III.A and `dev/plans/0.8.11-implementation.md` §1; rule implementations:
`src/python/eval/decision_rule_083.py`, `src/python/eval/decision_rule_084.py`.

| Axis (intent classes) | Competitor | Registered rule | Mechanics | Corpus |
| --- | --- | --- | --- | --- |
| Memory classes — **needle / multi_session / temporal** (F1/F2/F3) | **Mem0** | **`decide_083`** | Paired-delta on per-memory-class `FathomDB − Mem0`, **MDE ≤ 0.05**, paired bootstrap, per-class power guard + eu7-fidelity & latency BLOCK gates | LOCOMO (memory classes) + LME (weak) |
| Sensemaking — **global** (F4) | **Microsoft GraphRAG** | **`decide_084`** | LLM-judge **win-rate near-parity**, band **ε=0.05**, **question-clustered** bootstrap (≥5 runs, order-swapped) + bias-control BLOCK gates. **Corpus-capped at N=200** (AP-News max) | AP-News (BenchmarkQED) |
| Multi-hop — **multi_hop** (F5) | HippoRAG-2 (**unbuilt**) | **`[TBD: decide_08x]`** | competitor not built — **out of scope for Gate-0** | MuSiQue (2,417 answerable) |

---

## 3. Label-gap identification → scoped labeling pass plan

**Per-class node-level-retrieval-label status** (the question Gate-0 must answer):

| Intent class | Reused corpus | Has FathomDB-node-level retrieval labels? | Action |
| --- | --- | --- | --- |
| **needle** | IR gold (exact_fact 2,888) + LOCOMO factoid 841 | **Yes (doc-level qrels)** via IR gold; node-level derivable from doc→node map | **No new labeling.** Derive node-level from `expected_top_k_doc_ids` |
| **multi_hop** | MuSiQue (2,417 answerable) | **Yes (paragraph `is_supporting`)** — node-level derivable from paragraph→node map | **No new labeling.** Pure derivation |
| **global** | AP-News (350 AutoQ) | **N/A** — sensemaking judged by `decide_084` answer-quality win-rate, not retrieval recall | **No retrieval labels needed** by design |
| **multi_session** | LOCOMO (281) | **No — session-level evidence only** (`conv-N:session_M`) | **GAP → scoped labeling pass** |
| **temporal** | LOCOMO (321) | **No — session-level evidence only** | **GAP → scoped labeling pass** |

**The only gap requiring new work: refine LOCOMO `multi_session` + `temporal` evidence from
session-level to node-level.** LOCOMO already names the gold *session* (`conv-26:session_1`); the
oracle router needs the gold *node(s)/turn(s)* **within** that session.

**Scoped labeling pass spec:**

- **Corpus / size:** LOCOMO `temporal` (321) + `multi_session` (281) = **≤ 602 queries** — and
  bounded further: each query already cites 1–few sessions, so the pass only resolves the
  gold-supporting node(s) within an already-named session (a narrow refinement, not open search).
- **Method (cheapest-first):** Tier-0 — **deterministic $0** string-match: each query carries a
  short `answers` string; match it (and the `required_evidence` session) against the dialog turns
  in `locomo10.json` to pin the gold turn/node. Expected to resolve the large majority at **$0**.
  Tier-1 — only the residual (ambiguous/multi-turn) queries go to a **cheap-LLM** (`gemini-flash-lite`)
  node-pick, **cheap-validated** for field-population before any spend.
- **Cost:** **$0 expected**; hard ceiling **≤ $1** (Tier-1 residual only). Within the impl-plan
  Gate-0 ceiling.
- **Output (downstream):** a `locomo_node_gold` sidecar mapping `query_id → [node_id...]`, consumed
  by Gate-2's oracle routing. (Production of the sidecar is downstream execution; Gate-0 only
  **scopes** it.)

---

## 4. Scope guard — EXP-D excluded; corpus-cap reality

- **EXP-D (the ~269-Q entity-rich F4/M6 corpus acquisition) is EXCLUDED from Gate-0 and stays
  0.8.17.** Gate-0 is **reuse + scoped-gap node-labeling only** — explicitly **not** a fresh golden
  set. (Impl-plan §1 KILL/scope guard: if the labeling pass projects beyond "scoped to the gap"
  toward a fresh golden set, STOP + flag HITL — §8 scope-creep hotspot (b).)
- **Corpus-cap reality (PSD §III.A; `0.8.4-report` banner):** `decide_084` is **corpus-capped at
  N=200** — that is the AP-News maximum, and the comp MDE is **0.058 > ε=0.05** (NOT_REACHED).
  Because the bootstrap is **question-clustered**, **more runs cannot tighten the MDE — only more
  questions can.** F4 registration is therefore a **corpus/decision problem, not a re-run** — which
  is exactly why closing it (EXP-D's ~269 entity-rich Q, +69 past the N=200 max) is deferred to
  0.8.17 and out of Gate-0.

---

## Gate-0 verdict

1. **Re-scope holds.** The 5 intent classes are covered by reused assets — **needle** (IR-gold
   doc-qrels, 2,888) and **multi_hop** (MuSiQue, 2,417 answerable w/ `is_supporting`) carry
   derivable node-level retrieval labels; **global** needs none (sensemaking → `decide_084`
   answer-quality); **multi_session/temporal** (LOCOMO, 281/321) carry only **session-level**
   evidence — the single label gap. `decide_083` governs F1/F2/F3 (vs Mem0), `decide_084` governs
   F4 (vs GraphRAG, N=200 cap), MuSiQue/HippoRAG-2 is `[TBD]` and out of scope.
2. **One scoped labeling pass, ≤602 LOCOMO Q, $0 expected / ≤$1 hard cap** — refine LOCOMO
   temporal+multi_session evidence from session-level to node-level (deterministic match first,
   cheap-LLM residual only). No fresh golden set.
3. **EXP-D stays excluded (0.8.17).** Corpus-cap confirmed: `decide_084` N=200 is the AP-News max
   (comp MDE 0.058 > ε); more runs can't tighten it. Gate-0 = reuse + scoped-gap labeling, done.
