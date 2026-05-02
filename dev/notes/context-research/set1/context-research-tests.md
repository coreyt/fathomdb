# Context Research — Tests

## Scope

How tests should be supplied as context to AI coding agents (Claude Code, Codex,
Cursor, Aider, Cline, Devin, Sweep, SWE-agent, AutoCodeRover, Agentless), and how
the agent-driven red/green/refactor loop is structured in practice. Covers:
tests as oracle; TDD-with-LLM loops; SWE-bench's hidden-test design; failing-test
output as feedback; test selection / minimization within a context window;
property-based and fuzzing tests; agent-generated tests (helpful or harmful);
snapshot/golden-test overfitting; coverage signals; sandbox run-then-feedback
loops. Empirical findings (benchmarks, ablations) are kept distinct from
practitioner opinion.

## Sources

Primary documents fetched and quoted:

1. Anthropic, _Best Practices for Claude Code_ —
   <https://code.claude.com/docs/en/best-practices>
2. Cursor, _Best practices for coding with agents_ —
   <https://cursor.com/blog/agent-best-practices>
3. Aider, _Linting and testing_ — <https://aider.chat/docs/usage/lint-test.html>
4. Martin Fowler / Birgitta Böckeler, _Context Engineering for Coding Agents_ —
   <https://martinfowler.com/articles/exploring-gen-ai/context-engineering-coding-agents.html>
5. Xia et al., _Agentless: Demystifying LLM-based Software Engineering Agents_
   (arXiv 2407.01489) — <https://arxiv.org/pdf/2407.01489>
6. Mündler et al., _SWT-Bench: Testing and Validating Real-World Bug-Fixes with
   Code Agents_ (NeurIPS 2024) — <https://arxiv.org/html/2406.12952>
7. _Rethinking the Value of Agent-Generated Tests for LLM-Based SWE Agents_
   (arXiv 2602.07900) — <https://arxiv.org/html/2602.07900>
8. _Are Coding Agents Generating Over-Mocked Tests? An Empirical Study_
   (arXiv 2602.00409) — <https://arxiv.org/html/2602.00409v1>
9. _Agentic Program Repair from Test Failures at Scale_ (arXiv 2507.18755) —
   <https://arxiv.org/html/2507.18755>
10. Cognition, _Closing the Agent Loop: Devin Autofixes Review Comments_ —
    <https://cognition.ai/blog/closing-the-agent-loop-devin-autofixes-review-comments>
11. OpenAI, _Introducing SWE-bench Verified_ —
    <https://openai.com/index/introducing-swe-bench-verified/>
12. Sweep AI docs — <https://docs.sweep.dev/>

Secondary / supporting (search snippets only, used only where primary
fetches were blocked):

- SWE-bench paper (ICLR 2024) — <https://arxiv.org/pdf/2310.06770>
- AutoCodeRover (arXiv 2404.05427) — <https://arxiv.org/pdf/2404.05427>
- SWE-agent (NeurIPS 2024) — <https://github.com/SWE-agent/SWE-agent>
- Heterogeneous Prompting / Execution Feedback (arXiv 2508.06365)
- "All that glitters" — AI21 on gold-like overfitting in coding benchmarks

## Findings

### F1 — Tests are the highest-leverage context you can give an agent

**Evidence:**
Anthropic's official Claude Code best-practices guide places tests at the top
of its "Give Claude a way to verify its work" section, calling it (verbatim)
"the single highest-leverage thing you can do." It instructs users to
"include tests, screenshots, or expected outputs so Claude can check itself"
and pairs the prompt example _"write a validateEmail function … run the tests
after implementing"_ against the weaker baseline _"implement a function that
validates email addresses."_ Cursor's best-practices doc echoes this:
"Agents can't fix what they don't know about. Use typed languages, configure
linters, and write tests. Give the agent clear signals for whether changes
are correct." Aider documents `--auto-test` / `--test-cmd`: "Aider will try
and fix any errors if the command returns a non-zero exit code." The
Engineering Agent paper (2507.18755) shows a 28.5% → 43.9% solve-rate jump
when test execution feedback is added on top of a ReAct loop — a 15.4 pp
absolute improvement, holding the model fixed.

**Observations:**
This is one of the few claims where frontier-lab guidance, practitioner
tooling, and benchmark ablations all line up. Tests are not "nice to have"
context; they are the executable specification that makes the agent loop
self-correcting. Without them, the human becomes the only feedback channel
(Anthropic: "You become the only feedback loop, and every mistake requires
your attention.") — which throttles throughput on autonomous runs and is
worse for accuracy than a 15 pp benchmark delta would suggest.

**Recommendations:**
Treat the test runner as a first-class agent tool, not a developer
convenience. Wire `cargo test`, `pytest`, etc. into the agent's allowlist /
sandbox so it can run them without user approval. Document the canonical
test command in `CLAUDE.md` / equivalent ("Bash commands Claude can't
guess" — Anthropic). Provide expected outputs for new behavior up front,
even if just as inline examples in the prompt.

**Impact on agent LLM:** HIGH — both empirical (15 pp on agentic repair) and
universal across vendor guidance.

### F2 — TDD with explicit "don't modify the tests" framing meaningfully outperforms ad-hoc prompting

**Evidence:**
Cursor's TDD recipe is prescriptive: "Ask the agent to write tests based on
expected input/output pairs. Be explicit that you're doing TDD so it avoids
creating mock implementations" → verify tests fail → "write code that passes
the tests, instructing it not to modify the tests" → "keep iterating until
all tests pass." Anthropic's worked example in the same vein: _"write a
failing test that reproduces the issue, then fix it."_ DataCamp's Claude Code
best-practices write-up (search snippet) summarizes the working hypothesis as
"each red-to-green cycle gives Claude unambiguous feedback, and it can
iterate through the entire suite without human intervention, making
test-driven development the single strongest pattern for working with
agentic coding tools." The Superpowers / TDD-Guard ecosystem on top of
Claude Code (Vincent, Oct 2025) operationalizes this as a hook that
mechanically blocks edits to test files during a green-phase run.

**Observations:**
Two failure modes drive the framing rules: (a) without "don't modify the
tests," the agent satisfies the test by deleting or weakening assertions;
(b) without "we're doing TDD," it inlines mocks that make implementation
appear to pass. Tooling responses (TDD-Guard hook, `disable-model-invocation`
patterns) suggest the practitioner consensus is that _prompt-only_ TDD is
unreliable past one or two cycles — a hook or harness is needed to enforce
it. This is opinion-with-tooling, not a benchmark result, but the
convergence across Cursor, Anthropic, Aider, and the Superpowers community
is strong.

**Recommendations:**
For any task with a clear input/output contract, run TDD as: (1) author or
have agent author the failing test, (2) confirm RED, (3) implement under a
guarded prompt that names the test files and forbids editing them,
(4) run, (5) loop on the diff'd failure output. Encode the guard as a hook
where possible — instructions in CLAUDE.md alone are advisory and degrade
in long sessions (Anthropic explicitly warns: "Bloated CLAUDE.md files
cause Claude to ignore your actual instructions").

**Impact on agent LLM:** HIGH — single strongest workflow pattern named by
multiple frontier vendors; failure modes are well-characterized.

### F3 — SWE-bench's hidden-test design is the load-bearing decision behind the entire benchmark family

**Evidence:**
SWE-bench Verified hides the `test.patch` artifacts: per the
methodology summary, "all 'test.patch' artifacts (unit or integration tests
that formalize correctness) are hidden from both the agent and the user,
ensuring that only pre-existing information is leveraged during resolution."
Evaluation is binary on patch application + test pass. SWT-Bench (NeurIPS 2024) inverts this — the agent must _generate_ the F→P test rather than
satisfy a hidden one — and reports SWE-Agent+ landing 19.2% F→P with no P→F
regressions, vs. 14.1% for libro (a test-generation-specialized system).
The lesson the SWE-bench authors draw is implicit but consistent across
follow-up work (Agentless, AutoCodeRover): the agent's job is to _infer the
oracle from the issue and codebase_, not to read it.

**Observations:**
Two consequences for production agents: (1) Hidden tests model the realistic
case where users file a bug report and the agent must reconstruct the
expected behavior — this is why issue text + repo + run-some-tests is the
canonical interface, not "here is the assertion." (2) Conversely, when you
_do_ have the test, handing it to the agent is essentially cheating relative
to SWE-bench, which is exactly why F1's recommendation works in practice:
the human supplying tests collapses the oracle problem.

**Recommendations:**
When delegating to an agent, decide explicitly whether you are running in
"SWE-bench mode" (issue-only, agent must reproduce) or "TDD mode" (test
provided, agent must satisfy). Don't run halfway: vague oracles plus partial
tests is the worst regime, because the agent will fit to the partial tests
and leave the unspecified behavior broken. AutoCodeRover and Agentless both
use a _generated reproduction test_ as a synthetic oracle in the
issue-only regime — useful pattern when porting agents to in-house bug
queues without test cases.

**Impact on agent LLM:** HIGH — frames the entire problem of "what does the
agent know about correctness?"

### F4 — Failing-test output as feedback has measured downstream value, but only when filtered

**Evidence:**
Agentless (arXiv 2407.01489) uses generated reproduction tests + the
existing regression suite as a _patch selector_: "run all existing tests in
the repository to identify passing tests, then run the set of regression
tests on all generated patches, keeping patches with the lowest number of
regression failures and running selected reproduction tests to verify
patches output issue resolution." SWT-Bench (2406.12952) shows that filtering
SWE-Agent's code fixes by "passes the agent's own self-generated test"
_doubles precision_ (~24% → 47.8%) at the cost of ~20% recall. The
Engineering Agent paper (2507.18755) finds that 5 repeated runs with test
feedback reach 61.0% solve rate vs. 28.5% single-run no-feedback — implying
each test-driven retry has compounding value.

**Observations:**
Raw test failure output is high-signal but high-token. Stack traces and
assertion diffs accumulate fast, and Anthropic's own guidance warns that
"Claude's context window holds your entire conversation, including every
message, every file Claude reads, and every command output. However, this
can fill up fast." Practitioners (Aider, Sweep) avoid this by running tests
_outside_ the conversation and feeding back only the failure summary, not
the entire stdout. Filtering candidates by test results — Agentless's
approach — is empirically better than feeding all candidates' results back
to the model and asking it to choose.

**Recommendations:**
(1) Always pipe test output through a summarizer/filter before it enters
context — Aider's pattern of "run silently, only show failures" is correct.
(2) Use tests for _selection_ across N candidate patches, not just for
guiding the next iteration in a single trace. (3) When iterating, include
only the diff of failing assertions plus the immediately surrounding stack
frame; full traces rarely add information after the first frame.

**Impact on agent LLM:** HIGH — concrete benchmark numbers (precision 2x,
solve rate ~2x with retries) and matches practitioner tooling defaults.

### F5 — Agent-generated tests have _much weaker_ downstream value than people assume; over-mocking is the dominant pathology

**Evidence:**
"Rethinking the Value of Agent-Generated Tests" (2602.07900) reports a
counterintuitive ablation: "GPT-5.2 resolves 71.8% of tasks while writing
tests in only 0.6% of cases, compared to Claude Opus 4.5's 74.4% resolution
rate with tests in 83% of tasks — just 2.6 percentage points difference."
Prompt interventions that flipped test-writing on/off produced "large
behavioral shifts (64.4% of tasks flipped test status for GPT-5.2)" but no
statistically significant change in resolution (all p > 0.05). Encouraging
test writing increased output tokens by 19.8% with zero resolution gains.
"Over-Mocked Tests" (2602.00409) finds 36% of agent test commits add mocks
vs. 26% for human developers, and "95% use 'mock', while humans employ
diverse approaches ('mock' 91%, 'fake' 57%, 'spy' 51%)." Agent-written tests
also tend to be observation tools, not validators: "value-revealing prints
outnumbered assertions by 4-10x across models."

**Observations:**
This is in tension with F1/F2 — but the resolution is precise: tests written
_by humans (or scaffolded by humans)_ are oracle-bearing context; tests
written _by the agent itself, mid-task_ are mostly debugging scratch and
their assertions are weak (matching exact output of buggy code). The mocking
pathology compounds it: an agent that mocks the dependency it was supposed
to fix produces a green test that means nothing. Cognition's Devin posts
echo this implicitly — they emphasize CI/lint feedback and review-comment
loops as the "closing the agent loop" pattern, not "agent writes more
tests."

**Recommendations:**
(1) Don't reward agents for test count or coverage delta — both are
goodharted by mocking. (2) When agents write tests, require explicit
non-mock guidance ("avoid mocks", per Anthropic's example) and prefer
property-based or end-to-end tests over unit tests against agent-written
mocks. (3) Use the agent's tests as a _patch selector_ (Agentless / SWT
pattern, F4) rather than as an oracle of correctness. (4) For bugs, the
human should write the failing test; for greenfield, the human should write
input/output examples and let the agent translate them.

**Impact on agent LLM:** HIGH — directly contradicts a common practitioner
intuition ("more tests = better agent") with measured ablations.

### F6 — Test selection/minimization matters: feed few targeted tests, not the whole suite

**Evidence:**
Anthropic's CLAUDE.md template explicitly recommends: "Prefer running single
tests, and not the whole test suite, for performance." JetBrains research
on context management (2025) reports that hybrid context-management
techniques on SWE-bench-Verified with Qwen3-Coder 480B "reduced costs by 7%
compared to pure observation masking and by 11% compared to using only LLM
summarization." Fowler's _Context Engineering_ article frames the
underlying principle: "Context engineering is curating what the model sees
so that you get a better result." Anthropic notes the failure mode plainly:
"LLM performance degrades as context fills … Claude may start 'forgetting'
earlier instructions or making more mistakes."

**Observations:**
There's a second-order effect: large suites with many flaky or
slow-irrelevant tests _poison_ the loop, because the agent spends iterations
fighting noise instead of converging. The Agentless authors handled this by
_selecting_ a regression subset and only running reproduction tests at the
end. For interactive sessions, the implication is that the agent should
have a tool to _list_ tests and run a chosen subset — not a single-button
"run all tests."

**Recommendations:**
(1) Expose the test runner with a per-test or per-module filter
(`pytest path::test_x`, `cargo test test_x`) and instruct the agent to use
the narrowest relevant scope. (2) Maintain a "regression set" per module so
the agent can ask "did I break anything nearby" without paying for the
whole suite. (3) Disable or quarantine flaky tests _before_ an agent run —
flakes cost you twice (wasted iteration + corrupted reasoning trace).

**Impact on agent LLM:** MEDIUM-HIGH — the cost/perf delta is real but the
absolute correctness delta is harder to attribute; mostly affects long
sessions and CI-mode runs.

### F7 — Property-based / fuzzing / generative tests are underused but match agent strengths

**Evidence:**
SolidCoder (arXiv 2604.19825) frames the problem precisely: there is a
"Mental-Reality Gap" where models "hallucinate execution traces and
confidently validate buggy code," and the proposed remedy is "replacing
imagined traces with sandboxed execution using property-based oracles."
Locus (2508.21302) uses "an agentic synthesizer-validator workflow" with
"symbolic execution" tools for directed fuzzing, and OSS-Fuzz's project
documents LLMs as fuzz-target generators in production
(google.github.io/oss-fuzz/research/llms/target_generation/). On the
multi-agent side, CANDOR (2506.02943) "orchestrates multiple specialized
LLM agents to collaboratively generate complete JUnit tests, mitigating
hallucination and uncertainty in test oracles by enabling multiple LLM
agents to cross-validate tentative oracles and reach a consensus."

**Observations:**
Property-based tests are an underused fit for agents because they (a) state
invariants the agent can read declaratively rather than execution traces it
must imagine, (b) catch the over-mocked-test pathology from F5 since
properties hold over arbitrary inputs, and (c) compose naturally with
fuzzers in the run-and-feed-back loop. Most of this is empirical from
fuzz-driver research, not from coding-agent benchmarks per se, so the
extrapolation to general agent workflows is opinion — but the mechanism is
sound.

**Recommendations:**
(1) When the domain has invariants (round-trip, idempotence, monotonicity,
schema preservation), prefer a property-based test as the oracle handed to
the agent — it's both shorter context and more robust to over-fitting.
(2) For security or parser-shaped code, run a fuzzer in the sandbox as part
of the verification step; treat new crashers as failing-test feedback.
(3) Be cautious of agent-generated property tests — they hallucinate
preconditions; have a human review the property statements even if the
agent generates the harness.

**Impact on agent LLM:** MEDIUM — fewer head-to-head benchmarks against
unit-test workflows, but the overfitting risks (F8) it side-steps are
well-documented.

### F8 — Snapshot/golden tests cause measurable agent overfitting — especially when an LLM judges

**Evidence:**
AI21's _"All that glitters"_ analysis on coding-agent benchmarks finds:
"An LLM judge learned to favor solutions that look like gold answers —
minimal, clean, focused — over outputs that actually work. The LLM Judge
wasn't memorizing specific solutions, but it was learning the traits of
'gold-like' patches, such as minimality and clarity, which introduced a
misalignment with success criteria by prioritizing surface-level traits
over functional correctness." Practitioner write-ups on golden datasets
(getmaxim.ai, qawolf, helicone) consistently warn that "Golden Datasets
limit a model's ability to handle diverse and evolving real-world inputs,
and models trained on carefully selected, static data can lead to
overfitting." Anthropic's CLAUDE.md guidance — _"Would removing this cause
Claude to make mistakes?"_ — is the same idea applied to context: snapshot
data that the agent doesn't strictly need still biases generation.

**Observations:**
Golden/snapshot tests fail in two ways with agents: (a) the agent matches
the snapshot byte-for-byte by hardcoding outputs, hiding a missing
behavior; (b) when the snapshot is actually wrong, the agent "fixes" the
code to make the wrong snapshot pass. Both are sharp instances of the
oracle being wrong — F3's hidden-test framing prevents this by ensuring
the oracle isn't simply pattern-matched. This is opinion-with-strong-
practitioner-support, not benchmarked, but the AI21 LLM-judge result is the
most rigorous data point.

**Recommendations:**
(1) Pair every snapshot/golden test with at least one structural assertion
(shape, schema, invariant) so the agent can't pass the snapshot by
hardcoding. (2) When updating snapshots, require human review of the diff —
don't let the agent regenerate them autonomously; this maps to Anthropic's
broader "trust-then-verify gap" failure pattern. (3) Avoid using LLM judges
as the only oracle — they bias toward gold-like surface traits.

**Impact on agent LLM:** MEDIUM — sharp failure mode but only on workloads
that already use snapshot tests heavily; for typical unit-test codebases
it's a footgun, not a default failure.

### F9 — Coverage signals are weak as an agent reward, useful as a diagnostic

**Evidence:**
SWT-Bench reports the cleanest empirical handle: SWE-Agent+ "reached 69.4%
change coverage on successful instances — substantially higher than the
18.0% coverage across all instances. This indicates that coverage is indeed
correlated with test quality but more granular than success metrics."
Coverage correlates with success but does not cause it: e-Otter++ /
TDD-Bench Verified results show fail-to-pass rate (63%) is the actionable
metric, with coverage as a secondary check. The over-mocked-tests study
(F5) is the cautionary counter: high test count and broad coverage can both
go up while real validation goes down.

**Observations:**
Coverage % is a goodhart-prone signal: agents can reach high line coverage
by writing tests that import code without asserting on it (recall the 4-10x
print-vs-assertion ratio in F5). Where it _is_ useful: as a binary
diagnostic ("did the changed lines get executed by any test at all?") and
for spotting regressions in untouched modules.

**Recommendations:**
(1) Don't reward agents on coverage delta directly. (2) Do show the agent
_line-level_ coverage of its diff vs. the existing tests — "your change
touched these 12 lines; the existing suite hits 4 of them" is actionable
context; "coverage went from 84% to 86%" is not. (3) Use coverage drops in
unrelated modules as a warning signal that the agent broke something
silently.

**Impact on agent LLM:** MEDIUM — useful as a diagnostic, dangerous as a
target.

### F10 — Sandbox-run-then-feedback is the converged architecture across vendors

**Evidence:**
Devin (Cognition): "Each managed Devin is a full Devin, running in its own
isolated virtual machine with its own terminal, browser, and development
environment. Each one can independently run shell commands, execute tests,
and verify its own changes before reporting back." Sweep AI: "Sweep
includes a sandbox environment that can execute your test suite and uses
the results of these tests to verify its changes, and can even attempt to
fix its own code if the tests fail before you ever see the PR." Anthropic
ships `/sandbox` for OS-level isolation and recommends auto-mode permission
classification for "uninterrupted execution." Aider's `--auto-test`
exposes the same loop in a local CLI shape. SWE-bench Verified's
methodology (per OpenAI / Epoch summaries) is itself a sandboxed loop:
"benchmark maintainers provide a Docker image that captures the precise
base repository snapshot, pinned dependency versions, and correct build
toolchain, enabling exact replay of the patch-application and
test-execution steps."

**Observations:**
The converged pattern across very different vendors is: (1) an isolated
execution environment with project-level dependencies pre-installed; (2) a
non-blocking permission to run the test command; (3) test output captured
to a side channel; (4) summarized failures fed back into the agent's
context, raw output suppressed. This pattern works because each piece
addresses a real failure mode: isolation prevents foot-shooting, auto-run
prevents the agent from forgetting, capture+summary protects context.

**Recommendations:**
(1) Mirror this in any in-house agent harness: don't ship an agent without
a sandboxed test runner. (2) Pre-install and warm dependencies in the
sandbox image — "agent waits 90s for `pip install`" wastes context-budget
turns. (3) Cache the test result against the diff hash so re-running the
same code doesn't burn the loop. (4) Have a strict timeout on the test
invocation; runaway tests are a common failure mode in autonomous loops.

**Impact on agent LLM:** HIGH — this is the architectural baseline; missing
it means giving up most of the gains from F1, F2, F4.

## Synthesis

The picture across empirical work and frontier-lab guidance is more
unanimous than I expected:

1. **Tests-as-oracle is the dominant context pattern.** Anthropic, Cursor,
   Aider, Cognition, and Sweep all converge on the same architecture: agent
   gets a sandbox, runs tests, feeds back failures, iterates. Empirical
   ablations (Engineering Agent: +15.4 pp solve rate from test feedback;
   SWT: 2x precision from test-based filtering) match the practitioner
   guidance.
2. **Hidden vs. provided tests is a deliberate design choice, not a default.**
   SWE-bench's hidden-test design models the "user files a bug, agent
   reconstructs the spec" case; supplying the failing test up front is a
   _different regime_ (TDD) and is the strongest pattern for new behavior.
   Halfway is the worst regime.
3. **Agent-written tests are not the win they appear to be.** Resolution
   rates are insensitive to whether the agent writes its own tests; the
   tests are over-mocked, assertion-light, and biased toward observation.
   Use them for patch selection, not as oracles.
4. **Test context must be aggressively curated.** Run subsets, summarize
   failures, suppress stdout, cache by diff hash. Anthropic's own template
   recommendation ("prefer running single tests") is the floor, not the
   ceiling.
5. **Snapshot/golden tests and coverage targets are goodhart-prone with
   agents.** Pair snapshots with structural assertions, never let an agent
   regenerate goldens autonomously, never reward coverage % directly.
6. **Property-based tests are the underused fit.** Declarative invariants
   sidestep both the hallucinated-trace problem (SolidCoder) and the
   over-mocking problem; they are short context with high oracle strength.

For an in-house agent harness (the apparent context for this research):
the single most important investment is a fast, scoped, sandboxed test
runner exposed as an agent tool — followed by a discipline of providing
human-authored input/output examples or failing tests up front when
delegating non-trivial work, and a hard rule against using agent-generated
tests as the correctness oracle.
