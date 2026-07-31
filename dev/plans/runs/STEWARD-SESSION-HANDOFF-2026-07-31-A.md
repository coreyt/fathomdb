# Steward session hand-off — 2026-07-31-A

**Supersedes `STEWARD-SESSION-HANDOFF-2026-07-30-B.md`.**

## ★ IMMEDIATE NEXT STEP

**Commission Slice 40 (`R-20-PUB`).** Nothing blocks it. It is the final ladder slice of 0.8.20.

```sh
scripts/commission-manifest.sh 0.8.20 40      # regenerate the manifest
# pair it with dev/plans/runs/0.8.20-slice-40-commission-brief.md
```

⛔ **Branch from `git rev-parse origin/main`, not the manifest's `base sha`.** The manifest reports the
newest landed *ladder slice* (Slice 39, `91db34d8`); two cross-cutting units landed after it and carry no
slice number, so the generator cannot see them — `SLICE-ID-HARDENING` (`2008f529`) and `R-20-HARNESS`
(`b6cc8fa6`). Branching from `91db34d8` silently discards both, including the collect-all harness Slice 40
needs to see its own red list.

**Do not re-open the scope question.** `seq-219` ruled option (b), **fix everything** — nothing waived,
nothing deferred. That **reverses** `seq-206`, under which the CI reds were 39.5's to report. They are
Slice 40's to **fix**. Nine placements were ruled at `seq-223`.

## 1. The brief is ready, and it is two files

| file | read it? |
|---|---|
| `dev/plans/runs/0.8.20-slice-40-commission-brief.md` | **yes** — instructions, final form only |
| `dev/plans/runs/0.8.20-slice-40-brief-provenance.md` | **no** — retracted claims, drafting history, claim sourcing |

Split at `5d135bee`. The brief was ~1,121 lines of which roughly 40 % was archaeology: version history,
withdrawn instructions, and postmortems on how earlier drafts of itself were wrong. It is now 846 lines
with every instruction stated once, one numbering system (BASE items `B1`–`B10`, `PHASE 0`–`7`), and stop
signs cut 81 → 24 so they mark only irreversible or destructive actions.

⚠ **The brief still carries an adversarial-review obligation before it becomes a real commission** — see
its §14. Until TC-131 exists (ruled to 0.8.21) that review is the only control on the defect class, and
it has caught 100 % of it across 13 reviews, including two unmeasured fixes and one vacuous green.

## 2. What else changed this session — program hygiene, already landed

A cold start was measured at **~374,000 tokens across 47 files**, ~216,000 of it closed releases.
`STATUS-0.8.0.md` alone is 48,824 tokens for a release closed months ago.

**Phase 1 has LANDED.** §3 of `0.8.x-STEWARD-HANDOFF.md` is now liveness-aware: the live board only (per
`board_is_closed()`), and `status: ACTIVE`/`PROPOSED` ladders only. **Cold start is now ~174,800 tokens
across 13 files.** You are reading the cheaper version.

A `steward_cold_start_set` metric was added to `scripts/repo-prune/bin/context-clarity.sh` beside its
existing `cold_start_orient_set`, with a ratchet ceiling of **180,000** — deliberately just above the
current measurement, not at the 60,000 end state. A gate that is red the day it lands gets switched off.

⚠ **The metric cannot run yet.** `context-clarity.sh` dies at repo root on **TC-139**, which is Slice 40's
item **B6** and is RED-first. It was deliberately left broken so that slice's failing test still fails.
**Slice 40 clears it as a side effect** — after B6 lands, run
`bash scripts/repo-prune/bin/context-clarity.sh baseline` and the metric works.

Full plan and rationale: `dev/design/steward-cold-start-budget.md` (RATIFIED, ledger `seq-226`).
**You do not need to read it to commission Slice 40.**

## 3. Two things sequenced AROUND Slice 40 — do not start them early

- **Phase 2** (`steward-orient.sh` emits master §4 from `release-state`) runs **during** Slice 40's
  Windows-clock window. It is not blocked by Slice 40, but landing it on `main` mid-flight forces the
  slice branch to rebase, which restarts the **N=5 consecutive-green `rust-windows` accrual** — a 60–75
  minute serial floor — and trap 13 requires re-running every gate after a rebase. Brief §4.9 says that
  clock accrues while checkpoints A–E proceed and not to idle on it; Phase 2 fits there and cannot touch
  the slice branch.
- ⛔ **Phase 3** (split the live board and the master) is **gated on 0.8.20 publishing.** It would
  restructure `STATUS-0.8.20.md` while five `generated_views` anchor into it, while
  `check-board-currency.sh` requires Slice 40's merge SHA literally inside it, and while
  `preflight.sh --landing` §7 enforces the same. Not before publish.

## 4. State of record

- **Ladder:** every slice through 39 is COMPLETE and landed. **40 is the only one left.**
- **SCHEMA 24.** ⚠ `plan-0.8.20.md` still claims 22 and cites a stale path — a Steward edit, not Slice 40's.
- **Publish is behind two gates, neither sufficient alone** (`seq-202`, amended `seq-211`, `seq-223`):
  (i) every `ci.yml` job **that executed** concludes `success`, **measured on Slice 40's own landing
  commit**; **and** (ii) explicit HITL approval. **Gate (i) is not met** — five reds, all diagnosed in
  brief §10. Slice 40 closing green does **not** open the gate.
- **Unruled HITL decisions:** `publish`, `npm-dist-tag`. Do not seek them.

## 5. Open, unowned

- Two `scripts/repo-prune/` housekeeping defects found while probing: its `README.md` says `runs/ 660→168`
  where its own `baseline.json` says **670**; and `post.json` (`bb64a2d4` / 1,883,557 tokens) contradicts
  `DELTA-2026-06-26.md` (`fe2734e9` / 1,895,292). Neither affects any conclusion — direction is −46 %
  either way — but both should be reconciled by whoever next touches repo-prune.
- `dev/plans/runs/` has re-drifted to 359 files (`find -maxdepth 1 -type f`) from 168 after the
  2026-06-26 prune. Nothing ratchets it. That is the argument for Phase 3 + CI, not a task for today.

## 6. Discipline notes carried forward

- **Verify the FIX, not just the diagnosis.** Two fixes in the Slice 40 brief were propagated from sound
  diagnoses without being run, and both were wrong (the governed-surface pin re-issue, and "quote
  `$PRUNE_EXPR`"). Both were caught by adversarial review, not by the Steward.
- **A control that passes can still be vacuous.** The first `steward_cold_start_set` control appeared to
  prove the metric was derived from §3; it passed for an unrelated reason while the case it was meant to
  test went unmeasured. Codex caught it. Prove a control by making it **fail** first.
