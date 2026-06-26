---
name: slice-prompt-verify-test-claims
description: "When a slice prompt's guardrail/mechanism hinges on what a test asserts, read the WHOLE test file first — a partial read put a false premise in the Slice 21 prompt."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: d953dabe-e131-4552-843e-d416403203cf
---

When authoring a slice prompt whose guardrail or prescribed mechanism depends on **what an existing test asserts**, read the ENTIRE test file and confirm the assertion before stating it as fact in the prompt. Do not infer test behavior from the first screen of a file.

**Why:** In the Slice 21 (pyright-zero) prompt I wrote that `test_pyright_narrowing.py` "does NOT assert the line-30 negative error" and prescribed two mechanisms (a `# pyright: ignore` on the fixture, or a `[tool.pyright] exclude`) on that premise. The premise was **false** — `test_pyright_flags_unnarrowed_variant_key_access` (lines 121–146) requires pyright to emit that exact error. I had read lines 1–70 of the test (which only showed the reveal_type expectations) and assumed the rest. Both prescribed mechanisms would have broken a green test to fake a clean pyright. The Slice 21 agent caught it, escalated (didn't force), and amended with a cleaner fix: **relocate the negative-error fixture `tests/ → src/python/_typecheck_fixtures/` (byte-identical), outside pyright's `include`, repointing only the test's `_FIXTURE` path** — so `agent-verify`'s `pyright -p src/python` is 0/0 while the subprocess test still analyzes the fixture explicitly and the diagnostic still fires. Honest zero, config not weakened.

**How to apply:** Before a slice prompt asserts "test X does/doesn't check Y" as a load-bearing fact, open test X fully and verify. This is acute for prompts that rewrite or constrain conformance suites — e.g. the imminent Slice 25 (supersession conformance rewrite) makes many claims about `test_surface.py`, `surface.test.ts`, and the `no_recovery_surface` suites; read each in full before writing guardrails around them. A forced deviation is a defect report on the prompt ([[oob-creep-vs-justified-deviation]]); a false factual premise is worse — it can produce a *silently wrong* green if the implementer doesn't escalate. Treat such escalations as real saves ([[dont-dismiss-user-directed-subagents]]).
