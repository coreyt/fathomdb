---
name: airlock-batch-graphrag-cant-batch
description: Airlock batch reality — GraphRAG (sync dependent pipeline) CANNOT use the batch API; run it on local Qwen $0. Batch fits only our independent call sets; vLLM-batch executor = $0/fast batch ergonomics.
metadata: 
  node_type: memory
  type: reference
  originSessionId: 1a66de90-c67e-434a-a0e5-9ae699d3289c
---

**Airlock batch** (`~/projects/airlock/docs/guide/batch.md`): OpenAI `/v1/files`+`/v1/batches`
(~50% off, **~24h** turnaround, upstream model id + `custom_llm_provider=openai`) AND an **Airlock
Batch Gateway "executor mode"** for **local vLLM** (alias `qwen36-27b-vllm-batch`) that runs a
submitted JSONL against the live vLLM `/chat/completions` with bounded concurrency — **batch
ergonomics, $0, NO 24h wait.** Anthropic batch is wired in LiteLLM but **not configured** here →
claude judge runs sync. AI-Studio gemini + mistral batch via the gateway (need `airlock_batch`
alias). Reusable harness: `eval/p0a_batch_e2e.py` (`build_batch_jsonl`/`run_batch`),
`autoe_judge.build_autoe_batch_jsonl`.

**Key constraint:** **Microsoft GraphRAG's indexing AND global-query are synchronous, dependent
pipelines** calling `/chat/completions` directly — they neither submit to `/batches` nor tolerate
submit-once/poll-24h (stage N+1 needs stage N). **So GraphRAG CANNOT use the batch API.** Don't try
to batch it — point it at **local vLLM `qwen3.6-27b` (sync, $0)**, which beats batch on both cost
(free) and latency. Batch applies only to **our own independent call sets** (D2 cluster summaries,
C/D2 answer-gen) — use `qwen36-27b-vllm-batch` ($0/fast) or `gpt-5.4-nano` OpenAI batch for a
stronger registered answerer. Judge = sync claude-haiku (cross-family). See
`dev/design/0.8.4-closing-graphrag-gap.md` §6b. Pairs with
[[0.8.1-budget-discipline-cheap-validate-and-ledger]], [[priced-runs-need-resilience-before-spend]].
