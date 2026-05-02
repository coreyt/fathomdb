# Context Research: Comments

## Scope

This report examines how Claude and Codex benefit from source code comments and adjacent textual artifacts during AI-driven coding: inline comments, docstrings/JSDoc, rationale notes embedded near implementation, ADR-like fragments, and commit messages when they capture change intent. The focus is practical: when these artifacts improve retrieval, understanding, planning, and code modification; when they degrade performance through drift or noise; and what writing/placement/maintenance patterns matter most for agentic coding.

## Sources (URLs cited)

[S1]: <https://code.claude.com/docs/en/memory>
[S2]: <https://code.claude.com/docs/en/tutorials>
[S3]: <https://openai.com/index/introducing-codex/>
[S4]: <https://openai.com/business/guides-and-resources/how-openai-uses-codex/>
[S5]: <https://openai.com/index/harness-engineering/>
[S6]: <https://link.springer.com/article/10.1007/s10664-019-09694-w>
[S7]: <https://link.springer.com/article/10.1007/s10664-023-10425-5>
[S8]: <https://www.sciencedirect.com/science/article/abs/pii/S016412121100238X>
[S9]: <https://link.springer.com/article/10.1007/s10664-022-10284-6>
[S10]: <https://www.researchgate.net/publication/355393354_AugmentedCode_Examining_the_Effects_of_Natural_Language_Resources_in_Code_Retrieval_Models>
[S11]: <https://aclanthology.org/2022.emnlp-main.372/>
[S12] <https://link.springer.com/article/10.1007/s10664-021-09981-5>

## Findings F1..F6

### F1. Comments help coding agents most when they encode semantics the code does not state well on its own

Evidence: The comment taxonomy in [S6] distinguishes summary ("what"), expand ("how"), and rationale ("why") comments, plus notice/usage/exception comments. The inline-comment study in [S7] notes that comments are commonly written to explain a snippet, a decision, or a warning, and that high-quality comments improve tool performance. Claude's official project-memory docs explicitly recommend persistent context for coding standards, project architecture, and architectural decisions [S1]. Codex guidance similarly says AGENTS.md should capture business logic, known quirks, and dependencies the model cannot infer from code alone [S3][S4].

Observations: For Claude and Codex, the highest-value nearby prose is not narration of obvious control flow; it is missing semantics: invariants, hidden assumptions, failure modes, domain rules, deprecation guidance, and rationale for unusual implementation choices. This is where comments and docstrings act as semantic compression for the agent: a short piece of natural language preserves intent that would otherwise require multi-file inference. The "semantic compression" term here is an inference from [S1][S3][S4][S6], not wording used by the sources.

Recommendations: Prefer comments that explain why a branch exists, what invariant must hold, what side effect is non-obvious, or what behavior must remain compatible. Put warnings and usage constraints directly above the risky code or API surface. Avoid comments that merely restate syntax or variable names.

Impact on agent LLM = HIGH: These are exactly the facts agents struggle to recover from code alone, and they directly affect retrieval, planning, edit safety, and regression risk.

### F2. Concise, scoped textual context beats monolithic documentation for both Claude and Codex

Evidence: Claude's memory docs say CLAUDE.md is context rather than enforced configuration; "specific, concise, well-structured instructions work best," target under 200 lines, and use path-scoped rules to reduce noise [S1]. Codex's official launch post says AGENTS.md helps the agent navigate the repo, run tests, and follow project practices [S3]. OpenAI's Harness engineering writeup reports that one large AGENTS.md failed because context is scarce, too much guidance becomes non-guidance, and monolithic guidance "rots instantly"; the team instead used a short AGENTS.md as a table of contents pointing to structured docs, plans, and architecture notes [S5].

Observations: This strongly supports a layered context strategy for coding agents: a short root guide, deeper scoped docs, and nearby code-local explanations where needed. For comment-style artifacts, the implication is that small, colocated explanations outperform big prose blobs because they preserve token budget and arrive with the relevant code. ADR fragments and module notes work best when they are indexed and discoverable from the code path they govern.

Recommendations: Keep root-level CLAUDE.md or AGENTS.md short and map-like. Push detailed policies into scoped rules or module docs. For major decisions, keep a brief ADR fragment in-repo and link to it from the affected module README, package doc, or key file comment. Treat nearby text as a compressed index into deeper sources of truth, not as a full encyclopedia.

Impact on agent LLM = HIGH: Context-window pressure and retrieval precision are first-order constraints for agents; scoping and brevity materially change adherence and search quality.

### F3. Structured docstrings and documentation comments are more machine-usable than freeform prose

Evidence: [S9] finds that structural elements in docstrings/Javadoc are widely used in practice across Java and Python, and that models leveraging section structure improve comment-completion accuracy (Top-1 accuracy gains of 9.6% for Python and 7.8% for Java). The same paper notes that docstrings commonly encode summaries, parameters, returns, raises, and restrictions, and that its dataset is suitable for semantic code search because it provides comment-and-code pairs at function level [S9]. Claude's documentation workflows explicitly include finding missing JSDoc/docstrings, adding them, enriching them with context/examples, and checking them against project standards [S2].

Observations: Sectioned documentation is easier for agents to parse, compare, and reuse than freeform comment blocks. For public APIs, structured docstrings also expose behavior at the right granularity for retrieval and for downstream planning, especially around parameters, outputs, errors, and calling constraints. Nearby docstrings are thus more valuable than remote prose for interface understanding.

Recommendations: Standardize one docstring format per language. For non-trivial public functions/classes, document parameters, returns, raised errors, side effects, and restrictions. Keep inline comments for local rationale and docstrings for callable behavior. Add examples only when they disambiguate real usage.

Impact on agent LLM = HIGH: Structured docstrings directly improve machine readability and give the agent better anchors for search, code explanation, and test generation.

### F4. Stale, redundant, and inconsistent comments actively mislead coding agents

Evidence: The study in [S8] found a strong relationship between comment update practices and future bugs; deviations from the usual consistency pattern between code and comment are risky and should be reviewed carefully. The inline smell taxonomy in [S7] identifies 11 smell types and explicitly calls out low-quality comments, including redundancy and inconsistency with code, as harmful to comprehension and maintainability. Harness engineering reports the same failure mode at repo-doc scale: stale monolithic guidance became an "attractive nuisance," so the team added mechanical validation and a doc-gardening agent to catch outdated docs [S5].

Observations: For agents, stale prose is worse than missing prose because it competes with the code as a plausible source of truth. Redundant comments also consume retrieval budget without adding signal, which can distort plan generation or cause the model to anchor on outdated intent. This applies to inline comments, docstrings, TODOs, ADR snippets, and AGENTS/CLAUDE files alike.

Recommendations: Update comments in the same change that alters behavior. Prefer deleting a misleading comment to keeping it. Review old TODOs and task comments as debt, not decoration. Add lightweight checks where possible: comment-drift review prompts, doc freshness checks, or CI rules for structured docs and cross-links.

Impact on agent LLM = HIGH: Bad text is not neutral context for an agent; it is adversarially misleading context that can produce wrong edits and wrong explanations.

### F5. Nearby natural language improves retrieval and planning when it is versioned, colocated, and discoverable

Evidence: OpenAI's internal Codex guidance says prompts work better when they include file paths, component names, diffs, and doc snippets, and that AGENTS.md should capture repo facts the agent cannot infer [S4]. Harness engineering states "what Codex can't see doesn't exist" and describes pushing architectural discussions and decision logs into repository-local markdown so they become legible to the agent [S5]. AugmentedCode [S10] explicitly studies code comments, docstrings, and commit messages as natural-language resources for code retrieval and argues these resources can improve retrieval performance; it even anticipates machine/bot consumers of such retrieval systems. RACE [S11] shows retrieval-augmented commit-message generation outperforming baselines, reinforcing that retrieved textual exemplars can improve code-to-text reasoning.

Observations: Nearby natural language gives the agent searchable lexical anchors for business concepts, constraints, and intent that may not be obvious in identifiers alone. This is especially useful for planning edits across a repo. The explicit Claude/Codex evidence is strongest at repo-doc and prompt level [S1][S3][S4][S5]; the claim that nearby comments/docstrings aid retrieval inside coding agents is partly an inference from retrieval literature [S9][S10][S11], and should be read that way.

Recommendations: Keep repo-relevant prose in-repo and versioned. Add short module READMEs, package docs, or ADR fragments where domain concepts or architecture boundaries matter. When prompting Codex or Claude, include local doc snippets and file paths rather than broad prose summaries. Favor colocated markdown and code-adjacent explanations over chat-only or wiki-only context.

Impact on agent LLM = HIGH: Retrieval and planning quality depend on what natural-language anchors are available near the code and whether the agent can actually see them.

### F6. Commit messages and ADR fragments are useful context only when they capture rationale and behavior change, not just mechanics

Evidence: AugmentedCode treats commit messages as one of the natural-language assets associated with code and asks whether code comments, docstrings, and commit messages can improve code retrieval [S10]. RACE treats retrieved similar commits and their messages as exemplars for generating more accurate commit messages [S11]. Harness engineering operationalizes the broader pattern by checking design docs, execution plans, and decision logs into the repo, rather than leaving rationale in Slack or tacit memory [S5]. Claude's docs likewise recommend storing project architecture and repeated corrections in version-controlled instruction files [S1].

Observations: Commit messages are not adjacent to implementation in the file tree, but they are adjacent in version history and can still be high-signal context when the agent or retrieval layer looks backward. Their value is mostly rationale capture: why behavior changed, what invariant was preserved, what migration or compatibility rule mattered. ADR fragments serve the same role at a longer half-life. Both hurt when vague ("fix stuff") or purely mechanical ("rename vars").

Recommendations: Write commit messages that state intent, behavioral effect, and important constraints. Use ADR fragments for decisions with longer shelf life: architecture boundaries, tradeoffs, migrations, and forbidden patterns. Cross-link ADRs from module docs or key files when the decision materially shapes local implementation. Do not rely on private chat threads as the only record of architectural rationale.

Impact on agent LLM = MED: These artifacts are not always loaded by default, but when retrievable they provide high-value intent and can materially improve explanations, backtracking, and long-horizon edits.

## Synthesis

The common pattern across Claude docs, Codex guidance, and the research literature is that textual context helps coding agents only when it adds non-obvious semantics and stays trustworthy. The best artifacts are short, scoped, versioned, and close to the code or at least discoverable from it: rationale comments, structured docstrings, module-level notes, ADR fragments, and commit messages that explain intent. The worst artifacts are stale, redundant, overly broad, or disconnected from the implementation, because they consume context budget while competing with the code as a false source of truth. For agentic coding, the practical goal is not "more comments"; it is higher-signal natural language with disciplined placement, structure, and maintenance.
