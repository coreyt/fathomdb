# Context Research — dev-env

## Scope

The development environment (build system, linter, type checker, CI, language server, sandbox/runtime) is _both_ a context source (compile errors, lint warnings, type signatures, runtime traces) and a feedback channel (run command -> read output -> next action). This dimension surveys what frontier labs and recent literature say about how that channel should be structured: how rich the signal should be, how it should be truncated/summarized, how the surrounding sandbox should be shaped, and how loops should be budgeted. Out of scope: TDD/oracle semantics, retrieval, docs, persistent memory files.

## Sources

Primary frontier-lab and academic sources, post-2025-05-01 unless noted:

- Anthropic, "Claude Code Sandboxing" engineering post, 2025-10-20 — <https://www.anthropic.com/engineering/claude-code-sandboxing>
- Anthropic, "Effective context engineering for AI agents", 2025-09-29 — <https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents>
- Anthropic, Claude Code Hooks reference — <https://code.claude.com/docs/en/hooks>
- OpenAI, Codex sandboxing concept doc — <https://developers.openai.com/codex/concepts/sandboxing>
- OpenAI, Codex CLI features and best practices — <https://developers.openai.com/codex/cli/features> and <https://developers.openai.com/codex/learn/best-practices>
- Yang et al., "SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering," NeurIPS 2024 — <https://arxiv.org/abs/2405.15793>
- Gehring et al., "RLEF: Grounding Code LLMs in Execution Feedback with Reinforcement Learning," ICML 2025 spotlight — <https://arxiv.org/abs/2410.02089>
- Deligiannis et al., "RustAssistant: Using LLMs to Fix Compilation Errors in Rust Code," Microsoft Research, 2024 — <https://www.microsoft.com/en-us/research/publication/rustassistant-using-llms-to-fix-compilation-errors-in-rust-code/>
- "Budget-Aware Tool-Use Enables Effective Agent Scaling," 2025-11 — <https://arxiv.org/html/2511.17006v1>
- "SWE-RM: Execution-Free Feedback for Software Engineering Agents," 2025-12 — <https://www.arxiv.org/pdf/2512.21919>
- OpenHands Software Agent SDK, 2025-11 — <https://arxiv.org/html/2511.03690v1>
- "Sandboxing LLM coding agents (parts 1-2)," VirtusLab, 2025 — <https://virtuslab.com/blog/ai/sandboxing-llm-coding-agents-part1>
- LSP-AI / Agent Client Protocol coverage; "LSP: The Secret Weapon for AI Coding Tools" — <https://amirteymoori.com/lsp-language-server-protocol-ai-coding-tools/>
- Cognition, "Devin's 2025 Performance Review" — <https://cognition.ai/blog/devin-annual-performance-review-2025>
- "Solving Context Window Overflow in AI Agents," 2025-11 — <https://arxiv.org/html/2511.22729v1>

## Findings

### F1: Execution feedback in the loop is now a _trained_ capability, not just a prompt pattern

- Evidence: RLEF (Gehring et al., ICML 2025 spotlight, <https://arxiv.org/abs/2410.02089>) trains 8B and 70B code LLMs end-to-end with PPO using public-test execution feedback as the per-turn signal and private-test pass as the terminal reward. The trained models "reduce the amount of samples required by an order of magnitude" on competitive programming and _meaningfully take execution feedback into account and resolve errors over multiple turns_ — base models without RLEF largely fail to improve across turns. RLVR with verifiable code/execution rewards is now the dominant frontier-lab recipe (DeepSeek-R1, OpenAI o-series, Anthropic reasoning variants — see <https://blog.dailydoseofds.com/p/how-top-ai-labs-are-building-rl-agents>).
- Observations: Base/instruct models historically wasted multi-turn budgets — the "give it the error and let it retry" pattern produced <2x gains on most benchmarks. Post-RLEF/RLVR models are quantitatively different consumers of compiler/test output. This means dev-env feedback design now has a model-side counterpart that _expects_ structured, parseable failure signals.
- Recommendations: Treat compiler/test output as a first-class input format. Don't paraphrase it; pass it through cleanly. Older heuristics (summarize, soften, "translate to natural language") are negative-value against modern reasoning models.
- Impact on agent LLM: HIGH
  - Rationale: Order-of-magnitude sample efficiency improvement is directly attributable to the loop-shape, and frontier models are now trained to exploit it.

### F2: Compiler error message _quality_ materially gates LLM repair rates

- Evidence: RustAssistant (Microsoft Research 2024, <https://www.microsoft.com/en-us/research/publication/rustassistant-using-llms-to-fix-compilation-errors-in-rust-code/>) reaches ~74% peak fix accuracy on real-world open-source Rust errors and 93% on micro-benchmarks, vs `cargo fix` at <10%. The result depends on iterating between LLM and the compiler (multi-turn) and on the richness of `rustc` diagnostics (suggested fixes, span pointers, lifetime explanations). Cross-language compiler-quality surveys consistently rank Rust and Elm at the top and Java/TypeScript/Go lower for human and machine readability (<https://www.amazingcto.com/developer-productivity-compiler-errors/>).
- Observations: Where compiler diagnostics are weak (TypeScript "Type 'X' is not assignable to type 'Y'" without structural diff; Java stack-traces without source spans), the agent must do extra retrieval to rebuild the context the compiler already had. This shows up as more tool calls and more wrong-edit attempts.
- Recommendations: When designing tool wrappers, do not strip diagnostic structure. Pass `--error-format=json` (rustc), `--diagnostics-format` equivalents (clippy, mypy `--show-error-codes --show-column-numbers --pretty`), and surface the compiler's own suggestion blocks. For weak-diagnostic ecosystems, augment with LSP `textDocument/diagnostic` rather than parsing CLI output.
- Impact on agent LLM: HIGH
  - Rationale: 74% vs <10% repair rate (RustAssistant vs cargo fix on the same population) is a measured, large delta attributable to diagnostic richness plus loop iteration.

### F3: The agent-computer interface (ACI) — not raw shell — is what makes loops work

- Evidence: SWE-agent (Yang et al., NeurIPS 2024, <https://arxiv.org/abs/2405.15793>) showed that giving a model `bash` is _insufficient_: a tailored ACI (line-windowed file viewer, syntactic edit command with linter feedback inline, scoped search) lifted SWE-bench pass@1 from ~3% to 12.5%. The OpenHands V1 SDK (<https://arxiv.org/html/2511.03690v1>) generalizes this: tools are explicit, tool outputs are normalized, and the workspace is swappable (in-process / Docker / remote). Anthropic's context-engineering post (<https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents>, 2025-09-29) reinforces: tools should be "self-contained, robust to error, and extremely clear with respect to their intended use," and tool result clearing is one of the safest forms of compaction.
- Observations: Bare bash is high-variance because output shape varies per command and tools can fail in many ways. The fix is wrapping the dev-env primitives in a small set of well-typed verbs that emit consistent structured output.
- Recommendations: For a project's dev-env, expose verbs like `build()`, `lint()`, `typecheck()`, `test(filter)`, `run(target)` that return structured `{exit_code, stdout, stderr, diagnostics[]}` rather than asking the model to invoke `cargo build 2>&1 | tail -200`. Keep diagnostics array machine-parseable.
- Impact on agent LLM: HIGH
  - Rationale: 4x SWE-bench lift from ACI design alone is one of the largest documented engineering effects on agent success.

### F4: LSP gives agents semantic eyes the compile loop alone doesn't

- Evidence: Claude Code added native LSP integration in v2.0.74 (December 2025); coverage at <https://amirteymoori.com/lsp-language-server-protocol-ai-coding-tools/> and <https://blog.promptlayer.com/agent-client-protocol-the-lsp-for-ai-coding-agents/> describes "automatic error detection after every edit ... type mismatches, undefined variables, and missing imports, allowing the agent to fix problems before code runs." The Agent Client Protocol (ACP) and LSAP are emerging as standards for LSP-style semantic primitives (`find references`, `go to definition`, `rename symbol`) exposed to agents.
- Observations: LSP signals arrive _between_ edits at sub-second latency, before a full build. This shortens the feedback loop dramatically for incremental work and avoids flaky feedback from longer-running builds. It also gives the agent type information for cross-file changes the model could only otherwise get by reading every transitive dependency.
- Recommendations: Wire LSP into the agent harness (diagnostics + symbol search + hover for type) where available. For Rust, this is `rust-analyzer`; for Python, `pyright`; for TS, `tsserver`. Treat LSP diagnostics as an in-loop signal between edits, separate from the build feedback that runs at task boundaries.
- Impact on agent LLM: MED
  - Rationale: Strong qualitative claims and architectural alignment from major vendors, but quantitative head-to-head data (LSP-on vs LSP-off agent success) is still scarce in the public record.

### F5: Sandboxing reduces approval friction without measurably degrading capability

- Evidence: Anthropic's Claude Code sandboxing post (2025-10-20, <https://www.anthropic.com/engineering/claude-code-sandboxing>) reports "in internal usage, sandboxing safely reduces permission prompts by 84%" while keeping the capability surface intact (filesystem write to cwd, network via proxy). OpenAI Codex's sandbox doc (<https://developers.openai.com/codex/concepts/sandboxing>) frames it identically: sandboxing is about _reducing approval fatigue_ so the agent can run routine commands autonomously. VirtusLab's analysis (<https://virtuslab.com/blog/ai/sandboxing-llm-coding-agents-part1>) and the transactional-sandbox paper (arxiv 2512.12806) report compute-heavy overhead is minimal; gVisor I/O-heavy is 10-30%, ~14.5% for transactional designs.
- Observations: The capability/security tradeoff is not actually a tradeoff in the regime that matters for agents — defaults can be tight without losing meaningful capability, because agents mostly read/write within the project tree and call out to a small allowlist of registries. The 84% reduction in human-in-the-loop prompts is itself a productivity gain because each prompt would otherwise either block the agent (latency) or train users to rubber-stamp (security).
- Recommendations: Default sandbox = cwd-only filesystem write + proxy-network with allowlist for the project's registry/CI endpoints. Use OS primitives (bubblewrap on Linux, seatbelt on macOS) rather than container overhead when host-level threat is low.
- Impact on agent LLM: MED
  - Rationale: Quantified prompt-reduction (84%) is a real productivity number but doesn't directly measure task success. Capability cost appears small but is unmeasured at task-success granularity.

### F6: Tool-use loops plateau around 100 calls without explicit budget signaling

- Evidence: "Budget-Aware Tool-Use Enables Effective Agent Scaling" (<https://arxiv.org/html/2511.17006v1>, 2025-11) shows ReAct agents on BrowseComp saturate at a 100-tool-call budget and "fail to utilize any additionally allocated budget, even though the context window is not filled up" — the agent terminates believing it's done or stuck. Adding an explicit budget tracker yields 31.3% cost reduction at matched accuracy, plus continued scaling beyond the plateau. Real-world failure mode: the November 2025 "$47K agent loop" (4 agents in infinite loop for 11 days, <https://dev.to/waxell/the-47000-agent-loop-why-token-budget-alerts-arent-budget-enforcement-389i>).
- Observations: Two failures co-exist: (a) premature termination because the agent has no awareness of remaining headroom and (b) runaway loops because there's no enforced stop. Both are cured by the same fix: surface the budget _and_ enforce a hard cap.
- Recommendations: Always pass remaining-iteration / remaining-token state into the agent's context each turn. Combine with a hard external cap (iterations, wall time, tokens) the harness enforces regardless of the agent's belief.
- Impact on agent LLM: MED
  - Rationale: Big effect on cost efficiency and tail-risk; effect on top-line success is task-dependent.

### F7: Long stdout/stderr requires structured truncation, not naive cuts

- Evidence: "Solving Context Window Overflow in AI Agents" (<https://arxiv.org/html/2511.22729v1>, 2025-11) and LangChain Deep Agents context management (<https://www.blog.langchain.com/context-management-for-deepagents/>) converge on the same pattern: outputs over a threshold (commonly ~200 lines) cause "attention collapse"; the fix is to keep the head and tail, replace the middle with a pointer/file reference, and let the agent re-fetch via grep/read. Deep Agents truncate older tool calls when session crosses 85% of context, replacing with on-disk pointers. Tool-parameter truncation mid-generation (when the model emits a too-long file write) is identified as a silent-failure mode that requires runtime detection.
- Observations: For dev-env outputs specifically: a 50,000-line stack-trace dump or webpack bundle log is worse than useless — it crowds out the actual error and the relevant frames. Keep the first error and a tail; let the agent grep the saved log if it needs more.
- Recommendations: In the harness, cap any single tool output at ~200 lines / ~8KB; spill the full output to a file and surface the path. For builds and tests specifically, parse out the _failure section_ (most build tools have markers) and prefer that to head/tail. Detect generation-side truncation (`stop_reason == "length"`) and re-prompt the model to continue or rewrite, never silently accept the partial.
- Impact on agent LLM: MED
  - Rationale: Big quality effect on long-context tasks (attention collapse is real and well-documented), smaller effect on short tasks where outputs naturally fit.

### F8: Hooks externalize policy that prompts cannot reliably enforce

- Evidence: Claude Code hooks reference (<https://code.claude.com/docs/en/hooks>) defines deterministic lifecycle events (PreToolUse, PostToolUse, Stop, etc.) that fire shell commands regardless of model decisions. Anthropic's framing: hooks are "automated triggers that fire when particular events occur, regardless of what the AI 'decides' to do." Practitioner write-ups note hooks let you re-run formatters/linters/typecheckers on every edit so the agent is forced to consume the feedback (<https://medium.com/becoming-for-better/taming-claude-code-a-guide-to-claude-md-and-hooks-ed059879991c>, <https://paddo.dev/blog/claude-code-hooks-guardrails/>). Risk noted in the docs: hooks run with full user permissions outside the sandbox.
- Observations: Hooks turn the dev-env into an _active_ feedback layer instead of a passive one. PostToolUse-Edit -> auto-format-and-lint -> diagnostic-back-to-model is the canonical pattern; PreToolUse can deny destructive commands deterministically (a stronger guarantee than prompting "don't run `rm -rf`").
- Recommendations: For any project with a fast linter/typechecker, wire a PostEdit hook to run it and inject diagnostics as a tool result. For destructive operations and credential exfil, use PreToolUse deny-listing rather than relying on the model. Keep hooks idempotent and bounded — they run on the user's machine without sandbox.
- Impact on agent LLM: MED
  - Rationale: Improves loop quality and safety; effect size is unmeasured publicly but architecturally important. Without hooks, agents repeatedly forget to run the lint/format step a project requires.

## Synthesis

The frontier-lab consensus on dev-env is: a small, well-typed set of verbs with structured outputs (F3), wrapped in a sandbox tight enough to skip approvals (F5), driven by a loop with explicit budget signals and hard caps (F6), with outputs truncated structurally (F7), enriched by LSP for between-edit semantic feedback (F4), enriched further by hooks that enforce repo conventions automatically (F8) — all consumed by models that are now _trained_ to use execution feedback (F1) and that benefit disproportionately from rich diagnostics (F2).

For fathomdb specifically, the most actionable items are: (a) make `cargo`/`clippy`/`rustc` output flow through cleanly (F1, F2 — Rust's diagnostics are best-in-class, do not paraphrase), (b) wire `rust-analyzer` into the harness so the agent sees type/borrow errors before a full build (F4), (c) cap any tool output at ~200 lines with spill-to-file (F7), and (d) use PostEdit hooks for `cargo fmt` + `cargo clippy --fix` so style/lint debt cannot accumulate silently across turns (F8). The 0.6.0 rewrite is a good moment to bake these into whatever harness contract the project standardizes.

Open questions / weaker evidence: (i) quantitative LSP-on/off agent-success deltas are scarce — claims are strong but the numbers aren't there yet; (ii) sandbox capability cost is asserted-small but unmeasured at task-success granularity; (iii) the 100-call plateau is from BrowseComp (web search) not SWE-bench, so the specific number may not transfer to coding loops, though the _shape_ of the failure mode (premature termination without budget signal) clearly does.

## See also

- `tests` dim — TDD/oracle semantics; this dim provides the _infrastructure_ of running tests, that dim provides the _meaning_ of test outcomes.
- `retrieval` dim — file/symbol search; LSP `find references` / `workspace/symbol` straddle the boundary.
- `memory` dim — CLAUDE.md captures repeat-violation lessons that hooks cannot enforce statically.
- `tech docs` dim — compiler/tool docs (rustc book, clippy lints) feed into how well the agent interprets diagnostics.
