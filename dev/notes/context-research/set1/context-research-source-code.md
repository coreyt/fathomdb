# Context Research — Existing Source Code

## Scope

How should existing source code be supplied as context to AI coding agents
(Claude Code, Codex, Cursor, Aider, Cline, Devin, Sourcegraph Cody, etc.)?
Covers: whole-repo dump vs retrieval vs agentic search; repo maps / call
graphs; embedding RAG vs symbol-graph RAG vs agentic file-reading;
long-context (1M tok) failure modes (lost-in-the-middle, context rot);
chunking strategies (function/class/file/tree-sitter/AST); diff vs whole-file
edit context; transitive symbol resolution. Synthesized from 8 web searches
and 8 primary-source fetches (2026-05-01).

## Sources

- [Building a better repository map with tree sitter — aider blog](https://aider.chat/2023/10/22/repomap.html) — Paul Gauthier on tree-sitter repo-map; default 1k-token budget.
- [Repository map | aider docs](https://aider.chat/docs/repomap.html) — current docs for repo map operation.
- [Unified diffs make GPT-4 Turbo 3X less lazy — aider](https://aider.chat/docs/unified-diffs.html) — empirical edit-format ablation, 20% to 61%.
- [Claude Code Doesn't Index Your Codebase — Vadim's blog](https://vadim.blog/claude-code-no-indexing) — Cherny + Anthropic engineer quotes on agentic search vs RAG.
- [RepoCoder — arXiv 2303.12570](https://arxiv.org/abs/2303.12570) — iterative retrieval-generation, EMNLP 2023, +10% over in-file baseline.
- [CrossCodeEval — NeurIPS 2023 / arXiv 2310.11248](https://crosscodeeval.github.io/) — cross-file context required; even best retriever <20 EM.
- [Agentless — arXiv 2407.01489](https://arxiv.org/abs/2407.01489) — hierarchical file→class→line localization, 32% SWE-bench Lite at $0.70.
- [AutoCodeRover — arXiv 2404.05427](https://arxiv.org/html/arXiv:2404.05427) — AST-aware `search_class`/`search_method` API stack.
- [SWE-agent — arXiv 2405.15793](https://arxiv.org/pdf/2405.15793) — agent-computer-interface design; BM25 + bounded view ops.
- [cAST — arXiv 2506.15655](https://arxiv.org/html/2506.15655v1) — AST chunking ablation, +2.6 pp Pass@1 on SWE-Bench Claude.
- [Context Rot — Chroma Research](https://www.trychroma.com/research/context-rot) — 18-LLM evaluation; perf degrades non-uniformly with input length.
- [Lost in the Middle — TACL 2024](https://direct.mit.edu/tacl/article/doi/10.1162/tacl_a_00638/119630/Lost-in-the-Middle-How-Language-Models-Use-Long) — primacy/recency bias on retrieval position.
- [Gemini 1.5 Tech Report — arXiv 2403.05530](https://arxiv.org/pdf/2403.05530) — >99.7% NIAH recall to 1M tokens; >99% to 10M.
- [Cursor — Codebase Indexing docs](https://cursor.com/docs/context/codebase-indexing) — meaningful-chunk embeddings, 5-min resync, no plaintext storage.
- [How Cody understands your codebase — Sourcegraph](https://sourcegraph.com/blog/how-cody-understands-your-codebase) — moved off embeddings to BM25 + code-graph signals.
- [Agentic Context Fetching — Sourcegraph docs](https://sourcegraph.com/docs/cody/capabilities/agentic-context-fetching) — agent-driven context reflection loop.
- [Continue.dev — Codebase Retrieval](https://continue.dev/docs/walkthroughs/codebase-embeddings) — local all-MiniLM-L6-v2 default; retrieve-25 then LLM-rerank-5.
- [Devin 2.0 — Cognition](https://cognition.ai/blog/devin-2) — proactive codebase exploration, interactive plan, Devin Wiki re-index every few hours.

## Findings

### F1 — Frontier coding agents are abandoning embedding RAG for agentic search

**Evidence:**
> "Early versions of Claude Code used RAG + a local vector db, but we found pretty quickly that agentic search generally works better." — Boris Cherny (Anthropic), quoted in <https://vadim.blog/claude-code-no-indexing>
>
> "Claude Code doesn't use RAG currently. In our testing we found that agentic search outperformed [it] by a lot, and this was surprising." — Anthropic engineer, same source.
>
> Sourcegraph Cody similarly retired its `text-embedding-ada-002` pipeline because "code had to be sent to third parties... required complex vector database management... scaling to 100,000+ repositories proved resource-intensive." — <https://sourcegraph.com/blog/how-cody-understands-your-codebase>

**Observations:**

- Two independent frontier teams (Anthropic, Sourcegraph) report agentic search ≥ embedding RAG on real code tasks, citing not just quality but staleness, security, privacy, and operational simplicity.
- Claude Code's actual primitives are tiny: `Glob` (path patterns), `Grep` (regex), `Read` (file slice) — plus an `Explore` sub-agent on Haiku for cheap fan-out.
- This is opinion-backed-by-internal-eval, not a published ablation. The Amazon Science 2026 paper cited in secondary commentary (90% of RAG quality with keyword-only tool use) is the closest public empirical anchor.

**Recommendations:**

- Default a code agent to grep/glob/read tools over a vendored vector DB until a measured gap forces otherwise.
- Treat embeddings as an *optional* booster (e.g. for natural-language-only queries against unfamiliar repos), not a required substrate.
- Provide a sub-agent context for exploration so search churn does not pollute the planner's token budget.

**Impact on agent LLM:** HIGH — this directly shapes the tool surface and whether you ship an indexer at all.

### F2 — Iterative retrieve→generate→re-retrieve materially beats single-shot retrieval

**Evidence:**
> RepoCoder "significantly improves the In-File completion baseline by over 10% in all settings and consistently outperforms the vanilla retrieval-augmented code completion approach." — <https://arxiv.org/abs/2303.12570>
>
> Sourcegraph agentic chat: "proactively gathers context from your codebase, shell, and the web... reviewing, and refining context to deliver high-quality, context-rich responses." — <https://sourcegraph.com/docs/cody/capabilities/agentic-context-fetching>

**Observations:**

- RepoCoder is a controlled ablation: the same retriever + LM with iteration beats the same components without iteration. The first-pass generation acts as a second query, surfacing call sites and helpers a literal-prompt query misses.
- Modern agent loops (Claude Code, Cody agentic chat, Devin's planner) effectively bake this iteration into the control flow — the agent reads a file, learns a symbol name, then greps for it.
- Empirical (RepoCoder) and architectural (Cody/Claude Code) evidence agree.

**Recommendations:**

- Wire context fetching as a *loop* the agent can re-enter, not a one-shot prefix.
- Allow the agent to spend tokens on a second/third pass — this is cheap relative to wrong code.
- Log retrieval queries; the second query (post-generation) is a strong signal of what was missing.

**Impact on agent LLM:** HIGH — directly affects code-quality on real repository tasks.

### F3 — Cross-file context is required for repo-realistic completion, and even good retrieval is far from sufficient

**Evidence:**
> "Performance improves dramatically when the cross-file context is added to the prompts, regardless of the size of code LMs... OpenAI's ada embedding generally performs the best, but its downstream generation accuracy is still suboptimal (<20 EM)." — CrossCodeEval, <https://crosscodeeval.github.io/>
>
> "Unlike existing datasets where the correct answer could be predicted with only context from the current file, CrossCodeEval strictly requires cross-file context to correctly complete the missing code." — same.

**Observations:**

- Empirical ablation (NeurIPS 2023, 10k examples / 1k repos / 4 languages): in-file context is a ceiling well below what real software engineering needs.
- Even the best off-the-shelf retriever caps generation accuracy below 20% exact-match — retrieval quality, not just LM scale, is the bottleneck.
- This is the strongest empirical case against "just paste the open file" UX.

**Recommendations:**

- Context strategy for code agents must include a cross-file mechanism (repo map, symbol search, or retrieval) — relying on the active file is a known regression.
- Track *retrieval* quality independently of *generation* quality; CCE shows they decouple sharply.

**Impact on agent LLM:** HIGH — bounds the achievable quality of single-file-context systems.

### F4 — Long-context models do well on synthetic NIAH but degrade non-uniformly on real workloads

**Evidence:**
> Gemini 1.5 Pro: ">99.7% recall... up to 1 million tokens... extended to 10 million tokens for text" on NIAH. — <https://arxiv.org/pdf/2403.05530>
>
> Chroma's Context Rot study (18 LLMs incl. GPT-4.1, Claude 4, Gemini 2.5): "model performance varies significantly as input length changes, even on simple tasks... Even single distractors reduce performance compared to baseline... Counterintuitively, models performed better on shuffled, unstructured haystacks versus logically coherent ones." — <https://www.trychroma.com/research/context-rot>
>
> Lost-in-the-Middle (TACL): "performance is highest when relevant information occurs at the very start or end of the context, and rapidly degrades when models must reason over information in the middle." — <https://nelson-liu.github.io/lost-in-the-middle> (paper)

**Observations:**

- The headline NIAH numbers describe a single-needle factoid retrieval, not multi-hop code reasoning.
- Chroma's finding that *coherent* haystacks are *harder* than shuffled ones is striking for code: dumping a real repo (where related symbols cluster) may be worse than the synthetic benchmark suggests.
- Distractor sensitivity is highly relevant to repos full of similarly-named symbols.

**Recommendations:**

- Do not assume 1M-token context lets you dump the repo. Curate.
- Place the most decision-relevant material (the file you'll edit, the failing test, the called helper) at the *start* or *end* of the prompt, not the middle.
- Prune distractors aggressively — even one near-duplicate hurts.

**Impact on agent LLM:** HIGH — bounds the "just paste the whole repo" strategy that 1M-token marketing implies.

### F5 — AST/tree-sitter chunking measurably outperforms line-based chunking for code RAG

**Evidence:**
> cAST: "RepoEval Recall@5 improved by 1.8–4.3 points... Pass@1: StarCoder2-7B 51.7% with cAST vs. 47.5% with fixed-size chunking. SWE-Bench Pass@1 with Claude: 16.3% (cAST) vs. 13.7% (baseline). CrossCodeEval: up to 2.9-point improvement." — <https://arxiv.org/html/2506.15655v1>
>
> Aider repo map: "Using the AST, we can identify where functions, classes, variables, types and other definitions occur in the source code... The map is richer, showing full function call signatures." — <https://aider.chat/2023/10/22/repomap.html>
>
> cAST design principles: "syntactic integrity, high information density, language invariance" — measure chunk size by "non-whitespace characters rather than by lines."

**Observations:**

- This is one of the cleanest controlled ablations in the literature: same retriever, same LM, swap chunker → +2.6pp Pass@1 on SWE-Bench Claude.
- Effect size compounds: better recall feeds better generation.
- Aider's repo-map uses the same primitive (tree-sitter AST) but for a different purpose — surfacing signatures into the prompt rather than for chunked retrieval.

**Recommendations:**

- If embeddings are used, chunk on AST boundaries (function/class), not line windows.
- Char-count, not line-count, for chunk-size budget.
- Where retrieval is replaced by agentic search, still leverage AST-level signature extraction (à la Aider's repo-map) to give the planner a cheap whole-repo skeleton.

**Impact on agent LLM:** MEDIUM — meaningful Pass@1 lift, but only matters if a retrieval/index path exists at all.

### F6 — Repo map of signatures is a strong cheap prefix for any code agent

**Evidence:**
> Aider: "Aider uses a concise map of the whole git repository that includes the most important classes and functions along with their types and call signatures... Aider optimizes the repo map by selecting the most important parts of the codebase which will fit into the token budget assigned by the user (via the `--map-tokens` switch, which defaults to 1k tokens)." — <https://aider.chat/2023/10/22/repomap.html> and <https://aider.chat/docs/repomap.html>
>
> Devin Wiki: "automatically indexes repositories every few hours, producing browsable documentation and architecture diagrams that link directly to relevant parts of the code." — <https://cognition.ai/blog/devin-2>

**Observations:**

- A signatures-only map is small (default 1k tokens) but conveys the symbol space the agent should grep into. It is *complementary* to agentic search, not competitive with it.
- Aider's PageRank-style ranking of which symbols to include is a notable detail — relevance, not arbitrary truncation.
- Devin treats the same idea at a higher level: a continuously-regenerated wiki/architecture overview.

**Recommendations:**

- Pre-compute a repo signature map once per session and put it in the system prompt or first user turn — dirt cheap insurance against the agent not knowing what exists.
- Refresh on file edits; staleness is the main failure mode.

**Impact on agent LLM:** MEDIUM-HIGH — small token cost, large effect on the agent's "I don't know what to grep for" failure mode.

### F7 — Hierarchical localization (file → class → line) beats flat retrieval at SWE-bench economics

**Evidence:**
> Agentless: "hierarchical process to first localize the fault to specific files, then to relevant classes or functions, and finally to fine-grained edit locations... Results on the popular SWE-bench Lite benchmark show that surprisingly the simplistic Agentless is able to achieve both the highest performance (32.00%, 96 correct fixes) and low cost ($0.70)." — <https://arxiv.org/abs/2407.01489>
>
> AutoCodeRover seven-API surface: `search_class`, `search_class_in_file`, `search_method`, `search_method_in_class`, `search_method_in_file`, `search_code`, `search_code_in_file`. SWE-bench Pass@1 19% at $0.43/task. — <https://arxiv.org/html/arXiv:2404.05427>

**Observations:**

- Agentless and ACR converge on the same shape: structured navigation through the symbol graph rather than fuzzy semantic similarity.
- Cost is a first-order concern. A simple, tightly-scoped pipeline ($0.70/task, 32% resolve) outperformed many heavier agents at the time of publication.
- This is empirical: published ablations on the standard SWE-bench harness.

**Recommendations:**

- Expose an AST-level search tool (find class, find method, find usages) to the agent; do not rely on grep alone for symbol-level navigation in typed languages.
- Stage retrieval — don't ask the LM to localize a 5-line edit in one shot from a 200-file repo.
- Budget for re-localization; the localizer is allowed to be wrong on first pass.

**Impact on agent LLM:** HIGH — this is the dominant pattern at the top of SWE-bench-Lite.

### F8 — Diffs, not full-file rewrites, are the right edit format

**Evidence:**
> Aider: "GPT-4 Turbo (gpt-4-1106-preview): Baseline with SEARCH/REPLACE format: 20% score; with unified diffs: 61% score. Lazy comments reduced from 12 tasks to 4 tasks (3X improvement)." — <https://aider.chat/docs/unified-diffs.html>
>
> "Experiments without 'high level diff' prompting produce a 30-50% increase in editing errors. Experiments where flexible patching is disabled show a 9X increase in editing errors." — same.

**Observations:**

- This is one of the highest-effect-size ablations in the public literature on code agents: 3× pass rate from edit format alone, holding model and prompt constant.
- The mechanism is dual: token efficiency (cheap output) *and* a quality boost from forcing structured local reasoning.
- Caveat: input context still typically needs whole-function or whole-class views even when output is a diff.

**Recommendations:**

- Default tool-use shape: read whole-file (or whole-function) context in, emit unified diffs out.
- Implement *flexible* patching (fuzzy whitespace, indentation tolerance) — strict patch matching costs 9× error rate.
- Use "high-level diff" prompting (describe the change before the diff) to keep the model from getting lazy.

**Impact on agent LLM:** HIGH — directly governs both cost and reliability of edits.

### F9 — Bounded "agent-computer interface" view operations matter more than raw context window

**Evidence:**
> SWE-agent: "By restricting search output length and providing only relevant file fragments, the ACI reduces the likelihood of prompt overflow and hallucination caused by excessive or irrelevant context... Important operations (e.g., file navigation, editing) should be consolidated into as few actions as possible." — <https://arxiv.org/pdf/2405.15793>
>
> SWE-Edit decomposition: "a Viewer that extracts task-relevant code on demand, and an Editor that executes modifications from high-level plans." — <https://arxiv.org/abs/2604.26102>

**Observations:**

- SWE-agent's central thesis is that LM performance on code is a function of the *interface*, not just the model. Pagination of file views, capped grep output, and a syntax-aware editor materially improve outcomes without any model change.
- Cline/Cursor design rationale matches: bounded read/scroll/edit primitives, not "here's a 500k-token blob."
- Combines well with F4 (context rot) — the ACI is the mechanism that keeps long-context degradation from accumulating.

**Recommendations:**

- Cap tool-output sizes (e.g. grep returns top-N matches with line numbers; reads return a window with explicit "more available" signal).
- Provide explicit navigation operators (scroll, jump-to-symbol) rather than dumping entire files.
- Treat the ACI as a tunable artifact with its own evals; small UI tweaks (line numbers in reads, line ranges in writes) compound.

**Impact on agent LLM:** MEDIUM-HIGH — affects every multi-turn code task and stacks with F4.

### F10 — Retrieval is necessary for unfamiliar/large repos, but the dominant trend is "search-as-tool" not "embed-everything"

**Evidence:**
> Continue.dev: "Continue indexes your codebase so that it can later automatically pull in the most relevant context... a combination of embeddings-based retrieval and keyword search... initially retrieves 25 results from the vector database and then uses an LLM to select the top 5 results through re-ranking." — <https://continue.dev/docs/walkthroughs/codebase-embeddings>
>
> Cursor: "Cursor breaks your code into meaningful chunks (functions, classes, logical blocks)... index stays current through automatic sync every 5 minutes, processing only changed files... Code content is never stored in plaintext." — <https://cursor.com/docs/context/codebase-indexing>
>
> Cody (post-pivot): "an adapted form of the BM25 ranking function alongside other signals" — abandoned `text-embedding-ada-002`. — <https://sourcegraph.com/blog/how-cody-understands-your-codebase>

**Observations:**

- IDE-embedded tools (Cursor, Continue) keep an index because they need sub-second retrieval into a chat sidebar; agent CLIs (Claude Code, Codex CLI) skip indexing because they can afford the agent loop.
- Even tools that keep an index increasingly add a re-ranker (LLM-based) on top — embeddings alone aren't trusted.
- Cody's pivot away from embeddings to BM25-plus-signals shows the trend even for indexed systems.

**Recommendations:**

- If your form factor is in-IDE chat with sub-second turnaround → index, AST-chunk, embed + rerank.
- If your form factor is an agent loop (CLI, background runner) → grep/glob/read with a repo-map skeleton; skip the vector DB.
- Always implement an LLM rerank step over top-N retrieval; never feed top-K directly to the generator.

**Impact on agent LLM:** MEDIUM — selects between two viable architectures; either can work with right surrounding hygiene.

## Synthesis

A clear architectural consensus has emerged across frontier-lab agents and academic SWE-bench leaders:

1. **Default to agentic search over embedding RAG for code.** Anthropic and Sourcegraph have both publicly pivoted off vector DBs as primary mechanism. The empirical anchor is Amazon Science (cited in commentary on Claude Code) and the architectural anchor is Cody's BM25 pivot. Embedding RAG is now an optional booster, not the substrate. (F1, F10)

2. **Iterate the retrieval loop.** RepoCoder's >+10pp gain from iteration generalizes to real agent loops: read-then-grep-again is structurally equivalent to RepoCoder's iterative step and is a major source of quality. (F2)

3. **Cross-file context is non-negotiable.** CrossCodeEval is the empirical floor: in-file-only is a known ceiling. Either repo-map signatures, symbol-search tools, or retrieval — pick one or more. (F3, F6)

4. **Long context is a tool, not a strategy.** NIAH numbers are misleading; Context-Rot and Lost-in-the-Middle both show degradation under realistic distractor density and middle-position relevance. Curate to the start/end of the prompt; do not trust 1M tokens to do the work. (F4)

5. **Chunking matters when chunking is in play.** AST/tree-sitter chunking gives a clean +2–3pp on SWE-Bench Pass@1 over line chunks. If you embed, you should AST-chunk. (F5)

6. **Hierarchical, structured navigation beats flat search at top SWE-bench scores.** Agentless and AutoCodeRover both expose `find_class`/`find_method`/`find_usages`-shaped tools and stage localization (file → class → line). (F7)

7. **Diffs in, with whole-function context.** Unified-diff edit format triples Aider's edit-pass rate at fixed model. Read whole functions/classes; emit diffs. (F8)

8. **The agent-computer interface is its own design surface.** Bounded view operators (paginated reads, top-N grep, jump-to-symbol) prevent long-context degradation from compounding across turns and matter more than raw window size. (F9)

**Concrete starter stack for a new code agent (CLI form factor):**

- `Glob` + `Grep` + `Read(path, line_range)` as primary tools.
- Tree-sitter-derived signature map (≤2k tokens) injected on first turn; refreshed on file writes.
- AST-aware `find_class` / `find_method` / `find_usages` as first-class tools (matches AutoCodeRover/Agentless surface).
- Sub-agent for exploration (Haiku-class model) so heavy search does not pollute the planner's window.
- Edit interface: read full function, write unified diff with flexible patch (whitespace-tolerant) + linter feedback loop.
- No vector DB v1. Add only if a measured gap on natural-language queries against unfamiliar repos demands it.

**Where to revisit:** if the form factor is in-IDE chat with sub-second turnaround, the calculus flips toward Cursor/Continue-style AST-chunk + embed + rerank; CLI agents can afford agent-loop latency that browser sidebars cannot.
