---
name: background-exit-masks-real-exit
description: "A background/chained task's exit 0 can mask the real command's nonzero exit — read PIPESTATUS, not the trailing echo"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 0b648ac9-0875-4e55-b7e7-35645b13e613
---

Mid-run I caught a false PASS: pytest was reported as background "exit 0", but that 0 was the wrapper command's trailing `echo` — the REAL pytest exit was 2 (a collection error). Root cause was mine: I'd built the extension WITHOUT test-hooks for the smoke app, so a test-hooks-only symbol failed to import at pytest collection time. Rebuilding WITH test-hooks → clean 97/4. No code defect — the same code passed `cargo test`, the TS suite, and the live app.

**Why:** When a command is piped or chained (`pytest ... | tee`, `cmd && echo done`, or any wrapper that appends output), the captured/displayed exit reflects the LAST element of the pipe/chain, not the command you care about. A trailing `echo` always exits 0, silently converting a real failure into an apparent pass.

**How to apply:** Never trust a single "exit 0" from a piped/chained/backgrounded command. Read `${PIPESTATUS[@]}` (bash) to get each stage's real exit, or run the command bare and check `$?` directly. Cross-check a green claim against what the suite actually printed (e.g. "97/4" vs a collection error). Also: a collection/import error ≠ a code defect — confirm the build/feature-flags the test harness needs (here: test-hooks) before blaming the code. Related: [[orchestration-execution-traps]], [[conformance-rewrite-vacuous-green-trap]].
