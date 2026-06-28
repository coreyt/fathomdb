# GA-2 / Slice-40 — apply B-1, finalize the gate restructure, merge · track:GA · type:work

## Purpose (1–2 sentences)

Apply the ◆ B-1 ruling to Slice 40: pin eu7's corpus input, finalize AC-075, resolve B-3/`ac_020`, finish the
verification battery + release docs, and drive to a codex §9 PASS → merge (AC-075/076 land on `main`). The GA
critical-path work item.

## Prerequisites (verify ALL before starting — do not start if any is unmet)

- [ ] **◆ B-1 ruled** (corpus basis pinned). — verify: board §7 carries a "B-1 RULED" entry naming the pinned
  snapshot/basis (`grep -n "B-1" dev/plans/runs/STATUS-0.8.0.md`); the GA-1 output exists and B-1 is no longer "OWED".
- [ ] The Slice-40 branch state is known (re-run target). — verify: `git branch -a | grep slice-40` →
  `slice-40-20260607T145013Z` (gate restructure + AC-075/076, **NOT merged**; `main` does not yet contain AC-075).
  Confirm `git log --oneline main | grep -i AC-075` is **empty** (not yet landed).
- [ ] Recovery denylist + governed allowlist are **byte-frozen** going in. — verify: the recovery suites +
  `bindings.md` §10 + `governed-surface-allowlist.json` show zero intended diff in this slice.

## Work to-do (the steps)

**Follow the authoritative prompt `dev/plans/prompts/0.8.0-slice-40.md` — this scaffold is the launcher.**
Key reminders:

1. **Apply B-1**: pin eu7's corpus input to the ruled basis; finalize **AC-075** (real-embedder recall floor,
   honestly asserting) — **do NOT lower the floor or weaken the eu7 assert** ([[0.8.0-ga-blocked-recall-corpus]]).
2. **Resolve B-3 / `ac_020`** (parallel-read scaling): confirm on the canonical x86_64 perf runner **or** route to
   the reader-pool latency band — don't "fix" it by relaxing the assert on an idle dev box.
3. **B-2 is already resolved** (Slice 6: tokenizer exonerated; HITL ruling = run perf gates `--release`+isolated,
   tier AC-012 via AC-076 — no tokenizer change). Fold its CI-wiring (Q3) into this slice's verification.
4. Measure **everything `--release` + isolated**; read REAL exits ([[background-exit-masks-real-exit]]); finish the
   deferred bars (a final agent-verify, m release docs).
5. No engine/schema/SDK/migration change beyond the gate restructure; no push/tag.

## Output to the orchestrator (how this session reports back)

- Artifact(s): `dev/plans/runs/0.8.0-slice-40-output.json` + the **merge to `main`** (slice agent owns its worktree,
  `--no-ff`, no push).
- Schema/contract: `output.json` = per-bar GREEN/RED scoreboard (a–n incl. AC-075/AC-076), the recall verdict on the
  **ruled** basis, B-3 disposition, branch/baseline/merge SHAs, recovery+allowlist byte-freeze confirmation.
- Hand-off line: on a green slice the orchestrator runs **codex §9** → **◆ Slice-40 merge**; the merge lands
  AC-075/076 on `main`, which **unblocks IR-D** (AC-077 becomes the next free id) and feeds **◆ GA sign-off**.
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned.

## Full prompt / next

- Authoritative prompt: `dev/plans/prompts/0.8.0-slice-40.md` (re-scoped).
- On completion → orchestrator **codex §9** → **◆ Slice-40 merge**, then → **◆ GA sign-off**.
