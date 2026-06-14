# LongMemEval (LME) — external leaderboard reference + reading notes

> Reference data for the 0.8.1 graph/IR track. **Scope: LME (the original), NOT
> LME-V2.** Captured 2026-06-14 from HITL-supplied notes. These are *external,
> reported* numbers — vendor/author-reported, not independently reproduced here,
> and **end-to-end** (memory system **+** reader LLM), not first-stage retrieval
> recall. Treat as orientation, not ground truth.

## Why we care

LME tests how well an agent-memory stack **recalls, synthesizes, and reasons over
extended multi-session interactions** with deep context. Because it scores the
**whole stack**, a reported number is the product of two factors:

1. the **memory/database** (retrieval + structure + temporal handling) — *what
   FathomDB controls*; and
2. the **reader LLM** that answers from the retrieved context — *a confound*.

The same memory system scores very differently by reader (see Mastra below). So a
headline LME % is only interpretable **with its reader named**.

## Top reported LME performances (original LME)

| Memory system / DB | Reported accuracy | Reader LLM | Notes |
|---|---:|---|---|
| Exabase M-1 | 96.4% | Gemini 3 Flash | Highest reported on original LME; notable for a *small* reader. |
| Observational Memory (Mastra) | 94.87% | GPT-5-mini | Converts raw messages → dense observations in a stable context window. **84.23% with GPT-4o** — same memory, weaker reader. |
| Mem0 | 94.8% | Gemini 3 Pro | User-scoped, identity-aware long-term memory; cross-session retrieval. |
| Honcho | 92.6% | Gemini 3 Pro | |
| HydraDB | 90.79% | Gemini 3 Pro | |
| Memoria (MatrixOrigin) | 88.78% | Multiple | LME-small tier; near-perfect single-session recall + strong temporal. |
| Supermemory | 85.2% | Gemini 3 Pro | **81.6% with GPT-4o.** |

## Takeaways (load-bearing for our gate design)

1. **The reader is a huge variable.** Mastra: 84.23% → 94.87% by swapping GPT-4o →
   GPT-5-mini on the *identical* memory data. Any FathomDB LME target **must name a
   reader**; "90%" is meaningless without it.
2. **Basic factual recall is "solved."** The leaderboard separates on the hard
   classes: **temporal reasoning, knowledge updates, multi-session synthesis.**
   Those are exactly the classes FathomDB's graph/temporal work targets — and the
   classes where our Slice-30 run trailed BM25.
3. **Architecture pattern of the >90% systems:** structured **event ledgers /
   time-aware indexing**, not pure vector similarity injected per turn. Naive RAG
   underperforms because it re-injects context each turn and loses the timeline.
   This corroborates the FathomDB bi-temporal-edge direction.
4. **Consequence for our 90% target:** 90% end-to-end is achievable on LME *with a
   strong reader* (the leaderboard's 90%+ all use Gemini 3 Pro / GPT-5-mini-class).
   With a weaker reader the same retrieval scores lower. So our gate must separate
   the **retrieval contribution** (recall@K — what we own) from the **end-to-end
   number** (retrieval + named reader). See the experiment plan, §gates.

## Caveats

- All numbers are self/vendor-reported on the **original LME**; methodologies
  (reader, judge, prompt, retrieval budget) differ between rows → **rows are not
  strictly comparable to each other**, and none is comparable to a first-stage
  retrieval-recall metric.
- We are deliberately **not** chasing LME-V2 here.
- FathomDB's footprint contract (embedded SQLite, ≤1M, BYO-LLM, no in-engine LLM)
  differs from several leaderboard systems (managed services, large readers) — a
  like-for-like comparison must hold the reader fixed and run FathomDB under the
  identical-answerer protocol (Slice-25 harness).

Sources: HITL-supplied 2026-06-14 (vendor/author-reported leaderboard compilation).
Pricing context for readers: `dev/design/0.8.1-graph-experiment-plan.md §cost`.
