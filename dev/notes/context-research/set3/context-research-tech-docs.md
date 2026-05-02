# Scope

Web research on how Claude Code and OpenAI Codex use technical documentation during AI-driven coding, with emphasis on which documentation forms most improve agent performance: requirements, architecture descriptions, design docs, and interface/API contracts. Priority was given to official Anthropic/OpenAI docs and product posts, then to research papers on documentation retrieval, repository context, and agent coding performance.

## Sources (URLs cited)

- [S1] OpenAI, "Introducing Codex" (May 16, 2025): <https://openai.com/index/introducing-codex/>
- [S2] OpenAI Codex docs, "Custom instructions with AGENTS.md": <https://developers.openai.com/codex/guides/agents-md>
- [S3] OpenAI docs, "Docs MCP": <https://developers.openai.com/learn/docs-mcp>
- [S4] OpenAI API docs, "Function calling": <https://developers.openai.com/api/docs/guides/function-calling>
- [S5] OpenAI API docs, "Structured outputs": <https://developers.openai.com/api/docs/guides/structured-outputs>
- [S6] Anthropic Claude Code docs, "How Claude remembers your project": <https://code.claude.com/docs/en/memory>
- [S7] Anthropic Claude Code docs, "Common workflows": <https://code.claude.com/docs/en/common-workflows>
- [S8] Anthropic docs, "Claude prompting best practices" (long-context section): <https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#long-context-prompting>
- [S9] Anthropic Claude Code docs, "Connect Claude Code to tools via MCP": <https://code.claude.com/docs/en/mcp>
- [S10] Zhou et al., "DocPrompting: Generating Code by Retrieving the Docs" (arXiv:2207.05987): <https://arxiv.org/abs/2207.05987>
- [S11] Zhang et al., "RepoCoder: Repository-Level Code Completion Through Iterative Retrieval and Generation" (arXiv:2303.12570): <https://arxiv.org/abs/2303.12570>
- [S12] Zhu et al., "SWE-ContextBench: A Benchmark for Context Learning in Coding" (arXiv:2602.08316): <https://arxiv.org/abs/2602.08316>
- [S13] Wang et al., "Evaluating Repository-level Software Documentation via Question Answering and Feature-Driven Development" (arXiv:2604.06793): <https://arxiv.org/abs/2604.06793>

## Findings F1..F7

## F1. Requirements docs help most when they define success criteria, scope boundaries, and verification steps explicitly

- Evidence
  - Anthropic recommends instructions that keep only facts Claude should hold every session: build commands, conventions, project layout, and "always do X" rules; project `CLAUDE.md` should include build/test commands, coding standards, architectural decisions, naming conventions, and workflows [S6].
  - Anthropic's prompting guidance says long and complex tasks perform better when the prompt is clear, structured, and explicit about the task [S8].
  - OpenAI states Codex performs best with configured dev environments, reliable testing setups, and clear documentation; `AGENTS.md` can tell Codex how to navigate the repo and which commands to run for testing [S1][S2].
- Observations
  - For coding agents, a "requirement" is not just product intent. The highest-value requirement doc also encodes definition of done, non-goals, files or subsystems in scope, and exact validation commands.
  - Vague product prose leaves agents to infer acceptance criteria from code, which increases unnecessary exploration and patch churn.
- Recommendations
  - Put a short, explicit requirement block near the code task surface: goal, constraints, non-goals, acceptance checks, and exact test/lint commands.
  - Prefer imperative statements over narrative prose.
  - Treat "how to verify" as part of the requirement, not as a separate note.
- Impact on agent LLM = HIGH + rationale
  - Claude and Codex both officially expose repo instruction files as operating context. Missing success criteria directly affects planning, tool use, and stopping conditions.

## F2. Repo-local instruction files (`CLAUDE.md`, `AGENTS.md`) are the most leverage-dense documentation format for Claude and Codex

- Evidence
  - Anthropic positions `CLAUDE.md` as session-loaded project memory for coding standards, workflows, and project architecture; more specific files take precedence, and project files are meant to be shared in version control [S6].
  - Anthropic also notes shorter files produce better adherence, while overly broad information should move into scoped rules or topic files [S6].
  - OpenAI recommends `AGENTS.md` for navigation, test commands, and repo standards, and the Codex docs provide a dedicated guide for custom instructions with `AGENTS.md` [S1][S2].
- Observations
  - These files are not generic docs; they are agent operating manuals. Their value is high because they are loaded or referenced as part of normal execution rather than discovered opportunistically.
  - The most usable structure is layered: a short root file for global rules, plus narrower path-scoped files for subsystem-specific architecture or conventions.
- Recommendations
  - Keep root agent docs concise and operational.
  - Split subsystem-specific guidance into nested files or scoped rule files near the affected code.
  - Include commands, invariants, ownership boundaries, naming rules, and known traps before historical background.
- Impact on agent LLM = HIGH + rationale
  - This is the most direct, officially supported way to shape Claude/Codex behavior inside a codebase. Good repo-local docs reduce retrieval failure and ambiguity before the agent starts editing.

## F3. Architecture docs are useful only when they are concrete about flows, boundaries, and key models

- Evidence
  - Anthropic's recommended codebase-understanding workflow explicitly asks for "main architecture patterns," "key data models," and how cross-cutting concerns like authentication are handled [S7].
  - OpenAI presents Codex as useful for codebase Q&A and architecture understanding, and recommends well-scoped tasks for parallel agents [S1].
- Observations
  - Architecture docs help agents when they answer practical questions: where requests enter, which modules own state transitions, what persistence layers are touched, and what invariants span components.
  - Inference for Claude and Codex: architecture docs that stay at principle level but omit file paths, call chains, and affected interfaces are much less actionable for coding work.
  - Text-first diagrams are especially useful because agents can quote, summarize, and relate them back to source locations.
- Recommendations
  - Document request flows, component responsibilities, major data models, and failure boundaries in Markdown with text-renderable diagrams such as Mermaid.
  - Add "change surfaces" to architecture docs: which packages usually move together for a feature or bug class.
  - Link architecture sections to concrete source directories or entrypoints.
- Impact on agent LLM = HIGH + rationale
  - Architecture comprehension is a prerequisite for non-local edits, refactors, and safe bug fixes. Without this, agents over-read the repo or make brittle single-file patches.

## F4. Design docs and external requirements are invisible to the agent unless they are connected into the working context

- Evidence
  - Anthropic's MCP guidance explicitly uses design and planning examples such as implementing a feature from Jira and updating a template from Figma designs posted in Slack [S9].
  - Anthropic's Claude Code overview says Claude can pull external data sources such as Google Drive, Figma, and Slack through MCP [S9].
  - OpenAI's Docs MCP is explicitly described as a way to pull documentation into an agent's context while working [S3].
- Observations
  - A design spec sitting in Drive, Figma, Notion, or Jira does not help a coding agent unless the execution environment can fetch it and the prompt makes it relevant.
  - Inference for Claude and Codex: external product/design context is often more important than more code context when the task is UI behavior, workflow compliance, or feature parity with a spec.
- Recommendations
  - Connect issue trackers, design systems, and internal docs through MCP or equivalent retrieval paths.
  - Prefer canonical links over copied fragments so the agent sees the latest version.
  - Treat access control and trust boundaries as part of doc architecture; only expose sources the agent should rely on.
- Impact on agent LLM = HIGH + rationale
  - Many coding failures are requirements failures, not syntax failures. If the agent cannot see the authoritative spec, it will optimize for the codebase's current shape instead of intended behavior.

## F5. Interface and API contracts are most agent-usable when they are machine-readable, constrained, and example-rich

- Evidence
  - OpenAI's function-calling docs define functions by JSON Schema and explicitly say the schema informs the model what the function does and what inputs it expects [S4].
  - OpenAI's Structured Outputs docs say schema-constrained outputs provide reliable type safety and reduce the need for strongly worded prompts [S5].
  - RepoCoder specifically reports gains on repository-level completion including API invocation scenarios, showing that retrieving the right interface context matters for code generation [S11].
- Observations
  - Inference for Claude and Codex: interface specs like OpenAPI, JSON Schema, protobuf, GraphQL schema, SQL DDL, and typed examples are more useful than prose API references because they remove ambiguity about names, fields, enums, and required inputs.
  - Interface docs become much more valuable when they include examples, error shapes, auth expectations, and compatibility notes.
- Recommendations
  - Keep canonical contracts machine-readable and versioned with the code.
  - Add examples for happy path, error path, and edge constraints.
  - If prose docs exist, make them secondary to the contract and explicitly point back to the contract artifact.
- Impact on agent LLM = HIGH + rationale
  - Interface mismatches are a major source of agent-generated defects. Structured contracts sharply reduce guessing at call signatures and response shapes.

## F6. Retrieval quality and document structure matter more than raw document volume

- Evidence
  - Anthropic recommends putting long documents near the top of context, structuring multi-document inputs with tags and metadata, and grounding responses in quoted evidence first [S8].
  - DocPrompting shows that retrieving relevant documentation and then generating from it improves code generation performance across benchmarks [S10].
  - RepoCoder reports repository-level retrieval that improves over in-file baselines by more than 10% [S11].
  - SWE-ContextBench reports that correctly selected summarized experience improves accuracy while reducing runtime and token cost, while unfiltered or incorrectly selected experience can provide limited or negative benefit [S12].
- Observations
  - The failure mode is not just "too little context"; it is often "too much low-signal context."
  - The best agent-facing doc corpus is segmented, labeled, and summarizable: short entrypoints, topic files, scoped rules, and stable identifiers.
  - Inference for Claude and Codex: dumping a full wiki or monolithic design doc into context is usually worse than retrieving the relevant sections plus a concise summary.
- Recommendations
  - Build docs as retrievable chunks with stable headings and metadata.
  - Add short index files that point to deeper docs.
  - Ask agents to quote or cite the exact retrieved section before implementing, especially on multi-doc tasks.
- Impact on agent LLM = HIGH + rationale
  - Context windows are large but still selective systems. Better retrieval and chunk design directly improve precision, latency, and cost.

## F7. Documentation materially improves coding performance, but it complements rather than replaces source code and executable checks

- Evidence
  - SWD-Bench reports that higher-quality repository documentation improves SWE-Agent issue-solving by 20.00%, while also showing that source code provides complementary value [S13].
  - OpenAI says Codex provides terminal logs and test outputs as verifiable evidence and performs best with reliable testing setups [S1].
  - Anthropic's common workflows repeatedly end with running and verifying tests after code changes [S7].
- Observations
  - Good docs accelerate orientation, localization, and intent understanding. Code and tests remain the ground truth for actual behavior.
  - The most effective documentation for agents reduces search and hypothesis cost before execution; it does not eliminate the need to execute and verify.
- Recommendations
  - Pair every major architecture or requirements artifact with an executable validation path: tests, linters, type checks, contract tests, or fixture-based examples.
  - Add drift checks where possible so docs, contracts, and tests fail together when behavior changes.
  - Use docs to explain why; use tests and schemas to prove what.
- Impact on agent LLM = HIGH + rationale
  - This is the practical operating model for Claude/Codex-style coding agents: docs for intent and navigation, code/tests for truth and closure.

## Synthesis (1 paragraph)

The strongest pattern across Claude, Codex, and the cited research is that coding agents benefit less from "more documentation" in the abstract than from the right documentation in the right form: concise repo-local instruction files for operating rules, explicit requirement docs with acceptance checks, concrete architecture docs that map behavior to source structure, connected external design/product artifacts, and machine-readable interface contracts. The research reinforces the product docs: retrieval and summarization of the relevant slices outperform both in-file-only reasoning and indiscriminate context stuffing, while high-quality documentation measurably improves issue-solving when paired with source code and executable validation. In practice, the best agent-facing doc stack is layered, scoped, structured, and verifiable.

[S1]: <https://openai.com/index/introducing-codex/>
[S2]: <https://developers.openai.com/codex/guides/agents-md>
[S3]: <https://developers.openai.com/learn/docs-mcp>
[S4]: <https://developers.openai.com/api/docs/guides/function-calling>
[S5]: <https://developers.openai.com/api/docs/guides/structured-outputs>
[S6]: <https://code.claude.com/docs/en/memory>
[S7]: <https://code.claude.com/docs/en/common-workflows>
[S8]: <https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices#long-context-prompting>
[S9]: <https://code.claude.com/docs/en/mcp>
[S10]: <https://arxiv.org/abs/2207.05987>
[S11]: <https://arxiv.org/abs/2303.12570>
[S12]: <https://arxiv.org/abs/2602.08316>
[S13]: <https://arxiv.org/abs/2604.06793>
