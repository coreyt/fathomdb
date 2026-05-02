# Context Research — source-code

## Scope

How should an AI coding agent be given the *existing source code* as context? This dimension covers retrieval mechanism (RAG embeddings vs agentic grep/glob vs hybrid), repo maps, AST-aware chunking, recent-edits/git context, LSP/symbol signals, code chunking strategy, file selection/ranking, and long-context-window vs retrieval tradeoffs.

Out of scope: standalone tech docs, tests, build/CI signals, CLAUDE.md/AGENTS.md memory files (separate dimensions).

## Sources

Anthropic (Claude Code docs, Boris Cherny on X, Pragmatic Engineer interview), Cursor engineering blog (semsearch, secure indexing), Cognition Devin blog, Sourcegraph Cody docs/changelog, OpenAI Codex docs (developers.openai.com), arXiv (cAST 2506.15655, Lost-in-the-Middle 2307.03172, Context-Length-Alone 2510.05381, NL2Repo-Bench 2512.12730, exploratory study of code retrieval 2025), CodeRAG-Bench (NAACL 2025 Findings), SWE-Fixer (ACL 2025 Findings), Aider docs, Amazon Science (Feb 2026 paper, via secondary). All citations include URL + date in findings below.

## Findings

### F1: Frontier coding agents have split on retrieval mechanism — Anthropic abandoned RAG for agentic grep, Cursor doubled down on embeddings

- Evidence:
  - Boris Cherny (Anthropic, Claude Code creator), X post: "Early versions of Claude Code used RAG + a local vector db, but we found pretty quickly that agentic search generally works better. It is also simpler and doesn't have the same issues around security, privacy, staleness, and reliability." (<https://x.com/bcherny/status/2017824286489383315>)
  - Anonymous Anthropic engineer on HN: "In our testing we found that agentic search outperformed [it] by a lot, and this was surprising." (cited in <https://vadim.blog/claude-code-no-indexing>, 2026-03-03)
  - Cursor engineering blog "Improving agent with semantic search" (2025-11-06, <https://cursor.com/blog/semsearch>): semantic search delivers +12.5% accuracy on Cursor Context Bench (range 6.5%–23.5% per model); +2.6% code retention on 1000+ file projects via online A/B; user dissatisfaction +2.2% when semantic search is unavailable. They explicitly conclude "combining grep with semantic search produces optimal outcomes."
- Observations:
  - Both labs ran A/B tests on the same question and got opposite-feeling answers. The reconciling read: Cursor measured *agent*-grade retrieval where the agent already has tools; their +12.5% comes from *adding* embeddings on top of grep, not replacing it. Anthropic reports replacing grep with vector DB hurt.
  - Anthropic's argument is operational as much as quality: staleness, permission scoping, privacy of an embedding index — these dominate when the agent runs locally on a developer's box. Cursor solves the staleness/privacy problem with Merkle-tree sync + path obfuscation (<https://cursor.com/blog/secure-codebase-indexing>).
  - Sourcegraph Cody (<https://sourcegraph.com/docs/cody/capabilities/agentic-context-fetching>) and Devin (<https://cognition.ai/blog/devin-annual-performance-review-2025>) both ship hybrid: keyword + SCIP code graph + semantic + agentic refinement.
- Recommendations:
  - Default an agent's source-code retrieval to grep/glob/read tools driven by the model. Treat embeddings as an *addition* if you have remote infra and a controlled corpus, not a replacement.
  - If you adopt embeddings, plan for Merkle-tree-style incremental sync and a privacy story; don't hand-roll a stale local FAISS.
- Impact on agent LLM: HIGH
  - Rationale: This is the load-bearing decision for the entire dimension. Two frontier labs published quantified deltas; the choice changes infrastructure, security posture, and per-query token cost by an order of magnitude.

### F2: Agentic search is "good enough" because exact-match symbols dominate code retrieval

- Evidence:
  - Amazon Science paper, Feb 2026 (arXiv 2602.23368, summarized in <https://vadim.blog/claude-code-no-indexing> 2026-03-03): "keyword search via agentic tool use achieves over 90% of RAG-level performance without a vector database … exact-match retrieval with iterative refinement competes with semantic search [where] symbols are precise by definition."
  - Boris Cherny / Pragmatic Engineer interview (<https://newsletter.pragmaticengineer.com/p/building-claude-code-with-boris-cherny>, 2026-03-04): "Claude Code's 'agentic search' is really just glob and grep, and it outperformed RAG."
  - LSP-client ecosystem post (<https://lsp-client.github.io/>, 2025): "findReferences on a function returns only actual call sites, not comments, not string matches… 23 real results instead of 500+ grep matches." Symbol precision > grep precision > embedding precision for unambiguous identifiers.
- Observations:
  - The 90% number is a ceiling argument, not a floor. Agentic search is also iterative — wrong-on-first-try is recovered by the next tool call, so end-task success is closer to RAG than per-query recall suggests.
  - The "symbol precision" property is what makes code different from prose. For natural-language docs the symbol assumption breaks and embeddings recover their lead.
- Recommendations:
  - For the FathomDB Python/Rust crate context, grep/glob is sufficient; symbol names (`filter_json_fused_*`, `BuilderValidationError`, `Schema v24`) are unique enough that exact match wins.
  - Reserve semantic retrieval for cases where the user's query *doesn't* contain a symbol (e.g., "the place that handles corruption on open").
- Impact on agent LLM: HIGH
  - Rationale: Directly explains *why* the grep-first strategy works, which justifies skipping vector-DB infra for code context.

### F3: AST-aware chunking measurably beats line-based chunking when you do use embeddings

- Evidence:
  - cAST paper (arXiv 2506.15655, June 2025, <https://arxiv.org/html/2506.15655v1>): structure-aware chunking via tree-sitter + recursive split-merge improves Recall@5 by +1.8–4.3 points on RepoEval, Pass@1 by +4.1 points (StarCoder2 on RepoEval), Pass@1 +2.67 on SWE-bench (Claude generator), and +4.3 on CrossCodeEval multilingual.
  - Aider's repo map (<https://aider.chat/2023/10/22/repomap.html>, foundational; still cited 2025) uses tree-sitter to extract definitions/references and PageRank-rank symbols — same intuition: chunks at function/class boundaries beat arbitrary windows.
  - Supermemory's `code-chunk` (2025, <https://supermemory.ai/blog/building-code-chunk-ast-aware-code-chunking/>) reports 70% correct-answer rate AST-chunked vs 59% naive vs 61% language-aware.
- Observations:
  - Line-based chunking can split mid-function and orphan signatures from bodies — the chunk is unretrievable AND unhelpful when retrieved. AST chunking is the cheapest fix.
  - "Prepended metadata" (file path / class / function name) is part of why AST chunks help; the chunk carries breadcrumbs the retriever can match against.
- Recommendations:
  - If we ever index FathomDB for an agent (e.g., docs site, SDK examples), use tree-sitter chunking, not line counts. Chunk size by non-whitespace characters, not lines.
- Impact on agent LLM: MED
  - Rationale: Real measured deltas (+2.67 Pass@1 on SWE-bench is non-trivial) but only matters once you've committed to an embedding index — most coding agents we care about (Claude Code, Codex CLI) don't index.

### F4: Repo maps / symbol summaries are an effective "table of contents" without full retrieval

- Evidence:
  - Aider repo map docs (<https://aider.chat/docs/repomap.html>, foundational, still in active use 2025): "concise map of your whole git repository that includes the most important classes and functions along with their types and call signatures … helps aider write new code that respects … existing libraries, modules and abstractions."
  - Cognition Devin "DeepWiki" (<https://cognition.ai/blog/devin-annual-performance-review-2025>, 2025-12): auto-generates always-updating wikis with architecture diagrams; "Ask Devin uses information in the Wiki to better understand and find relevant context"; demonstrated on 5M-line COBOL and 500GB repos.
  - NL2Repo-Bench (arXiv 2512.12730, Dec 2025): "task_tracker exhibits the strongest correlation with model performance (0.711)" — task/state planning beats raw retrieval at long-horizon repo work; implicitly the repo map gives the planner something to plan over.
- Observations:
  - Weak evidence on quantified delta from a repo map *alone*: most of the data conflates repo-map + agentic search + planning. But every agent that scales to large repos ships some kind of structured map.
  - Devin's DeepWiki and Aider's tree-sitter map converge on the same idea from different ends: humans-readable docs vs LLM-readable signature digest. Both work.
- Recommendations:
  - For multi-crate workspaces like FathomDB, a tree-sitter-generated symbol/dependency map (functions, public types per crate, cross-crate references) given to the agent up-front would likely improve cross-crate edits. Not a vector DB — a static `repo_map.md` regenerated on each task.
- Impact on agent LLM: MED
  - Rationale: Plausible-and-consistent endorsement across 3+ vendors, but no public ablation isolates the repo-map contribution. Weak evidence on magnitude.

### F5: Long context windows do NOT replace retrieval for code, even at 1M tokens

- Evidence:
  - Tianpan summary of Claude/Gemini long-context evals (<https://tianpan.co/blog/2026-04-09-long-context-vs-rag-production-decision-framework>, 2026-04): "Claude achieves approximately 90% retrieval accuracy at 1M token contexts, meaning roughly 1 in 10 queries gets the wrong answer." Gemini 1.5 Pro at 99.7% on single-fact NIAH but ~60% on multi-fact realistic retrieval.
  - "Context Length Alone Hurts LLM Performance Despite Perfect Retrieval" (arXiv 2510.05381, Oct 2025): "even when a model can perfectly retrieve all the evidence … its performance still degrades substantially as input length increases."
  - "Lost in the Middle" (Liu et al., TACL 2024, still load-bearing in 2025 lit, <https://arxiv.org/abs/2307.03172>): U-shaped curve; middle of context is dramatically less reliable.
  - Latency/cost: 1M-token inference is ~30–60× slower and ~1250× more expensive per query than RAG-then-generate (sitepoint long-context-vs-rag, 2026).
- Observations:
  - The "stuff the whole repo in context" approach is tempting at 1M tokens but degrades on multi-fact reasoning, which is exactly what coding tasks require.
  - Anthropic's prompt-caching-driven economics (92% prefix reuse on Claude Code per LMCache analysis Dec 2025, <https://blog.lmcache.ai/en/2025/12/23/context-engineering-reuse-pattern-under-the-hood-of-claude-code/>) make iterative agentic search affordable, partially neutralizing the "burn tokens" critique.
- Recommendations:
  - Don't pre-load full repos. Let the agent retrieve narrowly and iteratively; rely on prompt caching for the constant prefix.
  - If forced to use long context, place the most critical evidence at the *start or end*, not the middle.
- Impact on agent LLM: HIGH
  - Rationale: Quantified accuracy + latency + cost penalties are all material; this directly affects agent task success.

### F6: LSP / static-analysis signals are an emerging, high-precision context channel

- Evidence:
  - Claude Code 2.0.74 (Dec 2025) added native LSP support (<https://lsp-client.github.io/>, 2025).
  - LSAP (Language Server Agent Protocol, <https://github.com/lsp-client/LSAP>, 2025): batches LSP atomic ops into single semantic calls for agents; "findReferences … 23 real results instead of 500+ grep matches."
  - OpenCode and Kiro both expose LSP-as-tool (<https://opencode.ai/docs/lsp/>, <https://kiro.dev/docs/cli/code-intelligence/>).
- Observations:
  - LSP gives the agent *typed* facts (definition site, real references, type-of-symbol, compile errors) that grep cannot. The cost is plumbing complexity and per-language LSP server availability.
  - This complements rather than replaces grep: grep is the fan-out tool, LSP is the precision tool.
- Recommendations:
  - When an agent works in a typed language with a mature LSP (Rust via rust-analyzer, TS, Python via pyright), exposing LSP "find references" / "go to definition" tools is a high-leverage addition.
- Impact on agent LLM: MED
  - Rationale: Strong directional consensus (multiple agents adopting LSP in 2025) but minimal published ablation isolating the win. Likely high once someone publishes numbers.

### F7: Recent-edits / git context narrows file selection and reduces hallucination

- Evidence:
  - Geoffrey Litt (Ink & Switch), Twitter 2025-06 (<https://x.com/geoffreylitt/status/1938239983464140920>): better diff views and "zoomed-out diffs" are needed for agent-driven workflows; reviewer bottleneck.
  - "Lore" paper (arXiv 2603.15566): treats git commit messages as a structured knowledge protocol for AI coding agents — commit history as first-class context.
  - The AI Stack blog (<https://www.theaistack.dev/p/git-memory>, 2025): recent-branch commits and HEAD diffs significantly reduce agent hallucinations vs only-current-state context.
- Observations:
  - Most agents already implicitly use `git diff`/`git log` via Bash tool, but the ergonomic story (which diffs to surface, how summarized) is unsettled.
  - SWE-bench Verified contamination concern: models "were 6× better at finding edited files without any additional context" (<https://arxiv.org/abs/2512.10218>, Dec 2025) — they've memorized historical diffs. Fresh git context for novel problems is what matters.
- Recommendations:
  - For multi-step edits, expose `git status`, `git diff HEAD~N..HEAD`, and `git log --oneline` to the agent. For PR review, consider Difftastic or AST-aware diff (semantic diff > textual diff).
- Impact on agent LLM: MED
  - Rationale: Practitioner consensus + plausible mechanism; lacks a controlled ablation. Strong qualitative signal that "what changed recently" is high-density context.

## Synthesis

The dominant 2025-2026 stance among frontier coding agents on **how to expose source code** is:

1. **Default to agentic grep/glob/read tools, not pre-built embedding indexes.** Anthropic has the strongest published claim, Amazon Science corroborates the ~90% number, and the operational simplicity (no stale index, no permission split, no privacy headache) is decisive on the local-machine deployment that defines Claude Code and Codex CLI.
2. **Embeddings remain a real win in specific niches** — large multi-repo cloud deployments (Cursor, Sourcegraph), or queries phrased without symbols. Cursor's +12.5% measurement is the single best public number for "embeddings *do* help if you do them right." Treat as additive, not foundational.
3. **Long-context windows do not erase retrieval.** 1M-token degradation is real; lost-in-the-middle is real; the cost is 1250× per query. Iterative narrow retrieval with aggressive prompt caching is the frontier-lab consensus.
4. **Repo maps and LSP are under-exploited high-leverage additions.** Aider's tree-sitter map and DeepWiki-style auto-docs both ship in production agents; Claude Code only just (Dec 2025) gained native LSP. Expect this to converge toward "agent has grep + LSP + tree-sitter symbol map + on-demand read."
5. **AST-aware chunking is the right default *if* you index.** cAST gives +2-4 Pass@1 across SWE-bench/RepoEval — small but free. Don't ship a line-based chunker.
6. **Git context (recent diffs, branch history) is a cheap reliability win** that every shipping agent uses informally; the formal protocol is unsettled but the direction is clear.

For FathomDB's own agents and any tooling we expose: lead with grep/glob/read, ship a tree-sitter-generated repo-map regenerated per task, expose `git diff`/`git log`, expose `rust-analyzer`/`pyright` LSP tools, and *do not* build a local vector index. If we ever expose a remote SDK-docs context channel (separate dim, not source code), revisit embeddings — but for source code, agentic search is the answer.

## See also

- Other dimensions: tech-docs (likely flips the conclusion toward embeddings), tests (different retrieval pattern), build-CI (different signal class), CLAUDE.md/AGENTS.md (memory layer, complementary).
- Foundational pieces still cited in 2025-26: Liu et al. "Lost in the Middle" (TACL 2024); SWE-bench original (ICLR 2024); Aider repo-map post (2023).
- Most-relevant 2025-26 reads: Cursor "Improving agent with semantic search" (2025-11-06); Boris Cherny on Pragmatic Engineer (2026-03-04); cAST (arXiv 2506.15655, June 2025); CodeRAG-Bench (NAACL 2025 Findings); LMCache "Context Engineering & Reuse Pattern Under the Hood of Claude Code" (2025-12-23).
