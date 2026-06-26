# Context Research — agent-scaffolding

## Scope

The "envelope" the AI coding agent runs inside, not the project artifacts it edits: memory files (CLAUDE.md, AGENTS.md, .cursor/rules/), system prompts and how vendors compose them, multi-agent orchestration (sub-agents, manager-worker), IDE/editor state surfaced to the model, few-shot examples in prompts, planning artifacts (plan mode, todo lists, scratchpads), prompt caching, conversation compaction, and skill/slash-command definitions. Out of scope (other lanes): tech docs, source retrieval, tests, CI feedback.

## Sources

Primary: Anthropic engineering blog (Sep 2025 context engineering, Jun 2025 multi-agent research, multi-agent code customer post), Anthropic Claude Code docs (best-practices, sub-agents, hooks, skills, compaction), Anthropic Claude API docs (prompt caching, compaction tool), OpenAI Codex docs (AGENTS.md, Skills), agents.md spec site, Cognition blog (Devin 2025 performance review), Cursor agent best-practices blog (Jan 2026), and corroborating practitioner posts (Simon Willison, Armin Ronacher on plan mode). All URLs and dates inline below.

## Findings

### F1: AGENTS.md has emerged as the cross-vendor standard for project scaffolding files; CLAUDE.md remains the Claude-native equivalent

- Evidence: agents.md ("README for agents"; 60,000+ OSS adopters; stewarded by the Agentic AI Foundation under the Linux Foundation; supported by Codex, Cursor, Copilot, Devin, Jules, Aider, Zed, Warp, goose, Factory, VS Code) — <https://agents.md>. OpenAI Codex "Custom instructions with AGENTS.md" — <https://developers.openai.com/codex/guides/agents-md> (2025). Anthropic Claude Code best-practices: CLAUDE.md is read every conversation; "include bash commands, code style, workflow rules; keep short and human-readable" — <https://code.claude.com/docs/en/best-practices>.
- Observations: No required schema; agents read the nearest file walking up the tree. Claude Code recognizes both AGENTS.md and CLAUDE.md. Practitioner consensus (Cursor, UX Planet Mar 2026): bullet-pointed conventions, build/test commands, dangerous-thing bans — not exhaustive style guides.
- Recommendations: One source of truth (AGENTS.md), symlink or @-include CLAUDE.md. Cap at a few hundred lines. Encode bright-line rules, build/test invocations, pointers to canonical examples — not narrative.
- Impact on agent LLM: HIGH
  - Rationale: 40-60% fewer revision cycles reported with well-tuned rules (Elementor Engineers, 2026-03); cross-vendor adoption means one file pays off across the agent zoo. Files are soft context, not policy.

### F2: Multi-agent orchestration buys breadth at ~15x token cost; bad fit for most coding tasks

- Evidence: Anthropic, "How we built our multi-agent research system" (2025-06-13) — <https://www.anthropic.com/engineering/multi-agent-research-system>: orchestrator+subagents beat single Opus 4 by 90.2% on research evals; "multi-agent systems use about 15x more tokens than chat"; "token usage by itself explains 80% of the variance." Verbatim caveat: "domains that require all agents to share the same context or involve many dependencies... most coding tasks... are not a good fit." Cognition Devin 2025 review (<https://cognition.ai/blog/devin-annual-performance-review-2025>): "writes stay single-threaded"; subagents should "contribute intelligence rather than actions"; managers "default to being overly prescriptive when lacking deep codebase context."
- Observations: Where subagents work for code is exactly Claude Code's Explore/Plan/general-purpose pattern: read-heavy, output-condensing side-quests — <https://code.claude.com/docs/en/sub-agents>.
- Recommendations: Default to single-agent for edits. Spawn subagents only for (a) parallel read/research, (b) isolating high-volume tool output, (c) independent verifications. Always specify objective, output schema, tool list, stop condition.
- Impact on agent LLM: HIGH
  - Rationale: 15x cost is real money; misuse on coding tasks burns tokens and produces worse outputs.

### F3: Prompt caching is the single biggest cost/latency lever for any agent loop

- Evidence: Anthropic prompt caching docs — <https://platform.claude.com/docs/en/build-with-claude/prompt-caching>: writes 1.25x (5-min TTL) or 2x (1-hour TTL) base input; reads 0.1x. Anthropic 1-hour TTL announcement (2025-05-22): "reduces costs by up to 90% and latency by up to 85% for long prompts" — <https://x.com/AnthropicAI/status/1925633128174899453>. Practitioner reports: 5-10x input-cost reduction on multi-turn loops with 10k-token system prompts (Introl, ngrok, 2025).
- Observations: Cache the system prompt + tool defs + AGENTS.md content at the head; vary only the tail. Break-even is ~2 reads at 5-min TTL; agent loops do dozens. Any mid-session edit to the cached prefix evicts.
- Recommendations: Cache-control the stable prefix (system, skills index, tool schemas, scaffolding). Don't mutate it mid-conversation. Use 1-hour TTL for long-running or scheduled loops. Track cache hit rate as a first-class metric.
- Impact on agent LLM: HIGH
  - Rationale: Measured 5-10x input cost reduction and 85ms+ latency wins; every loop turn is affected.

### F4: Context compaction is now first-party infrastructure — and lossy compaction is a dominant failure mode

- Evidence: Anthropic, "Effective context engineering for AI agents" (2025-09-29) — <https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents>: "context must be treated as a finite resource with diminishing marginal returns"; three tactics — compaction, structured note-taking, multi-agent delegation. Compaction API (`compact-2026-01-12`) is production across Anthropic, Bedrock, Vertex, Foundry — <https://platform.claude.com/docs/en/build-with-claude/compaction>. Industry analysis (Zylos, 2026-02): ~65% of enterprise AI failures in 2025 were context drift / memory loss, not raw context exhaustion.
- Observations: Claude Code `/compact` preserves architectural decisions, unresolved bugs, implementation details; drops redundant tool output. Factory.ai found structured summarization beats both OpenAI and Anthropic defaults on long sessions — <https://factory.ai/news/evaluating-compression>. Cursor's guidance: "start fresh" rather than over-compact when the task changes.
- Recommendations: Treat conversation as ephemeral; write load-bearing state (decisions, todo, plans) to disk. Compact proactively at task boundaries, not at the cliff. Don't rely on the model to remember what it didn't write down.
- Impact on agent LLM: HIGH
  - Rationale: On the critical path for any session >~30 turns; dominant failure mode when done badly.

### F5: Plan mode + persistent todo lists reduce thrash on non-trivial tasks

- Evidence: Ronacher, "What Actually Is Claude Code's Plan Mode?" (2025-12-17) — <https://lucumr.pocoo.org/2025/12/17/what-is-plan-mode/>: plan mode is a read-only sandbox producing a markdown plan; ExitPlanMode gates the write transition. VS Code planning docs (2025-10) and Cursor blog (2026-01) describe equivalent flows. Anthropic multi-agent post: extended thinking acts as "a controllable scratchpad." Academic: TodoEvolve (arXiv 2602.07839) on meta-planning with dynamic planner state.
- Observations: Two distinct scaffolding concerns. (1) Plan mode = tool-permission gate. (2) Todo list = structured scratchpad spanning compactions. Plan mode prevents premature writes; todo list keeps multi-step work coherent across compaction. Claude Code issues #12499, #15755 (late 2025) show plan→execute transitions are still rough.
- Recommendations: Use plan mode for >3-step tasks or expensive-to-undo work. Externalize plans to files so they survive compaction. Skip for trivial edits — overhead is real.
- Impact on agent LLM: MED
  - Rationale: Effects reported but not vendor-quantified; helps user trust and reduces wasted-edit cost on ambiguous tasks.

### F6: Few-shot examples are still positive-EV with frontier models, but should be canonical-not-exhaustive

- Evidence: Anthropic context engineering post (2025-09-29): "rather than stuffing a prompt full of edge cases... curate a set of diverse, canonical examples that effectively portray the expected behavior of the agent." Stanford 2026 AI Index references continued few-shot effectiveness ("including two few-shot examples improved performance"). OpenAI model spec (2025-12-18) — <https://model-spec.openai.com/2025-12-18.html> — preserves few-shot demonstration patterns.
- Observations: The classic "list every edge case" approach now hurts more than it helps because it eats the cache budget and biases toward verbose output. The 2025 consensus is small N (1-3), highly representative.
- Recommendations: When a tool or workflow has a canonical "shape," ship 1-3 worked examples in the skill/system prompt — not edge-case enumeration. Put examples in the cached prefix.
- Impact on agent LLM: MED
  - Rationale: Real but second-order effect; matters most for unusual tools/workflows the base model hasn't seen.

### F7: Skills (auto-invoked, model-discoverable) are diverging from slash commands (user-triggered)

- Evidence: Claude Code Skills — <https://code.claude.com/docs/en/skills>: skills "auto-invoke based on what you're doing"; SKILL.md frontmatter declares triggers. OpenAI Codex Skills follow the same Agent Skills open standard — <https://developers.openai.com/codex/skills>. Slash commands are "fixed-logic operations... hardcoded into the CLI" (MindStudio, 2026). Anthropic ships built-in subagent skills (Explore, Plan, general-purpose) and bundled skills (simplify, debug, loop).
- Observations: Split solves a real problem: skills give the model a discoverable playbook (model decides when); slash commands give the user explicit triggers. Skill descriptions are model-facing routing metadata — must be precise.
- Recommendations: Skills for self-triggered patterns ("run ruff after editing python"); slash commands for operator-driven actions ("review this PR"). Tight trigger descriptions or skills will misfire.
- Impact on agent LLM: MED
  - Rationale: Cuts boilerplate, JIT-loads niche guidance (cache-friendly). Poorly-scoped skills cause false-positive invocations.

### F8: IDE state context (selection, cursor, recent diff, open tabs) is standard input, but noisy

- Evidence: Cursor agent best-practices (2026-01-09) — <https://cursor.com/blog/agent-best-practices>: "Including irrelevant files can confuse the agent... let agents discover context through built-in search tools" rather than blanket-attaching tabs. VS Code Copilot planning docs (2025-10): plan agent uses workspace state but invites the user to scope it. Cursor's parallel-agent model uses git worktrees specifically to isolate context.
- Observations: More IDE context is not better. Vendor guidance: scoped @-mentions and search-on-demand beat dump-everything. Diff (current branch state, uncommitted) is the consistently-useful signal; selection/cursor matter only on deictic prompts ("this function", "here").
- Recommendations: Pass IDE state on demand. Always include branch + uncommitted-diff summary (cheap, high-signal). Pass selection/cursor only when the prompt is deictic. Avoid blanket open-tab attachment.
- Impact on agent LLM: MED
  - Rationale: Wrong defaults degrade quality measurably; remediation cost is low.

## Synthesis

Five themes recur across every primary source surveyed:

1. **Context is a budget, not a bucket.** Every paragraph in CLAUDE.md/AGENTS.md, tool description, and IDE auto-attachment competes for cache and attention.
2. **Persist load-bearing state to files; let conversation be ephemeral.** The model will be compacted; anything it must remember must be on disk. Scaffolding files (AGENTS.md, plans/, scratchpads/) are runtime, not config.
3. **Prompt caching is non-optional for agent loops.** Stable prefix → variable suffix is table stakes; 1-hour TTL flips the math for scheduled long-running work.
4. **Multi-agent for breadth, single-agent for code edits.** 15x tokens for 90% research lift; that ratio does not transfer to write-heavy coding.
5. **Soft scaffolding ≠ enforcement.** Memory files and skills are biases; hard rules belong in hooks. Conflating the two produces "I told it not to" failures.

Starter kit for a new agent project: one AGENTS.md (≤300 lines, bullet-form, build/test/style + bright lines), 1-3 skills with tight triggers, hooks for hard bans, prompt caching on the system+scaffolding prefix, and a plans/ directory the agent can write to. Default to single-agent; reach for subagents only to isolate verbose reads or run independent verifications.

## See also

- Anthropic, "Effective context engineering for AI agents" (2025-09-29) — <https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents>
- Anthropic, "How we built our multi-agent research system" (2025-06-13) — <https://www.anthropic.com/engineering/multi-agent-research-system>
- Anthropic, Claude Code best practices — <https://code.claude.com/docs/en/best-practices>
- Anthropic, Claude Code sub-agents — <https://code.claude.com/docs/en/sub-agents>
- Anthropic, Claude Code skills — <https://code.claude.com/docs/en/skills>
- Anthropic, Claude Code hooks — <https://code.claude.com/docs/en/hooks>
- Anthropic, prompt caching — <https://platform.claude.com/docs/en/build-with-claude/prompt-caching>
- Anthropic, compaction API — <https://platform.claude.com/docs/en/build-with-claude/compaction>
- Anthropic 1-hour cache TTL announcement (2025-05-22) — <https://x.com/AnthropicAI/status/1925633128174899453>
- OpenAI Codex AGENTS.md guide — <https://developers.openai.com/codex/guides/agents-md>
- AGENTS.md spec — <https://agents.md>
- OpenAI Codex Skills — <https://developers.openai.com/codex/skills>
- OpenAI Model Spec (2025-12-18) — <https://model-spec.openai.com/2025-12-18.html>
- Cognition, Devin 2025 performance review — <https://cognition.ai/blog/devin-annual-performance-review-2025>
- Cursor agent best practices (2026-01-09) — <https://cursor.com/blog/agent-best-practices>
- Simon Willison on Anthropic multi-agent (2025-06-14) — <https://simonwillison.net/2025/Jun/14/multi-agent-research-system/>
- Armin Ronacher on Plan Mode (2025-12-17) — <https://lucumr.pocoo.org/2025/12/17/what-is-plan-mode/>
- Factory.ai context-compression evaluation — <https://factory.ai/news/evaluating-compression>
- VS Code planning agent docs — <https://code.visualstudio.com/docs/copilot/agents/planning>
- awesome-claude-code (community skills/hooks/commands) — <https://github.com/hesreallyhim/awesome-claude-code>
- TodoEvolve (arXiv) — <https://arxiv.org/html/2602.07839>
