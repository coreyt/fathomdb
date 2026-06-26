---
name: conformance-rewrite-vacuous-green-trap
description: "Conformance/surface tests can pass while enforcing nothing; the 25.b subagent audit missed what codex caught — independent codex §9 is load-bearing, not redundant."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 7aacd556-dd31-405f-8249-33744be5eb59
---

0.8.0 Slice 25 (governed-surface conformance rewrite, closed 2026-06-04 @ `b86ef63`): the first
rewrite of `test_surface.py`/`surface.test.ts` PASSED its suites and a read-only adversarial subagent
audit ("25.b", 9 checks) returned a clean PASS — but **codex §9 found 2 × [P1]: the tests enforced
NOTHING.** (#1) The "live command surface" was a **hard-coded** verb tuple, so an unauthorized command
(`Engine.delete`/`read.*`) would pass the membership check **vacuously**. (#2) The cross-binding "parity"
test compared each binding's allowlist to a **same-file duplicate literal**, so Py↔TS drift was
undetectable — cosmetic. fix-1 fixed both: genuine `dir(Engine)` introspection (minus a documented
non-command exclusion set) in both bindings, and a **single shared contract**
(`src/conformance/governed-surface-allowlist.json`) both suites read so drift is structurally impossible.

**Why:** a test that asserts on a hand-curated subset, or compares a value to its own copy, is green by
construction — it looks like enforcement but is a tautology. This is the "silently-wrong green" failure
mode, one level up from [[slice-prompt-verify-test-claims]] (there the *prompt's* claim was false; here
the *test's* enforcement was hollow). The Explore-style 25.b audit accepted the assertions at face value;
**codex reasoned about what the assertion could/couldn't catch** and found the hole.

**How to apply:** (1) For any conformance/surface/parity test, ask "what fake violation would this
FAIL on?" — if you can't name one, it enforces nothing. Require the slice prompt to **demonstrate the
catch in RED** (inject a fake extra command / fake drift, watch it fail, revert). (2) Parity across
bindings needs a **single source of truth** both sides load — never two literals that can't diverge-
detect. (3) Surface allowlists must come from **real introspection minus an explicit exclusion set**, not
a hard-coded inclusion list. (4) Orchestration: the read-only subagent audit is a complement, NOT a
substitute for the independent **codex** pass — codex caught what the subagent missed. Keep codex the
primary §9 reviewer ([[orchestration-execution-traps]]).
