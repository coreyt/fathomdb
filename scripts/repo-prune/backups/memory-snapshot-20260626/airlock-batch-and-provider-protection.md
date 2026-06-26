---
name: airlock-batch-and-provider-protection
description: "Airlock OpenAI batch is configured+live and BYPASSES the provider-protection quarantine (~50% cheaper, ~24h); the gpt-5.4 \"flapping\" (429 + 200-empty) is the airlock quarantine draining after upstream insufficient_quota, NOT the model"
metadata: 
  node_type: memory
  type: reference
  originSessionId: a6f7b8e5-3e3c-45cb-b551-77108e9bb892
---

Airlock (litellm proxy at localhost:4000, project /home/coreyt/projects/airlock) — two facts
established 2026-06-22/23 during the 0.8.3 priced reruns:

**Provider-protection "flapping" (the 429 + 200-empty storms):**
- The `429 "Airlock temporarily blocked client key:... Retry after Ns"` is the AIRLOCK's quarantine
  (`airlock/fast/guardian.py::AirlockFastGuardian.async_pre_call_hook` → `_raise_provider_protection`),
  armed by upstream OpenAI `insufficient_quota` 429s, short-circuiting BEFORE upstream, draining via a
  DECREASING cooldown (300→208→88→0s). Root cause = OpenAI quota; the 429s = airlock wrapper.
- The `200-with-empty` answers (37% during a run fired right after a quota top-up) are **Cause A
  (transient infra/quarantine-drain), NOT Cause B (reasoning truncation).** Bucketing framework (per the
  airlock maintainer): A = `error!=null`/`response:null` (retry); B = `error:null` + `finish_reason:length`
  + `completion_tokens>0` + text 0 (reasoning budget starved → raise max_tokens, a CONFIG fix not retry);
  blocked = `content_filter`/`safety`. REPRODUCED the exact α=1.0/pool_n=10 path on 32 REAL LME questions
  (all 4 classes, real reranked context) → **0/32 empty, reasoning_tokens=0 + finish_reason=stop on every
  call** → B ruled out (and interactive payload sends NO max_tokens, so the model has full budget). The 37%
  was time-clustered (post-topup drain), not condition-clustered (α=1.0 reproduces clean). gpt-5.4 interactive
  does not emit reasoning tokens here. Original 152 not directly bucketable (interactive≠batch; deleted
  checkpoint stored only None, not finish_reason).
- gpt-5.4 THINKING LEVEL: NO reasoning_effort/thinking_level configured (config.yaml:40-43 bare
  `model: openai/gpt-5.4`; the `thinking_level: MEDIUM` at config.yaml:113 is a DIFFERENT model; reasoning-
  stripper scoped to kimi-dev only). Runs at OpenAI DEFAULT = **0 reasoning_tokens** on every call (even a
  bat-and-ball trap, answered right at rtok=0). Reasoning-CAPABLE (`reasoning_effort=high`→rtok=43;
  `=minimal`→HTTP400 unsupported) but the harness sends no param → structurally NO reasoning budget to starve
  ⇒ Cause B impossible. KEEP default for the eval — reused α=0.3/baseline/mem0 cells ran at it; changing it
  breaks comparability.
- MECHANISM (airlock maintainer, code-verified across guardian.py branches): on the GPT interactive path
  airlock CANNOT manufacture a 200-with-empty — every protection branch RAISES (RateLimitError on
  client/provider quarantine, ValueError on backoff/threat/no-healthy-fallback); monitor/post-call hooks only
  read/annotate. So the harness Nones were 429-quarantine short-circuits and/or UPSTREAM OpenAI degradation,
  NOT airlock-emitted empties. ⇒ The detector to TRUST is **429-rate**, not empty-rate.
- DETECT before firing a priced run (`/tmp/fdb_health_gate.py`): 15-20 REAL-question probes at RUN-SIZED
  (~32k-char) context, SAME client_id as the run → **abort on ANY 429** (binary: a 429 means quarantine is
  active AND observing it RE-ARMS `quarantine_until = max(…, now+300)` — so a probe burst that sees a 429 just
  reset the cooldown; stop probing immediately, wait ≥300s before re-gating), empty-rate >5% as a SECONDARY
  guard for upstream 200-degradation. Small-context-only bursts can read clean while the real payload trips it.

**Batch IS available + sidesteps the flapping:**
- OpenAI Batch API is CONFIGURED (config.yaml `files_settings: custom_llm_provider: openai`) and LIVE
  (`/v1/batches` responds). The guardian EXPLICITLY skips batch (`if not mcp and not batch:`), so batch
  BYPASSES the quarantine entirely → immune to the flapping.
- ~50% cheaper, ~24h turnaround. Use upstream id `gpt-5.4` + `?custom_llm_provider=openai` on /v1/files +
  /v1/batches; uploads straight to OpenAI (bypasses proxy alias + guardrails — fine for public eval data).
  Same model/temp0/seed0 → valid pairing vs the reused SYNC gpt-5.4 cells. Docs: `docs/guide/batch.md`.
- Other batch backends via Airlock Batch Gateway: aistudio(Gemini), mistral, local vLLM (qwen3.6-27b, executor
  mode). Output JSONL is OpenAI-shaped; idempotent on (input_file_id,model,endpoint,params); resumes missing rows.

**How to apply:** [[priced-runs-need-resilience-before-spend]] — run a 15+ probe health burst before any sync
priced run; if the endpoint flaps or to save ~50%, use the OpenAI batch path (build a 606-row JSONL runner).
The rerank_accuracy_run empty-answer fix (None→absent, resumable) makes a residual sync flap non-corrupting.
Relates to [[0.8.3-openai-quota-blocker-reblend-ready]].
