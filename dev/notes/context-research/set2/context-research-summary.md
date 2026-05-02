# Context Research — Cross-Cutting Summary

Synthesizes five dimension reports under `dev/tmp2/`:

- [tech-docs](context-research-tech-docs.md) — requirements, architecture, design, ADRs, interface specs
- [source-code](context-research-source-code.md) — repo retrieval, agentic vs RAG search, AST chunking, LSP, git
- [tests](context-research-tests.md) — TDD with agents, oracles, property-based testing, test-gaming
- [dev-env](context-research-dev-env.md) — build/lint/typecheck/CI/sandbox/hooks/budgets
- [agent-scaffolding](context-research-agent-scaffolding.md) — AGENTS.md, prompts, multi-agent, caching, compaction, plan mode

Recency window of cited sources: 2025-05-01 through 2026-04 (12 months); foundational older work cited only when still load-bearing.

---

## Cross-cutting themes

### T1: Context is a budget, not a bucket — every dimension says this

Appears in tech-docs F1/F4, source-code F5, dev-env F7, agent-scaffolding F1/F4. The frontier-lab consensus is unanimous: more context is not better, and pre-loading degrades performance. Mechanisms differ but the rule is the same — progressive disclosure (Skills, `@import`, ADR indexes), agentic retrieval over pre-built indexes, structural truncation of long outputs (~200 lines, head+tail+spill-to-file), and few-shot examples that are *canonical* not exhaustive.

### T2: Persist load-bearing state to disk; treat conversation as ephemeral

Compaction is now first-party (Anthropic `compact-2026-01-12`), and ~65% of enterprise AI failures in 2025 were context drift, not exhaustion (scaffolding F4). Tech-docs F5 (SPEC.md + progress logs), tests F2 (commit failing tests *before* implementation), and scaffolding F5 (plan mode + persistent todos) all point at the same answer: the agent will be compacted, so anything it must remember has to be on disk and re-readable.

### T3: Stale/wrong context is worse than missing context

Tech-docs F6 (confident architectural hallucination from outdated ADRs), tests F3 (wrong oracles → 76% test-gaming on impossible-SWEbench by GPT-5), source-code F5 (1M-context multi-fact recall ≈ 90% Claude / 60% Gemini — 1-in-10 confidently wrong). Mitigation is cheap and consistent across dims: co-locate docs with code, mark superseded material explicitly, use property-based oracles instead of pinned values, prefer iterative narrow retrieval over whole-repo dumps.

### T4: Structure beats prose, in every channel

- Tech-docs F2: prescriptive bullets ("Use X" / "Never Y") outperform narrative.
- Source-code F3: AST chunks (+2.67 SWE-bench Pass@1) over line-based.
- Dev-env F2/F3: structured `--error-format=json` diagnostics + typed verbs (build/lint/test) over raw bash (4× SWE-bench lift from ACI design).
- Tests F5: properties (Hypothesis) compress an input space into invariants — 23–37% relative pass@1 over example-based TDD.
- Scaffolding F1: bullet-form AGENTS.md, ≤300 lines, no narrative.

### T5: Verifiability is the load-bearing primitive

Frontier models are now *trained* on execution feedback (RLEF/RLVR — dev-env F1), so structured pass/fail signals are no longer just a prompt pattern but a model-side capability. Tests F1 (Anthropic: "single highest-leverage thing you can do"), dev-env F2 (RustAssistant 74% vs `cargo fix` <10% on the same population), and tech-docs F3 (OpenAPI/JSON-Schema grounding kills parameter hallucination) all point the same way: give the agent a binary oracle and rich diagnostics, and don't paraphrase them.

### T6: Cross-vendor convergence on AGENTS.md as the scaffolding standard

Tech-docs F7 + scaffolding F1: AGENTS.md is now stewarded by the Linux Foundation's Agentic AI Foundation; 60k+ OSS adopters; supported by Codex/Cursor/Copilot/Devin/Aider/Zed/Warp/goose/Factory/VS Code; Claude Code reads it natively. Author once, symlink CLAUDE.md → AGENTS.md.

---

## Conflicts and how to read them

### C1: RAG embeddings vs agentic grep

- Anthropic (Cherny): abandoned RAG; agentic grep "outperformed by a lot" + simpler ops.
- Cursor: +12.5% accuracy from semantic search — but **on top of** grep, not replacing it.
- Reconciliation (source-code F1/F2): grep wins for *local* agents on typed code with unique symbols; embeddings are an *additive* cloud-scale lever where you have remote infra and a controlled corpus. For a local agent on a single repo: grep + glob + read; do not build a local vector index.

### C2: Long-context windows vs retrieval

- Marketing implies 1M context lets you skip retrieval.
- Reality (source-code F5): ~90% Claude / 60% Gemini multi-fact recall, lost-in-the-middle still real, 1250× cost penalty, 30–60× latency. Iterative narrow retrieval + prompt caching wins on accuracy and economics.

### C3: Multi-agent

- Anthropic research mode: 90.2% lift over single Opus, 15× tokens.
- Same Anthropic post: "most coding tasks ... are not a good fit"; Cognition Devin: "writes stay single-threaded."
- Reconciliation (scaffolding F2): subagents for parallel reads / output condensation / independent verification; single-agent for edits.

---

## Top actions ranked by impact × effort

| # | Action | Dim(s) | Impact | Effort |
|---|--------|--------|--------|--------|
| 1 | Author one short AGENTS.md (≤300 lines, bullets, decisions+conventions+commands; **link** ADRs / API specs, don't inline). Symlink CLAUDE.md → AGENTS.md. | tech-docs F1/F4/F7, scaffolding F1 | HIGH | LOW |
| 2 | Enable prompt caching on the stable prefix (system + tool defs + scaffolding). 1-hour TTL for long sessions. Track cache-hit rate. | scaffolding F3 | HIGH (5–10× input cost, 85% latency) | LOW |
| 3 | TDD-with-agents discipline: commit failing tests *before* implementation, mark test files read-only during fix-to-spec, prefer property-based oracles for invariants. Cap retry-budget at ~2 same-issue corrections then `/clear`. | tests F1/F2/F3/F5/F8 | HIGH (test-gaming 76%→1% with right framing) | MED |
| 4 | Treat dev-env signals as first-class: structured diagnostics (`--error-format=json`), typed tool verbs (`build`/`lint`/`test`), ~200-line output cap with spill-to-file, PostEdit hooks for fmt/lint, sandbox to skip approval prompts (84% reduction). | dev-env F1/F2/F3/F5/F7/F8 | HIGH (4× SWE-bench lift from ACI alone) | MED |
| 5 | Source-code retrieval = grep/glob/read + tree-sitter repo map regenerated per task + LSP tools (`rust-analyzer`/`pyright`) + `git diff`/`git log` exposure. **No local vector index.** | source-code F1/F2/F4/F6/F7 | HIGH | MED |
| 6 | Add a SPEC.md + progress log discipline for multi-session work; co-locate ADRs with code; mark superseded ADRs explicitly. | tech-docs F5/F6 | MED–HIGH | LOW |
| 7 | Default to single-agent for code edits; reserve subagents for parallel reads / output isolation / independent verification with explicit objective + output schema + tool list + stop condition. | scaffolding F2 | HIGH cost-avoidance | LOW |
| 8 | Surface remaining-iteration / token state to the agent each turn, plus a hard external cap. | dev-env F6 | MED | LOW |

---

## Open questions / weak-evidence areas

- **LSP-on vs LSP-off agent success deltas**: claims strong, public ablation numbers thin (source-code F6, dev-env F4). Worth running internally if we ship LSP-as-tool.
- **Sandbox capability cost at task-success granularity**: 84% prompt reduction is measured (dev-env F5); whether *task success* changes is asserted-no-cost but unmeasured.
- **100-call tool-loop plateau**: from BrowseComp (web search), not SWE-bench (dev-env F6). The shape (premature termination without budget signal) clearly transfers; the specific number may not.
- **Repo-map isolated contribution**: Aider, Devin DeepWiki, Cognition all ship repo maps but no public ablation isolates the map's win from agentic search + planning (source-code F4).
- **Plan mode / progress files quantitative effect**: Anthropic guidance is concrete (tech-docs F5, scaffolding F5); no benchmark numbers published yet.

---

## Direct hits on existing fathomdb memory

- `feedback_tdd.md` (TDD required) — well-supported by tests F1/F2/F4. Add: read-only test files during fix-to-spec; property-based tests preferred for codecs / projection invariants / round-trip behaviour.
- The 0.6.0-rewrite branch is a natural moment to bake in the dev-env contract (T4/T5 above): structured diagnostics, typed verbs, ~200-line truncation, PostEdit fmt/clippy hooks. Rust's diagnostics are best-in-class — pass them through unparaphrased.
- Memory file shape: current MEMORY.md follows the bullet-form / link-out pattern recommended in tech-docs F1 + scaffolding F1. No change needed; resist drift toward narrative.

---

## Files

- `dev/tmp2/context-research-tech-docs.md` (7 findings, 1500w)
- `dev/tmp2/context-research-source-code.md` (7 findings, 1850w)
- `dev/tmp2/context-research-tests.md` (8 findings, 2100w)
- `dev/tmp2/context-research-dev-env.md` (8 findings, 2300w)
- `dev/tmp2/context-research-agent-scaffolding.md` (8 findings, 1830w)
- `dev/tmp2/context-research-summary.md` (this file)
