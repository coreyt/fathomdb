# Scope

Cross-cutting synthesis of six parallel research reports on how to provide Claude Code and OpenAI Codex the right context for AI-driven coding. This summary integrates findings from:

- `dev/tmp3/context-research-tech-docs.md`
- `dev/tmp3/context-research-source-code.md`
- `dev/tmp3/context-research-comments.md`
- `dev/tmp3/context-research-tests.md`
- `dev/tmp3/context-research-dev-env.md`
- `dev/tmp3/context-research-other.md`

It emphasizes what consistently matters across requirements, architecture/design/interface docs, source code, comments, tests, development environment, memory/prompts/orchestration, and live workflow artifacts.

## Key Sources

- OpenAI, "Introducing Codex": <https://openai.com/index/introducing-codex/>
- OpenAI, "Harness engineering: leveraging Codex in an agent-first world": <https://openai.com/index/harness-engineering/>
- OpenAI Codex docs, "AGENTS.md": <https://developers.openai.com/codex/guides/agents-md>
- OpenAI Codex docs, "Best practices": <https://developers.openai.com/codex/learn/best-practices>
- OpenAI, "A practical guide to building AI agents": <https://openai.com/business/guides-and-resources/a-practical-guide-to-building-ai-agents/>
- Anthropic, "Best Practices for Claude Code": <https://www.anthropic.com/engineering/claude-code-best-practices>
- Anthropic Claude Code docs, "Memory": <https://code.claude.com/docs/en/memory>
- Anthropic Claude Code docs, "Subagents": <https://docs.anthropic.com/en/docs/claude-code/sub-agents>
- Anthropic, "Contextual Retrieval": <https://www.anthropic.com/engineering/contextual-retrieval>
- Anthropic, "Multi-agent research system": <https://www.anthropic.com/engineering/multi-agent-research-system>
- Aider, "Repository map": <https://aider.chat/docs/repomap.html>
- RepoCoder paper: <https://arxiv.org/abs/2303.12570>
- SWE-bench paper: <https://arxiv.org/abs/2310.06770>
- Lost in the Middle paper: <https://direct.mit.edu/tacl/article/doi/10.1162/tacl_a_00638/119630/Lost-in-the-Middle-How-Language-Models-Use-Long>

## Cross-Cutting Findings

## F1. Layered context beats monolithic context

- Evidence
  OpenAI and Anthropic both recommend short repo-local instruction files (`AGENTS.md`, `CLAUDE.md`) plus deeper docs loaded only when relevant. OpenAI’s harness engineering writeup explicitly says the successful pattern was to give Codex "a map, not a 1,000-page instruction manual." Anthropic’s Claude Code guidance repeatedly warns that context fills quickly and degrades performance.
- Observations
  The winning pattern is a hierarchy:
  1. short root instructions
  2. scoped subsystem instructions
  3. retrievable deeper docs
  4. task-specific files/logs/tests
     Full-repo dumps, giant manuals, and long noisy sessions reduce adherence and salience even on large-context models.
- Recommendations
  Keep root agent docs concise and operational.
  Split detailed guidance by directory, subsystem, or topic.
  Retrieve only the slices needed for the current task.
- Impact on agent LLM = HIGH + rationale
  This shapes the default context every session and directly affects navigation, instruction retention, and token efficiency.

## F2. The highest-leverage context is executable, not descriptive

- Evidence
  Anthropic calls tests, screenshots, and expected outputs the single highest-leverage way to improve Claude Code. OpenAI says Codex performs best with reliable tests and configured environments, and can iteratively run them. SWE-bench and related repair literature reinforce that real software tasks require interaction with execution environments, not just code synthesis.
- Observations
  Across all six dimensions, the strongest context is anything that closes the loop:
  test commands, repro scripts, stack traces, CI failures, screenshots, contract tests, and precise "done when" checks. Natural-language requirements help planning, but execution artifacts determine whether the agent can self-correct.
- Recommendations
  Put narrow test commands and expected outputs in agent-facing repo instructions.
  Prefer one-command repros and targeted failures over broad prose descriptions.
  Treat verification steps as part of the spec.
- Impact on agent LLM = HIGH + rationale
  Without executable feedback, the agent optimizes for plausibility. With it, the agent can search, verify, and stop correctly.

## F3. Repo-local instructions are the control plane; memory is secondary

- Evidence
  Claude distinguishes checked-in `CLAUDE.md` from local memory and explicitly warns that memory is not the authoritative source of project rules. Codex says memories are optional local recall and should not hold required team guidance; `AGENTS.md` is the durable source of repository rules.
- Observations
  There is a consistent separation of concerns:
  checked-in instructions for mandatory team behavior
  local memory for convenience and recall
  live prompt for task-specific constraints
  This separation matters because cloud tasks, CI runs, and collaborator machines cannot safely depend on private local memory.
- Recommendations
  Put build/test commands, architecture boundaries, review rubrics, and forbidden patterns in version-controlled instruction files.
  Use memory only for stable but non-critical recall.
  Repeat the most important task constraints in the live prompt even if they already exist in repo instructions.
- Impact on agent LLM = HIGH + rationale
  This determines whether sessions start with the right defaults and whether behavior is consistent across users and environments.

## F4. Retrieval for coding should be hybrid: lexical, semantic, and structural

- Evidence
  Anthropic’s Contextual Retrieval argues for combining retrieval modes instead of relying on embeddings alone. Sourcegraph and Aider show production use of symbol graphs and repo maps. RepoCoder reports gains from iterative repository retrieval over in-file-only baselines.
- Observations
  Coding tasks generate mixed query types:
  exact identifiers
  stack traces
  file paths
  natural-language features
  architectural relationships
  No single retrieval mode serves all of them well. Exact search is best for names and errors; semantic search is better for analogous patterns; graph expansion is better for callers, callees, symbols, tests, and config neighbors.
- Recommendations
  Use BM25/grep/path search for exact identifiers and failures.
  Use embeddings for fuzzy intent and analogous implementations.
  Use symbol graphs or repo maps for cheap global awareness and dependency expansion.
  Rerank before sending context to the model.
- Impact on agent LLM = HIGH + rationale
  Retrieval quality is often the difference between a repo-correct edit and a generic but wrong patch.

## F5. Structured artifacts outperform prose-heavy artifacts

- Evidence
  The tech-docs and comments tracks both found that machine-readable contracts, structured docstrings, explicit examples, and stable headings beat freeform prose. OpenAI’s function-calling and structured-output docs show why schemas reduce ambiguity. Research on docstrings and documentation retrieval shows structure improves machine usability.
- Observations
  Agents benefit most from artifacts that are easy to parse, compare, and validate:
  OpenAPI
  JSON Schema
  protobuf
  GraphQL schema
  typed examples
  structured docstrings
  test cases
  Prose still matters for rationale and tradeoffs, but it is weaker as a contract surface.
- Recommendations
  Keep canonical interfaces machine-readable and versioned with code.
  Use structured docstrings for public APIs.
  Add happy-path, error-path, and edge-case examples.
  Treat prose as explanatory context, not the primary contract.
- Impact on agent LLM = HIGH + rationale
  Structured artifacts reduce guessing at names, types, constraints, and expected behavior.

## F6. Trustworthiness matters more than volume; stale text is actively harmful

- Evidence
  The comments track found consistent support for the harm caused by stale, redundant, or inconsistent comments. OpenAI’s harness engineering post makes the same point at repo-doc scale: monolithic stale guidance became an "attractive nuisance." Long-context research further shows that irrelevant or low-signal content can crowd out what matters.
- Observations
  Bad context is worse than missing context. This applies to comments, ADR fragments, root instruction files, issue descriptions, TODOs, and test output. Agents cannot reliably distinguish stale-but-confident prose from current truth unless the environment gives them stronger signals.
- Recommendations
  Delete misleading comments instead of leaving them in place.
  Update docs and comments in the same change as behavior changes.
  Keep issue templates and agent docs under active maintenance.
  Add lightweight freshness checks where practical.
- Impact on agent LLM = HIGH + rationale
  Stale context steers planning and edits toward wrong assumptions with high confidence.

## F7. Comments and docs help most when they encode intent that code alone does not expose

- Evidence
  The comments research consistently favored rationale, invariants, warnings, restrictions, and architectural decisions over commentary that simply restates syntax. Claude and Codex official guidance both encourage storing information the model cannot infer from code alone.
- Observations
  The most valuable natural language near code is semantic compression of hidden facts:
  why a branch exists
  what invariant must hold
  what compatibility rule matters
  what failure mode or quirk is non-obvious
  By contrast, paraphrasing obvious control flow consumes context without increasing understanding.
- Recommendations
  Prefer comments that explain why, constraints, side effects, and hazards.
  Use docstrings for callable behavior and inline comments for local rationale.
  Capture long-lived design decisions in ADR fragments or module docs and link them from the code path they govern.
- Impact on agent LLM = HIGH + rationale
  These are exactly the facts that agents struggle to reconstruct cheaply from source alone.

## F8. Environment legibility is part of context engineering

- Evidence
  The dev-env track found strong agreement across Anthropic and OpenAI: agents perform better when commands, logs, CI artifacts, sandboxes, and setup steps are explicit and reproducible. OpenAI’s Codex cloud and Anthropic’s Claude Code both emphasize isolated execution environments, permission controls, and verifiable logs.
- Observations
  A coding agent needs the repo to be legible as an operating environment, not just as a text corpus. Important environment context includes:
  how to build
  how to run focused tests
  where logs live
  which commands are safe
  how permissions work
  what CI artifacts mean
  whether execution is reproducible
- Recommendations
  Provide reproducible setup paths.
  Expose one-command focused logs and tests.
  Pre-approve bounded safe commands.
  Keep cloud and local assumptions aligned as much as possible.
- Impact on agent LLM = HIGH + rationale
  Strong environment legibility is what converts reasoning into reliable action.

## F9. Subagents and human review improve quality by isolating uncertainty and preserving clean context

- Evidence
  Anthropic’s subagent and multi-agent guidance emphasizes separate context windows for investigation. OpenAI recommends parallel well-scoped Codex tasks. Both vendors recommend human review of plans or outputs before integration.
- Observations
  The multi-agent advantage is not raw parallelism. It is context isolation:
  one agent explores
  one agent implements
  one agent reviews
  Human checkpoints remain important because plausible-but-wrong code often survives local reasoning unless somebody reviews the diff and evidence.
- Recommendations
  Split research, implementation, and review into separate contexts for non-trivial tasks.
  Use explicit ownership and output contracts for subagents.
  Keep a human gate before merge or production rollout.
- Impact on agent LLM = HIGH + rationale
  This reduces context pollution, self-confirmation bias, and long-horizon drift.

## Relative Impact By Dimension

- `tests`: HIGH. Strongest direct effect because tests shape both search and stopping criteria.
- `dev-env`: HIGH. Needed for reliable verification, reproducibility, and safe action.
- `source-code`: HIGH. Existing code, repo maps, and hybrid retrieval are core grounding signals.
- `tech-docs`: HIGH. Strong when scoped, structured, and tied to acceptance criteria.
- `other`: HIGH. Prompt scaffolding, issue/PR structure, editor state, and orchestration materially change outcomes.
- `comments`: MED to HIGH. High when they encode rationale/invariants; low or negative when stale or redundant.

## Practical Synthesis

The most effective context stack for Claude Code or Codex is layered and operational:

1. A short checked-in root instruction file that gives commands, scope boundaries, architecture entrypoints, and pointers to deeper docs.
2. Scoped subsystem docs and machine-readable interface contracts kept near the code they govern.
3. A repo map or symbol graph plus hybrid retrieval to find the right code, tests, and neighbors without flooding the context window.
4. High-trust nearby prose that captures rationale, invariants, and design decisions the code does not say clearly by itself.
5. Explicit executable feedback loops: focused tests, repro scripts, CI failures, logs, screenshots, and expected outputs.
6. Cleanly separated contexts for exploration, implementation, and review, with human checkpoints before final integration.

The main anti-patterns are equally clear: giant instruction files, stale comments, whole-repo dumps, weak or flaky tests, noisy logs, and reliance on private memory instead of checked-in repo guidance.

## Synthesis

Across academic work, frontier-lab docs, and engineering blog evidence, the central conclusion is that coding-agent performance depends less on raw context quantity than on context structure, trustworthiness, and executability. Claude Code and Codex both work best when the repository exposes a concise control plane (`CLAUDE.md` or `AGENTS.md`), retrievable deeper docs, machine-readable contracts, focused source-code navigation aids, and fast verification loops. Comments and design notes help when they preserve intent and invariants that code alone obscures, but they become liabilities when they drift. Multi-agent and IDE-attached workflows further improve results by isolating context and grounding the model in live task state. In practice, the right question is not "how much context should I give the agent?" but "what is the smallest set of high-trust artifacts that lets the agent navigate, decide, act, and verify?"
