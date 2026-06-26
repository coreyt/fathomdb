---
name: airlock-gemini-reasoning-and-grag-metering
description: gemini-3.5-flash is a reasoning model on the airlock — disable via reasoning_effort=minimal (gotchas per call path); GraphRAG query metering + costs
metadata: 
  node_type: memory
  type: reference
  originSessionId: c2742b5c-9247-4645-b7c9-53ed841368c4
---

0.8.4 gating-rerun infra findings (2026-06-24), reusable for any airlock eval:

**gemini-3.5-flash is a REASONING model.** At low max_tokens it returns `content:None` /
all-reasoning-tokens (a degenerate-answer trap). Disable reasoning with
`reasoning_effort` — but the valid value + delivery path differ by call path:
- Value: use **`minimal`** (gives `reasoning_tokens=0`/None), NOT `none` — `none` is not a
  valid OpenAI enum so litellm's `drop_params` silently strips it (reasoning stays ON);
  `low` keeps reasoning ON. `minimal` disables it AND survives litellm.
- **Direct airlock `/v1/chat/completions`** (our `_chat`): pass `reasoning_effort:"minimal"`
  as a top-level body field. ~2.3s/call with it off.
- **Microsoft GraphRAG** (graphrag 3.1.0 → graphrag_llm → litellm, `model_provider: openai`):
  a top-level `reasoning_effort` is DROPPED. Must go via settings.yaml model
  `call_args: { extra_body: { reasoning_effort: minimal } }` (extra_body forwards raw,
  bypassing drop_params). Verified `reasoning_tokens=0` in the index cache afterward.
- **claude-haiku judge must NOT receive `reasoning_effort`** → Anthropic/airlock returns
  HTTP 400 on every call (poisons the whole judge → all-ABSENT → compute_winrates raises).
  Gate the param to the answerer only.

**GraphRAG metering (airlock LiteLLM proxy has NO spend DB → no /spend endpoints):**
- INDEX calls ARE cached under `{root}/cache/**` with `usage` → sum prompt/completion.
- QUERY (global search) calls are **NOT cached** (cache delta = 0). Meter each query from
  its own `{root}/logs/query.log` stats block (`"prompt_tokens"`, `"completion_tokens"`);
  give each concurrent query a unique root (symlink shared `output/`) so logs don't clobber.

**Measured costs (gemini-3.5-flash reasoning-off, 0.30/2.50 per-1M estimate):** 200-doc
AP-News index = ~$3.16 (2,620 entities, 602 reports, levels 0:44/1:263/2:278/3:17);
GraphRAG **level-1 global query ≈ $0.10 each**; claude-haiku judge ≈ $0.0025/call.

**Airlock reliability traps (0.8.4 gating run burned ~2h on these):**
- **Per-provider daily budget cap.** `~/projects/airlock/config.yaml` →
  `router_settings.provider_budget_config.<provider>.budget_limit` (gemini default $25/day,
  anthropic/openai $50/day). Exhausted → every call to that provider 429s
  "crossed budget: ... >= 25.0". Mid-run storms of "429" were THIS, not TPM. Raising needs
  an airlock restart (resets the in-memory window too). The LiteLLM proxy has no spend DB.
- **gemini-3.5-flash is FLAKY/rate-limited under load on this airlock.** It was 2.3s at one
  point, later hung (HTTP 000, 17-45s) — its vertex backend 500s, aistudio path is RPM-limited
  (bursts hang, 20s-spaced calls succeed at 4-20s). Routing/health shifts across airlock
  restarts. gemini-3-flash (3.3s) and gemini-flash (2.3s) were stable when 3.5-flash wasn't.
- **Run at LOW concurrency (2-3), NOT 8.** Each GraphRAG query fans out to
  `concurrent_requests` sub-calls, so outer-8 × inner-6 = ~48 concurrent gemini calls
  SATURATED the upstream and calls HUNG (no clean 429 → no Retry-After → naive code pins a
  worker for the full HTTP timeout). Outer 2-3 / GraphRAG inner 2 ran clean at ~3.6 jobs/min,
  zero timeouts. Use a TIGHT per-call HTTP timeout (~60s, not 300s) so a hang fails fast →
  backoff+retry. Honor Retry-After. The claude judge provider stayed healthy throughout.

See [[0.8.4-tier1-fair-rerun-flips-graphrag-loss]], [[priced-runs-need-resilience-before-spend]].
