# Context Research — Other Forms of Context

## Scope

Synthesizes current (2024–2026) research and practitioner guidance on forms of context that materially affect AI coding agents (Claude Code, Codex, Cursor, Aider, Cline, Devin), excluding the dedicated tracks already covered (tech-docs, source code, comments, tests, dev-env). Categories surveyed: persistent agent memory, system prompts/personas, few-shot examples, conversation history & compaction, multi-agent context flow, external knowledge fetching, issue trackers / PR threads, IDE/editor state, git history, telemetry/observability, user-intent disambiguation, planning artifacts/scratchpads, tool catalogs.

Mix of empirical (peer-reviewed or vendor-benchmarked) and opinion (vendor docs, practitioner posts) is called out per finding.

## Sources

Primary documents fetched:

- Anthropic — *Building agents with the Claude Agent SDK* — https://claude.com/blog/building-agents-with-the-claude-agent-sdk
- Anthropic — *Effective context engineering for AI agents* — https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Anthropic — *The "think" tool: Enabling Claude to stop and think* — https://www.anthropic.com/engineering/claude-think-tool
- Anthropic / Claude Code Docs — *How Claude remembers your project* — https://code.claude.com/docs/en/memory
- LangChain — *Context Engineering for Agents* — https://www.langchain.com/blog/context-engineering-for-agents
- Cline Docs — *Cline Memory Bank* — https://docs.cline.bot/prompting/cline-memory-bank
- Aider — *Separating code reasoning and editing* — https://aider.chat/2024/09/26/architect.html
- Cognition — *Rebuilding Devin for Claude Sonnet 4.5* — https://cognition.ai/blog/devin-sonnet-4-5-lessons-and-challenges
- Packer et al., *MemGPT: Towards LLMs as Operating Systems*, arXiv:2310.08560 — https://arxiv.org/abs/2310.08560
- Liu et al., *Lost in the Middle: How Language Models Use Long Contexts*, TACL 2024 / arXiv:2307.03172
- Huang et al., *AgentCoder: Multi-Agent Code Generation with Effective Testing and Self-optimisation*, arXiv:2312.13010
- Hong et al., *MetaGPT: Meta Programming for a Multi-Agent Collaborative Framework*, arXiv:2308.00352
- Qian et al., *ChatDev: Communicative Agents for Software Development*, ACL 2024
- *Curiosity by Design: An LLM-based Coding Assistant Asking Clarification Questions*, arXiv:2507.21285
- Zheng et al., *When "A Helpful Assistant" Is Not Really Helpful: Personas in System Prompts Do Not Improve Performances of LLMs*, arXiv:2311.10054
- *ReSum: Unlocking Long-Horizon Search Intelligence via Context Summarization*, arXiv:2509.13313
- Cursor Docs / community — *.cursor/rules/* directory and Memory Bank framework
- Practitioner: HumanLayer — *Writing a good CLAUDE.md*; Sankalp — *Claude Code 2.0 guide*
- MCP / tool-overload analyses (Eclipsesource, Jenova, NebulaGG dev.to posts)
- Claude Cookbook — *SRE incident responder managed agent*

Secondary surveys consulted via web search but not deep-fetched (Sweep AI, GitHub Copilot Workspace, OpenCode, LSP-AI, AGENTS.md, Mem0, A-MEM).

## Findings

### F1 — CLAUDE.md is loaded as a user message, not a system prompt; size and specificity dominate adherence (category: memory)

**Evidence:** Claude Code Memory docs: "CLAUDE.md content is delivered as a user message after the system prompt, not as part of the system prompt itself. Claude reads it and tries to follow it, but there's no guarantee of strict compliance." Recommended size: "target under 200 lines per CLAUDE.md file. Longer files consume more context and reduce adherence." Hierarchy: managed policy → project (`./CLAUDE.md`) → user (`~/.claude/CLAUDE.md`) → local (`./CLAUDE.local.md`); files concatenated, ancestors loaded first.

**Observations:** This is empirical (vendor-stated). Many practitioners treat CLAUDE.md as a hard configuration file; it is not. Conflicts ("if two rules contradict, Claude may pick one arbitrarily") and dense paragraphs degrade adherence. Path-scoped rules under `.claude/rules/` with `paths:` frontmatter let large projects keep tokens spent only when relevant files are touched. CLAUDE.md survives `/compact` (re-read from disk); nested CLAUDE.md does not until subdir is re-touched.

**Recommendations:** Keep ≤200 lines, write specific verifiable instructions ("Use 2-space indentation", "Run `npm test` before committing"), prefer path-scoped rules over a single bloated file, audit periodically for contradictions, use `--append-system-prompt` only for truly system-level rules.

**Impact on agent LLM:** HIGH — directly shapes every turn; misuse silently dilutes the context budget for all sessions.

### F2 — Project memory bank pattern (Cline) externalizes durable state into structured markdown read every session (category: memory)

**Evidence:** Cline docs prescribe six core files — `projectbrief.md`, `productContext.md`, `activeContext.md`, `systemPatterns.md`, `techContext.md`, `progress.md` — read at the start of *every* task ("not optional"). `activeContext.md` "changes most frequently; update it after each session." Vendor-stated; widely cloned by Roo Code, Kilo, Cursor Memory Bank framework.

**Observations:** Opinion / convention, not benchmarked. Strength: forces the human and agent to maintain a durable, externally inspectable state; survives unbounded context growth. Weakness: read-every-session can be expensive; staleness manifests as confidently wrong agent behaviour ("memory drift"). Cursor's `.cursor/rules/` superseded the older single `.cursorrules` file because consolidation produced unmanageable bloat.

**Recommendations:** Use a small entry index file plus topic files loaded on demand (mirrors Claude Code auto-memory's `MEMORY.md` first-200-lines convention). Make staleness audit part of session lifecycle. Don't promote conversation transcripts into the memory bank — promote *decisions*.

**Impact on agent LLM:** HIGH for long-lived projects; MEDIUM for one-shot tasks where the load tax outweighs continuity.

### F3 — Auto-memory ("agent writes its own notes") is now first-class but its summaries are not trustworthy alone (category: memory)

**Evidence:** Claude Code auto-memory (v2.1.59+) writes `~/.claude/projects/<project>/memory/MEMORY.md` and topic files; first 200 lines / 25KB of `MEMORY.md` loaded each session. Cognition on Devin/Sonnet 4.5: "while we considered removing some of our own memory management and letting the model handle it, the model's summaries weren't comprehensive enough, sometimes paraphrasing tasks and leaving out important details, which resulted in performance degradation."

**Observations:** Empirical (Cognition production observation). Auto-memory is high-leverage when the human reviews it; dangerous when treated as ground truth. Cursor/Windsurf "memories" auto-saved from interactions show similar pitfall (LangChain blog cites unexpected location-data injection).

**Recommendations:** Keep auto-memory, but (a) audit it on a cadence, (b) never let agent-written summaries replace structured human-curated notes for safety-critical context, (c) prefer "decisions + invariants" entries over verbose narration.

**Impact on agent LLM:** MEDIUM — net positive when curated; net negative when blindly trusted.

### F4 — Hierarchical paged memory (MemGPT pattern) is the academic foundation for everything above (category: memory)

**Evidence:** Packer et al. 2023 (MemGPT) introduces "virtual context management … inspired by hierarchical memory systems in traditional operating systems," with main context (RAM) and external context (disk) plus interrupts. Evaluated on document analysis and multi-session chat, where it analyzes documents far exceeding the underlying model's window. A-MEM (2502.12110) and Mem0 (2504.19413) extend with associative / production-grade memory stores.

**Observations:** Empirical. The OS-paging analogy now underpins production agent harnesses (LangChain "context engineering" explicitly cites the RAM analogy). Research caveat: MemGPT's eval is conversational; coding-agent–specific benchmarks of paged memory are still thin.

**Recommendations:** Treat agent context as managed memory with explicit eviction; design "page-in" tools (Read-by-id, recall-by-tag) instead of dumping the world into the prompt.

**Impact on agent LLM:** MEDIUM — foundational influence; specific implementation choices vary.

### F5 — Personas in system prompts do *not* improve performance on objective coding/factual tasks (category: system-prompt)

**Evidence:** Zheng et al. (arXiv:2311.10054) — across 4 LLM families and 2,410 factual questions, "adding personas in system prompts does not improve model performance across a range of questions compared to the control setting where no persona is added." Domain-aligned roles produced small wins; selection of best persona was no better than random.

**Observations:** Empirical. Persona prompting helps subjective/style tasks (writing tone) but is irrelevant or net-negative for code correctness. Many CLAUDE.md / Cursor-rules files open with "You are a senior 10x engineer …" — that opening earns its tokens only if downstream behavioural rules depend on it.

**Recommendations:** Drop "you are an expert" preludes from coding-agent prompts. Spend tokens on concrete invariants (build, test, naming, lint, error-handling rules), not on identity.

**Impact on agent LLM:** LOW direct — except as wasted-token cost on every call (which compounds).

### F6 — System-prompt "altitude" matters: too brittle and you over-fit, too vague and you under-specify (category: system-prompt)

**Evidence:** Anthropic effective-context-engineering: "right altitude … specific enough to guide behavior effectively, yet flexible enough to provide the model with strong heuristics." Tools must be "self-contained, robust to error, and extremely clear with respect to their intended use" — and "if a human engineer can't definitively say which tool should be used in a given situation, an AI agent can't be expected to do better."

**Observations:** Vendor-stated, but consistent with repeated practitioner observation that 1000-line system prompts (Devin, early Cursor agent) underperform 200-line ones with sharp invariants. Aider's architect/editor split is a different cut at the same problem: have the architect prompt own *reasoning* and the editor prompt own *format*.

**Recommendations:** Periodically prune system prompts; run a deletion test ("does removing this rule change behaviour on our eval set?"); when a rule fires rarely, move to a skill/path-scoped rule.

**Impact on agent LLM:** HIGH — system prompt is the single most-amortized context surface.

### F7 — Few-shot exemplar diffs measurably improve code generation; example *quality* dominates quantity (category: examples)

**Evidence:** Survey work (PromptHub, Cognativ): few-shot prompting yields 15–40% accuracy improvement over zero-shot on code synthesis. ACM TOSEM 2025 (LLM-aware ICL for code): performance "heavily depends on the quality of demonstration examples"; random and pure-textual-similarity selection are sub-optimal vs. learned selectors. Anthropic guidance: "curate a set of diverse, canonical examples that effectively portray the expected behavior … examples are the 'pictures' worth a thousand words."

**Observations:** Empirical. Bigger isn't necessarily better — many-shot ICL (arXiv:2404.11018) helps on some tasks but eats context. For coding agents the actionable form is exemplar diffs / "good PR" snippets, not abstract prose rules.

**Recommendations:** Maintain 2–5 canonical-diff examples in the rules/skills directory for high-stakes patterns (error handling, public API change, schema migration). Refresh when conventions drift.

**Impact on agent LLM:** MEDIUM-HIGH for code style and idiom adherence; less so for novel logic.

### F8 — Compaction/summarization is required for long-horizon agents but introduces a fidelity floor (category: history)

**Evidence:** Anthropic / Claude Agent SDK: "compact feature automatically summarizes previous messages when the context limit approaches." Claude Code auto-compact triggers near 95% window. ReSum (arXiv:2509.13313): periodic structured summarization enables indefinite exploration in long-horizon search; experiments use up to 250-turn trajectories. Cognition on Devin: model's self-summaries "weren't comprehensive enough … resulted in performance degradation."

**Observations:** Empirical. Two distinct failure modes: (a) compaction drops a critical fact (a decision, invariant, or unresolved bug) and the agent silently regresses; (b) compaction happens too often, paying summarization cost on every turn. CLAUDE.md / rules survive compaction in Claude Code; conversation-only instructions do not.

**Recommendations:** Anything that must survive compaction belongs in a written artifact (CLAUDE.md, plan file, scratchpad), not chat. Prefer "structured note-taking" (Anthropic) — agent appends to a plan/notes file as it goes — over relying on auto-compact summaries.

**Impact on agent LLM:** HIGH — silent fidelity loss is the dominant failure mode of long agent runs.

### F9 — Lost-in-the-middle: position of relevant context inside the window predicts whether the model uses it (category: history)

**Evidence:** Liu et al. (TACL 2024, arXiv:2307.03172) — U-shaped accuracy curve on multi-doc QA and key-value retrieval; performance highest at the start and end of context, "significantly degrades when models must access relevant information in the middle of long contexts, even for explicitly long-context models."

**Observations:** Empirical, replicated. Practical implication for coding agents: dumping a 50-file codebase tour into the middle of the window is wasted; the same content placed at the boundaries (or fetched JIT via tools) is retrieved more reliably. Newer long-context models (Sonnet 4.5, Gemini 2.5) reduce but do not eliminate the effect.

**Recommendations:** Front-load invariants (rules, conventions). End-load the actual task. Avoid stuffing speculative context in the middle. Prefer agentic search (read by name) over RAG-dumps when possible.

**Impact on agent LLM:** HIGH at long contexts; LOW under ~32k tokens.

### F10 — Multi-agent / sub-agent architectures isolate context and improve per-task signal (category: multi-agent)

**Evidence:** Anthropic Claude Agent SDK: "subagents use their own isolated context windows, and only send relevant information back to the orchestrator … ideal for tasks that require sifting through large amounts of information where most of it won't be useful." Anthropic multi-agent researcher: subagents "in parallel with their own context windows, exploring different aspects of the question simultaneously." LangChain context-engineering: "many agents with isolated contexts outperformed single-agent."

**Observations:** Empirical (Anthropic-published) but with caveats — Cognition explicitly cautions on subagent delegation: their early Devin experiments found that handing off to subagents loses tacit context the orchestrator had ("improved judgment about when to externalize state" is something they're still tuning). MetaGPT and ChatDev show structured-document handoff (vs. free-form chat) reduces irrelevant content; AgentCoder's three-agent split (programmer / test designer / test executor) cuts token cost ~3x vs MetaGPT/ChatDev (56.9K vs 138.2K on HumanEval) while improving Pass@1.

**Recommendations:** Use subagents for *fan-out search and review*, not for sequential implementation steps that share thick context. Hand off via structured artifacts (plan files, JSON results), not transcripts. Be skeptical of frameworks that claim subagent delegation universally helps.

**Impact on agent LLM:** HIGH for parallelizable read-heavy tasks (multi-file search, parallel review); MEDIUM-LOW for tightly coupled implementation work.

### F11 — Architect/editor split (two-model role decomposition) gives a measurable benchmark lift (category: multi-agent)

**Evidence:** Aider Sept 2024 post: pairing a strong reasoning model (architect) with an editor model that produces well-formed diffs improved Aider code-edit benchmark from 77.4% (Sonnet solo) to 80.5% (Sonnet+Sonnet); o1-preview + DeepSeek hit 85% SOTA. Empirical on a public benchmark.

**Observations:** Confirms that *output-format pressure* and *reasoning pressure* compete for capacity; separating them helps. Generalizes to: have one prompt own "what to do," another own "how to write it" (e.g., commit message, diff, JSON tool call).

**Recommendations:** Worth piloting whenever the producing model struggles with strict output formats (diff syntax, JSON schemas, OpenAPI). Less useful when format is trivial.

**Impact on agent LLM:** MEDIUM — real but bounded gains; cost is doubled inference latency.

### F12 — Tool-catalog overload degrades selection accuracy non-linearly past ~5–10 tools (category: tool-catalog)

**Evidence:** Multiple practitioner analyses (Eclipsesource Jan 2026, Jenova, NebulaGG, PromptForward) converge: 5–7 tools is the practical upper limit for consistent accuracy without dynamic filtering; beyond that, attention dilution + tool collision + context-window cost compound. LangChain context-engineering blog: RAG-over-tool-descriptions improves selection accuracy "roughly threefold" when tool count is high. Anthropic: if a human can't disambiguate which tool to use, the model can't either.

**Observations:** Mostly opinion/practitioner — limited public benchmarks. But the mechanism (semantic blur between similar tools, parameter cross-contamination) is well-described and reproducible. MCP servers with dozens of overlapping tools (e.g., one-tool-per-API-endpoint server design) are the canonical anti-pattern.

**Recommendations:** Cap exposed tool count. Prefer few high-coverage tools (one `bash`, one `grep`, one structured-edit tool) over many narrow ones. When a workflow needs many domain tools, gate them behind a router/dispatcher and keep only the dispatcher in the top-level catalog.

**Impact on agent LLM:** HIGH — easily the biggest hidden tax in maximalist MCP setups.

### F13 — Scratchpads / "think" steps are first-class context, not decoration (category: planning)

**Evidence:** Anthropic "think" tool (production blogpost): on τ-bench airline domain, baseline 0.370 → 0.570 with think-tool + optimized prompting (a 54% relative improvement); SWE-bench isolated effect +1.6%. Anthropic context-engineering: "structured note-taking … track progress across complex tasks, maintaining critical context and dependencies." Cognition Devin: Sonnet 4.5 "more proactive about writing and executing short scripts and tests to create feedback loops" — improves long-task reliability.

**Observations:** Empirical. Two distinct mechanisms: (a) in-response scratchpad ("think" tool) for policy-heavy / sequential decisions before next tool call; (b) on-disk plan/TODO/scratchpad file (`PLAN.md`, `TODO.md`) for cross-turn persistence. Both compose with compaction (the file survives, the chat doesn't).

**Recommendations:** For any task >5 tool calls, instruct the agent to write a plan file before acting and update it as it goes. Don't rely on "let it think harder" — give it a place to think.

**Impact on agent LLM:** HIGH on multi-step / policy-heavy tasks; LOW on single-shot tasks.

### F14 — Clarifying questions before generation produce statistically large improvements in code quality (category: clarification)

**Evidence:** *Curiosity by Design* (arXiv:2507.21285) — user study: precision/focus improved in 82% of cases (mean 4.4/5), contextual alignment 78%, usefulness 80%, correctness 66% (median 4.3/5). All differences p<0.001, Cohen's d > 1.2 ("very large effect size"). Empirical.

**Observations:** Strong empirical support that the dominant failure mode of coding agents is acting on under-specified intent. But the practitioner ergonomics are bad — users dislike interruption. Devin's "Interactive Planning" mitigates by front-loading the clarification round before autonomous work begins.

**Recommendations:** Ask once, up-front, with a concrete proposal-and-confirm pattern ("I plan to do X; should Y or Z?"). Don't ask piecemeal mid-task. Build clarification into the planning artifact, not into chat back-and-forth.

**Impact on agent LLM:** HIGH — under-specification is the root cause of much "agent did the wrong thing" wreckage.

### F15 — IDE/editor state (cursor, selection, diagnostics, LSP) is materially higher-signal than raw text search (category: ide)

**Evidence:** Practitioner reports (LSP-AI, the/experts blog): "Finding all call sites of a function takes approximately 50ms with LSP compared to 45 seconds with traditional text search, which is a 900x improvement." Cursor 2.0 changelog and Kiro pitch lean heavily on real-time diagnostics as agent input. LSP-AI / lsp-mcp servers feed go-to-def, hover, references, and live diagnostics directly into Claude/OpenCode/Windsurf.

**Observations:** Mostly opinion + vendor benchmarks. Strong signal that LSP-derived context gives AST-level accuracy where grep-derived context is brittle. Diagnostics-as-feedback (compile errors, type errors) close the loop better than test failures alone for fast iteration.

**Recommendations:** When operating inside an IDE/editor harness, prefer LSP-driven discovery over text search. Feed live diagnostics to the agent automatically after each edit. For headless agents (Claude Code CLI), add an LSP-MCP bridge instead of relying on grep.

**Impact on agent LLM:** HIGH inside IDEs; MEDIUM in CLI agents that lack LSP integration.

### F16 — Git history / blame as targeted context improves repair accuracy (category: git)

**Evidence:** HAFixAgent (arXiv:2511.01047 referenced in search): "injecting historical context derived from git blame commits (the last change touching a buggy line) can improve LLM repair performance." Practitioner tools (Selvedge, git-ai) explicitly capture intent at change time precisely because diffs alone don't carry the *why*.

**Observations:** Empirical (HAFixAgent) but limited to repair tasks. Practical: blame surfaces *why* a line exists; commit messages on the touching commit are often more decision-relevant than surrounding code comments. Risk: dumping full blame for a 1000-line file is just noise — must be targeted to the line/range under question.

**Recommendations:** Expose `git blame -L`, `git log -p -- <file>`, and `git log <commit>` as agent tools rather than pre-loading. Use only when the task is "why is this here / why did this change."

**Impact on agent LLM:** MEDIUM — high signal when relevant, often irrelevant otherwise.

### F17 — Issue/PR thread context anchors the *what* and *why* of work (category: issues)

**Evidence:** Sweep AI design (issue → PR pipeline): operates as GitHub App with native access to issues/comments/PRs and uses semantic indexing to find related code from issue text. GitHub Copilot Coding Agent and OpenCode follow similar patterns — issue body + comments + linked PRs are the seed context. Largely opinion / product description rather than benchmarked.

**Observations:** Issue threads are uniquely valuable because they often contain the user-facing problem statement, reproduction steps, debate over approaches, and acceptance criteria — all of which are missing from code. Risk: long issue threads carry off-topic discussion; raw paste pollutes context.

**Recommendations:** Summarize issue threads down to {problem, repro, accept-criteria, decisions, open questions} before injecting. Link to the issue ID rather than embedding the full thread for follow-on agent runs.

**Impact on agent LLM:** HIGH for issue-driven tasks (bug-fix-from-ticket); LOW for greenfield work.

### F18 — Production telemetry (logs, traces, stack traces) feeds AI SRE-style debugging agents and shortens MTTR (category: telemetry)

**Evidence:** Datadog Bits AI SRE (vendor claim): up to 95% reduction in time-to-resolution. Anthropic SRE-incident-responder cookbook recipe is an officially supported pattern. AWS DevOps Agent, incident.io AI SRE, New Relic SRE Agent all converge on the same shape: agent receives alert + correlated logs/traces/metrics + recent deploy diff + similar past incidents.

**Observations:** Mostly vendor claims + practitioner consensus, not peer-reviewed. Mechanism is sound — the same hypothesis-test loop a human SRE runs, automated. Critical safety constraint: read-only by default, with human approval for remediation.

**Recommendations:** For coding agents that fix production-discovered bugs, feed the actual log/trace/stack rather than re-deriving from a ticket summary. Ensure the agent can fetch related deploy diffs (git log between two SHAs in production timeline).

**Impact on agent LLM:** HIGH for incident/debugging tasks; LOW for feature work.

### F19 — External documentation fetching (web search / docs MCP / llms.txt) is now expected agent capability (category: external-knowledge)

**Evidence:** Anthropic context engineering recommends "just in time" retrieval — keep "lightweight identifiers (file paths, stored queries, web links, etc.)" and dynamically load. MCP `docs` servers (Moov, Stripe, others) and `llms.txt` convention let agents fetch authoritative current docs rather than rely on stale training. Cursor / Continue.dev expose `@docs` for the same reason.

**Observations:** Mix of vendor convention and emergent standard. Strength: solves training-cutoff staleness and version-mismatch hallucination. Weakness: doc fetches are slow, expensive in tokens, and rarely cached well; over-eager fetching balloons cost.

**Recommendations:** Make external doc fetching a discretionary tool, not auto-RAG. Cache aggressively. Prefer authoritative `llms.txt` / vendor MCP over arbitrary web search where available. For library-API tasks, fetching the *exact version's* docs beats general web search.

**Impact on agent LLM:** MEDIUM — high when version-pinning matters; LOW for tasks the model already knows well.

### F20 — Filesystem/folder layout itself is a context-engineering surface (category: planning)

**Evidence:** Anthropic Claude Agent SDK blog: "the folder and file structure of an agent becomes a form of context engineering," allowing selective loading via `grep`/`tail`/`find` rather than pre-loading. Reinforced by `.claude/rules/` path-scoped loading, AGENTS.md/CLAUDE.md hierarchy, and the Cline memory bank's enforced structure.

**Observations:** Vendor opinion, but consistent with the lost-in-the-middle finding (F9): better to keep most context out and let the agent retrieve precise slices. Encourages designs like one-skill-per-file, one-rule-per-topic, plan files at known paths.

**Recommendations:** Treat the repo's `.claude/`, `docs/`, `dev/`, and skill directories as the agent's filing cabinet. Prefer many small named files over a few large catch-alls. Make the skill/rule index itself short enough to keep in working memory.

**Impact on agent LLM:** MEDIUM — second-order effect, but compounds across sessions.

## Synthesis

Across categories, three structural lessons recur:

1. **Context is finite, expensive, and adversarial.** Anthropic states it explicitly ("finite resource with diminishing marginal returns"); lost-in-the-middle (F9), tool overload (F12), and compaction-fidelity loss (F8) all manifest the same scarcity. Every default in modern coding-agent harnesses (CLAUDE.md size limits, path-scoped rules, subagent isolation, "just-in-time" retrieval, auto-compact) is a response to this scarcity.

2. **Externalize state into named artifacts that survive turns.** CLAUDE.md (F1), memory banks (F2), auto-memory (F3), MemGPT-style paged memory (F4), scratchpads/plan files (F13), and filesystem-as-context (F20) all instantiate the same pattern: durable, file-shaped, human-inspectable memory beats chat history. Compaction (F8) and lost-in-the-middle (F9) explain *why* — anything that lives only in chat is lossy.

3. **Specialize via decomposition, but pay the seams cost.** Subagent isolation (F10), architect/editor splits (F11), and three-agent test loops (AgentCoder; F10) all yield real gains for fan-out and format-strict tasks. But Cognition's hard-won lesson on Devin is that handoffs lose tacit state, and structured artifacts (plan files, JSON results) outperform free-form transcripts at the seam.

**Empirical-vs-opinion calibration.** Strongest empirical support: lost-in-the-middle (F9), clarifying questions (F14), few-shot ICL (F7), persona null-result (F5), think-tool benchmark (F13), Aider architect/editor benchmark (F11). Mostly opinion / vendor claim: tool overload thresholds (F12), memory bank structure (F2), telemetry MTTR claims (F18), filesystem-as-context (F20). Mixed: compaction (F8 — has both empirical ReSum and Cognition production observation), subagents (F10 — Anthropic-published evidence + Cognition cautionary tale).

**For a coding-agent stack design** (e.g., fathomdb's 0.6.0 agent surfaces), the load-bearing decisions in priority order: (a) keep CLAUDE.md ≤200 lines, specific, no persona fluff; (b) put durable state in files (plan, decisions, invariants) — never in chat; (c) cap tool catalog hard; (d) make the agent ask once, up-front, before autonomous runs; (e) use subagents only for fan-out search/review, not for shared-context implementation; (f) front-load invariants and end-load the task; (g) expose LSP/blame/issue-thread/telemetry as discretionary tools, not pre-loaded context.
