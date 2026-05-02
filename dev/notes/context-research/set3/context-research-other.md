# Scope

This report focuses on ancillary context that materially changes AI coding-agent performance when using Claude Code and Codex: persistent memory, prompt/system-instruction scaffolding, multi-agent coordination, live IDE/editor state, examples and few-shot artifacts, issue-tracker and pull-request context, human feedback loops, and when agentic tool use outperforms static retrieval/RAG. It prioritizes official Anthropic/OpenAI documentation and engineering posts, with careful inferences called out where the source is general to agents rather than specific to coding agents.

## Sources (URLs cited)

- [S1] <https://code.claude.com/docs/en/memory>
- [S2] <https://code.claude.com/docs/en/best-practices>
- [S3] <https://code.claude.com/docs/en/vs-code>
- [S4] <https://code.claude.com/docs/en/jetbrains>
- [S5] <https://code.claude.com/docs/en/github-actions>
- [S6] <https://www.anthropic.com/engineering/multi-agent-research-system>
- [S7] <https://openai.com/index/introducing-codex/>
- [S8] <https://developers.openai.com/codex/memories>
- [S9] <https://developers.openai.com/codex/guides/agents-md>
- [S10] <https://developers.openai.com/codex/ide>
- [S11] <https://developers.openai.com/codex/integrations/github>
- [S12] <https://developers.openai.com/codex/learn/best-practices>
- [S13] <https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide>
- [S14] <https://openai.com/business/guides-and-resources/a-practical-guide-to-building-ai-agents/>
- [S15] <https://developers.openai.com/blog/run-long-horizon-tasks-with-codex>

## Findings F1..F9

## F1. Shared rules and learned memory should be separate layers, not one context bucket

Evidence: Claude Code explicitly separates checked-in `CLAUDE.md` instructions from auto memory; its auto memory is machine-local, shared across worktrees in the same repo, loads only the first 200 lines or 25KB of `MEMORY.md` at session start, and pulls topic files on demand [S1]. Codex says memories are off by default, should not hold required team guidance, and should be treated as local recall while `AGENTS.md` remains the durable source of repository rules [S8].

Observations: The strongest pattern is a two-layer design. Repo-scoped instruction files carry policy, commands, architecture, and review norms that must apply every time. Local memory carries recurring pitfalls, workflow habits, and personal or machine-specific context. Claude and Codex both treat memory as non-authoritative compared with checked-in instructions, which means memory improves convenience but is a weak control plane. Codex’s ability to exclude externally-contextualized threads from memory generation is also a useful guard against contaminating future sessions with one-off web/MCP context [S8].

Recommendations: Put mandatory build/test commands, architectural constraints, review rubrics, and naming conventions in `CLAUDE.md` or `AGENTS.md`. Use memory only for stable but non-critical recall such as debugging notes or local workflow preferences. Keep memory entrypoints concise and topicized. Do not rely on memory for CI, cloud sandboxes, or cross-machine consistency.

Impact on agent LLM = HIGH + memory determines whether the agent starts each session with the right defaults or re-discovers them expensively and unreliably.

## F2. Prompt scaffolding and instruction hierarchy are major reliability levers, especially for long-horizon coding

Evidence: Codex best practices recommend a prompt structure with Goal, Context, Constraints, and Done-when criteria [S12]. The Codex launch post states that AGENTS.md files guide navigation, testing, and project standards, and it publishes part of the codex-1 system message to show default behaviors [S7]. The Codex Prompting Guide recommends starting from the standard Codex-Max prompt, being explicit about autonomy/tool use, and preserving assistant `phase` metadata because dropping it can significantly degrade performance in `gpt-5.3-codex` integrations [S13]. Claude recommends precise prompts with specific files and example patterns, and notes that `CLAUDE.md` content is delivered after the system prompt rather than as the system prompt itself [S1][S2].

Observations: Coding-agent performance is sensitive not just to what instructions exist, but to where they live in the stack. Claude’s docs make clear that `CLAUDE.md` is advisory user-level context, not hard system-level enforcement [S1]. Codex similarly benefits from clear AGENTS scaffolding and from harness behavior that preserves system-prompt conventions such as `phase` [S13]. The practical consequence is that brittle, verbose, or conflicting instruction layers tend to fail exactly on longer or riskier tasks.

Recommendations: Standardize a task template with Goal, Context, Constraints, Done-when, and explicit validation commands. Keep instruction files short, scoped, and non-overlapping. Duplicate the most critical task-specific constraints in the live prompt even if they are already in project instructions. If you are building your own Codex-like harness, preserve model-specific metadata and prompt structure exactly rather than paraphrasing them away.

Impact on agent LLM = HIGH + instruction quality directly affects scoping, tool choice, stopping behavior, and adherence to project constraints.

## F3. Context-budget hygiene is a first-order performance variable, not an operational detail

Evidence: Anthropic says Claude’s context window fills quickly with messages, file reads, and command output, and performance degrades as it fills; it recommends aggressive use of `/clear`, compaction, and session separation [S2]. Codex best practices recommend Plan mode for hard tasks so the agent gathers context before editing, and the Prompting Guide emphasizes batching reads and parallelizing exploration to reduce waste [S12][S13]. OpenAI’s long-horizon Codex post frames the current frontier as increasing task time horizon and the ability to stay coherent over longer runs [S15].

Observations: For coding agents, “more context” is often the wrong optimization. Long exploratory sessions accumulate failed branches, irrelevant reads, and stale assumptions. That hurts planning and edit quality. Claude’s explicit warning that performance degrades as context fills is a strong signal that context management is part of the product discipline, not just a UI affordance [S2]. Codex’s compaction- and batching-oriented guidance points in the same direction [S13][S15].

Recommendations: Separate exploration, implementation, and review into different sessions or subagents when possible. Reset or compact after dead-end attempts. Feed line-specific files, targeted logs, and exact commands rather than broad repository dumps. Treat context like a scarce budget that must be curated continuously.

Impact on agent LLM = HIGH + once the context window is polluted, otherwise-capable models spend tokens reasoning over noise and follow instructions less reliably.

## F4. Multi-agent coordination works best when it isolates uncertainty, not when it blindly multiplies autonomy

Evidence: Anthropic’s multi-agent research system uses a planning agent that spins up parallel agents to search simultaneously, and says the production challenge is not only search breadth but coordination, evaluation, and reliability [S6]. Claude best practices recommend subagents for investigation specifically because they explore in separate context windows and return summaries without cluttering the main conversation [S2]. OpenAI’s Codex launch recommends assigning well-scoped tasks to multiple agents simultaneously [S7]. OpenAI’s agent-building guide describes orchestration patterns from single-agent loops to manager/worker multi-agent systems and advises incremental adoption rather than jumping immediately to full autonomy [S14].

Observations: The win condition for multiple agents is context isolation plus parallel search, not raw token parallelism. Research, code archaeology, verification, and review are naturally separable from implementation. Claude’s subagent guidance is especially clear that separate contexts are a core benefit [S2]. OpenAI’s orchestration guidance implies the same pattern for Codex-like systems, even when the document is general to agents rather than coding agents [S14] (inference).

Recommendations: Use dedicated investigator, implementer, and reviewer agents for large or ambiguous tasks. Give each agent narrow tool access and a crisp subproblem. Use parallel fan-out when subproblems are independent or when the search space is broad; do not use it for tightly coupled edits that require a single shared mental model.

Impact on agent LLM = HIGH + decomposition often matters more than raw model quality for broad, messy engineering work.

## F5. Live IDE/editor state is high-value context because it captures what the developer is actually looking at now

Evidence: Claude’s VS Code extension supports plan review, auto-accept or gated edit modes, selection-aware `@` references with exact line ranges, automatic visibility of highlighted code, and resumable conversation history [S3]. Claude’s JetBrains plugin shares current selection/tab context, IDE diagnostics, and diff views directly with Claude [S4]. Codex’s IDE extension is designed to prompt with open files and selections, switch approval modes, delegate longer jobs to the cloud, and then apply follow-up diffs locally [S10].

Observations: IDE state solves a class of context problems that repo-level retrieval does not. Open files, selections, diagnostics, terminal output, and local diffs express the developer’s current intent and pain point with much less prompt entropy than a natural-language description. This is especially important for debugging, refactors anchored to a specific span, and follow-up edits after reviewing generated code.

Recommendations: Prefer agents that are attached to the active editor when doing interactive coding. Pass exact file-and-line references, selected regions, and diagnostics rather than paraphrasing them. Review diffs in the IDE, not only in raw terminal or chat output. Treat editor state as primary context for interactive tasks and repository memory/instructions as the background layer.

Impact on agent LLM = HIGH + live editor state sharply reduces ambiguity and helps the model align edits with the developer’s immediate focus.

## F6. Few-shot guidance is strongest when it comes from repo-native artifacts: existing code, tests, commands, and review templates

Evidence: Anthropic recommends pointing Claude to example patterns in the codebase rather than relying on vague instructions [S2]. Codex best practices encourage `@`-mentioning the right files, folders, docs, examples, and errors in the prompt [S12]. The Codex Prompting Guide says the open-source `codex-cli` agent is the best reference implementation for harness design and recommends explicit good/bad examples for tool use [S13].

Observations: In coding-agent workflows, “few-shot” is often not chat examples. It is the presence of canonical files, adjacent implementations, test cases, lint commands, review rubrics, and execution-plan templates. Those artifacts anchor the agent to local style and to acceptable operational behavior. This is more robust than generic “write idiomatic code” instructions because the agent can imitate known-good patterns under the actual repository’s constraints.

Recommendations: Maintain canonical examples for common patterns and refer to them explicitly in prompts and instruction files. Include exact test/lint commands in `CLAUDE.md` or `AGENTS.md`. Add reusable artifacts such as `PLANS.md`, `code_review.md`, or skill definitions when a workflow repeats. Prefer “follow this nearby pattern” over “do it in the usual way.”

Impact on agent LLM = HIGH + concrete exemplars reduce stylistic drift, unnecessary invention, and tool misuse.

## F7. Issue and PR context is highly effective when structured; raw tickets and raw diffs are not enough

Evidence: Claude Code GitHub Actions can act on PRs and issues via `@claude`, and Anthropic explicitly recommends using issue templates to provide context while keeping `CLAUDE.md` concise [S5]. Codex code review in GitHub reviews the PR diff, follows repository guidance, and posts review comments; in GitHub it intentionally focuses on P0/P1 issues, and OpenAI recommends repository-specific review guidance via `AGENTS.md` [S11][S12].

Observations: Issue trackers and PR systems are not merely workflow integrations; they are context sources with strong signal when normalized. Templates force reporters to provide reproduction steps, likely location, constraints, and acceptance criteria. PR reviews anchor the agent on the actual diff and on review policy. Codex’s severity filtering is a reminder that too much review noise reduces trust [S11]. Claude’s docs similarly suggest that issue templates improve performance rather than leaving the agent to infer missing structure from free-form tickets [S5].

Recommendations: Standardize issue templates with symptom, repro, scope boundaries, environment, and done-when fields. Expose the base-branch diff, related issue, and repository review rubric to the agent. Keep review guidance in a referenced file rather than repeating it ad hoc in each PR. Use the agent to triage or review, but preserve human ownership of merge decisions.

Impact on agent LLM = HIGH + structured issue/PR context converts ambiguous work requests into executable and reviewable tasks.

## F8. Human feedback loops are part of the optimal architecture, not a fallback for weak agents

Evidence: Claude recommends course-correcting early, stopping mid-action, rewinding, and clearing sessions after repeated failed corrections; Anthropic says tight feedback loops usually yield better solutions faster [S2]. Claude’s VS Code extension lets users review and edit plans before accepting them [S3]. Codex provides verifiable evidence through cited terminal logs and test outputs, and OpenAI says users should manually review and validate generated code before integration [S7]. Codex best practices recommend Plan mode for difficult tasks because it lets the agent ask clarifying questions and build a stronger plan first [S12].

Observations: The highest-performing operating mode is supervised autonomy. Both vendors are explicit that planning, traceability, and review are core workflow components. Human feedback is most valuable early, when it corrects scope and hidden constraints, and late, when it validates that logs/tests actually support the claimed result. This is particularly important in coding because plausible-but-wrong output often looks mergeable.

Recommendations: Add a mandatory plan-review checkpoint for non-trivial tasks. Require visible evidence: test output, diff, and rationale tied to the issue. Interrupt early when the agent drifts. Keep a hard human gate before merge or production rollout even when the agent ran tests successfully.

Impact on agent LLM = HIGH + fast human correction prevents expensive drift, and review of evidence is essential for safe integration.

## F9. Agentic search beats static RAG when the task depends on live state, tool execution, or multi-hop verification; static retrieval still wins for stable lookup

Evidence: OpenAI’s guide to building agents recommends starting with the simplest viable architecture and distinguishes data-retrieval tools from action and orchestration tools [S14]. Anthropic’s multi-agent research post says open-ended research is dynamic and path-dependent, which is why the system uses planning plus parallel search agents [S6]. Anthropic’s Claude best practices recommend letting Claude fetch what it needs with Bash, MCP, or file reads, while warning that context degrades when overloaded [S2]. Codex best practices similarly emphasize task context, MCP integration, and reusable skills rather than trying to stuff everything into the prompt [S12].

Observations: Static RAG is best when the needed answer is likely already present in stable documents or embeddings and the cost of tool use is not justified. Agentic search is better when the answer depends on current repo state, branch diffs, failing tests, IDE diagnostics, CI logs, or multi-step evidence gathering. For coding agents, many hard tasks are not “retrieve the right paragraph” problems; they are “find, run, compare, and verify” problems. This last sentence is an inference from [S2], [S6], and [S14], but it aligns with the official guidance.

Recommendations: Use a hybrid pattern. Start with cheap retrieval/indexed lookup for stable artifacts such as architecture docs, policy, and API references. Escalate to agentic repo search, shell commands, IDE diagnostics, or external tools when retrieval confidence is low or the task is stateful. Avoid dumping large retrieved corpora into the context window; use retrieval to seed the search, not to replace it.

Impact on agent LLM = HIGH + choosing the wrong context-acquisition mode either starves the agent of live evidence or overwhelms it with irrelevant retrieved text.

## Synthesis (1 paragraph)

Across Claude Code and Codex, the strongest pattern is that coding-agent quality depends less on raw model intelligence than on disciplined context engineering around it. Durable repo instructions (`CLAUDE.md`, `AGENTS.md`), concise local memory, live editor state, structured issue/PR metadata, and repo-native examples all reduce ambiguity before the model starts reasoning. Multi-agent setups help when they isolate research, implementation, and review into separate contexts, while human feedback remains essential at plan time and merge time. The best-performing workflow is therefore a hybrid: keep mandatory rules in checked-in instruction files, preserve a clean context budget, seed the agent with exact local examples and issue/diff context, let it use tools to explore live state when retrieval is insufficient, and require evidence-backed human review before accepting the result.

[S1]: <https://code.claude.com/docs/en/memory>
[S2]: <https://code.claude.com/docs/en/best-practices>
[S3]: <https://code.claude.com/docs/en/vs-code>
[S4]: <https://code.claude.com/docs/en/jetbrains>
[S5]: <https://code.claude.com/docs/en/github-actions>
[S6]: <https://www.anthropic.com/engineering/multi-agent-research-system>
[S7]: <https://openai.com/index/introducing-codex/>
[S8]: <https://developers.openai.com/codex/memories>
[S9]: <https://developers.openai.com/codex/guides/agents-md>
[S10]: <https://developers.openai.com/codex/ide>
[S11]: <https://developers.openai.com/codex/integrations/github>
[S12]: <https://developers.openai.com/codex/learn/best-practices>
[S13]: <https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide>
[S14]: <https://openai.com/business/guides-and-resources/a-practical-guide-to-building-ai-agents/>
[S15]: <https://developers.openai.com/blog/run-long-horizon-tasks-with-codex>
