# Scope
Research on how to provide existing source-code context to Claude and Codex for AI-driven coding, with emphasis on repository structure, repo maps, selective retrieval, embeddings/RAG, agentic search, code navigation, symbol graphs, and long-context strategies. Focus is on concrete practices that improve agent performance on real codebases rather than generic prompt advice.

# Sources (URLs cited)
- [S1] https://openai.com/index/harness-engineering/
- [S2] https://openai.com/index/introducing-codex/
- [S3] https://developers.openai.com/codex/guides/agents-md
- [S4] https://developers.openai.com/api/docs/models/gpt-5.2-codex
- [S5] https://developers.openai.com/codex/cloud
- [S6] https://code.claude.com/docs/en/best-practices
- [S7] https://code.claude.com/docs/en/memory
- [S8] https://code.claude.com/docs/en/sub-agents
- [S9] https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents
- [S10] https://www.anthropic.com/engineering/contextual-retrieval
- [S11] https://aider.chat/docs/repomap.html
- [S12] https://aider.chat/2023/10/22/repomap.html
- [S13] https://sourcegraph.com/docs/cody/core-concepts/context
- [S14] https://sourcegraph.com/docs/cody/core-concepts/code-graph
- [S15] https://sourcegraph.com/blog/how-cody-understands-your-codebase
- [S16] https://huggingface.co/papers/2303.12570
- [S17] https://direct.mit.edu/tacl/article/doi/10.1162/tacl_a_00638/119630/Lost-in-the-Middle-How-Language-Models-Use-Long

# Findings F1..F7

## F1. Top-level repo instructions should be a map, not an encyclopedia.
- Evidence
  OpenAI’s Codex harness writeup says the winning pattern was to “give Codex a map, not a 1,000-page instruction manual,” with a short `AGENTS.md` pointing to a structured in-repo knowledge base [S1]. OpenAI’s Codex launch and `AGENTS.md` docs say Codex uses repo-local instruction files to learn navigation, test commands, and project conventions, with directory-scoped precedence for nested files [S2][S3]. Anthropic’s Claude Code best-practices and memory docs say `CLAUDE.md` is loaded every session, should stay short, and should be broken into scoped rules and on-demand child files in larger repos [S6][S7].
- Observations
  Both Claude and Codex are being steered toward the same context pattern: a small, durable top-level index plus deeper documents loaded only when relevant. This is directly documented for each product, not just an inference [S1][S3][S6][S7].
- Recommendations
  Keep root `AGENTS.md`/`CLAUDE.md` to high-signal items only: build/test commands, architectural entrypoints, domain map, and pointers to deeper docs. Move volatile or subsystem-specific detail into `docs/`, `.claude/rules/`, or nested instruction files close to the code they govern.
- Impact on agent LLM = HIGH + rationale
  This changes the default context every session. If the root file is bloated, it crowds out task-specific code and causes navigation errors before retrieval even starts [S1][S6].

## F2. Progressive disclosure beats full-repo dumping, even for long-context models.
- Evidence
  Anthropic says Claude’s context window fills quickly and performance degrades as it fills; irrelevant history and large file reads make Claude forget earlier instructions and make more mistakes [S6]. The Lost-in-the-Middle paper shows long-context models still use information unevenly and often miss relevant material placed in the middle of large contexts [S17]. Anthropic’s Contextual Retrieval post says that for corpora under about 200k tokens it can be reasonable to include the whole knowledge base for Claude, but beyond that a retrieval approach scales better [S10]. OpenAI’s current Codex model docs advertise 400k context windows, yet OpenAI’s own Codex harness guidance still uses a small map plus structured docs rather than one giant prompt [S1][S4].
- Observations
  Large windows help, but they do not remove the need for selection. For both Claude and Codex, “can fit” is not the same as “should include.” The Codex tie here is partly direct from OpenAI’s harness design and partly inference from long-context research [S1][S4][S17].
- Recommendations
  Start each task with a compact repo overview, then pull in only the files, symbols, tests, and docs relevant to the task. Put must-follow instructions and acceptance criteria near the start of context, and prefer summaries over raw logs or large pasted transcripts.
- Impact on agent LLM = HIGH + rationale
  Overstuffed context directly harms reasoning quality, retrieval salience, and instruction retention [S6][S17].

## F3. Repo maps and symbol graphs are high-leverage context primitives for existing codebases.
- Evidence
  Aider’s repo map sends the model a concise repository-wide map of files plus key classes, functions, signatures, and selected defining lines, then trims it to budget using a graph-ranking algorithm [S11]. Aider’s implementation note says tree-sitter-based symbol extraction and reference analysis are what make large-repo comprehension tractable [S12]. Sourcegraph’s Cody docs say context comes not only from keyword search, but also from a Code Graph built from definitions, references, symbols, and doc comments [S13][S14].
- Observations
  A repo map gives the agent “ambient awareness” of the codebase without paying full-file costs. Symbol graphs are especially useful for code navigation tasks where the agent must identify where a responsibility lives before reading bodies. The explicit repo-map technique is from Aider, but it transfers cleanly to Claude and Codex as an inference because both benefit from the same token-budget constraints [S11][S12].
- Recommendations
  Maintain a machine-generated repo map or symbol index from tree-sitter, LSP, ctags, or similar tooling. Include file path, exported symbols, signatures, and structural relationships. Feed this map as a standing context layer, separate from the narrower task-specific file set.
- Impact on agent LLM = HIGH + rationale
  This improves first-hop navigation, lowers unnecessary file reads, and reduces the chance that the agent edits the right file for the wrong architectural reason [S11][S12][S14].

## F4. Retrieval for code should be hybrid: lexical, semantic, and structural.
- Evidence
  Anthropic’s Contextual Retrieval work shows that combining embeddings with BM25 reduces retrieval failures more than embeddings alone, and reranking improves further [S10]. Sourcegraph documents a production stack that mixes keyword search, native search, and code-graph retrieval rather than relying on one retrieval mode [S13][S14]. Sourcegraph’s engineering blog explains why they moved away from embeddings-only retrieval for large enterprise codebases: privacy, update complexity, and scale all became problems [S15]. RepoCoder reports that iterative repository retrieval plus generation outperforms in-file baselines and vanilla retrieval-augmented completion on repo-level tasks [S16].
- Observations
  Code queries are heterogeneous: symbol names, stack traces, file paths, natural-language feature requests, and architectural questions each need different retrieval behavior. Pure vector search is weak for exact identifiers; pure keyword search is weak for analogical patterns; neither alone finds hidden dependencies well.
- Recommendations
  Route retrieval by query type. Use BM25/regex/path search for identifiers, error strings, filenames, and APIs; embeddings for natural-language intent and analogous implementations; graph expansion for definitions, references, callers, callees, tests, and configs. Contextualize chunks before embedding by prepending file/module/repo metadata and rerank before sending to the model.
- Impact on agent LLM = HIGH + rationale
  Retrieval quality determines whether the agent sees the real local pattern or a plausible but irrelevant snippet. For repo-scale coding, this is often the difference between a correct refactor and a generic rewrite [S10][S15][S16].

## F5. Long-running coding work needs explicit memory artifacts and context isolation.
- Evidence
  Anthropic’s long-running-agent guidance uses an initializer agent that creates `init.sh`, a progress log, and a structured feature list so later sessions can recover state quickly and work incrementally [S9]. Claude subagents are documented as separate context windows that keep investigative or log-heavy work out of the main thread [S8]. Claude best practices recommend aggressive context management, clearing unrelated sessions, and parallel sessions for separate workstreams [S6]. OpenAI’s Codex product docs say each cloud task runs in its own sandboxed environment with the repository preloaded, and the product is built for parallel work [S2][S5].
- Observations
  Existing code context is not only source files. Plans, progress logs, generated schema docs, and verification scripts are also context objects. Claude documents this pattern directly; for Codex, the implication is direct from its sandbox/task model and OpenAI’s harness engineering [S1][S2][S5][S9].
- Recommendations
  Check in lightweight execution plans, progress notes, and generated architecture references. Use separate subagents or parallel tasks for wide searches, log triage, or documentation synthesis. Handoff artifacts should answer: what changed, what remains, what commands verify the current state.
- Impact on agent LLM = HIGH + rationale
  Without explicit memory artifacts, each new session re-spends tokens rediscovering the repo and is more likely to repeat failed approaches or stop early [S6][S8][S9].

## F6. Too little context causes repo-wrong changes even when the code is locally plausible.
- Evidence
  Sourcegraph’s docs say context-aware prompts help the model align with the libraries, style, and specifics of the user’s codebase [S13]. Anthropic’s Claude Code best practices explicitly recommend pointing Claude to existing patterns and specific files rather than giving underspecified prompts [S6]. RepoCoder shows measurable gains from repository-level retrieval compared with in-file-only completion [S16].
- Observations
  The common failure mode from insufficient context is not random syntax failure; it is “looks fine, violates local conventions/contracts.” That includes reimplementing utilities, editing the wrong layer, missing hidden config, or writing tests that ignore the real harness.
- Recommendations
  Always give the agent one or two canonical examples from the same subsystem, plus the relevant test command and any architecture note for that area. Retrieve sibling tests, interfaces, and call sites together instead of single-file context only. For unfamiliar repos, ask the agent to map request flow and module ownership before editing.
- Impact on agent LLM = HIGH + rationale
  Most expensive agent mistakes in mature repos come from missing local conventions or cross-file contracts, not from inability to write syntax [S6][S13][S16].

## F7. Verification context is part of source-code context.
- Evidence
  Anthropic says giving Claude a way to verify its work is the single highest-leverage practice; tests, screenshots, and expected outputs materially improve performance [S6]. Anthropic’s long-running harness notes a recurring failure mode where the agent declared a feature complete without real end-to-end testing [S9]. OpenAI’s Codex launch says agents perform best when provided with configured development environments, reliable tests, and clear documentation [S2].
- Observations
  For coding agents, “relevant context” includes the commands and artifacts that define done-ness. An agent that finds the right files but lacks the right validation loop will still stop too early or make unsafe edits.
- Recommendations
  Put lint, typecheck, unit, integration, and screenshot/trace commands in root instructions or subsystem docs. Keep small reproducible commands near the code they validate. Prefer explicit acceptance checks over prose-only requirements.
- Impact on agent LLM = HIGH + rationale
  Verification closes the loop between retrieval and correctness. Without it, the agent can navigate accurately and still ship the wrong change [S2][S6][S9].

# Synthesis (1 paragraph)
The strongest pattern across Claude, Codex, and adjacent high-signal engineering work is to treat codebase context as a layered retrieval system, not a monolithic prompt: a short root instruction file acts as a map; deeper architecture and workflow docs live in the repo as source of truth; machine-generated repo maps and symbol graphs provide cheap global awareness; hybrid retrieval pulls the exact files, symbols, tests, and neighbors needed for the current task; and explicit memory plus verification artifacts keep long-running work coherent across sessions. The main failure modes are symmetric: too much context leads to salience loss, stale instructions, and long-context degradation, while too little context leads to generic but repo-wrong changes. For existing codebases, the practical target is not “maximum tokens” but “maximum task-relevant structure per token.”
