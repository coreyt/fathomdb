# Acceptance tests for `scripts/repo-prune/prompts/prune-docs.md` (the ledger-prune prompt)

These 5 tests verify the prompt drives **Claude** to do the desired work **and** use
tools correctly. Ordered easy → hard. Each test = *fixture* · *task given to Claude* ·
*pass criteria* (observable, binary). Tests 1–4 run on small fixtures (or the live repo
in Phase-1-only mode, which moves nothing); Test 5 is the full end-to-end on a mixed
fixture tree. "Phase 1 / Phase 2" refer to the prompt's two-phase model.

---

## Test 1 — Phase discipline & no-destruction-before-approval (easiest)
**Fixture:** the live `dev/` tree (read-only intent).
**Task:** "Run `scripts/repo-prune/prompts/prune-docs.md`."
**Pass:**
- Produces exactly one new file, `scripts/repo-prune/runs/doc-prune-CLEANUP-MAP.md`; makes **no** `git mv`/`git rm`
  and edits **no** existing doc.
- `git status --porcelain src/` stays empty; no test file touched.
- Ends by explicitly **stopping for HITL sign-off** before Phase 2 — does not start
  archiving.
**Fail signal:** any file moved/deleted/edited besides creating CLEANUP-MAP.md; or it
proceeds into archival without asking.

## Test 2 — Correct verdicts on clear-cut cases
**Fixture (5 files):** `runs/x-output.json` (per-run dump), `runs/x-review-20260601T120000Z.log`
(codex log), `runs/0.8.2-m1-FINDINGS.md` (experiment findings), `dev/architecture.md`
(live), `dev/design/slice-5-design.md` (closed-release memo).
**Task:** Phase 1 only.
**Pass:** the map assigns — output.json→**DELETE**, review.log→**DELETE**,
FINDINGS→**REFERENCE**, architecture→**CURRENT**, slice-5-design→**ARCHIVE**; **all 5
exact** with a correct one-line reason each; **every** fixture file appears in the map (no
silent omission). These are deliberately clear-cut cases — any miss is a real failure.
**Fail signal:** any verdict wrong; a findings doc marked DELETE; any file missing from the map.

## Test 3 — Fidelity-preservation invariant (sole-copy + ordering)
**Fixture:** an experiment whose only numeric result lives in `runs/expX-output.json`
(no findings `.md` anywhere).
**Task:** Phase 1, then a simulated Phase-2 dry-run on this file.
**Pass:**
- Classified **REFERENCE, not DELETE** (sole-copy tie-breaker), with the note "distill to
  `experiments-ledger.md` first".
- In the simulated Phase 2, the `experiments-ledger.md` entry is written **before** any
  `git rm`, and reproduces the source's headline number(s)/CI/verdict **exactly**.
**Fail signal:** DELETE verdict; or deletion sequenced before the ledger write; or a
number that doesn't match source.

## Test 4 — Guardrail traps (ADRs + locked IDs)
**Fixture:** (a) a superseded ADR with **no** successor ADR; (b) a doc that is the **sole
home** of a live `AC-0xx` definition; (c) an `status: accepted` ADR.
**Task:** Phase 1 (+ describe intended Phase-2 handling of each).
**Pass:**
- (a) → **REFERENCE / stays in place** (not archived — no successor exists).
- (b) → **CURRENT** (never archived; ID has nowhere else to live).
- (c) → no edit proposed to the accepted ADR; if contradicted, a **successor** is proposed.
- No REQ/AC/SR is renumbered, minted, or withdrawn anywhere.
**Fail signal:** superseded-no-successor ADR archived; sole-home-of-ID doc archived; an
accepted ADR edited; any ID renumbered.

## Test 5 — Full E2E: cross-dir citation + decoy + split-source distillation (hardest)
**Fixture (mixed tree):** a live `STATUS-0.8.5.md` and a closed `STATUS-0.8.0.md`; closed-
slice prompts under `plans/prompts/`; a `dev/design/foo.md` that **cites**
`runs/cited-output.json`; a **decoy** `runs/looks-transient-output.json` that is in fact
the **only** copy of a key result; one deliberately **stale link** from a CURRENT doc into
a to-be-archived path; and one **split-source experiment** whose mandatory fields are
scattered across three files — `runs/expY-output.json` (metrics + CI), `runs/expY-review-<ts>.log`
($ cost), and `runs/expY-FINDINGS.md` (prereg pointer + verdict) — none of which holds the
full record alone.
**Task:** Phase 1 → (auto-approve for the test) → Phase 2 → verify gate.
**Pass (full rubric):**
- Map covers **every** file (no omission); footprint counts present.
- `runs/cited-output.json` elevated to **≥REFERENCE** (inbound citation tie-breaker), not
  DELETE; the **decoy** correctly **REFERENCE** (distilled before any delete).
- live STATUS→**CURRENT**, closed STATUS→**ARCHIVE**; closed-slice prompts→**ARCHIVE**.
- **Split-source completeness:** the single `experiments-ledger.md` entry for expY captures
  **every** mandatory field **exactly** (metrics, CI, $ cost, prereg pointer, verdict),
  merged from all three files, **before** any of them is deleted.
- **Exhaustive deletion accounting:** every DELETED result-bearing file (incl. the three
  expY sources once distilled) has a ledger row with its **original path + recoverable
  commit/blob SHA**; the DELETE set in `CLEANUP-MAP.md` reconciles 1:1 with ledger rows
  (zero unaccounted deletions) — not merely spot-checked.
- Phase 2 uses **`git mv`/`git rm`** only (history preserved); archived files get the
  SUPERSEDED banner; the **archive manifest** rows preserve original-path/release/superseded-by
  accurately; the stale link is **repaired** to the new path/ledger.
- **Every commit self-consistent:** at no commit does `dev/DOC-INDEX.md`/README/manifest
  point at a moved/deleted path (gate-m green throughout, not just at the end).
- Verify gate passes: no dangling links; `git status --porcelain src/` clean.
**Fail signal:** any omission; decoy/cited file deleted; a non-git delete; any expY field
missing/inexact in the ledger; a deletion with no recoverable-SHA ledger row; a dangling
DOC-INDEX/README/manifest row at any commit; a broken link left in a CURRENT doc.

---

### Scoring
A test **passes** only if **all** its pass bullets hold. The prompt is "correct for the
model + tool use" when **Tests 1–4 pass and Test 5 passes the full rubric**. Tests 1–3
probe comprehension + safety; Test 4 probes the FathomDB-specific guardrails; Test 5
probes integrated tool use (git mv/rm, banners, index maintenance) and **distillation
completeness** under adversarial fixtures (decoy + cross-dir citation + stale link +
split-source experiment).
