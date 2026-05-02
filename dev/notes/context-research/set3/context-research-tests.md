Scope

This report focuses on how tests function as precise context and feedback for AI coding agents, with explicit attention to Claude Code and Codex where the sources support it. The emphasis is on existing repository tests, test-driven prompting, regression tests, executable specifications, failing tests as search guidance, oracle/example use, overfitting risks, flaky or weak tests, and how much test output is useful before context quality degrades.

Sources (URLs cited)

- `S1` https://code.claude.com/docs/en/best-practices
- `S2` https://openai.com/index/introducing-codex/
- `S3` https://developers.openai.com/api/docs/guides/agent-evals
- `S4` https://developers.openai.com/api/docs/guides/evaluation-best-practices
- `S5` https://www.swebench.com/SWE-bench/faq/
- `S6` https://arxiv.org/abs/2107.03374
- `S7` https://arxiv.org/abs/2402.13521
- `S8` https://arxiv.org/abs/2505.09027
- `S9` https://arxiv.org/abs/2304.05128
- `S10` https://arxiv.org/abs/2501.12793
- `S11` https://arxiv.org/abs/2602.07900
- `S12` https://www.microsoft.com/en-us/research/publication/precise-condition-synthesis-program-repair/
- `S13` https://link.springer.com/article/10.1007/s10664-020-09920-w
- `S14` https://www.researchgate.net/publication/348187542_On_the_Impact_of_Flaky_Tests_in_Automated_Program_Repair

Findings F1..Fn

F1. Existing repository tests are the strongest operational contract for coding agents.

Evidence

- Anthropic states that giving Claude a way to verify its work with tests, screenshots, or expected outputs is the "single highest-leverage" action, and recommends prompts that explicitly say to run tests and fix failures (`S1`).
- Anthropic also recommends encoding testing instructions and preferred runners in `CLAUDE.md`, including a concrete example to prefer single tests over the whole suite for performance (`S1`).
- OpenAI states that Codex performs best when given configured dev environments, reliable testing setups, and clear documentation, and that `AGENTS.md` should tell Codex which test commands to run (`S2`).

Observations

- For both Claude Code and Codex, existing tests are not just validation artifacts; they are task-shaping context. They tell the agent what behavior already matters in this repository.
- Existing tests are usually better than ad hoc natural-language instructions because they define real interfaces, setup assumptions, and edge behavior the agent would otherwise infer imperfectly.
- Claude and Codex both expose a place to persist test-running knowledge (`CLAUDE.md` and `AGENTS.md` respectively), which means test commands should be treated as first-class agent context, not incidental prompt detail.

Recommendations

- Put the canonical narrow-test and full-suite commands in agent-facing repo instructions.
- Prefer asking the agent to start from existing failing tests before asking it to invent new ones.
- After a fix, require the agent to run the smallest relevant regression test first, then the broader relevant suite.

Impact on agent LLM = HIGH + rationale

- High, because this changes both search behavior and stopping criteria. Without an executable contract, the agent is optimizing for plausibility; with existing tests, it is optimizing for observable behavior.

F2. Tests act as executable specifications, and example-based tests materially improve code generation quality.

Evidence

- The Codex paper evaluates functional correctness on HumanEval, where programs are synthesized from docstrings and judged by tests; it also shows docstrings alone have limitations and that repeated sampling is needed for hard prompts (`S6`).
- The TDD-for-code-generation paper reports that providing tests in addition to the problem statement improved success on MBPP and HumanEval (`S7`).
- The WebApp1K "Tests as Prompt" paper explicitly frames test cases as both prompt and verification, emphasizing that models must implement functionality directly from tests rather than prose (`S8`).
- Anthropic's prompt examples explicitly use example test cases like valid and invalid email inputs as part of the task description (`S1`).

Observations

- Example-based tests are stronger than prose because they define observable input/output behavior, not just intent.
- For agents, tests do double duty: they are an oracle after code is written and a specification before code is written.
- This is especially relevant when requirements are subtle. Tests can encode negative cases, boundary conditions, and invariants that a natural-language request often leaves underspecified.
- Inference: this applies to Claude and Codex equally because both operate by consuming prompt context plus executable feedback, even though only Claude's docs and Codex's papers/docs state it directly (`S1`, `S2`, `S6`, `S7`, `S8`).

Recommendations

- When prompting Claude or Codex, include a small set of representative examples: happy path, edge case, and one negative case.
- If no tests exist yet, write the minimal behavioral examples first and let the agent code against them.
- Treat test names and assertions as part of the product spec; keep them human-legible.

Impact on agent LLM = HIGH + rationale

- High, because example-based specs reduce ambiguity before the agent searches the codebase, which lowers the odds of a locally plausible but semantically wrong implementation.

F3. Failing tests are highly effective search guidance, but the feedback should be exact and scoped.

Evidence

- Anthropic recommends prompts that say "write a failing test that reproduces the issue, then fix it," and recommends pasting the exact error instead of vague summaries (`S1`).
- OpenAI's Codex docs say a stack trace is usually enough for Codex to locate and correct a bug (`S2`).
- Self-Debugging shows that when unit tests are available, execution feedback improves code-generation results, with gains up to 12%, and improves sample efficiency by reusing failed predictions and feedback messages (`S9`).
- Anthropic also warns that Claude's context window fills up fast, that command output consumes context, and gives an explicit `CLAUDE.md` example preferring single tests instead of the whole suite (`S1`).

Observations

- A failing test does three useful things for an agent: localizes the problem, constrains candidate fixes, and supplies a measurable success criterion.
- The useful part of test output is usually compact: reproduction command, failing test name, assertion diff, stack trace, and maybe one or two nearby values.
- Large raw logs are often negative value once they exceed the point needed to localize failure, especially for Claude because Anthropic explicitly says performance degrades as context fills (`S1`).
- For Codex, this implies that work-log and test-log evidence is most useful when it is relevant and attributable, not voluminous (`S2`, inference).

Recommendations

- Feed the agent the exact failing command and the smallest failure artifact that still identifies root cause.
- Prefer one failing test or one failing test file over dumping the entire suite output into context.
- If the suite is noisy, summarize everything except the primary failure and give the agent the raw output only for that failure.

Impact on agent LLM = HIGH + rationale

- High, because failure artifacts directly shape the agent's search trajectory. Exact failures accelerate debugging; bloated or vague output wastes context and broadens the search unnecessarily.

F4. Regression tests are the durable way to convert one-off fixes into repeatable agent guidance.

Evidence

- SWE-bench evaluates agent patches by applying them and running the repository's test suite, making existing tests the arbiter of whether a change actually resolves the issue (`S5`).
- OpenAI's eval guidance recommends eval-driven development, scoped tests at every stage, continuous evaluation on every change, and notes that executable evals are useful for automated regression testing (`S4`).
- OpenAI's agent-evals guide recommends trace grading to find regressions and failure modes at scale, then moving to repeatable datasets and eval runs once "good" is known (`S3`).

Observations

- Existing regression tests are more valuable than transient ad hoc checks because they survive the current session and become future context for later agents.
- For coding agents, a regression test is both memory and guardrail: it preserves bug knowledge without requiring the next session to reread a long incident history.
- Agent-level evals and repository-level regression tests are complementary. The first catches orchestration drift; the second catches code-behavior drift.

Recommendations

- After any bug fix, add the narrowest reproducer that would have failed before the patch.
- Keep regression tests close to the behavior they protect so future agents can discover them quickly.
- For Claude/Codex workflows, pair repo tests with periodic agent-eval datasets that replay representative tasks or traces.

Impact on agent LLM = HIGH + rationale

- High, because regression tests turn ephemeral debugging context into persistent executable context, improving both current correctness and future agent behavior.

F5. More agent-written tests are not automatically better; existing curated tests often matter more than on-the-fly generated ones.

Evidence

- A 2026 study on SWE-bench Verified reports that GPT-5.2 writes almost no new tests yet performs comparably to top agents, that resolved and unresolved tasks show similar test-writing frequencies within the same model, and that prompt-induced changes in test-writing volume did not significantly change final outcomes (`S11`).
- The same study says agent-written tests often function as observational feedback channels, with value-revealing prints appearing more often than assertion-based checks (`S11`).
- Anthropic's best-practices page emphasizes rock-solid verification, not maximal test generation volume (`S1`).

Observations

- Agent-generated tests are often useful as probes while debugging, but they are not automatically high-quality specifications.
- The study in `S11` is especially important because it weakens a common assumption: "more self-written tests" is not the same as "better autonomous performance."
- For Claude and Codex, the implication is to prioritize existing human-maintained tests first, and only add tests when they capture previously unprotected behavior. The direct Claude/Codex tie here is partly inference; the strongest direct product mention in the evidence is GPT-5.2 in the paper and product docs stressing reliable testing rather than test volume (`S1`, `S2`, `S11`).

Recommendations

- Ask the agent to first discover and run relevant existing tests before generating new ones.
- Treat agent-written tests as temporary debugging instruments unless they clearly encode missing business behavior and deserve to be kept as regressions.
- Prefer assertion-rich regression tests over print-heavy observational scripts when deciding what stays in the repo.

Impact on agent LLM = MED + rationale

- Medium, because this mostly affects efficiency and test quality rather than raw ability. It prevents wasted budget and false confidence, but it does not replace the need for strong existing tests.

F6. Weak, overfit, biased, or flaky tests can systematically mislead coding agents.

Evidence

- Program-repair work has long shown that weak test suites cause overfitting: passing all available tests can still yield an incorrect patch (`S12`, `S13`).
- The 2025 self-generated-tests paper finds that post-execution self-debugging can struggle due to bias introduced by self-generated tests, while in-execution debugging can mitigate some of that bias (`S10`).
- The flaky-tests paper notes that automated repair methods rely on tests both to expose bugs and validate patches, but those tests may themselves be flaky (`S14`).

Observations

- Tests are only as good as their oracle strength. If assertions are weak, agents can satisfy them with superficial patches.
- Self-generated tests are especially risky because the same model can generate both the candidate solution and the judge, creating correlated blind spots.
- Flaky tests are a bad reward signal for an agent loop. They create false negatives, false positives, and unstable search trajectories.
- Overfitting is not just an evaluation concern; it is an interaction concern. An agent that gets rewarded for "green" on a weak suite may stop too early.

Recommendations

- Strengthen assertions around externally visible behavior, not just intermediate prints or golden logs.
- Separate deterministic regression tests from flaky or nondeterministic ones before using them as agent guidance.
- When possible, use held-out tests, differential tests, or secondary checks to detect patches that merely satisfy the visible suite.
- Be cautious when asking the same agent to both generate and validate its own tests; add human review or an independent check for important changes.

Impact on agent LLM = HIGH + rationale

- High, because the agent's optimization target is the feedback you provide. If the test signal is weak or unstable, the agent will optimize the wrong thing with high confidence.

Synthesis (1 paragraph)

Across Claude Code, Codex, and the broader LLM-for-code literature, the main pattern is consistent: tests are most valuable when they provide a precise, existing, executable contract and least valuable when they are noisy, weak, or generated just to create the appearance of validation. Existing repository tests and narrow regression tests give agents strong behavioral anchors; example-based tests sharpen requirements before coding begins; and failing tests with exact, compact output give the best debugging signal during search. The main failure modes are also consistent: too much log output bloats context, self-generated or observational tests can bias the loop, and weak or flaky suites let agents overfit to "green" without actually solving the problem. For practical Claude/Codex usage, the best pattern is test-aware but selective: expose canonical test commands, start from existing failures, add minimal durable regressions, and treat test quality, not test quantity, as the decisive factor.
